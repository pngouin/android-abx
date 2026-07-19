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
pub const CMD_ATTRIBUTE: u8 = 0x0F;

pub const TYPE_STRING: u8 = 0x10;
pub const TYPE_STRING_INTERNED: u8 = 0x20;
pub const TYPE_BYTES_HEX: u8 = 0x30;
pub const TYPE_BYTES_BASE64: u8 = 0x40;
pub const TYPE_INT: u8 = 0x50;
pub const TYPE_INT_HEX: u8 = 0x60;
pub const TYPE_LONG: u8 = 0x70;
pub const TYPE_LONG_HEX: u8 = 0x80;
pub const TYPE_FLOAT: u8 = 0x90;
pub const TYPE_DOUBLE: u8 = 0xA0;
pub const TYPE_BOOLEAN_TRUE: u8 = 0xB0;
pub const TYPE_BOOLEAN_FALSE: u8 = 0xC0;
