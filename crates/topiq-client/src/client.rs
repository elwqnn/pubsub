use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_util::codec::Framed;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use topiq_core::{Message, TopiqError, Subject};
use topiq_protocol::{Frame, TopiqCodec};

use crate::options::{ConnectAddr, ConnectOptions};
use crate::subscription_stream::SubscriptionStream;

/// Global counter for generating unique inbox subjects.
static INBOX_COUNTER: AtomicU64 = AtomicU64::new(0);

fn new_inbox() -> String {
    let id = INBOX_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("_INBOX.{id}")
}

/// Async client for the topiq message broker.
///
/// Internally spawns reader and writer tasks. Thread-safe and cloneable.
#[derive(Clone)]
pub struct Client {
    writer_tx: mpsc::Sender<Frame>,
    subscriptions: Arc<DashMap<u64, mpsc::Sender<Message>>>,
    next_sid: Arc<AtomicU64>,
    shutdown: CancellationToken,
    channel_buffer_size: usize,
}

impl Client {
    /// Connect to a topiq broker.
    pub async fn connect(options: ConnectOptions) -> Result<Self, TopiqError> {
        let stream = match &options.addr {
            ConnectAddr::SocketAddr(addr) => TcpStream::connect(addr).await?,
            ConnectAddr::Host(host) => TcpStream::connect(host.as_str()).await?,
        };
        let framed = Framed::new(stream, TopiqCodec::new(options.max_frame_size));
        let (sink, stream) = framed.split();

        let (writer_tx, writer_rx) = mpsc::channel::<Frame>(options.channel_buffer_size);
        let subscriptions: Arc<DashMap<u64, mpsc::Sender<Message>>> = Arc::new(DashMap::new());
        let shutdown = CancellationToken::new();

        // Spawn writer task.
        let writer_shutdown = shutdown.clone();
        tokio::spawn(Self::writer_loop(sink, writer_rx, writer_shutdown));

        // Spawn reader task -- give it a sender to the writer for Pong responses.
        let reader_subs = subscriptions.clone();
        let reader_shutdown = shutdown.clone();
        let pong_tx = writer_tx.clone();
        tokio::spawn(Self::reader_loop(
            stream,
            reader_subs,
            reader_shutdown,
            pong_tx,
        ));

        Ok(Self {
            writer_tx,
            subscriptions,
            next_sid: Arc::new(AtomicU64::new(1)),
            shutdown,
            channel_buffer_size: options.channel_buffer_size,
        })
    }

    /// Publish a message to a topic.
    ///
    /// Payload accepts `&str`, `&[u8]`, `String`, `Vec<u8>`, or `Bytes`.
    pub async fn publish(
        &self,
        topic: &str,
        payload: impl AsRef<[u8]>,
    ) -> Result<(), TopiqError> {
        let frame = Frame::Publish {
            topic: topic.to_owned(),
            payload: Bytes::copy_from_slice(payload.as_ref()),
            reply_to: None,
        };
        self.send(frame).await
    }

    /// Publish a message with a reply-to subject.
    pub async fn publish_with_reply(
        &self,
        topic: &str,
        payload: impl AsRef<[u8]>,
        reply_to: &str,
    ) -> Result<(), TopiqError> {
        let frame = Frame::Publish {
            topic: topic.to_owned(),
            payload: Bytes::copy_from_slice(payload.as_ref()),
            reply_to: Some(reply_to.to_owned()),
        };
        self.send(frame).await
    }

    /// Perform a request/reply exchange.
    ///
    /// Publishes `payload` to `subject` with an auto-generated reply inbox,
    /// waits for a single response, and returns it. The inbox subscription
    /// is cleaned up automatically.
    pub async fn request(
        &self,
        subject: &str,
        payload: impl AsRef<[u8]>,
        timeout_duration: Duration,
    ) -> Result<Message, TopiqError> {
        let inbox = new_inbox();
        let mut sub = self.subscribe(&inbox).await?;
        self.publish_with_reply(subject, payload, &inbox).await?;

        let result = timeout(timeout_duration, sub.next_message()).await;
        let _ = sub.unsubscribe().await;

        match result {
            Ok(Some(msg)) => Ok(msg),
            Ok(None) => Err(TopiqError::ConnectionClosed),
            Err(_) => Err(TopiqError::Timeout),
        }
    }

