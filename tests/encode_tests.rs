//! Integration tests for the ABX encoder (`AbxWriter`/`events_to_abx`),
//! asserting exact wire bytes against `tests/common/mod.rs`'s builders
//! (used here as the expected-bytes oracle, not as parser input).

// Fixture value 2.71828 is an intentionally imprecise literal, not an
// attempt at std::f64::consts::E.
#![allow(clippy::approx_constant)]

use abx::{AbxWriter, Attribute, AttributeValue, Event};

mod common;

/// `<tag attr=value/>`: builds expected bytes by appending the attribute
/// builder's output directly after `start_tag`'s (how the wire format lays
/// them out), then a back-referencing end tag.
fn assert_single_attr_roundtrip(value: AttributeValue, attr_bytes: Vec<u8>) {
    let mut w = AbxWriter::new(Vec::new()).unwrap();
    w.write_event(&Event::StartDocument).unwrap();
    w.write_event(&Event::StartTag {
        name: "tag".into(),
        attributes: vec![Attribute { name: "a".into(), value }],
    })
    .unwrap();
    w.write_event(&Event::EndTag { name: "tag".into() }).unwrap();
    w.write_event(&Event::EndDocument).unwrap();

    let mut start = common::start_tag("tag");
    start.extend(attr_bytes);
    let expected = common::document(&[start, interned_backref(common::CMD_END_TAG, 0)]);
    assert_eq!(w.into_inner(), expected);
}

#[test]
fn writer_writes_magic_header() {
    let w = AbxWriter::new(Vec::new()).unwrap();
    assert_eq!(w.into_inner(), abx::MAGIC.to_vec());
}

#[test]
fn writer_encodes_empty_document() {
    let mut w = AbxWriter::new(Vec::new()).unwrap();
    w.write_event(&Event::StartDocument).unwrap();
    w.write_event(&Event::EndDocument).unwrap();
    assert_eq!(w.into_inner(), common::document(&[]));
}

/// The pool is shared across every tag-name occurrence, so an end tag
/// naturally back-references its own start tag's name. `common::end_tag`
/// always assumes a fresh string, so build that token by hand.
fn interned_backref(cmd: u8, idx: u16) -> Vec<u8> {
    let mut v = vec![common::TYPE_STRING_INTERNED | cmd];
    v.extend(common::interned_ref(idx));
    v
}

#[test]
fn writer_encodes_start_and_end_tag() {
    let mut w = AbxWriter::new(Vec::new()).unwrap();
    w.write_event(&Event::StartDocument).unwrap();
    w.write_event(&Event::StartTag { name: "root".into(), attributes: vec![] }).unwrap();
    w.write_event(&Event::EndTag { name: "root".into() }).unwrap();
    w.write_event(&Event::EndDocument).unwrap();

    // "root" is interned fresh by StartTag; EndTag repeats the same name,
    // so it must be a back-reference, not a second fresh string.
    let expected = common::document(&[
        common::start_tag("root"),
        interned_backref(common::CMD_END_TAG, 0),
    ]);
    assert_eq!(w.into_inner(), expected);
}

#[test]
fn writer_interns_repeated_tag_name() {
    let mut w = AbxWriter::new(Vec::new()).unwrap();
    w.write_event(&Event::StartDocument).unwrap();
    w.write_event(&Event::StartTag { name: "pkg".into(), attributes: vec![] }).unwrap();
    w.write_event(&Event::EndTag { name: "pkg".into() }).unwrap();
    w.write_event(&Event::StartTag { name: "pkg".into(), attributes: vec![] }).unwrap();
    w.write_event(&Event::EndTag { name: "pkg".into() }).unwrap();
    w.write_event(&Event::EndDocument).unwrap();

    // Only the very first "pkg" occurrence is fresh; every occurrence after
    // that (end tag included) is a back-reference to the same pool index.
    let expected = common::document(&[
        common::start_tag("pkg"),
        interned_backref(common::CMD_END_TAG, 0),
        interned_backref(common::CMD_START_TAG, 0),
        interned_backref(common::CMD_END_TAG, 0),
    ]);
    assert_eq!(w.into_inner(), expected);
}

#[test]
fn writer_encodes_attr_null() {
    assert_single_attr_roundtrip(AttributeValue::Null, common::attr_null("a"));
}

#[test]
fn writer_encodes_attr_string() {
    assert_single_attr_roundtrip(
        AttributeValue::String("hello".into()),
        common::attr_string("a", "hello"),
    );
}

#[test]
fn writer_encodes_attr_bytes_hex() {
    assert_single_attr_roundtrip(
        AttributeValue::BytesHex(vec![0xDE, 0xAD, 0xBE, 0xEF]),
        common::attr_bytes_hex("a", &[0xDE, 0xAD, 0xBE, 0xEF]),
    );
}

