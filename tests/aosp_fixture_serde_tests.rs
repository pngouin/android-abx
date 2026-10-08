#![cfg(feature = "serialize")]
#![allow(clippy::approx_constant)]

use std::io::Cursor;

use android_abx::{AbxParser, AbxStreamParser};
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
    assert_eq!(
        pkg,
        SimplePkg {
            name: "com.example.chat".into(),
            version: 3,
            flags: 1
        }
    );
}

#[test]
fn simple_pkg_fixture_via_from_slice_matches_deserialize_next() {
    let data = include_bytes!("fixtures/simple_pkg.abx");
    let pkg: SimplePkg = android_abx::from_slice(data).unwrap();
    assert_eq!(
        pkg,
        SimplePkg {
            name: "com.example.chat".into(),
            version: 3,
            flags: 1
        }
    );
}

#[test]
fn simple_pkg_fixture_via_from_file() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_pkg.abx");
    let pkg: SimplePkg = android_abx::from_file(path).unwrap();
    assert_eq!(
        pkg,
        SimplePkg {
            name: "com.example.chat".into(),
            version: 3,
            flags: 1
        }
    );
}

#[derive(Debug, Deserialize, PartialEq)]
struct Permission {
    name: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct PkgWithChildren {
    name: String,
    description: String,
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
                Permission {
                    name: "INTERNET".into()
                },
                Permission {
                    name: "CAMERA".into()
                },
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
    assert_eq!(
        settings,
        Settings {
            enabled: true,
            hidden: false,
            count: 12345,
            ratio: 3.14
        }
    );
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
struct Item {
    id: i32,
    category: String,
    name: String,
}

fn expected_items() -> Vec<Item> {
    vec![
        Item {
            id: 1,
            category: "tools".into(),
            name: "Hammer".into(),
        },
        Item {
            id: 2,
            category: "tools".into(),
            name: "Wrench".into(),
        },
        Item {
            id: 3,
            category: "tools".into(),
            name: "Screwdriver".into(),
        },
        Item {
            id: 4,
            category: "parts".into(),
            name: "Bolt".into(),
        },
        Item {
            id: 5,
            category: "parts".into(),
            name: "Nut".into(),
        },
        Item {
            id: 6,
            category: "parts".into(),
            name: "Washer".into(),
        },
        Item {
            id: 7,
            category: "tools".into(),
            name: "Pliers".into(),
        },
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
    let streamed: Vec<Item> = stream_p
        .deserialize_iter::<Item>("item")
        .collect::<android_abx::Result<Vec<Item>>>()
        .unwrap();
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

    assert_eq!(note.title, "Tom & Jerry <3>");

    // `$text` keeps only Event::Text; entity references are dropped.
    assert_eq!(note.body, "Use quotes  apostrophes safely");
}
