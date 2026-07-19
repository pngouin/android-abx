//! # abx — Android Binary XML parser
//!
//! Parses the ABX (Android Binary XML) format produced by `BinaryXmlSerializer`
//! and read back by `BinaryXmlPullParser` in AOSP. Wire-format constants are
//! verified against AOSP's own source
//! (`BinaryXmlSerializer.java`/`FastDataOutput.java`) and tested against
//! real `.abx` files from an independent encoder, not just this crate's own
//! synthetic test data — see `tests/aosp_fixture_tests.rs`.
//!
//! ## Two parsers, one format
//!
//! | Parser | Input | When to use |
//! |---|---|---|
//! | [`AbxParser`] | `&[u8]` | Data already in memory |
//! | [`AbxStreamParser`] | `impl Read` | Files, sockets, pipes — any reader |
//!
//! ## Format overview
//!
//! Every file starts with the 4-byte magic `ABX\0` (`0x41 0x42 0x58 0x00`).
//! After the magic each token is a single byte split into two nibbles:
//!
//! ```text
//! high nibble (0xF0) → data-type  (TYPE_STRING, TYPE_INT, …)
//! low  nibble (0x0F) → event kind (START_TAG, ATTRIBUTE, …)
//! ```
//!
//! Interned strings are prefixed with a `u16` index; the sentinel value
//! `0xFFFF` means "new string follows as a length-prefixed UTF-8 blob".
//!
//! ## Quick start
//!
//! ```rust,ignore
//! // Slice-based
//! use abx::AbxParser;
//! let data = std::fs::read("foo.abx")?;
//! let mut p = AbxParser::new(&data)?;
//! while let Some(ev) = p.next_event()? { println!("{ev:?}"); }
//!
//! // Stream-based (no intermediate Vec)
//! use abx::AbxStreamParser;
//! let file = std::fs::File::open("foo.abx")?;
//! let mut p = AbxStreamParser::new(std::io::BufReader::new(file))?;
//! while let Some(ev) = p.next_event()? { println!("{ev:?}"); }
//!
//! // Convenience helper
//! let mut p = abx::open_file("foo.abx")?;
//! let xml = p.to_xml()?;
//! ```

use std::collections::HashMap;

use base64::Engine as _;
use nom::{
    bytes::complete::take,
    number::complete::{be_f32, be_f64, be_i32, be_i64, be_u16, be_u8},
    IResult,
};

// ---------------------------------------------------------------------------
// Protocol constants (mirrors BinaryXmlSerializer.java)
// ---------------------------------------------------------------------------

/// Magic header bytes: `ABX\0`
pub const MAGIC: [u8; 4] = [0x41, 0x42, 0x58, 0x00];

/// Sentinel in the interned-string index that signals a new string.
pub(crate) const INTERNED_NEW: u16 = 0xFFFF;

// Token command (low nibble)
pub(crate) const CMD_START_DOCUMENT: u8 = 0x00;
pub(crate) const CMD_END_DOCUMENT: u8 = 0x01;
pub(crate) const CMD_START_TAG: u8 = 0x02;
pub(crate) const CMD_END_TAG: u8 = 0x03;
pub(crate) const CMD_TEXT: u8 = 0x04;
pub(crate) const CMD_CDSECT: u8 = 0x05;
pub(crate) const CMD_ENTITY_REF: u8 = 0x06;
pub(crate) const CMD_IGNORABLE_WHITESPACE: u8 = 0x07;
pub(crate) const CMD_PROCESSING_INSTRUCTION: u8 = 0x08;
pub(crate) const CMD_COMMENT: u8 = 0x09;
pub(crate) const CMD_DOCDECL: u8 = 0x0A;
pub(crate) const CMD_ATTRIBUTE: u8 = 0x0F;

