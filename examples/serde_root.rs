//! `serde_root` — deserialize an ABX document's root element straight into
//! a struct with a single call, for files where the whole document *is*
//! one record (as opposed to a wrapper element containing many repeated
//! records — see `serde_pkgs` for that shape instead).
//!
//! Usage:
//!   cargo run --features serialize --example serde_root -- <input.abx>
//!
//! No parser to construct, and — like `quick_xml::de::from_str`/
//! `serde_json::from_slice` — no need to know or declare the root
//! element's tag name; deserialization is structural (attributes, child
//! elements, text), not name-based. Adapt the `Pkg` struct below to
//! whatever your root element actually carries.

use std::{env, process};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // fields are only ever read via the Debug print below
struct Pkg {
    name: String,
    version: Option<i32>,
    flags: Option<i32>,
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("Usage: serde_root <input.abx>");
        process::exit(if args.is_empty() { 1 } else { 0 });
    }

    let pkg: Pkg = abx::from_file(&args[0]).unwrap_or_else(|e| {
        eprintln!("Error reading '{}': {e}", args[0]);
        process::exit(1);
    });

    println!("{pkg:#?}");
}
