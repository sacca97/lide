//! The source code editor.
//!
//! The editor owns the text of one document. The editing model comes from
//! [`rune_edit::TextEdit`], which brings byte-accurate selections, an undo
//! history, IME composition and the usual key handling with it.
//!
//! What this crate adds is the *document* around the text: where it came
//! from, whether it has unsaved changes, and how to read and write it back.

use std::path::Path;
use std::sync::Arc;

use lide_resource::{LocalStore, Location, Store};
use rune_edit::TextEdit;

/// Errors raised while reading or writing a document.
#[derive(Debug, thiserror::Error)]
pub enum EditorError {
    /// The file could not be read.
    #[error("could not read {path}: {message}")]
    Read {
        /// The location that could not be read.
        path: String,
        /// Why.
        message: String,
    },
    /// The file could not be written.
    #[error("could not write {path}: {message}")]
    Write {
        /// The location that could not be written.
        path: String,
        /// Why.
        message: String,
    },
    /// The file was not valid UTF-8.
    #[error("{path} is not valid UTF-8")]
    NotUtf8 {
        /// The offending location.
        path: String,
    },
}

/// An open document.
#[derive(Debug, Clone)]
pub struct Editor {
    /// The text and the cursor.
    pub edit: TextEdit,
    /// Where the document lives.
    pub location: Option<Location>,
    /// The text as it was last written to disk or read from it.
    saved_text: String,
    /// The line ending the file was read with.
    line_ending: lide_text::LineEnding,
    /// The encoding the file was read with.
    encoding: lide_text::Encoding,
    /// The store that reads and writes the location.
    store: Arc<dyn Store>,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            edit: TextEdit::new(""),
            location: None,
            saved_text: String::new(),
            line_ending: lide_text::LineEnding::Lf,
            encoding: lide_text::Encoding::Utf8,
            store: Arc::new(LocalStore),
        }
    }
}

impl Editor {
    /// An empty document.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Open a file.
    ///
    /// The line ending and the byte order mark are preserved: the text is
    /// normalised to LF for editing and written back the way it came.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, EditorError> {
        Self::open_with(Arc::new(LocalStore), Location::local(path.as_ref()))
    }

    /// Open a document through a store.
    pub fn open_with(
        store: Arc<dyn Store>,
        location: Location,
    ) -> Result<Self, EditorError> {
        let bytes = store.read(&location).map_err(|error| EditorError::Read {
            path: location.to_string(),
            message: error.to_string(),
        })?;
        let buffer = lide_text::TextBuffer::from_bytes(&bytes).ok_or_else(|| {
            EditorError::NotUtf8 {
                path: location.to_string(),
            }
        })?;
        let line_ending = buffer.line_ending();
        let encoding = buffer.encoding();
        let mut edit = TextEdit::new(buffer.into_text());
        // Start at the top of the file rather than at its end.
        edit.set_caret(0);
        Ok(Self {
            saved_text: edit.text.clone(),
            edit,
            location: Some(location),
            line_ending,
            encoding,
            store,
        })
    }

    /// The title shown on the tab.
    pub fn title(&self) -> String {
        self.location
            .as_ref()
            .and_then(Location::file_name)
            .map(|name| name.to_string())
            .unwrap_or_else(|| "untitled".to_string())
    }

    /// The text of the document.
    pub fn text(&self) -> &str {
        &self.edit.text
    }

    /// Whether the document has unsaved changes.
    ///
    /// This compares against the text that was last read or written, so
    /// undoing back to the saved state also clears the marker.
    pub fn dirty(&self) -> bool {
        self.edit.text != self.saved_text
    }

    /// The line the cursor is on, counted from one.
    pub fn cursor_line(&self) -> usize {
        self.edit.line_of_byte(self.edit.focus) + 1
    }

    /// The column the cursor is on, counted from one.
    pub fn cursor_column(&self) -> usize {
        let line = self.edit.line_of_byte(self.edit.focus);
        let start = self.edit.line_range(line).start;
        // Count characters, not bytes: a column is a character position.
        self.edit.text[start..self.edit.focus].chars().count() + 1
    }

    /// Put the caret at `line` (counted from one) and `column` bytes into it.
    ///
    /// Search results and diagnostics both arrive as a line and a byte offset
    /// within it.
    pub fn go_to(&mut self, line: u64, column: usize) {
        if line == 0 {
            return;
        }
        let mut offset = 0usize;
        for (index, text) in self.edit.text.split_inclusive('\n').enumerate() {
            if index as u64 + 1 == line {
                offset += column.min(text.trim_end_matches('\n').len());
                break;
            }
            offset += text.len();
        }
        let offset = offset.min(self.edit.text.len());
        if self.edit.text.is_char_boundary(offset) {
            self.edit.set_caret(offset);
            self.edit.set_selection(offset, offset);
        }
    }

    /// The number of lines and the longest line's character count.
    pub fn line_stats(&self) -> (usize, usize) {
        line_stats(&self.edit.text)
    }

    /// The number of lines in the document.
    pub fn line_count(&self) -> usize {
        self.line_stats().0
    }

    /// Write the document back to disk.
    pub fn save(&mut self) -> Result<(), EditorError> {
        let Some(location) = self.location.clone() else {
            return Ok(());
        };
        let bytes = self.bytes();
        self.store
            .write(&location, &bytes)
            .map_err(|error| EditorError::Write {
                path: location.to_string(),
                message: error.to_string(),
            })?;
        self.saved_text = self.edit.text.clone();
        Ok(())
    }

    /// Reload the document from disk, discarding unsaved changes.
    pub fn reload(&mut self) -> Result<(), EditorError> {
        let Some(location) = self.location.clone() else {
            return Ok(());
        };
        let reloaded = Self::open_with(self.store.clone(), location)?;
        self.edit = reloaded.edit;
        self.saved_text = reloaded.saved_text;
        self.line_ending = reloaded.line_ending;
        self.encoding = reloaded.encoding;
        Ok(())
    }

    /// The bytes to write to disk.
    ///
    /// The line ending of the file is remembered when it is opened and
    /// reapplied here, so a file is written back byte for byte.
    fn bytes(&self) -> Vec<u8> {
        let text = self.line_ending.apply(&self.edit.text);
        let mut bytes = Vec::with_capacity(self.encoding.bom().len() + text.len());
        bytes.extend_from_slice(self.encoding.bom());
        bytes.extend_from_slice(text.as_bytes());
        bytes
    }

    /// The local path, when this is a local document.
    pub fn local_path(&self) -> Option<&Path> {
        self.location.as_ref().and_then(Location::as_local)
    }
}

