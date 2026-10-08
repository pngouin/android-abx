//! Parser and encoder for Android Binary XML (ABX).
//!
//! ABX is the binary XML format written by AOSP's `BinaryXmlSerializer` and read
//! by `BinaryXmlPullParser`. Android uses it for platform files such as
//! `/data/system/packages.xml`. It is not AXML, the format of compiled APK
//! resources such as `AndroidManifest.xml`, which this crate cannot read.
//!
//! # Parsing
//!
//! [`AbxParser`] reads a document from a byte slice and [`AbxStreamParser`] from
//! any [`Read`](std::io::Read). Both have the same methods and yield the same
//! [`Event`]s.
//!
//! ```
//! use android_abx::{AbxParser, Event};
//!
//! # let data = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_pkg.abx"));
//! let mut parser = AbxParser::new(data)?;
//! while let Some(event) = parser.next_event()? {
//!     if let Event::StartTag { name, attributes } = event {
//!         println!("<{name}> has {} attributes", attributes.len());
//!     }
//! }
//! # Ok::<(), android_abx::AbxError>(())
//! ```
//!
//! To convert a whole document to XML text, use [`abx_to_xml`]:
//!
//! ```
//! # let data = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_pkg.abx"));
//! let xml = android_abx::abx_to_xml(data)?;
//! assert_eq!(
//!     xml,
//!     r#"<?xml version="1.0" encoding="UTF-8"?><pkg name="com.example.chat" version="3" flags="1"></pkg>"#,
//! );
//! # Ok::<(), android_abx::AbxError>(())
//! ```
//!
//! # Encoding
//!
//! [`AbxWriter`] and [`events_to_abx`] encode [`Event`]s back to ABX. With the
//! `xml` feature, `xml_to_abx` encodes XML text.
//!
//! ```
//! use android_abx::{AbxParser, events_to_abx};
//!
//! # let data = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_pkg.abx"));
//! let events = AbxParser::new(data)?.collect_events()?;
//! assert_eq!(events_to_abx(&events)?, data);
//! # Ok::<(), android_abx::AbxError>(())
//! ```
//!
//! # Deserializing with serde
//!
//! With the `serialize` feature, an element can be deserialized into any type
//! that implements `serde::Deserialize`. Struct fields are filled from:
//!
//! - attributes, by name. Use `#[serde(rename = "...")]` for names that are not
//!   Rust identifiers;
//! - child elements, by tag name. A `Vec<T>` field takes every matching child,
//!   any other field takes the first one. A child with only text can fill a
//!   scalar field such as `String` or `u32`;
//! - the element's text, through a field renamed to `"$text"`. Only text events
//!   are collected: entity references such as `&amp;` are dropped.
//!
//! When an attribute and a child element have the same name, the attribute is
//! used. An `Option` field is `None` when the attribute or child is missing, or
//! when the attribute has a null value. Enums with unit variants are matched by
//! name against a string value.
//!
//! Use `from_slice`, `from_reader` or `from_file` when the root element is the
//! record you want. For repeated elements under a root, such as `<pkg>` entries
//! in `packages.xml`, use `AbxParser::deserialize_all` or
//! `AbxStreamParser::deserialize_iter`.
//!
//! # Feature flags
//!
//! - `serialize`: serde deserialization.
//! - `xml`: `xml_to_abx`, to encode XML text.

#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod error;
pub use error::{AbxError, Result};

mod wire;
pub use wire::MAGIC;
pub(crate) use wire::{
    CMD_ATTRIBUTE, CMD_CDSECT, CMD_COMMENT, CMD_DOCDECL, CMD_END_DOCUMENT, CMD_END_TAG,
    CMD_ENTITY_REF, CMD_IGNORABLE_WHITESPACE, CMD_PROCESSING_INSTRUCTION, CMD_START_DOCUMENT,
    CMD_START_TAG, CMD_TEXT, INTERNED_NEW, TYPE_BOOLEAN_FALSE, TYPE_BOOLEAN_TRUE,
    TYPE_BYTES_BASE64, TYPE_BYTES_HEX, TYPE_DOUBLE, TYPE_FLOAT, TYPE_INT, TYPE_INT_HEX, TYPE_LONG,
    TYPE_LONG_HEX, TYPE_NULL, TYPE_STRING, TYPE_STRING_INTERNED,
};

mod event;
pub(crate) use event::render_event;
pub use event::{Attribute, AttributeValue, Event, InternedStr};

mod decode;
pub use decode::stream;
pub use decode::stream::AbxStreamParser;
pub use decode::{AbxParser, AbxParserOwned};

mod encode;
#[cfg(feature = "xml")]
pub use encode::xml_to_abx;
pub use encode::{AbxWriter, events_to_abx};

#[cfg(feature = "serialize")]
mod de;
#[cfg(feature = "serialize")]
pub use de::{from_element, from_file, from_reader, from_slice};

/// Converts an ABX document to an XML string.
///
/// Shorthand for [`AbxParser::new`] followed by [`AbxParser::to_xml`].
///
/// # Errors
///
/// Returns an error if the input is truncated or malformed (unknown token,
/// invalid interned-string index, invalid UTF-8).
///
/// # Examples
///
/// ```
/// # let data = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_pkg.abx"));
/// let xml = android_abx::abx_to_xml(data)?;
/// assert!(xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?><pkg "#));
/// # Ok::<(), android_abx::AbxError>(())
/// ```
pub fn abx_to_xml(data: &[u8]) -> Result<String> {
    AbxParser::new(data)?.to_xml()
}

/// Parses an ABX document into a list of events.
///
/// Shorthand for [`AbxParser::new`] followed by [`AbxParser::collect_events`].
///
/// # Errors
///
/// Returns an error if the input is truncated or malformed (unknown token,
/// invalid interned-string index, invalid UTF-8).
///
/// # Examples
///
/// ```
/// use android_abx::Event;
///
/// # let data = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_pkg.abx"));
/// let events = android_abx::abx_events(data)?;
/// assert_eq!(events.first(), Some(&Event::StartDocument));
/// assert_eq!(events.last(), Some(&Event::EndDocument));
/// # Ok::<(), android_abx::AbxError>(())
/// ```
pub fn abx_events(data: &[u8]) -> Result<Vec<Event>> {
    AbxParser::new(data)?.collect_events()
}

/// Opens a file and returns an [`AbxStreamParser`] over it.
///
/// # Errors
///
/// Returns [`AbxError::Io`] if the file cannot be opened or read, and
/// [`AbxError::InvalidMagic`] or [`AbxError::UnexpectedEof`] if it does not start
/// with [`MAGIC`].
///
/// # Examples
///
/// ```no_run
/// let xml = android_abx::open_file("/data/system/packages.xml")?.to_xml()?;
/// # Ok::<(), android_abx::AbxError>(())
/// ```
pub fn open_file(
    path: impl AsRef<std::path::Path>,
) -> Result<AbxStreamParser<std::io::BufReader<std::fs::File>>> {
    let f = std::fs::File::open(path)?;
    AbxStreamParser::new(std::io::BufReader::new(f))
}
