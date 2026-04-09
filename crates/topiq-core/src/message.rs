use bytes::Bytes;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::topic::Subject;

/// A message flowing through the pub/sub system.
///
/// Payloads are opaque bytes -- the broker never inspects them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub topic: Subject,
    pub payload: Bytes,
    pub reply_to: Option<Subject>,
}

impl Serialize for Message {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Message", 3)?;
        s.serialize_field("topic", &self.topic)?;
        s.serialize_field("payload", self.payload.as_ref() as &[u8])?;
        s.serialize_field("reply_to", &self.reply_to)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for Message {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct MessageHelper {
            topic: Subject,
            payload: Vec<u8>,
            reply_to: Option<Subject>,
        }
        let helper = MessageHelper::deserialize(deserializer)?;
        Ok(Message {
            topic: helper.topic,
            payload: Bytes::from(helper.payload),
            reply_to: helper.reply_to,
        })
    }
}

impl Message {
    pub fn new(topic: Subject, payload: Bytes) -> Self {
        Self {
            topic,
            payload,
            reply_to: None,
        }
    }

    pub fn with_reply(topic: Subject, payload: Bytes, reply_to: Subject) -> Self {
        Self {
            topic,
            payload,
            reply_to: Some(reply_to),
        }
    }

    pub fn payload_len(&self) -> usize {
        self.payload.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_message() {
        let msg = Message::new(
            Subject::new("test.topic").unwrap(),
            Bytes::from("hello"),
        );
        assert_eq!(msg.topic.as_str(), "test.topic");
        assert_eq!(msg.payload, Bytes::from("hello"));
        assert!(msg.reply_to.is_none());
    }

    #[test]
    fn message_with_reply() {
        let msg = Message::with_reply(
            Subject::new("request").unwrap(),
            Bytes::from("ping"),
            Subject::new("reply.inbox").unwrap(),
        );
        assert_eq!(msg.reply_to.unwrap().as_str(), "reply.inbox");
    }

    #[test]
    fn payload_len() {
        let msg = Message::new(Subject::new("t").unwrap(), Bytes::from("12345"));
        assert_eq!(msg.payload_len(), 5);
    }
}
