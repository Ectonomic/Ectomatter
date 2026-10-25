//! The design tokens (design/ectomatter.tokens.json), as the build writes them
//! out for what Ectomatter lays out and draws in Rust: the Library's pages,
//! Settings', the grounds its pictures are drawn on. They hold the values the
//! window's Slint globals hold (`Space`, `Ink`), from the same file.

include!(concat!(env!("OUT_DIR"), "/tokens.rs"));
