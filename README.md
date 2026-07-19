# abx

A Rust parser for Android Binary XML (ABX) — the binary format used by
Android's `BinaryXmlSerializer`/`BinaryXmlPullParser` for on-device system
config files (`packages.xml`, `settings_*.xml`, `users/*.xml`, and similar).

Two parsers sharing one event model, optional `serde` deserialization
straight into your own structs, and a wire format verified against both
AOSP's own source and real independently-encoded files — not just this
crate's own test data.

```toml
[dependencies]
abx = "0.1"
# or, for serde support:
abx = { version = "0.1", features = ["serialize"] }
```

## Not AXML

If you're looking to parse `AndroidManifest.xml` or `res/**/*.xml` pulled
out of an APK, this is the wrong crate — that's **AXML**, a completely
different, chunk-based binary format (produced by `aapt`/`aapt2`, consumed
by the native `AssetManager`/`ResourceTypes.cpp`), unrelated to ABX beyond
sharing the "Android binary XML" nickname. `abx` only implements ABX: the
flat, `BinaryXmlSerializer`-produced format used by platform/system_server
config files. See `CLAUDE.md` for the full comparison if you're not sure
which one you have.

## Quick start

Walking raw events:

```rust
let data = std::fs::read("packages.abx")?;
let mut p = abx::AbxParser::new(&data)?;
while let Some(ev) = p.next_event()? {
    println!("{ev:?}");
}
```

Or streaming from any reader (files, sockets, pipes) without loading the
whole document into memory:

```rust
let mut p = abx::open_file("packages.abx")?;
let xml = p.to_xml()?;
```

With `serialize`, deserialize straight into your own types. For a
document whose root *is* the record you want:

```rust
#[derive(serde::Deserialize)]
struct Settings {
    enabled: bool,
    count: i32,
}

let settings: Settings = abx::from_file("settings.abx")?;
// or abx::from_slice(&bytes) / abx::from_reader(reader)
```

For a wrapper document containing many repeated records — e.g. AOSP's
`packages.xml` shape, one root with many `<pkg>` children:

```rust
#[derive(serde::Deserialize)]
struct Pkg {
    name: String,
    version: Option<i32>,
}

let mut p = abx::open_file("packages.abx")?;
for pkg in p.deserialize_iter::<Pkg>("pkg") {
    println!("{:?}", pkg?);
}
```

Struct fields map to attributes and child elements by name (`#[serde(rename
= "...")]` for names that aren't valid Rust identifiers), a field renamed to
`$text` captures an element's own direct text content, and a `Vec<T>` field
collects repeated same-named children. See `examples/` (`abx2xml`,
`serde_pkgs`, `serde_root`) for complete, runnable programs, and `src/de.rs`'s
module doc for the full mapping rules and how they relate to `quick-xml`'s.

## Design

Two independent parser implementations sharing the same `Event`/`Attribute`/
`AttributeValue` types and XML-rendering logic, so their output is always
identical:

- **`AbxParser`** — zero-allocation, operates on an in-memory `&[u8]`.
- **`AbxStreamParser`** — reads from any `impl std::io::Read` through an
  internal ring buffer that grows and refills on demand, for files, sockets,
  or pipes too large (or too live) to buffer up front.

Both expose the same convenience surface: `to_xml`/`write_xml`,
`find_attribute`/`find_all_attributes`, `attributes_of`/`all_attributes_of`,
`into_map`, and (with `serialize`) `deserialize_next`/`deserialize_all`,
plus `deserialize_iter` for true lazy streaming on `AbxStreamParser`.

## Verified against real data, not just this crate's own tests

The wire-format constants were checked directly against AOSP's own source
(`BinaryXmlSerializer.java`/`FastDataOutput.java`) and against real `.abx`
files produced by an independent `xml2abx` tool — not just this crate's own
synthetic test blobs. That distinction mattered in practice: an earlier
version of this crate had every data-type nibble constant off by one slot
relative to the real protocol, and the entire test suite passed anyway,
because the test builders and the parser shared the same wrong assumption.
It only surfaced by decoding a real independently-encoded file, which
`tests/aosp_fixture_tests.rs`/`tests/aosp_fixture_serde_tests.rs` now do on
every test run, against fixtures checked into `tests/fixtures/`. See
`CLAUDE.md`'s "FIXED" section for the full story if you're curious, or
touching those constants.

## Benchmarks

```bash
cargo bench --bench parsing
cargo bench --bench deserialize --features serialize
```

`criterion`-based, comparing `AbxParser` against `AbxStreamParser` (and, in
the second file, `deserialize_all`/`deserialize_iter` against raw event
walking) on synthetic data at a few sizes. `AbxParser` is consistently
faster, as expected from its zero-copy design over the streaming ring
buffer — but the serde layer's overhead over raw event walking turned out to
be negligible, which wasn't a given going in. HTML report with full
breakdowns and error bars at `target/criterion/report/index.html` after a
run. See `CLAUDE.md` for one recorded run's numbers (not committed
baselines — rerun locally rather than trusting a specific figure).

## Feature flags

- `serialize` — `serde::Deserialize` support: `from_slice`/`from_reader`/
  `from_file`, `deserialize_next`/`deserialize_all`/`deserialize_iter`.

## License

MIT.
