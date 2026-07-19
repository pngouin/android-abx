//! [`ValueDeserializer`]/[`FieldValue`]: turns a single attribute, the text
//! content, or a group of same-named child elements — one value out of
//! [`super::element::ElementMapAccess`] — into a `serde` value.

use serde::de::{self, DeserializeSeed, Deserializer, SeqAccess, Visitor};

use crate::{AbxError, AttributeValue, Result};

use super::element::ElementDeserializer;
use super::traversal::ElementData;

/// Generates `Deserializer` methods that try `FromStr` on the two textual
/// sources (`Text`, a `String`-typed attribute) before falling back to
/// `deserialize_any`. Already-typed `AttributeValue`s skip straight to
/// `deserialize_any`'s exact visit call. A `Children` group forwards to the
/// same named method on the first child's `ElementDeserializer`, so its
/// target-type information isn't lost on the way down.
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
pub(crate) enum FieldValue<'de> {
    Attr(&'de AttributeValue),
    Text(&'de str),
    Children(Vec<&'de ElementData>),
}

pub(crate) struct ValueDeserializer<'de>(pub(crate) FieldValue<'de>);

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
