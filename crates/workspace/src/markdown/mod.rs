//! Markdown support for the editor.
//!
//! * [`ast`] and [`parse`] turn Markdown source into a document tree. They
//!   are pure Rust with no UI dependency, so they can be tested without a
//!   display server.
//! * [`html`] exports a document as a standalone HTML file.
//!
//! The live preview is drawn by `rune-kit`, which renders Markdown with
//! tables, task lists, math and Mermaid diagrams. This crate provides the
//! document model, the outline and the HTML export.

pub mod ast;
pub mod html;
pub mod parse;

pub use ast::{Alignment, Block, Inline, ListItem, OutlineEntry};
pub use parse::{default_options, parse, parse_with};

/// The headings of a Markdown document, with the line each is on.
pub fn outline(source: &str) -> Vec<OutlineEntry> {
    parse::outline(source)
}
