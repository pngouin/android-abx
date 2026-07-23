#![cfg(feature = "xml")]
//! Integration tests for the XML-text → ABX pipeline (`xml_to_abx`, needs
//! the `xml` feature / `quick-xml`).

use abx::{AbxParser, Attribute, AttributeValue, Event};

#[test]
fn xml_to_abx_encodes_self_closing_tag() {
    let bytes = abx::xml_to_abx("<a/>").unwrap();
    let events = AbxParser::new(&bytes).unwrap().collect_events().unwrap();
    assert_eq!(
        events,
        vec![
            Event::StartDocument,
            Event::StartTag {
                name: "a".into(),
                attributes: vec![]
            },
            Event::EndTag { name: "a".into() },
            Event::EndDocument,
        ]
    );
}

fn attr<'a>(attributes: &'a [Attribute], name: &str) -> &'a AttributeValue {
    &attributes.iter().find(|a| a.name == name).unwrap().value
}

#[test]
fn xml_to_abx_encodes_cdata_comment_pi_doctype() {
    let xml =
        "<!DOCTYPE root><root><!--a comment--><![CDATA[raw <not-a-tag>]]><?pi target data?></root>";
    let bytes = abx::xml_to_abx(xml).unwrap();
    let events = AbxParser::new(&bytes).unwrap().collect_events().unwrap();
    assert_eq!(
        events,
        vec![
            Event::StartDocument,
            Event::DocDecl("root".into()),
            Event::StartTag {
                name: "root".into(),
                attributes: vec![]
            },
            Event::Comment("a comment".into()),
            Event::CdataSection("raw <not-a-tag>".into()),
            Event::ProcessingInstruction("pi target data".into()),
            Event::EndTag {
                name: "root".into()
            },
            Event::EndDocument,
        ]
    );
}

#[test]
fn xml_to_abx_passes_namespace_prefixed_names_through_opaquely() {
    // Plain quick_xml::Reader (not NsReader) is used deliberately -- this
    // crate's Event/Attribute model has no namespace concept, so `foo:bar`
    // stays a literal opaque name, matching how ABX itself treats it.
    let xml = r#"<ns:root xmlns:ns="urn:example" ns:attr="v"></ns:root>"#;
    let bytes = abx::xml_to_abx(xml).unwrap();
    let events = AbxParser::new(&bytes).unwrap().collect_events().unwrap();
    match &events[1] {
        Event::StartTag { name, attributes } => {
            assert_eq!(name, "ns:root");
            assert_eq!(*attr(attributes, "xmlns:ns"), s("urn:example"));
            assert_eq!(*attr(attributes, "ns:attr"), s("v"));
        }
        other => panic!("expected StartTag, got {other:?}"),
    }
    assert_eq!(
        events[2],
        Event::EndTag {
            name: "ns:root".into()
        }
    );
}

#[test]
fn xml_to_abx_preserves_whitespace_only_text_literally() {
    // No IgnorableWhitespace guessing: indentation between sibling elements
    // is preserved as ordinary Text, byte for byte.
    let xml = "<root>\n  <a/>\n  <b/>\n</root>";
    let bytes = abx::xml_to_abx(xml).unwrap();
    let events = AbxParser::new(&bytes).unwrap().collect_events().unwrap();
    assert_eq!(
        events,
        vec![
            Event::StartDocument,
            Event::StartTag {
                name: "root".into(),
                attributes: vec![]
            },
            Event::Text("\n  ".into()),
            Event::StartTag {
                name: "a".into(),
                attributes: vec![]
            },
            Event::EndTag { name: "a".into() },
            Event::Text("\n  ".into()),
            Event::StartTag {
                name: "b".into(),
                attributes: vec![]
            },
            Event::EndTag { name: "b".into() },
            Event::Text("\n".into()),
            Event::EndTag {
                name: "root".into()
            },
            Event::EndDocument,
        ]
    );
}

