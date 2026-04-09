use std::fmt;

use tokio::sync::mpsc;

use crate::message::Message;

/// Unique identifier for a subscription within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(pub u64);

impl fmt::Display for SubscriptionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sub:{}", self.0)
    }
}

/// A tagged message destined for a specific subscription.
///
/// When multiple subscriptions share a session channel, this tag
/// lets the session map messages back to the correct subscription id.
pub type TaggedMessage = (SubscriptionId, Message);

/// Sender half that tags messages with a subscription id before sending.
#[derive(Debug, Clone)]
pub struct SubscriptionSender {
    sid: SubscriptionId,
    tx: mpsc::Sender<TaggedMessage>,
}

impl SubscriptionSender {
    pub fn new(sid: SubscriptionId, tx: mpsc::Sender<TaggedMessage>) -> Self {
        Self { sid, tx }
    }

    pub fn sid(&self) -> SubscriptionId {
        self.sid
    }

    /// Try to send a message without blocking. Returns false if the channel is full.
    pub fn try_send(&self, msg: Message) -> bool {
        self.tx.try_send((self.sid, msg)).is_ok()
    }

    pub async fn send(&self, msg: Message) -> bool {
        self.tx.send((self.sid, msg)).await.is_ok()
    }

    /// Check if the receiving end has been dropped.
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;
    use crate::topic::Subject;

    #[tokio::test]
    async fn subscription_sender_tags_messages() {
        let (tx, mut rx) = mpsc::channel(8);
        let sender = SubscriptionSender::new(SubscriptionId(42), tx);

        let msg = Message::new(Subject::new("test").unwrap(), Bytes::from("data"));
        assert!(sender.try_send(msg));

        let (sid, received) = rx.recv().await.unwrap();
        assert_eq!(sid, SubscriptionId(42));
        assert_eq!(received.payload, Bytes::from("data"));
    }

    #[tokio::test]
    async fn sender_detects_closed_channel() {
        let (tx, rx) = mpsc::channel::<TaggedMessage>(1);
        let sender = SubscriptionSender::new(SubscriptionId(1), tx);
        drop(rx);
        assert!(sender.is_closed());
    }

    #[test]
    fn subscription_id_display() {
        assert_eq!(SubscriptionId(7).to_string(), "sub:7");
    }
}
