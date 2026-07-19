//! [`AbxWriter`] — encodes [`Event`]s to the ABX wire format, the mechanical
//! reverse of [`crate::AbxParser`]/[`crate::AbxStreamParser`]'s decoding.

use std::collections::HashMap;
use std::io::Write;

use crate::{Attribute, AttributeValue, Event, InternedStr, Result, MAGIC};
use crate::{
    CMD_ATTRIBUTE, CMD_CDSECT, CMD_COMMENT, CMD_DOCDECL, CMD_END_DOCUMENT, CMD_END_TAG,
    CMD_ENTITY_REF, CMD_IGNORABLE_WHITESPACE, CMD_PROCESSING_INSTRUCTION, CMD_START_DOCUMENT,
    CMD_START_TAG, CMD_TEXT,
};
use crate::{
    INTERNED_NEW, TYPE_BOOLEAN_FALSE, TYPE_BOOLEAN_TRUE, TYPE_BYTES_BASE64, TYPE_BYTES_HEX,
    TYPE_DOUBLE, TYPE_FLOAT, TYPE_INT, TYPE_INT_HEX, TYPE_LONG, TYPE_LONG_HEX, TYPE_NULL,
    TYPE_STRING, TYPE_STRING_INTERNED,
};

/// Tracks tag/attribute names already written, so repeats become a `u16`
/// back-reference instead of a fresh string. Covers names only, never
/// attribute values — matches AOSP's `attribute()`, which never
/// auto-interns a value.
///
/// Below `LINEAR_SCAN_LIMIT` unique names, scans `names` linearly instead
/// of hashing — real documents repeat a small, bounded vocabulary of
/// names, so scanning a short `Vec` beats hash overhead. A linear scan is
/// O(n²) in the number of unique names though, so past the limit this
/// switches to a `HashMap`.
const LINEAR_SCAN_LIMIT: usize = 32;

struct InternedPool {
    names: Vec<InternedStr>,
    index: Option<HashMap<InternedStr, u16>>,
}

impl InternedPool {
    fn new() -> Self {
        InternedPool { names: Vec::new(), index: None }
    }

    fn find(&self, s: &InternedStr) -> Option<u16> {
        match &self.index {
            Some(index) => index.get(s).copied(),
            None => self.names.iter().position(|n| n == s).map(|i| i as u16),
        }
    }

    fn write(&mut self, out: &mut impl Write, s: &InternedStr) -> Result<()> {
        if let Some(idx) = self.find(s) {
            out.write_all(&idx.to_be_bytes())?;
        } else {
            out.write_all(&INTERNED_NEW.to_be_bytes())?;
            write_utf(out, s)?;

            // 0xFFFF is the INTERNED_NEW sentinel, so indices only go up
            // to 0xFFFE -- past that, stop caching. Matches real AOSP,
            // which also keeps working past its cap instead of erroring;
            // only uncached names lose the back-reference.
            if self.names.len() < INTERNED_NEW as usize {
                let idx = self.names.len() as u16;
                self.names.push(s.clone());
                if let Some(index) = &mut self.index {
                    index.insert(s.clone(), idx);
                } else if self.names.len() > LINEAR_SCAN_LIMIT {
                    self.index = Some(self.names.iter().cloned().zip(0u16..).collect());
                }
            }
        }
        Ok(())
    }
}

fn write_utf(out: &mut impl Write, s: &str) -> Result<()> {
    let bytes = s.as_bytes();
    out.write_all(&(bytes.len() as u16).to_be_bytes())?;
    out.write_all(bytes)?;
    Ok(())
}

fn write_bytes_blob(out: &mut impl Write, bytes: &[u8]) -> Result<()> {
    out.write_all(&(bytes.len() as u16).to_be_bytes())?;
    out.write_all(bytes)?;
    Ok(())
}

/// Encodes [`Event`]s to any `W: Write` sink. `Vec<u8>` covers the
/// in-memory case (it implements `Write`); a file/socket/`BufWriter` covers
/// streaming — unlike the decode side, writing has no ring-buffer/refill
/// complexity to split across two types.
pub struct AbxWriter<W: Write> {
    writer: W,
    pool: InternedPool,
}

impl<W: Write> AbxWriter<W> {
    /// Create a writer, writing the 4-byte magic header immediately.
    pub fn new(mut writer: W) -> Result<Self> {
        writer.write_all(&MAGIC)?;
        Ok(AbxWriter { writer, pool: InternedPool::new() })
    }