#[test]
fn writer_encodes_attr_bytes_base64() {
    assert_single_attr_roundtrip(
        AttributeValue::BytesBase64(vec![1, 2, 3]),
        common::attr_bytes_base64("a", &[1, 2, 3]),
    );
}

#[test]
fn writer_encodes_attr_int() {
    assert_single_attr_roundtrip(AttributeValue::Int(-42), common::attr_int("a", -42));
}

#[test]
fn writer_encodes_attr_int_hex() {
    assert_single_attr_roundtrip(AttributeValue::IntHex(0xCAFEBABE), common::attr_int_hex("a", 0xCAFEBABE));
}

#[test]
fn writer_encodes_attr_long() {
    assert_single_attr_roundtrip(AttributeValue::Long(-123456789012), common::attr_long("a", -123456789012));
}

#[test]
fn writer_encodes_attr_long_hex() {
    assert_single_attr_roundtrip(
        AttributeValue::LongHex(0xDEADBEEFCAFEBABE),
        common::attr_long_hex("a", 0xDEADBEEFCAFEBABE),
    );
}

#[test]
fn writer_encodes_attr_float() {
    assert_single_attr_roundtrip(AttributeValue::Float(3.5), common::attr_float("a", 3.5));
}

#[test]
fn writer_encodes_attr_double() {
    assert_single_attr_roundtrip(AttributeValue::Double(2.71828), common::attr_double("a", 2.71828));
}

#[test]
fn writer_encodes_attr_boolean_true() {
    assert_single_attr_roundtrip(AttributeValue::Boolean(true), common::attr_bool("a", true));
}

#[test]
fn writer_encodes_attr_boolean_false() {
    assert_single_attr_roundtrip(AttributeValue::Boolean(false), common::attr_bool("a", false));
}

#[test]
fn writer_does_not_intern_repeated_attribute_string_value() {
    // AOSP's generic attribute(name, value) never interns the value, only
    // the name — a repeated String value is written fresh every time.
    let mut w = AbxWriter::new(Vec::new()).unwrap();
    w.write_event(&Event::StartDocument).unwrap();
    w.write_event(&Event::StartTag {
        name: "tag".into(),
        attributes: vec![
            Attribute { name: "a".into(), value: AttributeValue::String("dup".into()) },
            Attribute { name: "b".into(), value: AttributeValue::String("dup".into()) },
        ],
    })
    .unwrap();
    w.write_event(&Event::EndTag { name: "tag".into() }).unwrap();
    w.write_event(&Event::EndDocument).unwrap();

    let mut start = common::start_tag("tag");
    start.extend(common::attr_string("a", "dup"));
    start.extend(common::attr_string("b", "dup"));
    let expected = common::document(&[start, interned_backref(common::CMD_END_TAG, 0)]);
    assert_eq!(w.into_inner(), expected);
}

/// Both the empty-string (-> `TYPE_NULL`) and non-empty (-> `TYPE_STRING`)
/// cases for one of the seven text-bearing `Event` variants that all share
/// AOSP's `writeToken` shape.
fn assert_text_like_roundtrip(cmd: u8, make_event: impl Fn(String) -> Event) {
    for s in ["", "hello"] {
        let mut w = AbxWriter::new(Vec::new()).unwrap();
        w.write_event(&Event::StartDocument).unwrap();
        w.write_event(&make_event(s.to_string())).unwrap();
        w.write_event(&Event::EndDocument).unwrap();

        let expected = common::document(&[common::text_token(cmd, s)]);
        assert_eq!(w.into_inner(), expected, "mismatch for cmd {cmd:#x} with {s:?}");
    }
}

#[test]
fn writer_encodes_text() {
    assert_text_like_roundtrip(common::CMD_TEXT, Event::Text);
}

#[test]
fn writer_encodes_cdata_section() {
    assert_text_like_roundtrip(common::CMD_CDSECT, Event::CdataSection);
}

#[test]
fn writer_encodes_comment() {
    assert_text_like_roundtrip(common::CMD_COMMENT, Event::Comment);
}

#[test]
fn writer_encodes_processing_instruction() {
    assert_text_like_roundtrip(common::CMD_PROCESSING_INSTRUCTION, Event::ProcessingInstruction);
}

#[test]
fn writer_encodes_entity_reference() {
    assert_text_like_roundtrip(common::CMD_ENTITY_REF, Event::EntityReference);
}

#[test]
fn writer_encodes_ignorable_whitespace() {
    assert_text_like_roundtrip(common::CMD_IGNORABLE_WHITESPACE, Event::IgnorableWhitespace);
}

#[test]
fn writer_encodes_docdecl() {
    assert_text_like_roundtrip(common::CMD_DOCDECL, Event::DocDecl);
}

