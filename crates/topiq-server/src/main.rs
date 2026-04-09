mod cli;
mod shutdown;

use std::sync::Arc;

use clap::Parser;
use tokio_util::sync::CancellationToken;
use tracing::info;

use topiq_core::BrokerConfig;
use topiq_broker::{Router, SubscriptionRegistry};
use topiq_transport_tcp::TcpTransportListener;

use crate::cli::Cli;
use crate::shutdown::wait_for_shutdown;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "topiq=info".parse().unwrap()),
        )
        .init();

    let args = Cli::parse();

    let config = BrokerConfig {
        bind_addr: args.bind,
        max_connections: args.max_connections,
        channel_buffer_size: args.channel_buffer,
        max_frame_size: args.max_frame_size,
        ..Default::default()
    };

    let shutdown = CancellationToken::new();
    let registry = Arc::new(SubscriptionRegistry::new());
    let router = Arc::new(Router::new(registry.clone()));

    let listener =
        TcpTransportListener::bind(&config, router, registry, shutdown.clone()).await?;

    let addr = listener.local_addr()?;
    info!("topiq-server listening on {}", addr);

    tokio::select! {
        result = listener.run() => {
            if let Err(e) = result {
                tracing::error!(error = %e, "listener error");
            }
        }
        _ = wait_for_shutdown(shutdown.clone()) => {}
    }

    info!("shutting down");

    // Give sessions time to drain.
    tokio::time::sleep(config.shutdown_timeout).await;

    info!("shutdown complete");
    Ok(())
}
