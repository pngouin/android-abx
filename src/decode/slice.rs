//! [`AbxParser`] — zero-allocation pull parser over an in-memory `&[u8]` —
//! and [`AbxParserOwned`], a heap-owning wrapper around it.
//!
//! The public surface is intentionally identical to
//! [`AbxStreamParser`](crate::AbxStreamParser) so the two types are
//! interchangeable; just swap the constructor.

use std::collections::HashMap;

use nom::{
    IResult, Parser,
    bytes::complete::take,
    number::complete::{be_f32, be_f64, be_i32, be_i64, be_u8, be_u16},
};

use crate::{
    AbxError, Attribute, AttributeValue, CMD_ATTRIBUTE, CMD_CDSECT, CMD_COMMENT, CMD_DOCDECL,
    CMD_END_DOCUMENT, CMD_END_TAG, CMD_ENTITY_REF, CMD_IGNORABLE_WHITESPACE,
    CMD_PROCESSING_INSTRUCTION, CMD_START_DOCUMENT, CMD_START_TAG, CMD_TEXT, Event, MAGIC, Result,
    TYPE_BOOLEAN_FALSE, TYPE_BOOLEAN_TRUE, TYPE_BYTES_BASE64, TYPE_BYTES_HEX, TYPE_DOUBLE,
    TYPE_FLOAT, TYPE_INT, TYPE_INT_HEX, TYPE_LONG, TYPE_LONG_HEX, TYPE_NULL, TYPE_STRING,
    TYPE_STRING_INTERNED, render_event,
};

use crate::INTERNED_NEW;
use crate::InternedStr;

// ---------------------------------------------------------------------------
// Low-level nom parsers (stateless, operate on &[u8])
// ---------------------------------------------------------------------------

fn parse_utf_string(input: &[u8]) -> IResult<&[u8], String> {
    let (input, len) = be_u16(input)?;
    let (input, bytes) = take(len).parse(input)?;
    let s = std::str::from_utf8(bytes)
        .map_err(|_| {
            nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Verify))
        })?
        .to_owned();
    Ok((input, s))
}

fn parse_bytes_blob(input: &[u8]) -> IResult<&[u8], Vec<u8>> {
    let (input, len) = be_u16(input)?;
    let (input, bytes) = take(len).parse(input)?;
    Ok((input, bytes.to_vec()))
}

// ---------------------------------------------------------------------------
// Slice-based parser  (AbxParser)
// ---------------------------------------------------------------------------

/// Zero-allocation pull parser that works on an in-memory `&[u8]`.
///
/// Advance through the document with [`next_event`](AbxParser::next_event) or
/// use one of the higher-level helpers.
#[derive(Debug)]
pub struct AbxParser<'a> {
    rest: &'a [u8],
    pool: Vec<InternedStr>,
}

impl<'a> AbxParser<'a> {
    /// Create a parser, validating the 4-byte magic header.
    pub fn new(input: &'a [u8]) -> Result<Self> {
        if input.len() < 4 {
            return Err(AbxError::UnexpectedEof("magic header"));
        }
        let magic: [u8; 4] = input[..4].try_into().unwrap();
        if magic != MAGIC {
            return Err(AbxError::InvalidMagic {
                expected: MAGIC,
                actual: magic,
            });
        }
        Ok(AbxParser {
            rest: &input[4..],
            pool: Vec::with_capacity(32),
        })
    }

