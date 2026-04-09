use std::net::SocketAddr;

use tokio::net::TcpStream;
use tokio_util::codec::Framed;

use topiq_protocol::TopiqCodec;

/// A framed TCP connection that reads/writes `Frame` values.
pub struct Connection {
    framed: Framed<TcpStream, TopiqCodec>,
    peer_addr: SocketAddr,
}

impl Connection {
    pub fn new(stream: TcpStream, max_frame_size: usize) -> Self {
        let peer_addr = stream
            .peer_addr()
            .unwrap_or_else(|_| ([0, 0, 0, 0], 0).into());
        let codec = TopiqCodec::new(max_frame_size);
        Self {
            framed: Framed::new(stream, codec),
            peer_addr,
        }
    }

    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    /// Split into the underlying Framed for use in select! loops.
    pub fn into_framed(self) -> Framed<TcpStream, TopiqCodec> {
        self.framed
    }
}
