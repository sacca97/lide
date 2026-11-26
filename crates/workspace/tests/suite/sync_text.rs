//! The status-bar jump from a LaTeX source to its PDF.

use std::path::PathBuf;

use lide_workspace::Workspace;
use rune::{Env, PointerButton, Runtime};

fn isolate_settings() {
    crate::isolate_settings();
}

fn scratch() -> PathBuf {
    let directory = std::env::temp_dir().join(format!("lide-sync-text-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn rect(rt: &Runtime<Workspace>, key: &str) -> [f32; 4] {
    rt.elements()
        .iter()
        .find(|element| element.key.as_deref() == Some(key))
        .map(|element| element.rect)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

#[test]
fn the_status_bar_jumps_from_the_source_to_the_pdf() {
    isolate_settings();
    let directory = scratch();
    let tex = directory.join("main.tex");
    std::fs::write(
        &tex,
        "\\documentclass{article}\n\\begin{document}\nHello.\n\\end{document}\n",
    )
    .unwrap();

    let mut rt = Runtime::new(
        Workspace::new(),
        lide_workspace::view,
        (1200.0, 800.0),
        Env::default(),
    );
    rt.frame(0.0);
    rt.state.open_path(&tex).expect("the source opens");
    rt.invalidate();
    for _ in 0..40 {
        rt.frame(1.0 / 60.0);
        if rt
            .elements()
            .iter()
            .any(|element| element.key.as_deref() == Some("sync-pdf"))
        {
            break;
        }
    }

    let rect = rect(&rt, "sync-pdf");
    let (x, y) = (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
    rt.pointer_move(x, y);
    rt.button_down(x, y, PointerButton::Primary);
    rt.button_up(x, y, PointerButton::Primary);
    rt.frame(1.0 / 60.0);

    assert_eq!(
        rt.state.notice.as_deref(),
        Some("build the document first"),
        "the PDF jump asked for the built document"
    );

    let _ = std::fs::remove_dir_all(&directory);
}