fn s(v: &str) -> AttributeValue {
    AttributeValue::String(v.to_string())
}

#[test]
fn simple_pkg_fixture_round_trips() {
    let xml = include_str!("fixtures/simple_pkg.xml");
    let bytes = abx::xml_to_abx(xml).unwrap();
    let attrs = AbxParser::new(&bytes)
        .unwrap()
        .attributes_of("pkg")
        .unwrap()
        .unwrap();
    assert_eq!(*attr(&attrs, "name"), s("com.example.chat"));
    assert_eq!(*attr(&attrs, "version"), s("3"));
    assert_eq!(*attr(&attrs, "flags"), s("1"));
}

#[test]
fn nested_permissions_fixture_round_trips() {
    let xml = include_str!("fixtures/nested_permissions.xml");
    let bytes = abx::xml_to_abx(xml).unwrap();
    let events = AbxParser::new(&bytes).unwrap().collect_events().unwrap();

    let pkg_name = events.iter().find_map(|e| match e {
        Event::StartTag { name, attributes } if name == "pkg" => {
            attr(attributes, "name").as_string()
        }
        _ => None,
    });
    assert_eq!(pkg_name, Some("com.example.chat"));

    let permission_names: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            Event::StartTag { name, attributes } if name == "permission" => {
                attr(attributes, "name").as_string()
            }
            _ => None,
        })
        .collect();
    assert_eq!(permission_names, vec!["INTERNET", "CAMERA"]);

    let description_start = events
        .iter()
        .position(|e| matches!(e, Event::StartTag { name, .. } if name == "description"))
        .unwrap();
    assert_eq!(
        events[description_start + 1],
        Event::Text("A chat app".into())
    );
}

#[test]
fn booleans_fixture_stays_string_typed() {
    // xml2abx infers TYPE_BOOLEAN_TRUE/FALSE here; xml_to_abx does not --
    // every attribute value stays a plain String.
    let xml = include_str!("fixtures/booleans.xml");
    let bytes = abx::xml_to_abx(xml).unwrap();
    let attrs = AbxParser::new(&bytes)
        .unwrap()
        .attributes_of("settings")
        .unwrap()
        .unwrap();
    assert_eq!(*attr(&attrs, "enabled"), s("true"));
    assert_eq!(*attr(&attrs, "hidden"), s("false"));
    assert_eq!(*attr(&attrs, "count"), s("12345"));
    assert_eq!(*attr(&attrs, "ratio"), s("3.14"));
}

#[test]
fn repeated_strings_fixture_round_trips() {
    let xml = include_str!("fixtures/repeated_strings.xml");
    let bytes = abx::xml_to_abx(xml).unwrap();
    let events = AbxParser::new(&bytes).unwrap().collect_events().unwrap();

    let items: Vec<(&str, &str, &str)> = events
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
}

#[test]
fn special_chars_fixture_decodes_entities_consistently() {
    // Unlike xml2abx (which leaves attribute-value entities raw/escaped),
    // xml_to_abx decodes entities the same way in attributes and text.
    let xml = include_str!("fixtures/special_chars.xml");
    let bytes = abx::xml_to_abx(xml).unwrap();
    let mut p = AbxParser::new(&bytes).unwrap();
    let attrs = p.attributes_of("note").unwrap().unwrap();
    assert_eq!(*attr(&attrs, "title"), s("Tom & Jerry <3>"));

    // Round-tripping back to XML re-escapes the decoded characters, so the
    // rendered attribute text matches the original source exactly.
    let full_xml = abx::xml_to_abx(xml)
        .and_then(|b| abx::abx_to_xml(&b))
        .unwrap();
    assert!(full_xml.contains(r#"title="Tom &amp; Jerry &lt;3&gt;""#));
    assert!(full_xml.contains(r#">Use &quot;quotes&quot; &amp; &apos;apostrophes&apos; safely<"#));
}
