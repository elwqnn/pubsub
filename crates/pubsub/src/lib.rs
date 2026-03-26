//! Lightweight TCP-based pub/sub message broker.
//!
//! This crate re-exports the consumer-facing API so users only need
//! a single dependency.
//!
//! # Quick start
//!
//! ```ignore
//! use std::time::Duration;
//! use pubsub::{Client, ConnectOptions};
//!
//! #[tokio::main]
//! async fn main() -> pubsub::Result<()> {
//!     let client = Client::connect(ConnectOptions::default()).await?;
//!
//!     // Publish (accepts &str, String, Vec<u8>, Bytes, etc.)
//!     client.publish("greet", "hello world").await?;
//!
//!     // Subscribe
//!     let mut sub = client.subscribe("greet").await?;
//!     let msg = sub.next_message().await.unwrap();
//!     assert_eq!(msg.payload.as_ref(), b"hello world");
//!
//!     // Request/reply
//!     let reply = client.request("service.echo", "ping", Duration::from_secs(5)).await?;
//!
//!     client.close().await;
//!     Ok(())
//! }
//! ```

// Re-export bytes::Bytes so users don't need a separate `bytes` dependency.
pub use bytes::Bytes;

// Client types.
pub use pubsub_client::{Client, ConnectOptions, SubscriptionStream};

// Core types that users interact with.
pub use pubsub_core::{Message, PubSubError, Result, Subject};

/// Server-side types for embedding a broker in your application.
///
/// Enable with the `server` feature flag:
/// ```toml
/// [dependencies]
/// pubsub = { version = "0.1", features = ["server"] }
/// ```
#[cfg(feature = "server")]
pub mod server {
    pub use pubsub_broker::{Router, RoutingResult, SubscriptionRegistry};
    pub use pubsub_core::BrokerConfig;
    pub use pubsub_transport_tcp::TcpTransportListener;
    pub use tokio_util::sync::CancellationToken;
}
