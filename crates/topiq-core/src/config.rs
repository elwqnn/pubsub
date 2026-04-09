use std::net::SocketAddr;
use std::time::Duration;

/// Configuration for the topiq broker.
#[derive(Debug, Clone)]
pub struct BrokerConfig {
    /// Address to bind the listener to.
    pub bind_addr: SocketAddr,
    /// Maximum number of concurrent connections.
    pub max_connections: usize,
    /// Per-session channel buffer size (messages).
    pub channel_buffer_size: usize,
    /// Maximum wire frame size in bytes.
    pub max_frame_size: usize,
    /// Maximum subscriptions per connection (0 = unlimited).
    pub max_subscriptions_per_connection: usize,
    /// Interval between server-initiated pings to detect dead connections.
    pub ping_interval: Duration,
    /// How long to wait for a pong before considering the connection dead.
    pub ping_timeout: Duration,
    /// Timeout for graceful shutdown drain.
    pub shutdown_timeout: Duration,
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            bind_addr: ([127, 0, 0, 1], 4222).into(),
            max_connections: 1024,
            channel_buffer_size: 256,
            max_frame_size: 64 * 1024,
            max_subscriptions_per_connection: 1024,
            ping_interval: Duration::from_secs(30),
            ping_timeout: Duration::from_secs(5),
            shutdown_timeout: Duration::from_secs(5),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let cfg = BrokerConfig::default();
        assert_eq!(cfg.bind_addr.port(), 4222);
        assert_eq!(cfg.max_connections, 1024);
        assert_eq!(cfg.channel_buffer_size, 256);
        assert_eq!(cfg.max_frame_size, 65536);
        assert_eq!(cfg.max_subscriptions_per_connection, 1024);
        assert_eq!(cfg.ping_interval, Duration::from_secs(30));
        assert_eq!(cfg.ping_timeout, Duration::from_secs(5));
        assert_eq!(cfg.shutdown_timeout, Duration::from_secs(5));
    }
}
