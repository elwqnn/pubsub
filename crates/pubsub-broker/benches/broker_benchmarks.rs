use std::sync::Arc;

use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

use pubsub_broker::registry::SubscriptionRegistry;
use pubsub_broker::router::Router;
use pubsub_broker::trie::TopicTrie;
use pubsub_core::{
    Message, Subject, SubscriptionId, SubscriptionSender, TaggedMessage,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sid(n: u64) -> SubscriptionId {
    SubscriptionId(n)
}

fn make_sender(id: u64) -> (SubscriptionSender, mpsc::Receiver<TaggedMessage>) {
    let (tx, rx) = mpsc::channel(4096);
    (SubscriptionSender::new(sid(id), tx), rx)
}

/// Build a trie with `n` exact subscriptions on topics like `org.team.svc.N`.
fn build_trie_exact(n: u64) -> TopicTrie {
    let mut trie = TopicTrie::new();
    for i in 0..n {
        trie.insert(&format!("org.team.svc.{i}"), sid(i));
    }
    trie
}

/// Build a trie with `n` subscriptions on the SAME topic (fanout scenario).
fn build_trie_fanout(topic: &str, n: u64) -> TopicTrie {
    let mut trie = TopicTrie::new();
    for i in 0..n {
        trie.insert(topic, sid(i));
    }
    trie
}

/// Build a trie with mixed patterns: exact, wildcard, and suffix.
fn build_trie_mixed(n: u64) -> TopicTrie {
    let mut trie = TopicTrie::new();
    for i in 0..n {
        match i % 3 {
            0 => trie.insert(&format!("org.team.svc.{i}"), sid(i)),
            1 => trie.insert(&format!("org.team.*.{i}"), sid(i)),
            _ => trie.insert("org.team.>", sid(i)),
        }
    }
    trie
}

// ---------------------------------------------------------------------------
// TopicTrie::matches -- the single hottest function
// ---------------------------------------------------------------------------

fn bench_trie_match_exact(c: &mut Criterion) {
    let mut group = c.benchmark_group("trie_match_exact");

    for n in [10, 100, 1_000, 10_000] {
        let trie = build_trie_exact(n);
        // Match a topic that exists (last one)
        let topic = format!("org.team.svc.{}", n - 1);
        group.bench_with_input(
            BenchmarkId::new("subscriptions", n),
            &(&trie, &topic),
            |b, (trie, topic)| {
                b.iter(|| black_box(trie.matches(topic)));
            },
        );
    }

    group.finish();
}

fn bench_trie_match_miss(c: &mut Criterion) {
    let mut group = c.benchmark_group("trie_match_miss");

    for n in [10, 100, 1_000, 10_000] {
        let trie = build_trie_exact(n);
        group.bench_with_input(
            BenchmarkId::new("subscriptions", n),
            &trie,
            |b, trie| {
                b.iter(|| black_box(trie.matches("no.such.topic")));
            },
        );
    }

    group.finish();
}

fn bench_trie_match_wildcard(c: &mut Criterion) {
    let mut group = c.benchmark_group("trie_match_wildcard");

    // Trie with wildcard subscriptions: org.team.*.N
    for n in [10, 100, 1_000] {
        let mut trie = TopicTrie::new();
        for i in 0..n {
            trie.insert(&format!("org.*.svc.{i}"), sid(i));
        }
        // This topic matches exactly one wildcard pattern
        let topic = format!("org.team.svc.{}", n / 2);
        group.bench_with_input(
            BenchmarkId::new("patterns", n),
            &(&trie, &topic),
            |b, (trie, topic)| {
                b.iter(|| black_box(trie.matches(topic)));
            },
        );
    }

    group.finish();
}

fn bench_trie_match_suffix(c: &mut Criterion) {
    let mut group = c.benchmark_group("trie_match_suffix");

    // Multiple > wildcards at different depths
    let mut trie = TopicTrie::new();
    trie.insert(">", sid(0));
    trie.insert("org.>", sid(1));
    trie.insert("org.team.>", sid(2));
    trie.insert("org.team.svc.>", sid(3));

    for depth in [1, 2, 4, 8] {
        let topic = (0..depth)
            .map(|i| match i {
                0 => "org",
                1 => "team",
                2 => "svc",
                _ => "extra",
            })
            .collect::<Vec<_>>()
            .join(".");

        group.bench_with_input(
            BenchmarkId::new("topic_depth", depth),
            &topic,
            |b, topic| {
                b.iter(|| black_box(trie.matches(topic)));
            },
        );
    }

    group.finish();
}

fn bench_trie_match_fanout(c: &mut Criterion) {
    let mut group = c.benchmark_group("trie_match_fanout");

    for n in [1, 10, 100, 1_000] {
        let trie = build_trie_fanout("events.user.login", n);
        group.bench_with_input(
            BenchmarkId::new("subscribers", n),
            &trie,
            |b, trie| {
                b.iter(|| black_box(trie.matches("events.user.login")));
            },
        );
    }

    group.finish();
}

fn bench_trie_match_mixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("trie_match_mixed");

    for n in [30, 300, 3_000] {
        let trie = build_trie_mixed(n);
        // This topic will match exact + wildcard + suffix patterns
        group.bench_with_input(
            BenchmarkId::new("patterns", n),
            &trie,
            |b, trie| {
                b.iter(|| black_box(trie.matches("org.team.svc.0")));
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// TopicTrie::insert -- subscription churn cost
// ---------------------------------------------------------------------------

fn bench_trie_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("trie_insert");

    group.bench_function("exact_4_tokens", |b| {
        b.iter_with_setup(
            TopicTrie::new,
            |mut trie| {
                trie.insert(black_box("org.team.svc.sensor"), sid(1));
                black_box(&trie);
            },
        );
    });

    group.bench_function("wildcard_pattern", |b| {
        b.iter_with_setup(
            TopicTrie::new,
            |mut trie| {
                trie.insert(black_box("org.*.svc.>"), sid(1));
                black_box(&trie);
            },
        );
    });

    // Insert into an already-populated trie
    group.bench_function("into_10k_trie", |b| {
        let mut trie = build_trie_exact(10_000);
        let mut counter = 10_000u64;
        b.iter(|| {
            trie.insert(&format!("org.team.svc.{counter}"), sid(counter));
            counter += 1;
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Router::route -- full dispatch pipeline (async)
// ---------------------------------------------------------------------------

fn bench_router_route(c: &mut Criterion) {
    let mut group = c.benchmark_group("router_route");
    let rt = Runtime::new().unwrap();

    for n in [1u64, 10, 100, 1_000] {
        // Build registry with `n` subscribers all on the same topic
        let registry = Arc::new(SubscriptionRegistry::new());
        let router = Router::new(registry.clone());

        let mut _receivers = Vec::new();
        rt.block_on(async {
            for i in 0..n {
                let (sender, rx) = make_sender(i);
                registry
                    .subscribe(Subject::new("events.user.login").unwrap(), None, sender)
                    .await;
                _receivers.push(rx);
            }
        });

        let msg = Message::new(
            Subject::new("events.user.login").unwrap(),
            Bytes::from_static(b"benchmark payload data here"),
        );

        group.bench_with_input(
            BenchmarkId::new("fanout_subscribers", n),
            &(&router, &msg),
            |b, (router, msg)| {
                b.to_async(&rt).iter(|| async {
                    black_box(router.route(msg).await);
                });
            },
        );

        // Drain receivers to avoid backpressure
        rt.block_on(async {
            for rx in &mut _receivers {
                while rx.try_recv().is_ok() {}
            }
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Router::route -- miss (no matching subscribers)
// ---------------------------------------------------------------------------

fn bench_router_route_miss(c: &mut Criterion) {
    let mut group = c.benchmark_group("router_route_miss");
    let rt = Runtime::new().unwrap();

    let registry = Arc::new(SubscriptionRegistry::new());
    let router = Router::new(registry.clone());

    // Subscribe to a different topic
    let (_sender, _rx) = make_sender(1);
    rt.block_on(async {
        let (sender, _rx) = make_sender(1);
        registry
            .subscribe(Subject::new("other.topic").unwrap(), None, sender)
            .await;
    });

    let msg = Message::new(
        Subject::new("events.user.login").unwrap(),
        Bytes::from_static(b"payload"),
    );

    group.bench_function("no_match", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(router.route(&msg).await);
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Router::route with queue groups
// ---------------------------------------------------------------------------

fn bench_router_queue_group(c: &mut Criterion) {
    let mut group = c.benchmark_group("router_queue_group");
    let rt = Runtime::new().unwrap();

    for n in [2u64, 10, 50] {
        let registry = Arc::new(SubscriptionRegistry::new());
        let router = Router::new(registry.clone());

        let mut _receivers = Vec::new();
        rt.block_on(async {
            for i in 0..n {
                let (sender, rx) = make_sender(i);
                registry
                    .subscribe(
                        Subject::new("work.tasks").unwrap(),
                        Some("workers".into()),
                        sender,
                    )
                    .await;
                _receivers.push(rx);
            }
        });

        let msg = Message::new(
            Subject::new("work.tasks").unwrap(),
            Bytes::from_static(b"task data"),
        );

        group.bench_with_input(
            BenchmarkId::new("group_members", n),
            &(&router, &msg),
            |b, (router, msg)| {
                b.to_async(&rt).iter(|| async {
                    black_box(router.route(msg).await);
                });
            },
        );

        rt.block_on(async {
            for rx in &mut _receivers {
                while rx.try_recv().is_ok() {}
            }
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// SubscriptionRegistry::match_topic -- RwLock overhead
// ---------------------------------------------------------------------------

fn bench_registry_match(c: &mut Criterion) {
    let mut group = c.benchmark_group("registry_match_topic");
    let rt = Runtime::new().unwrap();

    for n in [10u64, 100, 1_000] {
        let registry = Arc::new(SubscriptionRegistry::new());

        rt.block_on(async {
            for i in 0..n {
                let (sender, _rx) = make_sender(i);
                registry
                    .subscribe(
                        Subject::new(&format!("org.team.svc.{i}")).unwrap(),
                        None,
                        sender,
                    )
                    .await;
            }
        });

        let topic = Subject::new(&format!("org.team.svc.{}", n / 2)).unwrap();

        group.bench_with_input(
            BenchmarkId::new("subscriptions", n),
            &(&registry, &topic),
            |b, (registry, topic)| {
                b.to_async(&rt).iter(|| async {
                    black_box(registry.match_topic(topic).await);
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Throughput benchmarks -- big numbers: millions of ops/sec
// ---------------------------------------------------------------------------

/// Raw trie lookup throughput: batches 10K lookups per iteration and reports
/// millions of lookups/sec across different subscription counts.
fn bench_trie_lookup_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("trie_lookup_throughput");
    let batch: u64 = 10_000;
    group.throughput(Throughput::Elements(batch));

    for n in [100, 1_000, 10_000, 100_000, 1_000_000, 5_000_000] {
        let trie = build_trie_exact(n);
        let topics: Vec<String> = (0..batch)
            .map(|i| format!("org.team.svc.{}", i % n))
            .collect();

        group.bench_with_input(
            BenchmarkId::new("subscriptions", n),
            &(&trie, &topics),
            |b, (trie, topics)| {
                b.iter(|| {
                    for topic in topics.iter() {
                        black_box(trie.matches(topic));
                    }
                });
            },
        );
    }

    group.finish();
}

/// Full routing pipeline throughput: routes messages through the async router
/// and reports total deliveries/sec (subscribers x messages).
fn bench_routing_delivery_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("routing_delivery_throughput");
    let rt = Runtime::new().unwrap();

    for (subs, msgs) in [(10u64, 1_000u64), (100, 1_000), (1_000, 500)] {
        let deliveries = subs * msgs;
        group.throughput(Throughput::Elements(deliveries));

        let registry = Arc::new(SubscriptionRegistry::new());
        let router = Router::new(registry.clone());

        rt.block_on(async {
            for i in 0..subs {
                let (tx, mut rx) = mpsc::channel(8192);
                let sender = SubscriptionSender::new(sid(i), tx);
                registry
                    .subscribe(
                        Subject::new("stream.ingest.events").unwrap(),
                        None,
                        sender,
                    )
                    .await;
                tokio::spawn(async move {
                    while rx.recv().await.is_some() {}
                });
            }
        });

        let msg = Message::new(
            Subject::new("stream.ingest.events").unwrap(),
            Bytes::from_static(b"throughput benchmark payload"),
        );

        group.bench_with_input(
            BenchmarkId::new("deliveries", format!("{subs}subs_{msgs}msgs")),
            &(&router, &msg),
            |b, (router, msg)| {
                b.to_async(&rt).iter(|| async {
                    for _ in 0..msgs {
                        black_box(router.route(msg).await);
                    }
                });
            },
        );
    }

    group.finish();
}

/// Wildcard pattern routing throughput: 50 single-wildcard subscribers +
/// 10 suffix-wildcard subscribers. Each published message fans out to 60
/// receivers, reporting total deliveries/sec across 1K messages.
fn bench_wildcard_routing_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("wildcard_routing_throughput");
    let rt = Runtime::new().unwrap();
    let msgs: u64 = 1_000;
    let deliveries_per_msg: u64 = 60; // 50 wildcard + 10 suffix

    group.throughput(Throughput::Elements(msgs * deliveries_per_msg));

    let registry = Arc::new(SubscriptionRegistry::new());
    let router = Router::new(registry.clone());

    rt.block_on(async {
        let mut id = 0u64;
        // 50 subscribers matching "sensors.*.temperature"
        for _ in 0..50 {
            let (tx, mut rx) = mpsc::channel(8192);
            let sender = SubscriptionSender::new(sid(id), tx);
            registry
                .subscribe(
                    Subject::new("sensors.*.temperature").unwrap(),
                    None,
                    sender,
                )
                .await;
            tokio::spawn(async move {
                while rx.recv().await.is_some() {}
            });
            id += 1;
        }
        // 10 catch-all subscribers matching "sensors.>"
        for _ in 0..10 {
            let (tx, mut rx) = mpsc::channel(8192);
            let sender = SubscriptionSender::new(sid(id), tx);
            registry
                .subscribe(
                    Subject::new("sensors.>").unwrap(),
                    None,
                    sender,
                )
                .await;
            tokio::spawn(async move {
                while rx.recv().await.is_some() {}
            });
            id += 1;
        }
    });

    let messages: Vec<Message> = (0..msgs)
        .map(|i| {
            Message::new(
                Subject::new(&format!("sensors.room{}.temperature", i % 100)).unwrap(),
                Bytes::from_static(b"{\"value\":23.5}"),
            )
        })
        .collect();

    group.bench_function("60_wildcard_subs_1k_msgs", |b| {
        b.to_async(&rt).iter(|| async {
            for msg in messages.iter() {
                black_box(router.route(msg).await);
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_trie_match_exact,
    bench_trie_match_miss,
    bench_trie_match_wildcard,
    bench_trie_match_suffix,
    bench_trie_match_fanout,
    bench_trie_match_mixed,
    bench_trie_insert,
    bench_router_route,
    bench_router_route_miss,
    bench_router_queue_group,
    bench_registry_match,
    bench_trie_lookup_throughput,
    bench_routing_delivery_throughput,
    bench_wildcard_routing_throughput,
);
criterion_main!(benches);
