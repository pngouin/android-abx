#![cfg(feature = "serialize")]
//! serde deserialization tests against the same real, AOSP-encoded `.abx`
//! fixtures as `tests/aosp_fixture_tests.rs`, exercising the `serialize`
//! feature — `deserialize_next`, `deserialize_all`, `deserialize_iter`, and
//! nested-child mapping — against real data rather than only the hand-built
//! blobs in `tests/serde_tests.rs`.

use std::io::Cursor;

use abx::{AbxParser, AbxStreamParser};
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct SimplePkg {
    name: String,
    version: i32,
    flags: i32,
}

#[test]
fn simple_pkg_fixture_deserializes_typed_ints() {
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
    enabled: bool,
    hidden: bool,
    count: i32,
    ratio: f64,
}

#[test]
fn booleans_fixture_deserializes_typed_attributes() {
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
fn special_chars_fixture_text_drops_entity_references() {
    let data = include_bytes!("fixtures/special_chars.abx");
    let mut p = AbxParser::new(data).unwrap();
    let note: Note = p.deserialize_next("note").unwrap().unwrap();

    // title is the real AttributeValue::String, already decoded (see
    // special_chars_fixture in tests/aosp_fixture_tests.rs) — serde sees
    // the same value as the event-level API, since both read from the
    // same AttributeValue.
    assert_eq!(note.title, "Tom & Jerry <3>");

    // Limitation of the "$text" convenience field: read_element_body()
    // (src/de/traversal.rs) only accumulates Event::Text, silently
    // skipping Event::EntityReference and Event::IgnorableWhitespace —
    // both of which this source text is full of (five EntityReference
    // events split the text into six Text pieces). to_xml() renders all
    // three event kinds correctly (see special_chars_fixture); $text does
    // not, so it drops down to this (note the doubled space, from two
    // adjacent Text(" ") pieces either side of the &amp; entity):
    assert_eq!(note.body, "Use quotes  apostrophes safely");
}
