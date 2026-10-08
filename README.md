# android-abx

[![CI](https://github.com/pngouin/android-abx/actions/workflows/ci.yml/badge.svg)](https://github.com/pngouin/android-abx/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/android-abx.svg)](https://crates.io/crates/android-abx)
[![docs.rs](https://docs.rs/android-abx/badge.svg)](https://docs.rs/android-abx)

A Rust parser and encoder for Android Binary XML (ABX), the format Android's
`BinaryXmlSerializer` uses for system files such as `packages.xml`,
`settings_*.xml` and `users/*.xml`.

```toml
[dependencies]
android-abx = "0.4"
```

This crate does not read `AndroidManifest.xml` or resource XML from an APK.
Those use AXML, an unrelated binary format.

## Examples

Read events from a byte slice:

```rust
let data = std::fs::read("packages.abx")?;
let mut parser = android_abx::AbxParser::new(&data)?;
while let Some(event) = parser.next_event()? {
    println!("{event:?}");
}
```

Stream a file to XML text without loading it into memory:

```rust
let xml = android_abx::open_file("packages.abx")?.to_xml()?;
```

Deserialize repeated elements into your own type (`serde` feature):

```rust
#[derive(serde::Deserialize)]
struct Pkg {
    name: String,
    version: Option<i32>,
}

let mut parser = android_abx::open_file("packages.abx")?;
for pkg in parser.deserialize_iter::<Pkg>("pkg") {
    println!("{:?}", pkg?);
}
```

Use `android_abx::from_file`/`from_slice`/`from_reader` when the root element
itself is the struct. Field mapping rules are in the
[crate docs](https://docs.rs/android-abx/latest/android_abx/#deserializing-with-serde).

Encode XML text to ABX (`xml` feature):

```rust
let bytes = android_abx::xml_to_abx(&xml)?;
```

`xml_to_abx` writes every attribute as a string, like AOSP's `attribute()`.
For typed values (`Int`, `Boolean`, ...), build `Event`s and call
`events_to_abx`.

More complete programs are in [`examples/`](examples/): `abx2xml`,
`xml2abx`, `serde_pkgs`, `serde_root`.

## Feature flags

- `serde`: `serde::Deserialize` support (`from_slice`, `from_reader`,
  `from_file`, `deserialize_next`, `deserialize_all`, `deserialize_iter`).
- `xml`: `xml_to_abx`, using `quick-xml`. `AbxWriter` and `events_to_abx`
  are always available.

## Compatibility

Test fixtures are written by AOSP's own `BinaryXmlSerializer`, compiled from
vendored source, and the encoder's output matches it byte for byte. See
[docs/AOSP_VERIFICATION.md](docs/AOSP_VERIFICATION.md).

Known limitations:

- A `$text` field only collects text events, so entity references inside
  the text are dropped: `Use &quot;quotes&quot;` becomes `Use quotes`. The
  event API and `to_xml` keep them.
- Strings are decoded as standard UTF-8, not Java's modified UTF-8. Strings
  containing NUL or characters outside the BMP (such as emoji) are rejected
  or misdecoded.

## Documentation

- [API docs](https://docs.rs/android-abx)
- [Design](docs/DESIGN.md): parsers, encoder, interned names
- [AOSP verification](docs/AOSP_VERIFICATION.md): what is checked and how to re-run it
- [Benchmarks](docs/BENCHMARKS.md)
- [Contributing](CONTRIBUTING.md)

## License

MIT
