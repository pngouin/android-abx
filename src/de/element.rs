//! [`ElementDeserializer`]/[`ElementMapAccess`]: turns one
//! [`ElementData`](super::traversal::ElementData) — an element's attributes,
//! children, and text, already collected by `super::traversal` — into a
//! `serde` map (attribute/child/`$text` name -> value).

use std::collections::HashSet;

use serde::de::{DeserializeSeed, Deserializer, MapAccess, Visitor};

use crate::{AbxError, Attribute, InternedStr, Result};

use super::traversal::ElementData;
use super::value::{FieldValue, ValueDeserializer};
use super::TEXT_FIELD;

/// Generates `Deserializer` methods that try `FromStr` on the leaf text
/// before falling back to `deserialize_any` — element text is always a
/// plain string on the wire, so this is the one place that has to parse it
/// itself for numeric/bool/char leaf children to work.
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
pub(crate) struct ElementDeserializer<'de> {
    pub(crate) attributes: &'de [Attribute],
    pub(crate) text: Option<&'de str>,
    pub(crate) children: &'de [(InternedStr, ElementData)],
}

impl<'de> ElementDeserializer<'de> {
    pub(crate) fn from_data(data: &'de ElementData) -> Self {
        ElementDeserializer { attributes: &data.attributes, text: data.text.as_deref(), children: &data.children }
    }
}

impl<'de> Deserializer<'de> for ElementDeserializer<'de> {
    type Error = AbxError;

    /// A leaf element (no attributes or children — just optional text, or
    /// nothing) deserializes as a plain scalar via `visit_str`/`visit_unit`;
    /// anything richer is struct/map shaped.
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
        // Skip building the attr-name HashSet for flat/leaf elements, where
        // grouping is a no-op anyway.
        let children = if self.children.is_empty() {
            Vec::new()
        } else {
            let attr_names: HashSet<&str> = self.attributes.iter().map(|a| a.name.as_str()).collect();
            group_children(self.children, &attr_names)
        };
        let text_shadowed = self.text.is_some()
            && (self.attributes.iter().any(|a| a.name == TEXT_FIELD)
                || children.iter().any(|(name, _)| *name == TEXT_FIELD));
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
    children: &'de [(InternedStr, ElementData)],
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
            let key = serde::de::value::StrDeserializer::<AbxError>::new(attr.name.as_str());
            return seed.deserialize(key).map(Some);
        }
        if let Some((name, items)) = self.children.next() {
            self.pending = Some(FieldValue::Children(items));
            let key = serde::de::value::StrDeserializer::<AbxError>::new(name);
            return seed.deserialize(key).map(Some);
        }
        if let Some(t) = self.text.take() {
            self.pending = Some(FieldValue::Text(t));
            let key = serde::de::value::StrDeserializer::<AbxError>::new(TEXT_FIELD);
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
