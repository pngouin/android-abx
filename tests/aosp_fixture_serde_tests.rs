#![cfg(feature = "serialize")]
//! serde deserialization tests against the real, independently-encoded
//! `.abx` fixtures in `tests/fixtures/` (see `tests/aosp_fixture_tests.rs`
//! for why these matter more than synthetic test data: they're what caught
//! the `TYPE_*` nibble bug that every synthetic-blob test missed). These
//! specifically exercise the `serialize` feature — `deserialize_next`,
//! `deserialize_all`, `deserialize_iter`, and nested-child mapping — against
//! that same real data, rather than only the hand-built blobs in
//! `tests/serde_tests.rs`.

use std::io::Cursor;

use abx::{AbxParser, AbxStreamParser};
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct SimplePkg {
    name: String,
    // xml2abx stores "version"/"flags" as plain (interned) strings, not
    // TYPE_INT -- this exercises ValueDeserializer's str::parse fallback
    // for a String-typed attribute against real external data.
    version: i32,
    flags: i32,
}

#[test]
fn simple_pkg_fixture_deserializes_with_string_to_int_coercion() {
    let data = include_bytes!("fixtures/simple_pkg.abx");
    let mut p = AbxParser::new(data).unwrap();
    let pkg: SimplePkg = p.deserialize_next("pkg").unwrap().unwrap();
    assert_eq!(pkg, SimplePkg { name: "com.example.chat".into(), version: 3, flags: 1 });
}

#[test]
fn simple_pkg_fixture_via_from_slice_matches_deserialize_next() {
    // The one-shot entry point should need neither a parser instance nor
    // the "pkg" tag name spelled out -- same real fixture, same result.
    let data = include_bytes!("fixtures/simple_pkg.abx");
    let pkg: SimplePkg = abx::from_slice(data).unwrap();
    assert_eq!(pkg, SimplePkg { name: "com.example.chat".into(), version: 3, flags: 1 });
}

#[test]
fn simple_pkg_fixture_via_from_file() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_pkg.abx");
    let pkg: SimplePkg = abx::from_file(path).unwrap();
    assert_eq!(pkg, SimplePkg { name: "com.example.chat".into(), version: 3, flags: 1 });
}

#[derive(Debug, Deserialize, PartialEq)]
struct Permission {
    name: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct PkgWithChildren {
    name: String,
    // leaf child -> plain scalar field
    description: String,
    // repeated children -> Vec<T>
    permission: Vec<Permission>,
}

#[test]
fn nested_permissions_fixture_deserializes_with_children() {
    let data = include_bytes!("fixtures/nested_permissions.abx");
    let mut p = AbxParser::new(data).unwrap();
    let pkg: PkgWithChildren = p.deserialize_next("pkg").unwrap().unwrap();
    assert_eq!(
        pkg,
        PkgWithChildren {
            name: "com.example.chat".into(),
            description: "A chat app".into(),
            permission: vec![
                Permission { name: "INTERNET".into() },
                Permission { name: "CAMERA".into() },
            ],
        }
    );
}

#[test]
fn nested_permissions_fixture_deserializes_identically_via_streaming() {
    let data = include_bytes!("fixtures/nested_permissions.abx");
    let mut slice_p = AbxParser::new(data).unwrap();
    let expected: PkgWithChildren = slice_p.deserialize_next("pkg").unwrap().unwrap();

    let mut stream_p = AbxStreamParser::new(Cursor::new(data.to_vec())).unwrap();
    let streamed: PkgWithChildren = stream_p.deserialize_next("pkg").unwrap().unwrap();

    assert_eq!(expected, streamed);
}

#[derive(Debug, Deserialize, PartialEq)]
struct Settings {
    // xml2abx really does infer TYPE_BOOLEAN_TRUE/FALSE here.
    enabled: bool,
    hidden: bool,
    // ...but not numeric types -- these are plain strings on the wire,
    // coerced by str::parse the same way as SimplePkg's fields above.
    count: i32,
    ratio: f64,
}

#[test]
fn booleans_fixture_deserializes_bool_and_numeric_coercion() {
    let data = include_bytes!("fixtures/booleans.abx");
    let mut p = AbxParser::new(data).unwrap();
    let settings: Settings = p.deserialize_next("settings").unwrap().unwrap();
    assert_eq!(settings, Settings { enabled: true, hidden: false, count: 12345, ratio: 3.14 });
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
struct Item {
    id: i32,
    category: String,
    name: String,
}

fn expected_items() -> Vec<Item> {
    vec![
        Item { id: 1, category: "tools".into(), name: "Hammer".into() },
        Item { id: 2, category: "tools".into(), name: "Wrench".into() },
        Item { id: 3, category: "tools".into(), name: "Screwdriver".into() },
        Item { id: 4, category: "parts".into(), name: "Bolt".into() },
        Item { id: 5, category: "parts".into(), name: "Nut".into() },
        Item { id: 6, category: "parts".into(), name: "Washer".into() },
        Item { id: 7, category: "tools".into(), name: "Pliers".into() },
    ]
}

#[test]
fn repeated_strings_fixture_deserialize_all() {
    let data = include_bytes!("fixtures/repeated_strings.abx");
    let mut p = AbxParser::new(data).unwrap();
    let items: Vec<Item> = p.deserialize_all("item").unwrap();
    assert_eq!(items, expected_items());
}

#[test]
fn repeated_strings_fixture_deserialize_iter_streaming_matches_slice() {
    let data = include_bytes!("fixtures/repeated_strings.abx");
    let mut stream_p = AbxStreamParser::new(Cursor::new(data.to_vec())).unwrap();
    let streamed: Vec<Item> =
        stream_p.deserialize_iter::<Item>("item").collect::<abx::Result<Vec<Item>>>().unwrap();
    assert_eq!(streamed, expected_items());
}

#[derive(Debug, Deserialize, PartialEq)]
struct Note {
    title: String,
    #[serde(rename = "$text")]
    body: String,
}

#[test]
fn special_chars_fixture_title_attribute_is_raw_escaped_text() {
    let data = include_bytes!("fixtures/special_chars.abx");
    let mut p = AbxParser::new(data).unwrap();
    let note: Note = p.deserialize_next("note").unwrap().unwrap();

    // xml2abx does not XML-decode entities inside attribute *values*
    // (see tests/aosp_fixture_tests.rs::special_chars_fixture for the
    // verified raw bytes) -- serde sees exactly the same raw string as the
    // event-level API does, since both read from the same AttributeValue.
    assert_eq!(note.title, "Tom &amp; Jerry &lt;3&gt;");

    // Real, documented limitation of the "$text" convenience field, found
    // by this fixture: read_element_body() (src/de.rs) only accumulates
    // Event::Text into the text buffer, silently skipping
    // Event::EntityReference and Event::IgnorableWhitespace -- both of
    // which the source text (`Use &quot;quotes&quot; &amp;
    // &apos;apostrophes&apos; safely`) is full of, since xml2abx correctly
    // splits entities out of text content into their own EntityReference
    // events (unlike its attribute-value handling above). The event-level
    // API's to_xml() renders all three event kinds and round-trips
    // correctly (see tests/aosp_fixture_tests.rs::special_chars_fixture);
    // $text does not, so it drops both the entity characters and the
    // whitespace between text runs down to this:
    assert_eq!(note.body, "Use quotesapostrophes safely");
}
