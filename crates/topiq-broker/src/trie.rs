use std::collections::HashMap;

use topiq_core::SubscriptionId;

/// A trie node for hierarchical topic matching.
///
/// Supports exact matching, single-token wildcards (`*`), and
/// suffix wildcards (`>` = match one or more remaining tokens).
#[derive(Debug, Default)]
struct TrieNode {
    /// Children keyed by literal token value.
    children: HashMap<String, TrieNode>,
    /// Child for single-token wildcard `*`.
    wildcard: Option<Box<TrieNode>>,
    /// Subscriptions that used `>` at this level (match everything below).
    full_wildcard_subs: Vec<SubscriptionId>,
    /// Subscriptions that terminate exactly at this node.
    subscriptions: Vec<SubscriptionId>,
}

impl TrieNode {
    fn is_empty(&self) -> bool {
        self.children.is_empty()
            && self.wildcard.is_none()
            && self.full_wildcard_subs.is_empty()
            && self.subscriptions.is_empty()
    }
}

/// A trie for matching publish topics against subscription patterns.
///
/// Insert subscription patterns (which may contain `*` and `>`),
/// then query with concrete topics to find all matching subscription ids.
#[derive(Debug, Default)]
pub struct TopicTrie {
    root: TrieNode,
}

impl TopicTrie {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a subscription pattern into the trie.
    pub fn insert(&mut self, subject: &str, sid: SubscriptionId) {
        let mut node = &mut self.root;

        for token in subject.split('.') {
            match token {
                ">" => {
                    node.full_wildcard_subs.push(sid);
                    return;
                }
                "*" => {
                    node = node.wildcard.get_or_insert_with(|| Box::new(TrieNode::default()));
                }
                literal => {
                    node = node
                        .children
                        .entry(literal.to_owned())
                        .or_default();
                }
            }
        }

        node.subscriptions.push(sid);
    }

    /// Remove a subscription from the trie. Returns true if found and removed.
    pub fn remove(&mut self, subject: &str, sid: SubscriptionId) -> bool {
        Self::remove_recursive(&mut self.root, &mut subject.split('.'), sid)
    }

    fn remove_recursive<'a>(
        node: &mut TrieNode,
        tokens: &mut impl Iterator<Item = &'a str>,
        sid: SubscriptionId,
    ) -> bool {
        let Some(token) = tokens.next() else {
            // End of pattern: remove from exact-match subscriptions.
            return remove_from_vec(&mut node.subscriptions, sid);
        };

        match token {
            ">" => remove_from_vec(&mut node.full_wildcard_subs, sid),
            "*" => {
                let Some(wildcard_node) = node.wildcard.as_mut() else {
                    return false;
                };
                let removed = Self::remove_recursive(wildcard_node, tokens, sid);
                if removed && wildcard_node.is_empty() {
                    node.wildcard = None;
                }
                removed
            }
            literal => {
                let Some(child) = node.children.get_mut(literal) else {
                    return false;
                };
                let removed = Self::remove_recursive(child, tokens, sid);
                if removed && child.is_empty() {
                    node.children.remove(literal);
                }
                removed
            }
        }
    }

    /// Find all subscription ids matching a concrete (non-wildcard) topic.
    pub fn matches(&self, topic: &str) -> Vec<SubscriptionId> {
        let mut result = Vec::with_capacity(4);
        let tokens: Vec<&str> = topic.split('.').collect();
        Self::collect_matches(&self.root, &tokens, 0, &mut result);
        result
    }

    fn collect_matches(
        node: &TrieNode,
        tokens: &[&str],
        depth: usize,
        result: &mut Vec<SubscriptionId>,
    ) {
        // `>` at any level matches all remaining tokens (one or more).
        if depth < tokens.len() {
            result.extend_from_slice(&node.full_wildcard_subs);
        }

        // If we've consumed all tokens, collect exact matches.
        if depth == tokens.len() {
            result.extend_from_slice(&node.subscriptions);
            return;
        }

        let token = tokens[depth];

        // Try literal child.
        if let Some(child) = node.children.get(token) {
            Self::collect_matches(child, tokens, depth + 1, result);
        }

        // Try `*` wildcard (matches exactly one token).
        if let Some(wildcard_node) = &node.wildcard {
            Self::collect_matches(wildcard_node, tokens, depth + 1, result);
        }
    }
}

