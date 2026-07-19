//! `serde` support: deserialize a single ABX element's attributes, child
//! elements, and direct text content into a Rust struct.
//!
//! ## Quick start
//!
//! For a document that *is* one record — a config file whose root element
//! holds the data you want — use one of the one-shot entry points, same
//! shape as `serde_json::from_slice`/`quick_xml::de::from_str`:
//!
//! ```rust,ignore
//! let pkg: Pkg = abx::from_file("pkg.abx")?;   // from a path
//! let pkg: Pkg = abx::from_slice(&bytes)?;     // from an in-memory buffer
//! let pkg: Pkg = abx::from_reader(reader)?;    // from any std::io::Read
//! ```
//!
//! None of these check the root element's tag name against `T` —
//! deserialization is structural, not name-based.
//!
//! For a document whose interesting content is *repeated* elements under an
//! outer wrapper (AOSP's `packages.xml` shape: one `<packages>` root, many
//! `<pkg>` children), use the parser directly and name the repeated element:
//!
//! ```rust,ignore
//! let mut p = abx::open_file("packages.abx")?;
//! let pkgs: Vec<Pkg> = p.deserialize_all("pkg")?;
//! ```
//!
//! ([`crate::AbxStreamParser::deserialize_iter`] is the lazy equivalent, for
//! streaming without collecting a `Vec` upfront.)
//!
//! A struct field maps to:
//! - an **attribute** by its own name, or `#[serde(rename = "...")]` for an
//!   attribute name that isn't a valid Rust identifier;
//! - a **child element**, the same way — a nested struct field consumes the
//!   first matching child, a `Vec<T>` field consumes all of them, and a
//!   scalar field (`String`, `i32`, ...) consumes a *leaf* child (one with
//!   no attributes or children of its own) as its text content;
//! - the element's own **direct text content**, via a field renamed to
//!   `$text`, the same convention `quick-xml`'s serde support uses.
//!
//! # Differences from `quick-xml`
//!
//! No `@attr` prefix: an attribute always wins over a same-named child
//! element instead of requiring `#[serde(rename = "@name")]` to
//! disambiguate. A field literally named `$text` loses silently to that
//! precedence rather than erroring.
//!
//! No `$value` enum-of-elements mapping (`xs:choice`-style heterogeneous
//! children) — every child element name maps to one field, not a variant
//! selector. And no `Serialize` side — this crate only parses ABX, so
//! deserializing into a struct is one-way.
//!
//! ## Internal layout
//!
//! `traversal` walks the event stream and builds the recursive `ElementData`
//! tree; `element` (`ElementDeserializer`) turns one `ElementData` into a
//! `serde` map; `value` (`ValueDeserializer`) turns one attribute, the text,
//! or a same-named child group into a single `serde` value. None of this is
//! public API — only the functions re-exported below are.

use std::io::Read;

use serde::de::{self, DeserializeOwned};

use crate::{AbxError, Attribute, Result};

mod element;
mod traversal;
mod value;

pub(crate) use traversal::{find_and_consume_element, find_and_consume_root_element};

/// Struct field name convention for an element's direct text content.
pub(crate) const TEXT_FIELD: &str = "$text";

impl de::Error for AbxError {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        AbxError::Deserialization(msg.to_string())
    }
}

/// Deserialize a single element's attributes (and optional text content)
/// into `T`, honoring `#[serde(rename = "...")]`, `Option<T>` for absent
/// attributes, and numeric/bytes coercions. This convenience entry point has
/// no child elements to offer — it's meant for callers who already have an
/// `Event::StartTag`'s attributes in hand. Nested-child mapping is only
/// available through [`crate::AbxParser::deserialize_next`] and
/// [`crate::AbxStreamParser::deserialize_next`], which build the child tree
/// by walking the event stream.
pub fn from_element<T: DeserializeOwned>(attributes: &[Attribute], text: Option<&str>) -> Result<T> {
    T::deserialize(element::ElementDeserializer { attributes, text, children: &[] })
}

/// Deserialize an entire in-memory ABX document into `T`, using its root
/// element. The one-shot entry point for "this whole document is one
/// struct" — no parser to construct, no element name to spell out. Matches
/// quick-xml's `from_str`/serde_json's `from_slice`: the root's tag name is
/// not checked against `T` at all, so name your types however you like.
///
/// For a document whose interesting content is *repeated* elements nested
/// under an outer wrapper (e.g. AOSP's `packages.xml`), use
/// [`crate::AbxParser::deserialize_all`]/`deserialize_iter` instead — this
/// function is for when the root itself is the record you want.
pub fn from_slice<T: DeserializeOwned>(data: &[u8]) -> Result<T> {
    let mut parser = crate::AbxParser::new(data)?;
    find_and_consume_root_element(&mut parser)
}

/// Streaming equivalent of [`from_slice`]: deserialize the root element of
/// an ABX document read from any [`std::io::Read`] source.
pub fn from_reader<R: Read, T: DeserializeOwned>(reader: R) -> Result<T> {
    let mut parser = crate::AbxStreamParser::new(reader)?;
    find_and_consume_root_element(&mut parser)
}

/// Open a file and deserialize its root element into `T`.
pub fn from_file<T: DeserializeOwned>(path: impl AsRef<std::path::Path>) -> Result<T> {
    let mut parser = crate::open_file(path)?;
    find_and_consume_root_element(&mut parser)
}
