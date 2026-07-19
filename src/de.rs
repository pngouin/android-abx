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
//! None of these need the root element's tag name — deserialization is
//! structural, not name-based (see "Relationship to `quick-xml`'s `de`
//! module" below).
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
//!   `$text`, mirroring the convention used by `quick-xml`'s serde support.
//!
//! This covers the common ABX shape of a flat or shallowly-nested,
//! optionally-repeated element (e.g. AOSP's `packages.xml`/`settings.xml`
//! entries), which is what [`crate::AbxParser::deserialize_next`] and
//! [`crate::AbxStreamParser::deserialize_next`] are built on.
//!
//! # Relationship to `quick-xml`'s `de` module
//!
//! This module deliberately follows [quick-xml's `de` module
//! conventions](https://docs.rs/quick-xml/latest/quick_xml/de/index.html)
//! where they carry over unchanged, and diverges where noted:
//!
//! - **Aligned:** [`crate::from_slice`]/[`crate::from_reader`] mirror
//!   `quick_xml::de::from_str`/`from_reader` (and `serde_json::from_slice`)
//!   exactly — a one-shot top-level function per input source, root tag name
//!   unchecked. The `$text` field name; `Option<T>` distinguishing an
//!   absent attribute/child (`None`, handled by serde's own default
//!   behavior for `Option` fields) from one present-but-empty (`Some("")`);
//!   unit-variant enums matched by name (`#[serde(rename = "...")]` on the
//!   variant, same as on a field) against a string value; repeated child
//!   elements collected into a `Vec<T>` field; a text-only child collapsing
//!   to a plain scalar field.
//! - **No `@attr` prefix:** quick-xml requires `#[serde(rename = "@name")]`
//!   because a struct field can be *either* an attribute or a child element
//!   sharing one namespace. We resolve that collision by precedence instead
//!   — **an attribute always wins over a same-named child** — so a bare
//!   field name unambiguously means "look up this name, attribute first,
//!   then child element". The one sharp edge this (and the `$text`
//!   convention) leaves: an attribute or child literally named `$text`
//!   would collide with the synthetic text-content key; whichever of
//!   attribute/child/text-content wins the above precedence order suppresses
//!   the others silently rather than raising a `duplicate field` error.
//! - **No `$value` choice/enum-of-elements mapping:** quick-xml can deserialize
//!   an enum field from *which* child element tag is present (heterogeneous
//!   "pick one of several shapes" content, `xs:choice`-style). We don't
//!   support that — every child element name maps to a specific field, not
//!   a variant selector.
//! - **No `Serialize`:** this crate only parses ABX, it has no ABX encoder,
//!   so there's no equivalent of quick-xml's serialize side — deserializing
//!   into a struct is one-way.

use std::collections::HashSet;
use std::io::Read;

use serde::de::{self, DeserializeOwned, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};

use crate::{AbxError, AbxParser, AbxStreamParser, Attribute, AttributeValue, Event, Result};

/// Struct field name convention for an element's direct text content.
pub const TEXT_FIELD: &str = "$text";

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
    T::deserialize(ElementDeserializer { attributes, text, children: &[] })
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

// ---------------------------------------------------------------------------
// EventSource — unifies AbxParser and AbxStreamParser for the generic
// find-element / collect-body traversal below, so it's written once instead
// of once per parser type.
// ---------------------------------------------------------------------------

pub(crate) trait EventSource {
    fn next_event(&mut self) -> Result<Option<Event>>;
}

impl<'de> EventSource for AbxParser<'de> {
    fn next_event(&mut self) -> Result<Option<Event>> {
        self.next_event()
    }
}

impl<R: Read> EventSource for AbxStreamParser<R> {
    fn next_event(&mut self) -> Result<Option<Event>> {
        self.next_event()
    }
}

