//! [`xml_to_abx`] — parses plain XML text with `quick-xml`'s `Reader` (not
//! `NsReader` — no namespace concept, `foo:bar` stays an opaque name) and
//! re-encodes it as ABX bytes via [`AbxWriter`].
//!
//! UTF-8 input only. Attribute values always become
//! `AttributeValue::String` — no int/bool inference, matching AOSP's own
//! `attribute()`. The `<?xml ...?>` prolog is consumed, not emitted;
//! `StartDocument`/`EndDocument` are synthesized as bookends instead.
//!
//! `quick-xml` splits `&ref;`/`&#N;` references out of text as their own
//! `GeneralRef` event rather than inlining them — maps directly onto
//! `Event::EntityReference`. `IgnorableWhitespace` is never produced:
//! whitespace-only runs are preserved literally as `Text`.

use quick_xml::events::{BytesStart, Event as XmlEvent};
use quick_xml::reader::Reader;
use quick_xml::XmlVersion;

use super::AbxWriter;
use crate::{AbxError, Attribute, AttributeValue, Event, InternedStr, Result};

fn utf8(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|_| AbxError::InvalidUtf8)
}

fn xml_err(e: impl std::fmt::Display) -> AbxError {
    AbxError::Xml(e.to_string())
}

fn start_tag_attributes(start: &BytesStart) -> Result<Vec<Attribute>> {
    let mut attributes = Vec::new();
    for attr in start.attributes() {
        let attr = attr.map_err(xml_err)?;
        let name = utf8(attr.key.as_ref())?;
        let value = attr.normalized_value(XmlVersion::Implicit1_0).map_err(xml_err)?;
        attributes.push(Attribute {
            name: InternedStr::from(name),
            value: AttributeValue::String(value.into_owned()),
        });
    }
    Ok(attributes)
}

/// Encode XML text to ABX bytes.
pub fn xml_to_abx(xml: &str) -> Result<Vec<u8>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut w = AbxWriter::new(Vec::new())?;
    w.write_event(&Event::StartDocument)?;

    loop {
        match reader.read_event().map_err(xml_err)? {
            XmlEvent::Start(start) => {
                let name = utf8(start.name().as_ref())?.to_string();
                let attributes = start_tag_attributes(&start)?;
                w.write_event(&Event::StartTag { name: name.into(), attributes })?;
            }
            XmlEvent::Empty(start) => {
                let name: InternedStr = utf8(start.name().as_ref())?.into();
                let attributes = start_tag_attributes(&start)?;
                w.write_event(&Event::StartTag { name: name.clone(), attributes })?;
                w.write_event(&Event::EndTag { name })?;
            }
            XmlEvent::End(end) => {
                let name = utf8(end.name().as_ref())?.to_string();
                w.write_event(&Event::EndTag { name: name.into() })?;
            }
            XmlEvent::Text(text) => {
                let s = text.decode().map_err(xml_err)?;
                w.write_event(&Event::Text(s.into_owned()))?;
            }
            XmlEvent::GeneralRef(bytes_ref) => {
                let s = bytes_ref.decode().map_err(xml_err)?;
                w.write_event(&Event::EntityReference(s.into_owned()))?;
            }
            XmlEvent::CData(cdata) => {
                let s = utf8(cdata.as_ref())?.to_string();
                w.write_event(&Event::CdataSection(s))?;
            }
            XmlEvent::Comment(comment) => {
                let s = utf8(comment.as_ref())?.to_string();
                w.write_event(&Event::Comment(s))?;
            }
            XmlEvent::PI(pi) => {
                let s = utf8(pi.as_ref())?.to_string();
                w.write_event(&Event::ProcessingInstruction(s))?;
            }
            XmlEvent::DocType(doctype) => {
                let s = utf8(doctype.as_ref())?.to_string();
                w.write_event(&Event::DocDecl(s))?;
            }
            XmlEvent::Decl(_) => {}
            XmlEvent::Eof => break,
        }
    }

    w.write_event(&Event::EndDocument)?;
    Ok(w.into_inner())
}
