use base64::Engine as _;

// Signed, like Java's `Integer.toString(v, 16)`: 0xCAFEBABE -> "-35014542".
fn format_signed_hex(v: i64) -> String {
    if v < 0 {
        format!("-{:x}", v.unsigned_abs())
    } else {
        format!("{v:x}")
    }
}

/// A typed attribute value.
///
/// Each variant is one of the value types of the wire format. Use
/// [`as_str`](Self::as_str) to render any variant as text, or a typed accessor
/// such as [`as_int`](Self::as_int) to read a specific one.
///
/// # Examples
///
/// ```
/// use android_abx::AttributeValue;
///
/// let value = AttributeValue::Int(42);
/// assert_eq!(value.as_int(), Some(42));
/// assert_eq!(value.as_bool(), None);
/// assert_eq!(value.as_str(), "42");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    /// No value.
    Null,
    /// A string.
    String(String),
    /// Bytes, rendered as lowercase hex.
    BytesHex(Vec<u8>),
    /// Bytes, rendered as Base64.
    BytesBase64(Vec<u8>),
    /// A 32-bit integer, rendered in decimal.
    Int(i32),
    /// A 32-bit integer, rendered in signed hex.
    IntHex(u32),
    /// A 64-bit integer, rendered in decimal.
    Long(i64),
    /// A 64-bit integer, rendered in signed hex.
    LongHex(u64),
    /// A 32-bit float.
    Float(f32),
    /// A 64-bit float.
    Double(f64),
    /// A boolean.
    Boolean(bool),
}

impl AttributeValue {
    /// Renders the value as text, the same way AOSP's `BinaryXmlPullParser` does.
    ///
    /// - `Null` is the empty string.
    /// - Bytes are lowercase hex or standard Base64.
    /// - `IntHex` and `LongHex` are signed, like Java's `Integer.toString(v, 16)`:
    ///   `0xCAFEBABE` renders as `-35014542`.
    /// - Whole `Float` and `Double` values keep a `.0` suffix.
    ///
    /// The result is not XML-escaped.
    ///
    /// # Examples
    ///
    /// ```
    /// use android_abx::AttributeValue;
    ///
    /// assert_eq!(AttributeValue::IntHex(0xff).as_str(), "ff");
    /// assert_eq!(AttributeValue::IntHex(0xCAFEBABE).as_str(), "-35014542");
    /// assert_eq!(AttributeValue::Double(1.0).as_str(), "1.0");
    /// assert_eq!(AttributeValue::BytesHex(vec![0xab, 0x01]).as_str(), "ab01");
    /// ```
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

    /// Returns the string if this is [`String`](Self::String), otherwise `None`.
    pub fn as_string(&self) -> Option<&str> {
        if let AttributeValue::String(s) = self {
            Some(s)
        } else {
            None
        }
    }
    /// Returns the value if this is [`Int`](Self::Int), otherwise `None`.
    pub fn as_int(&self) -> Option<i32> {
        if let AttributeValue::Int(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    /// Returns the value if this is [`IntHex`](Self::IntHex), otherwise `None`.
    pub fn as_int_hex(&self) -> Option<u32> {
        if let AttributeValue::IntHex(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    /// Returns the value if this is [`Long`](Self::Long), otherwise `None`.
    pub fn as_long(&self) -> Option<i64> {
        if let AttributeValue::Long(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    /// Returns the value if this is [`LongHex`](Self::LongHex), otherwise `None`.
    pub fn as_long_hex(&self) -> Option<u64> {
        if let AttributeValue::LongHex(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    /// Returns the value if this is [`Float`](Self::Float), otherwise `None`.
    pub fn as_float(&self) -> Option<f32> {
        if let AttributeValue::Float(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    /// Returns the value if this is [`Double`](Self::Double), otherwise `None`.
    pub fn as_double(&self) -> Option<f64> {
        if let AttributeValue::Double(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    /// Returns the value if this is [`Boolean`](Self::Boolean), otherwise `None`.
    pub fn as_bool(&self) -> Option<bool> {
        if let AttributeValue::Boolean(b) = self {
            Some(*b)
        } else {
            None
        }
    }
    /// Returns the bytes if this is [`BytesHex`](Self::BytesHex) or
    /// [`BytesBase64`](Self::BytesBase64), otherwise `None`.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            AttributeValue::BytesHex(b) | AttributeValue::BytesBase64(b) => Some(b),
            _ => None,
        }
    }
}

/// A tag or attribute name.
///
/// An alias of [`SmolStr`](smol_str::SmolStr): names up to 23 bytes are stored
/// inline, and cloning never allocates.
pub type InternedStr = smol_str::SmolStr;

/// An attribute: a name and a typed value.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    /// The attribute name.
    pub name: InternedStr,
    /// The attribute value.
    pub value: AttributeValue,
}

impl Attribute {
    /// Renders the value as text. See [`AttributeValue::as_str`].
    pub fn as_str(&self) -> std::borrow::Cow<'_, str> {
        self.value.as_str()
    }
}

/// A parse event, one per token in the document.
///
/// The parsers produce events, and [`AbxWriter`](crate::AbxWriter) encodes them.
/// The variants follow the token types of Java's `XmlPullParser`.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Start of the document.
    StartDocument,
    /// End of the document.
    EndDocument,
    /// An opening tag with its attributes.
    StartTag {
        /// The element's tag name.
        name: InternedStr,
        /// Attributes in document order.
        attributes: Vec<Attribute>,
    },
    /// A closing tag.
    EndTag {
        /// The element's tag name, matching the corresponding [`Event::StartTag`].
        name: InternedStr,
    },
    /// Character data, not escaped.
    Text(String),
    /// The content of a `<![CDATA[...]]>` section.
    CdataSection(String),
    /// The content of a `<!--...-->` comment.
    Comment(String),
    /// The content of a `<?...?>` processing instruction.
    ProcessingInstruction(String),
    /// An entity reference by name: `"amp"` for `&amp;`, not the resolved `&`.
    EntityReference(String),
    /// Whitespace marked as ignorable.
    IgnorableWhitespace(String),
    /// The content of a `<!DOCTYPE ...>` declaration.
    DocDecl(String),
}

pub(crate) fn xml_escape(s: &str) -> std::borrow::Cow<'_, str> {
    match s.find(['<', '>', '&', '"', '\'']) {
        None => std::borrow::Cow::Borrowed(s),
        Some(first) => {
            let mut out = String::with_capacity(s.len() + 8);
            out.push_str(&s[..first]);
            for c in s[first..].chars() {
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
        }
    }
}

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
