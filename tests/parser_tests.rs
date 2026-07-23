//! Integration tests for the abx crate.
//!
//! We build synthetic ABX blobs by hand (matching the AOSP wire format)
//! so the tests are self-contained with no binary fixtures required.

// Fixture values 3.14/2.718_281_828 are intentionally imprecise literals,
// not attempts at std::f32::consts::PI/std::f64::consts::E.
#![allow(clippy::approx_constant)]

use abx::{AbxParser, AttributeValue, Event};

mod common;
use common::*;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_invalid_magic() {
    let data = b"\x00\x00\x00\x00rest";
    let err = AbxParser::new(data).unwrap_err();
    assert!(matches!(err, abx::AbxError::InvalidMagic { .. }));
}

#[test]
fn test_empty_document() {
    let mut data = with_magic(&[]);
    data.push(CMD_START_DOCUMENT);
    data.push(CMD_END_DOCUMENT);

    let mut p = AbxParser::new(&data).unwrap();
    assert!(matches!(
        p.next_event().unwrap(),
        Some(Event::StartDocument)
    ));
    assert!(matches!(p.next_event().unwrap(), Some(Event::EndDocument)));
    assert!(p.next_event().unwrap().is_none());
}

#[test]
fn test_simple_element_no_attrs() {
    // <root/>
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("root"));
    body.push(TYPE_STRING | CMD_END_TAG);
    body.extend(interned_ref(0)); // "root" already in pool at index 0
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let events = AbxParser::new(&data).unwrap().collect_events().unwrap();

    assert_eq!(events.len(), 4); // StartDocument, StartTag<root>, EndTag</root>, EndDocument
    let names: Vec<&str> = events
        .iter()
        .filter_map(|e| {
            if let Event::StartTag { name, .. } = e {
                Some(name.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(names, ["root"]);
}

#[test]
fn test_string_attribute() {
    // <pkg name="com.example"/>
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("pkg")); // pool[0]
    // attribute token = TYPE_STRING | CMD_ATTRIBUTE
    body.push(TYPE_STRING | CMD_ATTRIBUTE);
    body.extend(interned_new("name")); // pool[1]
    body.extend(utf("com.example")); // plain UTF value
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let mut p = AbxParser::new(&data).unwrap();
    let _ = p.next_event().unwrap(); // StartDocument
    let ev = p.next_event().unwrap().unwrap();

    if let Event::StartTag { name, attributes } = ev {
        assert_eq!(name, "pkg");
        assert_eq!(attributes.len(), 1);
        assert_eq!(attributes[0].name, "name");
        assert_eq!(
            attributes[0].value,
            AttributeValue::String("com.example".into())
        );
    } else {
        panic!("expected StartTag");
    }
}

#[test]
fn test_interned_string_attribute() {
    // Attribute value stored as an interned string.
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("item")); // pool[0] = "item"
    body.push(TYPE_STRING_INTERNED | CMD_ATTRIBUTE);
    body.extend(interned_new("key")); // pool[1] = "key"
    body.extend(interned_new("value_str")); // pool[2] = "value_str"  (the attribute value)
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let mut p = AbxParser::new(&data).unwrap();
    let _ = p.next_event().unwrap();
    let ev = p.next_event().unwrap().unwrap();

    if let Event::StartTag { attributes, .. } = ev {
        assert_eq!(
            attributes[0].value,
            AttributeValue::String("value_str".into())
        );
    } else {
        panic!();
    }
}

#[test]
fn test_int_attribute() {
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("e"));
    body.push(TYPE_INT | CMD_ATTRIBUTE);
    body.extend(interned_new("n"));
    body.extend(i32_be(-42));
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let mut p = AbxParser::new(&data).unwrap();
    let _ = p.next_event().unwrap();
    let ev = p.next_event().unwrap().unwrap();

    if let Event::StartTag { attributes, .. } = ev {
        assert_eq!(attributes[0].value, AttributeValue::Int(-42));
        assert_eq!(attributes[0].value.as_int(), Some(-42));
    } else {
        panic!();
    }
}

