//! Git, through the `git` command line.
//!
//! The editor reads the repository's status and a file's diff; staging,
//! commits and the network operations stay in the terminal. Every read runs
//! `git` as a subprocess and parses what it says.
//!
//! The alternative — `libgit2` — is a large native dependency for behaviour
//! that the installed `git` already implements correctly, including the
//! parts that are hard to get right: the index format, the ignore rules, and
//! what "modified" means for a submodule.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Errors from the git operations.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// `git` is not installed.
    #[error("git is not installed")]
    NotInstalled,
    /// `git` exited with an error.
    #[error("git {command} failed: {message}")]
    Failed {
        /// The git command that failed.
        command: String,
        /// What git said.
        message: String,
    },
    /// The folder is not in a repository.
    #[error("{0} is not in a git repository")]
    NotARepository(PathBuf),
}

/// How a file changed, from the point of view of the index or the worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Nothing changed.
    Unmodified,
    /// A new file.
    Added,
    /// A changed file.
    Modified,
    /// A deleted file.
    Deleted,
    /// A renamed file.
    Renamed,
    /// A file git does not track yet.
    Untracked,
    /// A file with a merge conflict.
    Conflicted,
    /// A file git ignores.
    Ignored,
}

impl Change {
    fn from_code(code: char) -> Change {
        match code {
            'M' => Change::Modified,
            'A' => Change::Added,
            'D' => Change::Deleted,
            'R' => Change::Renamed,
            'C' => Change::Added,
            'U' => Change::Conflicted,
            '?' => Change::Untracked,
            '!' => Change::Ignored,
            ' ' => Change::Unmodified,
            _ => Change::Modified,
        }
    }

    /// The letter shown in the status list.
    pub fn letter(self) -> &'static str {
        match self {
            Change::Unmodified => " ",
            Change::Added => "A",
            Change::Modified => "M",
            Change::Deleted => "D",
            Change::Renamed => "R",
            Change::Untracked => "?",
            Change::Conflicted => "U",
            Change::Ignored => "!",
        }
    }

    /// Whether the change is staged (in the index) rather than in the worktree.
    pub fn is_unmerged(self) -> bool {
        matches!(self, Change::Conflicted)
    }
}

/// One entry of `git status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The path, relative to the repository root.
    pub path: PathBuf,
    /// The state of the index.
    pub index: Change,
    /// The state of the worktree.
    pub worktree: Change,
}

impl Entry {
    /// Whether the change is staged.
    pub fn staged(&self) -> bool {
        !matches!(self.index, Change::Unmodified | Change::Untracked)
    }

    /// Whether there are changes that are not staged.
    pub fn unstaged(&self) -> bool {
        !matches!(self.worktree, Change::Unmodified | Change::Untracked)
            || self.index == Change::Untracked
    }

    /// The two-letter code git shows.
    pub fn code(&self) -> String {
        format!(
            "{}{}",
            self.index.letter(),
            self.worktree.letter()
        )
    }
}

/// The status of a repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// The current branch, or the commit when HEAD is detached.
    pub branch: String,
    /// The changed files.
    pub entries: Vec<Entry>,
}

impl Status {
    /// Whether the worktree is clean.
    pub fn is_clean(&self) -> bool {
        self.entries.is_empty()
    }

    /// The number of staged entries.
    pub fn staged(&self) -> usize {
        self.entries.iter().filter(|entry| entry.staged()).count()
    }

    /// The number of unstaged entries.
    pub fn unstaged(&self) -> usize {
        self.entries.iter().filter(|entry| entry.unstaged()).count()
    }
}

/// A repository.
#[derive(Debug, Clone)]
pub struct Repo {
    /// The root of the worktree.
    pub root: PathBuf,
}

impl Repo {
    /// Open the repository that contains `path`.
    pub fn discover(path: impl AsRef<Path>) -> Result<Self, GitError> {
        let path = path.as_ref();
        let output = run(path, &["rev-parse", "--show-toplevel"])?;
        let root = output.trim();
        if root.is_empty() {
            return Err(GitError::NotARepository(path.to_path_buf()));
        }
        Ok(Self {
            root: PathBuf::from(root),
        })
    }

    /// Whether `path` is inside a repository.
    pub fn is_repository(path: impl AsRef<Path>) -> bool {
        Self::discover(path).is_ok()
    }

    /// The repository's status.
    pub fn status(&self) -> Result<Status, GitError> {
        let output = self.run(&["status", "--porcelain=v1", "-b", "-z"])?;
        Ok(parse_status(&output))
    }

