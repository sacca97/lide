//! Text encoding and line-ending helpers.
//!
//! Editing, undo, selections and cursor movement belong to Rune's
//! `TextEdit`, which the editor uses directly. What remains here is the part
//! the editor needs around it: decoding a file as UTF-8 (with or without a
//! byte order mark), remembering the line ending it used, and writing it back
//! the same way.
//!
//! Offsets exposed by this module are byte offsets into the UTF-8 text.

pub mod buffer;

pub use buffer::{Encoding, LineEnding, TextBuffer};