#[test]
fn test_int_hex_attribute() {
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("e"));
    body.push(TYPE_INT_HEX | CMD_ATTRIBUTE);
    body.extend(interned_new("flags"));
    body.extend(i32_be(0x00FF_ABCD_u32 as i32));
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let mut p = AbxParser::new(&data).unwrap();
    let _ = p.next_event().unwrap();
    let ev = p.next_event().unwrap().unwrap();

    if let Event::StartTag { attributes, .. } = ev {
        assert_eq!(attributes[0].value, AttributeValue::IntHex(0x00FF_ABCD));
        assert_eq!(attributes[0].value.as_str(), "ffabcd");
    } else {
        panic!();
    }
}

#[test]
fn test_int_hex_attribute_negative_renders_like_real_aosp() {
    // Real AOSP renders TYPE_INT_HEX via Integer.toString(v, 16), which
    // treats v as signed: negative -> "-" + hex(magnitude), e.g.
    // 0xCAFEBABE is "-35014542", not "cafebabe".
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("e"));
    body.push(TYPE_INT_HEX | CMD_ATTRIBUTE);
    body.extend(interned_new("v"));
    body.extend(i32_be(0xCAFEBABE_u32 as i32));
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let mut p = AbxParser::new(&data).unwrap();
    let _ = p.next_event().unwrap();
    let ev = p.next_event().unwrap().unwrap();

    if let Event::StartTag { attributes, .. } = ev {
        assert_eq!(attributes[0].value, AttributeValue::IntHex(0xCAFEBABE));
        assert_eq!(attributes[0].value.as_str(), "-35014542");
    } else {
        panic!();
    }
}

#[test]
fn test_long_hex_attribute_negative_renders_like_real_aosp() {
    // Same as test_int_hex_attribute_negative_renders_like_real_aosp, for
    // TYPE_LONG_HEX / Long.toString(v, 16). Confirmed against real AOSP:
    // Long.toString(0xDEADBEEFCAFEBABEL, 16) is "-2152411035014542".
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("e"));
    body.push(TYPE_LONG_HEX | CMD_ATTRIBUTE);
    body.extend(interned_new("v"));
    body.extend(i64_be(0xDEADBEEFCAFEBABE_u64 as i64));
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let mut p = AbxParser::new(&data).unwrap();
    let _ = p.next_event().unwrap();
    let ev = p.next_event().unwrap().unwrap();

    if let Event::StartTag { attributes, .. } = ev {
        assert_eq!(
            attributes[0].value,
            AttributeValue::LongHex(0xDEADBEEFCAFEBABE)
        );
        assert_eq!(attributes[0].value.as_str(), "-2152411035014542");
    } else {
        panic!();
    }
}

#[test]
fn test_long_attribute() {
    let v: i64 = 9_876_543_210;
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("e"));
    body.push(TYPE_LONG | CMD_ATTRIBUTE);
    body.extend(interned_new("ts"));
    body.extend(i64_be(v));
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let mut p = AbxParser::new(&data).unwrap();
    let _ = p.next_event().unwrap();
    let ev = p.next_event().unwrap().unwrap();

    if let Event::StartTag { attributes, .. } = ev {
        assert_eq!(attributes[0].value, AttributeValue::Long(v));
    } else {
        panic!();
    }
}

#[test]
fn test_float_attribute() {
    let v: f32 = 3.14;
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("e"));
    body.push(TYPE_FLOAT | CMD_ATTRIBUTE);
    body.extend(interned_new("f"));
    body.extend(f32_be(v));
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let mut p = AbxParser::new(&data).unwrap();
    let _ = p.next_event().unwrap();
    let ev = p.next_event().unwrap().unwrap();

    if let Event::StartTag { attributes, .. } = ev {
        assert_eq!(attributes[0].value, AttributeValue::Float(v));
    } else {
        panic!();
    }
}