/// Advance to the next `<element>` in the stream (skipping everything else,
/// same as [`crate::AbxParser::attributes_of`]), consume its body up to and
/// including its matching end tag — recursively collecting child elements —
/// and deserialize the resulting attribute/child/text tree into `T`.
/// `Ok(None)` at end of document.
pub(crate) fn find_and_consume_element<S, T>(source: &mut S, element: &str) -> Result<Option<T>>
where
    S: EventSource,
    T: DeserializeOwned,
{
    loop {
        match source.next_event()? {
            Some(Event::StartTag { name, attributes }) if name == element => {
                return deserialize_started_element(source, attributes).map(Some);
            }
            Some(Event::EndDocument) | None => return Ok(None),
            _ => {}
        }
    }
}

/// Advance to the document's root element — whichever tag it is, unlike
/// [`find_and_consume_element`] this doesn't filter by name, matching
/// quick-xml's `from_str`/serde_json's `from_slice`: deserialization is
/// structural, so the root's tag name is never checked against `T`. Errors
/// if the document has no element at all.
pub(crate) fn find_and_consume_root_element<S, T>(source: &mut S) -> Result<T>
where
    S: EventSource,
    T: DeserializeOwned,
{
    loop {
        match source.next_event()? {
            Some(Event::StartTag { attributes, .. }) => {
                return deserialize_started_element(source, attributes);
            }
            Some(Event::EndDocument) | None => {
                return Err(AbxError::Deserialization("no root element found in document".to_string()));
            }
            _ => {}
        }
    }
}

/// Shared by both traversal functions above: given a `StartTag`'s already-read
/// attributes, consume the rest of its body and deserialize the result.
fn deserialize_started_element<S, T>(source: &mut S, attributes: Vec<Attribute>) -> Result<T>
where
    S: EventSource,
    T: DeserializeOwned,
{
    let (text, children) = read_element_body(source)?;
    let de = ElementDeserializer { attributes: &attributes, text: text.as_deref(), children: &children };
    T::deserialize(de)
}

/// A child element's own attributes/text/children, fully collected — the
/// recursive counterpart to the flat `(attributes, text)` pair
/// [`from_element`] takes directly.
struct ElementData {
    attributes: Vec<Attribute>,
    text: Option<String>,
    children: Vec<(String, ElementData)>,
}

/// Consume events up to (and including) the end tag that closes the element
/// whose start tag was just read: direct-child `Text` content is
/// accumulated, and each nested `StartTag` is recursively collected in full
/// (its own attributes, text, and children) rather than being skipped.
fn read_element_body<S: EventSource>(source: &mut S) -> Result<(Option<String>, Vec<(String, ElementData)>)> {
    let mut text = String::new();
    let mut has_text = false;
    let mut children = Vec::new();
    loop {
        match source.next_event()? {
            Some(Event::StartTag { name, attributes }) => {
                let (child_text, child_children) = read_element_body(source)?;
                children.push((name, ElementData { attributes, text: child_text, children: child_children }));
            }
            Some(Event::EndTag { .. }) => break,
            Some(Event::Text(t)) => {
                has_text = true;
                text.push_str(&t);
            }
            Some(Event::EndDocument) | None => break,
            _ => {}
        }
    }
    Ok((has_text.then_some(text), children))
}

/// Generates `Deserializer` methods on `ElementDeserializer` that try
/// `FromStr` on the leaf text before falling back to `deserialize_any`.
/// Element text is always a plain string in the wire format (unlike
/// attributes, which carry their own `AttributeValue` type), so this is the
/// one place that has to parse it itself to make numeric/bool/char leaf
/// children work — the same thing quick-xml's element-text deserializer
/// does.
macro_rules! scalar_from_leaf_text {
    ($($method:ident => $visit:ident : $ty:ty),+ $(,)?) => {
        $(
            fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
                if self.attributes.is_empty() && self.children.is_empty() {
                    if let Some(t) = self.text {
                        if let Ok(v) = t.parse::<$ty>() {
                            return visitor.$visit(v);
                        }
                    }
                }
                self.deserialize_any(visitor)
            }
        )+
    };
}

