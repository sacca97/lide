//! Workspace search.
//!
//! The search runs `ripgrep` as a subprocess rather than walking the tree
//! itself. `rg` is faster than anything written here would be, it already
//! knows about `.gitignore`, binary files and encodings, and it is installed
//! on most Linux systems. What this crate adds is the protocol: it asks for
//! `--json`, parses the stream, and hands the editor matches it can jump to.
//!
//! The results arrive incrementally on a background thread, so a search over
//! a large tree does not block the window.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;

use serde::Deserialize;

/// What to search for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// The text or pattern.
    pub pattern: String,
    /// Whether the whole pattern is a regular expression.
    pub regex: bool,
    /// Whether matching distinguishes case.
    pub case_sensitive: bool,
    /// Whether to match whole words only.
    pub whole_word: bool,
}

impl Query {
    /// Whether the query is worth running.
    pub fn is_searchable(&self) -> bool {
        !self.pattern.trim().is_empty()
    }

    /// The arguments to give ripgrep.
    ///
    /// `--json` reports every match with its byte offsets, which is what the
    /// editor needs to put the caret on it.
    pub fn arguments(&self) -> Vec<String> {
        let mut args = vec!["--json".to_string(), "--color=never".to_string()];
        if !self.case_sensitive {
            args.push("--ignore-case".to_string());
        }
        if self.whole_word {
            args.push("--word-regexp".to_string());
        }
        if !self.regex {
            args.push("--fixed-strings".to_string());
        }
        args.push("--".to_string());
        args.push(self.pattern.clone());
        args
    }
}

/// One matching line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// The file the match is in.
    pub path: PathBuf,
    /// The line the match starts on, counted from one.
    pub line: u64,
    /// The text of the line, without its line ending.
    pub text: String,
    /// The byte offsets of the matches within the line.
    pub ranges: Vec<std::ops::Range<usize>>,
}

impl Match {
    /// The text the match found.
    pub fn matched_text(&self) -> &str {
        match self.ranges.first() {
            Some(range) => self.text.get(range.clone()).unwrap_or(""),
            None => "",
        }
    }
}

/// Replace every match of `query` in `text`.
///
/// A literal query is matched literally, even if the replacement contains
/// `$`; a regular-expression query keeps `$1` backreferences, which is what a
/// user typing a regex expects. The count is the number of matches, not the
/// number of resulting differences.
pub fn replace_all(text: &str, query: &Query, replacement: &str) -> (String, usize) {
    let pattern = if query.regex {
        query.pattern.clone()
    } else {
        regex::escape(&query.pattern)
    };
    let pattern = if query.whole_word {
        format!(r"\b(?:{pattern})\b")
    } else {
        pattern
    };
    let mut builder = regex::RegexBuilder::new(&pattern);
    builder.case_insensitive(!query.case_sensitive);
    let Ok(regex) = builder.build() else {
        return (text.to_string(), 0);
    };
    let count = regex.find_iter(text).count();
    if count == 0 {
        return (text.to_string(), 0);
    }
    let replaced = if query.regex {
        regex.replace_all(text, replacement).into_owned()
    } else {
        regex
            .replace_all(text, regex::NoExpand(replacement))
            .into_owned()
    };
    (replaced, count)
}

/// A replace running on a background thread.
///
/// One file at a time, so a large tree does not block the window and a file
/// that cannot be written leaves the others done. Files that are not UTF-8
/// are skipped rather than guessed at.
#[derive(Debug)]
pub struct Replace {
    results: mpsc::Receiver<ReplaceUpdate>,
    /// Whether the worker is still going.
    pub running: bool,
    /// Files changed so far.
    pub files: Vec<PathBuf>,
    /// Matches replaced so far.
    pub matches: usize,
    /// Files skipped because they were not UTF-8 or could not be written.
    pub skipped: usize,
    /// Why the worker stopped early, if it did.
    pub error: Option<String>,
}

