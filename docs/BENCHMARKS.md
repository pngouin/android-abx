# Benchmarks

```bash
cargo bench --bench parsing
cargo bench --bench deserialize --features serde
cargo bench --bench encoding
cargo bench --bench xml_encoding --features xml
```

The benchmarks use `criterion` on synthetic documents of 100, 1,000 and
10,000 elements, plus one AOSP fixture. The HTML report is written to
`target/criterion/report/index.html`.

One local run at 10,000 elements (about 600 KB). There is no committed
baseline, so rerun on your machine before comparing:

| Benchmark | Time |
|---|---|
| `parse_events/AbxParser` | 2.49 ms |
| `parse_events/AbxStreamParser` | 2.79 ms |
| `to_xml/AbxParser` | 2.26 ms |
| `to_xml/AbxStreamParser` | 2.31 ms |
| `deserialize_all/AbxParser` | 2.32 ms |
| `deserialize_iter` (streaming) | 2.49 ms |
| `events_to_abx/AbxWriter` | 341 µs |
| `xml_to_abx` | 2.56 ms |

`AbxStreamParser` is up to about 10% slower than `AbxParser`. Deserializing
costs no more than collecting raw events. Most of `xml_to_abx` is spent in
`quick-xml`'s tokenizer.
