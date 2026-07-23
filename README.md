# Android-ABX

[![CI](https://github.com/pngouin/android-abx/actions/workflows/ci.yml/badge.svg)](https://github.com/pngouin/android-abx/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/android-abx.svg)](https://crates.io/crates/android-abx)
[![docs.rs](https://docs.rs/android-abx/badge.svg)](https://docs.rs/android-abx)

A Rust parser and encoder for Android Binary XML (ABX) — the binary format
used by Android's `BinaryXmlSerializer`/`BinaryXmlPullParser` for on-device
system config files (`packages.xml`, `settings_*.xml`, `users/*.xml`, and
similar).

Two decode parsers sharing one event model, an encoder that goes the other
way (`Event`s or plain XML text back to ABX bytes), optional `serde`
deserialization, and a wire format checked against real AOSP source and
real encoded files in both directions — not just this crate's own tests.

```toml
[dependencies]
android-abx = "0.1"
# or, for serde support:
android-abx = { version = "0.1", features = ["serialize"] }
# or, to encode XML text into ABX bytes:
android-abx = { version = "0.1", features = ["xml"] }
```

The crates.io package is `android-abx`; the Rust import path stays `abx`
(set via `[lib] name` in `Cargo.toml`), so all the code below is
`use abx::...`, not `use android_abx::...`.

## Not AXML

If you're looking to parse `AndroidManifest.xml` or `res/**/*.xml` pulled
out of an APK, this is the wrong crate — that's **AXML**, a completely
different, chunk-based binary format, unrelated to ABX beyond sharing the
"Android binary XML" nickname:

| | **ABX** (this crate) | **AXML** |
|---|---|---|
| Used for | Platform config: `packages.xml`, `users/*.xml`, `settings_*.xml` | Compiled resources *inside an APK*: `AndroidManifest.xml`, `res/**/*.xml` |
| Produced by | `BinaryXmlSerializer`, at file-write time on a running system | `aapt`/`aapt2`, at APK *build* time |
| Wire shape | Flat token stream: magic `ABX\0`, then one byte per `XmlPullParser` event, each optionally followed by a typed value | Chunk-based (`ResChunk_header`): a string-pool chunk, a resource-map chunk, then a flat array of element nodes |
| Attribute values | A closed set of primitives — string, int, long, float, double, bool, bytes — no concept of a "resource" | Can be a literal *or* a reference into the resource table (`@string/foo`), resolved at load time |
| Typical tooling | `xml2abx`/`abx2xml` | `aapt2 dump xmltree`, `apktool`, `androguard` |

`abx`'s `nom`-based token-stream parser doesn't transfer to AXML — that
needs chunk/length-prefixed parsing and a resource-table-aware resolver.

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
`xml2abx`, `serde_pkgs`, `serde_root`) for complete, runnable programs, and
`src/de/mod.rs`'s module doc for the full mapping rules and how they relate
to `quick-xml`'s.

With `xml`, encode plain XML text straight into ABX bytes:

```rust
let bytes = abx::xml_to_abx(&xml_string)?;
std::fs::write("packages.abx", bytes)?;
```

Attribute values are always encoded as plain strings — matching real
AOSP's own `attribute()` method, which never infers a type from text
either; only the caller's choice of API (`attributeInt`, `attributeBoolean`,
...) decides that. To encode typed values, build the `Event` stream
directly and hand it to the lower-level encoder instead:

```rust
let events = vec![
    abx::Event::StartDocument,
    abx::Event::StartTag {
        name: "settings".into(),
        attributes: vec![abx::Attribute { name: "count".into(), value: abx::AttributeValue::Int(42) }],
    },
    abx::Event::EndTag { name: "settings".into() },
    abx::Event::EndDocument,
];
let bytes = abx::events_to_abx(&events)?;
```

## Design

Two parser implementations sharing the same `Event`/`Attribute`/
`AttributeValue` types and XML-rendering logic, so their output is always
identical:

- **`AbxParser`** — zero-allocation, operates on an in-memory `&[u8]`.
- **`AbxStreamParser`** — reads from any `impl std::io::Read` through an
  internal ring buffer, for files, sockets, or pipes too large (or too
  live) to buffer up front.

Both expose the same convenience surface: `to_xml`/`write_xml`,
`find_attribute`/`find_all_attributes`, `attributes_of`/`all_attributes_of`,
`into_map`, and (with `serialize`) `deserialize_next`/`deserialize_all`,
plus `deserialize_iter` for true lazy streaming on `AbxStreamParser`.

Encoding is one `AbxWriter<W: Write>`, not two — writing has no
ring-buffer/refill complexity to split, so `Vec<u8>` (in-memory) and a
file/socket (streaming) share the same type. Its interned-string pool
covers tag/attribute *names* only, never values, matching real AOSP's
generic `attribute()` (see "Verified against real AOSP" below).

Tag and attribute names (`Event::StartTag`/`EndTag`'s `name`,
`Attribute::name`) are `InternedStr` (`smol_str::SmolStr`), not `String`.
The wire format interns these — the same handful of names repeat across
every element in a document — and they're almost always short, so
`SmolStr` stores anything up to 23 bytes inline: cloning a repeated name is
a stack copy with no heap allocation at all. `InternedStr` behaves like a
read-only `String` day to day — `Deref<Target = str>`, compares equal to
`str`/`&str`/`String` directly (`name == "pkg"` just works) — it just can't
be mutated in place, since the buffer may be shared. It's also
`Send + Sync`, unlike an `Rc`-based alternative would be.

## Verified against real AOSP

The wire format is checked against real AOSP directly, not just this
crate's own tests — in both directions, and by actually compiling and
running AOSP's real source, not only reading it.

**Decode:** wire constants were checked against AOSP's own source
(`BinaryXmlSerializer.java`/`FastDataOutput.java`) and against real `.abx`
files, not just this crate's own synthetic test blobs. That distinction
mattered: an earlier version of this crate had every data-type nibble one
slot off from the real protocol, and the test suite passed anyway because
the test builders and the parser shared the same wrong assumption. It only
surfaced by decoding a real file — before the fix, a real `.abx` decoded to
just the bare XML declaration, body silently dropped, no error. Real
fixtures in `tests/fixtures/` — now generated directly by the real AOSP
`BinaryXmlSerializer` (see "Re-running the AOSP check" below), originally
by an independent `xml2abx` tool — checked on every test run via
`tests/aosp_fixture_tests.rs`/`tests/aosp_fixture_serde_tests.rs`, close
that gap.

**Encode:** `AbxWriter`'s wire encoding was grounded directly against
`BinaryXmlSerializer.java`'s source, not inferred by inverting the
decoder — catching two things a naive inversion would have missed
(`StartDocument`/`EndDocument` carry a type nibble the decoder never
checks; `StartTag`/`EndTag` use `TYPE_STRING_INTERNED`, not `TYPE_STRING`).

**Then taken further:** the real, unmodified AOSP `BinaryXmlSerializer`/
`BinaryXmlPullParser` were compiled and run directly. A document covering
every `AttributeValue` variant, every text-bearing `Event` variant, and
repeated-name interning round-tripped byte-for-byte identical against
`events_to_abx`. That run also found two real bugs:

- **In AOSP's own parser, not this crate**: `BinaryXmlPullParser` can't
  correctly read back a `TYPE_NULL` text token that `BinaryXmlSerializer`
  itself can produce (e.g. `text(null)`) — it reads a length-prefixed
  payload unconditionally without checking the type nibble first,
  desyncing and silently truncating the rest of the document. `AbxWriter`
  now always emits `TYPE_STRING` for text-bearing events, the only form
  verified safe against the real parser.
- **In this crate's own rendering**: `AttributeValue::as_str()` rendered
  hex-typed (`IntHex`/`LongHex`) values as raw two's-complement hex,
  special-casing only exactly `u32::MAX`/`u64::MAX` as `-1`. Real AOSP's
  `Integer.toString(v, 16)` treats the value as signed for *any* negative
  input (`0xCAFEBABE` renders as `"-35014542"`, not `"cafebabe"`) — fixed
  to match. Only the rendered text form was affected; wire bytes and
  decoded values were always correct.

Also confirmed: real AOSP's interning pool doesn't error past its
65,535-entry cap, it silently stops caching new names while everything
already interned keeps working — `AbxWriter` now matches that instead of
returning a hard error.

### Re-running the AOSP check

`tests/fixtures/aosp_verify/` re-runs all of this: compiles and runs the
real AOSP source and writes every `.abx` fixture in `tests/fixtures/`
(decoded by `tests/aosp_fixture_tests.rs`/`tests/aosp_fixture_serde_tests.rs`),
and re-checks the findings above, printing PASS/FAIL for each. The real
AOSP source and its one dependency (`xmlpull`) are vendored under `vendor/`
(each keeping its own original license — see `vendor/NOTICE.md`), so this
builds fully offline:

```bash
cd tests/fixtures/aosp_verify

# with podman or docker: the check runs as part of the build, so a failing
# build means a check failed
podman build -t abx-aosp-verify -f Containerfile .   # or: docker build ...
podman create --name abx-aosp-verify-tmp abx-aosp-verify
podman cp abx-aosp-verify-tmp:/work/aosp_verify.abx .
podman rm abx-aosp-verify-tmp

# or directly with a JDK (javac) on PATH, no container needed:
./build-and-run.sh aosp_verify.abx
```

`refresh-vendored-sources.sh` re-fetches `vendor/aosp/` from current AOSP
`main` (maintainer-run only, review the diff before committing) — useful
for catching upstream behavior changes.

## Known limitations

- **`$text` drops interleaved entities/whitespace.** The `#[serde(rename =
  "$text")]` convenience field only accumulates `Event::Text`, silently
  skipping `Event::EntityReference`/`Event::IgnorableWhitespace` content in
  between — so `Use &quot;quotes&quot; safely` comes out as
  `"Use quotes safely"`, entities dropped rather than decoded. The
  event-level API (`next_event`/`to_xml`) handles all three event kinds
  and round-trips exactly; only the `$text` shortcut has this gap.
- **Modified UTF-8 isn't supported.** AOSP's `writeUTF` encodes strings via
  Java's modified UTF-8 (NUL as `0xC0 0x80`; astral/emoji characters as
  CESU-8 surrogate pairs), not plain UTF-8. This crate decodes with
  `std::str::from_utf8`, which rejects or misdecodes those byte sequences.
  Irrelevant for typical config content (ASCII/BMP, no embedded NUL) — a
  real gap only for exotic input.

## Benchmarks

```bash
cargo bench --bench parsing
cargo bench --bench deserialize --features serialize
cargo bench --bench encoding
cargo bench --bench xml_encoding --features xml
```

`criterion`-based: `AbxParser` vs `AbxStreamParser` on decode,
`AbxWriter`/`events_to_abx` vs `xml_to_abx` on encode, at a few sizes. HTML
report at `target/criterion/report/index.html` after a run. One local run
at 10,000 elements (~600 KB; not committed baselines, rerun locally rather
than trusting these):

| Benchmark | Time |
|---|---|
| `parse_events/AbxParser` | 2.71ms |
| `parse_events/AbxStreamParser` | 3.04ms |
| `to_xml/AbxParser` | 2.24ms |
| `deserialize_all/AbxParser` | 2.33ms |
| `deserialize_iter` (streaming) | 2.74ms |
| `events_to_abx/AbxWriter` | 341µs |
| `xml_to_abx` | 2.56ms |

`AbxParser` beats `AbxStreamParser` by roughly 1.1–1.2x, the expected
irreducible cost of the ring buffer's bookkeeping over a zero-copy slice.
The serde layer's overhead over raw event walking is negligible — for the
streaming parser it's actually *faster* than collecting every raw `Event`
into a `Vec`, since `deserialize_iter` only retains the small deserialized
struct per element instead of every event's owned strings. `xml_to_abx` is
dominated by `quick-xml`'s tokenizer, not this crate's own encoding —
`events_to_abx` alone is ~7.5x faster than the full XML-text pipeline at
the same size.

Three optimization passes came out of these benchmarks:

- **Decode**: skipped a per-element `HashSet` allocation in the serde
  layer that was unused for flat elements (`deserialize_all` ~15–19%
  faster), wrote numeric/bool attribute values straight into the XML
  output buffer instead of via an intermediate allocation (`to_xml` ~11%
  faster), and switched the interned-name pool to `smol_str::SmolStr`
  (`parse_events` ~43% faster, `to_xml` ~40% faster versus the original
  `String`-based pool — the single biggest win, and the reason
  `InternedStr` exists).
- **Encode**: `InternedPool`'s `HashMap` was doing 5 hashed lookups per
  element against a map of just a handful of keys — real documents repeat
  a small, bounded vocabulary of names, so a linear scan over the tiny
  list beats hashing (`events_to_abx` ~48% faster). But a linear scan is
  O(n²) in the number of *unique* names — a test with 65,535 distinct
  names went from milliseconds to 49 seconds. Fixed with a hybrid: linear
  scan below 32 unique names, `HashMap` past that — ~41% faster than the
  original rather than the riskier ~48%.
- **Streaming decode**: `AbxStreamParser`'s ring buffer was compacting
  (sliding unconsumed bytes to the front) on *every* call that checked
  buffer capacity, not just the ones that actually needed to refill from
  the reader — so nearly every event paid for a memmove of the whole
  unconsumed buffer tail even when nothing needed to be read. Gating
  compaction behind an "is a refill actually about to happen?" check
  dropped `parse_events`/`to_xml`/`deserialize_all` on `AbxStreamParser`
  by 26–34% and shrank its gap against `AbxParser` from ~1.5–1.8x down to
  the ~1.1–1.2x above.

## Feature flags

- `serialize` — `serde::Deserialize` support: `from_slice`/`from_reader`/
  `from_file`, `deserialize_next`/`deserialize_all`/`deserialize_iter`.
- `xml` — `xml_to_abx`, encoding plain XML text into ABX bytes (pulls in
  `quick-xml`). The lower-level `AbxWriter`/`events_to_abx` need no extra
  dependency and are always available.

## Development

Formatting/lints and commit messages are checked via
[pre-commit](https://pre-commit.com/) (config in `.pre-commit-config.yaml`):
`cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and
a [Conventional Commits](https://www.conventionalcommits.org/) check on the
commit message. `.git/hooks/` isn't tracked by git, so after cloning:

```bash
pip install pre-commit
pre-commit install --hook-type pre-commit --hook-type commit-msg
```

`pre-commit run --all-files` runs everything on demand without committing.

## About this project

This crate has also served as a real-world test case for agentic AI coding
assistants — used to evaluate how such tools handle a non-trivial Rust
project over many sessions: implementing a binary format parser/encoder,
cross-checking it against real upstream (AOSP) source, and general ongoing
maintenance.

## License

MIT.
