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

pub(crate) fn check_magic(input: &[u8]) -> Result<()> {
    let Some(&magic) = input.first_chunk::<4>() else {
        return Err(AbxError::UnexpectedEof("magic header"));
    };
    if magic != MAGIC {
        return Err(AbxError::InvalidMagic {
            expected: MAGIC,
            actual: magic,
        });
    }
    Ok(())
}

/// A pull parser over an ABX document in memory.
///
/// Call [`next_event`](Self::next_event) until it returns `None`, or use a helper
/// such as [`to_xml`](Self::to_xml) or [`find_attribute`](Self::find_attribute).
/// Helpers consume events, so each call continues where the previous one
/// stopped.
///
/// To read from a file or another [`Read`](std::io::Read) source, use
/// [`AbxStreamParser`](crate::AbxStreamParser), which has the same methods.
///
/// # Examples
///
/// ```
/// use android_abx::{AbxParser, Event};
///
/// # let data = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_pkg.abx"));
/// let mut parser = AbxParser::new(data)?;
/// assert_eq!(parser.next_event()?, Some(Event::StartDocument));
/// assert!(matches!(parser.next_event()?, Some(Event::StartTag { name, .. }) if name == "pkg"));
/// # Ok::<(), android_abx::AbxError>(())
/// ```
#[derive(Debug)]
pub struct AbxParser<'a> {
    rest: &'a [u8],
    pool: Vec<InternedStr>,
}

impl<'a> AbxParser<'a> {
    /// Creates a parser over `input` and checks its header.
    ///
    /// # Errors
    ///
    /// Returns [`AbxError::UnexpectedEof`] if `input` is shorter than 4 bytes, or
    /// [`AbxError::InvalidMagic`] if it does not start with [`MAGIC`].
    pub fn new(input: &'a [u8]) -> Result<Self> {
        check_magic(input)?;
        Ok(AbxParser {
            rest: &input[4..],
            pool: Vec::with_capacity(32),
        })
    }