#[test]
fn events_to_abx_round_trips_through_decoder() {
    let events = vec![
        Event::StartDocument,
        Event::StartTag { name: "packages".into(), attributes: vec![] },
        Event::StartTag {
            name: "pkg".into(),
            attributes: vec![
                Attribute { name: "name".into(), value: AttributeValue::String("com.example.app".into()) },
                Attribute { name: "version".into(), value: AttributeValue::Int(3) },
            ],
        },
        Event::Text("hello".into()),
        Event::EndTag { name: "pkg".into() },
        Event::EndTag { name: "packages".into() },
        Event::EndDocument,
    ];

    let bytes = abx::events_to_abx(&events).unwrap();
    let decoded = abx::AbxParser::new(&bytes).unwrap().collect_events().unwrap();
    assert_eq!(decoded, events);
}

/// Matches AOSP's `FastDataOutput.writeUTF()`, which accepts a string right
/// up to `MAX_UNSIGNED_SHORT` (65,535) encoded bytes.
#[test]
fn writer_encodes_string_at_max_length_boundary() {
    let s = "a".repeat(65_535);
    let mut w = AbxWriter::new(Vec::new()).unwrap();
    w.write_event(&Event::StartDocument).unwrap();
    w.write_event(&Event::Text(s.clone())).unwrap();
    w.write_event(&Event::EndDocument).unwrap();

    let bytes = w.into_inner();
    let evs = abx::AbxParser::new(&bytes).unwrap().collect_events().unwrap();
    assert_eq!(evs[1], Event::Text(s));
}

/// Matches AOSP's `FastDataOutput.writeUTF()`, which throws
/// `UTFDataFormatException` once the encoded length exceeds
/// `MAX_UNSIGNED_SHORT` (65,535) — this crate rejects instead of silently
/// truncating the `u16` wire length prefix (which would corrupt everything
/// written after).
#[test]
fn writer_errors_on_oversized_text() {
    let s = "a".repeat(65_536);
    let mut w = AbxWriter::new(Vec::new()).unwrap();
    w.write_event(&Event::StartDocument).unwrap();
    let err = w.write_event(&Event::Text(s)).unwrap_err();
    assert!(matches!(err, abx::AbxError::ValueTooLong { len: 65_536, max: 65_535 }));
}

/// Same boundary as `writer_errors_on_oversized_text`, via an attribute's
/// `String` value rather than a text-bearing event.
#[test]
fn writer_errors_on_oversized_attr_string() {
    let s = "a".repeat(65_536);
    let mut w = AbxWriter::new(Vec::new()).unwrap();
    w.write_event(&Event::StartDocument).unwrap();
    let err = w
        .write_event(&Event::StartTag {
            name: "tag".into(),
            attributes: vec![Attribute { name: "a".into(), value: AttributeValue::String(s) }],
        })
        .unwrap_err();
    assert!(matches!(err, abx::AbxError::ValueTooLong { len: 65_536, max: 65_535 }));
}

/// Matches AOSP's `BinaryXmlSerializer.attributeBytesHex`/
/// `attributeBytesBase64`, which explicitly check `value.length >
/// MAX_UNSIGNED_SHORT` and throw before writing anything.
#[test]
fn writer_errors_on_oversized_bytes_blob() {
    let b = vec![0u8; 65_536];
    let mut w = AbxWriter::new(Vec::new()).unwrap();
    w.write_event(&Event::StartDocument).unwrap();
    let err = w
        .write_event(&Event::StartTag {
            name: "tag".into(),
            attributes: vec![Attribute { name: "a".into(), value: AttributeValue::BytesHex(b) }],
        })
        .unwrap_err();
    assert!(matches!(err, abx::AbxError::ValueTooLong { len: 65_536, max: 65_535 }));
}

#[test]
fn writer_gracefully_degrades_past_interned_pool_limit() {
    // Matches real AOSP: past its 65535-entry cap, a new name is written
    // fresh instead of cached, but nothing errors, and names interned
    // before the cap still back-reference correctly.
    let mut w = AbxWriter::new(Vec::new()).unwrap();
    w.write_event(&Event::StartDocument).unwrap();
    // Fill every valid index (0..=0xFFFE = 0xFFFF entries) -- 0xFFFF itself
    // is reserved as the INTERNED_NEW sentinel.
    for i in 0..0xFFFFu32 {
        w.write_event(&Event::StartTag { name: format!("n{i}").into(), attributes: vec![] }).unwrap();
    }
    // Past the cap: a brand-new name, then a repeat of a pre-cap name.
    w.write_event(&Event::StartTag { name: "over".into(), attributes: vec![] }).unwrap();
    w.write_event(&Event::StartTag { name: "n0".into(), attributes: vec![] }).unwrap();
    w.write_event(&Event::EndDocument).unwrap();

    let events = abx::AbxParser::new(&w.into_inner()).unwrap().collect_events().unwrap();
    assert_eq!(
        events[events.len() - 3],
        Event::StartTag { name: "over".into(), attributes: vec![] }
    );
    assert_eq!(events[events.len() - 2], Event::StartTag { name: "n0".into(), attributes: vec![] });
}
