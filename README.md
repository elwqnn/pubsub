# pubsub

A lightweight TCP-based publish/subscribe message broker written in Rust.

Subjects use dot-separated tokens (`sensors.temp.room1`). Wildcards, queue groups, and request/reply are supported. The broker can run as a standalone process or be embedded directly in your application.

## When to use this

pubsub is a good fit when you want loosely-coupled communication between parts of a Tokio application — tasks that publish events without knowing who is listening, worker pools that share load through queue groups, or internal services exposed over a topic rather than a direct function call. It works equally well when those parts live in the same process or across separate ones.

It is not a replacement for a production message broker. There is no persistence, no clustering, and no at-least-once delivery guarantee.

## Usage modes

### 1. Connect to a standalone broker

Run `pubsub-server` as a separate process and connect from any number of clients:

```toml
[dependencies]
pubsub = "0.1"
tokio = { version = "1", features = ["full"] }
```

```rust
use pubsub::{Client, ConnectOptions};

#[tokio::main]
async fn main() -> pubsub::Result<()> {
    let client = Client::connect(ConnectOptions::default()).await?;

    client.publish("greet", "hello world").await?;

    let mut sub = client.subscribe("greet").await?;
    let msg = sub.next_message().await.unwrap();
    println!("{}", String::from_utf8_lossy(&msg.payload));

    client.close().await;
    Ok(())
}
```

### 2. Embed the broker in your application

Run broker and clients in the same process — no separate server needed:

```toml
[dependencies]
pubsub = { version = "0.1", features = ["server"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use std::sync::Arc;
use pubsub::server::{BrokerConfig, CancellationToken, Router, SubscriptionRegistry, TcpTransportListener};
use pubsub::{Client, ConnectOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let shutdown = CancellationToken::new();
    let registry = Arc::new(SubscriptionRegistry::new());
    let router = Arc::new(Router::new(registry.clone()));
    let config = BrokerConfig::default();

    let listener = TcpTransportListener::bind(&config, router, registry, shutdown.clone()).await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move { listener.run().await });

    let client = Client::connect(ConnectOptions::new(addr)).await?;
    // ... use client normally
    client.close().await;
    shutdown.cancel();
    Ok(())
}
```

See [`examples/chat-server.rs`](crates/pubsub/examples/chat-server.rs) for a full embedded broker example.

## Subject syntax

Subjects are dot-separated tokens. Two wildcards are supported on the subscriber side:

| Pattern | Matches |
|---------|---------|
| `sensors.temp.room1` | exactly that subject |
| `sensors.*.room1` | any single token in the middle position |
| `sensors.>` | any subject starting with `sensors.` |

```rust
// Exact
let mut sub = client.subscribe("sensors.temp.room1").await?;

// Single-token wildcard
let mut sub = client.subscribe("sensors.*.room1").await?;

// Suffix wildcard (one or more tokens)
let mut sub = client.subscribe("sensors.>").await?;
```

## Request / reply

`request` publishes to a subject and waits for a single response on an auto-generated inbox:

```rust
use std::time::Duration;

let reply = client.request("service.echo", "ping", Duration::from_secs(5)).await?;
println!("{}", String::from_utf8_lossy(&reply.payload));
```

The responder subscribes and publishes to `msg.reply_to`:

```rust
let mut sub = client.subscribe("service.echo").await?;
while let Some(msg) = sub.next_message().await {
    if let Some(reply_to) = &msg.reply_to {
        client.publish(reply_to.as_str(), &msg.payload).await?;
    }
}
```

## Queue groups (load balancing)

Subscribers in the same queue group share the message load — each message is delivered to exactly one member:

```rust
// Three workers, each receives roughly 1/3 of the messages
let _w1 = client.subscribe_queue("jobs.>", "workers").await?;
let _w2 = client.subscribe_queue("jobs.>", "workers").await?;
let _w3 = client.subscribe_queue("jobs.>", "workers").await?;
```

## Crates

| Crate | Description |
|-------|-------------|
| [`pubsub`](crates/pubsub) | Public facade — start here |
| [`pubsub-server`](crates/pubsub-server) | Standalone broker binary |
| `pubsub-client` | Async TCP client |
| `pubsub-broker` | Routing engine (trie, queue groups) |
| `pubsub-transport-tcp` | TCP listener and session management |
| `pubsub-core` | Shared types (`Message`, `Subject`, `BrokerConfig`) |
| `pubsub-protocol` | Wire format (length-prefixed msgpack frames) |

## Running the standalone server

```sh
cargo run -p pubsub-server -- --bind 0.0.0.0:4222

# With logging
RUST_LOG=info cargo run -p pubsub-server

# All options
cargo run -p pubsub-server -- --help
```

See the [`pubsub-server` README](crates/pubsub-server/README.md) for full CLI reference.

## Building

```sh
cargo build --workspace
cargo test --workspace
```

## License

MIT