    /// Subscribe to a subject pattern. Returns a stream of messages.
    pub async fn subscribe(&self, subject: &str) -> Result<SubscriptionStream, TopiqError> {
        self.subscribe_inner(subject, None).await
    }

    /// Subscribe to a subject with a queue group.
    pub async fn subscribe_queue(
        &self,
        subject: &str,
        queue_group: &str,
    ) -> Result<SubscriptionStream, TopiqError> {
        self.subscribe_inner(subject, Some(queue_group.to_owned()))
            .await
    }

    async fn subscribe_inner(
        &self,
        subject: &str,
        queue_group: Option<String>,
    ) -> Result<SubscriptionStream, TopiqError> {
        // Validate subject locally.
        let _ = Subject::new(subject)?;

        let sid = self.next_sid.fetch_add(1, Ordering::Relaxed);
        let (msg_tx, msg_rx) = mpsc::channel(self.channel_buffer_size);

        self.subscriptions.insert(sid, msg_tx);

        let frame = Frame::Subscribe {
            sid,
            subject: subject.to_owned(),
            queue_group,
        };
        self.send(frame).await?;

        Ok(SubscriptionStream::new(
            sid,
            msg_rx,
            self.writer_tx.clone(),
            self.subscriptions.clone(),
        ))
    }

    /// Unsubscribe from a subscription by sid.
    pub async fn unsubscribe(&self, sid: u64) -> Result<(), TopiqError> {
        self.subscriptions.remove(&sid);
        let frame = Frame::Unsubscribe { sid };
        self.send(frame).await
    }

    /// Close the client connection.
    pub async fn close(&self) {
        self.shutdown.cancel();
    }

    async fn send(&self, frame: Frame) -> Result<(), TopiqError> {
        self.writer_tx
            .send(frame)
            .await
            .map_err(|_| TopiqError::ConnectionClosed)
    }

    async fn writer_loop(
        mut sink: futures::stream::SplitSink<Framed<TcpStream, TopiqCodec>, Frame>,
        mut rx: mpsc::Receiver<Frame>,
        shutdown: CancellationToken,
    ) {
        loop {
            tokio::select! {
                frame = rx.recv() => {
                    let Some(frame) = frame else { break };
                    if let Err(e) = sink.send(frame).await {
                        error!(error = %e, "writer error");
                        break;
                    }
                }
                _ = shutdown.cancelled() => break,
            }
        }
        debug!("writer task ended");
    }

    async fn reader_loop(
        mut stream: futures::stream::SplitStream<Framed<TcpStream, TopiqCodec>>,
        subscriptions: Arc<DashMap<u64, mpsc::Sender<Message>>>,
        shutdown: CancellationToken,
        pong_tx: mpsc::Sender<Frame>,
    ) {
        loop {
            tokio::select! {
                frame = stream.next() => {
                    match frame {
                        Some(Ok(Frame::Message { topic, sid, payload, reply_to })) => {
                            let Ok(subject) = Subject::new(&topic) else {
                                warn!(topic, "received message with invalid topic");
                                continue;
                            };
                            let reply_subject = reply_to
                                .and_then(|r| Subject::new(&r).ok());
                            let msg = match reply_subject {
                                Some(reply) => Message::with_reply(subject, payload, reply),
                                None => Message::new(subject, payload),
                            };
                            if let Some(tx) = subscriptions.get(&sid) {
                                let _ = tx.try_send(msg);
                            }
                        }
                        Some(Ok(Frame::Ping)) => {
                            let _ = pong_tx.try_send(Frame::Pong);
                        }
                        Some(Ok(Frame::Ok)) => {} // Ack, ignore.
                        Some(Ok(Frame::Err { message })) => {
                            warn!(message, "server error");
                        }
                        Some(Ok(_)) => {} // Ignore unexpected frames.
                        Some(Err(e)) => {
                            error!(error = %e, "reader error");
                            break;
                        }
                        None => {
                            debug!("server disconnected");
                            break;
                        }
                    }
                }
                _ = shutdown.cancelled() => break,
            }
        }

        debug!("reader task ended");
        shutdown.cancel(); // Signal writer to stop too.
    }
}