#[test]
fn test_double_attribute() {
    let v: f64 = 2.718_281_828;
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("e"));
    body.push(TYPE_DOUBLE | CMD_ATTRIBUTE);
    body.extend(interned_new("d"));
    body.extend(f64_be(v));
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let mut p = AbxParser::new(&data).unwrap();
    let _ = p.next_event().unwrap();
    let ev = p.next_event().unwrap().unwrap();

    if let Event::StartTag { attributes, .. } = ev {
        assert_eq!(attributes[0].value, AttributeValue::Double(v));
    } else {
        panic!();
    }
}

#[test]
fn test_boolean_attributes() {
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("e"));
    body.push(TYPE_BOOLEAN_TRUE | CMD_ATTRIBUTE);
    body.extend(interned_new("a"));
    body.push(TYPE_BOOLEAN_FALSE | CMD_ATTRIBUTE);
    body.extend(interned_new("b"));
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let mut p = AbxParser::new(&data).unwrap();
    let _ = p.next_event().unwrap();
    let ev = p.next_event().unwrap().unwrap();

    if let Event::StartTag { attributes, .. } = ev {
        assert_eq!(attributes[0].value, AttributeValue::Boolean(true));
        assert_eq!(attributes[1].value, AttributeValue::Boolean(false));
    } else {
        panic!();
    }
}

#[test]
fn test_bytes_hex_attribute() {
    let bytes: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF];
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("e"));
    body.push(TYPE_BYTES_HEX | CMD_ATTRIBUTE);
    body.extend(interned_new("h"));
    body.extend(u16_be(bytes.len() as u16));
    body.extend_from_slice(bytes);
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let mut p = AbxParser::new(&data).unwrap();
    let _ = p.next_event().unwrap();
    let ev = p.next_event().unwrap().unwrap();

    if let Event::StartTag { attributes, .. } = ev {
        assert_eq!(
            attributes[0].value,
            AttributeValue::BytesHex(bytes.to_vec())
        );
        assert_eq!(attributes[0].value.as_str(), "deadbeef");
    } else {
        panic!();
    }
}

#[test]
fn test_bytes_base64_attribute() {
    let bytes: &[u8] = b"hello";
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("e"));
    body.push(TYPE_BYTES_BASE64 | CMD_ATTRIBUTE);
    body.extend(interned_new("b64"));
    body.extend(u16_be(bytes.len() as u16));
    body.extend_from_slice(bytes);
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let mut p = AbxParser::new(&data).unwrap();
    let _ = p.next_event().unwrap();
    let ev = p.next_event().unwrap().unwrap();

    if let Event::StartTag { attributes, .. } = ev {
        assert_eq!(
            attributes[0].value,
            AttributeValue::BytesBase64(b"hello".to_vec())
        );
        assert_eq!(attributes[0].value.as_str(), "aGVsbG8=");
    } else {
        panic!();
    }
}

