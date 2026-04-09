use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use dashmap::DashMap;
use futures::Stream;
use tokio::sync::mpsc;

use topiq_core::{Message, TopiqError};
use topiq_protocol::Frame;

/// An async stream of messages for a subscription.
///
/// Implements [`futures::Stream`] so it can be used with `StreamExt`
/// combinators like `.map()`, `.filter()`, and in `select!` loops.
pub struct SubscriptionStream {
    sid: u64,
    rx: mpsc::Receiver<Message>,
    writer_tx: mpsc::Sender<Frame>,
    subscriptions: Arc<DashMap<u64, mpsc::Sender<Message>>>,
}

impl SubscriptionStream {
    pub(crate) fn new(
        sid: u64,
        rx: mpsc::Receiver<Message>,
        writer_tx: mpsc::Sender<Frame>,
        subscriptions: Arc<DashMap<u64, mpsc::Sender<Message>>>,
    ) -> Self {
        Self {
            sid,
            rx,
            writer_tx,
            subscriptions,
        }
    }

    /// The subscription id.
    pub fn sid(&self) -> u64 {
        self.sid
    }

    /// Receive the next message. Returns None if the subscription is closed.
    pub async fn next_message(&mut self) -> Option<Message> {
        self.rx.recv().await
    }

    /// Unsubscribe from this subscription.
    ///
    /// Removes the local subscription entry and sends an unsubscribe
    /// frame to the broker.
    pub async fn unsubscribe(&self) -> Result<(), TopiqError> {
        self.subscriptions.remove(&self.sid);
        self.writer_tx
            .send(Frame::Unsubscribe { sid: self.sid })
            .await
            .map_err(|_| TopiqError::ConnectionClosed)
    }
}

impl Stream for SubscriptionStream {
    type Item = Message;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}
