use std::net::SocketAddr;

use clap::Parser;

/// A lightweight pub-sub message broker.
#[derive(Parser, Debug)]
#[command(name = "pubsub-server", version, about)]
pub struct Cli {
    /// Address to bind the listener to.
    #[arg(short, long, default_value = "127.0.0.1:4222")]
    pub bind: SocketAddr,

    /// Maximum number of concurrent connections.
    #[arg(long, default_value = "1024")]
    pub max_connections: usize,

    /// Per-session message channel buffer size.
    #[arg(long, default_value = "256")]
    pub channel_buffer: usize,

    /// Maximum wire frame size in bytes.
    #[arg(long, default_value = "65536")]
    pub max_frame_size: usize,
}
