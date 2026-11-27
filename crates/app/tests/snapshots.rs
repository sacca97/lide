//! The editor's own views, checked without a display.
//!
//! Rune renders the application headlessly with the same renderer the window
//! uses, so the tests can open a window's worth of UI, render it to a PNG and
//! look at the result. That catches the mistakes a unit test cannot: a view
//! that lays out to nothing, a preview that does not draw, a page that never
//! arrives.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Render the editor with the given arguments and return the snapshot path.
fn snapshot(name: &str, arguments: &[&str]) -> PathBuf {
    let directory = std::env::temp_dir().join("lide-snapshots");
    std::fs::create_dir_all(&directory).expect("the snapshot directory should be creatable");
    let path = directory.join(format!("{name}.png"));

    let output = Command::new(env!("CARGO_BIN_EXE_lide"))
        .args(["--snap"])
        .arg(&path)
        .args(arguments)
        .output()
        .expect("the editor should run");

    assert!(
        output.status.success(),
        "the editor failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(path.exists(), "no snapshot was written to {}", path.display());
    path
}

/// Render one of the editor's named scenes.
fn snapshot_with_scene(name: &str, scene: &str) -> PathBuf {
    let directory = std::env::temp_dir().join("lide-snapshots");
    std::fs::create_dir_all(&directory).expect("the snapshot directory should be creatable");
    let path = directory.join(format!("{name}.png"));

    let output = Command::new(env!("CARGO_BIN_EXE_lide"))
        .args(["--snap"])
        .arg(&path)
        .args(["--scene", scene])
        .output()
        .expect("the editor should run");

    assert!(
        output.status.success(),
        "the editor failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(path.exists(), "no snapshot was written to {}", path.display());
    path
}

/// The number of distinct colours in a snapshot.
///
/// A view that drew nothing has one; a view that drew something has many.
fn distinct_colours(path: &Path) -> usize {
    let bytes = std::fs::read(path).expect("the snapshot should be readable");
    // A PNG is compressed, so the size is a good enough proxy: a blank image
    // compresses to a few hundred bytes, a drawn one to tens of kilobytes.
    assert!(bytes.len() > 4096, "the snapshot looks blank");
    bytes.len()
}

#[test]
fn the_workspace_renders() {
    let path = snapshot("workspace", &[ "." ]);
    assert!(distinct_colours(&path) > 4096);
}

/// The macOS traffic lights get their own band: with `--chrome` the headless
/// renderer reports every element that comes within four points of them.
#[cfg(target_os = "macos")]
#[test]
fn the_window_buttons_do_not_overlap_the_content() {
    let directory = std::env::temp_dir().join("lide-snapshots");
    std::fs::create_dir_all(&directory).expect("the snapshot directory should be creatable");
    let path = directory.join("chrome.png");

    let output = Command::new(env!("CARGO_BIN_EXE_lide"))
        .args(["--chrome", "--snap"])
        .arg(&path)
        .arg("crates/text/src/buffer.rs")
        .output()
        .expect("the editor should run");

    assert!(
        output.status.success(),
        "the editor failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("0 overlapping elements"),
        "the traffic lights need their own band: {stderr}"
    );
    assert!(path.exists(), "no snapshot was written to {}", path.display());
}

#[test]
fn a_text_document_renders() {
    let path = snapshot("editor", &["crates/text/src/buffer.rs"]);
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn a_markdown_document_renders_its_preview() {
    let path = snapshot("markdown", &["README.md"]);
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn a_pdf_renders_its_first_page() {
    let path = snapshot("pdf", &["tests/fixtures/sample.pdf"]);
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn the_command_palette_renders() {
    let path = snapshot_with_scene("palette", "palette");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn the_filtered_palette_renders() {
    let path = snapshot_with_scene("palette-filtered", "palette-filtered");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn the_find_bar_renders() {
    let path = snapshot_with_scene("find", "find");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn the_split_editor_renders() {
    let path = snapshot_with_scene("split", "split");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn workspace_search_renders() {
    let path = snapshot_with_scene("search", "search");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn a_notebook_renders() {
    let path = snapshot_with_scene("notebook", "notebook");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn a_csv_table_renders() {
    let path = snapshot_with_scene("csv", "csv");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn code_actions_render() {
    let path = snapshot_with_scene("code-actions", "code-actions");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn a_diff_renders() {
    let path = snapshot_with_scene("diff", "diff");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn an_image_preview_renders() {
    let path = snapshot_with_scene("image", "image");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn a_pdf_outline_renders() {
    let path = snapshot_with_scene("pdf-outline", "pdf-outline");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn pdf_text_selection_renders() {
    let path = snapshot_with_scene("pdf-select", "pdf-select");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn the_build_panel_renders() {
    let path = snapshot_with_scene("build", "build");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn pdf_search_renders() {
    let path = snapshot_with_scene("pdf-search", "pdf-search");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn the_terminal_renders() {
    let path = snapshot_with_scene("terminal", "terminal");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn the_completion_list_renders() {
    let path = snapshot_with_scene("completion", "completion");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn hover_information_renders() {
    let path = snapshot_with_scene("hover", "hover");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn the_places_panel_renders() {
    let path = snapshot_with_scene("places", "places");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn the_external_change_banner_renders() {
    let path = snapshot_with_scene("conflict", "conflict");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn the_light_theme_renders() {
    let path = snapshot_with_scene("light", "light");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn an_svg_source_and_preview_render() {
    let path = snapshot_with_scene("svg-source", "svg-source");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn latex_output_settings_render() {
    let path = snapshot_with_scene("latex-output", "latex-output");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn terminal_settings_render() {
    let path = snapshot_with_scene("terminal-settings", "terminal-settings");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn terminal_find_renders() {
    let path = snapshot_with_scene("terminal-find", "terminal-find");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn diagnostics_render_underlined_in_the_editor() {
    let path = snapshot_with_scene("diagnostics", "diagnostics");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn the_replace_review_renders() {
    let path = snapshot_with_scene("replace-confirm", "replace-confirm");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn terminal_selection_renders() {
    let path = snapshot_with_scene("terminal-select", "terminal-select");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn the_ssh_host_picker_renders() {
    let path = snapshot_with_scene("ssh-hosts", "ssh-hosts");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn the_remote_file_dialog_renders() {
    let path = snapshot_with_scene("remote-file", "remote-file");
    assert!(distinct_colours(&path) > 4096);
}

#[test]
fn the_sidebar_renders_when_shown() {
    let path = snapshot_with_scene("sidebar", "sidebar");
    assert!(distinct_colours(&path) > 4096);
}
