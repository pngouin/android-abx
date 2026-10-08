//! Streaming parser over any [`Read`] source.

use std::io::Read;

use winnow::{
    Partial,
    error::{ErrMode, Needed},
    stream::{Offset, StreamIsPartial},
};

use super::grammar;
use crate::{Attribute, AttributeValue, Event, Result, render_event};

use std::collections::HashMap;

const INITIAL_BUF: usize = 4096;
const READ_CHUNK: usize = 4096;

/// A pull parser that reads an ABX document from any [`Read`] source.
///
/// The input is read in 4 KiB chunks into an internal buffer, which only grows
/// to fit a single value larger than that. Same methods as
/// [`AbxParser`](crate::AbxParser); it also implements [`Iterator`], yielding
/// `Result<Event>`.
///
/// # Examples
///
/// ```
/// use android_abx::{AbxStreamParser, Event};
///
/// # let data = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_pkg.abx"));
/// let parser = AbxStreamParser::new(&data[..])?;
/// let tags = parser
///     .filter(|ev| matches!(ev, Ok(Event::StartTag { .. })))
///     .count();
/// assert_eq!(tags, 1);
/// # Ok::<(), android_abx::AbxError>(())
/// ```
#[derive(Debug)]
pub struct AbxStreamParser<R: Read> {
    reader: R,
    buf: Vec<u8>,
    pos: usize,
    len: usize,
    eof: bool,
    pool: Vec<crate::InternedStr>,
}

impl<R: Read> AbxStreamParser<R> {
    /// Creates a parser and reads the header from `reader`.
    ///
    /// # Errors
    ///
    /// Returns [`AbxError::Io`](crate::AbxError::Io) if reading fails, [`AbxError::UnexpectedEof`](crate::AbxError::UnexpectedEof) if
    /// the input is shorter than 4 bytes, or [`AbxError::InvalidMagic`](crate::AbxError::InvalidMagic) if it does
    /// not start with [`MAGIC`](crate::MAGIC).
    pub fn new(reader: R) -> Result<Self> {
        let mut p = AbxStreamParser {
            reader,
            buf: vec![0u8; INITIAL_BUF],
            pos: 0,
            len: 0,
            eof: false,
            pool: Vec::with_capacity(32),
        };

        p.ensure(4)?;
        crate::decode::check_magic(&p.buf[p.pos..p.len])?;
        p.pos += 4;
        Ok(p)
    }

    #[inline]
    fn available(&self) -> usize {
        self.len - self.pos
    }

