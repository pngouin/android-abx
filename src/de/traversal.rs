//! Walks the event stream: finds a matching (or the root) element, then
//! recursively collects its attributes/text/children into an [`ElementData`]
//! tree ready for [`super::element::ElementDeserializer`].

use std::io::Read;

use serde::de::DeserializeOwned;

use crate::{AbxError, AbxParser, AbxStreamParser, Attribute, Event, InternedStr, Result};

use super::element::ElementDeserializer;

// ---------------------------------------------------------------------------
// EventSource — unifies AbxParser and AbxStreamParser so the traversal
// below is written once, not once per parser type.
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
/// [`super::from_element`] takes directly.
pub(crate) struct ElementData {
    pub(crate) attributes: Vec<Attribute>,
    pub(crate) text: Option<String>,
    pub(crate) children: ChildList,
}

pub(crate) type ChildList = Vec<(InternedStr, ElementData)>;

/// Consume events up to (and including) the end tag that closes the element
/// whose start tag was just read: direct-child `Text` content is
/// accumulated, and each nested `StartTag` is recursively collected in full
/// (its own attributes, text, and children) rather than being skipped.
fn read_element_body<S: EventSource>(source: &mut S) -> Result<(Option<String>, ChildList)> {
    let mut text = String::new();
    let mut has_text = false;
    let mut children = Vec::new();
    loop {
        match source.next_event()? {
            Some(Event::StartTag { name, attributes }) => {
                let (child_text, child_children) = read_element_body(source)?;
                children.push((name, ElementData { attributes, text: child_text, children: child_children }));
            }
            // Doesn't check the end tag's name against the element being
            // closed: real AOSP's own BinaryXmlPullParser.nextToken() reads
            // an END_TAG's name into mCurrentName with no stack or
            // validation either, so this is parity, not a gap.
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
