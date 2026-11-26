//! The LaTeX source + PDF split.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use lide_workspace::{Document, Workspace};

fn isolate_settings() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let directory = std::env::temp_dir().join("lide-test-config-latex-preview");
        let _ = std::fs::create_dir_all(&directory);
        // SAFETY: this runs once, before any test touches the settings.
        unsafe { std::env::set_var("LIDE_CONFIG_DIR", &directory) };
    });
}

fn scratch() -> PathBuf {
    let directory = std::env::temp_dir().join(format!("lide-latex-preview-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

#[test]
fn the_built_pdf_opens_beside_the_source() {
    isolate_settings();
    let directory = scratch();
    let tex = directory.join("main.tex");
    let pdf = directory.join("main.pdf");
    std::fs::write(&tex, "\\documentclass{article}\n").unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/sample.pdf"),
        &pdf,
    )
    .unwrap();

    let mut workspace = Workspace::new();
    workspace.settings.split = false;
    workspace.open_folder(&directory).expect("the folder opens");
    workspace.open_path(&tex).expect("the source opens");
    workspace.open_pdf_beside_source();

    assert_eq!(workspace.documents.len(), 2, "source and PDF");
    let active = workspace
        .active_document()
        .and_then(|document| document.path());
    assert_eq!(active, Some(tex.as_path()), "the source stays on the left");
    let secondary = workspace
        .split_document
        .and_then(|index| workspace.documents.get(index))
        .and_then(|document| document.path());
    assert_eq!(secondary, Some(pdf.as_path()), "the PDF is on the right");
    assert_eq!(workspace.focused_pane, 0);
    assert!(workspace.settings.split, "the split is remembered");
    assert!(matches!(
        workspace.documents.get(1),
        Some(Document::Pdf(_))
    ));

    let _ = std::fs::remove_dir_all(&directory);
}
