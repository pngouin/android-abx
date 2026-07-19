#![allow(dead_code)]
//! Shared synthetic-data generator for the criterion benches in this
//! directory, reusing the same wire-format builders as the integration
//! tests (`tests/common/mod.rs`) rather than keeping a second copy.

#[path = "../../tests/common/mod.rs"]
mod wire;
pub use wire::*;

/// A synthetic ABX document with `n` repeated `<pkg>` elements, each
/// carrying a string, an int, and a bool attribute — roughly the shape of
/// a real AOSP `packages.xml` record (see `tests/fixtures/simple_pkg.xml`,
/// generated from real data via the local `xml2abx` tool).
pub fn synthetic_document(n: usize) -> Vec<u8> {
    let mut parts = Vec::with_capacity(n * 5);
    for i in 0..n {
        parts.push(start_tag("pkg"));
        parts.push(attr_string("name", &format!("com.example.app{i}")));
        parts.push(attr_int("version", i as i32));
        parts.push(attr_bool("enabled", i % 2 == 0));
        parts.push(end_tag("pkg"));
    }
    document(&parts)
}
