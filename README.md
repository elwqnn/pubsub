# topiq

[![Crates.io](https://img.shields.io/crates/v/topiq.svg)](https://crates.io/crates/topiq)
[![docs.rs](https://docs.rs/topiq/badge.svg)](https://docs.rs/topiq)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![CI](https://github.com/Elwqnn/topiq/actions/workflows/ci.yml/badge.svg)](https://github.com/Elwqnn/topiq/actions/workflows/ci.yml)

A fast, lightweight publish/subscribe message broker for Rust. Simple API, minimal setup, built on Tokio.

Use it when your services or tasks need to talk to each other without being tightly coupled, whether they live in the same process or across the network. Supports wildcards, queue groups for load balancing, and request/reply.

> This is not a replacement for a production message broker like NATS or Kafka. There is no persistence, clustering, or delivery guarantees. It is designed for internal communication where speed and simplicity matter more than absolute reliability.

## Quick start

```toml
[dependencies]
topiq = "0.1"
tokio = { version = "1", features = ["full"] }
```

```rust
use topiq::{Client, ConnectOptions};

#[tokio::main]
async fn main() -> topiq::Result<()> {
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
cargo install topiq-server

topiq-server                        # binds to 127.0.0.1:4222
topiq-server --bind 0.0.0.0:4222    # listen on all interfaces
RUST_LOG=info topiq-server          # with logging
```

Or embed the broker directly in your application with the `server` feature. See the [`topiq` crate docs](crates/topiq/README.md) for details.

## Features

- **Wildcards**: subscribe to `sensors.*.room1` or `sensors.>` to match multiple subjects
- **Queue groups**: load-balance messages across a pool of subscribers
- **Request/reply**: synchronous-style RPC over async pub/sub
- **Embeddable**: run the broker in-process with `features = ["server"]`

## Documentation

- [**Client API guide**](crates/topiq/README.md): connecting, subscribing, wildcards, request/reply, queue groups, embedding a broker
- [**Server CLI reference**](crates/topiq-server/README.md): running and configuring the standalone broker
- [**Examples**](crates/topiq/examples): runnable demos

## Crates

| Crate | Description |
|-------|-------------|
| [`topiq`](crates/topiq) | Client library (start here) |
| [`topiq-server`](crates/topiq-server) | Standalone broker binary |

The remaining crates (`topiq-core`, `topiq-protocol`, `topiq-broker`, `topiq-client`, `topiq-transport-tcp`) are internal implementation details.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
