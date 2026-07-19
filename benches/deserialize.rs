//! Benchmarks the `serialize` layer against the raw event-level API on
//! identical synthetic data — how much `deserialize_all`/`deserialize_iter`
//! cost over just walking `Event`s, and (again) `AbxParser` vs
//! `AbxStreamParser`, this time through the serde entry points.
//!
//! Run with: `cargo bench --bench deserialize --features serialize`

use std::io::Cursor;

use abx::{AbxParser, AbxStreamParser};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use serde::Deserialize;

mod common;
use common::synthetic_document;

const SIZES: [usize; 3] = [100, 1_000, 10_000];

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // fields only ever observed via black_box, never individually read
struct Pkg {
    name: String,
    version: i32,
    enabled: bool,
}

fn bench_deserialize_all(c: &mut Criterion) {
    let mut group = c.benchmark_group("deserialize_all");
    for &n in &SIZES {
        let data = synthetic_document(n);
        group.throughput(Throughput::Bytes(data.len() as u64));

        group.bench_with_input(BenchmarkId::new("AbxParser", n), &data, |b, data| {
            b.iter(|| {
                let mut p = AbxParser::new(black_box(data)).unwrap();
                black_box(p.deserialize_all::<Pkg>("pkg").unwrap())
            });
        });

        group.bench_with_input(BenchmarkId::new("AbxStreamParser", n), &data, |b, data| {
            b.iter(|| {
                let mut p = AbxStreamParser::new(Cursor::new(black_box(data.clone()))).unwrap();
                black_box(p.deserialize_all::<Pkg>("pkg").unwrap())
            });
        });
    }
    group.finish();
}

fn bench_streaming_deserialize_vs_raw_events(c: &mut Criterion) {
    let mut group = c.benchmark_group("streaming_deserialize_vs_events");
    for &n in &SIZES {
        let data = synthetic_document(n);
        group.throughput(Throughput::Bytes(data.len() as u64));

        group.bench_with_input(BenchmarkId::new("deserialize_iter", n), &data, |b, data| {
            b.iter(|| {
                let mut p = AbxStreamParser::new(Cursor::new(black_box(data.clone()))).unwrap();
                black_box(p.deserialize_iter::<Pkg>("pkg").collect::<abx::Result<Vec<Pkg>>>().unwrap())
            });
        });

        group.bench_with_input(BenchmarkId::new("raw_events", n), &data, |b, data| {
            b.iter(|| {
                let mut p = AbxStreamParser::new(Cursor::new(black_box(data.clone()))).unwrap();
                black_box(p.collect_events().unwrap())
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_deserialize_all, bench_streaming_deserialize_vs_raw_events);
criterion_main!(benches);
