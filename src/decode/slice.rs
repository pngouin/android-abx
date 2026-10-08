use std::collections::HashMap;

use crate::{AbxError, Attribute, AttributeValue, Event, MAGIC, Result, render_event};

use super::grammar;
use crate::InternedStr;

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

    /// Reads the next event, or returns `None` at the end of the input.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is truncated or malformed (unknown token,
    /// invalid interned-string index, invalid UTF-8).
    /// On error, the parser is left at the start of the failing event.
    pub fn next_event(&mut self) -> Result<Option<Event>> {
        if self.rest.is_empty() {
            return Ok(None);
        }
        let mut input = self.rest;
        let pool_len = self.pool.len();
        match grammar::event(&mut input, &mut self.pool) {
            Ok(ev) => {
                self.rest = input;
                Ok(Some(ev))
            }
            Err(e) => {
                self.pool.truncate(pool_len);
                Err(grammar::into_abx_error(e))
            }
        }
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
