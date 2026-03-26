use thiserror::Error;

/// Errors that can occur in the pubsub system.
#[derive(Debug, Error)]
pub enum PubSubError {
    #[error("invalid subject: {reason}")]
    InvalidSubject { reason: String },

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("unsupported protocol version: {version}")]
    UnsupportedVersion { version: u8 },

    #[error("frame too large: {size} bytes (max {max})")]
    FrameTooLarge { size: usize, max: usize },

    #[error("transport error: {0}")]
    Transport(#[from] std::io::Error),

    #[error("codec error: {0}")]
    Codec(String),

    #[error("connection closed")]
    ConnectionClosed,

    #[error("request timed out")]
    Timeout,

    #[error("buffer full for subscription {sid}")]
    Backpressure { sid: u64 },
}

pub type Result<T> = std::result::Result<T, PubSubError>;
