use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use tokio::sync::RwLock;

use topiq_core::{Subject, SubscriptionId, SubscriptionSender};

use crate::trie::TopicTrie;

/// Metadata for a registered subscription.
struct SubscriptionEntry {
    subject: Subject,
    queue_group: Option<Arc<str>>,
    sender: SubscriptionSender,
}

/// Thread-safe subscription registry.
///
/// Manages the topic trie and subscription state. The trie is behind an
/// `RwLock` (reads concurrent, writes exclusive). Individual subscription
/// entries are in a `DashMap` for O(1) lookup during fan-out.
pub struct SubscriptionRegistry {
    trie: RwLock<TopicTrie>,
    entries: DashMap<SubscriptionId, SubscriptionEntry>,
    next_id: AtomicU64,
}

impl SubscriptionRegistry {
    pub fn new() -> Self {
        Self {
            trie: RwLock::new(TopicTrie::new()),
            entries: DashMap::new(),
            next_id: AtomicU64::new(1),
        }
    }

    /// Register a new subscription. Returns the assigned subscription id.
    pub async fn subscribe(
        &self,
        subject: Subject,
        queue_group: Option<String>,
        sender: SubscriptionSender,
    ) -> SubscriptionId {
        let sid = sender.sid();
        let queue_group: Option<Arc<str>> = queue_group.map(|s| Arc::from(s.as_str()));

        {
            let mut trie = self.trie.write().await;
            trie.insert(subject.as_str(), sid);
        }

        self.entries.insert(
            sid,
            SubscriptionEntry {
                subject,
                queue_group,
                sender,
            },
        );

        sid
    }

    /// Remove a subscription. Returns true if it existed.
    pub async fn unsubscribe(&self, sid: SubscriptionId) -> bool {
        let entry = self.entries.remove(&sid);
        let Some((_, entry)) = entry else {
            return false;
        };

        let mut trie = self.trie.write().await;
        trie.remove(entry.subject.as_str(), sid);
        true
    }

    /// Find all subscription ids matching a concrete topic.
    pub async fn match_topic(&self, topic: &Subject) -> Vec<SubscriptionId> {
        let trie = self.trie.read().await;
        trie.matches(topic.as_str())
    }

    /// Get the sender for a subscription, if it still exists.
    pub fn get_sender(&self, sid: SubscriptionId) -> Option<SubscriptionSender> {
        self.entries.get(&sid).map(|e| e.sender.clone())
    }

    /// Get the queue group for a subscription, if any.
    pub fn get_queue_group(&self, sid: SubscriptionId) -> Option<Arc<str>> {
        self.entries
            .get(&sid)
            .and_then(|e| e.queue_group.clone())
    }

    /// Get both sender and queue group in a single DashMap lookup.
    pub fn get_routing_info(
        &self,
        sid: SubscriptionId,
    ) -> Option<(SubscriptionSender, Option<Arc<str>>)> {
        self.entries
            .get(&sid)
            .map(|e| (e.sender.clone(), e.queue_group.clone()))
    }

    /// Generate a unique internal subscription id.
    pub fn next_id(&self) -> SubscriptionId {
        SubscriptionId(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Number of active subscriptions.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for SubscriptionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use topiq_core::TaggedMessage;

    use super::*;

    fn make_sender(sid: SubscriptionId) -> (SubscriptionSender, mpsc::Receiver<TaggedMessage>) {
        let (tx, rx) = mpsc::channel(16);
        (SubscriptionSender::new(sid, tx), rx)
    }

    #[tokio::test]
    async fn subscribe_and_match() {
        let registry = SubscriptionRegistry::new();
        let sid = SubscriptionId(1);
        let (sender, _rx) = make_sender(sid);

        registry
            .subscribe(Subject::new("a.b").unwrap(), None, sender)
            .await;

        let matches = registry.match_topic(&Subject::new("a.b").unwrap()).await;
        assert_eq!(matches, vec![sid]);
    }

    #[tokio::test]
    async fn unsubscribe_removes_from_trie() {
        let registry = SubscriptionRegistry::new();
        let sid = SubscriptionId(1);
        let (sender, _rx) = make_sender(sid);

        registry
            .subscribe(Subject::new("a.b").unwrap(), None, sender)
            .await;
        assert!(registry.unsubscribe(sid).await);

        let matches = registry.match_topic(&Subject::new("a.b").unwrap()).await;
        assert!(matches.is_empty());
        assert!(registry.is_empty());
    }

    #[tokio::test]
    async fn unsubscribe_nonexistent_returns_false() {
        let registry = SubscriptionRegistry::new();
        assert!(!registry.unsubscribe(SubscriptionId(99)).await);
    }

    #[tokio::test]
    async fn get_sender_works() {
        let registry = SubscriptionRegistry::new();
        let sid = SubscriptionId(1);
        let (sender, _rx) = make_sender(sid);

        registry
            .subscribe(Subject::new("a.b").unwrap(), None, sender)
            .await;

        assert!(registry.get_sender(sid).is_some());
        assert!(registry.get_sender(SubscriptionId(99)).is_none());
    }

    #[tokio::test]
    async fn next_id_increments() {
        let registry = SubscriptionRegistry::new();
        let id1 = registry.next_id();
        let id2 = registry.next_id();
        assert_ne!(id1, id2);
        assert_eq!(id1.0 + 1, id2.0);
    }

    #[tokio::test]
    async fn wildcard_matching_through_registry() {
        let registry = SubscriptionRegistry::new();

        let sid1 = SubscriptionId(1);
        let (sender1, _rx1) = make_sender(sid1);
        registry
            .subscribe(Subject::new("a.>").unwrap(), None, sender1)
            .await;

        let sid2 = SubscriptionId(2);
        let (sender2, _rx2) = make_sender(sid2);
        registry
            .subscribe(Subject::new("a.b").unwrap(), None, sender2)
            .await;

        let mut matches = registry.match_topic(&Subject::new("a.b").unwrap()).await;
        matches.sort_by_key(|s| s.0);
        assert_eq!(matches, vec![sid1, sid2]);
    }
}
