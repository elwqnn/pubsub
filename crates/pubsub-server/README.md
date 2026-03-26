# pubsub-server

Standalone pub/sub message broker binary.

Run it as a long-lived process; any number of clients connect to it over TCP using the [`pubsub`](https://crates.io/crates/pubsub) client library.

## Install

```sh
cargo install pubsub-server
```

## Run

```sh
# Default: binds to 127.0.0.1:4222
pubsub-server

# Listen on all interfaces
pubsub-server --bind 0.0.0.0:4222

# With logging
RUST_LOG=info pubsub-server
```

## CLI reference

```
Usage: pubsub-server [OPTIONS]

Options:
  -b, --bind <ADDR>              Address to bind the listener to [default: 127.0.0.1:4222]
      --max-connections <N>      Maximum number of concurrent connections [default: 1024]
      --channel-buffer <N>       Per-session message channel buffer size [default: 256]
      --max-frame-size <BYTES>   Maximum wire frame size in bytes [default: 65536]
  -h, --help                     Print help
  -V, --version                  Print version
```

## Connecting clients

```toml
[dependencies]
pubsub = "0.1"
```

```rust
use pubsub::{Client, ConnectOptions};

let client = Client::connect(ConnectOptions::from_host("localhost:4222")).await?;
client.publish("hello", "world").await?;
```

## Signals

The server handles `SIGINT` (Ctrl+C) and `SIGTERM` gracefully. Active sessions are given a short drain period before the process exits.

## Logging

Log level is controlled via the `RUST_LOG` environment variable:

```sh
RUST_LOG=debug pubsub-server   # verbose
RUST_LOG=warn  pubsub-server   # errors and warnings only
```

## License

MIT