/// Measure a document once for the gutter.
fn line_stats(text: &str) -> (usize, usize) {
    let mut lines = 0;
    let mut longest = 0;
    for line in text.lines() {
        lines += 1;
        longest = longest.max(line.chars().count());
    }
    (lines.max(1), longest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_document_has_one_line() {
        let editor = Editor::empty();
        assert_eq!(editor.line_count(), 1);
        assert_eq!(editor.cursor_line(), 1);
        assert_eq!(editor.cursor_column(), 1);
    }

    #[test]
    fn the_cursor_position_is_counted_in_characters() {
        let mut editor = Editor::empty();
        editor.edit = TextEdit::new("héllo");
        // Byte 3 is after the two-byte "é", so it is the third character,
        // not the fourth.
        editor.edit.set_caret(3);
        assert_eq!(editor.cursor_line(), 1);
        assert_eq!(editor.cursor_column(), 3);
    }

    #[test]
    fn saving_writes_the_file_back() {
        let directory = std::env::temp_dir().join("lide-editor-test");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("sample.txt");
        std::fs::write(&path, "hello\n").unwrap();

        let mut editor = Editor::open(&path).unwrap();
        assert_eq!(editor.text(), "hello\n");
        editor.edit.replace_text("goodbye\n");
        assert!(editor.dirty());
        editor.save().unwrap();
        assert!(!editor.dirty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "goodbye\n");

        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn saving_keeps_crlf_line_endings() {
        let directory = std::env::temp_dir().join("lide-editor-test-crlf");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("sample.txt");
        std::fs::write(&path, "one\r\ntwo\r\n").unwrap();

        let mut editor = Editor::open(&path).unwrap();
        assert_eq!(editor.text(), "one\ntwo\n");
        editor.edit.replace_text("three\nfour\n");
        editor.save().unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "three\r\nfour\r\n"
        );

        std::fs::remove_dir_all(&directory).ok();
    }
}
