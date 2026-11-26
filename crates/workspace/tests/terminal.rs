//! The integrated terminal, end to end.
//!
//! A real shell runs in a real PTY; the test types a command the way a person
//! would, polls the workspace the way the editor does, and looks at the grid.

use std::time::{Duration, Instant};

use lide_workspace::Workspace;

/// Keep the tests from reading or writing the real settings file.
fn isolate_settings() {
    use std::sync::OnceLock;
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let directory = std::env::temp_dir().join("lide-test-config-terminal");
        let _ = std::fs::create_dir_all(&directory);
        // SAFETY: this runs once, before any test touches the settings.
        unsafe { std::env::set_var("LIDE_CONFIG_DIR", &directory) };
    });
}

/// Poll until `done` is true, or give up.
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

/// The terminal's grid as text.
fn screen(workspace: &Workspace) -> String {
    workspace
        .documents
        .iter()
        .find_map(|document| match document {
            lide_workspace::Document::Terminal(terminal) => {
                Some(terminal.emulator.snapshot().rows.join("\n"))
            }
            _ => None,
        })
        .unwrap_or_default()
}

#[test]
fn a_terminal_runs_a_command_and_shows_its_output() {
    isolate_settings();

    let mut workspace = Workspace::new();
    workspace
        .open_terminal()
        .expect("a terminal should start in the current directory");

    // The shell prints a prompt; wait for it so that the next write is not
    // swallowed by a shell that has not started reading yet.
    assert!(
        wait_for(&mut workspace, |workspace| !screen(workspace).is_empty()),
        "the shell should print a prompt"
    );

    if let Some(terminal) = workspace.active_terminal_mut() {
        terminal.write("echo lide-integration-test\r");
    }

    let found = wait_for(&mut workspace, |workspace| {
        screen(workspace).contains("lide-integration-test")
    });
    assert!(
        found,
        "the command's output should reach the terminal grid:\n{}",
        screen(&workspace)
    );
}

#[test]
fn a_terminal_is_a_document_with_a_title() {
    isolate_settings();

    let mut workspace = Workspace::new();
    workspace.open_terminal().expect("a terminal should start");

    let document = workspace.active_document().expect("a document");
    assert!(!document.title().is_empty(), "the tab needs a title");
    assert!(!document.is_dirty(), "a terminal cannot be dirty");
    assert!(!workspace.has_unsaved_changes());

    // Closing it must not leave the shell running.
    workspace.close(0);
    assert!(workspace.documents.is_empty());
}

#[test]
fn the_visible_grid_is_what_copy_takes() {
    isolate_settings();

    let mut workspace = Workspace::new();
    workspace
        .open_terminal()
        .expect("a terminal should start in the current directory");
    assert!(
        wait_for(&mut workspace, |workspace| !screen(workspace).is_empty()),
        "the shell should print a prompt"
    );

    if let Some(terminal) = workspace.active_terminal_mut() {
        terminal.write("echo lide-copy-test\r");
    }
    assert!(
        wait_for(&mut workspace, |workspace| {
            screen(workspace).contains("lide-copy-test")
        }),
        "the output should reach the grid:\n{}",
        screen(&workspace)
    );

    let text = workspace.terminal_text().expect("the terminal has text");
    assert!(text.contains("lide-copy-test"), "copied text: {text:?}");
    assert!(text.lines().count() > 1, "the grid has more than one line");
}

#[test]
fn the_terminal_scrollback_can_be_searched() {
    isolate_settings();

    let mut workspace = Workspace::new();
    workspace
        .open_terminal()
        .expect("a terminal should start in the current directory");
    assert!(
        wait_for(&mut workspace, |workspace| !screen(workspace).is_empty()),
        "the shell should print a prompt"
    );

    if let Some(terminal) = workspace.active_terminal_mut() {
        terminal.write("seq 1 200 | sed 's/^/marker-/'\r");
    }
    assert!(
        wait_for(&mut workspace, |workspace| {
            screen(workspace).contains("marker-200")
        }),
        "the output should reach the grid"
    );

    workspace.open_terminal_find();
    assert!(workspace.terminal_find.open);
    workspace.terminal_find.query = rune_edit::TextEdit::new("marker-7");
    workspace.terminal_find_next();

    assert!(
        workspace.terminal_find.message.contains(" of "),
        "the bar counts the matches: {:?}",
        workspace.terminal_find.message
    );
    let grid = screen(&workspace);
    assert!(
        grid.lines().next().unwrap_or_default() == "marker-7",
        "the match is brought to the top of the view: {:?}",
        grid.lines().take(3).collect::<Vec<_>>()
    );

    workspace.terminal_find.close();
    assert!(!workspace.terminal_find.open);
}

#[test]
fn a_terminal_uses_the_configured_shell_and_directory() {
    isolate_settings();
    let root = std::env::temp_dir().join(format!("lide-terminal-prefs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("sub")).unwrap();

    let mut workspace = Workspace::new();
    workspace.open_folder(&root).expect("the folder opens");
    workspace.settings.terminal_shell = "/bin/sh".to_string();
    // A relative directory is taken from the project root.
    workspace.settings.terminal_directory = "sub".to_string();
    workspace.open_terminal().expect("the terminal starts");

    let terminal = workspace.active_terminal_mut().expect("the terminal is active");
    assert_eq!(terminal.session.shell, "/bin/sh");
    assert_eq!(terminal.directory, root.join("sub"));

    let _ = std::fs::remove_dir_all(&root);
}
