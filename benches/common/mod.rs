#![allow(dead_code)]
//! Shared synthetic-data generator for the criterion benches in this
//! directory, reusing the same wire-format builders as the integration
//! tests (`tests/common/mod.rs`) rather than keeping a second copy.

#[path = "../../tests/common/mod.rs"]
mod wire;
pub use wire::*;

use abx::{Attribute, AttributeValue, Event};

/// A synthetic ABX document with `n` repeated `<pkg>` elements, each
/// carrying a string, an int, and a bool attribute — roughly the shape of
/// a real AOSP `packages.xml` record (see `tests/fixtures/simple_pkg.xml`,
/// generated from real data via the local `xml2abx` tool).
pub fn synthetic_document(n: usize) -> Vec<u8> {
    let mut parts = Vec::with_capacity(n * 5);
    for i in 0..n {
        parts.push(start_tag("pkg"));
        parts.push(attr_string("name", &format!("com.example.app{i}")));
        parts.push(attr_int("version", i as i32));
        parts.push(attr_bool("enabled", i % 2 == 0));
        parts.push(end_tag("pkg"));
    }
    document(&parts)
}

/// The `Event`-stream equivalent of `synthetic_document(n)` — same shape,
/// for benchmarking the encode direction (`AbxWriter`/`events_to_abx`).
pub fn synthetic_events(n: usize) -> Vec<Event> {
    let mut events = Vec::with_capacity(n * 2 + 2);
    events.push(Event::StartDocument);
    for i in 0..n {
        events.push(Event::StartTag {
            name: "pkg".into(),
            attributes: vec![
                Attribute {
                    name: "name".into(),
                    value: AttributeValue::String(format!("com.example.app{i}")),
                },
                Attribute {
                    name: "version".into(),
                    value: AttributeValue::Int(i as i32),
                },
                Attribute {
                    name: "enabled".into(),
                    value: AttributeValue::Boolean(i % 2 == 0),
                },
            ],
        });
        events.push(Event::EndTag { name: "pkg".into() });
    }
    events.push(Event::EndDocument);
    events
}

/// The plain-XML-text equivalent of `synthetic_document(n)` — for
/// benchmarking `xml_to_abx`.
pub fn synthetic_xml(n: usize) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<packages>\n");
    for i in 0..n {
        s.push_str(&format!(
            "  <pkg name=\"com.example.app{i}\" version=\"{i}\" enabled=\"{}\"/>\n",
            i % 2 == 0
        ));
    }
    s.push_str("</packages>\n");
    s
}
