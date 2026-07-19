//! Benchmarks `AbxWriter`/`events_to_abx` — the reverse of `parsing.rs` —
//! encoding a synthetic `Event` stream to ABX bytes, at a few sizes.
//!
//! Run with: `cargo bench --bench encoding`

use abx::events_to_abx;
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

criterion_group!(benches, bench_events_to_abx);
criterion_main!(benches);
