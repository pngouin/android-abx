//! Compares `AbxParser` (in-memory, zero-copy) against `AbxStreamParser`
//! (streaming, ring-buffered — here fed via an in-memory `Cursor`, so this
//! isolates the ring-buffer/refill overhead itself rather than I/O cost) on
//! identical synthetic data, at a few sizes.
//!
//! This crate has one implementation with two parsing strategies rather
//! than a second-language port, so this is the closest equivalent here to
//! the Rust-vs-C++ comparison in
//! <https://github.com/rhythmcache/android-xml-converter>'s benchmarks —
//! same idea (which of two implementations is faster, at what data size),
//! adapted to what's actually comparable in this crate. That project times
//! two separate compiled binaries end-to-end with `hyperfine`; this instead
//! uses `criterion`, the standard in-process benchmarking tool for a Rust
//! *library* (statistically-sound sampling, throughput reporting, HTML
//! reports, regression detection across runs — see `target/criterion/report/index.html`
//! after running).
//!
//! Run with: `cargo bench --bench parsing`

use std::io::Cursor;

use abx::{AbxParser, AbxStreamParser};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

mod common;
use common::synthetic_document;

const SIZES: [usize; 3] = [100, 1_000, 10_000];

fn bench_parse_events(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_events");
    for &n in &SIZES {
        let data = synthetic_document(n);
        group.throughput(Throughput::Bytes(data.len() as u64));

        group.bench_with_input(BenchmarkId::new("AbxParser", n), &data, |b, data| {
            b.iter(|| {
                let mut p = AbxParser::new(black_box(data)).unwrap();
                black_box(p.collect_events().unwrap())
            });
        });

        group.bench_with_input(BenchmarkId::new("AbxStreamParser", n), &data, |b, data| {
            b.iter(|| {
                let mut p = AbxStreamParser::new(Cursor::new(black_box(data.clone()))).unwrap();
                black_box(p.collect_events().unwrap())
            });
        });
    }
    group.finish();
}

fn bench_to_xml(c: &mut Criterion) {
    let mut group = c.benchmark_group("to_xml");
    for &n in &SIZES {
        let data = synthetic_document(n);
        group.throughput(Throughput::Bytes(data.len() as u64));

        group.bench_with_input(BenchmarkId::new("AbxParser", n), &data, |b, data| {
            b.iter(|| black_box(abx::abx_to_xml(black_box(data)).unwrap()));
        });

        group.bench_with_input(BenchmarkId::new("AbxStreamParser", n), &data, |b, data| {
            b.iter(|| {
                let mut p = AbxStreamParser::new(Cursor::new(black_box(data.clone()))).unwrap();
                black_box(p.to_xml().unwrap())
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_parse_events, bench_to_xml);
criterion_main!(benches);
