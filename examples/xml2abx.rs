//! `xml2abx` — command-line tool built on top of the `abx` library, the
//! reverse of the `abx2xml` example.
//!
//! Usage:
//!   xml2abx <input.xml>            → writes ABX bytes to stdout
//!   xml2abx <input.xml> <out.abx>  → writes to file
//!   echo … | xml2abx -             → reads from stdin

use std::{
    fs,
    io::{self, Read, Write},
    process,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("Usage: xml2abx <input.xml|-> [output.abx|-]");
        process::exit(if args.is_empty() { 1 } else { 0 });
    }

    let xml = if args[0] == "-" {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf).unwrap_or_else(|e| {
            eprintln!("Error reading stdin: {e}");
            process::exit(1);
        });
        buf
    } else {
        fs::read_to_string(&args[0]).unwrap_or_else(|e| {
            eprintln!("Error reading '{}': {e}", args[0]);
            process::exit(1);
        })
    };

    let data = abx::xml_to_abx(&xml).unwrap_or_else(|e| {
        eprintln!("Encode error: {e}");
        process::exit(1);
    });

    if args.get(1).map(|s| s.as_str()).unwrap_or("-") == "-" {
        io::stdout().write_all(&data).ok();
    } else {
        fs::write(&args[1], &data).unwrap_or_else(|e| {
            eprintln!("Error writing '{}': {e}", args[1]);
            process::exit(1);
        });
    }
}
