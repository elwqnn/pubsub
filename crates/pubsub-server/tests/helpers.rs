use std::net::SocketAddr;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use pubsub_core::BrokerConfig;
use pubsub_broker::{Router, SubscriptionRegistry};
use pubsub_transport_tcp::TcpTransportListener;

/// Start a test broker on a random port. Returns the bound address and shutdown token.
pub async fn start_test_server() -> (SocketAddr, CancellationToken) {
    let shutdown = CancellationToken::new();
    let registry = Arc::new(SubscriptionRegistry::new());
    let router = Arc::new(Router::new(registry.clone()));

    let config = BrokerConfig {
        bind_addr: ([127, 0, 0, 1], 0).into(), // OS-assigned port
        max_connections: 64,
        channel_buffer_size: 64,
        max_frame_size: 64 * 1024,
        ..Default::default()
    };

    let listener = TcpTransportListener::bind(&config, router, registry, shutdown.clone())
        .await
        .expect("failed to bind test server");

    let addr = listener.local_addr().expect("failed to get local addr");

    tokio::spawn(async move {
        let _ = listener.run().await;
    });

    (addr, shutdown)
}
