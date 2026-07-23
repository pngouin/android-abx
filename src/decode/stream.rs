//! [`AbxStreamParser`] — pull parser over any `Read` source.
//!
//! Unlike [`crate::AbxParser`] which requires the whole document in memory,
//! `AbxStreamParser` reads from any `std::io::Read` implementor (files, TCP
//! sockets, `stdin`, in-memory `Cursor<Vec<u8>>`, …) using a small internal
//! ring buffer.
//!
//! The public surface is intentionally identical to `AbxParser` so the two
//! types are interchangeable; just swap the constructor.
//!
//! # Internal design
//!
//! We keep a `Vec<u8>` ring buffer (`buf`) and a `pos` cursor.  When a nom
//! parser reports `Incomplete` we refill from the reader, slide unconsumed
//! bytes to the front, and retry.  This gives us:
//!
//! - **bounded memory** — the buffer only grows when a single atom (e.g. a
//!   very long string) exceeds the current capacity.
//! - **zero extra copies** — nom operates directly on `&buf[pos..]`.
//! - **identical event types** — `Event`, `Attribute`, `AttributeValue` are
//!   all shared with the slice parser.

use std::io::Read;

use nom::{
    number::streaming::{be_f32, be_f64, be_i32, be_i64, be_u16, be_u8},
    Needed,
};

use crate::{
    Attribute, AttributeValue, AbxError, Event, Result, MAGIC,
    CMD_ATTRIBUTE, CMD_CDSECT, CMD_COMMENT, CMD_DOCDECL, CMD_END_DOCUMENT,
    CMD_END_TAG, CMD_ENTITY_REF, CMD_IGNORABLE_WHITESPACE,
    CMD_PROCESSING_INSTRUCTION, CMD_START_DOCUMENT, CMD_START_TAG, CMD_TEXT,
    TYPE_BOOLEAN_FALSE, TYPE_BOOLEAN_TRUE, TYPE_BYTES_BASE64, TYPE_BYTES_HEX,
    TYPE_DOUBLE, TYPE_FLOAT, TYPE_INT, TYPE_INT_HEX, TYPE_LONG, TYPE_LONG_HEX,
    TYPE_NULL, TYPE_STRING, TYPE_STRING_INTERNED,
    render_event,
};

use crate::INTERNED_NEW;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Buffer constants
// ---------------------------------------------------------------------------

/// Initial ring-buffer capacity (4 KiB).
const INITIAL_BUF: usize = 4096;
/// How many bytes to try to read per refill.
const READ_CHUNK: usize = 4096;

// ---------------------------------------------------------------------------
// AbxStreamParser
// ---------------------------------------------------------------------------

/// Pull parser that reads from any `R: Read` source.
///
/// # Example
/// ```rust,ignore
/// use abx::AbxStreamParser;
/// use std::io::BufReader;
///
/// let file = std::fs::File::open("backup.abx")?;
/// let mut p = AbxStreamParser::new(BufReader::new(file))?;
///
/// while let Some(ev) = p.next_event()? {
///     match ev {
///         abx::Event::StartTag { name, attributes } => {
///             println!("<{name}>");
///             for a in &attributes {
///                 println!("  {}={}", a.name, a.as_str());
///             }
///         }
///         abx::Event::EndTag { name } => println!("</{name}>"),
///         _ => {}
///     }
/// }
/// ```
#[derive(Debug)]
pub struct AbxStreamParser<R: Read> {
    reader: R,
    /// Internal ring buffer.
    buf: Vec<u8>,
    /// Read cursor inside `buf`.
    pos: usize,
    /// Total valid bytes in `buf` (always >= pos).
    len: usize,
    /// `true` once the underlying reader returned 0 bytes.
    eof: bool,
    /// Interned string pool.
    pool: Vec<crate::InternedStr>,
}

impl<R: Read> AbxStreamParser<R> {
    // -----------------------------------------------------------------------
    // Constructor
    // -----------------------------------------------------------------------

    /// Create a new parser from any reader.
    ///
    /// Reads and validates the 4-byte magic header immediately.  Returns an
    /// error if the reader is too short or the header does not match.
    pub fn new(reader: R) -> Result<Self> {
        let mut p = AbxStreamParser {
            reader,
            buf: vec![0u8; INITIAL_BUF],
            pos: 0,
            len: 0,
            eof: false,
            pool: Vec::with_capacity(32),
        };

        // Read at least 4 bytes for the magic header.
        p.ensure(4)?;

        let magic: [u8; 4] = p.buf[p.pos..p.pos + 4].try_into().unwrap();
        if magic != MAGIC {
            return Err(AbxError::InvalidMagic { expected: MAGIC, actual: magic });
        }
        p.pos += 4;
        Ok(p)
    }

    // -----------------------------------------------------------------------
    // Buffer management
    // -----------------------------------------------------------------------

    /// Number of unconsumed bytes currently in the buffer.
    #[inline]
    fn available(&self) -> usize {
        self.len - self.pos
    }