#[derive(Debug)]
enum ReplaceUpdate {
    File(PathBuf, usize),
    Skipped,
    Error(String),
    Done,
}

impl Replace {
    /// Start replacing in `paths`.
    pub fn start(paths: Vec<PathBuf>, query: Query, replacement: String) -> Self {
        let (tx, results) = mpsc::channel();
        std::thread::spawn(move || {
            for path in paths {
                let bytes = match std::fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        let _ = tx.send(ReplaceUpdate::Error(format!(
                            "could not read {}: {error}",
                            path.display()
                        )));
                        continue;
                    }
                };
                let Ok(text) = String::from_utf8(bytes) else {
                    let _ = tx.send(ReplaceUpdate::Skipped);
                    continue;
                };
                let (updated, count) = replace_all(&text, &query, &replacement);
                if count == 0 {
                    continue;
                }
                if let Err(error) = std::fs::write(&path, updated) {
                    let _ = tx.send(ReplaceUpdate::Error(format!(
                        "could not write {}: {error}",
                        path.display()
                    )));
                    continue;
                }
                let _ = tx.send(ReplaceUpdate::File(path, count));
            }
            let _ = tx.send(ReplaceUpdate::Done);
        });
        Self {
            results,
            running: true,
            files: Vec::new(),
            matches: 0,
            skipped: 0,
            error: None,
        }
    }

    /// Collect what the worker has done. Returns whether anything changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(update) = self.results.try_recv() {
            match update {
                ReplaceUpdate::File(path, count) => {
                    self.files.push(path);
                    self.matches += count;
                    changed = true;
                }
                ReplaceUpdate::Skipped => {
                    self.skipped += 1;
                    changed = true;
                }
                ReplaceUpdate::Error(error) => {
                    self.error = Some(error);
                    changed = true;
                }
                ReplaceUpdate::Done => {
                    self.running = false;
                    changed = true;
                }
            }
        }
        changed
    }

    /// The unique files the search found in.
    pub fn files_replaced(&self) -> usize {
        self.files.len()
    }
}

/// A search that is in progress.
#[derive(Debug)]
pub struct Search {
    /// The query the results belong to.
    pub query: Query,
    /// The matches found so far.
    pub matches: Vec<Match>,
    /// Whether ripgrep is still running.
    pub running: bool,
    /// A message for the user: no ripgrep, a bad pattern, and so on.
    pub error: Option<String>,
    /// The files that have already been reported, so the view can group them.
    files_seen: usize,
    results: mpsc::Receiver<Update>,
    child: Option<Child>,
}

/// What the worker thread sends back.
#[derive(Debug)]
enum Update {
    Match(Match),
    Error(String),
    Done,
}

impl Search {
    /// Run `query` over `root`.
    pub fn start(root: impl AsRef<Path>, query: Query) -> Self {
        let root = root.as_ref().to_path_buf();
        let (tx, results) = mpsc::channel();

        let mut child = match spawn(&root, &query) {
            Ok(child) => child,
            Err(error) => {
                return Self {
                    query,
                    matches: Vec::new(),
                    running: false,
                    error: Some(error),
                    files_seen: 0,
                    results,
                    child: None,
                }
            }
        };

        // The reader thread owns the pipe; the search keeps the child so that
        // it can be killed when the query changes or the editor goes away.
        if let Some(stdout) = child.stdout.take() {
            let reader_root = root.clone();
            std::thread::spawn(move || {
                read_stream(BufReader::new(stdout), &reader_root, |update| {
                    let _ = tx.send(update);
                });
            });
        }

        Self {
            query,
            matches: Vec::new(),
            running: true,
            error: None,
            files_seen: 0,
            results,
            child: Some(child),
        }
    }

