//! The file tree starts collapsed and remembers what was opened.

use std::path::PathBuf;

use lide_workspace::Workspace;

fn isolate_settings() {
    crate::isolate_settings();
}

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("lide-tree-state-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("src")).unwrap();
    std::fs::create_dir_all(directory.join("docs")).unwrap();
    std::fs::write(directory.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(directory.join("docs/readme.md"), "# docs\n").unwrap();
    directory
}

#[test]
fn a_new_workspace_starts_collapsed_and_remembers_an_opened_folder() {
    isolate_settings();
    let root = scratch("one");
    let src = root.join("src");
    let docs = root.join("docs");

    let mut workspace = Workspace::new();
    workspace.open_folder(&root).expect("the folder opens");
    assert!(!workspace.directory_expanded(&src), "a new tree is collapsed");
    assert!(!workspace.directory_expanded(&docs));

    workspace.toggle_directory(&src);
    assert!(workspace.directory_expanded(&src));
    assert!(!workspace.directory_expanded(&docs));

    // A fresh window reads the saved state for the same workspace.
    let mut reopened = Workspace::new();
    reopened.open_folder(&root).expect("the folder opens again");
    assert!(
        reopened.directory_expanded(&src),
        "the opened folder was remembered"
    );
    assert!(!reopened.directory_expanded(&docs));

    // A different workspace still starts fully collapsed.
    let other = scratch("two");
    let other_src = other.join("src");
    let mut fresh = Workspace::new();
    fresh.open_folder(&other).expect("the other folder opens");
    assert!(!fresh.directory_expanded(&other_src));

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&other);
}
