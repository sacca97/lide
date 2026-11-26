//! Replacing across the workspace, end to end.
//!
//! A real ripgrep search over a real temporary tree, then the background
//! replace worker, then the files on disk.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use lide_workspace::{DialogState, Workspace};
use rune_edit::TextEdit;

fn isolate_settings() {
    crate::isolate_settings();
}

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("lide-replace-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn wait_for(workspace: &mut Workspace, mut done: impl FnMut(&Workspace) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        workspace.poll();
        if done(workspace) {
            return true;
        }
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn replace_all_writes_every_matching_file() {
    isolate_settings();
    let directory = scratch("all");
    std::fs::write(directory.join("a.txt"), "cat cat\n").unwrap();
    std::fs::write(directory.join("b.txt"), "dog cat\n").unwrap();
    std::fs::write(directory.join("c.txt"), "nothing here\n").unwrap();

    let mut workspace = Workspace::new();
    workspace.open_folder(&directory).expect("the folder opens");
    workspace.search_query = TextEdit::new("cat");
    workspace.run_search();

    assert!(
        wait_for(&mut workspace, |workspace| {
            workspace
                .search
                .as_ref()
                .map(|search| !search.running)
                .unwrap_or(false)
        }),
        "the search finishes"
    );
    assert_eq!(
        workspace.search.as_ref().unwrap().matches.len(),
        2,
        "two lines hold the three occurrences"
    );

    workspace.search_replacement = TextEdit::new("bird");
    workspace.begin_replace_all();
    match workspace.dialog {
        Some(DialogState::ConfirmReplace { files, matches }) => {
            assert_eq!((files, matches), (2, 3), "two files, three matches");
        }
        _ => panic!("the confirmation dialog opens"),
    }

    workspace.confirm_replace_all();
    assert!(
        wait_for(&mut workspace, |workspace| workspace.replace.is_none()),
        "the replace finishes"
    );

    assert_eq!(
        std::fs::read_to_string(directory.join("a.txt")).unwrap(),
        "bird bird\n"
    );
    assert_eq!(
        std::fs::read_to_string(directory.join("b.txt")).unwrap(),
        "dog bird\n"
    );
    assert_eq!(
        std::fs::read_to_string(directory.join("c.txt")).unwrap(),
        "nothing here\n",
        "a file without the pattern is not rewritten"
    );
    let notice = workspace.notice.clone().unwrap_or_default();
    assert!(
        notice.contains("3") && notice.contains("2"),
        "the notice counts matches and files: {notice}"
    );

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_dirty_file_stops_replace_all() {
    isolate_settings();
    let directory = scratch("dirty");
    let path = directory.join("a.txt");
    std::fs::write(&path, "cat\n").unwrap();

    let mut workspace = Workspace::new();
    workspace.open_folder(&directory).expect("the folder opens");
    workspace.open_path(&path).expect("the file opens");
    workspace
        .active_editor_mut()
        .unwrap()
        .edit
        .insert("unsaved ");

    workspace.search_query = TextEdit::new("cat");
    workspace.run_search();
    assert!(
        wait_for(&mut workspace, |workspace| {
            workspace
                .search
                .as_ref()
                .map(|search| !search.running)
                .unwrap_or(false)
        }),
        "the search finishes"
    );

    workspace.search_replacement = TextEdit::new("bird");
    workspace.begin_replace_all();

    assert!(workspace.dialog.is_none(), "nothing to confirm");
    assert!(
        workspace
            .notice
            .as_deref()
            .unwrap_or_default()
            .contains("save"),
        "the notice asks for a save: {:?}",
        workspace.notice
    );

    let _ = std::fs::remove_dir_all(&directory);
}
