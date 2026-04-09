use bytes::Bytes;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Current wire protocol version.
pub const PROTOCOL_VERSION: u8 = 1;

/// A frame on the wire. This is the unit of communication between client and broker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// Client -> Broker: publish a message to a topic.
    Publish {
        topic: String,
        payload: Bytes,
        reply_to: Option<String>,
    },

    /// Client -> Broker: subscribe to a subject pattern.
    Subscribe {
        sid: u64,
        subject: String,
        queue_group: Option<String>,
    },

    /// Client -> Broker: unsubscribe from a subscription.
    Unsubscribe { sid: u64 },

    /// Broker -> Client: deliver a message to a subscriber.
    Message {
        topic: String,
        sid: u64,
        payload: Bytes,
        reply_to: Option<String>,
    },

    /// Bidirectional: keepalive ping.
    Ping,

    /// Bidirectional: keepalive pong.
    Pong,

    /// Broker -> Client: operation succeeded.
    Ok,

    /// Broker -> Client: operation failed.
    Err { message: String },
}

// Serialize borrows from Frame to avoid payload copy and string clones.
impl Serialize for Frame {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        FrameSerHelper::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Frame {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        FrameDeHelper::deserialize(deserializer).map(Frame::from)
    }
}

/// Serialize-only helper: borrows from Frame to avoid copies.
/// Uses `serde_bytes` for payload fields to encode as msgpack binary.
#[derive(Serialize)]
enum FrameSerHelper<'a> {
    Publish {
        topic: &'a str,
        #[serde(with = "serde_bytes")]
        payload: &'a [u8],
        reply_to: &'a Option<String>,
    },
    Subscribe {
        sid: u64,
        subject: &'a str,
        queue_group: &'a Option<String>,
    },
    Unsubscribe {
        sid: u64,
    },
    Message {
        topic: &'a str,
        sid: u64,
        #[serde(with = "serde_bytes")]
        payload: &'a [u8],
        reply_to: &'a Option<String>,
    },
    Ping,
    Pong,
    Ok,
    Err {
        message: &'a str,
    },
}

impl<'a> From<&'a Frame> for FrameSerHelper<'a> {
    fn from(frame: &'a Frame) -> Self {
        match frame {
            Frame::Publish {
                topic,
                payload,
                reply_to,
            } => FrameSerHelper::Publish {
                topic,
                payload: payload.as_ref(),
                reply_to,
            },
            Frame::Subscribe {
                sid,
                subject,
                queue_group,
            } => FrameSerHelper::Subscribe {
                sid: *sid,
                subject,
                queue_group,
            },
            Frame::Unsubscribe { sid } => FrameSerHelper::Unsubscribe { sid: *sid },
            Frame::Message {
                topic,
                sid,
                payload,
                reply_to,
            } => FrameSerHelper::Message {
                topic,
                sid: *sid,
                payload: payload.as_ref(),
                reply_to,
            },
            Frame::Ping => FrameSerHelper::Ping,
            Frame::Pong => FrameSerHelper::Pong,
            Frame::Ok => FrameSerHelper::Ok,
            Frame::Err { message } => FrameSerHelper::Err { message },
        }
    }
}

/// Deserialize-only helper: owns data, uses `serde_bytes` for binary decoding.
#[derive(Deserialize)]
enum FrameDeHelper {
    Publish {
        topic: String,
        #[serde(with = "serde_bytes")]
        payload: Vec<u8>,
        reply_to: Option<String>,
    },
    Subscribe {
        sid: u64,
        subject: String,
        queue_group: Option<String>,
    },
    Unsubscribe {
        sid: u64,
    },
    Message {
        topic: String,
        sid: u64,
        #[serde(with = "serde_bytes")]
        payload: Vec<u8>,
        reply_to: Option<String>,
    },
    Ping,
    Pong,
    Ok,
    Err {
        message: String,
    },
}

impl From<FrameDeHelper> for Frame {
    fn from(helper: FrameDeHelper) -> Self {
        match helper {
            FrameDeHelper::Publish {
                topic,
                payload,
                reply_to,
            } => Frame::Publish {
                topic,
                payload: Bytes::from(payload),
                reply_to,
            },
            FrameDeHelper::Subscribe {
                sid,
                subject,
                queue_group,
            } => Frame::Subscribe {
                sid,
                subject,
                queue_group,
            },
            FrameDeHelper::Unsubscribe { sid } => Frame::Unsubscribe { sid },
            FrameDeHelper::Message {
                topic,
                sid,
                payload,
                reply_to,
            } => Frame::Message {
                topic,
                sid,
                payload: Bytes::from(payload),
                reply_to,
            },
            FrameDeHelper::Ping => Frame::Ping,
            FrameDeHelper::Pong => Frame::Pong,
            FrameDeHelper::Ok => Frame::Ok,
            FrameDeHelper::Err { message } => Frame::Err { message },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(frame: &Frame) -> Frame {
        let encoded = rmp_serde::to_vec(frame).unwrap();
        rmp_serde::from_slice(&encoded).unwrap()
    }

    #[test]
    fn roundtrip_publish() {
        let frame = Frame::Publish {
            topic: "test.topic".into(),
            payload: Bytes::from("hello"),
            reply_to: None,
        };
        assert_eq!(roundtrip(&frame), frame);
    }

    #[test]
    fn roundtrip_publish_with_reply() {
        let frame = Frame::Publish {
            topic: "req".into(),
            payload: Bytes::from("data"),
            reply_to: Some("inbox.123".into()),
        };
        assert_eq!(roundtrip(&frame), frame);
    }

    #[test]
    fn roundtrip_subscribe() {
        let frame = Frame::Subscribe {
            sid: 42,
            subject: "sensors.>".into(),
            queue_group: Some("workers".into()),
        };
        assert_eq!(roundtrip(&frame), frame);
    }

    #[test]
    fn roundtrip_unsubscribe() {
        let frame = Frame::Unsubscribe { sid: 7 };
        assert_eq!(roundtrip(&frame), frame);
    }

    #[test]
    fn roundtrip_message() {
        let frame = Frame::Message {
            topic: "sensors.temp".into(),
            sid: 1,
            payload: Bytes::from("25.3"),
            reply_to: None,
        };
        assert_eq!(roundtrip(&frame), frame);
    }

    #[test]
    fn roundtrip_ping_pong() {
        assert_eq!(roundtrip(&Frame::Ping), Frame::Ping);
        assert_eq!(roundtrip(&Frame::Pong), Frame::Pong);
    }

    #[test]
    fn roundtrip_ok() {
        assert_eq!(roundtrip(&Frame::Ok), Frame::Ok);
    }

    #[test]
    fn roundtrip_err() {
        let frame = Frame::Err {
            message: "bad request".into(),
        };
        assert_eq!(roundtrip(&frame), frame);
    }

    #[test]
    fn empty_payload() {
        let frame = Frame::Publish {
            topic: "t".into(),
            payload: Bytes::new(),
            reply_to: None,
        };
        assert_eq!(roundtrip(&frame), frame);
    }
}
