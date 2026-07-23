//! Benchmarks `AbxWriter`/`events_to_abx` — the reverse of `parsing.rs` —
//! encoding a synthetic `Event` stream to ABX bytes, at a few sizes.
//!
//! Run with: `cargo bench --bench encoding`

use abx::{events_to_abx, AbxParser};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

mod common;
use common::synthetic_events;

const SIZES: [usize; 3] = [100, 1_000, 10_000];

fn bench_events_to_abx(c: &mut Criterion) {
    let mut group = c.benchmark_group("events_to_abx");
    for &n in &SIZES {
        let events = synthetic_events(n);
        let bytes_len = events_to_abx(&events).unwrap().len();
        group.throughput(Throughput::Bytes(bytes_len as u64));

        group.bench_with_input(BenchmarkId::new("AbxWriter", n), &events, |b, events| {
            b.iter(|| black_box(events_to_abx(black_box(events)).unwrap()));
        });
    }
    group.finish();
}

/// Encodes the event stream decoded from the real AOSP-generated
/// `aosp_verify.abx` fixture, instead of only synthetic data. The
/// `assert_eq!` below is a cheap, one-time correctness guard, not part of
/// the timed loop — it would have caught the `write_utf`/`write_bytes_blob`
/// length-prefix truncation bug this bench was added alongside.
fn bench_encode_real_fixture(c: &mut Criterion) {
    let data = include_bytes!("../tests/fixtures/aosp_verify.abx");
    let events = AbxParser::new(data).unwrap().collect_events().unwrap();
    assert_eq!(events_to_abx(&events).unwrap(), data);

    let mut group = c.benchmark_group("encode_real_fixture");
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function(BenchmarkId::new("AbxWriter", "aosp_verify"), |b| {
        b.iter(|| black_box(events_to_abx(black_box(&events)).unwrap()));
    });

    group.finish();
}

criterion_group!(benches, bench_events_to_abx, bench_encode_real_fixture);
criterion_main!(benches);