// Data type (high nibble). Values match AOSP's BinaryXmlSerializer.java
// exactly: `n << 4` for n = 1..=13 (high-nibble 0x00 is never used on the
// wire — every token always OR's in an explicit type flag, even the
// "absent value" case, TYPE_NULL).
pub(crate) const TYPE_NULL: u8 = 0x10;
pub(crate) const TYPE_STRING: u8 = 0x20;
pub(crate) const TYPE_STRING_INTERNED: u8 = 0x30;
pub(crate) const TYPE_BYTES_HEX: u8 = 0x40;
pub(crate) const TYPE_BYTES_BASE64: u8 = 0x50;
pub(crate) const TYPE_INT: u8 = 0x60;
pub(crate) const TYPE_INT_HEX: u8 = 0x70;
pub(crate) const TYPE_LONG: u8 = 0x80;
pub(crate) const TYPE_LONG_HEX: u8 = 0x90;
pub(crate) const TYPE_FLOAT: u8 = 0xA0;
pub(crate) const TYPE_DOUBLE: u8 = 0xB0;
pub(crate) const TYPE_BOOLEAN_TRUE: u8 = 0xC0;
pub(crate) const TYPE_BOOLEAN_FALSE: u8 = 0xD0;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum AbxError {
    #[error("invalid magic header: expected {expected:?}, got {actual:?}")]
    InvalidMagic {
        expected: [u8; 4],
        actual: [u8; 4],
    },
    #[error("unexpected end of input while reading {0}")]
    UnexpectedEof(&'static str),
    #[error("invalid interned string index {0}")]
    BadInternedIndex(u16),
    #[error("invalid UTF-8 in string")]
    InvalidUtf8,
    #[error("unknown attribute type 0x{0:02X}")]
    UnknownAttributeType(u8),
    #[error("unknown command 0x{0:02X}")]
    UnknownCommand(u8),
    #[error("nom parse error: {0}")]
    Nom(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("deserialization error: {0}")]
    Deserialization(String),
}

impl<I: std::fmt::Debug> From<nom::Err<nom::error::Error<I>>> for AbxError {
    fn from(e: nom::Err<nom::error::Error<I>>) -> Self {
        AbxError::Nom(format!("{:?}", e))
    }
}

pub type Result<T> = std::result::Result<T, AbxError>;

// ---------------------------------------------------------------------------
// Typed attribute value
// ---------------------------------------------------------------------------

/// The typed payload of an XML attribute.
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    Null,
    String(String),
    /// Bytes whose canonical text form is lowercase hex.
    BytesHex(Vec<u8>),
    /// Bytes whose canonical text form is Base64.
    BytesBase64(Vec<u8>),
    Int(i32),
    IntHex(u32),
    Long(i64),
    LongHex(u64),
    Float(f32),
    Double(f64),
    Boolean(bool),
}

impl AttributeValue {
    /// Render the value as a human-readable string, mirroring the original
    /// Java serializer's output.
    pub fn as_str(&self) -> std::borrow::Cow<'_, str> {
        use std::borrow::Cow;
        match self {
            AttributeValue::Null => Cow::Borrowed(""),
            AttributeValue::String(s) => Cow::Borrowed(s.as_str()),
            AttributeValue::BytesHex(b) => Cow::Owned(faster_hex::hex_string(b)),
            AttributeValue::BytesBase64(b) => {
                Cow::Owned(base64::engine::general_purpose::STANDARD.encode(b))
            }
            AttributeValue::Int(v) => Cow::Owned(v.to_string()),
            AttributeValue::IntHex(v) => {
                if *v == u32::MAX {
                    Cow::Owned("-1".to_string())
                } else {
                    Cow::Owned(format!("{:x}", v))
                }
            }
            AttributeValue::Long(v) => Cow::Owned(v.to_string()),
            AttributeValue::LongHex(v) => {
                if *v == u64::MAX {
                    Cow::Owned("-1".to_string())
                } else {
                    Cow::Owned(format!("{:x}", v))
                }
            }
            AttributeValue::Float(v) => {
                if v.fract() == 0.0 && v.is_finite() {
                    Cow::Owned(format!("{:.1}", v))
                } else {
                    Cow::Owned(v.to_string())
                }
            }
            AttributeValue::Double(v) => {
                if v.fract() == 0.0 && v.is_finite() {
                    Cow::Owned(format!("{:.1}", v))
                } else {
                    Cow::Owned(v.to_string())
                }
            }
            AttributeValue::Boolean(b) => {
                if *b {
                    Cow::Borrowed("true")
                } else {
                    Cow::Borrowed("false")
                }
            }
        }
    }

    // Typed accessors ----------------------------------------------------------

    pub fn as_string(&self) -> Option<&str> {
        if let AttributeValue::String(s) = self { Some(s) } else { None }
    }
    pub fn as_int(&self) -> Option<i32> {
        if let AttributeValue::Int(v) = self { Some(*v) } else { None }
    }
    pub fn as_int_hex(&self) -> Option<u32> {
        if let AttributeValue::IntHex(v) = self { Some(*v) } else { None }
    }
    pub fn as_long(&self) -> Option<i64> {
        if let AttributeValue::Long(v) = self { Some(*v) } else { None }
    }
    pub fn as_long_hex(&self) -> Option<u64> {
        if let AttributeValue::LongHex(v) = self { Some(*v) } else { None }
    }
    pub fn as_float(&self) -> Option<f32> {
        if let AttributeValue::Float(v) = self { Some(*v) } else { None }
    }
    pub fn as_double(&self) -> Option<f64> {
        if let AttributeValue::Double(v) = self { Some(*v) } else { None }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let AttributeValue::Boolean(b) = self { Some(*b) } else { None }
    }
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            AttributeValue::BytesHex(b) | AttributeValue::BytesBase64(b) => Some(b),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Attribute
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub name: String,
    pub value: AttributeValue,
}

