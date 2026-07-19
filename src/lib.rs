//! # abx — Android Binary XML parser
//!
//! Parses the ABX (Android Binary XML) format produced by `BinaryXmlSerializer`
//! and read back by `BinaryXmlPullParser` in AOSP.
//!
//! Not to be confused with **AXML**, the unrelated chunk-based binary format
//! used for compiled resources inside APKs (`AndroidManifest.xml`,
//! `res/**/*.xml`) — this crate does not read that format. See `CLAUDE.md`
//! for the comparison.
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
//!
//! ## Crate layout
//!
//! `error`, `wire`, `event`, `decode` (in-memory + streaming parsers, see
//! [`stream`]), and `de` (serde support, behind the `serialize` feature)
//! are internal modules — everything is re-exported at the crate root, so
//! `abx::Event` etc. work regardless of which file it's defined in.

mod error;
pub use error::{AbxError, Result};

mod wire;
pub use wire::MAGIC;
pub(crate) use wire::{
    INTERNED_NEW,
    CMD_ATTRIBUTE, CMD_CDSECT, CMD_COMMENT, CMD_DOCDECL, CMD_END_DOCUMENT,
    CMD_END_TAG, CMD_ENTITY_REF, CMD_IGNORABLE_WHITESPACE,
    CMD_PROCESSING_INSTRUCTION, CMD_START_DOCUMENT, CMD_START_TAG, CMD_TEXT,
    TYPE_BOOLEAN_FALSE, TYPE_BOOLEAN_TRUE, TYPE_BYTES_BASE64, TYPE_BYTES_HEX,
    TYPE_DOUBLE, TYPE_FLOAT, TYPE_INT, TYPE_INT_HEX, TYPE_LONG, TYPE_LONG_HEX,
    TYPE_NULL, TYPE_STRING, TYPE_STRING_INTERNED,
};

mod event;
pub use event::{Attribute, AttributeValue, Event, InternedStr};
pub(crate) use event::render_event;

mod decode;
pub use decode::{AbxParser, AbxParserOwned};
pub use decode::stream;
pub use decode::stream::AbxStreamParser;

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
