# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Test Commands

```sh
cargo build --workspace                              # build all crates
cargo test --workspace                               # run all tests
cargo test -p topiq-broker                          # test a single crate
cargo test -p topiq-broker topic_trie               # run tests matching a name
cargo clippy --workspace --all-targets -D warnings   # lint (CI runs this)
cargo bench --workspace                              # run all benchmarks
cargo bench -p topiq-broker                         # bench a single crate
```

The `topiq-server` binary: `cargo run -p topiq-server -- --bind 127.0.0.1:4222`

Examples require the `server` feature: `cargo run -p topiq --features server --example chat-server`

## Architecture

Lightweight TCP pub/sub message broker. Seven workspace crates, layered bottom-up:

```
topiq (facade, re-exports client + optional server)
  topiq-client        -- async Client, SubscriptionStream, ConnectOptions
  topiq-server        -- standalone CLI binary (clap)
  topiq-transport-tcp -- TcpTransportListener, Session (per-connection loop)
  topiq-broker        -- Router, SubscriptionRegistry, TopicTrie, QueueGroupSelector
  topiq-protocol      -- Frame enum, TopiqCodec (MessagePack over tokio-util)
  topiq-core          -- Message, Subject, SubscriptionId, BrokerConfig, TopiqError
```

**Only `topiq` and `topiq-server` are public crates.** The rest are internal.

### Key data flow

1. Client sends TCP bytes -> `TopiqCodec` decodes into `Frame`
2. `Session` dispatches frames: `Frame::Publish` -> `Router::route()`
3. `Router` queries `TopicTrie` for matching subscriptions, fans out via MPSC channels
4. For ACK-aware subscriptions, `Router` assigns a msg_id via `AckTracker::track()` before sending
5. `Session` encodes outbound `Frame::Message` (with optional msg_id) back to the subscriber's TCP stream

### At-least-once delivery (ACK protocol)

Opt-in per subscription. Fire-and-forget (no ACK) is the default.

- **Client API**: `subscribe_with_ack(subject)` creates an ACK-aware subscription. Use `next_delivery()` to get a `Delivery` with an `ack()` method. `next_message()` still works but discards the msg_id.
- **Wire protocol**: `Frame::Subscribe` carries optional `ack_policy`, `Frame::Message` carries optional `msg_id`, `Frame::Ack { msg_id }` is the ACK frame. Protocol version 2 (v1 frames still accepted).
- **Broker internals**: `AckTracker` (DashMap-based) tracks pending messages. `run_redelivery_scanner` is a background task that periodically drains expired entries and redelivers or drops after `max_redeliveries`.
- **Message IDs**: assigned at delivery time (not publish time) -- the same published message gets different IDs for different subscribers.

### Feature flags

The `topiq` crate has one feature: `server` -- enables embedding the broker in-process (adds `topiq-broker` + `topiq-transport-tcp` deps).

### Important patterns

- **Subject wildcards**: `*` matches one token, `>` matches the rest. Validated at construction via `Subject::new()`.
- **Queue groups**: round-robin delivery to one subscriber per group via `QueueGroupSelector`.
- **Request/reply**: client generates a unique inbox subject for `reply_to`.
- **Concurrency**: `DashMap` for subscription lookups and ACK tracking, `RwLock` around trie writes, `Semaphore` for connection limits, `CancellationToken` for shutdown.
- **Serialization**: MessagePack (rmp-serde) with `serde_bytes` for efficient binary payloads.
- **Edition 2024**, MSRV 1.85, resolver v3.

## CI

GitHub Actions runs `cargo test --workspace` and `cargo clippy --workspace --all-targets -D warnings` on push/PR to main.