// ---------------------------------------------------------------------------
// ElementDeserializer — attributes + child elements (+ optional $text).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct ElementDeserializer<'de> {
    attributes: &'de [Attribute],
    text: Option<&'de str>,
    children: &'de [(String, ElementData)],
}

impl<'de> ElementDeserializer<'de> {
    fn from_data(data: &'de ElementData) -> Self {
        ElementDeserializer { attributes: &data.attributes, text: data.text.as_deref(), children: &data.children }
    }
}

impl<'de> Deserializer<'de> for ElementDeserializer<'de> {
    type Error = AbxError;

    /// A "leaf" element (no attributes, no children — just optional text, or
    /// nothing at all) deserializes as a plain scalar: the text content via
    /// `visit_str` (letting serde's own string parsing handle numbers/bools),
    /// or unit if there's no text either. Anything richer is struct/map
    /// shaped. This makes the same type work whether it's reached as the top
    /// level, a singular nested-struct field, or one item of a `Vec<T>`
    /// field (see `ValueDeserializer`'s `Children` handling below).
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        if self.attributes.is_empty() && self.children.is_empty() {
            match self.text {
                Some(t) => visitor.visit_str(t),
                None => visitor.visit_unit(),
            }
        } else {
            self.deserialize_map(visitor)
        }
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let attr_names: HashSet<&str> = self.attributes.iter().map(|a| a.name.as_str()).collect();
        let children = group_children(self.children, &attr_names);
        let text_shadowed = attr_names.contains(TEXT_FIELD) || children.iter().any(|(name, _)| *name == TEXT_FIELD);
        visitor.visit_map(ElementMapAccess {
            attrs: self.attributes.iter(),
            children: children.into_iter(),
            text: if text_shadowed { None } else { self.text },
            pending: None,
        })
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_map(visitor)
    }

    // Leaf text needs a shot at FromStr parsing before falling back to
    // deserialize_any's visit_str/visit_unit — plain string-visiting alone
    // doesn't make numeric/bool/char targets work (unlike the typed
    // AttributeValue path in ValueDeserializer, element text is always a
    // plain string in the wire format, so this is the one place that has to
    // parse it itself, the same way quick-xml's element-text deserializer
    // does).
    scalar_from_leaf_text! {
        deserialize_bool => visit_bool: bool,
        deserialize_i8 => visit_i8: i8,
        deserialize_i16 => visit_i16: i16,
        deserialize_i32 => visit_i32: i32,
        deserialize_i64 => visit_i64: i64,
        deserialize_i128 => visit_i128: i128,
        deserialize_u8 => visit_u8: u8,
        deserialize_u16 => visit_u16: u16,
        deserialize_u32 => visit_u32: u32,
        deserialize_u64 => visit_u64: u64,
        deserialize_u128 => visit_u128: u128,
        deserialize_f32 => visit_f32: f32,
        deserialize_f64 => visit_f64: f64,
        deserialize_char => visit_char: char,
    }

    serde::forward_to_deserialize_any! {
        str string bytes byte_buf option unit unit_struct newtype_struct seq
        tuple tuple_struct enum identifier ignored_any
    }
}

/// Group children by tag name, preserving first-occurrence order, and
/// dropping any whose name collides with an attribute (attribute wins).
fn group_children<'de>(
    children: &'de [(String, ElementData)],
    attr_names: &HashSet<&str>,
) -> Vec<(&'de str, Vec<&'de ElementData>)> {
    let mut groups: Vec<(&'de str, Vec<&'de ElementData>)> = Vec::new();
    for (name, data) in children {
        let name = name.as_str();
        if attr_names.contains(name) {
            continue;
        }
        match groups.iter_mut().find(|(n, _)| *n == name) {
            Some((_, items)) => items.push(data),
            None => groups.push((name, vec![data])),
        }
    }
    groups
}

