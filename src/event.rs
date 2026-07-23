//! The shared event/data model — [`Event`], [`Attribute`], [`AttributeValue`],
//! [`InternedStr`] — and the XML-rendering logic built on them.
//! `to_xml`/`write_xml` on both parsers call into [`render_event`].

use base64::Engine as _;

/// Renders a hex-typed attribute value like AOSP's `Integer.toString(v, 16)`/
/// `Long.toString(v, 16)`: `v` is signed, so a negative value renders as
/// `-` + hex(magnitude), not the raw bit pattern (e.g. `0xCAFEBABE` is
/// `"-35014542"`, not `"cafebabe"`).
fn format_signed_hex(v: i64) -> String {
    if v < 0 {
        format!("-{:x}", v.unsigned_abs())
    } else {
        format!("{v:x}")
    }
}

// ---------------------------------------------------------------------------
// Typed attribute value
// ---------------------------------------------------------------------------

/// The typed payload of an XML attribute.
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    /// No value (AOSP's `TYPE_NULL`).
    Null,
    /// A plain UTF-8 string.
    String(String),
    /// Bytes whose canonical text form is lowercase hex.
    BytesHex(Vec<u8>),
    /// Bytes whose canonical text form is Base64.
    BytesBase64(Vec<u8>),
    /// A 32-bit signed integer, rendered in decimal.
    Int(i32),
    /// A 32-bit value rendered in hex — see [`AttributeValue::as_str`] for
    /// the signed-magnitude rendering rule this follows.
    IntHex(u32),
    /// A 64-bit signed integer, rendered in decimal.
    Long(i64),
    /// A 64-bit value rendered in hex — see [`AttributeValue::as_str`] for
    /// the signed-magnitude rendering rule this follows.
    LongHex(u64),
    /// A 32-bit floating-point value.
    Float(f32),
    /// A 64-bit floating-point value.
    Double(f64),
    /// A boolean, rendered as `"true"`/`"false"`.
    Boolean(bool),
}

