use std::sync::atomic::{AtomicUsize, Ordering};

use topiq_core::SubscriptionId;

/// Round-robin selector for queue group members.
///
/// When multiple subscribers share a queue group, only one
/// receives each message. This rotates through members fairly.
#[derive(Debug)]
pub struct QueueGroupSelector {
    counter: AtomicUsize,
}

impl QueueGroupSelector {
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }

    /// Pick the next member from the list. Returns None if members is empty.
    pub fn select<'a>(&self, members: &'a [SubscriptionId]) -> Option<&'a SubscriptionId> {
        if members.is_empty() {
            return None;
        }
        let idx = self.counter.fetch_add(1, Ordering::Relaxed) % members.len();
        Some(&members[idx])
    }

    /// Return the index of the next member for a group of `len` items.
    pub fn select_index(&self, len: usize) -> usize {
        self.counter.fetch_add(1, Ordering::Relaxed) % len.max(1)
    }
}

impl Default for QueueGroupSelector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sid(n: u64) -> SubscriptionId {
        SubscriptionId(n)
    }

    #[test]
    fn round_robin_selection() {
        let selector = QueueGroupSelector::new();
        let members = vec![sid(1), sid(2), sid(3)];

        let first = *selector.select(&members).unwrap();
        let second = *selector.select(&members).unwrap();
        let third = *selector.select(&members).unwrap();
        let fourth = *selector.select(&members).unwrap();

        assert_eq!(first, sid(1));
        assert_eq!(second, sid(2));
        assert_eq!(third, sid(3));
        assert_eq!(fourth, sid(1)); // wraps around
    }

    #[test]
    fn empty_members_returns_none() {
        let selector = QueueGroupSelector::new();
        assert!(selector.select(&[]).is_none());
    }

    #[test]
    fn single_member() {
        let selector = QueueGroupSelector::new();
        let members = vec![sid(42)];
        assert_eq!(*selector.select(&members).unwrap(), sid(42));
        assert_eq!(*selector.select(&members).unwrap(), sid(42));
    }
}
