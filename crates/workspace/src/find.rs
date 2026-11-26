//! Find and replace in the current document.
//!
//! The search is plain text. It runs over the whole document rather than the
//! visible part, so the match count is the document's, not the viewport's.
//! Matching is case-insensitive unless the query contains an upper-case
//! letter, which is what most editors do and what people expect from "smart
//! case".

use std::ops::Range;

use rune_edit::TextEdit;

/// The state of the find and replace bar.
#[derive(Debug)]
pub struct FindState {
    /// The text to find.
    pub query: TextEdit,
    /// The text to replace it with.
    pub replacement: TextEdit,
    /// Whether the bar is open.
    pub open: bool,
    /// Whether to replace too, rather than only find.
    pub replacing: bool,
    /// Whether matching distinguishes case.
    pub case_sensitive: bool,
    /// The matches in the document, in order.
    pub matches: Vec<Range<usize>>,
    /// The index of the match the editor is on.
    pub current: usize,
    /// The query the matches were computed for.
    computed_for: String,
}

impl Default for FindState {
    fn default() -> Self {
        Self {
            query: TextEdit::new(""),
            replacement: TextEdit::new(""),
            open: false,
            replacing: false,
            case_sensitive: false,
            matches: Vec::new(),
            current: 0,
            computed_for: String::new(),
        }
    }
}

impl FindState {
    /// Open the bar, keeping whatever was searched for last time.
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Close the bar and forget the matches.
    pub fn close(&mut self) {
        self.open = false;
        self.matches.clear();
        self.computed_for.clear();
    }

    /// Show the replace field as well.
    pub fn open_replace(&mut self) {
        self.open = true;
        self.replacing = true;
    }

    /// The query, as text.
    pub fn query_text(&self) -> &str {
        &self.query.text
    }

    /// Recompute the matches when the query or the document changed.
    ///
    /// Returns `true` when the matches changed.
    pub fn refresh(&mut self, text: &str) -> bool {
        let query = self.query.text.clone();
        if query == self.computed_for && !self.matches.is_empty() {
            return false;
        }
        self.computed_for.clone_from(&query);
        let case_sensitive = self.case_sensitive || has_uppercase(&query);
        self.matches = find_all(text, &query, case_sensitive);
        if self.current >= self.matches.len() {
            self.current = 0;
        }
        true
    }

    /// The number of matches.
    pub fn len(&self) -> usize {
        self.matches.len()
    }

    /// Whether there are no matches.
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }

    /// The current match.
    pub fn current_match(&self) -> Option<Range<usize>> {
        self.matches.get(self.current).cloned()
    }

    /// Move to the next match, wrapping around.
    pub fn next(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.current = (self.current + 1) % self.matches.len();
    }

    /// Move to the previous match, wrapping around.
    pub fn previous(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.current = if self.current == 0 {
            self.matches.len() - 1
        } else {
            self.current - 1
        };
    }

    /// The edit that replaces the current match.
    pub fn replacement_edit(&self) -> Option<(Range<usize>, String)> {
        let range = self.current_match()?;
        Some((range, self.replacement.text.clone()))
    }

    /// The edits that replace every match, in document order.
    pub fn replace_all_edits(&self) -> Vec<(Range<usize>, String)> {
        self.matches
            .iter()
            .map(|range| (range.clone(), self.replacement.text.clone()))
            .collect()
    }
}

/// Whether `text` contains a character with an upper-case form.
///
/// This is "smart case": typing `foo` matches `Foo` and `FOO`, while typing
/// `Foo` matches only `Foo`.
pub fn has_uppercase(text: &str) -> bool {
    text.chars()
        .any(|ch| ch.is_uppercase() && ch.to_lowercase().next() != Some(ch))
}

/// Find every occurrence of `query` in `text`.
///
/// An empty query matches nothing: highlighting the whole document would be
/// noise, not a search.
pub fn find_all(text: &str, query: &str, case_sensitive: bool) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }

    let (haystack, needle) = if case_sensitive {
        (text.to_string(), query.to_string())
    } else {
        (text.to_lowercase(), query.to_lowercase())
    };

    // Matching on the lowered text is only sound when lowering did not change
    // the byte lengths; when it did (some scripts change length when case
    // folded) fall back to matching the original text.
    if haystack.len() != text.len() {
        return find_exact(text, query, case_sensitive);
    }

    let mut matches = Vec::new();
    let mut start = 0;
    while let Some(index) = haystack[start..].find(&needle) {
        let begin = start + index;
        let end = begin + needle.len();
        matches.push(begin..end);
        // Step past this match so that overlapping matches are not reported.
        start = end.max(begin + 1);
        if start >= haystack.len() {
            break;
        }
    }
    matches
}

