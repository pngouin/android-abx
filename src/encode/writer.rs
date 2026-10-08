use std::collections::HashMap;
use std::io::Write;

use crate::{AbxError, Attribute, AttributeValue, Event, InternedStr, MAGIC, Result};
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

// Linear scan below this many names, `HashMap` above.
const LINEAR_SCAN_LIMIT: usize = 32;

const MAX_UNSIGNED_SHORT: usize = 65_535;

/// Interns tag/attribute names only, never values (AOSP `attribute()`).
struct InternedPool {
    names: Vec<InternedStr>,
    index: Option<HashMap<InternedStr, u16>>,
}

impl InternedPool {
    fn new() -> Self {
        InternedPool {
            names: Vec::new(),
            index: None,
        }
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

            // Past 0xFFFE entries, stop caching (AOSP `writeInternedUTF`).
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

fn write_bytes_blob(out: &mut impl Write, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_UNSIGNED_SHORT {
        return Err(AbxError::ValueTooLong {
            len: bytes.len(),
            max: MAX_UNSIGNED_SHORT,
        });
    }
    out.write_all(&(bytes.len() as u16).to_be_bytes())?;
    out.write_all(bytes)?;
    Ok(())
}

fn write_utf(out: &mut impl Write, s: &str) -> Result<()> {
    write_bytes_blob(out, s.as_bytes())
}

/// Encodes [`Event`]s as ABX to any [`Write`] sink.
///
/// Tag and attribute names are interned, attribute values are not. Events are
/// written as given: the writer does not check that tags are balanced. Writes
/// are small and frequent, so wrap files and sockets in a
/// [`BufWriter`](std::io::BufWriter).
///
/// # Examples
///
/// ```
/// use android_abx::{AbxParser, AbxWriter, Attribute, AttributeValue, Event};
///
/// let mut writer = AbxWriter::new(Vec::new())?;
/// writer.write_event(&Event::StartDocument)?;
/// writer.write_event(&Event::StartTag {
///     name: "pkg".into(),
///     attributes: vec![Attribute {
///         name: "version".into(),
///         value: AttributeValue::Int(3),
///     }],
/// })?;
/// writer.write_event(&Event::EndTag { name: "pkg".into() })?;
/// writer.write_event(&Event::EndDocument)?;
/// let data = writer.into_inner();
///
/// let xml = AbxParser::new(&data)?.to_xml()?;
/// assert!(xml.ends_with(r#"<pkg version="3"></pkg>"#));
/// # Ok::<(), android_abx::AbxError>(())
/// ```
pub struct AbxWriter<W: Write> {
    writer: W,
    pool: InternedPool,
}

impl<W: Write> AbxWriter<W> {
    /// Creates a writer and writes the header.
    ///
    /// # Errors
    ///
    /// Returns [`AbxError::Io`] if writing fails.
    pub fn new(mut writer: W) -> Result<Self> {
        writer.write_all(&MAGIC)?;
        Ok(AbxWriter {
            writer,
            pool: InternedPool::new(),
        })
    }

    /// Encodes and writes one event.
    ///
    /// # Errors
    ///
    /// Returns [`AbxError::ValueTooLong`] if a string or byte value is longer than
    /// 65,535 bytes, or [`AbxError::Io`] if writing fails. After an error, the output
    /// is incomplete and should be discarded.
    pub fn write_event(&mut self, ev: &Event) -> Result<()> {
        match ev {
            Event::StartDocument => self.writer.write_all(&[CMD_START_DOCUMENT | TYPE_NULL])?,
            Event::EndDocument => self.writer.write_all(&[CMD_END_DOCUMENT | TYPE_NULL])?,
            Event::StartTag { name, attributes } => {
                self.writer
                    .write_all(&[TYPE_STRING_INTERNED | CMD_START_TAG])?;
                self.pool.write(&mut self.writer, name)?;
                for attr in attributes {
                    self.write_attribute(attr)?;
                }
            }
            Event::EndTag { name } => {
                self.writer
                    .write_all(&[TYPE_STRING_INTERNED | CMD_END_TAG])?;
                self.pool.write(&mut self.writer, name)?;
            }
            Event::Text(s) => self.write_text_token(CMD_TEXT, s)?,
            Event::CdataSection(s) => self.write_text_token(CMD_CDSECT, s)?,
            Event::Comment(s) => self.write_text_token(CMD_COMMENT, s)?,
            Event::ProcessingInstruction(s) => {
                self.write_text_token(CMD_PROCESSING_INSTRUCTION, s)?
            }
            Event::EntityReference(s) => self.write_text_token(CMD_ENTITY_REF, s)?,
            Event::IgnorableWhitespace(s) => self.write_text_token(CMD_IGNORABLE_WHITESPACE, s)?,
            Event::DocDecl(s) => self.write_text_token(CMD_DOCDECL, s)?,
        }
        Ok(())
    }

    // Always TYPE_STRING: AOSP's parser misreads TYPE_NULL text tokens.
    fn write_text_token(&mut self, cmd: u8, s: &str) -> Result<()> {
        self.writer.write_all(&[TYPE_STRING | cmd])?;
        write_utf(&mut self.writer, s)?;
        Ok(())
    }

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

    /// Returns the underlying writer, without flushing it.
    pub fn into_inner(self) -> W {
        self.writer
    }
}
