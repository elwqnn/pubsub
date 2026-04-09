use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use topiq_core::TopiqError;

use crate::frame::{Frame, PROTOCOL_VERSION};

/// Minimum frame: 1 byte version + at least 1 byte msgpack.
const MIN_FRAME_BODY: usize = 2;

/// Header: 4 bytes for frame length.
const HEADER_LEN: usize = 4;

/// Codec for encoding/decoding `Frame` values on the wire.
///
/// Wire format: `[4B frame body length (big-endian)][1B version][N bytes msgpack Frame]`
///
/// The 4-byte length covers the version byte + msgpack body.
pub struct TopiqCodec {
    max_frame_size: usize,
}

impl TopiqCodec {
    pub fn new(max_frame_size: usize) -> Self {
        Self { max_frame_size }
    }
}

impl Decoder for TopiqCodec {
    type Item = Frame;
    type Error = TopiqError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Frame>, TopiqError> {
        // Need at least the 4-byte header to read frame length.
        if src.len() < HEADER_LEN {
            return Ok(None);
        }

        // Peek at the length without consuming.
        let body_len = u32::from_be_bytes([src[0], src[1], src[2], src[3]]) as usize;

        if body_len < MIN_FRAME_BODY {
            return Err(TopiqError::Protocol(format!(
                "frame body too small: {} bytes",
                body_len
            )));
        }

        if body_len > self.max_frame_size {
            return Err(TopiqError::FrameTooLarge {
                size: body_len,
                max: self.max_frame_size,
            });
        }

        let total_len = HEADER_LEN + body_len;
        if src.len() < total_len {
            // Reserve space for the rest of the frame to avoid repeated allocations.
            src.reserve(total_len - src.len());
            return Ok(None);
        }

        // Consume the header.
        src.advance(HEADER_LEN);

        // Read the version byte.
        let version = src[0];
        if version != PROTOCOL_VERSION {
            src.advance(body_len);
            return Err(TopiqError::UnsupportedVersion { version });
        }
        src.advance(1);

        // Deserialize the msgpack body.
        let msgpack_len = body_len - 1;
        let msgpack_data = &src[..msgpack_len];
        let frame = rmp_serde::from_slice(msgpack_data)
            .map_err(|e| TopiqError::Codec(e.to_string()))?;

        src.advance(msgpack_len);
        Ok(Some(frame))
    }
}

impl Encoder<Frame> for TopiqCodec {
    type Error = TopiqError;

    fn encode(&mut self, item: Frame, dst: &mut BytesMut) -> Result<(), TopiqError> {
        let msgpack_data =
            rmp_serde::to_vec(&item).map_err(|e| TopiqError::Codec(e.to_string()))?;

        let body_len = 1 + msgpack_data.len(); // version byte + msgpack

        if body_len > self.max_frame_size {
            return Err(TopiqError::FrameTooLarge {
                size: body_len,
                max: self.max_frame_size,
            });
        }

        dst.reserve(HEADER_LEN + body_len);
        dst.put_u32(body_len as u32);
        dst.put_u8(PROTOCOL_VERSION);
        dst.put_slice(&msgpack_data);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;

    fn codec() -> TopiqCodec {
        TopiqCodec::new(64 * 1024)
    }

    fn encode_frame(frame: &Frame) -> BytesMut {
        let mut codec = codec();
        let mut buf = BytesMut::new();
        codec.encode(frame.clone(), &mut buf).unwrap();
        buf
    }

    #[test]
    fn roundtrip_through_codec() {
        let frame = Frame::Publish {
            topic: "test".into(),
            payload: Bytes::from("hello"),
            reply_to: None,
        };

        let mut buf = encode_frame(&frame);
        let mut codec = codec();
        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded, frame);
    }

    #[test]
    fn partial_frame_returns_none() {
        let frame = Frame::Ping;
        let full = encode_frame(&frame);

        let mut codec = codec();

        // Feed only partial data.
        let mut partial = BytesMut::from(&full[..2]);
        assert!(codec.decode(&mut partial).unwrap().is_none());

        // Feed all but last byte.
        let mut almost = BytesMut::from(&full[..full.len() - 1]);
        assert!(codec.decode(&mut almost).unwrap().is_none());
    }

    #[test]
    fn complete_frame_after_buffering() {
        let frame = Frame::Pong;
        let full = encode_frame(&frame);

        let mut codec = codec();
        let mut buf = BytesMut::new();

        // Feed byte by byte.
        for &b in full.iter() {
            buf.put_u8(b);
            if buf.len() < full.len() {
                assert!(codec.decode(&mut buf).unwrap().is_none());
            }
        }

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded, frame);
    }

    #[test]
    fn oversized_frame_rejected_on_decode() {
        let frame = Frame::Publish {
            topic: "t".into(),
            payload: Bytes::from(vec![0u8; 100]),
            reply_to: None,
        };

        let mut buf = encode_frame(&frame);

        // Use a codec with tiny max.
        let mut small_codec = TopiqCodec::new(10);
        let result = small_codec.decode(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn oversized_frame_rejected_on_encode() {
        let frame = Frame::Publish {
            topic: "t".into(),
            payload: Bytes::from(vec![0u8; 100]),
            reply_to: None,
        };

        let mut small_codec = TopiqCodec::new(10);
        let mut buf = BytesMut::new();
        let result = small_codec.encode(frame, &mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn version_mismatch_rejected() {
        let frame = Frame::Ping;
        let mut buf = encode_frame(&frame);

        // Corrupt the version byte (byte index 4).
        buf[4] = 99;

        let mut codec = codec();
        let result = codec.decode(&mut buf);
        assert!(matches!(
            result,
            Err(TopiqError::UnsupportedVersion { version: 99 })
        ));
    }

    #[test]
    fn multiple_frames_in_buffer() {
        let f1 = Frame::Ping;
        let f2 = Frame::Pong;
        let f3 = Frame::Ok;

        let mut buf = BytesMut::new();
        let mut codec = codec();
        codec.encode(f1.clone(), &mut buf).unwrap();
        codec.encode(f2.clone(), &mut buf).unwrap();
        codec.encode(f3.clone(), &mut buf).unwrap();

        assert_eq!(codec.decode(&mut buf).unwrap().unwrap(), f1);
        assert_eq!(codec.decode(&mut buf).unwrap().unwrap(), f2);
        assert_eq!(codec.decode(&mut buf).unwrap().unwrap(), f3);
        assert!(codec.decode(&mut buf).unwrap().is_none());
    }

    #[test]
    fn all_frame_variants_through_codec() {
        let frames = vec![
            Frame::Publish {
                topic: "a.b".into(),
                payload: Bytes::from("data"),
                reply_to: Some("inbox".into()),
            },
            Frame::Subscribe {
                sid: 1,
                subject: "a.>".into(),
                queue_group: Some("q".into()),
            },
            Frame::Unsubscribe { sid: 1 },
            Frame::Message {
                topic: "a.b".into(),
                sid: 1,
                payload: Bytes::from("msg"),
                reply_to: None,
            },
            Frame::Ping,
            Frame::Pong,
            Frame::Ok,
            Frame::Err {
                message: "fail".into(),
            },
        ];

        let mut codec = codec();
        let mut buf = BytesMut::new();

        for f in &frames {
            codec.encode(f.clone(), &mut buf).unwrap();
        }

        for expected in &frames {
            let decoded = codec.decode(&mut buf).unwrap().unwrap();
            assert_eq!(&decoded, expected);
        }
    }
}
