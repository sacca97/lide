//! The diff and conflict viewers, against real git repositories.
//!
//! There is no staging or commit UI any more: the editor only shows a file's
//! diff, and a clean conflict view for an unmerged working copy.

use std::path::{Path, PathBuf};
use std::process::Command;

use lide_workspace::{Document, Workspace};

fn isolate_settings() {
    crate::isolate_settings();
}

fn git(directory: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        .env("GIT_AUTHOR_NAME", "lide")
        .env("GIT_AUTHOR_EMAIL", "lide@example.invalid")
        .env("GIT_COMMITTER_NAME", "lide")
        .env("GIT_COMMITTER_EMAIL", "lide@example.invalid")
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("lide-git-diff-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn repository(name: &str) -> PathBuf {
    let directory = scratch(name);
    git(&directory, &["init", "--initial-branch=main"]);
    std::fs::write(directory.join("a.txt"), "one\n").unwrap();
    git(&directory, &["add", "--all"]);
    git(&directory, &["commit", "-m", "first"]);
    directory
}

#[test]
fn the_active_file_opens_as_a_diff() {
    isolate_settings();
    let directory = repository("plain");
    std::fs::write(directory.join("a.txt"), "two\n").unwrap();

    let mut workspace = Workspace::new();
    workspace.open_folder(&directory).expect("the folder opens");
    workspace
        .open_path(directory.join("a.txt"))
        .expect("the file opens");
    workspace.refresh_git(true);
    workspace.open_diff_for_active();

    match workspace.active_document() {
        Some(Document::Diff(diff)) => {
            assert_eq!(diff.path, PathBuf::from("a.txt"));
            assert!(!diff.staged);
            assert!(!diff.conflict);
            assert_eq!(diff.title(), "a.txt (diff)");
            assert!(
                diff.lines.iter().any(|line| line.text.contains("one")),
                "the old line is in the diff"
            );
            assert!(
                diff.lines.iter().any(|line| line.text.contains("two")),
                "the new line is in the diff"
            );
        }
        _ => panic!("the diff is the active document"),
    }

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn an_unmerged_file_opens_as_a_conflict_view() {
    isolate_settings();
    let directory = repository("conflict");
    git(&directory, &["checkout", "-b", "other"]);
    std::fs::write(directory.join("a.txt"), "two\n").unwrap();
    git(&directory, &["commit", "-am", "other"]);
    git(&directory, &["checkout", "main"]);
    std::fs::write(directory.join("a.txt"), "three\n").unwrap();
    git(&directory, &["commit", "-am", "main"]);
    // Leave the merge stopped with the conflict in the worktree.
    let _ = Command::new("git")
        .args(["merge", "other"])
        .current_dir(&directory)
        .output();

    let mut workspace = Workspace::new();
    workspace.open_folder(&directory).expect("the folder opens");
    workspace
        .open_path(directory.join("a.txt"))
        .expect("the file opens");
    workspace.refresh_git(true);
    workspace.open_diff_for_active();

    match workspace.active_document() {
        Some(Document::Diff(diff)) => {
            assert!(diff.conflict, "the conflict view is chosen");
            assert!(!diff.staged, "a conflict is not a staged diff");
            assert_eq!(diff.title(), "a.txt (conflict)");
            assert!(
                diff.lines.iter().any(|line| line.text.starts_with("<<<<<<<")),
                "ours starts: {:?}",
                diff.lines.iter().map(|line| &line.text).collect::<Vec<_>>()
            );
            assert!(diff.lines.iter().any(|line| line.text == "three"));
            assert!(diff.lines.iter().any(|line| line.text == "two"));
            assert!(diff.lines.iter().any(|line| line.text.starts_with(">>>>>>>")));
        }
        _ => panic!("the conflict view is the active document"),
    }

    let _ = std::fs::remove_dir_all(&directory);
}
