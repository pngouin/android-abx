use std::io::Read;

use serde::de::DeserializeOwned;

use crate::{AbxError, AbxParser, AbxStreamParser, Attribute, Event, InternedStr, Result};

use super::element::ElementDeserializer;

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
                return Err(AbxError::Deserialization(
                    "no root element found in document".to_string(),
                ));
            }
            _ => {}
        }
    }
}

fn deserialize_started_element<S, T>(source: &mut S, attributes: Vec<Attribute>) -> Result<T>
where
    S: EventSource,
    T: DeserializeOwned,
{
    let (text, children) = read_element_body(source)?;
    let de = ElementDeserializer {
        attributes: &attributes,
        text: text.as_deref(),
        children: &children,
    };
    T::deserialize(de)
}

pub(crate) struct ElementData {
    pub(crate) attributes: Vec<Attribute>,
    pub(crate) text: Option<String>,
    pub(crate) children: ChildList,
}

pub(crate) type ChildList = Vec<(InternedStr, ElementData)>;

/// Consume up to the matching end tag, collecting text and children.
fn read_element_body<S: EventSource>(source: &mut S) -> Result<(Option<String>, ChildList)> {
    let mut text = String::new();
    let mut has_text = false;
    let mut children = Vec::new();
    loop {
        match source.next_event()? {
            Some(Event::StartTag { name, attributes }) => {
                let (child_text, child_children) = read_element_body(source)?;
                children.push((
                    name,
                    ElementData {
                        attributes,
                        text: child_text,
                        children: child_children,
                    },
                ));
            }
            // Name not checked, same as AOSP BinaryXmlPullParser.
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