fn remove_from_vec(vec: &mut Vec<SubscriptionId>, sid: SubscriptionId) -> bool {
    if let Some(pos) = vec.iter().position(|s| *s == sid) {
        vec.swap_remove(pos);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sid(n: u64) -> SubscriptionId {
        SubscriptionId(n)
    }

    fn sorted(mut v: Vec<SubscriptionId>) -> Vec<SubscriptionId> {
        v.sort_by_key(|s| s.0);
        v
    }

    #[test]
    fn exact_match() {
        let mut trie = TopicTrie::new();
        trie.insert("a.b.c", sid(1));
        assert_eq!(trie.matches("a.b.c"), vec![sid(1)]);
        assert!(trie.matches("a.b").is_empty());
        assert!(trie.matches("a.b.c.d").is_empty());
    }

    #[test]
    fn single_token_wildcard() {
        let mut trie = TopicTrie::new();
        trie.insert("a.*.c", sid(1));
        assert_eq!(trie.matches("a.b.c"), vec![sid(1)]);
        assert_eq!(trie.matches("a.x.c"), vec![sid(1)]);
        assert!(trie.matches("a.b.d").is_empty());
        assert!(trie.matches("a.b.c.d").is_empty());
    }

    #[test]
    fn suffix_wildcard() {
        let mut trie = TopicTrie::new();
        trie.insert("a.>", sid(1));
        assert_eq!(trie.matches("a.b"), vec![sid(1)]);
        assert_eq!(trie.matches("a.b.c"), vec![sid(1)]);
        assert_eq!(trie.matches("a.b.c.d"), vec![sid(1)]);
        assert!(trie.matches("a").is_empty()); // `>` matches ONE or more
        assert!(trie.matches("b.c").is_empty());
    }

    #[test]
    fn root_suffix_wildcard() {
        let mut trie = TopicTrie::new();
        trie.insert(">", sid(1));
        assert_eq!(trie.matches("a"), vec![sid(1)]);
        assert_eq!(trie.matches("a.b"), vec![sid(1)]);
        assert_eq!(trie.matches("x.y.z"), vec![sid(1)]);
    }

    #[test]
    fn multiple_subscriptions_same_pattern() {
        let mut trie = TopicTrie::new();
        trie.insert("a.b", sid(1));
        trie.insert("a.b", sid(2));
        assert_eq!(sorted(trie.matches("a.b")), vec![sid(1), sid(2)]);
    }

    #[test]
    fn multiple_patterns_match_same_topic() {
        let mut trie = TopicTrie::new();
        trie.insert("a.b.c", sid(1));
        trie.insert("a.*.c", sid(2));
        trie.insert("a.>", sid(3));
        assert_eq!(sorted(trie.matches("a.b.c")), vec![sid(1), sid(2), sid(3)]);
    }

    #[test]
    fn star_at_different_positions() {
        let mut trie = TopicTrie::new();
        trie.insert("*.b.c", sid(1));
        trie.insert("a.*.c", sid(2));
        trie.insert("a.b.*", sid(3));
        let result = sorted(trie.matches("a.b.c"));
        assert_eq!(result, vec![sid(1), sid(2), sid(3)]);
    }

    #[test]
    fn no_match_returns_empty() {
        let mut trie = TopicTrie::new();
        trie.insert("a.b.c", sid(1));
        assert!(trie.matches("x.y.z").is_empty());
    }

    #[test]
    fn remove_exact() {
        let mut trie = TopicTrie::new();
        trie.insert("a.b", sid(1));
        trie.insert("a.b", sid(2));
        assert!(trie.remove("a.b", sid(1)));
        assert_eq!(trie.matches("a.b"), vec![sid(2)]);
    }

    #[test]
    fn remove_wildcard() {
        let mut trie = TopicTrie::new();
        trie.insert("a.>", sid(1));
        assert!(trie.remove("a.>", sid(1)));
        assert!(trie.matches("a.b").is_empty());
    }

    #[test]
    fn remove_star() {
        let mut trie = TopicTrie::new();
        trie.insert("a.*.c", sid(1));
        assert!(trie.remove("a.*.c", sid(1)));
        assert!(trie.matches("a.b.c").is_empty());
    }

    #[test]
    fn remove_nonexistent_returns_false() {
        let mut trie = TopicTrie::new();
        trie.insert("a.b", sid(1));
        assert!(!trie.remove("a.b", sid(99)));
        assert!(!trie.remove("x.y", sid(1)));
    }

    #[test]
    fn empty_trie_matches_nothing() {
        let trie = TopicTrie::new();
        assert!(trie.matches("anything").is_empty());
    }

    #[test]
    fn single_token_subject() {
        let mut trie = TopicTrie::new();
        trie.insert("hello", sid(1));
        assert_eq!(trie.matches("hello"), vec![sid(1)]);
        assert!(trie.matches("world").is_empty());
    }

    #[test]
    fn star_only() {
        let mut trie = TopicTrie::new();
        trie.insert("*", sid(1));
        assert_eq!(trie.matches("anything"), vec![sid(1)]);
        assert!(trie.matches("a.b").is_empty()); // * matches exactly one token
    }

    #[test]
    fn deep_nesting() {
        let mut trie = TopicTrie::new();
        trie.insert("a.b.c.d.e.f", sid(1));
        trie.insert("a.b.c.>", sid(2));
        let result = sorted(trie.matches("a.b.c.d.e.f"));
        assert_eq!(result, vec![sid(1), sid(2)]);
    }
}
