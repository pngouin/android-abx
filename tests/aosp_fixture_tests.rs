//! Regression tests against real `.abx` files — every fixture in
//! `tests/fixtures/` is produced by the real, unmodified AOSP
//! `BinaryXmlSerializer` itself (compiled and run directly, not just read),
//! via `tests/fixtures/aosp_verify/`. See that directory's `Containerfile`/
//! `build-and-run.sh` to regenerate all of them at once, and `CLAUDE.md`'s
//! "VERIFIED" section for what running the real source turned up.
//!
//! These fixtures matter for the same reason real data always beats
//! synthetic blobs here: an earlier version of this crate had every
//! `TYPE_*` constant off by `0x10` relative to real AOSP, and every
//! synthetic-blob test passed anyway because `tests/common/mod.rs` used the
//! same wrong constants. Only real, independently-encoded data caught it —
//! see the "FIXED" section in `CLAUDE.md`. (At the time, these fixtures
//! were produced by a third-party `xml2abx` tool rather than real AOSP
//! source directly; they've since been regenerated the stronger way.)
//!
//! Source `.xml` files sit alongside each `.abx` fixture in
//! `tests/fixtures/` as human-readable reference — they're no longer the
//! literal generation input (the corresponding Java in
//! `tests/fixtures/aosp_verify/Harness.java` is), so may differ in
//! incidental ways like exact attribute typing (e.g. `version="3"` as text
//! vs. `attributeInt`) that don't affect the logical content.
//!
//! Most tests below also assert the exact wire bytes, built independently
//! via `tests/common/mod.rs`'s builders rather than by decoding `data` or
//! re-encoding through `AbxWriter` — so a symmetric bug shared between
//! decode and encode can't hide here. Safe to rely on those builders in
//! this file specifically because every fixture's exact content is fully
//! known up front (authored in `Harness.java`), unlike a black-box
//! independent tool's output.

// Fixture values like 3.14/2.71828 are intentionally imprecise literals,
// not attempts at std::f64::consts::PI/E — the exact byte/string
// representation of the literal itself is what's under test.
#![allow(clippy::approx_constant)]

use abx::{AbxParser, AbxStreamParser, Attribute, AttributeValue, Event};
use std::io::Cursor;

mod common;

fn events(bytes: &[u8]) -> Vec<Event> {
    AbxParser::new(bytes).unwrap().collect_events().unwrap()
}

fn attr<'a>(attributes: &'a [Attribute], name: &str) -> &'a AttributeValue {
    &attributes.iter().find(|a| a.name == name).unwrap().value
}

/// `type_nibble|cmd` + a back-reference to pool index `idx` — for
/// constructing expected bytes where `tests/common/mod.rs`'s `start_tag`/
/// `end_tag`/`attr_*` builders (which always assume a *fresh* string) can't
/// express a repeated name or interned value.
fn backref(type_nibble: u8, cmd: u8, idx: u16) -> Vec<u8> {
    let mut v = vec![type_nibble | cmd];
    v.extend(common::interned_ref(idx));
    v
}

/// A plain (non-interned-value) string attribute whose *name* is a
/// back-reference to pool index `name_idx` rather than a fresh string —
/// `tests/common/mod.rs::attr_string` always interns the name fresh, which
/// is only right for a name's first occurrence.
fn attr_string_named_backref(name_idx: u16, value: &str) -> Vec<u8> {
    let mut v = vec![common::TYPE_STRING | common::CMD_ATTRIBUTE];
    v.extend(common::interned_ref(name_idx));
    v.extend(common::utf(value));
    v
}

/// An `attributeInterned`-style attribute token: `TYPE_STRING_INTERNED|
/// CMD_ATTRIBUTE` + pre-built name bytes + pre-built value bytes (each
/// either `common::interned_new(s)` for a first occurrence or
/// `common::interned_ref(idx)` for a repeat — the caller tracks pool
/// state). No dedicated builder for this exists in `tests/common/mod.rs`
/// since nothing else needs it.
fn attr_interned_raw(name_bytes: Vec<u8>, value_bytes: Vec<u8>) -> Vec<u8> {
    let mut v = vec![common::TYPE_STRING_INTERNED | common::CMD_ATTRIBUTE];
    v.extend(name_bytes);
    v.extend(value_bytes);
    v
}

