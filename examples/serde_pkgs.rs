//! `serde_pkgs` — stream ABX elements straight into typed structs via serde,
//! instead of walking raw `Event`s by hand.
//!
//! Usage:
//!   cargo run --features serialize --example serde_pkgs -- <input.abx> [element-name]
//!
//! Adapt the `Pkg` struct below (field names, `#[serde(rename = "...")]`,
//! `Option<T>` for attributes that aren't always present) to whatever
//! attributes your `.abx` file's elements actually carry.
//!
//! This is for a *wrapper* document — many repeated `<element>`s under some
//! outer root. If your whole document is a single record instead, see
//! `serde_root` (`abx::from_file`) — much less code for that shape.

use std::{env, process};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Pkg {
    name: String,
    // Not every <pkg> is guaranteed to carry a version, so this is
    // Option<T> rather than a plain i32 — a missing attribute deserializes
    // to None instead of erroring.
    version: Option<i32>,
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("Usage: serde_pkgs <input.abx> [element-name=pkg]");
        process::exit(if args.is_empty() { 1 } else { 0 });
    }

    let element = args.get(1).map(String::as_str).unwrap_or("pkg");

    let mut parser = abx::open_file(&args[0]).unwrap_or_else(|e| {
        eprintln!("Error opening '{}': {e}", args[0]);
        process::exit(1);
    });

    // deserialize_iter is lazy: each <element> is parsed and dropped as we
    // go, so this holds one Pkg at a time in memory regardless of file size.
    let mut count = 0;
    for result in parser.deserialize_iter::<Pkg>(element) {
        match result {
            Ok(pkg) => {
                match pkg.version {
                    Some(v) => println!("{} (version {v})", pkg.name),
                    None => println!("{} (no version)", pkg.name),
                }
                count += 1;
            }
            Err(e) => {
                eprintln!("Parse error: {e}");
                process::exit(1);
            }
        }
    }
    eprintln!("({count} <{element}> element(s))");
}
