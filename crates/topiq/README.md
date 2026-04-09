# pubsub

Lightweight TCP-based publish/subscribe client library.

This crate re-exports the full consumer-facing API. It is the only dependency you need for client-only usage. Add the `server` feature to embed a broker in your application.

## Add to your project

```toml
# Client only
pubsub = "0.1"

# Client + embedded broker
pubsub = { version = "0.1", features = ["server"] }
```

## Quick start

```rust
use pubsub::{Client, ConnectOptions};

#[tokio::main]
async fn main() -> pubsub::Result<()> {
    // Connects to 127.0.0.1:4222 by default
    let client = Client::connect(ConnectOptions::default()).await?;

    client.publish("greet", "hello world").await?;

    let mut sub = client.subscribe("greet").await?;
    let msg = sub.next_message().await.unwrap();
    println!("{}", String::from_utf8_lossy(&msg.payload));

    client.close().await;
    Ok(())
}
```

`Client` is cheaply cloneable and safe to share across tasks.

## Connecting

```rust
use pubsub::ConnectOptions;

// Default: 127.0.0.1:4222
ConnectOptions::default()

// Specific address
ConnectOptions::new("127.0.0.1:4222".parse()?)

// Host string (DNS resolved at connect time)
ConnectOptions::from_host("broker.example.com:4222")

// With tuning
ConnectOptions::from_host("localhost:4222")
    .max_frame_size(128 * 1024)
    .channel_buffer_size(512)
```

## Subjects and wildcards

Subjects are dot-separated tokens. Wildcards are only valid on the subscriber side:

```rust
// Exact match
client.subscribe("sensors.temp.room1").await?;

// * matches exactly one token
client.subscribe("sensors.*.room1").await?;

// > matches one or more trailing tokens
client.subscribe("sensors.>").await?;
```

## Publishing

The payload accepts anything that implements `AsRef<[u8]>` (`&str`, `String`, `Vec<u8>`, `Bytes`):

```rust
client.publish("events.user.login", "alice").await?;
client.publish("events.user.login", b"raw bytes".as_slice()).await?;
client.publish("events.user.login", bytes::Bytes::from("...")).await?;
```

## Request / reply

```rust
use std::time::Duration;

// Caller
let reply = client.request("service.echo", "ping", Duration::from_secs(5)).await?;

// Responder
let mut sub = client.subscribe("service.echo").await?;
while let Some(msg) = sub.next_message().await {
    if let Some(reply_to) = &msg.reply_to {
        client.publish(reply_to.as_str(), &msg.payload).await?;
    }
}
```

## Queue groups

Multiple subscribers sharing a queue group receive messages round-robin, useful for load balancing workers:

```rust
let _w1 = client.subscribe_queue("jobs.>", "workers").await?;
let _w2 = client.subscribe_queue("jobs.>", "workers").await?;
```

## Iterating a subscription with StreamExt

`SubscriptionStream` implements `futures::Stream`:

```rust
use futures::StreamExt;

let mut sub = client.subscribe("events.>").await?;
while let Some(msg) = sub.next().await {
    println!("{}: {}", msg.topic, String::from_utf8_lossy(&msg.payload));
}
```

## Embedding a broker (`server` feature)

```rust
use std::sync::Arc;
use pubsub::server::{BrokerConfig, CancellationToken, Router, SubscriptionRegistry, TcpTransportListener};
use pubsub::{Client, ConnectOptions};

let shutdown = CancellationToken::new();
let registry = Arc::new(SubscriptionRegistry::new());
let router = Arc::new(Router::new(registry.clone()));

let listener = TcpTransportListener::bind(
    &BrokerConfig::default(),
    router,
    registry,
    shutdown.clone(),
).await?;

let addr = listener.local_addr()?;
tokio::spawn(async move { listener.run().await });

let client = Client::connect(ConnectOptions::new(addr)).await?;
```

See the [`chat-server` example](examples/chat-server.rs) for a full working demo.

## License

MIT or Apache-2.0
