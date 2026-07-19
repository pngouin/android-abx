//! The two decoding backends for the ABX wire format: [`slice`] (in-memory,
//! `&[u8]`) and [`stream`] (any `impl std::io::Read`).
//!
//! Deliberately not unified behind a shared trait: `slice`'s and `stream`'s
//! `match` arms are structurally duplicated so each can use its own buffer
//! strategy (`nom::number::complete::*` vs `nom::number::streaming::*`) —
//! see "Two parsers, one wire format" in `CLAUDE.md`.

mod slice;
pub use slice::{AbxParser, AbxParserOwned};

pub mod stream;
