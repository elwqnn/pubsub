# pubsub

A fast, lightweight publish/subscribe message broker for Rust. Simple API, minimal setup, built on Tokio.

Use it when your services or tasks need to talk to each other without being tightly coupled, whether they live in the same process or across the network. Supports wildcards, queue groups for load balancing, and request/reply.

> This is not a replacement for a production message broker like NATS or Kafka. There is no persistence, clustering, or delivery guarantees. It is designed for internal communication where speed and simplicity matter more than absolute reliability.

## Quick start

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

## Running the server

```sh
cargo install pubsub-server

pubsub-server                        # binds to 127.0.0.1:4222
pubsub-server --bind 0.0.0.0:4222   # listen on all interfaces
RUST_LOG=info pubsub-server          # with logging
```

Or embed the broker directly in your application with the `server` feature. See the [`pubsub` crate docs](crates/pubsub/README.md) for details.

## Features

- **Wildcards**: subscribe to `sensors.*.room1` or `sensors.>` to match multiple subjects
- **Queue groups**: load-balance messages across a pool of subscribers
- **Request/reply**: synchronous-style RPC over async pub/sub
- **Embeddable**: run the broker in-process with `features = ["server"]`

## Documentation

- [**Client API guide**](crates/pubsub/README.md): connecting, subscribing, wildcards, request/reply, queue groups, embedding a broker
- [**Server CLI reference**](crates/pubsub-server/README.md): running and configuring the standalone broker
- [**Examples**](crates/pubsub/examples): runnable demos

## Crates

| Crate | Description |
|-------|-------------|
| [`pubsub`](crates/pubsub) | Client library (start here) |
| [`pubsub-server`](crates/pubsub-server) | Standalone broker binary |

The remaining crates (`pubsub-core`, `pubsub-protocol`, `pubsub-broker`, `pubsub-client`, `pubsub-transport-tcp`) are internal implementation details.

## License

MIT