    /// Returns `true` if all the input has been read.
    pub fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }

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
            TYPE_STRING_INTERNED => Ok(AttributeValue::String(String::from(self.read_interned()?))),
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

    /// Reads the next event, or returns `None` at the end of the input.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is truncated or malformed (unknown token,
    /// invalid interned-string index, invalid UTF-8).
    /// The parser's position after an error is unspecified.
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
                let mut attributes = Vec::with_capacity(4);
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

    /// Reads all remaining events into a `Vec`.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is truncated or malformed (unknown token,
    /// invalid interned-string index, invalid UTF-8).
    pub fn collect_events(&mut self) -> Result<Vec<Event>> {
        let mut events = Vec::new();
        while let Some(ev) = self.next_event()? {
            events.push(ev);
        }
        Ok(events)
    }

    /// Returns the value of attribute `attr` on the next `<element>` tag that has it.
    ///
    /// Events are consumed up to and including the matching tag. Returns `Ok(None)`
    /// if the document ends first.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is truncated or malformed (unknown token,
    /// invalid interned-string index, invalid UTF-8).
    ///
    /// # Examples
    ///
    /// ```
    /// use android_abx::{AbxParser, AttributeValue};
    ///
    /// # let data = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_pkg.abx"));
    /// let mut parser = AbxParser::new(data)?;
    /// let name = parser.find_attribute("pkg", "name")?;
    /// assert_eq!(name, Some(AttributeValue::String("com.example.chat".into())));
    /// # Ok::<(), android_abx::AbxError>(())
    /// ```
    pub fn find_attribute(&mut self, element: &str, attr: &str) -> Result<Option<AttributeValue>> {
        loop {
            match self.next_event()? {
                Some(Event::StartTag { name, attributes }) if name == element => {
                    if let Some(a) = attributes.into_iter().find(|a| a.name == attr) {
                        return Ok(Some(a.value));
                    }
                }
                Some(Event::EndDocument) | None => return Ok(None),
                _ => {}
            }
        }
    }

    /// Returns the value of attribute `attr` on every remaining `<element>` tag.
    ///
    /// Tags without `attr` are skipped. Consumes the rest of the document.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is truncated or malformed (unknown token,
    /// invalid interned-string index, invalid UTF-8).
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

    /// Returns the attributes of the next `<element>` tag.
    ///
    /// Events are consumed up to and including the matching tag. Returns `Ok(None)`
    /// if the document ends first.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is truncated or malformed (unknown token,
    /// invalid interned-string index, invalid UTF-8).
    pub fn attributes_of(&mut self, element: &str) -> Result<Option<Vec<Attribute>>> {
        loop {
            match self.next_event()? {
                Some(Event::StartTag { name, attributes }) if name == element => {
                    return Ok(Some(attributes));
                }
                Some(Event::EndDocument) | None => return Ok(None),
                _ => {}
            }
        }
    }

    /// Returns the attributes of every remaining `<element>` tag.
    ///
    /// Consumes the rest of the document.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is truncated or malformed (unknown token,
    /// invalid interned-string index, invalid UTF-8).
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

    /// Renders the remaining events as an XML string.
    ///
    /// The output starts with an `<?xml ...?>` declaration. Text and attribute values
    /// are escaped; empty elements are written as an opening and a closing tag.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is truncated or malformed (unknown token,
    /// invalid interned-string index, invalid UTF-8).
    ///
    /// # Examples
    ///
    /// ```
    /// use android_abx::AbxParser;
    ///
    /// # let data = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_pkg.abx"));
    /// let xml = AbxParser::new(data)?.to_xml()?;
    /// assert!(xml.ends_with(r#"<pkg name="com.example.chat" version="3" flags="1"></pkg>"#));
    /// # Ok::<(), android_abx::AbxError>(())
    /// ```
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

    /// Writes the remaining events as XML to `writer`.
    ///
    /// Same output as [`to_xml`](Self::to_xml), without building the whole string
    /// in memory.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is truncated or malformed (unknown token,
    /// invalid interned-string index, invalid UTF-8).
    /// Also returns [`AbxError::Io`](crate::AbxError::Io) if reading or writing fails.
    pub fn write_xml(&mut self, writer: &mut impl std::io::Write) -> Result<()> {
        writer.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>")?;
        let mut tmp = String::new();
        while let Some(ev) = self.next_event()? {
            if matches!(ev, Event::EndDocument) {
                break;
            }
            tmp.clear();
            render_event(&ev, &mut tmp);
            writer.write_all(tmp.as_bytes())?;
        }
        Ok(())
    }

    /// Deserializes the next `<element>` into `T`.
    ///
    /// Events are consumed up to and including the element's closing tag. Returns
    /// `Ok(None)` if the document ends first. See
    /// [Deserializing with serde](crate#deserializing-with-serde) for how fields are
    /// matched.
    ///
    /// # Errors
    ///
    /// Returns a parse error if the input is malformed, or
    /// [`AbxError::Deserialization`](crate::AbxError::Deserialization) if the element
    /// does not match `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// use android_abx::AbxParser;
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Permission {
    ///     name: String,
    /// }
    ///
    /// #[derive(Deserialize)]
    /// struct Pkg {
    ///     name: String,
    ///     description: String,
    ///     permission: Vec<Permission>,
    /// }
    ///
    /// # let data = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nested_permissions.abx"));
    /// let mut parser = AbxParser::new(data)?;
    /// let pkg: Pkg = parser.deserialize_next("pkg")?.unwrap();
    /// assert_eq!(pkg.name, "com.example.chat");
    /// assert_eq!(pkg.description, "A chat app");
    /// assert_eq!(pkg.permission.len(), 2);
    /// # Ok::<(), android_abx::AbxError>(())
    /// ```
    #[cfg(feature = "serialize")]
    pub fn deserialize_next<T: serde::de::DeserializeOwned>(
        &mut self,
        element: &str,
    ) -> Result<Option<T>> {
        crate::de::find_and_consume_element(self, element)
    }

    /// Deserializes every remaining `<element>` into a `Vec<T>`.
    ///
    /// # Errors
    ///
    /// Same as [`deserialize_next`](Self::deserialize_next).
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

    /// Collects the attributes of every remaining tag, grouped by tag name.
    ///
    /// Values are rendered with [`AttributeValue::as_str`](crate::AttributeValue::as_str).
    /// Text and nesting are discarded.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is truncated or malformed (unknown token,
    /// invalid interned-string index, invalid UTF-8).
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

/// An ABX document that owns its bytes.
///
/// Useful to keep a document in a struct without a lifetime. Call
/// [`parser`](Self::parser) to read it.
///
/// # Examples
///
/// ```
/// use android_abx::AbxParserOwned;
///
/// # let data = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_pkg.abx"));
/// let doc = AbxParserOwned::new(data.to_vec())?;
/// let xml = doc.parser()?.to_xml()?;
/// assert!(xml.contains("<pkg "));
/// # Ok::<(), android_abx::AbxError>(())
/// ```
#[derive(Debug)]
pub struct AbxParserOwned {
    data: Vec<u8>,
}

impl AbxParserOwned {
    /// Takes ownership of `data` and checks its header.
    ///
    /// # Errors
    ///
    /// Returns [`AbxError::UnexpectedEof`] if `data` is shorter than 4 bytes, or
    /// [`AbxError::InvalidMagic`] if it does not start with [`MAGIC`].
    pub fn new(data: Vec<u8>) -> Result<Self> {
        check_magic(&data)?;
        Ok(Self { data })
    }

    /// Returns a parser positioned at the start of the document.
    ///
    /// # Errors
    ///
    /// Never fails in practice: the header was already checked by
    /// [`new`](Self::new).
    pub fn parser(&self) -> Result<AbxParser<'_>> {
        AbxParser::new(&self.data)
    }
}
