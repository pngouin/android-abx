//! Usage: cargo run --features serialize --example serde_root -- <input.abx>

use std::{env, process};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
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

    let pkg: Pkg = android_abx::from_file(&args[0]).unwrap_or_else(|e| {
        eprintln!("Error reading '{}': {e}", args[0]);
        process::exit(1);
    });

    println!("{pkg:#?}");
}
