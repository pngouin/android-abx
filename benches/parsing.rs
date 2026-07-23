//! Compares `AbxParser` (in-memory) against `AbxStreamParser` (streaming,
//! fed via an in-memory `Cursor` to isolate ring-buffer/refill overhead
//! from actual I/O cost) on synthetic data at a few sizes. HTML report at
//! `target/criterion/report/index.html` after running.
//!
//! Run with: `cargo bench --bench parsing`

use std::io::Cursor;

use abx::{AbxParser, AbxStreamParser};
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

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

/// Same shape as `bench_parse_events`, but on the real AOSP-generated
/// `aosp_verify.abx` fixture (see `tests/aosp_fixture_tests.rs::aosp_verify_fixture`,
/// which verifies this file byte-for-byte) instead of only synthetic data.
fn bench_parse_real_fixture(c: &mut Criterion) {
    let data = include_bytes!("../tests/fixtures/aosp_verify.abx");
    let mut group = c.benchmark_group("parse_real_fixture");
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function(BenchmarkId::new("AbxParser", "aosp_verify"), |b| {
        b.iter(|| {
            let mut p = AbxParser::new(black_box(data)).unwrap();
            black_box(p.collect_events().unwrap())
        });
    });

    group.bench_function(BenchmarkId::new("AbxStreamParser", "aosp_verify"), |b| {
        b.iter(|| {
            let mut p = AbxStreamParser::new(Cursor::new(black_box(data.to_vec()))).unwrap();
            black_box(p.collect_events().unwrap())
        });
    });

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

criterion_group!(
    benches,
    bench_parse_events,
    bench_parse_real_fixture,
    bench_to_xml
);
criterion_main!(benches);
