//! Regression tests against real `.abx` files produced by an independent
//! encoder — the user's local `xml2abx` binary — rather than by this repo's
//! own `tests/common/mod.rs` builder helpers.
//!
//! This distinction matters: an earlier version of this crate had every
//! `TYPE_*` data-type nibble constant off by `0x10` relative to real AOSP
//! (`BinaryXmlSerializer.java`), and every one of the ~50 other tests in
//! this suite passed the whole time, because `tests/common/mod.rs` defined
//! the *same* wrong constants to build its synthetic test blobs — internally
//! self-consistent, but both wrong relative to the real wire format. These
//! fixtures, decoded by a real independent tool, are what actually caught
//! it. See "KNOWN BUG" (now "Fixed") in `CLAUDE.md` for the full story.
//!
//! Source `.xml` files sit alongside each `.abx` fixture in
//! `tests/fixtures/` for provenance/readability. Regenerate with:
//! `xml2abx tests/fixtures/<name>.xml tests/fixtures/<name>.abx`

use abx::{AbxParser, AbxStreamParser, Attribute, AttributeValue, Event};
use std::io::Cursor;

fn events(bytes: &[u8]) -> Vec<Event> {
    AbxParser::new(bytes).unwrap().collect_events().unwrap()
}

fn attr<'a>(attributes: &'a [Attribute], name: &str) -> &'a AttributeValue {
    &attributes.iter().find(|a| a.name == name).unwrap().value
}

#[test]
fn simple_pkg_fixture() {
    let data = include_bytes!("fixtures/simple_pkg.abx");

    let xml = abx::abx_to_xml(data).unwrap();
    assert_eq!(
        xml,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<pkg name=\"com.example.chat\" version=\"3\" flags=\"1\"></pkg>\n"
    );

    // The slice and streaming parsers must agree on real external data too,
    // not just on our own synthetic blobs.
    let mut sp = AbxStreamParser::new(Cursor::new(data.to_vec())).unwrap();
    assert_eq!(sp.to_xml().unwrap(), xml);
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
}

#[test]
fn booleans_fixture_type_inference() {
    // xml2abx infers TYPE_BOOLEAN_TRUE/FALSE for "true"/"false" attribute
    // values, but does not infer numeric types (verified via hex dump —
    // "count"/"ratio" are stored as plain interned strings on the real
    // wire). Both are exercised here.
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
    assert_eq!(*attr(attributes, "count"), AttributeValue::String("12345".into()));
    assert_eq!(*attr(attributes, "ratio"), AttributeValue::String("3.14".into()));
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
    // xml2abx does NOT decode XML entities inside attribute values (unlike
    // text content, decoded correctly below via EntityReference events) —
    // a real, verified limitation of that independent tool, not of this
    // crate. The raw wire bytes literally contain the still-escaped text
    // (confirmed via hex dump: `od -c tests/fixtures/special_chars.abx`),
    // and this crate faithfully reports exactly that rather than silently
    // "fixing" it.
    assert_eq!(title, "Tom &amp; Jerry &lt;3&gt;");

    // Entities inside *text content* are correctly split into their own
    // EntityReference events by xml2abx, and this crate reconstructs the
    // exact original escaped form on re-render — a genuine round trip, not
    // a coincidence of this specific input.
    let xml = abx::abx_to_xml(data).unwrap();
    assert!(xml.contains(r#">Use &quot;quotes&quot; &amp; &apos;apostrophes&apos; safely<"#));
}

#[test]
fn repeated_strings_fixture_interning() {
    let data = include_bytes!("fixtures/repeated_strings.abx");
    let evs = events(data);

    let items: Vec<(&str, &str, &str)> = evs
        .iter()
        .filter_map(|e| match e {
            Event::StartTag { name, attributes } if name == "item" => Some((
                attr(attributes, "id").as_string()?,
                attr(attributes, "category").as_string()?,
                attr(attributes, "name").as_string()?,
            )),
            _ => None,
        })
        .collect();

    assert_eq!(
        items,
        vec![
            ("1", "tools", "Hammer"),
            ("2", "tools", "Wrench"),
            ("3", "tools", "Screwdriver"),
            ("4", "parts", "Bolt"),
            ("5", "parts", "Nut"),
            ("6", "parts", "Washer"),
            ("7", "tools", "Pliers"),
        ]
    );

    // Same real, interning-heavy data through the streaming parser must
    // decode identically to the slice parser.
    let mut sp = AbxStreamParser::new(Cursor::new(data.to_vec())).unwrap();
    assert_eq!(sp.collect_events().unwrap(), evs);
}
