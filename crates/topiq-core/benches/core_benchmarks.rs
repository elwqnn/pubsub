use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use topiq_core::{Message, Subject};

// ---------------------------------------------------------------------------
// Subject::new -- validation cost per subscribe/publish
// ---------------------------------------------------------------------------

fn bench_subject_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("subject_validation");

    group.bench_function("simple_3_tokens", |b| {
        b.iter(|| Subject::new(black_box("sensors.temp.room1")).unwrap());
    });

    group.bench_function("deep_8_tokens", |b| {
        b.iter(|| {
            Subject::new(black_box("org.division.team.service.sensors.temp.room1.value")).unwrap()
        });
    });

    group.bench_function("wildcard_star", |b| {
        b.iter(|| Subject::new(black_box("sensors.*.room1")).unwrap());
    });

    group.bench_function("wildcard_gt", |b| {
        b.iter(|| Subject::new(black_box("sensors.>")).unwrap());
    });

    group.bench_function("single_token", |b| {
        b.iter(|| Subject::new(black_box("hello")).unwrap());
    });

    group.bench_function("max_depth_16_tokens", |b| {
        b.iter(|| {
            Subject::new(black_box("a.b.c.d.e.f.g.h.i.j.k.l.m.n.o.p")).unwrap()
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Message::clone -- fanout amplification cost
// ---------------------------------------------------------------------------

fn bench_message_clone(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_clone");
    let topic = Subject::new("sensors.temp.room1").unwrap();

    for size in [0, 64, 1024, 16_384] {
        let payload = Bytes::from(vec![0xABu8; size]);
        let msg = Message::new(topic.clone(), payload);

        group.bench_with_input(
            BenchmarkId::new("payload_bytes", size),
            &msg,
            |b, msg| {
                b.iter(|| black_box(msg.clone()));
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Message construction
// ---------------------------------------------------------------------------

fn bench_message_new(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_new");

    for size in [0, 64, 1024] {
        let payload = Bytes::from(vec![0xABu8; size]);
        group.bench_with_input(
            BenchmarkId::new("payload_bytes", size),
            &payload,
            |b, payload| {
                b.iter(|| {
                    let topic = Subject::new("sensors.temp.room1").unwrap();
                    black_box(Message::new(topic, payload.clone()));
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_subject_validation,
    bench_message_clone,
    bench_message_new,
);
criterion_main!(benches);
