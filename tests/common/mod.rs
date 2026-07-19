//! Shared helpers for building synthetic ABX blobs by hand, used across the
//! integration test binaries (matching the AOSP wire format so tests are
//! self-contained with no binary fixtures required).
#![allow(dead_code)]

use abx::MAGIC;

pub fn u16_be(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}
pub fn i32_be(v: i32) -> [u8; 4] {
    v.to_be_bytes()
}
pub fn i64_be(v: i64) -> [u8; 8] {
    v.to_be_bytes()
}
pub fn f32_be(v: f32) -> [u8; 4] {
    v.to_be_bytes()
}
pub fn f64_be(v: f64) -> [u8; 8] {
    v.to_be_bytes()
}

/// Write a length-prefixed UTF-8 string (ABX "UTF" encoding).
pub fn utf(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut out = u16_be(bytes.len() as u16).to_vec();
    out.extend_from_slice(bytes);
    out
}

/// Write an interned string reference (new = 0xFFFF + utf).
pub fn interned_new(s: &str) -> Vec<u8> {
    let mut out = u16_be(0xFFFF).to_vec();
    out.extend_from_slice(&utf(s));
    out
}

/// Write an interned string back-reference.
pub fn interned_ref(idx: u16) -> Vec<u8> {
    u16_be(idx).to_vec()
}

/// Prepend the MAGIC header.
pub fn with_magic(body: &[u8]) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(body);
    out
}

pub const CMD_START_DOCUMENT: u8 = 0x00;
pub const CMD_END_DOCUMENT: u8 = 0x01;
pub const CMD_START_TAG: u8 = 0x02;
pub const CMD_END_TAG: u8 = 0x03;
pub const CMD_TEXT: u8 = 0x04;
pub const CMD_CDSECT: u8 = 0x05;
pub const CMD_ENTITY_REF: u8 = 0x06;
pub const CMD_IGNORABLE_WHITESPACE: u8 = 0x07;
pub const CMD_PROCESSING_INSTRUCTION: u8 = 0x08;
pub const CMD_COMMENT: u8 = 0x09;
pub const CMD_DOCDECL: u8 = 0x0A;
pub const CMD_ATTRIBUTE: u8 = 0x0F;

// Matches AOSP's BinaryXmlSerializer.java exactly: `n << 4` for n = 1..=13.
pub const TYPE_NULL: u8 = 0x10;
pub const TYPE_STRING: u8 = 0x20;
pub const TYPE_STRING_INTERNED: u8 = 0x30;
pub const TYPE_BYTES_HEX: u8 = 0x40;
pub const TYPE_BYTES_BASE64: u8 = 0x50;
pub const TYPE_INT: u8 = 0x60;
pub const TYPE_INT_HEX: u8 = 0x70;
pub const TYPE_LONG: u8 = 0x80;
pub const TYPE_LONG_HEX: u8 = 0x90;
pub const TYPE_FLOAT: u8 = 0xA0;
pub const TYPE_DOUBLE: u8 = 0xB0;
pub const TYPE_BOOLEAN_TRUE: u8 = 0xC0;
pub const TYPE_BOOLEAN_FALSE: u8 = 0xD0;

// ---------------------------------------------------------------------------
// Higher-level element/attribute/document builders
// ---------------------------------------------------------------------------

pub fn start_tag(name: &str) -> Vec<u8> {
    let mut out = vec![TYPE_STRING_INTERNED | CMD_START_TAG];
    out.extend(interned_new(name));
    out
}

pub fn end_tag(name: &str) -> Vec<u8> {
    let mut out = vec![TYPE_STRING_INTERNED | CMD_END_TAG];
    out.extend(interned_new(name));
    out
}

pub fn text(s: &str) -> Vec<u8> {
    let mut out = vec![TYPE_STRING | CMD_TEXT];
    out.extend(utf(s));
    out
}

