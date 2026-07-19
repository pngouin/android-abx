//! `abx2xml` — command-line tool built on top of the `abx` library.
//!
//! Usage:
//!   abx2xml <input.abx>            → prints XML to stdout
//!   abx2xml <input.abx> <out.xml>  → writes to file
//!   echo … | abx2xml -             → reads from stdin

use std::{
    fs,
    io::{self, Read, Write},
    process,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("Usage: abx2xml <input.abx|-> [output.xml|-]");
        process::exit(if args.is_empty() { 1 } else { 0 });
    }

    let data = if args[0] == "-" {
        let mut buf = Vec::new();
        io::stdin().read_to_end(&mut buf).unwrap_or_else(|e| {
            eprintln!("Error reading stdin: {e}");
            process::exit(1);
        });
        buf
    } else {
        fs::read(&args[0]).unwrap_or_else(|e| {
            eprintln!("Error reading '{}': {e}", args[0]);
            process::exit(1);
        })
    };

    let xml = abx::abx_to_xml(&data).unwrap_or_else(|e| {
        eprintln!("Parse error: {e}");
        process::exit(1);
    });

    if args.get(1).map(|s| s.as_str()).unwrap_or("-") == "-" {
        io::stdout().write_all(xml.as_bytes()).ok();
    } else {
        fs::write(&args[1], xml.as_bytes()).unwrap_or_else(|e| {
            eprintln!("Error writing '{}': {e}", args[1]);
            process::exit(1);
        });
    }
}
