use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::time::{interval, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use topiq_core::{
    Message, TopiqError, Subject, SubscriptionId, SubscriptionSender, TaggedMessage,
};
use topiq_broker::{Router, SubscriptionRegistry};
use topiq_protocol::Frame;

use crate::connection::Connection;

/// Configuration parameters for a session, extracted to avoid too many constructor args.
pub struct SessionConfig {
    pub channel_buffer_size: usize,
    pub max_subscriptions: usize,
    pub ping_interval: Duration,
    pub ping_timeout: Duration,
}

/// Per-connection session handler.
///
/// Runs a `select!` loop over:
/// 1. Inbound frames from the client
/// 2. Outbound messages from subscriptions
/// 3. Periodic ping keepalive
/// 4. Shutdown signal
pub struct Session {
    peer_addr: SocketAddr,
    framed: futures::stream::SplitStream<
        tokio_util::codec::Framed<tokio::net::TcpStream, topiq_protocol::TopiqCodec>,
    >,
    sink: futures::stream::SplitSink<
        tokio_util::codec::Framed<tokio::net::TcpStream, topiq_protocol::TopiqCodec>,
        Frame,
    >,
    router: Arc<Router>,
    registry: Arc<SubscriptionRegistry>,
    shutdown: CancellationToken,
    /// Maps client-chosen sid -> server-internal SubscriptionId.
    subscriptions: HashMap<u64, SubscriptionId>,
    /// Maps server-internal SubscriptionId -> client sid (for outbound routing).
    internal_to_client: HashMap<SubscriptionId, u64>,
    outbound_rx: mpsc::Receiver<TaggedMessage>,
    outbound_tx: mpsc::Sender<TaggedMessage>,
    max_subscriptions: usize,
    ping_interval: Duration,
    ping_timeout: Duration,
}

impl Session {
    pub fn new(
        connection: Connection,
        router: Arc<Router>,
        registry: Arc<SubscriptionRegistry>,
        shutdown: CancellationToken,
        config: SessionConfig,
    ) -> Self {
        let peer_addr = connection.peer_addr();
        let framed = connection.into_framed();
        let (sink, stream) = framed.split();
        let (outbound_tx, outbound_rx) = mpsc::channel(config.channel_buffer_size);

        Self {
            peer_addr,
            framed: stream,
            sink,
            router,
            registry,
            shutdown,
            subscriptions: HashMap::new(),
            internal_to_client: HashMap::new(),
            outbound_rx,
            outbound_tx,
            max_subscriptions: config.max_subscriptions,
            ping_interval: config.ping_interval,
            ping_timeout: config.ping_timeout,
        }
    }

    pub async fn run(mut self) {
        info!(peer = %self.peer_addr, "session started");

        let mut ping_timer = interval(self.ping_interval);
        ping_timer.reset(); // Don't fire immediately
        let mut awaiting_pong = false;
        let mut pong_deadline: Option<Instant> = None;

        loop {
            // Calculate sleep for pong timeout
            let pong_timeout = async {
                match pong_deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending().await,
                }
            };

            tokio::select! {
                frame = self.framed.next() => {
                    match frame {
                        Some(Ok(frame)) => {
                            // Any valid frame resets pong tracking
                            if awaiting_pong {
                                awaiting_pong = false;
                                pong_deadline = None;
                            }
                            if let Err(e) = self.handle_frame(frame).await {
                                warn!(peer = %self.peer_addr, error = %e, "frame handling error");
                                let _ = self.send_frame(Frame::Err { message: e.to_string() }).await;
                            }
                        }
                        Some(Err(e)) => {
                            warn!(peer = %self.peer_addr, error = %e, "connection error");
                            break;
                        }
                        None => {
                            debug!(peer = %self.peer_addr, "client disconnected");
                            break;
                        }
                    }
                }

                tagged = self.outbound_rx.recv() => {
                    let Some(first) = tagged else {
                        break;
                    };

                    // Feed the first message, then drain any buffered messages
                    // and flush once -- avoids a syscall per message.
                    if let Err(e) = self.feed_tagged(first).await {
                        error!(peer = %self.peer_addr, error = %e, "failed to send message");
                        break;
                    }
                    while let Ok(tagged) = self.outbound_rx.try_recv() {
                        if let Err(e) = self.feed_tagged(tagged).await {
                            error!(peer = %self.peer_addr, error = %e, "failed to send message");
                            break;
                        }
                    }
                    if let Err(e) = self.sink.flush().await {
                        error!(peer = %self.peer_addr, error = %e, "failed to flush");
                        break;
                    }
                }

                _ = ping_timer.tick() => {
                    if awaiting_pong {
                        // Already waiting for a pong -- don't double-ping
                        continue;
                    }
                    if let Err(e) = self.send_frame(Frame::Ping).await {
                        error!(peer = %self.peer_addr, error = %e, "failed to send ping");
                        break;
                    }
                    awaiting_pong = true;
                    pong_deadline = Some(Instant::now() + self.ping_timeout);
                }

                _ = pong_timeout => {
                    warn!(peer = %self.peer_addr, "ping timeout -- closing connection");
                    break;
                }

                _ = self.shutdown.cancelled() => {
                    debug!(peer = %self.peer_addr, "shutdown signal received");
                    break;
                }
            }
        }

        self.cleanup().await;
        info!(peer = %self.peer_addr, "session ended");
    }

    /// Encode a tagged outbound message into the write buffer without flushing.
    async fn feed_tagged(&mut self, tagged: TaggedMessage) -> Result<(), TopiqError> {
        let (internal_sid, msg) = tagged;
        let Some(&client_sid) = self.internal_to_client.get(&internal_sid) else {
            return Ok(());
        };
        let reply_to = msg.reply_to.as_ref().map(|s: &Subject| s.as_str().to_owned());
        let frame = Frame::Message {
            topic: msg.topic.as_str().to_owned(),
            sid: client_sid,
            payload: msg.payload,
            reply_to,
        };
        self.sink.feed(frame).await
    }

    async fn send_frame(&mut self, frame: Frame) -> Result<(), TopiqError> {
        self.sink.send(frame).await
    }

    async fn handle_frame(&mut self, frame: Frame) -> Result<(), TopiqError> {
        match frame {
            Frame::Publish {
                topic,
                payload,
                reply_to,
            } => self.handle_publish(topic, payload, reply_to).await,

            Frame::Subscribe {
                sid,
                subject,
                queue_group,
            } => self.handle_subscribe(sid, subject, queue_group).await,

            Frame::Unsubscribe { sid } => self.handle_unsubscribe(sid).await,

            Frame::Ping => self.send_frame(Frame::Pong).await,

            Frame::Pong => Ok(()),

            other => Err(TopiqError::Protocol(format!(
                "unexpected frame from client: {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    async fn handle_publish(
        &mut self,
        topic: String,
        payload: Bytes,
        reply_to: Option<String>,
    ) -> Result<(), TopiqError> {
        let subject = Subject::new(&topic)?;
        let reply_subject = reply_to.map(|r| Subject::new(&r)).transpose()?;

        let msg = match reply_subject {
            Some(reply) => Message::with_reply(subject, payload, reply),
            None => Message::new(subject, payload),
        };

        self.router.route(&msg).await;
        Ok(())
    }

    async fn handle_subscribe(
        &mut self,
        client_sid: u64,
        subject: String,
        queue_group: Option<String>,
    ) -> Result<(), TopiqError> {
        if self.max_subscriptions > 0 && self.subscriptions.len() >= self.max_subscriptions {
            return Err(TopiqError::Protocol(format!(
                "subscription limit reached (max {})",
                self.max_subscriptions
            )));
        }

        let subject = Subject::new(&subject)?;

        // Generate a server-internal unique ID.
        let internal_sid = self.registry.next_id();
        let sender = SubscriptionSender::new(internal_sid, self.outbound_tx.clone());

        self.registry
            .subscribe(subject, queue_group, sender)
            .await;

        self.subscriptions.insert(client_sid, internal_sid);
        self.internal_to_client.insert(internal_sid, client_sid);

        debug!(peer = %self.peer_addr, client_sid, internal_sid = internal_sid.0, "subscribed");
        self.send_frame(Frame::Ok).await
    }

    async fn handle_unsubscribe(&mut self, client_sid: u64) -> Result<(), TopiqError> {
        if let Some(internal_sid) = self.subscriptions.remove(&client_sid) {
            self.internal_to_client.remove(&internal_sid);
            self.registry.unsubscribe(internal_sid).await;
            debug!(peer = %self.peer_addr, client_sid, "unsubscribed");
        }
        self.send_frame(Frame::Ok).await
    }

    async fn cleanup(&mut self) {
        for (_, internal_sid) in self.subscriptions.drain() {
            self.registry.unsubscribe(internal_sid).await;
        }
        self.internal_to_client.clear();
        debug!(peer = %self.peer_addr, "session cleaned up");
    }
}
