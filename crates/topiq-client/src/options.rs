use std::net::SocketAddr;

/// How the client should resolve the broker address.
#[derive(Debug, Clone)]
pub(crate) enum ConnectAddr {
    SocketAddr(SocketAddr),
    Host(String),
}

/// Options for connecting to a topiq broker.
#[derive(Debug, Clone)]
pub struct ConnectOptions {
    pub(crate) addr: ConnectAddr,
    pub(crate) max_frame_size: usize,
    pub(crate) channel_buffer_size: usize,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            addr: ConnectAddr::SocketAddr(([127, 0, 0, 1], 4222).into()),
            max_frame_size: 64 * 1024,
            channel_buffer_size: 256,
        }
    }
}

impl ConnectOptions {
    /// Create options with a specific socket address.
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr: ConnectAddr::SocketAddr(addr),
            ..Default::default()
        }
    }

    /// Create options from a host string (e.g., `"127.0.0.1:4222"`, `"localhost:4222"`).
    ///
    /// DNS resolution happens at connect time.
    pub fn from_host(host: impl Into<String>) -> Self {
        Self {
            addr: ConnectAddr::Host(host.into()),
            ..Default::default()
        }
    }

    /// Set the maximum wire frame size in bytes.
    pub fn max_frame_size(mut self, size: usize) -> Self {
        self.max_frame_size = size;
        self
    }

    /// Set the per-subscription channel buffer size (messages).
    pub fn channel_buffer_size(mut self, size: usize) -> Self {
        self.channel_buffer_size = size;
        self
    }
}
