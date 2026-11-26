//! What happens when a file changes on disk while the editor has it open.
//!
//! The watcher is asynchronous, so each test polls the workspace the way the
//! editor does — once per frame — until the change is seen or a timeout
//! expires.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use lide_workspace::Workspace;

/// Keep the tests from reading or writing the real settings file.
fn isolate_settings() {
    use std::sync::OnceLock;
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let directory = std::env::temp_dir().join("lide-test-config");
        let _ = std::fs::create_dir_all(&directory);
        // SAFETY: this runs once, before any test touches the settings.
        unsafe { std::env::set_var("LIDE_CONFIG_DIR", &directory) };
    });
}

/// A temporary directory that is removed when it goes out of scope.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("lide-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the scratch directory should be creatable");
        Self { path }
    }

    fn file(&self, name: &str, text: &str) -> PathBuf {
        let path = self.path.join(name);
        std::fs::write(&path, text).expect("the file should be writable");
        path
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Poll the workspace until `done` is true, or give up after two seconds.
fn wait_for(workspace: &mut Workspace, mut done: impl FnMut(&Workspace) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        workspace.poll();
        if done(workspace) {
            return true;
        }
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_clean_file_is_reloaded_when_it_changes_on_disk() {
    let scratch = Scratch::new("reload");
    let file = scratch.file("notes.txt", "one\n");

    isolate_settings();
    let mut workspace = Workspace::new();
    workspace.open_folder(scratch.path()).expect("the folder should open");
    workspace.open_path(&file).expect("the file should open");

    std::fs::write(&file, "two\n").expect("the file should be writable");

    let reloaded = wait_for(&mut workspace, |workspace| {
        workspace
            .active_document()
            .and_then(|document| document.text())
            .map(|text| text == "two\n")
            .unwrap_or(false)
    });

    assert!(reloaded, "the document should have been reloaded from disk");
    assert!(
        !workspace.active_document().unwrap().is_dirty(),
        "a reload leaves nothing to save"
    );
    assert!(
        workspace.changed_on_disk.is_empty(),
        "a clean reload needs no warning"
    );
}

#[test]
fn a_dirty_file_is_reported_not_overwritten() {
    let scratch = Scratch::new("conflict");
    let file = scratch.file("notes.txt", "one\n");

    isolate_settings();
    let mut workspace = Workspace::new();
    workspace.open_folder(scratch.path()).expect("the folder should open");
    workspace.open_path(&file).expect("the file should open");

    // Edit in the editor, which makes the document dirty.
    workspace
        .active_editor_mut()
        .expect("a text document")
        .edit
        .replace_text("mine\n");

    std::fs::write(&file, "theirs\n").expect("the file should be writable");

    let reported = wait_for(&mut workspace, |workspace| {
        workspace.changed_on_disk.iter().any(|path| path == &file)
    });

    assert!(reported, "the change should have been reported");

    let text = workspace
        .active_document()
        .and_then(|document| document.text())
        .unwrap()
        .to_string();
    assert_eq!(
        text, "mine\n",
        "the editor must never silently discard the user's text"
    );
}

#[test]
fn the_editor_s_own_save_is_not_a_change() {
    let scratch = Scratch::new("own-save");
    let file = scratch.file("notes.txt", "one\n");

    isolate_settings();
    let mut workspace = Workspace::new();
    workspace.open_folder(scratch.path()).expect("the folder should open");
    workspace.open_path(&file).expect("the file should open");

    workspace
        .active_editor_mut()
        .expect("a text document")
        .edit
        .replace_text("saved\n");
    workspace.save(0);

    // Give the watcher time to report the write the editor itself made.
    for _ in 0..20 {
        workspace.poll();
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(
        workspace.changed_on_disk.is_empty(),
        "the editor's own save must not be reported as an external change"
    );
}

#[test]
fn accepting_the_change_takes_the_disk_version() {
    let scratch = Scratch::new("accept");
    let file = scratch.file("notes.txt", "one\n");

    isolate_settings();
    let mut workspace = Workspace::new();
    workspace.open_folder(scratch.path()).expect("the folder should open");
    workspace.open_path(&file).expect("the file should open");
    workspace
        .active_editor_mut()
        .expect("a text document")
        .edit
        .replace_text("mine\n");

    std::fs::write(&file, "theirs\n").expect("the file should be writable");
    assert!(
        wait_for(&mut workspace, |workspace| !workspace.changed_on_disk.is_empty()),
        "the change should have been reported"
    );

    workspace.accept_external_change();

    assert_eq!(
        workspace.active_document().and_then(|d| d.text()).unwrap(),
        "theirs\n"
    );
    assert!(
        workspace.changed_on_disk.is_empty(),
        "the warning clears once the choice is made"
    );
}

#[test]
fn keeping_the_local_text_dismisses_the_warning() {
    let scratch = Scratch::new("keep");
    let file = scratch.file("notes.txt", "one\n");

    isolate_settings();
    let mut workspace = Workspace::new();
    workspace.open_folder(scratch.path()).expect("the folder should open");
    workspace.open_path(&file).expect("the file should open");
    workspace
        .active_editor_mut()
        .expect("a text document")
        .edit
        .replace_text("mine\n");

    std::fs::write(&file, "theirs\n").expect("the file should be writable");
    assert!(
        wait_for(&mut workspace, |workspace| !workspace.changed_on_disk.is_empty()),
        "the change should have been reported"
    );

    workspace.keep_local_change();

    assert_eq!(
        workspace.active_document().and_then(|d| d.text()).unwrap(),
        "mine\n"
    );
    assert!(workspace.changed_on_disk.is_empty());
}

#[test]
fn a_search_match_opens_the_file_at_the_match() {
    if std::process::Command::new("rg").arg("--version").output().is_err() {
        eprintln!("skipping: ripgrep is not installed");
        return;
    }

    let scratch = Scratch::new("search");
    let file = scratch.file("notes.txt", "one\ntwo\nthree\n");

    isolate_settings();
    let mut workspace = Workspace::new();
    workspace.open_folder(scratch.path()).expect("the folder should open");
    workspace.search_query = rune_edit::TextEdit::new("three");
    workspace.run_search();

    for _ in 0..200 {
        workspace.poll();
        if workspace.match_count() > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(workspace.match_count(), 1, "one match");

    workspace.open_match(file.clone(), 3, 0);
    let editor = workspace.active_editor_mut().expect("a text document");
    assert_eq!(editor.cursor_line(), 3, "the caret is on the matching line");
}