impl AttributeValue {
    /// Render the value as a human-readable string, mirroring the original
    /// Java serializer's output.
    pub fn as_str(&self) -> std::borrow::Cow<'_, str> {
        use std::borrow::Cow;
        match self {
            AttributeValue::Null => Cow::Borrowed(""),
            AttributeValue::String(s) => Cow::Borrowed(s.as_str()),
            AttributeValue::BytesHex(b) => Cow::Owned(faster_hex::hex_string(b)),
            AttributeValue::BytesBase64(b) => {
                Cow::Owned(base64::engine::general_purpose::STANDARD.encode(b))
            }
            AttributeValue::Int(v) => Cow::Owned(v.to_string()),
            AttributeValue::IntHex(v) => Cow::Owned(format_signed_hex(*v as i32 as i64)),
            AttributeValue::Long(v) => Cow::Owned(v.to_string()),
            AttributeValue::LongHex(v) => Cow::Owned(format_signed_hex(*v as i64)),
            AttributeValue::Float(v) => {
                if v.fract() == 0.0 && v.is_finite() {
                    Cow::Owned(format!("{:.1}", v))
                } else {
                    Cow::Owned(v.to_string())
                }
            }
            AttributeValue::Double(v) => {
                if v.fract() == 0.0 && v.is_finite() {
                    Cow::Owned(format!("{:.1}", v))
                } else {
                    Cow::Owned(v.to_string())
                }
            }
            AttributeValue::Boolean(b) => {
                if *b {
                    Cow::Borrowed("true")
                } else {
                    Cow::Borrowed("false")
                }
            }
        }
    }

    // Typed accessors ----------------------------------------------------------

    /// Returns the inner string if this is [`AttributeValue::String`], else `None`.
    pub fn as_string(&self) -> Option<&str> {
        if let AttributeValue::String(s) = self {
            Some(s)
        } else {
            None
        }
    }
    /// Returns the inner value if this is [`AttributeValue::Int`], else `None`.
    pub fn as_int(&self) -> Option<i32> {
        if let AttributeValue::Int(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    /// Returns the inner value if this is [`AttributeValue::IntHex`], else `None`.
    pub fn as_int_hex(&self) -> Option<u32> {
        if let AttributeValue::IntHex(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    /// Returns the inner value if this is [`AttributeValue::Long`], else `None`.
    pub fn as_long(&self) -> Option<i64> {
        if let AttributeValue::Long(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    /// Returns the inner value if this is [`AttributeValue::LongHex`], else `None`.
    pub fn as_long_hex(&self) -> Option<u64> {
        if let AttributeValue::LongHex(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    /// Returns the inner value if this is [`AttributeValue::Float`], else `None`.
    pub fn as_float(&self) -> Option<f32> {
        if let AttributeValue::Float(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    /// Returns the inner value if this is [`AttributeValue::Double`], else `None`.
    pub fn as_double(&self) -> Option<f64> {
        if let AttributeValue::Double(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    /// Returns the inner value if this is [`AttributeValue::Boolean`], else `None`.
    pub fn as_bool(&self) -> Option<bool> {
        if let AttributeValue::Boolean(b) = self {
            Some(*b)
        } else {
            None
        }
    }
    /// Returns the inner bytes if this is [`AttributeValue::BytesHex`] or
    /// [`AttributeValue::BytesBase64`], else `None`.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            AttributeValue::BytesHex(b) | AttributeValue::BytesBase64(b) => Some(b),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// InternedStr — a cheaply-clonable string for tag/attribute names
// ---------------------------------------------------------------------------

/// A tag or attribute name read from the wire format's interned-string
/// pool. The same handful of names (`pkg`, `name`, `version`, ...) repeat
/// across every element in a typical document, so back-reference clones
/// need to be cheap.
///
/// `InternedStr` is [`smol_str::SmolStr`]: strings up to 23 bytes are
/// stored inline (clone is a stack copy), longer ones fall back to a
/// reference-counted `Arc<str>` (clone is a refcount bump) — either way, no
/// allocation on clone.
pub type InternedStr = smol_str::SmolStr;

// ---------------------------------------------------------------------------
// Attribute
// ---------------------------------------------------------------------------

/// One XML attribute: a name plus its typed value.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    /// The attribute's name.
    pub name: InternedStr,
    /// The attribute's typed value.
    pub value: AttributeValue,
}

impl Attribute {
    /// Render [`Attribute::value`] as a human-readable string; see
    /// [`AttributeValue::as_str`].
    pub fn as_str(&self) -> std::borrow::Cow<'_, str> {
        self.value.as_str()
    }
}

// ---------------------------------------------------------------------------
// XML Event  (shared by both parsers)
// ---------------------------------------------------------------------------

/// One `XmlPullParser`-style parse event, as read from (or written to) an
/// ABX stream.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The start of the document — always the first event.
    StartDocument,
    /// The end of the document — always the last event.
    EndDocument,
    /// The opening tag of an element, with its attributes.
    StartTag {
        /// The element's tag name.
        name: InternedStr,
        /// The element's attributes, in document order.
        attributes: Vec<Attribute>,
    },
    /// The closing tag of an element.
    EndTag {
        /// The element's tag name, matching the corresponding [`Event::StartTag`].
        name: InternedStr,
    },
    /// Plain character data between tags.
    Text(String),
    /// A `<![CDATA[...]]>` section's raw content.
    CdataSection(String),
    /// A `<!--...-->` comment's raw content.
    Comment(String),
    /// A `<?...?>` processing instruction's raw content.
    ProcessingInstruction(String),
    /// An entity reference's raw name (e.g. `"amp"` for `&amp;`), not the
    /// resolved character — see the crate docs for why.
    EntityReference(String),
    /// Whitespace-only character data that a validating parser would treat
    /// as ignorable.
    IgnorableWhitespace(String),
    /// A `<!DOCTYPE ...>` declaration's raw content.
    DocDecl(String),
}

// ---------------------------------------------------------------------------
// Shared XML rendering helper
// ---------------------------------------------------------------------------

pub(crate) fn xml_escape(s: &str) -> std::borrow::Cow<'_, str> {
    if s.bytes()
        .any(|c| matches!(c, b'<' | b'>' | b'&' | b'"' | b'\''))
    {
        let mut out = String::with_capacity(s.len() + 8);
        for c in s.chars() {
            match c {
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '&' => out.push_str("&amp;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&apos;"),
                other => out.push(other),
            }
        }
        std::borrow::Cow::Owned(out)
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

/// Write `value`'s XML-attribute text form directly into `buf`, without
/// allocating an intermediate `String`. Numeric/bool/bytes output can never
/// contain an XML-special character, so those variants also skip the
/// escaping scan entirely.
fn push_attr_value(buf: &mut String, value: &AttributeValue) {
    use std::fmt::Write as _;
    match value {
        AttributeValue::Null => {}
        AttributeValue::String(s) => buf.push_str(&xml_escape(s)),
        AttributeValue::BytesHex(b) => buf.push_str(&faster_hex::hex_string(b)),
        AttributeValue::BytesBase64(b) => {
            base64::engine::general_purpose::STANDARD.encode_string(b, buf);
        }
        AttributeValue::Int(v) => {
            let _ = write!(buf, "{v}");
        }
        AttributeValue::IntHex(v) => {
            let _ = write!(buf, "{}", format_signed_hex(*v as i32 as i64));
        }
        AttributeValue::Long(v) => {
            let _ = write!(buf, "{v}");
        }
        AttributeValue::LongHex(v) => {
            let _ = write!(buf, "{}", format_signed_hex(*v as i64));
        }
        AttributeValue::Float(v) => {
            if v.fract() == 0.0 && v.is_finite() {
                let _ = write!(buf, "{v:.1}");
            } else {
                let _ = write!(buf, "{v}");
            }
        }
        AttributeValue::Double(v) => {
            if v.fract() == 0.0 && v.is_finite() {
                let _ = write!(buf, "{v:.1}");
            } else {
                let _ = write!(buf, "{v}");
            }
        }
        AttributeValue::Boolean(b) => buf.push_str(if *b { "true" } else { "false" }),
    }
}

/// Shared render-to-XML logic used by both parsers.
pub(crate) fn render_event(ev: &Event, buf: &mut String) {
    match ev {
        Event::StartDocument | Event::EndDocument => {}
        Event::StartTag { name, attributes } => {
            buf.push('<');
            buf.push_str(name);
            for attr in attributes {
                buf.push(' ');
                buf.push_str(&attr.name);
                buf.push_str("=\"");
                push_attr_value(buf, &attr.value);
                buf.push('"');
            }
            buf.push('>');
        }
        Event::EndTag { name } => {
            buf.push_str("</");
            buf.push_str(name);
            buf.push('>');
        }
        Event::Text(t) if !t.is_empty() => buf.push_str(&xml_escape(t)),
        Event::CdataSection(t) => {
            buf.push_str("<![CDATA[");
            buf.push_str(t);
            buf.push_str("]]>");
        }
        Event::Comment(t) => {
            buf.push_str("<!--");
            buf.push_str(t);
            buf.push_str("-->");
        }
        Event::ProcessingInstruction(t) => {
            buf.push_str("<?");
            buf.push_str(t);
            buf.push_str("?>");
        }
        Event::EntityReference(t) => {
            buf.push('&');
            buf.push_str(t);
            buf.push(';');
        }
        Event::IgnorableWhitespace(t) => buf.push_str(t),
        Event::DocDecl(t) => {
            buf.push_str("<!DOCTYPE ");
            buf.push_str(t);
            buf.push('>');
        }
        _ => {}
    }
}
