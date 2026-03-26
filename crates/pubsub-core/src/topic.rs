use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::PubSubError;

/// A validated subject string used for topic-based pub/sub routing.
///
/// Subjects use `.` as a token separator. Wildcards are supported:
/// - `*` matches exactly one token
/// - `>` matches one or more tokens (must be last token)
///
/// Examples: `sensors.temp.room1`, `sensors.*.room1`, `sensors.>`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Subject(Arc<str>);

impl Serialize for Subject {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Subject {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Subject::new(&s).map_err(serde::de::Error::custom)
    }
}

impl Subject {
    pub fn new(raw: &str) -> crate::Result<Self> {
        validate_subject(raw)?;
        Ok(Self(Arc::from(raw)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn tokens(&self) -> impl Iterator<Item = &str> {
        self.0.split('.')
    }

    pub fn is_wildcard(&self) -> bool {
        self.0.contains('*') || self.0.contains('>')
    }

    pub fn token_count(&self) -> usize {
        self.0.split('.').count()
    }
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Subject {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Maximum allowed subject length in bytes.
const MAX_SUBJECT_LEN: usize = 256;

fn validate_subject(raw: &str) -> crate::Result<()> {
    if raw.is_empty() {
        return Err(PubSubError::InvalidSubject {
            reason: "subject cannot be empty".into(),
        });
    }

    if raw.len() > MAX_SUBJECT_LEN {
        return Err(PubSubError::InvalidSubject {
            reason: format!(
                "subject exceeds maximum length of {} bytes (got {})",
                MAX_SUBJECT_LEN,
                raw.len()
            ),
        });
    }

    if raw.starts_with('.') || raw.ends_with('.') {
        return Err(PubSubError::InvalidSubject {
            reason: "subject cannot start or end with '.'".into(),
        });
    }

    if raw.contains("..") {
        return Err(PubSubError::InvalidSubject {
            reason: "subject cannot contain empty tokens (double dots)".into(),
        });
    }

    let tokens: Vec<&str> = raw.split('.').collect();
    for (i, token) in tokens.iter().enumerate() {
        if token.is_empty() {
            return Err(PubSubError::InvalidSubject {
                reason: "subject contains an empty token".into(),
            });
        }

        if *token == ">" && i != tokens.len() - 1 {
            return Err(PubSubError::InvalidSubject {
                reason: "'>' wildcard must be the last token".into(),
            });
        }

        if (token.contains('*') || token.contains('>')) && token.len() > 1 {
            return Err(PubSubError::InvalidSubject {
                reason: format!("wildcard token '{}' must stand alone", token),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_simple_subject() {
        assert!(Subject::new("sensors.temp.room1").is_ok());
    }

    #[test]
    fn valid_single_token() {
        assert!(Subject::new("hello").is_ok());
    }

    #[test]
    fn valid_wildcard_star() {
        assert!(Subject::new("sensors.*.room1").is_ok());
    }

    #[test]
    fn valid_wildcard_gt() {
        assert!(Subject::new("sensors.>").is_ok());
    }

    #[test]
    fn valid_star_only() {
        assert!(Subject::new("*").is_ok());
    }

    #[test]
    fn valid_gt_only() {
        assert!(Subject::new(">").is_ok());
    }

    #[test]
    fn invalid_empty() {
        assert!(Subject::new("").is_err());
    }

    #[test]
    fn invalid_double_dot() {
        assert!(Subject::new("sensors..temp").is_err());
    }

    #[test]
    fn invalid_leading_dot() {
        assert!(Subject::new(".sensors").is_err());
    }

    #[test]
    fn invalid_trailing_dot() {
        assert!(Subject::new("sensors.").is_err());
    }

    #[test]
    fn invalid_gt_not_last() {
        assert!(Subject::new("sensors.>.temp").is_err());
    }

    #[test]
    fn invalid_mixed_wildcard() {
        assert!(Subject::new("sensors.te*").is_err());
    }

    #[test]
    fn tokens_returns_segments() {
        let s = Subject::new("a.b.c").unwrap();
        let tokens: Vec<&str> = s.tokens().collect();
        assert_eq!(tokens, vec!["a", "b", "c"]);
    }

    #[test]
    fn is_wildcard_detection() {
        assert!(!Subject::new("a.b").unwrap().is_wildcard());
        assert!(Subject::new("a.*").unwrap().is_wildcard());
        assert!(Subject::new("a.>").unwrap().is_wildcard());
    }

    #[test]
    fn display_roundtrip() {
        let s = Subject::new("foo.bar").unwrap();
        assert_eq!(s.to_string(), "foo.bar");
    }

    #[test]
    fn serde_roundtrip() {
        let s = Subject::new("a.b.c").unwrap();
        let encoded = rmp_serde::to_vec(&s).unwrap();
        let decoded: Subject = rmp_serde::from_slice(&encoded).unwrap();
        assert_eq!(s, decoded);
    }
}