    /// The staged diff, or the worktree diff when `staged` is false.
    pub fn diff(&self, staged: bool) -> Result<String, GitError> {
        if staged {
            self.run(&["diff", "--cached", "--no-color"])
        } else {
            self.run(&["diff", "--no-color"])
        }
    }

    /// The diff of one file.
    pub fn diff_file(&self, path: &Path, staged: bool) -> Result<String, GitError> {
        let file = path.to_string_lossy().to_string();
        if staged {
            self.run(&["diff", "--cached", "--no-color", "--", &file])
        } else {
            self.run(&["diff", "--no-color", "--", &file])
        }
    }

    fn run(&self, args: &[&str]) -> Result<String, GitError> {
        run(&self.root, args)
    }
}

/// Run git in `directory` and return its standard output.
fn run(directory: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        // A command started from a window has no terminal to prompt on; fail
        // with git's message instead of waiting for credentials or an editor.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .output()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                GitError::NotInstalled
            } else {
                GitError::Failed {
                    command: args.join(" "),
                    message: error.to_string(),
                }
            }
        })?;

    if !output.status.success() {
        return Err(GitError::Failed {
            command: args.join(" "),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Parse `git status --porcelain=v1 -b -z`.
///
/// The `-z` form separates records with NUL and does not quote paths, which
/// is what makes a path with a space or a newline in it survive.
pub fn parse_status(output: &str) -> Status {
    let mut status = Status::default();

    for record in output.split('\0') {
        if record.is_empty() {
            continue;
        }
        if let Some(header) = record.strip_prefix("## ") {
            status.branch = parse_branch(header);
            continue;
        }
        // A rename record is followed by the original path in the next
        // record, which is a bare path with no status prefix; it is skipped
        // because the entry is already complete.
        let mut chars = record.chars();
        let (Some(index), Some(worktree)) = (chars.next(), chars.next()) else {
            continue;
        };
        let path: String = chars.as_str().to_string();
        if index == 'R' || index == 'C' || worktree == 'R' || worktree == 'C' {
            // "new\0old" — the old path arrives as its own record; drop it.
        }
        let path = path.trim_start();
        if path.is_empty() {
            continue;
        }
        status.entries.push(Entry {
            path: PathBuf::from(path),
            index: Change::from_code(index),
            worktree: Change::from_code(worktree),
        });
    }

    status
}

/// The branch name from a `## ...` header.
fn parse_branch(header: &str) -> String {
    // "main...origin/main [ahead 1]" or "HEAD (no branch)" or "No commits yet on main".
    let header = header.trim();
    if let Some(rest) = header.strip_prefix("No commits yet on ") {
        return rest.to_string();
    }
    if header.starts_with("HEAD (no branch)") {
        return "HEAD".to_string();
    }
    let name = header
        .split("...")
        .next()
        .unwrap_or(header)
        .split(' ')
        .next()
        .unwrap_or(header);
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_worktree_parses_to_no_entries() {
        let status = parse_status("## main...origin/main\0");
        assert_eq!(status.branch, "main");
        assert!(status.is_clean());
    }

    #[test]
    fn a_modified_file_is_parsed() {
        let status = parse_status("## main\0 M src/main.rs\0");
        assert_eq!(status.entries.len(), 1);
        let entry = &status.entries[0];
        assert_eq!(entry.path, PathBuf::from("src/main.rs"));
        assert_eq!(entry.index, Change::Unmodified);
        assert_eq!(entry.worktree, Change::Modified);
        assert!(!entry.staged());
        assert!(entry.unstaged());
        assert_eq!(entry.code(), " M");
    }

    #[test]
    fn a_staged_file_is_parsed() {
        let status = parse_status("## main\0M  src/main.rs\0");
        let entry = &status.entries[0];
        assert_eq!(entry.index, Change::Modified);
        assert!(entry.staged());
        assert!(!entry.unstaged());
        assert_eq!(entry.code(), "M ");
    }

    #[test]
    fn a_file_that_is_both_staged_and_modified_counts_twice() {
        let status = parse_status("## main\0MM src/main.rs\0");
        assert_eq!(status.staged(), 1);
        assert_eq!(status.unstaged(), 1);
    }

    #[test]
    fn an_untracked_file_is_parsed() {
        let status = parse_status("## main\0?? notes.txt\0");
        let entry = &status.entries[0];
        assert_eq!(entry.index, Change::Untracked);
        assert_eq!(entry.path, PathBuf::from("notes.txt"));
        assert!(!entry.staged());
        assert!(entry.unstaged());
    }

    #[test]
    fn a_path_with_a_space_survives() {
        let status = parse_status("## main\0?? my notes.txt\0");
        assert_eq!(status.entries[0].path, PathBuf::from("my notes.txt"));
    }

    #[test]
    fn a_deleted_file_is_parsed() {
        let status = parse_status("## main\0 D gone.txt\0");
        assert_eq!(status.entries[0].worktree, Change::Deleted);
    }

    #[test]
    fn a_fresh_repository_names_its_unborn_branch() {
        assert_eq!(parse_branch("No commits yet on main"), "main");
    }

    #[test]
    fn a_detached_head_is_named() {
        assert_eq!(parse_branch("HEAD (no branch)"), "HEAD");
    }

    #[test]
    fn an_ahead_branch_is_trimmed_to_its_name() {
        assert_eq!(parse_branch("main...origin/main [ahead 1]"), "main");
        assert_eq!(parse_branch("feature/x...origin/feature/x"), "feature/x");
    }
}

/// End-to-end tests against the real `git`.
#[cfg(test)]
mod repository_tests {
    use super::*;

    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("lide-git-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            let scratch = Self { path };
            scratch.git(&["init", "-q", "-b", "main"]);
            scratch.git(&["config", "user.email", "test@example.invalid"]);
            scratch.git(&["config", "user.name", "Test"]);
            scratch.git(&["config", "commit.gpgsign", "false"]);
            std::fs::write(scratch.path.join("a.txt"), "one\n").unwrap();
            scratch.git(&["add", "a.txt"]);
            scratch.git(&["commit", "-q", "-m", "first"]);
            scratch
        }

        fn git(&self, args: &[&str]) -> String {
            let output = Command::new("git")
                .args(args)
                .current_dir(&self.path)
                .output()
                .expect("git should run");
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8_lossy(&output.stdout).into_owned()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn git_installed() -> bool {
        Command::new("git").arg("--version").output().is_ok()
    }

    #[test]
    fn status_reports_a_modified_file() {
        if !git_installed() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let scratch = Scratch::new("status");
        std::fs::write(scratch.path.join("a.txt"), "two\n").unwrap();

        let repo = Repo::discover(&scratch.path).unwrap();
        let status = repo.status().unwrap();
        assert_eq!(status.branch, "main");
        assert_eq!(status.entries.len(), 1);
        assert_eq!(status.entries[0].path, PathBuf::from("a.txt"));
        assert!(status.entries[0].unstaged());
    }

    #[test]
    fn the_diff_contains_the_change() {
        if !git_installed() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let scratch = Scratch::new("diff");
        std::fs::write(scratch.path.join("a.txt"), "two\n").unwrap();

        let repo = Repo::discover(&scratch.path).unwrap();
        let diff = repo.diff(false).unwrap();
        assert!(diff.contains("-one"), "the old line is in the diff: {diff}");
        assert!(diff.contains("+two"), "the new line is in the diff: {diff}");
    }

    #[test]
    fn a_plain_folder_is_not_a_repository() {
        if !git_installed() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let directory = std::env::temp_dir().join(format!("lide-not-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        assert!(!Repo::is_repository(&directory));
        let _ = std::fs::remove_dir_all(&directory);
    }
}

/// What a line of a unified diff is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    /// A file header (`diff --git`, `---`, `+++`, index lines).
    Header,
    /// A hunk header (`@@ -1,2 +1,3 @@`).
    Hunk,
    /// A line that only exists in the new file.
    Added,
    /// A line that only exists in the old file.
    Removed,
    /// A line that is in both.
    Context,
    /// Anything else, such as `\ No newline at end of file`.
    Other,
}

/// One line of a unified diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// What the line is.
    pub kind: DiffKind,
    /// The line as git wrote it, without its trailing newline.
    pub text: String,
    /// The old line number, when the line has one.
    pub old_line: Option<u64>,
    /// The new line number, when the line has one.
    pub new_line: Option<u64>,
}

/// Parse `git diff` output into lines the editor can draw.
///
/// Line numbers are tracked through the hunks, so a click can jump to the
/// right place even though the diff itself only carries the hunk headers.
pub fn parse_diff(text: &str) -> Vec<DiffLine> {
    let mut lines = Vec::new();
    let mut old_line = 0u64;
    let mut new_line = 0u64;

    for raw in text.lines() {
        if raw.starts_with("diff --git")
            || raw.starts_with("index ")
            || raw.starts_with("--- ")
            || raw.starts_with("+++ ")
            || raw.starts_with("new file")
            || raw.starts_with("deleted file")
            || raw.starts_with("similarity index")
            || raw.starts_with("rename ")
        {
            lines.push(DiffLine {
                kind: DiffKind::Header,
                text: raw.to_string(),
                old_line: None,
                new_line: None,
            });
            continue;
        }

        if raw.starts_with("@@") {
            // `@@ -old,count +new,count @@`
            if let Some((old, new)) = parse_hunk_header(raw) {
                old_line = old;
                new_line = new;
            }
            lines.push(DiffLine {
                kind: DiffKind::Hunk,
                text: raw.to_string(),
                old_line: None,
                new_line: None,
            });
            continue;
        }

        if raw.starts_with('+') {
            lines.push(DiffLine {
                kind: DiffKind::Added,
                text: raw.to_string(),
                old_line: None,
                new_line: Some(new_line),
            });
            new_line += 1;
            continue;
        }
        if raw.starts_with('-') {
            lines.push(DiffLine {
                kind: DiffKind::Removed,
                text: raw.to_string(),
                old_line: Some(old_line),
                new_line: None,
            });
            old_line += 1;
            continue;
        }
        if raw.starts_with(' ') {
            lines.push(DiffLine {
                kind: DiffKind::Context,
                text: raw.to_string(),
                old_line: Some(old_line),
                new_line: Some(new_line),
            });
            old_line += 1;
            new_line += 1;
            continue;
        }
        lines.push(DiffLine {
            kind: DiffKind::Other,
            text: raw.to_string(),
            old_line: None,
            new_line: None,
        });
    }

    lines
}

/// The starting line numbers of a hunk header.
fn parse_hunk_header(header: &str) -> Option<(u64, u64)> {
    let rest = header.trim_start_matches('@');
    let mut parts = rest.split_whitespace();
    let old = parts.next()?.trim_start_matches('-');
    let new = parts.next()?.trim_start_matches('+');
    let old = old.split(',').next()?.parse().ok()?;
    let new = new.split(',').next()?.parse().ok()?;
    Some((old, new))
}

#[cfg(test)]
mod diff_tests {
    use super::*;

    const SAMPLE: &str = "\
diff --git a/a.txt b/a.txt
index 111..222 100644
--- a/a.txt
+++ b/a.txt
@@ -1,3 +1,4 @@
 one
-two
+TWO
+three
 four
";

    #[test]
    fn a_diff_is_split_into_kinds() {
        let lines = parse_diff(SAMPLE);
        assert_eq!(lines[0].kind, DiffKind::Header);
        assert_eq!(lines[4].kind, DiffKind::Hunk);
        assert_eq!(lines[5].kind, DiffKind::Context);
        assert_eq!(lines[6].kind, DiffKind::Removed);
        assert_eq!(lines[7].kind, DiffKind::Added);
    }

    #[test]
    fn line_numbers_are_tracked() {
        let lines = parse_diff(SAMPLE);
        let context = lines.iter().find(|line| line.text == " one").unwrap();
        assert_eq!(context.old_line, Some(1));
        assert_eq!(context.new_line, Some(1));

        let removed = lines.iter().find(|line| line.text == "-two").unwrap();
        assert_eq!(removed.old_line, Some(2));
        assert_eq!(removed.new_line, None);

        let added = lines.iter().find(|line| line.text == "+TWO").unwrap();
        assert_eq!(added.new_line, Some(2));
        assert_eq!(added.old_line, None);

        let last = lines.iter().find(|line| line.text == " four").unwrap();
        assert_eq!(last.old_line, Some(3));
        assert_eq!(last.new_line, Some(4));
    }

    #[test]
    fn a_hunk_header_gives_its_starting_lines() {
        assert_eq!(parse_hunk_header("@@ -10,5 +12,7 @@ fn main"), Some((10, 12)));
        assert_eq!(parse_hunk_header("@@ -1 +1 @@"), Some((1, 1)));
    }

    #[test]
    fn an_empty_diff_has_no_lines() {
        assert!(parse_diff("").is_empty());
    }

    #[test]
    fn a_no_newline_marker_is_other() {
        let lines = parse_diff("\\ No newline at end of file\n");
        assert_eq!(lines[0].kind, DiffKind::Other);
    }
}