    /// Collect whatever the search has found since the last call.
    ///
    /// Returns `true` when the view should be redrawn.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.results.try_recv() {
                Ok(Update::Match(found)) => {
                    changed = true;
                    if self
                        .matches
                        .last()
                        .map(|last| last.path != found.path)
                        .unwrap_or(true)
                    {
                        self.files_seen += 1;
                    }
                    self.matches.push(found);
                }
                Ok(Update::Error(error)) => {
                    changed = true;
                    self.error = Some(error);
                }
                Ok(Update::Done) => {
                    changed = true;
                    self.running = false;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.running = false;
                    break;
                }
            }
        }
        changed
    }

    /// Cancel the search.
    pub fn cancel(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.running = false;
    }

    /// The number of files with at least one match.
    pub fn file_count(&self) -> usize {
        self.files_seen
    }

    /// The matches in one file.
    pub fn matches_for(&self, path: &Path) -> impl Iterator<Item = &Match> {
        let path = path.to_path_buf();
        self.matches.iter().filter(move |found| found.path == path)
    }
}

impl Drop for Search {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Start ripgrep and wire its output to a worker thread.
fn spawn(root: &Path, query: &Query) -> Result<Child, String> {
    let child = Command::new("rg")
        .args(query.arguments())
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|error| format!("could not run ripgrep: {error}"))?;

    Ok(child)
}

/// Read ripgrep's JSON stream, calling `on_match` for each match.
///
/// This is separate from [`Search`] so that it can be tested against a fixed
/// stream without running a process.
fn read_stream<R: BufRead>(reader: R, root: &Path, mut on_event: impl FnMut(Update)) {
    for line in reader.lines() {
        let Ok(line) = line else {
            break;
        };
        if line.is_empty() {
            continue;
        }
        match parse_event(&line, root) {
            Ok(Some(found)) => on_event(Update::Match(found)),
            Ok(None) => {}
            Err(error) => on_event(Update::Error(error)),
        }
    }
    on_event(Update::Done);
}

