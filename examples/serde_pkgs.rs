//! Usage: cargo run --features serde --example serde_pkgs -- <input.abx> [element-name]

use std::{env, process};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Pkg {
    name: String,
    version: Option<i32>,
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("Usage: serde_pkgs <input.abx> [element-name=pkg]");
        process::exit(if args.is_empty() { 1 } else { 0 });
    }

    let element = args.get(1).map(String::as_str).unwrap_or("pkg");

    let mut parser = android_abx::open_file(&args[0]).unwrap_or_else(|e| {
        eprintln!("Error opening '{}': {e}", args[0]);
        process::exit(1);
    });

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