impl Attribute {
    pub fn as_str(&self) -> std::borrow::Cow<'_, str> {
        self.value.as_str()
    }
}

// ---------------------------------------------------------------------------
// XML Event  (shared by both parsers)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    StartDocument,
    EndDocument,
    StartTag { name: String, attributes: Vec<Attribute> },
    EndTag { name: String },
    Text(String),
    CdataSection(String),
    Comment(String),
    ProcessingInstruction(String),
    EntityReference(String),
    IgnorableWhitespace(String),
    DocDecl(String),
}

// ---------------------------------------------------------------------------
// Shared XML rendering helper
// ---------------------------------------------------------------------------

pub(crate) fn xml_escape(s: &str) -> std::borrow::Cow<'_, str> {
    if s.chars().any(|c| matches!(c, '<' | '>' | '&' | '"' | '\'')) {
        let mut out = String::with_capacity(s.len() + 8);
        for c in s.chars() {
            match c {
                '<'  => out.push_str("&lt;"),
                '>'  => out.push_str("&gt;"),
                '&'  => out.push_str("&amp;"),
                '"'  => out.push_str("&quot;"),
                '\'' => out.push_str("&apos;"),
                other => out.push(other),
            }
        }
        std::borrow::Cow::Owned(out)
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

/// Shared render-to-XML logic used by both parsers.
pub(crate) fn render_event(ev: &Event, buf: &mut String) {
    match ev {
        Event::StartDocument | Event::EndDocument => {}
        Event::StartTag { name, attributes } => {
            buf.push('<');
            buf.push_str(name);
            for attr in attributes {
                buf.push(' ');
                buf.push_str(&attr.name);
                buf.push_str("=\"");
                buf.push_str(&xml_escape(&attr.value.as_str()));
                buf.push('"');
            }
            buf.push('>');
        }
        Event::EndTag { name } => {
            buf.push_str("</");
            buf.push_str(name);
            buf.push('>');
        }
        Event::Text(t) if !t.is_empty() => buf.push_str(&xml_escape(t)),
        Event::CdataSection(t) => {
            buf.push_str("<![CDATA[");
            buf.push_str(t);
            buf.push_str("]]>");
        }
        Event::Comment(t) => {
            buf.push_str("<!--");
            buf.push_str(t);
            buf.push_str("-->");
        }
        Event::ProcessingInstruction(t) => {
            buf.push_str("<?");
            buf.push_str(t);
            buf.push_str("?>");
        }
        Event::EntityReference(t) => {
            buf.push('&');
            buf.push_str(t);
            buf.push(';');
        }
        Event::IgnorableWhitespace(t) => buf.push_str(t),
        Event::DocDecl(t) => {
            buf.push_str("<!DOCTYPE ");
            buf.push_str(t);
            buf.push('>');
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Low-level nom parsers (stateless, operate on &[u8])
// ---------------------------------------------------------------------------

fn parse_utf_string(input: &[u8]) -> IResult<&[u8], String> {
    let (input, len) = be_u16(input)?;
    let (input, bytes) = take(len)(input)?;
    let s = std::str::from_utf8(bytes)
        .map_err(|_| {
            nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Verify))
        })?
        .to_owned();
    Ok((input, s))
}

fn parse_bytes_blob(input: &[u8]) -> IResult<&[u8], Vec<u8>> {
    let (input, len) = be_u16(input)?;
    let (input, bytes) = take(len)(input)?;
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
    pool: Vec<String>,
}

impl<'a> AbxParser<'a> {
    /// Create a parser, validating the 4-byte magic header.
    pub fn new(input: &'a [u8]) -> Result<Self> {
        if input.len() < 4 {
            return Err(AbxError::UnexpectedEof("magic header"));
        }
        let magic: [u8; 4] = input[..4].try_into().unwrap();
        if magic != MAGIC {
            return Err(AbxError::InvalidMagic { expected: MAGIC, actual: magic });
        }
        Ok(AbxParser { rest: &input[4..], pool: Vec::with_capacity(32) })
    }

    /// `true` when no more bytes remain.
    pub fn is_empty(&self) -> bool { self.rest.is_empty() }

    // -- internal helpers --

    fn run<F, T>(&mut self, f: F) -> Result<T>
    where F: Fn(&'a [u8]) -> IResult<&'a [u8], T>,
    {
        let (rest, val) = f(self.rest).map_err(|e| AbxError::Nom(format!("{e:?}")))?;
        self.rest = rest;
        Ok(val)
    }

    fn read_u8(&mut self)  -> Result<u8>  { self.run(be_u8) }
    fn read_u16(&mut self) -> Result<u16> { self.run(be_u16) }
    fn read_i32(&mut self) -> Result<i32> { self.run(be_i32) }
    fn read_i64(&mut self) -> Result<i64> { self.run(be_i64) }
    fn read_f32(&mut self) -> Result<f32> { self.run(be_f32) }
    fn read_f64(&mut self) -> Result<f64> { self.run(be_f64) }

    fn read_utf(&mut self)        -> Result<String>  { self.run(parse_utf_string) }
    fn read_bytes_blob(&mut self) -> Result<Vec<u8>> { self.run(parse_bytes_blob) }

    fn read_interned(&mut self) -> Result<String> {
        let idx = self.read_u16()?;
        if idx == INTERNED_NEW {
            let s = self.read_utf()?;
            self.pool.push(s.clone());
            Ok(s)
        } else {
            self.pool.get(idx as usize).cloned()
                .ok_or(AbxError::BadInternedIndex(idx))
        }
    }

    fn read_attr_value(&mut self, type_nibble: u8) -> Result<AttributeValue> {
        match type_nibble {
            TYPE_NULL            => Ok(AttributeValue::Null),
            TYPE_STRING          => Ok(AttributeValue::String(self.read_utf()?)),
            TYPE_STRING_INTERNED => Ok(AttributeValue::String(self.read_interned()?)),
            TYPE_BYTES_HEX       => Ok(AttributeValue::BytesHex(self.read_bytes_blob()?)),
            TYPE_BYTES_BASE64    => Ok(AttributeValue::BytesBase64(self.read_bytes_blob()?)),
            TYPE_INT             => Ok(AttributeValue::Int(self.read_i32()?)),
            TYPE_INT_HEX         => Ok(AttributeValue::IntHex(self.read_i32()? as u32)),
            TYPE_LONG            => Ok(AttributeValue::Long(self.read_i64()?)),
            TYPE_LONG_HEX        => Ok(AttributeValue::LongHex(self.read_i64()? as u64)),
            TYPE_FLOAT           => Ok(AttributeValue::Float(self.read_f32()?)),
            TYPE_DOUBLE          => Ok(AttributeValue::Double(self.read_f64()?)),
            TYPE_BOOLEAN_TRUE    => Ok(AttributeValue::Boolean(true)),
            TYPE_BOOLEAN_FALSE   => Ok(AttributeValue::Boolean(false)),
            other => Err(AbxError::UnknownAttributeType(other)),
        }
    }

    // -- public event API --

    /// Pull the next [`Event`].  Returns `None` at end of input.
    pub fn next_event(&mut self) -> Result<Option<Event>> {
        if self.rest.is_empty() { return Ok(None); }

        let token      = self.read_u8()?;
        let cmd        = token & 0x0F;
        let type_nibble = token & 0xF0;

        let event = match cmd {
            CMD_START_DOCUMENT => Event::StartDocument,
            CMD_END_DOCUMENT   => return Ok(Some(Event::EndDocument)),

            CMD_START_TAG => {
                let name = self.read_interned()?;
                let mut attributes = Vec::new();
                loop {
                    if self.rest.is_empty() { break; }
                    let next = self.rest[0];
                    if (next & 0x0F) != CMD_ATTRIBUTE { break; }
                    self.rest = &self.rest[1..];
                    let attr_type = next & 0xF0;
                    let attr_name = self.read_interned()?;
                    let attr_value = self.read_attr_value(attr_type)?;
                    attributes.push(Attribute { name: attr_name, value: attr_value });
                }
                Event::StartTag { name, attributes }
            }

            CMD_END_TAG => Event::EndTag { name: self.read_interned()? },

            CMD_TEXT => Event::Text(
                if type_nibble == TYPE_STRING { self.read_utf()? } else { String::new() }
            ),
            CMD_CDSECT => Event::CdataSection(
                if type_nibble == TYPE_STRING { self.read_utf()? } else { String::new() }
            ),
            CMD_ENTITY_REF => Event::EntityReference(
                if type_nibble == TYPE_STRING { self.read_utf()? } else { String::new() }
            ),
            CMD_IGNORABLE_WHITESPACE => Event::IgnorableWhitespace(
                if type_nibble == TYPE_STRING { self.read_utf()? } else { String::new() }
            ),
            CMD_PROCESSING_INSTRUCTION => Event::ProcessingInstruction(
                if type_nibble == TYPE_STRING { self.read_utf()? } else { String::new() }
            ),
            CMD_COMMENT => Event::Comment(
                if type_nibble == TYPE_STRING { self.read_utf()? } else { String::new() }
            ),
            CMD_DOCDECL => Event::DocDecl(
                if type_nibble == TYPE_STRING { self.read_utf()? } else { String::new() }
            ),

            other => return Err(AbxError::UnknownCommand(other)),
        };

        Ok(Some(event))
    }

    // -- convenience API (mirrors AbxStreamParser) --

    /// Drain all remaining events into a `Vec`.
    pub fn collect_events(&mut self) -> Result<Vec<Event>> {
        let mut events = Vec::new();
        while let Some(ev) = self.next_event()? { events.push(ev); }
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
    pub fn find_all_attributes(&mut self, element: &str, attr: &str) -> Result<Vec<AttributeValue>> {
        let mut out = Vec::new();
        while let Some(ev) = self.next_event()? {
            if let Event::StartTag { name, attributes } = ev {
                if name == element {
                    out.extend(attributes.into_iter().filter(|a| a.name == attr).map(|a| a.value));
                }
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
            if let Event::StartTag { name, attributes } = ev {
                if name == element { out.push(attributes); }
            }
        }
        Ok(out)
    }

    /// Render the rest of the document as an XML string.
    pub fn to_xml(&mut self) -> Result<String> {
        let mut buf = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        while let Some(ev) = self.next_event()? {
            if matches!(ev, Event::EndDocument) { break; }
            render_event(&ev, &mut buf);
        }
        Ok(buf)
    }

    /// Find the next `<element>`, deserialize its attributes (and direct
    /// text content, via a `#[serde(rename = "$text")]` field) into `T`,
    /// then skip past its matching end tag. `Ok(None)` at end of document.
    #[cfg(feature = "serialize")]
    pub fn deserialize_next<T: serde::de::DeserializeOwned>(&mut self, element: &str) -> Result<Option<T>> {
        crate::de::find_and_consume_element(self, element)
    }

    /// Deserialize every remaining `<element>` into a `Vec<T>`.
    #[cfg(feature = "serialize")]
    pub fn deserialize_all<T: serde::de::DeserializeOwned>(&mut self, element: &str) -> Result<Vec<T>> {
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
                let entry = map.entry(name).or_default();
                let mut attrs = HashMap::new();
                for attr in attributes {
                    attrs.insert(attr.name, attr.value.as_str().into_owned());
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
            return Err(AbxError::InvalidMagic { expected: MAGIC, actual: magic });
        }
        Ok(Self { data })
    }

    /// Create a fresh [`AbxParser`] borrowing from the stored bytes.
    pub fn parser(&self) -> Result<AbxParser<'_>> {
        AbxParser::new(&self.data)
    }
}

// ---------------------------------------------------------------------------
// Stream parser module
// ---------------------------------------------------------------------------

pub mod stream;
pub use stream::AbxStreamParser;

#[cfg(feature = "serialize")]
mod de;
#[cfg(feature = "serialize")]
pub use de::{from_element, from_file, from_reader, from_slice};

// ---------------------------------------------------------------------------
// Convenience top-level functions
// ---------------------------------------------------------------------------

/// Convert ABX bytes to an XML string.
pub fn abx_to_xml(data: &[u8]) -> Result<String> {
    AbxParser::new(data)?.to_xml()
}

/// Parse ABX bytes and return all events.
pub fn abx_events(data: &[u8]) -> Result<Vec<Event>> {
    AbxParser::new(data)?.collect_events()
}

/// Open a file and return a buffered [`AbxStreamParser`] over it.
pub fn open_file(
    path: impl AsRef<std::path::Path>,
) -> Result<AbxStreamParser<std::io::BufReader<std::fs::File>>> {
    let f = std::fs::File::open(path)?;
    AbxStreamParser::new(std::io::BufReader::new(f))
}