#[test]
fn test_interned_string_reuse() {
    // Use "pkg" twice via pool index 0.
    let mut body = vec![CMD_START_DOCUMENT];
    // First START_TAG introduces "pkg" (pool[0])
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("pkg")); // pool[0] = "pkg"
    // Second START_TAG reuses pool[0]
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_ref(0)); // back-ref to "pkg"
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let events = AbxParser::new(&data).unwrap().collect_events().unwrap();

    let names: Vec<&str> = events
        .iter()
        .filter_map(|e| {
            if let Event::StartTag { name, .. } = e {
                Some(name.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(names, ["pkg", "pkg"]);
}

#[test]
fn test_text_event() {
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("root"));
    body.push(TYPE_STRING | CMD_TEXT);
    body.extend(utf("hello world"));
    body.push(TYPE_STRING | CMD_END_TAG);
    body.extend(interned_ref(0));
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let events = AbxParser::new(&data).unwrap().collect_events().unwrap();

    assert!(matches!(&events[2], Event::Text(t) if t == "hello world"));
}

#[test]
fn test_to_xml_roundtrip() {
    // <manifest package="com.example" versionCode="42"/>
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("manifest"));
    body.push(TYPE_STRING | CMD_ATTRIBUTE);
    body.extend(interned_new("package"));
    body.extend(utf("com.example"));
    body.push(TYPE_INT | CMD_ATTRIBUTE);
    body.extend(interned_new("versionCode"));
    body.extend(i32_be(42));
    body.push(TYPE_STRING | CMD_END_TAG);
    body.extend(interned_ref(0));
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let xml = AbxParser::new(&data).unwrap().to_xml().unwrap();

    assert!(xml.contains(r#"<manifest "#));
    assert!(xml.contains(r#"package="com.example""#));
    assert!(xml.contains(r#"versionCode="42""#));
    assert!(xml.contains("</manifest>"));
}

#[test]
fn test_find_attribute() {
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("component"));
    body.push(TYPE_STRING | CMD_ATTRIBUTE);
    body.extend(interned_new("package"));
    body.extend(utf("com.foo.bar"));
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let val = AbxParser::new(&data)
        .unwrap()
        .find_attribute("component", "package")
        .unwrap();
    assert_eq!(val.as_str(), "com.foo.bar");
}

#[test]
fn test_xml_entity_escaping() {
    // Attribute value containing XML special chars.
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("e"));
    body.push(TYPE_STRING | CMD_ATTRIBUTE);
    body.extend(interned_new("v"));
    body.extend(utf("<foo>&\"bar\"</foo>"));
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let xml = AbxParser::new(&data).unwrap().to_xml().unwrap();
    assert!(xml.contains("&lt;foo&gt;&amp;&quot;bar&quot;&lt;/foo&gt;"));
}

// ===========================================================================
// Stream parser tests
// ===========================================================================
//
// We reuse the same hand-built ABX blobs but feed them through
// `AbxStreamParser<Cursor<Vec<u8>>>` to exercise the Read-based path.

use abx::AbxStreamParser;
use std::io::{Cursor, Read};

fn stream(data: Vec<u8>) -> AbxStreamParser<Cursor<Vec<u8>>> {
    AbxStreamParser::new(Cursor::new(data)).expect("stream parser construction failed")
}

#[test]
fn stream_invalid_magic() {
    let err = AbxStreamParser::new(Cursor::new(b"\x00\x00\x00\x00rest".to_vec())).unwrap_err();
    assert!(matches!(err, abx::AbxError::InvalidMagic { .. }));
}

#[test]
fn stream_simple_element_no_attrs() {
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("root"));
    body.push(TYPE_STRING | CMD_END_TAG);
    body.extend(interned_ref(0));
    body.push(CMD_END_DOCUMENT);

    let events = stream(with_magic(&body)).collect_events().unwrap();
    // StartDocument, StartTag[name=root], EndTag[name=root], EndDocument
    assert!(
        matches!(&events[1], Event::StartTag { name, attributes } if name == "root" && attributes.is_empty())
    );
    assert!(matches!(&events[2], Event::EndTag { name } if name == "root"));
}

#[test]
fn stream_string_attribute() {
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("pkg"));
    body.push(TYPE_STRING | CMD_ATTRIBUTE);
    body.extend(interned_new("name"));
    body.extend(utf("com.example"));
    body.push(CMD_END_DOCUMENT);

    let mut p = stream(with_magic(&body));
    let _ = p.next_event().unwrap();
    let ev = p.next_event().unwrap().unwrap();

    if let Event::StartTag { name, attributes } = ev {
        assert_eq!(name, "pkg");
        assert_eq!(
            attributes[0].value,
            AttributeValue::String("com.example".into())
        );
    } else {
        panic!("expected StartTag");
    }
}

#[test]
fn stream_all_numeric_types() {
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("e"));
    // i32
    body.push(TYPE_INT | CMD_ATTRIBUTE);
    body.extend(interned_new("i"));
    body.extend(i32_be(-7));
    // u32 hex
    body.push(TYPE_INT_HEX | CMD_ATTRIBUTE);
    body.extend(interned_new("ih"));
    body.extend(i32_be(0xCAFE_u32 as i32));
    // i64
    body.push(TYPE_LONG | CMD_ATTRIBUTE);
    body.extend(interned_new("l"));
    body.extend(i64_be(1_234_567_890_123));
    // f32
    body.push(TYPE_FLOAT | CMD_ATTRIBUTE);
    body.extend(interned_new("f"));
    body.extend(f32_be(1.5_f32));
    // f64
    body.push(TYPE_DOUBLE | CMD_ATTRIBUTE);
    body.extend(interned_new("d"));
    body.extend(f64_be(2.5_f64));
    // bool true / false
    body.push(TYPE_BOOLEAN_TRUE | CMD_ATTRIBUTE);
    body.extend(interned_new("bt"));
    body.push(TYPE_BOOLEAN_FALSE | CMD_ATTRIBUTE);
    body.extend(interned_new("bf"));
    body.push(CMD_END_DOCUMENT);

    let mut p = stream(with_magic(&body));
    let _ = p.next_event().unwrap();
    let ev = p.next_event().unwrap().unwrap();

    if let Event::StartTag { attributes, .. } = ev {
        assert_eq!(attributes[0].value, AttributeValue::Int(-7));
        assert_eq!(attributes[1].value, AttributeValue::IntHex(0xCAFE));
        assert_eq!(attributes[2].value, AttributeValue::Long(1_234_567_890_123));
        assert_eq!(attributes[3].value, AttributeValue::Float(1.5));
        assert_eq!(attributes[4].value, AttributeValue::Double(2.5));
        assert_eq!(attributes[5].value, AttributeValue::Boolean(true));
        assert_eq!(attributes[6].value, AttributeValue::Boolean(false));
    } else {
        panic!();
    }
}

#[test]
fn stream_bytes_hex_and_base64() {
    let raw: &[u8] = &[0xAB, 0xCD];
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("e"));
    body.push(TYPE_BYTES_HEX | CMD_ATTRIBUTE);
    body.extend(interned_new("h"));
    body.extend(u16_be(raw.len() as u16));
    body.extend_from_slice(raw);
    body.push(TYPE_BYTES_BASE64 | CMD_ATTRIBUTE);
    body.extend(interned_new("b"));
    body.extend(u16_be(raw.len() as u16));
    body.extend_from_slice(raw);
    body.push(CMD_END_DOCUMENT);

    let mut p = stream(with_magic(&body));
    let _ = p.next_event().unwrap();
    let ev = p.next_event().unwrap().unwrap();

    if let Event::StartTag { attributes, .. } = ev {
        assert_eq!(attributes[0].value, AttributeValue::BytesHex(raw.to_vec()));
        assert_eq!(attributes[0].value.as_str(), "abcd");
        assert_eq!(
            attributes[1].value,
            AttributeValue::BytesBase64(raw.to_vec())
        );
    } else {
        panic!();
    }
}

#[test]
fn stream_interned_string_reuse() {
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("tag")); // pool[0]
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_ref(0)); // reuse "tag"
    body.push(CMD_END_DOCUMENT);

    let events = stream(with_magic(&body)).collect_events().unwrap();
    let names: Vec<&str> = events
        .iter()
        .filter_map(|e| {
            if let Event::StartTag { name, .. } = e {
                Some(name.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(names, ["tag", "tag"]);
}

#[test]
fn stream_to_xml() {
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("manifest"));
    body.push(TYPE_STRING | CMD_ATTRIBUTE);
    body.extend(interned_new("package"));
    body.extend(utf("com.stream"));
    body.push(TYPE_INT | CMD_ATTRIBUTE);
    body.extend(interned_new("versionCode"));
    body.extend(i32_be(7));
    body.push(TYPE_STRING | CMD_END_TAG);
    body.extend(interned_ref(0));
    body.push(CMD_END_DOCUMENT);

    let xml = stream(with_magic(&body)).to_xml().unwrap();
    assert!(xml.contains(r#"<manifest "#));
    assert!(xml.contains(r#"package="com.stream""#));
    assert!(xml.contains(r#"versionCode="7""#));
    assert!(xml.contains("</manifest>"));
}

#[test]
fn stream_write_xml() {
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("root"));
    body.push(TYPE_STRING | CMD_END_TAG);
    body.extend(interned_ref(0));
    body.push(CMD_END_DOCUMENT);

    let mut out: Vec<u8> = Vec::new();
    stream(with_magic(&body)).write_xml(&mut out).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("<root>"));
    assert!(s.contains("</root>"));
}

#[test]
fn stream_find_attribute() {
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("component"));
    body.push(TYPE_STRING | CMD_ATTRIBUTE);
    body.extend(interned_new("package"));
    body.extend(utf("com.stream.test"));
    body.push(CMD_END_DOCUMENT);

    let val = stream(with_magic(&body))
        .find_attribute("component", "package")
        .unwrap();
    assert_eq!(val.as_str(), "com.stream.test");
}

#[test]
fn stream_iterator() {
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("a"));
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("b"));
    body.push(CMD_END_DOCUMENT);

    let events: Vec<Event> = stream(with_magic(&body))
        .collect::<abx::Result<Vec<_>>>()
        .unwrap();

    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::StartTag { name, .. } if name == "a"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::StartTag { name, .. } if name == "b"))
    );
}

