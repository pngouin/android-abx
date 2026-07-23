//! [`AbxError`] — this crate's single error type — and the [`Result`] alias
//! built on it.

/// Everything that can go wrong parsing, encoding, or deserializing ABX data.
#[derive(Debug, thiserror::Error)]
pub enum AbxError {
    /// The 4-byte `ABX\0` magic header didn't match at the start of the input.
    #[error("invalid magic header: expected {expected:?}, got {actual:?}")]
    InvalidMagic {
        /// The magic bytes this crate expects (`ABX\0`).
        expected: [u8; 4],
        /// The bytes actually found at the start of the input.
        actual: [u8; 4],
    },
    /// The input ended before a complete token or value could be read.
    #[error("unexpected end of input while reading {0}")]
    UnexpectedEof(&'static str),
    /// An interned-string back-reference pointed past the end of the pool.
    #[error("invalid interned string index {0}")]
    BadInternedIndex(u16),
    /// A string's bytes were not valid UTF-8.
    #[error("invalid UTF-8 in string")]
    InvalidUtf8,
    /// An attribute value's type nibble didn't match any known `TYPE_*` constant.
    #[error("unknown attribute type 0x{0:02X}")]
    UnknownAttributeType(u8),
    /// A token's command nibble didn't match any known `CMD_*` constant.
    #[error("unknown command 0x{0:02X}")]
    UnknownCommand(u8),
    /// A string or byte blob was too long to fit the wire format's `u16` length prefix.
    #[error("value too long: {len} bytes exceeds maximum of {max}")]
    ValueTooLong {
        /// The value's actual length in bytes.
        len: usize,
        /// The maximum length the wire format can express (`u16::MAX`, 65,535).
        max: usize,
    },
    /// The underlying `nom` parser failed; the message is `nom`'s own formatted error.
    #[error("nom parse error: {0}")]
    Nom(String),
    /// A read or write on the underlying `Read`/`Write` failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Mapping decoded ABX events onto a `serde` type failed (`serialize` feature).
    #[error("deserialization error: {0}")]
    Deserialization(String),
    /// Parsing XML text failed while encoding it to ABX (`xml` feature).
    #[error("XML parse error: {0}")]
    Xml(String),
}

impl<I: std::fmt::Debug> From<nom::Err<nom::error::Error<I>>> for AbxError {
    fn from(e: nom::Err<nom::error::Error<I>>) -> Self {
        AbxError::Nom(format!("{:?}", e))
    }
}

/// This crate's [`Result`](std::result::Result) alias, with [`AbxError`] as the error type.
pub type Result<T> = std::result::Result<T, AbxError>;