    /// Compact, then read until `needed` bytes are available or EOF.
    fn ensure(&mut self, needed: usize) -> Result<()> {
        // Hot path: skip the compaction memmove when no refill is needed.
        if self.available() >= needed || self.eof {
            return Ok(());
        }

        if self.pos > 0 {
            self.buf.copy_within(self.pos..self.len, 0);
            self.len -= self.pos;
            self.pos = 0;
        }

        while self.available() < needed && !self.eof {
            let spare = self.buf.len() - self.len;
            if spare < READ_CHUNK {
                self.buf
                    .resize(self.len + READ_CHUNK.max(needed - self.available()), 0);
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

    /// Reads the next event, or returns `None` at the end of the input.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is truncated or malformed (unknown token,
    /// invalid interned-string index, invalid UTF-8).
    /// Also returns [`AbxError::Io`](crate::AbxError::Io) if reading fails.
    /// On error, the parser is left at the start of the failing event.
    pub fn next_event(&mut self) -> Result<Option<Event>> {
        self.ensure(1)?;
        if self.available() == 0 {
            return Ok(None);
        }
        let pool_len = self.pool.len();
        let result = self.parse_event();
        if result.is_err() {
            self.pool.truncate(pool_len);
        }
        result.map(Some)
    }

    /// Parses one event from the buffer, refilling and retrying from its start on `Incomplete`.
    fn parse_event(&mut self) -> Result<Event> {
        loop {
            let window = &self.buf[self.pos..self.len];
            let mut input = Partial::new(window);
            if self.eof {
                let _ = input.complete();
            }
            let pool_len = self.pool.len();
            match grammar::event(&mut input, &mut self.pool) {
                Ok(ev) => {
                    self.pos += input.offset_from(&Partial::new(window));
                    return Ok(ev);
                }
                Err(ErrMode::Incomplete(needed)) => {
                    self.pool.truncate(pool_len);
                    let more = match needed {
                        Needed::Size(n) => n.get(),
                        Needed::Unknown => 1,
                    };
                    self.ensure(self.available() + more)?;
                }
                Err(e) => return Err(grammar::into_abx_error(e)),
            }
        }
    }

    /// Reads all remaining events into a `Vec`.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is truncated or malformed (unknown token,
    /// invalid interned-string index, invalid UTF-8).
    /// Also returns [`AbxError::Io`](crate::AbxError::Io) if reading fails.
    pub fn collect_events(&mut self) -> Result<Vec<Event>> {
        let mut out = Vec::new();
        while let Some(ev) = self.next_event()? {
            out.push(ev);
        }
        Ok(out)
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
    /// Also returns [`AbxError::Io`](crate::AbxError::Io) if reading fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use android_abx::{AbxStreamParser, AttributeValue};
    ///
    /// # let data = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_pkg.abx"));
    /// let mut parser = AbxStreamParser::new(&data[..])?;
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
    /// Also returns [`AbxError::Io`](crate::AbxError::Io) if reading fails.
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
    /// Also returns [`AbxError::Io`](crate::AbxError::Io) if reading fails.
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
    /// Also returns [`AbxError::Io`](crate::AbxError::Io) if reading fails.
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
    /// [`AbxError::Deserialization`](crate::AbxError::Deserialization)(crate::AbxError::Deserialization) if the element
    /// does not match `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// use android_abx::AbxStreamParser;
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
    /// let mut parser = AbxStreamParser::new(&data[..])?;
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

    /// Returns an iterator that deserializes each remaining `<element>` into `T`.
    ///
    /// Elements are read one at a time, so memory use does not grow with the
    /// document.
    ///
    /// # Examples
    ///
    /// ```
    /// use android_abx::AbxStreamParser;
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Permission {
    ///     name: String,
    /// }
    ///
    /// # let data = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nested_permissions.abx"));
    /// let mut parser = AbxStreamParser::new(&data[..])?;
    /// let names = parser
    ///     .deserialize_iter::<Permission>("permission")
    ///     .map(|p| p.map(|p| p.name))
    ///     .collect::<Result<Vec<_>, _>>()?;
    /// assert_eq!(names, ["INTERNET", "CAMERA"]);
    /// # Ok::<(), android_abx::AbxError>(())
    /// ```
    #[cfg(feature = "serialize")]
    pub fn deserialize_iter<'p, T: serde::de::DeserializeOwned>(
        &'p mut self,
        element: &'p str,
    ) -> DeserializeIter<'p, R, T> {
        DeserializeIter {
            parser: self,
            element,
            _marker: std::marker::PhantomData,
        }
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
    /// Also returns [`AbxError::Io`](crate::AbxError::Io) if reading fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use android_abx::AbxStreamParser;
    ///
    /// # let data = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_pkg.abx"));
    /// let xml = AbxStreamParser::new(&data[..])?.to_xml()?;
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
    /// Also returns [`AbxError::Io`](crate::AbxError::Io)(crate::AbxError::Io) if reading or writing fails.
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

    /// Collects the attributes of every remaining tag, grouped by tag name.
    ///
    /// Values are rendered with [`AttributeValue::as_str`](crate::AttributeValue::as_str).
    /// Text and nesting are discarded.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is truncated or malformed (unknown token,
    /// invalid interned-string index, invalid UTF-8).
    /// Also returns [`AbxError::Io`](crate::AbxError::Io) if reading fails.
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

    /// Returns the underlying reader. Bytes already buffered but not parsed are lost.
    pub fn into_inner(self) -> R {
        self.reader
    }
}

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

/// Iterator returned by [`AbxStreamParser::deserialize_iter`].
///
/// Yields `Result<T>` for each matching element.
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