struct ElementMapAccess<'de> {
    attrs: std::slice::Iter<'de, Attribute>,
    children: std::vec::IntoIter<(&'de str, Vec<&'de ElementData>)>,
    text: Option<&'de str>,
    pending: Option<FieldValue<'de>>,
}

impl<'de> MapAccess<'de> for ElementMapAccess<'de> {
    type Error = AbxError;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        if let Some(attr) = self.attrs.next() {
            self.pending = Some(FieldValue::Attr(&attr.value));
            let key = de::value::StrDeserializer::<AbxError>::new(attr.name.as_str());
            return seed.deserialize(key).map(Some);
        }
        if let Some((name, items)) = self.children.next() {
            self.pending = Some(FieldValue::Children(items));
            let key = de::value::StrDeserializer::<AbxError>::new(name);
            return seed.deserialize(key).map(Some);
        }
        if let Some(t) = self.text.take() {
            self.pending = Some(FieldValue::Text(t));
            let key = de::value::StrDeserializer::<AbxError>::new(TEXT_FIELD);
            return seed.deserialize(key).map(Some);
        }
        Ok(None)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        let value = self
            .pending
            .take()
            .expect("next_value_seed called before next_key_seed");
        seed.deserialize(ValueDeserializer(value))
    }

    fn size_hint(&self) -> Option<usize> {
        let (attrs_lower, _) = self.attrs.size_hint();
        let (children_lower, _) = self.children.size_hint();
        Some(attrs_lower + children_lower + self.text.is_some() as usize)
    }
}

// ---------------------------------------------------------------------------
// ValueDeserializer — a single attribute value, the text content, or a
// group of same-named child elements.
// ---------------------------------------------------------------------------

/// Generates `Deserializer` methods on `ValueDeserializer` that try `FromStr`
/// before falling back to `deserialize_any`, for the two textual sources
/// (`Text`, and a `String`-typed attribute) — already-typed numeric/boolean
/// `AttributeValue`s skip straight to `deserialize_any`'s exact visit call,
/// which is always correct for them (no parsing needed). A `Children` group
/// forwards to the *same* named method on the first child's own
/// `ElementDeserializer`, rather than going through `deserialize_any`, so a
/// leaf child's target-type information isn't lost on the way down.
macro_rules! scalar_from_text_or_children {
    ($($method:ident => $visit:ident : $ty:ty),+ $(,)?) => {
        $(
            fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
                match &self.0 {
                    FieldValue::Attr(AttributeValue::String(s)) => {
                        if let Ok(v) = s.parse::<$ty>() {
                            return visitor.$visit(v);
                        }
                    }
                    FieldValue::Text(s) => {
                        if let Ok(v) = s.parse::<$ty>() {
                            return visitor.$visit(v);
                        }
                    }
                    FieldValue::Children(items) => {
                        return ElementDeserializer::from_data(items[0]).$method(visitor);
                    }
                    _ => {}
                }
                self.deserialize_any(visitor)
            }
        )+
    };
}

