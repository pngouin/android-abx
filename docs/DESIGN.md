# Design

## Wire format

A document starts with the 4-byte magic `ABX\0`. Each following token is one
byte: the low nibble is the event (`START_TAG`, `ATTRIBUTE`, `TEXT`, ...) and
the high nibble is the type of the value that follows, if any (string, int,
long, float, double, boolean, hex or base64 bytes, interned string). Tag and
attribute names are interned: a `u16` index into a per-document pool, where
`0xFFFF` means a new length-prefixed string follows and is added to the pool.

## ABX vs AXML

| | ABX (this crate) | AXML |
|---|---|---|
| Used for | Platform config: `packages.xml`, `users/*.xml`, `settings_*.xml` | Compiled resources in an APK: `AndroidManifest.xml`, `res/**/*.xml` |
| Written by | `BinaryXmlSerializer` on a running device | `aapt`/`aapt2` at build time |
| Layout | Flat token stream, one byte per pull-parser event | Chunks (`ResChunk_header`): string pool, resource map, element nodes |
| Attribute values | Fixed set of primitives | Literals or references into the resource table (`@string/foo`) |
| Tools | `abx2xml`, `xml2abx` | `aapt2 dump xmltree`, `apktool`, `androguard` |

Parsing AXML needs chunk parsing and a resource-table resolver, so it would
be a separate crate.

## Decoding

Two parser types produce the same `Event`, `Attribute` and `AttributeValue`
values:

- `AbxParser` reads an in-memory `&[u8]`.
- `AbxStreamParser` reads any `std::io::Read` through a buffer that refills
  in 4 KiB chunks and only grows to fit a single value larger than that.

Both call one [`winnow`](https://docs.rs/winnow) grammar
(`src/decode/grammar.rs`). `AbxParser` passes its slice, which winnow treats
as complete input. `AbxStreamParser` wraps its buffered bytes in winnow's
`Partial`, so running out of bytes returns `Incomplete` instead of an error;
the parser then refills and parses the event again from its start. As a
result, both parsers return the same events and the same errors for the same
bytes, and after an error both are left at the start of the failing event.

Both types offer the same helpers: `to_xml`, `write_xml`, `find_attribute`,
`find_all_attributes`, `attributes_of`, `all_attributes_of`, `into_map`,
and, with `serde`, `deserialize_next` and `deserialize_all`.
`AbxStreamParser` also has `deserialize_iter`.

## Encoding

`AbxWriter<W: Write>` encodes `Event`s. `events_to_abx` is a wrapper that
writes to a `Vec<u8>`, and `xml_to_abx` (feature `xml`) parses XML text with
`quick-xml` and feeds the writer.

The writer interns tag and attribute names, never attribute values, like
AOSP's `attribute()`. AOSP's `attributeInterned()` has no equivalent here.
Text events are always written with `TYPE_STRING`, even when empty, because
AOSP's `BinaryXmlPullParser` cannot read back a `TYPE_NULL` text token.

## Interned names

Tag and attribute names are `InternedStr`, an alias for
`smol_str::SmolStr`. Names up to 23 bytes are stored inline, so cloning a
repeated name does not allocate. It derefs to `str` and compares equal to
`&str` and `String` (`name == "pkg"`). Attribute values stay `String`
because they are mostly unique.