/// One line of ripgrep's `--json` output.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum Event {
    #[serde(rename = "match")]
    Match {
        data: MatchData,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct MatchData {
    path: TextOrBytes,
    lines: TextOrBytes,
    line_number: Option<u64>,
    submatches: Vec<Submatch>,
}

#[derive(Debug, Deserialize)]
struct Submatch {
    start: usize,
    end: usize,
}

/// ripgrep reports text as `{"text": "..."}` or, for non-UTF-8, `{"bytes": "..."}`.
#[derive(Debug, Deserialize)]
struct TextOrBytes {
    text: Option<String>,
    bytes: Option<String>,
}

impl TextOrBytes {
    /// The text, decoded when ripgrep gave bytes.
    fn text(&self) -> String {
        if let Some(text) = &self.text {
            return text.clone();
        }
        let Some(encoded) = &self.bytes else {
            return String::new();
        };
        // Standard base64, as ripgrep writes it.
        decode_base64(encoded).unwrap_or_default()
    }
}

/// Parse one line of ripgrep output.
fn parse_event(line: &str, root: &Path) -> Result<Option<Match>, String> {
    let event: Event = match serde_json::from_str(line) {
        Ok(event) => event,
        // A line that is not JSON is a diagnostic from ripgrep, not a match.
        Err(_) => return Ok(None),
    };

    let Event::Match { data } = event else {
        return Ok(None);
    };

    let path = root.join(data.path.text());
    let text = data.lines.text();
    let text = text.trim_end_matches(['\n', '\r']).to_string();

    Ok(Some(Match {
        path,
        line: data.line_number.unwrap_or(1),
        ranges: data
            .submatches
            .into_iter()
            .map(|submatch| submatch.start..submatch.end)
            .collect(),
        text,
    }))
}

/// Decode the base64 ripgrep uses for non-UTF-8 paths and lines.
fn decode_base64(input: &str) -> Option<String> {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (index, byte) in TABLE.iter().enumerate() {
        lookup[*byte as usize] = index as u8;
    }

    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in input.bytes() {
        if byte == b'=' {
            break;
        }
        let value = lookup[byte as usize];
        if value == 255 {
            continue;
        }
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_query_is_not_searchable() {
        assert!(!Query::default().is_searchable());
        assert!(!Query {
            pattern: "   ".to_string(),
            ..Default::default()
        }
        .is_searchable());
        assert!(Query {
            pattern: "x".to_string(),
            ..Default::default()
        }
        .is_searchable());
    }

    #[test]
    fn a_literal_query_asks_for_a_fixed_string() {
        let query = Query {
            pattern: "a.b".to_string(),
            ..Default::default()
        };
        let args = query.arguments();
        assert!(args.contains(&"--fixed-strings".to_string()));
        assert!(args.contains(&"--ignore-case".to_string()));
        assert_eq!(args.last().unwrap(), "a.b", "the pattern is last");
    }

    #[test]
    fn a_regex_query_does_not_ask_for_a_fixed_string() {
        let query = Query {
            pattern: "fn\\s+\\w+".to_string(),
            regex: true,
            case_sensitive: true,
            whole_word: false,
        };
        let args = query.arguments();
        assert!(!args.contains(&"--fixed-strings".to_string()));
        assert!(!args.contains(&"--ignore-case".to_string()));
    }

    #[test]
    fn whole_word_search_sets_the_flag() {
        let query = Query {
            pattern: "run".to_string(),
            whole_word: true,
            ..Default::default()
        };
        assert!(query.arguments().contains(&"--word-regexp".to_string()));
    }

    #[test]
    fn a_match_line_is_parsed() {
        let line = r#"{"type":"match","data":{"path":{"text":"src/main.rs"},"lines":{"text":"fn main() {}\n"},"line_number":3,"absolute_offset":42,"submatches":[{"match":{"text":"main"},"start":3,"end":7}]}}"#;
        let found = parse_event(line, Path::new("/project"))
            .unwrap()
            .expect("a match");
        assert_eq!(found.path, Path::new("/project/src/main.rs"));
        assert_eq!(found.line, 3);
        assert_eq!(found.text, "fn main() {}");
        assert_eq!(found.ranges, vec![3..7]);
        assert_eq!(found.matched_text(), "main");
    }

    #[test]
    fn several_matches_on_one_line_are_kept() {
        let line = r#"{"type":"match","data":{"path":{"text":"a.txt"},"lines":{"text":"a a a\n"},"line_number":1,"absolute_offset":0,"submatches":[{"match":{"text":"a"},"start":0,"end":1},{"match":{"text":"a"},"start":2,"end":3}]}}"#;
        let found = parse_event(line, Path::new("/p")).unwrap().unwrap();
        assert_eq!(found.ranges.len(), 2);
    }

    #[test]
    fn begin_and_end_events_are_ignored() {
        let begin = r#"{"type":"begin","data":{"path":{"text":"a.txt"}}}"#;
        let end = r#"{"type":"end","data":{"path":{"text":"a.txt"},"binary_offset":null,"stats":{}}}"#;
        assert!(parse_event(begin, Path::new("/p")).unwrap().is_none());
        assert!(parse_event(end, Path::new("/p")).unwrap().is_none());
    }

    #[test]
    fn non_json_lines_are_ignored() {
        assert!(parse_event("rg: some warning", Path::new("/p"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_stream_is_read_to_the_end() {
        let stream = concat!(
            r#"{"type":"begin","data":{"path":{"text":"a.txt"}}}"#,
            "\n",
            r#"{"type":"match","data":{"path":{"text":"a.txt"},"lines":{"text":"one\n"},"line_number":1,"absolute_offset":0,"submatches":[{"match":{"text":"one"},"start":0,"end":3}]}}"#,
            "\n",
            r#"{"type":"end","data":{"path":{"text":"a.txt"},"binary_offset":null,"stats":{}}}"#,
            "\n",
        );
        let mut matches = Vec::new();
        let mut done = false;
        read_stream(std::io::Cursor::new(stream), Path::new("/p"), |update| match update {
            Update::Match(found) => matches.push(found),
            Update::Done => done = true,
            Update::Error(error) => panic!("unexpected error: {error}"),
        });
        assert_eq!(matches.len(), 1);
        assert!(done, "the stream ends with a Done");
    }

    #[test]
    fn base64_is_decoded() {
        assert_eq!(decode_base64("aGVsbG8=").as_deref(), Some("hello"));
        assert_eq!(decode_base64("").as_deref(), Some(""));
    }
}

/// An end-to-end search over a real directory, when ripgrep is installed.
#[cfg(test)]
mod replace_tests {
    use super::*;

    fn literal(pattern: &str) -> Query {
        Query {
            pattern: pattern.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn a_literal_replacement_is_not_a_regular_expression() {
        let (out, count) = replace_all("cat cat", &literal("cat"), "$1 dog");
        assert_eq!(out, "$1 dog $1 dog");
        assert_eq!(count, 2);
    }

    #[test]
    fn replacement_ignores_case_when_the_query_does() {
        let (out, count) = replace_all("Cat cat CAT", &literal("cat"), "dog");
        assert_eq!(out, "dog dog dog");
        assert_eq!(count, 3);
    }

    #[test]
    fn a_regex_replacement_keeps_its_captures() {
        let query = Query {
            pattern: r"(\w+)@example\.com".to_string(),
            regex: true,
            case_sensitive: true,
            whole_word: false,
        };
        let (out, count) = replace_all("a@example.com b@example.com", &query, "$1@test.org");
        assert_eq!(out, "a@test.org b@test.org");
        assert_eq!(count, 2);
    }

    #[test]
    fn a_whole_word_replacement_leaves_parts_of_words_alone() {
        let query = Query {
            whole_word: true,
            ..literal("cat")
        };
        let (out, count) = replace_all("cat scatter cat", &query, "dog");
        assert_eq!(out, "dog scatter dog");
        assert_eq!(count, 2);
    }

    #[test]
    fn no_match_changes_nothing() {
        let (out, count) = replace_all("a dog", &literal("cat"), "bird");
        assert_eq!(out, "a dog");
        assert_eq!(count, 0);
    }

    #[test]
    fn the_worker_replaces_in_a_file() {
        let directory = std::env::temp_dir().join(format!(
            "lide-replace-worker-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("a.txt");
        std::fs::write(&path, "one cat\ntwo cat\n").unwrap();

        let mut replace = Replace::start(vec![path.clone()], literal("cat"), "dog".to_string());
        for _ in 0..200 {
            replace.poll();
            if !replace.running {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!replace.running, "the worker finishes");
        assert_eq!(replace.matches, 2);
        assert_eq!(replace.files_replaced(), 1);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one dog\ntwo dog\n");

        let _ = std::fs::remove_dir_all(&directory);
    }
}

#[cfg(test)]
mod process_tests {
    use super::*;

    #[test]
    fn a_real_search_finds_the_text() {
        if which("rg").is_none() {
            eprintln!("skipping: ripgrep is not installed");
            return;
        }

        let root = std::env::temp_dir().join("lide-search-test");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), "alpha\nbeta\ngamma\n").unwrap();
        std::fs::write(root.join("b.txt"), "nothing here\n").unwrap();

        let mut search = Search::start(
            &root,
            Query {
                pattern: "beta".to_string(),
                ..Default::default()
            },
        );

        for _ in 0..200 {
            search.poll();
            if !search.running {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(search.error.is_none(), "search error: {:?}", search.error);
        assert_eq!(search.matches.len(), 1);
        let found = &search.matches[0];
        assert_eq!(found.path, root.join("a.txt"));
        assert_eq!(found.line, 2);
        assert_eq!(found.text, "beta");
        assert_eq!(found.matched_text(), "beta");

        std::fs::remove_dir_all(&root).ok();
    }

    fn which(program: &str) -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|directory| directory.join(program))
            .find(|candidate| candidate.is_file())
    }
}