#[test]
fn simple_pkg_fixture() {
    let data = include_bytes!("fixtures/simple_pkg.abx");
    let evs = events(data);

    // version/flags are attributeInt in the real fixture (see Harness.java)
    // -- properly typed, unlike a plain string. as_str() renders the same
    // digits either way, so the XML text form doesn't change.
    assert_eq!(
        evs,
        vec![
            Event::StartDocument,
            Event::StartTag {
                name: "pkg".into(),
                attributes: vec![
                    Attribute { name: "name".into(), value: AttributeValue::String("com.example.chat".into()) },
                    Attribute { name: "version".into(), value: AttributeValue::Int(3) },
                    Attribute { name: "flags".into(), value: AttributeValue::Int(1) },
                ],
            },
            Event::EndTag { name: "pkg".into() },
            Event::EndDocument,
        ]
    );

    let xml = abx::abx_to_xml(data).unwrap();
    assert_eq!(
        xml,
        r#"<?xml version="1.0" encoding="UTF-8"?><pkg name="com.example.chat" version="3" flags="1"></pkg>"#
    );

    // The slice and streaming parsers must agree on real external data too,
    // not just on our own synthetic blobs.
    let mut sp = AbxStreamParser::new(Cursor::new(data.to_vec())).unwrap();
    assert_eq!(sp.to_xml().unwrap(), xml);

    let mut start = common::start_tag("pkg");
    start.extend(common::attr_string("name", "com.example.chat"));
    start.extend(common::attr_int("version", 3));
    start.extend(common::attr_int("flags", 1));
    let expected = common::document(&[start, backref(common::TYPE_STRING_INTERNED, common::CMD_END_TAG, 0)]);
    assert_eq!(data, &expected[..]);
}

#[test]
fn nested_permissions_fixture() {
    let data = include_bytes!("fixtures/nested_permissions.abx");
    let evs = events(data);

    let pkg_name = evs.iter().find_map(|e| match e {
        Event::StartTag { name, attributes } if name == "pkg" => {
            attr(attributes, "name").as_string()
        }
        _ => None,
    });
    assert_eq!(pkg_name, Some("com.example.chat"));

    let permission_names: Vec<&str> = evs
        .iter()
        .filter_map(|e| match e {
            Event::StartTag { name, attributes } if name == "permission" => attr(attributes, "name").as_string(),
            _ => None,
        })
        .collect();
    assert_eq!(permission_names, vec!["INTERNET", "CAMERA"]);

    let description_start = evs
        .iter()
        .position(|e| matches!(e, Event::StartTag { name, .. } if name == "description"))
        .unwrap();
    assert_eq!(evs[description_start + 1], Event::Text("A chat app".into()));

    // Pool order: pkg=0, name=1, description=2, permission=3.
    let mut pkg_start = common::start_tag("pkg");
    pkg_start.extend(common::attr_string("name", "com.example.chat"));

    // "name" is already interned (pool index 1, from pkg's own "name"
    // attribute) by the time the first <permission> is reached, so its
    // attribute name here is a back-reference too, not a fresh intern.
    let mut permission1 = common::start_tag("permission");
    permission1.extend(attr_string_named_backref(1, "INTERNET"));
    let mut permission2 = backref(common::TYPE_STRING_INTERNED, common::CMD_START_TAG, 3);
    permission2.extend(attr_string_named_backref(1, "CAMERA"));

    let expected = common::document(&[
        pkg_start,
        common::start_tag("description"),
        common::text("A chat app"),
        backref(common::TYPE_STRING_INTERNED, common::CMD_END_TAG, 2),
        permission1,
        backref(common::TYPE_STRING_INTERNED, common::CMD_END_TAG, 3),
        permission2,
        backref(common::TYPE_STRING_INTERNED, common::CMD_END_TAG, 3),
        backref(common::TYPE_STRING_INTERNED, common::CMD_END_TAG, 0),
    ]);
    assert_eq!(data, &expected[..]);
}