    /// Encode and write a single [`Event`].
    pub fn write_event(&mut self, ev: &Event) -> Result<()> {
        match ev {
            Event::StartDocument => self.writer.write_all(&[CMD_START_DOCUMENT | TYPE_NULL])?,
            Event::EndDocument => self.writer.write_all(&[CMD_END_DOCUMENT | TYPE_NULL])?,
            Event::StartTag { name, attributes } => {
                self.writer.write_all(&[TYPE_STRING_INTERNED | CMD_START_TAG])?;
                self.pool.write(&mut self.writer, name)?;
                for attr in attributes {
                    self.write_attribute(attr)?;
                }
            }
            Event::EndTag { name } => {
                self.writer.write_all(&[TYPE_STRING_INTERNED | CMD_END_TAG])?;
                self.pool.write(&mut self.writer, name)?;
            }
            Event::Text(s) => self.write_text_token(CMD_TEXT, s)?,
            Event::CdataSection(s) => self.write_text_token(CMD_CDSECT, s)?,
            Event::Comment(s) => self.write_text_token(CMD_COMMENT, s)?,
            Event::ProcessingInstruction(s) => self.write_text_token(CMD_PROCESSING_INSTRUCTION, s)?,
            Event::EntityReference(s) => self.write_text_token(CMD_ENTITY_REF, s)?,
            Event::IgnorableWhitespace(s) => self.write_text_token(CMD_IGNORABLE_WHITESPACE, s)?,
            Event::DocDecl(s) => self.write_text_token(CMD_DOCDECL, s)?,
        }
        Ok(())
    }

    /// Shared shape for the seven text-bearing events: always `TYPE_STRING`
    /// + length-prefixed UTF-8, even for an empty string. Never
    /// `TYPE_NULL` — real AOSP's own parser can't correctly read that form
    /// back (a confirmed bug: it calls `readUTF()` unconditionally here,
    /// unlike the `ATTRIBUTE` branch), so `TYPE_STRING` with an empty
    /// payload is the only safe choice.
    fn write_text_token(&mut self, cmd: u8, s: &str) -> Result<()> {
        self.writer.write_all(&[TYPE_STRING | cmd])?;
        write_utf(&mut self.writer, s)?;
        Ok(())
    }

    /// Write one attribute: `type_nibble|CMD_ATTRIBUTE` + interned name +
    /// the value's payload. `String` values are never interned — matches
    /// AOSP's `attribute()`, which only interns the name.
    fn write_attribute(&mut self, attr: &Attribute) -> Result<()> {
        let type_nibble = match &attr.value {
            AttributeValue::Null => TYPE_NULL,
            AttributeValue::String(_) => TYPE_STRING,
            AttributeValue::BytesHex(_) => TYPE_BYTES_HEX,
            AttributeValue::BytesBase64(_) => TYPE_BYTES_BASE64,
            AttributeValue::Int(_) => TYPE_INT,
            AttributeValue::IntHex(_) => TYPE_INT_HEX,
            AttributeValue::Long(_) => TYPE_LONG,
            AttributeValue::LongHex(_) => TYPE_LONG_HEX,
            AttributeValue::Float(_) => TYPE_FLOAT,
            AttributeValue::Double(_) => TYPE_DOUBLE,
            AttributeValue::Boolean(true) => TYPE_BOOLEAN_TRUE,
            AttributeValue::Boolean(false) => TYPE_BOOLEAN_FALSE,
        };
        self.writer.write_all(&[type_nibble | CMD_ATTRIBUTE])?;
        self.pool.write(&mut self.writer, &attr.name)?;
        match &attr.value {
            AttributeValue::Null | AttributeValue::Boolean(_) => {}
            AttributeValue::String(s) => write_utf(&mut self.writer, s)?,
            AttributeValue::BytesHex(b) | AttributeValue::BytesBase64(b) => {
                write_bytes_blob(&mut self.writer, b)?
            }
            AttributeValue::Int(v) => self.writer.write_all(&v.to_be_bytes())?,
            AttributeValue::IntHex(v) => self.writer.write_all(&v.to_be_bytes())?,
            AttributeValue::Long(v) => self.writer.write_all(&v.to_be_bytes())?,
            AttributeValue::LongHex(v) => self.writer.write_all(&v.to_be_bytes())?,
            AttributeValue::Float(v) => self.writer.write_all(&v.to_be_bytes())?,
            AttributeValue::Double(v) => self.writer.write_all(&v.to_be_bytes())?,
        }
        Ok(())
    }

    /// Unwrap the underlying writer.
    pub fn into_inner(self) -> W {
        self.writer
    }
}
