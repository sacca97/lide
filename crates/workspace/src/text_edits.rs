//! Working with a server's edits and a caret's position.
//!
//! A language server speaks in lines and UTF-16 characters; the editor works
//! in bytes. These helpers translate between the two, apply a batch of edits
//! without disturbing the offsets that follow them, and find the word around
//! a caret — which is what a rename prefills.

/// Apply a server's edits to `text`.
///
/// The edits arrive in document order; they are applied from the end so that
/// earlier offsets stay valid. An edit that overlaps one already applied, or
/// that points outside the text, is dropped rather than corrupting the file.
pub(crate) fn apply_text_edits(text: &str, edits: &[lide_lsp::client::ServerTextEdit]) -> String {
    let lines = text.lines().count().max(1) as u64;
    let mut offsets: Vec<(usize, usize, &str)> = Vec::with_capacity(edits.len());
    for edit in edits {
        // A position past the last line is a server bug; clamping it into an
        // append would quietly move text, so the edit is dropped.
        if edit.start.line >= lines || edit.end.line >= lines {
            continue;
        }
        let start = offset_of(text, edit.start.line, edit.start.character);
        let end = offset_of(text, edit.end.line, edit.end.character);
        offsets.push((start, end, edit.new_text.as_str()));
    }
    offsets.sort_by_key(|(start, _, _)| *start);

    let mut out = String::with_capacity(text.len() + 64);
    let mut last = 0usize;
    for (start, end, new_text) in offsets {
        if start < last || end < start || end > text.len() || !text.is_char_boundary(start) {
            continue;
        }
        out.push_str(&text[last..start]);
        out.push_str(new_text);
        last = end;
    }
    out.push_str(&text[last..]);
    out
}

/// The identifier around `caret`, for prefilling a rename.
pub(crate) fn word_at(text: &str, caret: usize) -> String {
    let caret = caret.min(text.len());
    let mut start = caret;
    for (index, ch) in text[..caret].char_indices().rev() {
        if ch.is_alphanumeric() || ch == '_' {
            start = index;
        } else {
            break;
        }
    }
    let mut end = caret;
    for (index, ch) in text[caret..].char_indices() {
        if ch.is_alphanumeric() || ch == '_' {
            end = caret + index + ch.len_utf8();
        } else {
            break;
        }
    }
    text[start..end].to_string()
}

/// The byte offset of an LSP position in `text`.
///
/// LSP counts lines from zero and characters in UTF-16 code units, which is
/// not the same as bytes whenever the text is not plain ASCII.
pub(crate) fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = Vec::with_capacity(text.lines().count() + 1);
    starts.push(0);
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

/// The byte offset of a UTF-16 `(line, character)` position, with line starts
/// already computed.
pub(crate) fn offset_at_line(text: &str, starts: &[usize], line: u64, character: u64) -> usize {
    let start = starts
        .get(line as usize)
        .copied()
        .unwrap_or(text.len());
    let end = text[start..]
        .find('\n')
        .map(|index| start + index)
        .unwrap_or(text.len());
    let mut units = 0u64;
    for (index, ch) in text[start..end].char_indices() {
        if units >= character {
            return start + index;
        }
        units += ch.len_utf16() as u64;
    }
    end
}

pub(crate) fn offset_of(text: &str, line: u64, character: u64) -> usize {
    let mut offset = 0usize;
    for (current_line, current) in text.split_inclusive('\n').enumerate() {
        let current_line = current_line as u64;
        if current_line == line {
            let mut units = 0u64;
            for (index, ch) in current.char_indices() {
                if units >= character {
                    return offset + index;
                }
                units += ch.len_utf16() as u64;
            }
            return offset + current.trim_end_matches('\n').len();
        }
        offset += current.len();
    }
    offset.min(text.len())
}

/// Replace `range` in `text`, returning the new string.
pub(crate) fn replace_range(text: &str, range: std::ops::Range<usize>, replacement: &str) -> String {
    let mut out = String::with_capacity(text.len() + replacement.len());
    out.push_str(&text[..range.start]);
    out.push_str(replacement);
    out.push_str(&text[range.end.min(text.len())..]);
    out
}
