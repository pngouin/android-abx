use serde::de::{self, DeserializeSeed, Deserializer, SeqAccess, Visitor};

use crate::{AbxError, AttributeValue, Result};

use super::element::ElementDeserializer;
use super::traversal::ElementData;

// Parse string values with `FromStr`; forward children to the same method.
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
                AttributeValue::BytesHex(b) | AttributeValue::BytesBase64(b) => {
                    visitor.visit_bytes(b)
                }
                AttributeValue::Int(n) => visitor.visit_i32(*n),
                AttributeValue::IntHex(n) => visitor.visit_u32(*n),
                AttributeValue::Long(n) => visitor.visit_i64(*n),
                AttributeValue::LongHex(n) => visitor.visit_u64(*n),
                AttributeValue::Float(f) => visitor.visit_f32(*f),
                AttributeValue::Double(f) => visitor.visit_f64(*f),
                AttributeValue::Boolean(b) => visitor.visit_bool(*b),
            },
            FieldValue::Children(items) => {
                ElementDeserializer::from_data(items[0]).deserialize_any(visitor)
            }
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        if matches!(&self.0, FieldValue::Attr(AttributeValue::Null)) {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match &self.0 {
            FieldValue::Attr(AttributeValue::BytesHex(b) | AttributeValue::BytesBase64(b)) => {
                return de::value::SeqDeserializer::<_, AbxError>::new(b.iter().copied())
                    .deserialize_seq(visitor);
            }
            FieldValue::Children(items) => {
                return visitor.visit_seq(ChildSeqAccess { iter: items.iter() });
            }
            _ => {}
        }
        self.deserialize_any(visitor)
    }

    // Unit variants only, matched by name.
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        use de::IntoDeserializer;
        match self.0 {
            FieldValue::Text(s) => {
                visitor.visit_enum(IntoDeserializer::<AbxError>::into_deserializer(s))
            }
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

struct ChildSeqAccess<'a, 'de> {
    iter: std::slice::Iter<'a, &'de ElementData>,
}

impl<'a, 'de> SeqAccess<'de> for ChildSeqAccess<'a, 'de> {
    type Error = AbxError;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>> {
        match self.iter.next() {
            Some(&data) => seed
                .deserialize(ElementDeserializer::from_data(data))
                .map(Some),
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}
