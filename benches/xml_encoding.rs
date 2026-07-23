//! Benchmarks `xml_to_abx` (the XML-text-parsing half of encoding, needs
//! `quick-xml`) at a few sizes.
//!
//! Run with: `cargo bench --bench xml_encoding --features xml`

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

mod common;
use common::synthetic_xml;

const SIZES: [usize; 3] = [100, 1_000, 10_000];

fn bench_xml_to_abx(c: &mut Criterion) {
    let mut group = c.benchmark_group("xml_to_abx");
    for &n in &SIZES {
        let xml = synthetic_xml(n);
        group.throughput(Throughput::Bytes(xml.len() as u64));

        group.bench_with_input(BenchmarkId::new("xml_to_abx", n), &xml, |b, xml| {
            b.iter(|| black_box(abx::xml_to_abx(black_box(xml)).unwrap()));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_xml_to_abx);
criterion_main!(benches);