#[test]
fn stream_into_map() {
    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("item"));
    body.push(TYPE_STRING | CMD_ATTRIBUTE);
    body.extend(interned_new("key"));
    body.extend(utf("alpha"));
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_ref(0)); // "item" again
    body.push(TYPE_STRING | CMD_ATTRIBUTE);
    body.extend(interned_ref(1)); // "key" again
    body.extend(utf("beta"));
    body.push(CMD_END_DOCUMENT);

    let map = stream(with_magic(&body)).into_map().unwrap();
    let items = map.get("item").unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].get("key").unwrap(), "alpha");
    assert_eq!(items[1].get("key").unwrap(), "beta");
}

#[test]
fn stream_tiny_read_chunks() {
    // Force the parser to refill the buffer many times by wrapping the source
    // in a reader that returns 1 byte at a time.
    struct OneByteReader<'a>(&'a [u8]);
    impl<'a> Read for OneByteReader<'a> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.0.is_empty() || buf.is_empty() {
                return Ok(0);
            }
            buf[0] = self.0[0];
            self.0 = &self.0[1..];
            Ok(1)
        }
    }

    let mut body = vec![CMD_START_DOCUMENT];
    body.push(TYPE_STRING | CMD_START_TAG);
    body.extend(interned_new("root"));
    body.push(TYPE_STRING | CMD_ATTRIBUTE);
    body.extend(interned_new("x"));
    body.extend(utf("hello"));
    body.push(CMD_END_DOCUMENT);

    let data = with_magic(&body);
    let mut p = AbxStreamParser::new(OneByteReader(&data)).unwrap();
    let _ = p.next_event().unwrap(); // StartDocument
    let ev = p.next_event().unwrap().unwrap();

    if let Event::StartTag { name, attributes } = ev {
        assert_eq!(name, "root");
        assert_eq!(attributes[0].value, AttributeValue::String("hello".into()));
    } else {
        panic!("expected StartTag, got {:?}", ev);
    }
}
