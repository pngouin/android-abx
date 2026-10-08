#![allow(dead_code)]

#[path = "../../tests/common/mod.rs"]
mod wire;
pub use wire::*;

use android_abx::{Attribute, AttributeValue, Event};

/// `n` `<pkg>` elements, each with string, int and bool attributes.
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

/// [`synthetic_document`] as events.
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

/// [`synthetic_document`] as XML text.
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