#[derive(Clone)]
enum FieldValue<'de> {
    Attr(&'de AttributeValue),
    Text(&'de str),
    Children(Vec<&'de ElementData>),
}

struct ValueDeserializer<'de>(FieldValue<'de>);

impl<'de> Deserializer<'de> for ValueDeserializer<'de> {
    type Error = AbxError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.0 {
            FieldValue::Text(s) => visitor.visit_str(s),
            FieldValue::Attr(v) => match v {
                AttributeValue::Null => visitor.visit_unit(),
                AttributeValue::String(s) => visitor.visit_str(s),
                AttributeValue::BytesHex(b) | AttributeValue::BytesBase64(b) => visitor.visit_bytes(b),
                AttributeValue::Int(n) => visitor.visit_i32(*n),
                AttributeValue::IntHex(n) => visitor.visit_u32(*n),
                AttributeValue::Long(n) => visitor.visit_i64(*n),
                AttributeValue::LongHex(n) => visitor.visit_u64(*n),
                AttributeValue::Float(f) => visitor.visit_f32(*f),
                AttributeValue::Double(f) => visitor.visit_f64(*f),
                AttributeValue::Boolean(b) => visitor.visit_bool(*b),
            },
            // A singular (non-Vec) target field: use the first matching
            // child, delegating to ElementDeserializer's own leaf-or-struct
            // logic (so a text-only child still collapses to a scalar).
            FieldValue::Children(items) => ElementDeserializer::from_data(items[0]).deserialize_any(visitor),
        }
    }

    /// Attributes present with a `Null` value deserialize as `None`;
    /// everything else (including a missing attribute/child, handled
    /// upstream by serde's own "absent key => None" behavior for
    /// `Option<T>` fields) as `Some`.
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        if matches!(&self.0, FieldValue::Attr(AttributeValue::Null)) {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    /// `Vec<u8>` from `BytesHex`/`BytesBase64`, or a `Vec<T>` field from all
    /// same-named child elements (each recursively deserialized).
    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match &self.0 {
            FieldValue::Attr(AttributeValue::BytesHex(b) | AttributeValue::BytesBase64(b)) => {
                return de::value::SeqDeserializer::<_, AbxError>::new(b.iter().copied()).deserialize_seq(visitor);
            }
            FieldValue::Children(items) => {
                return visitor.visit_seq(ChildSeqAccess { iter: items.iter() });
            }
            _ => {}
        }
        self.deserialize_any(visitor)
    }

    /// Unit-variant enums, selected by matching a string value (attribute or
    /// `$text`) against a variant name — mirrors quick-xml's rule that
    /// "variant names become element or attribute names". Non-string values
    /// fall through to [`deserialize_any`](Self::deserialize_any), which
    /// reports a clear type-mismatch error (enum variants carrying data
    /// aren't supported, same as quick-xml's derive-based deserialization).
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        use de::IntoDeserializer;
        match self.0 {
            FieldValue::Text(s) => visitor.visit_enum(IntoDeserializer::<AbxError>::into_deserializer(s)),
            FieldValue::Attr(AttributeValue::String(s)) => {
                visitor.visit_enum(IntoDeserializer::<AbxError>::into_deserializer(s.as_str()))
            }
            _ => self.deserialize_any(visitor),
        }
    }

    scalar_from_text_or_children! {
        deserialize_bool => visit_bool: bool,
        deserialize_i8 => visit_i8: i8,
        deserialize_i16 => visit_i16: i16,
        deserialize_i32 => visit_i32: i32,
        deserialize_i64 => visit_i64: i64,
        deserialize_i128 => visit_i128: i128,
        deserialize_u8 => visit_u8: u8,
        deserialize_u16 => visit_u16: u16,
        deserialize_u32 => visit_u32: u32,
        deserialize_u64 => visit_u64: u64,
        deserialize_u128 => visit_u128: u128,
        deserialize_f32 => visit_f32: f32,
        deserialize_f64 => visit_f64: f64,
        deserialize_char => visit_char: char,
    }

    serde::forward_to_deserialize_any! {
        str string bytes byte_buf unit unit_struct newtype_struct tuple
        tuple_struct map struct identifier ignored_any
    }
}

/// Yields each child in a same-named group, recursively deserialized —
/// backs a `Vec<T>` field.
struct ChildSeqAccess<'a, 'de> {
    iter: std::slice::Iter<'a, &'de ElementData>,
}

impl<'a, 'de> SeqAccess<'de> for ChildSeqAccess<'a, 'de> {
    type Error = AbxError;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>> {
        match self.iter.next() {
            Some(&data) => seed.deserialize(ElementDeserializer::from_data(data)).map(Some),
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}