    /// `true` when no more bytes remain.
    pub fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }

    // -- internal helpers --

    fn run<F, T>(&mut self, f: F) -> Result<T>
    where
        F: Fn(&'a [u8]) -> IResult<&'a [u8], T>,
    {
        let (rest, val) = f(self.rest).map_err(|e| AbxError::Nom(format!("{e:?}")))?;
        self.rest = rest;
        Ok(val)
    }

    fn read_u8(&mut self) -> Result<u8> {
        self.run(be_u8)
    }
    fn read_u16(&mut self) -> Result<u16> {
        self.run(be_u16)
    }
    fn read_i32(&mut self) -> Result<i32> {
        self.run(be_i32)
    }
    fn read_i64(&mut self) -> Result<i64> {
        self.run(be_i64)
    }
    fn read_f32(&mut self) -> Result<f32> {
        self.run(be_f32)
    }
    fn read_f64(&mut self) -> Result<f64> {
        self.run(be_f64)
    }

    fn read_utf(&mut self) -> Result<String> {
        self.run(parse_utf_string)
    }
    fn read_bytes_blob(&mut self) -> Result<Vec<u8>> {
        self.run(parse_bytes_blob)
    }

    /// Read an interned string. Every occurrence after the first is a
    /// back-reference into `pool`, resolved with `InternedStr::clone` (a
    /// refcount bump) rather than a fresh allocation and copy.
    fn read_interned(&mut self) -> Result<InternedStr> {
        let idx = self.read_u16()?;
        if idx == INTERNED_NEW {
            let s: InternedStr = self.read_utf()?.into();
            self.pool.push(s.clone());
            Ok(s)
        } else {
            self.pool
                .get(idx as usize)
                .cloned()
                .ok_or(AbxError::BadInternedIndex(idx))
        }
    }

    fn read_attr_value(&mut self, type_nibble: u8) -> Result<AttributeValue> {
        match type_nibble {
            TYPE_NULL => Ok(AttributeValue::Null),
            TYPE_STRING => Ok(AttributeValue::String(self.read_utf()?)),
            TYPE_STRING_INTERNED => Ok(AttributeValue::String(self.read_interned()?.to_string())),
            TYPE_BYTES_HEX => Ok(AttributeValue::BytesHex(self.read_bytes_blob()?)),
            TYPE_BYTES_BASE64 => Ok(AttributeValue::BytesBase64(self.read_bytes_blob()?)),
            TYPE_INT => Ok(AttributeValue::Int(self.read_i32()?)),
            TYPE_INT_HEX => Ok(AttributeValue::IntHex(self.read_i32()? as u32)),
            TYPE_LONG => Ok(AttributeValue::Long(self.read_i64()?)),
            TYPE_LONG_HEX => Ok(AttributeValue::LongHex(self.read_i64()? as u64)),
            TYPE_FLOAT => Ok(AttributeValue::Float(self.read_f32()?)),
            TYPE_DOUBLE => Ok(AttributeValue::Double(self.read_f64()?)),
            TYPE_BOOLEAN_TRUE => Ok(AttributeValue::Boolean(true)),
            TYPE_BOOLEAN_FALSE => Ok(AttributeValue::Boolean(false)),
            other => Err(AbxError::UnknownAttributeType(other)),
        }
    }

    // -- public event API --

    /// Pull the next [`Event`].  Returns `None` at end of input.
    pub fn next_event(&mut self) -> Result<Option<Event>> {
        if self.rest.is_empty() {
            return Ok(None);
        }

        let token = self.read_u8()?;
        let cmd = token & 0x0F;
        let type_nibble = token & 0xF0;

        let event = match cmd {
            CMD_START_DOCUMENT => Event::StartDocument,
            CMD_END_DOCUMENT => return Ok(Some(Event::EndDocument)),

            CMD_START_TAG => {
                let name = self.read_interned()?;
                let mut attributes = Vec::new();
                loop {
                    if self.rest.is_empty() {
                        break;
                    }
                    let next = self.rest[0];
                    if (next & 0x0F) != CMD_ATTRIBUTE {
                        break;
                    }
                    self.rest = &self.rest[1..];
                    let attr_type = next & 0xF0;
                    let attr_name = self.read_interned()?;
                    let attr_value = self.read_attr_value(attr_type)?;
                    attributes.push(Attribute {
                        name: attr_name,
                        value: attr_value,
                    });
                }
                Event::StartTag { name, attributes }
            }

            CMD_END_TAG => Event::EndTag {
                name: self.read_interned()?,
            },

            CMD_TEXT => Event::Text(if type_nibble == TYPE_STRING {
                self.read_utf()?
            } else {
                String::new()
            }),
            CMD_CDSECT => Event::CdataSection(if type_nibble == TYPE_STRING {
                self.read_utf()?
            } else {
                String::new()
            }),
            CMD_ENTITY_REF => Event::EntityReference(if type_nibble == TYPE_STRING {
                self.read_utf()?
            } else {
                String::new()
            }),
            CMD_IGNORABLE_WHITESPACE => Event::IgnorableWhitespace(if type_nibble == TYPE_STRING {
                self.read_utf()?
            } else {
                String::new()
            }),
            CMD_PROCESSING_INSTRUCTION => {
                Event::ProcessingInstruction(if type_nibble == TYPE_STRING {
                    self.read_utf()?
                } else {
                    String::new()
                })
            }
            CMD_COMMENT => Event::Comment(if type_nibble == TYPE_STRING {
                self.read_utf()?
            } else {
                String::new()
            }),
            CMD_DOCDECL => Event::DocDecl(if type_nibble == TYPE_STRING {
                self.read_utf()?
            } else {
                String::new()
            }),

            other => return Err(AbxError::UnknownCommand(other)),
        };

        Ok(Some(event))
    }

    // -- convenience API (mirrors AbxStreamParser) --

    /// Drain all remaining events into a `Vec`.
    pub fn collect_events(&mut self) -> Result<Vec<Event>> {
        let mut events = Vec::new();
        while let Some(ev) = self.next_event()? {
            events.push(ev);
        }
        Ok(events)
    }

    /// Return the value of the first matching `attr_name` inside any
    /// `<element_name>` in the remaining stream.
    pub fn find_attribute(&mut self, element: &str, attr: &str) -> Option<AttributeValue> {
        loop {
            match self.next_event().ok()? {
                Some(Event::StartTag { name, attributes }) if name == element => {
                    if let Some(a) = attributes.into_iter().find(|a| a.name == attr) {
                        return Some(a.value);
                    }
                }
                Some(Event::EndDocument) | None => return None,
                _ => {}
            }
        }
    }

    /// All values of `attr_name` found in `<element_name>` tags.
    pub fn find_all_attributes(
        &mut self,
        element: &str,
        attr: &str,
    ) -> Result<Vec<AttributeValue>> {
        let mut out = Vec::new();
        while let Some(ev) = self.next_event()? {
            if let Event::StartTag { name, attributes } = ev
                && name == element
            {
                out.extend(
                    attributes
                        .into_iter()
                        .filter(|a| a.name == attr)
                        .map(|a| a.value),
                );
            }
        }
        Ok(out)
    }

    /// Attributes of the first `<element_name>` tag.
    pub fn attributes_of(&mut self, element: &str) -> Option<Vec<Attribute>> {
        loop {
            match self.next_event().ok()? {
                Some(Event::StartTag { name, attributes }) if name == element => {
                    return Some(attributes);
                }
                Some(Event::EndDocument) | None => return None,
                _ => {}
            }
        }
    }

    /// Attributes of every `<element_name>` tag.
    pub fn all_attributes_of(&mut self, element: &str) -> Result<Vec<Vec<Attribute>>> {
        let mut out = Vec::new();
        while let Some(ev) = self.next_event()? {
            if let Event::StartTag { name, attributes } = ev
                && name == element
            {
                out.push(attributes);
            }
        }
        Ok(out)
    }

    /// Render the rest of the document as an XML string.
    pub fn to_xml(&mut self) -> Result<String> {
        let mut buf = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        while let Some(ev) = self.next_event()? {
            if matches!(ev, Event::EndDocument) {
                break;
            }
            render_event(&ev, &mut buf);
        }
        Ok(buf)
    }

    /// Find the next `<element>`, deserialize its attributes (and direct
    /// text content, via a `#[serde(rename = "$text")]` field) into `T`,
    /// then skip past its matching end tag. `Ok(None)` at end of document.
    #[cfg(feature = "serialize")]
    pub fn deserialize_next<T: serde::de::DeserializeOwned>(
        &mut self,
        element: &str,
    ) -> Result<Option<T>> {
        crate::de::find_and_consume_element(self, element)
    }

    /// Deserialize every remaining `<element>` into a `Vec<T>`.
    #[cfg(feature = "serialize")]
    pub fn deserialize_all<T: serde::de::DeserializeOwned>(
        &mut self,
        element: &str,
    ) -> Result<Vec<T>> {
        let mut out = Vec::new();
        while let Some(item) = self.deserialize_next(element)? {
            out.push(item);
        }
        Ok(out)
    }

    /// Collect the whole document into a `HashMap<element → Vec<HashMap<attr → value_str>>>`.
    pub fn into_map(mut self) -> Result<HashMap<String, Vec<HashMap<String, String>>>> {
        let mut map: HashMap<String, Vec<HashMap<String, String>>> = HashMap::new();
        while let Some(ev) = self.next_event()? {
            if let Event::StartTag { name, attributes } = ev {
                let entry = map.entry(name.into()).or_default();
                let mut attrs = HashMap::new();
                for attr in attributes {
                    attrs.insert(attr.name.into(), attr.value.as_str().into_owned());
                }
                entry.push(attrs);
            }
        }
        Ok(map)
    }
}

// ---------------------------------------------------------------------------
// Owned wrapper
// ---------------------------------------------------------------------------

/// Heap-owning wrapper. Stores the raw bytes and hands out [`AbxParser`]
/// borrows without lifetime gymnastics on the call-site.
///
/// ```rust,ignore
/// let owned = AbxParserOwned::new(std::fs::read("foo.abx")?)?;
/// let xml = owned.parser()?.to_xml()?;
/// ```
#[derive(Debug)]
pub struct AbxParserOwned {
    data: Vec<u8>,
}

impl AbxParserOwned {
    /// Validate the magic header and store the bytes.
    pub fn new(data: Vec<u8>) -> Result<Self> {
        if data.len() < 4 {
            return Err(AbxError::UnexpectedEof("magic header"));
        }
        let magic: [u8; 4] = data[..4].try_into().unwrap();
        if magic != MAGIC {
            return Err(AbxError::InvalidMagic {
                expected: MAGIC,
                actual: magic,
            });
        }
        Ok(Self { data })
    }

    /// Create a fresh [`AbxParser`] borrowing from the stored bytes.
    pub fn parser(&self) -> Result<AbxParser<'_>> {
        AbxParser::new(&self.data)
    }
}
