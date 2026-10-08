//! serde deserialization. Fields map to attributes, child elements
//! (`Vec<T>` for repeats) or `$text`; an attribute wins over a same-named child.

use std::io::Read;

use serde::de::{self, DeserializeOwned};

use crate::{AbxError, Attribute, Result};

mod element;
mod traversal;
mod value;

pub(crate) use traversal::{find_and_consume_element, find_and_consume_root_element};

pub(crate) const TEXT_FIELD: &str = "$text";

impl de::Error for AbxError {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        AbxError::Deserialization(msg.to_string())
    }
}

/// Deserializes `T` from an element's attributes and optional text.
///
/// Useful when an [`Event::StartTag`](crate::Event::StartTag) is already at
/// hand. Child elements are not available here: use
/// [`AbxParser::deserialize_next`](crate::AbxParser::deserialize_next) for types
/// with child-element fields.
///
/// # Errors
///
/// Returns [`AbxError::Deserialization`] if the attributes do not match `T`.
///
/// # Examples
///
/// ```
/// use android_abx::{Attribute, AttributeValue};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Pkg {
///     name: String,
///     version: Option<u32>,
/// }
///
/// let attributes = [Attribute {
///     name: "name".into(),
///     value: AttributeValue::String("com.example".into()),
/// }];
/// let pkg: Pkg = android_abx::from_element(&attributes, None)?;
/// assert_eq!(pkg.name, "com.example");
/// assert_eq!(pkg.version, None);
/// # Ok::<(), android_abx::AbxError>(())
/// ```
pub fn from_element<T: DeserializeOwned>(
    attributes: &[Attribute],
    text: Option<&str>,
) -> Result<T> {
    T::deserialize(element::ElementDeserializer {
        attributes,
        text,
        children: &[],
    })
}

/// Deserializes the root element of an ABX document into `T`.
///
/// The root's tag name is not checked. See
/// [Deserializing with serde](crate#deserializing-with-serde) for how fields are
/// matched.
///
/// # Errors
///
/// Returns a parse error if the input is malformed, or
/// [`AbxError::Deserialization`] if the document has no root element or it does
/// not match `T`.
///
/// # Examples
///
/// ```
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Pkg {
///     name: String,
///     version: u32,
/// }
///
/// # let data = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_pkg.abx"));
/// let pkg: Pkg = android_abx::from_slice(data)?;
/// assert_eq!(pkg.name, "com.example.chat");
/// assert_eq!(pkg.version, 3);
/// # Ok::<(), android_abx::AbxError>(())
/// ```
pub fn from_slice<T: DeserializeOwned>(data: &[u8]) -> Result<T> {
    let mut parser = crate::AbxParser::new(data)?;
    find_and_consume_root_element(&mut parser)
}

/// Deserializes the root element of an ABX document read from `reader` into `T`.
///
/// See [`from_slice`].
///
/// # Errors
///
/// Same as [`from_slice`], plus [`AbxError::Io`] if reading fails.
pub fn from_reader<R: Read, T: DeserializeOwned>(reader: R) -> Result<T> {
    let mut parser = crate::AbxStreamParser::new(reader)?;
    find_and_consume_root_element(&mut parser)
}

/// Deserializes the root element of an ABX file into `T`.
///
/// See [`from_slice`].
///
/// # Errors
///
/// Same as [`from_slice`], plus [`AbxError::Io`] if the file cannot be opened or
/// read.
///
/// # Examples
///
/// ```no_run
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct User {
///     id: u32,
///     name: String,
/// }
///
/// let user: User = android_abx::from_file("/data/system/users/0.xml")?;
/// # Ok::<(), android_abx::AbxError>(())
/// ```
pub fn from_file<T: DeserializeOwned>(path: impl AsRef<std::path::Path>) -> Result<T> {
    let mut parser = crate::open_file(path)?;
    find_and_consume_root_element(&mut parser)
}