    /// Compact the buffer (slide unconsumed bytes to front) then read from the
    /// underlying reader until we have at least `needed` bytes available, or
    /// until EOF.
    fn ensure(&mut self, needed: usize) -> Result<()> {
        // Compact first so we always have room at the back.
        if self.pos > 0 {
            self.buf.copy_within(self.pos..self.len, 0);
            self.len -= self.pos;
            self.pos = 0;
        }

        while self.available() < needed && !self.eof {
            // Grow if necessary.
            let spare = self.buf.len() - self.len;
            if spare < READ_CHUNK {
                self.buf.resize(self.len + READ_CHUNK.max(needed - self.available()), 0);
            }

            let n = self.reader.read(&mut self.buf[self.len..])?;
            if n == 0 {
                self.eof = true;
            } else {
                self.len += n;
            }
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Primitive readers (nom-based, with auto-refill on Incomplete)
    // -----------------------------------------------------------------------

    /// Run a nom parser against the unconsumed tail of the buffer, refilling
    /// if necessary.  Returns the parsed value and advances `pos`.
    fn parse<F, T>(&mut self, mut f: F) -> Result<T>
    where
        F: FnMut(&[u8]) -> nom::IResult<&[u8], T>,
    {
        loop {
            match f(&self.buf[self.pos..self.len]) {
                Ok((rest, val)) => {
                    self.pos = self.len - rest.len();
                    return Ok(val);
                }
                Err(nom::Err::Incomplete(Needed::Size(n))) => {
                    let need = self.available() + n.get();
                    self.ensure(need)?;
                    if self.eof && self.available() < n.get() {
                        return Err(AbxError::UnexpectedEof("primitive"));
                    }
                }
                Err(nom::Err::Incomplete(Needed::Unknown)) => {
                    // Should not happen with our complete:: parsers, but handle gracefully.
                    self.ensure(self.available() + 1)?;
                    if self.eof {
                        return Err(AbxError::UnexpectedEof("primitive"));
                    }
                }
                Err(e) => return Err(AbxError::Nom(format!("{e:?}"))),
            }
        }
    }

    fn read_u8(&mut self)  -> Result<u8>  { self.parse(|i| be_u8(i)) }
    fn read_u16(&mut self) -> Result<u16> { self.parse(|i| be_u16(i)) }
    fn read_i32(&mut self) -> Result<i32> { self.parse(|i| be_i32(i)) }
    fn read_i64(&mut self) -> Result<i64> { self.parse(|i| be_i64(i)) }
    fn read_f32(&mut self) -> Result<f32> { self.parse(|i| be_f32(i)) }
    fn read_f64(&mut self) -> Result<f64> { self.parse(|i| be_f64(i)) }

    /// Read a `u16`-length-prefixed UTF-8 blob.
    fn read_utf(&mut self) -> Result<String> {
        let len = self.read_u16()? as usize;
        // Make sure the whole string payload is buffered.
        self.ensure(len)?;
        if self.available() < len {
            return Err(AbxError::UnexpectedEof("UTF string payload"));
        }
        let s = std::str::from_utf8(&self.buf[self.pos..self.pos + len])
            .map_err(|_| AbxError::InvalidUtf8)?
            .to_owned();
        self.pos += len;
        Ok(s)
    }

    /// Read a `u16`-length-prefixed raw byte blob.
    fn read_bytes_blob(&mut self) -> Result<Vec<u8>> {
        let len = self.read_u16()? as usize;
        self.ensure(len)?;
        if self.available() < len {
            return Err(AbxError::UnexpectedEof("bytes payload"));
        }
        let v = self.buf[self.pos..self.pos + len].to_vec();
        self.pos += len;
        Ok(v)
    }

    /// Read an interned string. Every occurrence after the first is a
    /// back-reference into `pool`, resolved with `InternedStr::clone` (a
    /// refcount bump) rather than a fresh allocation and copy.
    fn read_interned(&mut self) -> Result<crate::InternedStr> {
        let idx = self.read_u16()?;
        if idx == INTERNED_NEW {
            let s: crate::InternedStr = self.read_utf()?.into();
            self.pool.push(s.clone());
            Ok(s)
        } else {
            self.pool.get(idx as usize).cloned()
                .ok_or(AbxError::BadInternedIndex(idx))
        }
    }

    // -----------------------------------------------------------------------
    // Attribute value
    // -----------------------------------------------------------------------

    fn read_attr_value(&mut self, type_nibble: u8) -> Result<AttributeValue> {
        match type_nibble {
            TYPE_NULL            => Ok(AttributeValue::Null),
            TYPE_STRING          => Ok(AttributeValue::String(self.read_utf()?)),
            TYPE_STRING_INTERNED => Ok(AttributeValue::String(self.read_interned()?.to_string())),
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

    // -----------------------------------------------------------------------
    // Peek helpers (non-consuming, with refill)
    // -----------------------------------------------------------------------

    /// Peek at the next byte without consuming it. Returns `None` on EOF.
    fn peek_u8(&mut self) -> Result<Option<u8>> {
        self.ensure(1)?;
        Ok(self.buf.get(self.pos).copied())
    }

    // -----------------------------------------------------------------------
    // Public event API
    // -----------------------------------------------------------------------

    /// Pull the next [`Event`].  Returns `None` at end of input.
    pub fn next_event(&mut self) -> Result<Option<Event>> {
        // Refill at least 1 byte.
        self.ensure(1)?;
        if self.available() == 0 {
            return Ok(None);
        }

        let token       = self.read_u8()?;
        let cmd         = token & 0x0F;
        let type_nibble  = token & 0xF0;

        let event = match cmd {
            CMD_START_DOCUMENT => Event::StartDocument,
            CMD_END_DOCUMENT   => return Ok(Some(Event::EndDocument)),

            CMD_START_TAG => {
                let name = self.read_interned()?;
                let mut attributes = Vec::new();

                // Eagerly consume following ATTRIBUTE tokens without peeking
                // across I/O boundaries more than necessary.
                loop {
                    match self.peek_u8()? {
                        Some(next) if (next & 0x0F) == CMD_ATTRIBUTE => {
                            self.pos += 1; // consume peeked byte
                            let attr_type  = next & 0xF0;
                            let attr_name  = self.read_interned()?;
                            let attr_value = self.read_attr_value(attr_type)?;
                            attributes.push(Attribute { name: attr_name, value: attr_value });
                        }
                        _ => break,
                    }
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

    // -----------------------------------------------------------------------
    // Convenience API  (same surface as AbxParser)
    // -----------------------------------------------------------------------

    /// Drain all remaining events into a `Vec`.
    pub fn collect_events(&mut self) -> Result<Vec<Event>> {
        let mut out = Vec::new();
        while let Some(ev) = self.next_event()? { out.push(ev); }
        Ok(out)
    }

    /// Return the value of the first matching `attr` inside any `<element>` tag.
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

    /// All values of `attr` found in `<element>` tags.
    pub fn find_all_attributes(&mut self, element: &str, attr: &str) -> Result<Vec<AttributeValue>> {
        let mut out = Vec::new();
        while let Some(ev) = self.next_event()? {
            if let Event::StartTag { name, attributes } = ev
                && name == element
            {
                out.extend(attributes.into_iter().filter(|a| a.name == attr).map(|a| a.value));
            }
        }
        Ok(out)
    }

    /// Attributes of the first `<element>` tag.
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

    /// Attributes of every `<element>` tag.
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

    /// Lazily deserialize every remaining `<element>` as a `T`, one at a
    /// time, without buffering the whole document or the whole result set —
    /// the streaming counterpart to [`deserialize_all`](Self::deserialize_all).
    #[cfg(feature = "serialize")]
    pub fn deserialize_iter<'p, T: serde::de::DeserializeOwned>(
        &'p mut self,
        element: &'p str,
    ) -> DeserializeIter<'p, R, T> {
        DeserializeIter { parser: self, element, _marker: std::marker::PhantomData }
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

    /// Write the rest of the document as XML into any `std::io::Write` sink.
    ///
    /// More memory-efficient than [`to_xml`](AbxStreamParser::to_xml) for very
    /// large files because it does not accumulate the whole result in a `String`.
    pub fn write_xml(&mut self, writer: &mut impl std::io::Write) -> Result<()> {
        writer.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>")?;
        // One scratch buffer reused (cleared, not reallocated) across every
        // event, instead of a fresh allocation per event.
        let mut tmp = String::new();
        while let Some(ev) = self.next_event()? {
            if matches!(ev, Event::EndDocument) { break; }
            tmp.clear();
            render_event(&ev, &mut tmp);
            writer.write_all(tmp.as_bytes())?;
        }
        Ok(())
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

    /// Unwrap the underlying reader, discarding any buffered data.
    pub fn into_inner(self) -> R {
        self.reader
    }
}

// ---------------------------------------------------------------------------
// Iterator impl  — lets you use `for ev in parser { … }`
// ---------------------------------------------------------------------------

impl<R: Read> Iterator for AbxStreamParser<R> {
    type Item = Result<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_event() {
            Ok(Some(ev)) => Some(Ok(ev)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

// ---------------------------------------------------------------------------
// DeserializeIter — lazy struct-per-element streaming, from deserialize_iter
// ---------------------------------------------------------------------------

/// Lazily yields each remaining `<element>`, deserialized into `T`. See
/// [`AbxStreamParser::deserialize_iter`].
#[cfg(feature = "serialize")]
pub struct DeserializeIter<'p, R: Read, T> {
    parser: &'p mut AbxStreamParser<R>,
    element: &'p str,
    _marker: std::marker::PhantomData<T>,
}

#[cfg(feature = "serialize")]
impl<'p, R: Read, T: serde::de::DeserializeOwned> Iterator for DeserializeIter<'p, R, T> {
    type Item = Result<T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.parser.deserialize_next(self.element).transpose()
    }
}
