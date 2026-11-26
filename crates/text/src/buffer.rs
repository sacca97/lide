//! A UTF-8 document buffer and the metadata needed to write it back.
//!
//! The text is normalised to LF for editing; the file's original line ending
//! and byte order mark are remembered separately and reapplied by
//! [`TextBuffer::to_bytes`].

use std::borrow::Cow;

/// Line terminator style used by a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineEnding {
    /// Unix line feed: `\n`.
    #[default]
    Lf,
    /// Windows carriage return + line feed: `\r\n`.
    CrLf,
    /// Classic Mac carriage return: `\r`.
    Cr,
}

impl LineEnding {
    /// The literal bytes used for this line ending.
    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
            LineEnding::Cr => "\r",
        }
    }

    /// The length of this line ending in bytes.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(self) -> usize {
        self.as_str().len()
    }

    /// Guess the line ending of `text` from its first terminator.
    pub fn detect(text: &str) -> LineEnding {
        let bytes = text.as_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            match byte {
                b'\n' => return LineEnding::Lf,
                b'\r' => {
                    if bytes.get(index + 1) == Some(&b'\n') {
                        return LineEnding::CrLf;
                    }
                    return LineEnding::Cr;
                }
                _ => {}
            }
        }
        LineEnding::default()
    }

    /// Convert LF-only text into text using this line ending.
    pub fn apply(self, text: &str) -> Cow<'_, str> {
        match self {
            LineEnding::Lf => Cow::Borrowed(text),
            LineEnding::CrLf => Cow::Owned(text.replace('\n', "\r\n")),
            LineEnding::Cr => Cow::Owned(text.replace('\n', "\r")),
        }
    }
}

/// The encoding a document is stored in.
///
/// Only UTF-8 (with or without a byte order mark) is supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Encoding {
    /// UTF-8 without a byte order mark.
    #[default]
    Utf8,
    /// UTF-8 with a byte order mark.
    Utf8Bom,
}

impl Encoding {
    /// The byte order mark for this encoding, if any.
    pub fn bom(self) -> &'static [u8] {
        match self {
            Encoding::Utf8 => &[],
            Encoding::Utf8Bom => &[0xEF, 0xBB, 0xBF],
        }
    }
}

/// A text document and how it was stored.
#[derive(Debug, Clone, Default)]
pub struct TextBuffer {
    text: String,
    line_ending: LineEnding,
    encoding: Encoding,
}

impl TextBuffer {
    /// Create an empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a buffer from a string.
    ///
    /// All line endings are normalised to LF; the detected line ending is
    /// remembered so that the document can be written back byte-identically.
    pub fn from_text(text: &str) -> Self {
        let line_ending = LineEnding::detect(text);
        Self {
            text: normalize_newlines(text),
            line_ending,
            encoding: Encoding::default(),
        }
    }

    /// Create a buffer from raw bytes, stripping a UTF-8 byte order mark and
    /// detecting the line ending.
    ///
    /// Returns `None` if the bytes are not valid UTF-8.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let (encoding, bytes) = match bytes.strip_prefix(Encoding::Utf8Bom.bom()) {
            Some(rest) => (Encoding::Utf8Bom, rest),
            None => (Encoding::Utf8, bytes),
        };
        let text = std::str::from_utf8(bytes).ok()?;
        let mut buffer = Self::from_text(text);
        buffer.encoding = encoding;
        Some(buffer)
    }

    /// The line ending used when this document is written to disk.
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// Override the line ending used when this document is written to disk.
    pub fn set_line_ending(&mut self, line_ending: LineEnding) {
        self.line_ending = line_ending;
    }

    /// The encoding of this document.
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// Total length in bytes, excluding any byte order mark.
    pub fn len_bytes(&self) -> usize {
        self.text.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The whole text as a `String`, with LF line endings.
    pub fn to_text(&self) -> String {
        self.text.clone()
    }

    /// Consume the buffer, returning its text without a copy.
    pub fn into_text(self) -> String {
        self.text
    }

    /// The bytes to write to disk for this document.
    pub fn to_bytes(&self) -> Vec<u8> {
        let text = self.line_ending.apply(&self.text);
        let mut bytes = Vec::with_capacity(self.encoding.bom().len() + text.len());
        bytes.extend_from_slice(self.encoding.bom());
        bytes.extend_from_slice(text.as_bytes());
        bytes
    }
}

/// Convert CRLF and lone CR to LF.
pub(crate) fn normalize_newlines(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_endings_are_detected_and_normalised() {
        let buffer = TextBuffer::from_text("one\r\ntwo\rthree\nfour");
        assert_eq!(buffer.line_ending(), LineEnding::CrLf);
        assert_eq!(buffer.to_text(), "one\ntwo\nthree\nfour");
        assert_eq!(buffer.to_bytes(), b"one\r\ntwo\r\nthree\r\nfour".to_vec());
    }

    #[test]
    fn a_bom_is_stripped_and_written_back() {
        let mut bytes = Encoding::Utf8Bom.bom().to_vec();
        bytes.extend_from_slice(b"hello\n");
        let buffer = TextBuffer::from_bytes(&bytes).expect("valid UTF-8");
        assert_eq!(buffer.encoding(), Encoding::Utf8Bom);
        assert_eq!(buffer.to_text(), "hello\n");
        assert_eq!(buffer.to_bytes(), bytes);
    }

    #[test]
    fn invalid_utf8_is_refused() {
        assert!(TextBuffer::from_bytes(&[0xFF, 0xFE, 0x00]).is_none());
    }

    #[test]
    fn an_empty_buffer_is_empty() {
        let buffer = TextBuffer::new();
        assert!(buffer.is_empty());
        assert_eq!(buffer.len_bytes(), 0);
        assert_eq!(buffer.to_text(), "");
        assert_eq!(buffer.to_bytes(), Vec::<u8>::new());
    }
}
