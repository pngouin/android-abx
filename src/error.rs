//! [`AbxError`] — this crate's single error type — and the [`Result`] alias
//! built on it.

#[derive(Debug, thiserror::Error)]
pub enum AbxError {
    #[error("invalid magic header: expected {expected:?}, got {actual:?}")]
    InvalidMagic {
        expected: [u8; 4],
        actual: [u8; 4],
    },
    #[error("unexpected end of input while reading {0}")]
    UnexpectedEof(&'static str),
    #[error("invalid interned string index {0}")]
    BadInternedIndex(u16),
    #[error("invalid UTF-8 in string")]
    InvalidUtf8,
    #[error("unknown attribute type 0x{0:02X}")]
    UnknownAttributeType(u8),
    #[error("unknown command 0x{0:02X}")]
    UnknownCommand(u8),
    #[error("nom parse error: {0}")]
    Nom(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("deserialization error: {0}")]
    Deserialization(String),
}

impl<I: std::fmt::Debug> From<nom::Err<nom::error::Error<I>>> for AbxError {
    fn from(e: nom::Err<nom::error::Error<I>>) -> Self {
        AbxError::Nom(format!("{:?}", e))
    }
}

pub type Result<T> = std::result::Result<T, AbxError>;
