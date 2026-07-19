//! Wire-format protocol constants (mirrors AOSP's `BinaryXmlSerializer.java`
//! exactly — see `CLAUDE.md`'s "FIXED" section for how the `TYPE_*` values
//! were verified). Shared by both parsers (`src/parser.rs`, `src/stream.rs`).

/// Magic header bytes: `ABX\0`
pub const MAGIC: [u8; 4] = [0x41, 0x42, 0x58, 0x00];

/// Sentinel in the interned-string index that signals a new string.
pub(crate) const INTERNED_NEW: u16 = 0xFFFF;

// Token command (low nibble)
pub(crate) const CMD_START_DOCUMENT: u8 = 0x00;
pub(crate) const CMD_END_DOCUMENT: u8 = 0x01;
pub(crate) const CMD_START_TAG: u8 = 0x02;
pub(crate) const CMD_END_TAG: u8 = 0x03;
pub(crate) const CMD_TEXT: u8 = 0x04;
pub(crate) const CMD_CDSECT: u8 = 0x05;
pub(crate) const CMD_ENTITY_REF: u8 = 0x06;
pub(crate) const CMD_IGNORABLE_WHITESPACE: u8 = 0x07;
pub(crate) const CMD_PROCESSING_INSTRUCTION: u8 = 0x08;
pub(crate) const CMD_COMMENT: u8 = 0x09;
pub(crate) const CMD_DOCDECL: u8 = 0x0A;
pub(crate) const CMD_ATTRIBUTE: u8 = 0x0F;

// Data type (high nibble). Values match AOSP's BinaryXmlSerializer.java
// exactly: `n << 4` for n = 1..=13 (high-nibble 0x00 is never used on the
// wire — every token always OR's in an explicit type flag, even the
// "absent value" case, TYPE_NULL).
pub(crate) const TYPE_NULL: u8 = 0x10;
pub(crate) const TYPE_STRING: u8 = 0x20;
pub(crate) const TYPE_STRING_INTERNED: u8 = 0x30;
pub(crate) const TYPE_BYTES_HEX: u8 = 0x40;
pub(crate) const TYPE_BYTES_BASE64: u8 = 0x50;
pub(crate) const TYPE_INT: u8 = 0x60;
pub(crate) const TYPE_INT_HEX: u8 = 0x70;
pub(crate) const TYPE_LONG: u8 = 0x80;
pub(crate) const TYPE_LONG_HEX: u8 = 0x90;
pub(crate) const TYPE_FLOAT: u8 = 0xA0;
pub(crate) const TYPE_DOUBLE: u8 = 0xB0;
pub(crate) const TYPE_BOOLEAN_TRUE: u8 = 0xC0;
pub(crate) const TYPE_BOOLEAN_FALSE: u8 = 0xD0;
