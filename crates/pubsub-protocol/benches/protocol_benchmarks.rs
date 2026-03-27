use bytes::{Bytes, BytesMut};
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use pubsub_protocol::codec::PubSubCodec;
use pubsub_protocol::frame::Frame;
use tokio_util::codec::{Decoder, Encoder};

const MAX_FRAME: usize = 64 * 1024;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn publish_frame(payload_size: usize) -> Frame {
    Frame::Publish {
        topic: "sensors.temp.room1".into(),
        payload: Bytes::from(vec![0xABu8; payload_size]),
        reply_to: None,
    }
}

fn message_frame(payload_size: usize) -> Frame {
    Frame::Message {
        topic: "sensors.temp.room1".into(),
        sid: 42,
        payload: Bytes::from(vec![0xABu8; payload_size]),
        reply_to: None,
    }
}

fn encode_to_buf(frame: &Frame) -> BytesMut {
    let mut codec = PubSubCodec::new(MAX_FRAME);
    let mut buf = BytesMut::with_capacity(256);
    codec.encode(frame.clone(), &mut buf).unwrap();
    buf
}

// ---------------------------------------------------------------------------
// Frame msgpack encode (no codec framing)
// ---------------------------------------------------------------------------

fn bench_frame_serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_serialize");

    group.bench_function("ping", |b| {
        b.iter(|| black_box(rmp_serde::to_vec(&Frame::Ping).unwrap()));
    });

    for size in [0, 64, 1024, 16_384] {
        let frame = publish_frame(size);
        group.bench_with_input(
            BenchmarkId::new("publish_bytes", size),
            &frame,
            |b, frame| {
                b.iter(|| black_box(rmp_serde::to_vec(frame).unwrap()));
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Frame msgpack decode
// ---------------------------------------------------------------------------

fn bench_frame_deserialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_deserialize");

    let ping_bytes = rmp_serde::to_vec(&Frame::Ping).unwrap();
    group.bench_function("ping", |b| {
        b.iter(|| {
            let f: Frame = rmp_serde::from_slice(black_box(&ping_bytes)).unwrap();
            black_box(f);
        });
    });

    for size in [0, 64, 1024, 16_384] {
        let frame = publish_frame(size);
        let encoded = rmp_serde::to_vec(&frame).unwrap();
        group.bench_with_input(
            BenchmarkId::new("publish_bytes", size),
            &encoded,
            |b, data| {
                b.iter(|| {
                    let f: Frame = rmp_serde::from_slice(black_box(data)).unwrap();
                    black_box(f);
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// PubSubCodec encode (length-prefix + version + msgpack)
// ---------------------------------------------------------------------------

fn bench_codec_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("codec_encode");

    for size in [0, 64, 1024, 16_384] {
        let frame = publish_frame(size);
        group.bench_with_input(
            BenchmarkId::new("publish_bytes", size),
            &frame,
            |b, frame| {
                let mut codec = PubSubCodec::new(MAX_FRAME);
                b.iter(|| {
                    let mut buf = BytesMut::with_capacity(size + 64);
                    codec.encode(frame.clone(), &mut buf).unwrap();
                    black_box(buf);
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// PubSubCodec decode
// ---------------------------------------------------------------------------

fn bench_codec_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("codec_decode");

    for size in [0, 64, 1024, 16_384] {
        let frame = publish_frame(size);
        let wire = encode_to_buf(&frame);
        group.bench_with_input(
            BenchmarkId::new("publish_bytes", size),
            &wire,
            |b, wire| {
                let mut codec = PubSubCodec::new(MAX_FRAME);
                b.iter(|| {
                    let mut buf = wire.clone();
                    let decoded = codec.decode(&mut buf).unwrap().unwrap();
                    black_box(decoded);
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Full codec roundtrip
// ---------------------------------------------------------------------------

fn bench_codec_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("codec_roundtrip");

    for size in [0, 64, 1024, 16_384] {
        let frame = publish_frame(size);
        group.bench_with_input(
            BenchmarkId::new("publish_bytes", size),
            &frame,
            |b, frame| {
                let mut codec = PubSubCodec::new(MAX_FRAME);
                b.iter(|| {
                    let mut buf = BytesMut::with_capacity(size + 64);
                    codec.encode(frame.clone(), &mut buf).unwrap();
                    let decoded = codec.decode(&mut buf).unwrap().unwrap();
                    black_box(decoded);
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Message frame (broker -> client direction)
// ---------------------------------------------------------------------------

fn bench_message_frame_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_frame_roundtrip");

    for size in [64, 1024] {
        let frame = message_frame(size);
        group.bench_with_input(
            BenchmarkId::new("payload_bytes", size),
            &frame,
            |b, frame| {
                let mut codec = PubSubCodec::new(MAX_FRAME);
                b.iter(|| {
                    let mut buf = BytesMut::with_capacity(size + 64);
                    codec.encode(frame.clone(), &mut buf).unwrap();
                    let decoded = codec.decode(&mut buf).unwrap().unwrap();
                    black_box(decoded);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_frame_serialize,
    bench_frame_deserialize,
    bench_codec_encode,
    bench_codec_decode,
    bench_codec_roundtrip,
    bench_message_frame_roundtrip,
);
criterion_main!(benches);
