/// The error type for every fallible operation in this crate.
#[derive(Debug, thiserror::Error)]
pub enum AbxError {
    /// The input does not start with [`MAGIC`](crate::MAGIC).
    #[error("invalid magic header: expected {expected:?}, got {actual:?}")]
    InvalidMagic {
        /// Expected magic.
        expected: [u8; 4],
        /// Magic found.
        actual: [u8; 4],
    },
    /// The input ended in the middle of a token. The string names what was being read.
    #[error("unexpected end of input while reading {0}")]
    UnexpectedEof(&'static str),
    /// An interned-string reference points past the strings read so far.
    #[error("invalid interned string index {0}")]
    BadInternedIndex(u16),
    /// String is not valid UTF-8.
    #[error("invalid UTF-8 in string")]
    InvalidUtf8,
    /// An attribute has an unknown type nibble.
    #[error("unknown attribute type 0x{0:02X}")]
    UnknownAttributeType(u8),
    /// A token has an unknown command nibble.
    #[error("unknown command 0x{0:02X}")]
    UnknownCommand(u8),
    /// A string or byte value is longer than the 65,535 bytes the format allows.
    #[error("value too long: {len} bytes exceeds maximum of {max}")]
    ValueTooLong {
        /// Actual length in bytes.
        len: usize,
        /// Maximum length (`u16::MAX`).
        max: usize,
    },
    /// Low-level parse failure.
    #[error("nom parse error: {0}")]
    Nom(String),
    /// Reading or writing failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Deserializing into a serde type failed.
    #[error("deserialization error: {0}")]
    Deserialization(String),
    /// The XML text passed to `xml_to_abx` is malformed.
    #[error("XML parse error: {0}")]
    Xml(String),
}

/// A [`Result`](std::result::Result) with [`AbxError`] as the error type.
pub type Result<T> = std::result::Result<T, AbxError>;