/// A case-aware search that does not assume the lowered text has the same
/// byte length.
fn find_exact(text: &str, query: &str, case_sensitive: bool) -> Vec<Range<usize>> {
    let mut matches = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let Some((begin, end)) = find_from(text, query, start, case_sensitive) else {
            break;
        };
        matches.push(begin..end);
        start = end.max(begin + 1);
    }
    matches
}

/// Find `query` in `text` at or after `from`.
fn find_from(text: &str, query: &str, from: usize, case_sensitive: bool) -> Option<(usize, usize)> {
    if case_sensitive {
        let index = text.get(from..)?.find(query)?;
        let begin = from + index;
        return Some((begin, begin + query.len()));
    }

    // Character-wise comparison, which is correct for any script.
    let query_chars = query.chars().count();
    if query_chars == 0 {
        return None;
    }
    let mut offsets: Vec<(usize, char)> =
        text.char_indices().skip_while(|(i, _)| *i < from).collect();
    offsets.push((text.len(), '\0'));

    for window_start in 0..offsets.len().saturating_sub(query_chars) {
        let mut matched = true;
        for (offset, query_char) in query.chars().enumerate() {
            let text_char = offsets[window_start + offset].1;
            if !chars_eq(text_char, query_char) {
                matched = false;
                break;
            }
        }
        if matched {
            let begin = offsets[window_start].0;
            let end = offsets[window_start + query_chars].0;
            return Some((begin, end));
        }
    }
    None
}

/// Case-insensitive character equality that respects the multi-character
/// lower-case mappings (e.g. the German sharp s).
fn chars_eq(a: char, b: char) -> bool {
    if a == b {
        return true;
    }
    let lower_a: String = a.to_lowercase().collect();
    let lower_b: String = b.to_lowercase().collect();
    lower_a == lower_b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_every_occurrence() {
        let text = "one two one two one";
        let matches = find_all(text, "one", true);
        assert_eq!(matches.len(), 3);
        assert_eq!(&text[matches[0].clone()], "one");
        assert_eq!(&text[matches[2].clone()], "one");
    }

    #[test]
    fn an_empty_query_finds_nothing() {
        assert!(find_all("anything", "", true).is_empty());
    }

    #[test]
    fn matching_ignores_case_by_default() {
        let matches = find_all("Hello HELLO hello", "hello", false);
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn case_sensitive_matching_is_exact() {
        let matches = find_all("Hello HELLO hello", "hello", true);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], 12..17);
    }

    #[test]
    fn matches_do_not_overlap() {
        let matches = find_all("aaaa", "aa", true);
        assert_eq!(matches, vec![0..2, 2..4]);
    }

    #[test]
    fn multi_byte_text_is_handled() {
        let text = "héllo wörld héllo";
        let matches = find_all(text, "héllo", true);
        assert_eq!(matches.len(), 2);
        assert_eq!(&text[matches[0].clone()], "héllo");
        assert_eq!(&text[matches[1].clone()], "héllo");
    }

    #[test]
    fn case_insensitive_matching_works_on_non_ascii() {
        let matches = find_all("ÉCOLE école", "école", false);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn navigation_wraps_around() {
        let mut state = FindState {
            query: TextEdit::new("a"),
            ..Default::default()
        };
        state.refresh("a a a");
        assert_eq!(state.current, 0);
        state.next();
        assert_eq!(state.current, 1);
        state.next();
        state.next();
        assert_eq!(
            state.current, 0,
            "next wraps from the last match to the first"
        );
        state.previous();
        assert_eq!(
            state.current, 2,
            "previous wraps from the first to the last"
        );
    }

    #[test]
    fn replaces_the_current_match() {
        let mut state = FindState {
            query: TextEdit::new("one"),
            replacement: TextEdit::new("two"),
            ..Default::default()
        };
        state.refresh("one one");
        let (range, text) = state.replacement_edit().unwrap();
        assert_eq!(range, 0..3);
        assert_eq!(text, "two");
    }

    #[test]
    fn replacing_everything_yields_one_edit_per_match() {
        let mut state = FindState {
            query: TextEdit::new("x"),
            replacement: TextEdit::new("yy"),
            ..Default::default()
        };
        state.refresh("xaxbx");
        assert_eq!(state.replace_all_edits().len(), 3);
    }

    #[test]
    fn smart_case_makes_an_upper_case_query_exact() {
        let mut state = FindState {
            query: TextEdit::new("Foo"),
            ..Default::default()
        };
        state.refresh("foo Foo FOO");
        assert_eq!(state.len(), 1, "an upper-case query is matched exactly");
    }

    #[test]
    fn refresh_is_cheap_when_nothing_changed() {
        let mut state = FindState {
            query: TextEdit::new("a"),
            ..Default::default()
        };
        assert!(state.refresh("aaa"));
        assert!(!state.refresh("aaa"), "the second refresh changes nothing");
    }
}
