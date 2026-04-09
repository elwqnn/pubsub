use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use topiq_core::{BrokerConfig, TopiqError};
use topiq_broker::{Router, SubscriptionRegistry};

use crate::connection::Connection;
use crate::session::{Session, SessionConfig};

/// TCP transport listener that accepts connections and spawns sessions.
pub struct TcpTransportListener {
    tcp_listener: TcpListener,
    connection_limit: Arc<Semaphore>,
    router: Arc<Router>,
    registry: Arc<SubscriptionRegistry>,
    shutdown: CancellationToken,
    channel_buffer_size: usize,
    max_frame_size: usize,
    max_subscriptions_per_connection: usize,
    ping_interval: Duration,
    ping_timeout: Duration,
}

impl TcpTransportListener {
    pub async fn bind(
        config: &BrokerConfig,
        router: Arc<Router>,
        registry: Arc<SubscriptionRegistry>,
        shutdown: CancellationToken,
    ) -> Result<Self, TopiqError> {
        let tcp_listener = TcpListener::bind(config.bind_addr).await?;
        info!(addr = %config.bind_addr, "listening");

        Ok(Self {
            tcp_listener,
            connection_limit: Arc::new(Semaphore::new(config.max_connections)),
            router,
            registry,
            shutdown,
            channel_buffer_size: config.channel_buffer_size,
            max_frame_size: config.max_frame_size,
            max_subscriptions_per_connection: config.max_subscriptions_per_connection,
            ping_interval: config.ping_interval,
            ping_timeout: config.ping_timeout,
        })
    }

    /// Accept connections until shutdown.
    pub async fn run(&self) -> Result<(), TopiqError> {
        let mut backoff = Duration::from_millis(1);

        loop {
            // Race permit acquisition against shutdown so we don't block
            // shutdown when at max connections.
            let permit = tokio::select! {
                permit = self.connection_limit.clone().acquire_owned() => {
                    permit.unwrap()
                }
                _ = self.shutdown.cancelled() => {
                    return Ok(());
                }
            };

            let stream = tokio::select! {
                result = self.tcp_listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            backoff = Duration::from_millis(1);
                            stream
                        }
                        Err(e) => {
                            error!(error = %e, "accept error, backing off");
                            tokio::time::sleep(backoff).await;
                            backoff = (backoff * 2).min(Duration::from_millis(64));
                            drop(permit);
                            continue;
                        }
                    }
                }
                _ = self.shutdown.cancelled() => {
                    return Ok(());
                }
            };

            let connection = Connection::new(stream, self.max_frame_size);
            let session = Session::new(
                connection,
                self.router.clone(),
                self.registry.clone(),
                self.shutdown.clone(),
                SessionConfig {
                    channel_buffer_size: self.channel_buffer_size,
                    max_subscriptions: self.max_subscriptions_per_connection,
                    ping_interval: self.ping_interval,
                    ping_timeout: self.ping_timeout,
                },
            );

            tokio::spawn(async move {
                session.run().await;
                drop(permit);
            });
        }
    }

    pub fn local_addr(&self) -> Result<std::net::SocketAddr, TopiqError> {
        self.tcp_listener.local_addr().map_err(TopiqError::from)
    }
}