/// Generic text-bearing token: `TYPE_STRING` + length-prefixed UTF-8, even
/// for an empty string — what `AbxWriter` actually emits.
pub fn text_token(cmd: u8, s: &str) -> Vec<u8> {
    let mut out = vec![TYPE_STRING | cmd];
    out.extend(utf(s));
    out
}

/// The `TYPE_NULL` form of a text-bearing token — never emitted by
/// `AbxWriter` (see `text_token`), but still valid, decodable wire data
/// (this crate's own decoder handles it, only real AOSP's parser doesn't).
pub fn null_text_token(cmd: u8) -> Vec<u8> {
    vec![TYPE_NULL | cmd]
}

pub fn attr_string(name: &str, value: &str) -> Vec<u8> {
    let mut out = vec![TYPE_STRING | CMD_ATTRIBUTE];
    out.extend(interned_new(name));
    out.extend(utf(value));
    out
}

pub fn attr_int(name: &str, value: i32) -> Vec<u8> {
    let mut out = vec![TYPE_INT | CMD_ATTRIBUTE];
    out.extend(interned_new(name));
    out.extend(i32_be(value));
    out
}

pub fn attr_int_hex(name: &str, value: u32) -> Vec<u8> {
    let mut out = vec![TYPE_INT_HEX | CMD_ATTRIBUTE];
    out.extend(interned_new(name));
    out.extend(i32_be(value as i32));
    out
}

pub fn attr_long(name: &str, value: i64) -> Vec<u8> {
    let mut out = vec![TYPE_LONG | CMD_ATTRIBUTE];
    out.extend(interned_new(name));
    out.extend(i64_be(value));
    out
}

pub fn attr_long_hex(name: &str, value: u64) -> Vec<u8> {
    let mut out = vec![TYPE_LONG_HEX | CMD_ATTRIBUTE];
    out.extend(interned_new(name));
    out.extend(i64_be(value as i64));
    out
}

pub fn attr_float(name: &str, value: f32) -> Vec<u8> {
    let mut out = vec![TYPE_FLOAT | CMD_ATTRIBUTE];
    out.extend(interned_new(name));
    out.extend(f32_be(value));
    out
}

pub fn attr_double(name: &str, value: f64) -> Vec<u8> {
    let mut out = vec![TYPE_DOUBLE | CMD_ATTRIBUTE];
    out.extend(interned_new(name));
    out.extend(f64_be(value));
    out
}

pub fn attr_bool(name: &str, value: bool) -> Vec<u8> {
    let ty = if value { TYPE_BOOLEAN_TRUE } else { TYPE_BOOLEAN_FALSE };
    let mut out = vec![ty | CMD_ATTRIBUTE];
    out.extend(interned_new(name));
    out
}

pub fn attr_null(name: &str) -> Vec<u8> {
    let mut out = vec![TYPE_NULL | CMD_ATTRIBUTE];
    out.extend(interned_new(name));
    out
}

pub fn attr_bytes_hex(name: &str, value: &[u8]) -> Vec<u8> {
    let mut out = vec![TYPE_BYTES_HEX | CMD_ATTRIBUTE];
    out.extend(interned_new(name));
    out.extend(u16_be(value.len() as u16));
    out.extend_from_slice(value);
    out
}

pub fn attr_bytes_base64(name: &str, value: &[u8]) -> Vec<u8> {
    let mut out = vec![TYPE_BYTES_BASE64 | CMD_ATTRIBUTE];
    out.extend(interned_new(name));
    out.extend(u16_be(value.len() as u16));
    out.extend_from_slice(value);
    out
}

/// Assemble a full ABX document: magic header, `StartDocument`, the given
/// pre-built event byte sequences concatenated in order, then `EndDocument`.
pub fn document(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut body = vec![CMD_START_DOCUMENT | TYPE_NULL];
    for p in parts {
        body.extend_from_slice(p);
    }
    body.push(CMD_END_DOCUMENT | TYPE_NULL);
    with_magic(&body)
}
