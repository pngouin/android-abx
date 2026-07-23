#![cfg(feature = "serialize")]
//! Integration tests for `serde` deserialization of ABX elements into
//! structs, exercised through both `AbxParser` and `AbxStreamParser`.

// Fixture value 2.718281828 is an intentionally imprecise literal, not an
// attempt at std::f64::consts::E.
#![allow(clippy::approx_constant)]

use std::io::Cursor;

use abx::{AbxParser, AbxStreamParser, Attribute, AttributeValue};
use serde::Deserialize;

mod common;
use common::*;

#[derive(Debug, Deserialize, PartialEq, Clone)]
struct Pkg {
    name: String,
    version: i32,
}

#[test]
fn deserialize_flat_struct_from_attributes() {
    let data = document(&[
        start_tag("pkg"),
        attr_string("name", "com.example"),
        attr_int("version", 3),
        end_tag("pkg"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let pkg: Pkg = p.deserialize_next("pkg").unwrap().expect("element found");
    assert_eq!(
        pkg,
        Pkg {
            name: "com.example".into(),
            version: 3
        }
    );
}

#[test]
fn from_element_direct_no_parser() {
    let attrs = vec![
        Attribute {
            name: "name".into(),
            value: AttributeValue::String("x".into()),
        },
        Attribute {
            name: "version".into(),
            value: AttributeValue::Int(9),
        },
    ];
    let pkg: Pkg = abx::from_element(&attrs, None).unwrap();
    assert_eq!(
        pkg,
        Pkg {
            name: "x".into(),
            version: 9
        }
    );
}

#[derive(Debug, Deserialize, PartialEq)]
struct Renamed {
    #[serde(rename = "pkg-name")]
    name: String,
}

#[test]
fn deserialize_with_rename() {
    // Attribute name "pkg-name" isn't a valid Rust identifier, so the
    // struct field must use #[serde(rename)] to reach it.
    let data = document(&[
        start_tag("pkg"),
        attr_string("pkg-name", "com.example"),
        end_tag("pkg"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let v: Renamed = p.deserialize_next("pkg").unwrap().unwrap();
    assert_eq!(v.name, "com.example");
}

#[derive(Debug, Deserialize, PartialEq)]
struct Flagged {
    id: i32,
    flag: Option<bool>,
}

#[test]
fn deserialize_optional_missing_attribute() {
    let data = document(&[
        start_tag("e"),
        attr_int("id", 1),
        attr_bool("flag", true),
        end_tag("e"),
        start_tag("e"),
        attr_int("id", 2),
        end_tag("e"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let a: Flagged = p.deserialize_next("e").unwrap().unwrap();
    let b: Flagged = p.deserialize_next("e").unwrap().unwrap();
    assert_eq!(
        a,
        Flagged {
            id: 1,
            flag: Some(true)
        }
    );
    assert_eq!(b, Flagged { id: 2, flag: None });
}

#[derive(Debug, Deserialize, PartialEq)]
struct Nullable {
    value: Option<String>,
}

#[test]
fn deserialize_null_attribute_is_none() {
    // Distinct from a *missing* attribute: this one is present with an
    // explicit TYPE_NULL value.
    let data = document(&[start_tag("e"), attr_null("value"), end_tag("e")]);
    let mut p = AbxParser::new(&data).unwrap();
    let v: Nullable = p.deserialize_next("e").unwrap().unwrap();
    assert_eq!(v, Nullable { value: None });
}

#[derive(Debug, Deserialize, PartialEq)]
struct Numbers {
    long_val: i64,
    hex_flags: u32,
    big_hex: u64,
    ratio: f32,
    precise: f64,
}

#[test]
fn deserialize_numeric_coercion() {
    let data = document(&[
        start_tag("n"),
        attr_long("long_val", 9_876_543_210),
        attr_int_hex("hex_flags", 0xDEAD_BEEF),
        attr_long_hex("big_hex", 0xFFFF_FFFF_FFFF_FFFF),
        attr_float("ratio", 1.5),
        attr_double("precise", 2.718281828),
        end_tag("n"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let n: Numbers = p.deserialize_next("n").unwrap().unwrap();
    assert_eq!(
        n,
        Numbers {
            long_val: 9_876_543_210,
            hex_flags: 0xDEAD_BEEF,
            big_hex: 0xFFFF_FFFF_FFFF_FFFF,
            ratio: 1.5,
            precise: 2.718281828,
        }
    );
}

#[derive(Debug, Deserialize, PartialEq)]
struct Note {
    id: i32,
    #[serde(rename = "$text")]
    body: String,
}

#[test]
fn deserialize_text_content() {
    let data = document(&[
        start_tag("note"),
        attr_int("id", 7),
        text("hello world"),
        end_tag("note"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let n: Note = p.deserialize_next("note").unwrap().unwrap();
    assert_eq!(
        n,
        Note {
            id: 7,
            body: "hello world".into()
        }
    );
}

#[derive(Debug, Deserialize, PartialEq)]
struct Outer {
    attr: i32,
}

#[test]
fn deserialize_skips_nested_children() {
    // The nested <inner> element's own attributes must not leak into Outer,
    // and the parser must still land on the *second* top-level <outer>
    // afterwards rather than getting confused by the extra end tag.
    let data = document(&[
        start_tag("outer"),
        attr_int("attr", 1),
        start_tag("inner"),
        attr_string("x", "ignored"),
        end_tag("inner"),
        end_tag("outer"),
        start_tag("outer"),
        attr_int("attr", 2),
        end_tag("outer"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let a: Outer = p.deserialize_next("outer").unwrap().unwrap();
    let b: Outer = p.deserialize_next("outer").unwrap().unwrap();
    assert_eq!(a, Outer { attr: 1 });
    assert_eq!(b, Outer { attr: 2 });
}

#[derive(Debug, Deserialize, PartialEq)]
struct Blob {
    hex: Vec<u8>,
    b64: Vec<u8>,
}

#[test]
fn deserialize_bytes_field() {
    let data = document(&[
        start_tag("blob"),
        attr_bytes_hex("hex", &[0xDE, 0xAD, 0xBE, 0xEF]),
        attr_bytes_base64("b64", &[1, 2, 3, 4, 5]),
        end_tag("blob"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let b: Blob = p.deserialize_next("blob").unwrap().unwrap();
    assert_eq!(
        b,
        Blob {
            hex: vec![0xDE, 0xAD, 0xBE, 0xEF],
            b64: vec![1, 2, 3, 4, 5]
        }
    );
}

#[test]
fn deserialize_missing_element_returns_none() {
    let data = document(&[start_tag("a"), end_tag("a")]);
    let mut p = AbxParser::new(&data).unwrap();
    let result: abx::Result<Option<Pkg>> = p.deserialize_next("pkg");
    assert!(matches!(result, Ok(None)));
}

#[test]
fn deserialize_unknown_attributes_ignored_by_default() {
    let data = document(&[
        start_tag("pkg"),
        attr_string("name", "x"),
        attr_int("version", 1),
        attr_string("extra", "ignored"),
        end_tag("pkg"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let pkg: Pkg = p.deserialize_next("pkg").unwrap().unwrap();
    assert_eq!(
        pkg,
        Pkg {
            name: "x".into(),
            version: 1
        }
    );
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Strict {
    #[allow(dead_code)]
    name: String,
}

#[test]
fn deserialize_deny_unknown_fields_errors() {
    let data = document(&[
        start_tag("pkg"),
        attr_string("name", "x"),
        attr_string("extra", "y"),
        end_tag("pkg"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let result: abx::Result<Option<Strict>> = p.deserialize_next("pkg");
    assert!(
        result.is_err(),
        "expected deny_unknown_fields to reject 'extra'"
    );
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
struct Item {
    id: i32,
    name: String,
}

fn many_items(n: i32) -> Vec<Vec<u8>> {
    let mut parts = Vec::new();
    for i in 0..n {
        parts.push(start_tag("item"));
        parts.push(attr_int("id", i));
        parts.push(attr_string("name", &format!("name-{i}")));
        parts.push(end_tag("item"));
    }
    parts
}

#[test]
fn deserialize_all_streaming_matches_slice() {
    let data = document(&many_items(25));

    let mut slice_parser = AbxParser::new(&data).unwrap();
    let expected: Vec<Item> = slice_parser.deserialize_all("item").unwrap();
    assert_eq!(expected.len(), 25);

    let mut stream_parser = AbxStreamParser::new(Cursor::new(data.clone())).unwrap();
    let streamed: Vec<Item> = stream_parser.deserialize_all("item").unwrap();

    assert_eq!(expected, streamed);
}

#[test]
fn deserialize_iter_lazy_streaming() {
    let data = document(&many_items(5));
    let mut stream_parser = AbxStreamParser::new(Cursor::new(data)).unwrap();
    let items: Vec<Item> = stream_parser
        .deserialize_iter::<Item>("item")
        .collect::<abx::Result<Vec<Item>>>()
        .unwrap();

    assert_eq!(items.len(), 5);
    assert_eq!(
        items[0],
        Item {
            id: 0,
            name: "name-0".into()
        }
    );
    assert_eq!(
        items[4],
        Item {
            id: 4,
            name: "name-4".into()
        }
    );
}

// ---------------------------------------------------------------------------
// Comparison against quick-xml's serde conventions (docs.rs/quick-xml/de):
// unit-variant enums selected by matching the string value against the
// variant name (quick-xml: "Variant names become element or attribute
// names"), and the Some("") vs None distinction for optional attributes.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Status {
    Active,
    Inactive,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Entry {
    status: Status,
}

#[test]
fn deserialize_enum_field_from_string_attribute() {
    let data = document(&[
        start_tag("e"),
        attr_string("status", "active"),
        end_tag("e"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let e: Entry = p.deserialize_next("e").unwrap().unwrap();
    assert_eq!(
        e,
        Entry {
            status: Status::Active
        }
    );
}

#[derive(Debug, Deserialize, PartialEq)]
struct Mood {
    #[serde(rename = "$text")]
    value: Status,
}

#[test]
fn deserialize_enum_field_from_text_content() {
    let data = document(&[start_tag("note"), text("inactive"), end_tag("note")]);
    let mut p = AbxParser::new(&data).unwrap();
    let n: Mood = p.deserialize_next("note").unwrap().unwrap();
    assert_eq!(
        n,
        Mood {
            value: Status::Inactive
        }
    );
}

#[test]
fn deserialize_enum_unknown_variant_errors() {
    let data = document(&[start_tag("e"), attr_string("status", "bogus"), end_tag("e")]);
    let mut p = AbxParser::new(&data).unwrap();
    let result: abx::Result<Option<Entry>> = p.deserialize_next("e");
    assert!(result.is_err(), "unknown enum variant should be rejected");
}

#[derive(Debug, Deserialize, PartialEq)]
enum Level {
    #[serde(rename = "lo")]
    Low,
    #[serde(rename = "hi")]
    High,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Threshold {
    level: Level,
}

#[test]
fn deserialize_enum_variant_rename() {
    let data = document(&[start_tag("t"), attr_string("level", "hi"), end_tag("t")]);
    let mut p = AbxParser::new(&data).unwrap();
    let t: Threshold = p.deserialize_next("t").unwrap().unwrap();
    assert_eq!(t, Threshold { level: Level::High });
}

#[derive(Debug, Deserialize, PartialEq)]
struct Named {
    label: Option<String>,
}

#[test]
fn deserialize_empty_string_attribute_is_some_empty_not_none() {
    // A present-but-empty attribute is Some(""), distinct from an absent
    // one (None) — same distinction quick-xml documents for optional
    // fields ("Some("") represents an empty attribute").
    let data = document(&[start_tag("e"), attr_string("label", ""), end_tag("e")]);
    let mut p = AbxParser::new(&data).unwrap();
    let e: Named = p.deserialize_next("e").unwrap().unwrap();
    assert_eq!(
        e,
        Named {
            label: Some(String::new())
        }
    );

    let data2 = document(&[start_tag("e"), end_tag("e")]);
    let mut p2 = AbxParser::new(&data2).unwrap();
    let e2: Named = p2.deserialize_next("e").unwrap().unwrap();
    assert_eq!(e2, Named { label: None });
}

// ---------------------------------------------------------------------------
// Nested child elements as struct fields (quick-xml-style element mapping,
// without the @attr prefix — see src/de/mod.rs module docs for the
// precedence rule used instead: attribute wins over a same-named child).
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, PartialEq)]
struct Meta {
    key: String,
    value: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct PkgWithMeta {
    name: String,
    meta: Meta,
}

#[test]
fn deserialize_single_nested_child_struct() {
    let data = document(&[
        start_tag("pkg"),
        attr_string("name", "com.example"),
        start_tag("meta"),
        attr_string("key", "a"),
        attr_string("value", "b"),
        end_tag("meta"),
        end_tag("pkg"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let pkg: PkgWithMeta = p.deserialize_next("pkg").unwrap().unwrap();
    assert_eq!(
        pkg,
        PkgWithMeta {
            name: "com.example".into(),
            meta: Meta {
                key: "a".into(),
                value: "b".into()
            }
        }
    );
}

#[derive(Debug, Deserialize, PartialEq)]
struct Permission {
    name: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct PkgWithPerms {
    name: String,
    permission: Vec<Permission>,
}

#[test]
fn deserialize_repeated_children_as_vec() {
    let data = document(&[
        start_tag("pkg"),
        attr_string("name", "com.example"),
        start_tag("permission"),
        attr_string("name", "INTERNET"),
        end_tag("permission"),
        start_tag("permission"),
        attr_string("name", "CAMERA"),
        end_tag("permission"),
        end_tag("pkg"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let pkg: PkgWithPerms = p.deserialize_next("pkg").unwrap().unwrap();
    assert_eq!(
        pkg,
        PkgWithPerms {
            name: "com.example".into(),
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

#[derive(Debug, Deserialize, PartialEq)]
struct WithDescription {
    id: i32,
    // <description> is a leaf child (just text, no attributes/children of
    // its own) so it should deserialize straight into a String, the same
    // way an XML text-only element does in quick-xml.
    description: String,
}

#[test]
fn deserialize_leaf_child_as_scalar_string() {
    let data = document(&[
        start_tag("pkg"),
        attr_int("id", 1),
        start_tag("description"),
        text("A nice app"),
        end_tag("description"),
        end_tag("pkg"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let pkg: WithDescription = p.deserialize_next("pkg").unwrap().unwrap();
    assert_eq!(
        pkg,
        WithDescription {
            id: 1,
            description: "A nice app".into()
        }
    );
}

#[derive(Debug, Deserialize, PartialEq)]
struct WithCount {
    // Leaf child text parsed as a number, via serde's own str->number
    // fallback (no special-casing needed on our side).
    count: i32,
}

#[test]
fn deserialize_leaf_child_as_scalar_number() {
    let data = document(&[
        start_tag("e"),
        start_tag("count"),
        text("42"),
        end_tag("count"),
        end_tag("e"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let e: WithCount = p.deserialize_next("e").unwrap().unwrap();
    assert_eq!(e, WithCount { count: 42 });
}

#[derive(Debug, Deserialize, PartialEq)]
struct MaybeMeta {
    meta: Option<Meta>,
}

#[test]
fn deserialize_missing_child_is_none_for_option() {
    let data = document(&[start_tag("e"), end_tag("e")]);
    let mut p = AbxParser::new(&data).unwrap();
    let e: MaybeMeta = p.deserialize_next("e").unwrap().unwrap();
    assert_eq!(e, MaybeMeta { meta: None });
}

#[derive(Debug, Deserialize, PartialEq)]
struct IdHolder {
    id: i32,
}

#[test]
fn deserialize_attribute_wins_over_same_named_child() {
    // Both an "id" attribute and an "id" child element are present; the
    // attribute must win (and this must not panic/error as a "duplicate
    // field", which is the trap quick-xml's @attr prefix exists to avoid).
    let data = document(&[
        start_tag("e"),
        attr_int("id", 1),
        start_tag("id"),
        text("2"),
        end_tag("id"),
        end_tag("e"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let e: IdHolder = p.deserialize_next("e").unwrap().unwrap();
    assert_eq!(e, IdHolder { id: 1 });
}

#[derive(Debug, Deserialize, PartialEq)]
struct Grandchild {
    label: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Child {
    grandchild: Grandchild,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Root {
    child: Child,
}

#[test]
fn deserialize_deeply_nested_children() {
    let data = document(&[
        start_tag("root"),
        start_tag("child"),
        start_tag("grandchild"),
        attr_string("label", "deep"),
        end_tag("grandchild"),
        end_tag("child"),
        end_tag("root"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let root: Root = p.deserialize_next("root").unwrap().unwrap();
    assert_eq!(
        root,
        Root {
            child: Child {
                grandchild: Grandchild {
                    label: "deep".into()
                }
            }
        }
    );
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictNoChildren {
    #[allow(dead_code)]
    name: String,
}

#[test]
fn deserialize_deny_unknown_fields_rejects_unknown_child() {
    let data = document(&[
        start_tag("pkg"),
        attr_string("name", "x"),
        start_tag("unexpected"),
        end_tag("unexpected"),
        end_tag("pkg"),
    ]);
    let mut p = AbxParser::new(&data).unwrap();
    let result: abx::Result<Option<StrictNoChildren>> = p.deserialize_next("pkg");
    assert!(
        result.is_err(),
        "deny_unknown_fields should reject an unmapped child element too"
    );
}

// ---------------------------------------------------------------------------
// Top-level from_slice/from_reader/from_file: one-shot "whole document is
// one struct" deserialization, matching quick-xml's from_str/from_reader
// and serde_json's from_slice/from_reader — no manual parser construction,
// and (like both of those) the root element's tag name is not checked
// against the Rust type at all.
// ---------------------------------------------------------------------------

#[test]
fn from_slice_deserializes_root_element_regardless_of_tag_name() {
    let data = document(&[
        start_tag("anything"), // deliberately not "pkg" -- name is unchecked
        attr_string("name", "com.example"),
        attr_int("version", 3),
        end_tag("anything"),
    ]);
    let pkg: Pkg = abx::from_slice(&data).unwrap();
    assert_eq!(
        pkg,
        Pkg {
            name: "com.example".into(),
            version: 3
        }
    );
}

#[test]
fn from_reader_deserializes_root_element_streaming() {
    let data = document(&[
        start_tag("pkg"),
        attr_string("name", "com.example"),
        attr_int("version", 3),
        end_tag("pkg"),
    ]);
    let pkg: Pkg = abx::from_reader(Cursor::new(data)).unwrap();
    assert_eq!(
        pkg,
        Pkg {
            name: "com.example".into(),
            version: 3
        }
    );
}

#[test]
fn from_slice_and_from_reader_agree() {
    let data = document(&[
        start_tag("pkg"),
        attr_string("name", "com.example"),
        attr_int("version", 7),
        end_tag("pkg"),
    ]);
    let via_slice: Pkg = abx::from_slice(&data).unwrap();
    let via_reader: Pkg = abx::from_reader(Cursor::new(data)).unwrap();
    assert_eq!(via_slice, via_reader);
}

#[test]
fn from_slice_errors_when_document_has_no_root_element() {
    let data = document(&[]); // StartDocument + EndDocument only
    let result: abx::Result<Pkg> = abx::from_slice(&data);
    assert!(
        result.is_err(),
        "a document with no elements at all should not silently succeed"
    );
}

#[test]
fn from_slice_supports_nested_children_like_deserialize_next() {
    // from_slice/from_reader reuse the same ElementDeserializer as
    // deserialize_next, so nested-child mapping works identically -- not a
    // separate, more limited code path.
    let data = document(&[
        start_tag("pkg"),
        attr_string("name", "com.example"),
        start_tag("permission"),
        attr_string("name", "INTERNET"),
        end_tag("permission"),
        start_tag("permission"),
        attr_string("name", "CAMERA"),
        end_tag("permission"),
        end_tag("pkg"),
    ]);

    #[derive(Debug, Deserialize, PartialEq)]
    struct Permission {
        name: String,
    }
    #[derive(Debug, Deserialize, PartialEq)]
    struct PkgWithPerms {
        name: String,
        permission: Vec<Permission>,
    }

    let pkg: PkgWithPerms = abx::from_slice(&data).unwrap();
    assert_eq!(
        pkg,
        PkgWithPerms {
            name: "com.example".into(),
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
