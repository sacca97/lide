//! Language servers, against a real one.
//!
//! The test skips itself when no server is installed, so it is useful on a
//! developer machine and harmless in a container.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use lide_workspace::Workspace;

/// Keep the tests from reading or writing the real settings file.
fn isolate_settings() {
    crate::isolate_settings();
}

fn installed(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[test]
fn clangd_reports_an_error_in_a_c_file() {
    if !installed("clangd") {
        eprintln!("skipping: clangd is not installed");
        return;
    }
    isolate_settings();

    let root: PathBuf = std::env::temp_dir().join("lide-lsp-test");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    // `undefined_identifier` is never declared, which clangd reports.
    let file = root.join("bad.c");
    std::fs::write(&file, "int main(void) { return undefined_identifier; }\n").unwrap();

    let mut workspace = Workspace::new();
    workspace.open_folder(&root).expect("the folder should open");
    workspace.open_path(&file).expect("the file should open");

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut found = Vec::new();
    while Instant::now() < deadline {
        workspace.poll();
        found = workspace.servers.diagnostics_for(&file).to_vec();
        if !found.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    assert!(
        !found.is_empty(),
        "clangd should have reported the undefined identifier"
    );
    assert!(
        found
            .iter()
            .any(|diag| diag.message.contains("undefined_identifier")),
        "the diagnostic should name the identifier, got: {:?}",
        found.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(
        found.iter().any(|diag| diag.severity.is_problem()),
        "at least one diagnostic is an error or warning"
    );

    workspace.servers.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn rust_analyzer_starts_for_a_rust_project() {
    if !installed("rust-analyzer") {
        eprintln!("skipping: rust-analyzer is not installed");
        return;
    }
    isolate_settings();

    // This repository is a Cargo workspace, so rust-analyzer has something to
    // start on.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let file = root.join("crates/workspace/src/lib.rs");

    let mut workspace = Workspace::new();
    workspace.open_folder(&root).expect("the folder should open");
    workspace.open_path(&file).expect("the file should open");

    let deadline = Instant::now() + Duration::from_secs(60);
    let mut status = None;
    while Instant::now() < deadline {
        workspace.poll();
        status = workspace.servers.status();
        if status.as_deref() == Some("1 language server") {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    assert_eq!(
        status.as_deref(),
        Some("1 language server"),
        "rust-analyzer should have started"
    );

    workspace.servers.shutdown();
}

#[test]
fn clangd_answers_a_hover() {
    if !installed("clangd") {
        eprintln!("skipping: clangd is not installed");
        return;
    }
    isolate_settings();

    let root: PathBuf = std::env::temp_dir().join("lide-lsp-hover");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let file = root.join("hover.c");
    std::fs::write(
        &file,
        "int add(int a, int b) { return a + b; }\nint main(void) { return add(1, 2); }\n",
    )
    .unwrap();

    let mut workspace = Workspace::new();
    workspace.open_folder(&root).expect("the folder should open");
    workspace.open_path(&file).expect("the file should open");

    // Put the caret on `add` in the call.
    workspace
        .active_editor_mut()
        .expect("a text document")
        .go_to(2, 24);

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut hover = None;
    while Instant::now() < deadline {
        workspace.poll();
        // The server has to be up before the request can be sent.
        workspace.show_hover();
        if let Some(text) = workspace.hover.clone() {
            hover = Some(text);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let hover = hover.expect("clangd should answer a hover");
    assert!(
        hover.contains("add"),
        "the hover should describe the symbol, got: {hover:?}"
    );

    workspace.servers.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn synctex_reports_missing_data_instead_of_panicking() {
    isolate_settings();

    let root: PathBuf = std::env::temp_dir().join("lide-synctex-test");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let tex = root.join("main.tex");
    std::fs::write(&tex, "\\documentclass{article}\n\\begin{document}hi\\end{document}\n").unwrap();
    // A PDF next to it, so that the "build first" check passes.
    std::fs::write(root.join("main.pdf"), b"%PDF-1.7\n").unwrap();

    let mut workspace = Workspace::new();
    workspace.open_folder(&root).expect("the folder should open");
    workspace.open_path(&tex).expect("the file should open");

    workspace.sync_forward();

    // The fake PDF has no .synctex file. That is a notice, not a panic.
    assert!(
        workspace.notice.is_some(),
        "missing SyncTeX data should be reported"
    );
    assert!(workspace.servers.all_diagnostics().is_empty());

    workspace.servers.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}