#[test]
fn booleans_fixture_typed_attributes() {
    // Every attribute here is properly typed in the real fixture (see
    // Harness.java): attributeBoolean for enabled/hidden, attributeInt for
    // count, attributeDouble for ratio. Real AOSP's own attribute() would
    // also be able to leave these as plain strings, same as any other
    // caller choice -- this fixture specifically exercises the typed path.
    let data = include_bytes!("fixtures/booleans.abx");
    let evs = events(data);

    let attributes = evs
        .iter()
        .find_map(|e| match e {
            Event::StartTag { name, attributes } if name == "settings" => Some(attributes),
            _ => None,
        })
        .unwrap();

    assert_eq!(*attr(attributes, "enabled"), AttributeValue::Boolean(true));
    assert_eq!(*attr(attributes, "hidden"), AttributeValue::Boolean(false));
    assert_eq!(*attr(attributes, "count"), AttributeValue::Int(12345));
    assert_eq!(*attr(attributes, "ratio"), AttributeValue::Double(3.14));

    let xml = abx::abx_to_xml(data).unwrap();
    assert!(xml.contains(r#"enabled="true" hidden="false" count="12345" ratio="3.14""#));

    let mut start = common::start_tag("settings");
    start.extend(common::attr_bool("enabled", true));
    start.extend(common::attr_bool("hidden", false));
    start.extend(common::attr_int("count", 12345));
    start.extend(common::attr_double("ratio", 3.14));
    let expected =
        common::document(&[start, backref(common::TYPE_STRING_INTERNED, common::CMD_END_TAG, 0)]);
    assert_eq!(data, &expected[..]);
}

#[test]
fn special_chars_fixture() {
    let data = include_bytes!("fixtures/special_chars.abx");
    let evs = events(data);

    let title = evs
        .iter()
        .find_map(|e| match e {
            Event::StartTag { name, attributes } if name == "note" => attr(attributes, "title").as_string(),
            _ => None,
        })
        .unwrap();
    // BinaryXmlSerializer.attribute() takes a plain Java String with no
    // XML-escaping concept at that API level, so the real fixture passes
    // the already-*decoded* value (see Harness.java) -- unlike the
    // previous xml2abx-sourced fixture, which left it raw/escaped (a quirk
    // of that independent tool, not of AOSP itself; still true of
    // `xml2abx` today, just no longer what's checked into this repo).
    assert_eq!(title, "Tom & Jerry <3>");

    // Text content is split into explicit text()/entityRef() calls in
    // Harness.java, the same way a real XML-aware caller built on this API
    // would emit entities as distinct tokens -- this crate reconstructs
    // the exact original escaped form on re-render either way.
    let xml = abx::abx_to_xml(data).unwrap();
    assert!(xml.contains(r#"title="Tom &amp; Jerry &lt;3&gt;""#));
    assert!(xml.contains(r#">Use &quot;quotes&quot; &amp; &apos;apostrophes&apos; safely<"#));

    let mut start = common::start_tag("note");
    start.extend(common::attr_string("title", "Tom & Jerry <3>"));
    let expected = common::document(&[
        start,
        common::text("Use "),
        common::text_token(common::CMD_ENTITY_REF, "quot"),
        common::text("quotes"),
        common::text_token(common::CMD_ENTITY_REF, "quot"),
        common::text(" "),
        common::text_token(common::CMD_ENTITY_REF, "amp"),
        common::text(" "),
        common::text_token(common::CMD_ENTITY_REF, "apos"),
        common::text("apostrophes"),
        common::text_token(common::CMD_ENTITY_REF, "apos"),
        common::text(" safely"),
        backref(common::TYPE_STRING_INTERNED, common::CMD_END_TAG, 0),
    ]);
    assert_eq!(data, &expected[..]);
}

#[test]
fn repeated_strings_fixture_interning() {
    let data = include_bytes!("fixtures/repeated_strings.abx");
    let evs = events(data);

    // "id" is attributeInt in the real fixture (not a plain string), so
    // compare its rendered text form via as_str() rather than as_string()
    // (which only extracts the String variant).
    let items: Vec<(String, &str, &str)> = evs
        .iter()
        .filter_map(|e| match e {
            Event::StartTag { name, attributes } if name == "item" => Some((
                attr(attributes, "id").as_str().into_owned(),
                attr(attributes, "category").as_string()?,
                attr(attributes, "name").as_string()?,
            )),
            _ => None,
        })
        .collect();

    assert_eq!(
        items,
        vec![
            ("1".to_string(), "tools", "Hammer"),
            ("2".to_string(), "tools", "Wrench"),
            ("3".to_string(), "tools", "Screwdriver"),
            ("4".to_string(), "parts", "Bolt"),
            ("5".to_string(), "parts", "Nut"),
            ("6".to_string(), "parts", "Washer"),
            ("7".to_string(), "tools", "Pliers"),
        ]
    );

    // Same real, interning-heavy data through the streaming parser must
    // decode identically to the slice parser.
    let mut sp = AbxStreamParser::new(Cursor::new(data.to_vec())).unwrap();
    assert_eq!(sp.collect_events().unwrap(), evs);

    // "id" is attributeInt in the real fixture; "category"/"name" use
    // attributeInterned (not plain attribute), so repeated values
    // back-reference on the wire -- real AOSP's plain attribute() never
    // auto-interns a value (see CLAUDE.md), only attributeInterned() does,
    // so this is what keeps this fixture's interning coverage genuine
    // rather than replicating a third-party tool's behavior by guesswork.
    // Pool order: catalog=0, item=1, id=2, category=3, tools=4, name=5,
    // Hammer=6, Wrench=7, Screwdriver=8, parts=9, Bolt=10, Nut=11,
    // Washer=12, Pliers=13.
    let item = |id: i32, id_new: bool, cat_new: Option<&str>, name_new: &str| {
        let mut v = if id == 1 { common::start_tag("item") } else { backref(common::TYPE_STRING_INTERNED, common::CMD_START_TAG, 1) };
        v.extend(if id_new {
            common::attr_int("id", id)
        } else {
            let mut a = backref(common::TYPE_INT, common::CMD_ATTRIBUTE, 2);
            a.extend(common::i32_be(id));
            a
        });
        v.extend(match cat_new {
            Some(cat) => attr_interned_raw(common::interned_new("category"), common::interned_new(cat)),
            None => attr_interned_raw(common::interned_ref(3), common::interned_ref(4)), // "tools"
        });
        v.extend(attr_interned_raw(
            if id == 1 { common::interned_new("name") } else { common::interned_ref(5) },
            common::interned_new(name_new),
        ));
        v.extend(backref(common::TYPE_STRING_INTERNED, common::CMD_END_TAG, 1));
        v
    };

    let parts_item = |id: i32, name_new: &str, cat_new: bool| {
        let mut v = backref(common::TYPE_STRING_INTERNED, common::CMD_START_TAG, 1);
        let mut a = backref(common::TYPE_INT, common::CMD_ATTRIBUTE, 2);
        a.extend(common::i32_be(id));
        v.extend(a);
        v.extend(if cat_new {
            attr_interned_raw(common::interned_ref(3), common::interned_new("parts"))
        } else {
            attr_interned_raw(common::interned_ref(3), common::interned_ref(9)) // "parts"
        });
        v.extend(attr_interned_raw(common::interned_ref(5), common::interned_new(name_new)));
        v.extend(backref(common::TYPE_STRING_INTERNED, common::CMD_END_TAG, 1));
        v
    };

    let expected = common::document(&[
        common::start_tag("catalog"),
        item(1, true, Some("tools"), "Hammer"),
        item(2, false, None, "Wrench"),
        item(3, false, None, "Screwdriver"),
        parts_item(4, "Bolt", true),
        parts_item(5, "Nut", false),
        parts_item(6, "Washer", false),
        item(7, false, None, "Pliers"),
        backref(common::TYPE_STRING_INTERNED, common::CMD_END_TAG, 0),
    ]);
    assert_eq!(data, &expected[..]);
}

/// Covers every `AttributeValue` variant abx's encoder can produce and
/// every text-bearing `Event` variant in one document.
#[test]
fn aosp_verify_fixture() {
    let data = include_bytes!("fixtures/aosp_verify.abx");
    let evs = events(data);

    assert_eq!(
        evs,
        vec![
            Event::StartDocument,
            Event::StartTag {
                name: "root".into(),
                attributes: vec![
                    Attribute { name: "str".into(), value: AttributeValue::String("hello".into()) },
                    Attribute {
                        name: "bh".into(),
                        value: AttributeValue::BytesHex(vec![0xDE, 0xAD, 0xBE, 0xEF]),
                    },
                    Attribute {
                        name: "bb".into(),
                        value: AttributeValue::BytesBase64(vec![1, 2, 3]),
                    },
                    Attribute { name: "i".into(), value: AttributeValue::Int(-42) },
                    Attribute { name: "ih".into(), value: AttributeValue::IntHex(0xCAFEBABE) },
                    Attribute { name: "l".into(), value: AttributeValue::Long(-123456789012) },
                    Attribute {
                        name: "lh".into(),
                        value: AttributeValue::LongHex(0xDEADBEEFCAFEBABE),
                    },
                    Attribute { name: "f".into(), value: AttributeValue::Float(3.5) },
                    Attribute { name: "d".into(), value: AttributeValue::Double(2.71828) },
                    Attribute { name: "bt".into(), value: AttributeValue::Boolean(true) },
                    Attribute { name: "bf".into(), value: AttributeValue::Boolean(false) },
                ],
            },
            Event::Text("hello world".into()),
            Event::CdataSection("raw <not-a-tag>".into()),
            Event::Comment("a comment".into()),
            Event::ProcessingInstruction("pi target data".into()),
            Event::EntityReference("amp".into()),
            Event::DocDecl("some-decl".into()),
            Event::IgnorableWhitespace("   ".into()),
            Event::Text("".into()),
            Event::StartTag { name: "root".into(), attributes: vec![] },
            Event::EndTag { name: "root".into() },
            Event::EndTag { name: "root".into() },
            Event::EndDocument,
        ]
    );

    // events_to_abx re-encoding this real-AOSP-decoded stream must byte-match
    // the real AOSP serializer's own output exactly.
    assert_eq!(abx::events_to_abx(&evs).unwrap(), data);

    let mut root_start = common::start_tag("root");
    root_start.extend(common::attr_string("str", "hello"));
    root_start.extend(common::attr_bytes_hex("bh", &[0xDE, 0xAD, 0xBE, 0xEF]));
    root_start.extend(common::attr_bytes_base64("bb", &[1, 2, 3]));
    root_start.extend(common::attr_int("i", -42));
    root_start.extend(common::attr_int_hex("ih", 0xCAFEBABE));
    root_start.extend(common::attr_long("l", -123456789012));
    root_start.extend(common::attr_long_hex("lh", 0xDEADBEEFCAFEBABE));
    root_start.extend(common::attr_float("f", 3.5));
    root_start.extend(common::attr_double("d", 2.71828));
    root_start.extend(common::attr_bool("bt", true));
    root_start.extend(common::attr_bool("bf", false));

    let expected_bytes = common::document(&[
        root_start,
        common::text_token(common::CMD_TEXT, "hello world"),
        common::text_token(common::CMD_CDSECT, "raw <not-a-tag>"),
        common::text_token(common::CMD_COMMENT, "a comment"),
        common::text_token(common::CMD_PROCESSING_INSTRUCTION, "pi target data"),
        common::text_token(common::CMD_ENTITY_REF, "amp"),
        common::text_token(common::CMD_DOCDECL, "some-decl"),
        common::text_token(common::CMD_IGNORABLE_WHITESPACE, "   "),
        common::text_token(common::CMD_TEXT, ""),
        backref(common::TYPE_STRING_INTERNED, common::CMD_START_TAG, 0), // 2nd "root"
        backref(common::TYPE_STRING_INTERNED, common::CMD_END_TAG, 0),
        backref(common::TYPE_STRING_INTERNED, common::CMD_END_TAG, 0),
    ]);
    assert_eq!(data, &expected_bytes[..]);
}
