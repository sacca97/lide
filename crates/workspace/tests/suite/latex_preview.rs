//! The LaTeX source + PDF split.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use lide_workspace::{Document, Workspace};
use rune::{Env, PointerButton, Runtime};

fn isolate_settings() {
    crate::isolate_settings();
}

fn scratch() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let directory = std::env::temp_dir().join(format!(
        "lide-latex-preview-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn runtime() -> Runtime<Workspace> {
    isolate_settings();
    let mut rt = Runtime::new(
        Workspace::new(),
        lide_workspace::view,
        (1200.0, 800.0),
        Env::default(),
    );
    rt.frame(0.0);
    rt
}

fn rect(rt: &Runtime<Workspace>, key: &str) -> [f32; 4] {
    rt.elements()
        .iter()
        .find(|element| element.key.as_deref() == Some(key))
        .map(|element| element.rect)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click(rt: &mut Runtime<Workspace>, key: &str) {
    let rect = rect(rt, key);
    let (x, y) = (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
    rt.pointer_move(x, y);
    rt.button_down(x, y, PointerButton::Primary);
    rt.button_up(x, y, PointerButton::Primary);
    rt.frame(1.0 / 60.0);
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
    assert!(workspace.build.is_none(), "opening the preview does not build");
    assert!(matches!(
        workspace.documents.get(1),
        Some(Document::Pdf(_))
    ));

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_plain_click_in_a_latex_source_still_moves_the_caret() {
    isolate_settings();
    let directory = scratch();
    let tex = directory.join("main.tex");
    std::fs::write(&tex, "one\ntwo\nthree\nfour\n").unwrap();

    let mut rt = runtime();
    rt.state.open_folder(&directory).expect("the folder opens");
    rt.state.open_path(&tex).expect("the source opens");
    rt.invalidate();
    rt.frame(0.0);
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
        if rt
            .elements()
            .iter()
            .any(|element| element.key.as_deref() == Some("editor-0"))
        {
            break;
        }
    }

    let editor = rect(&rt, "editor-0");
    let line = rt.state.settings.line_height();
    let x = editor[0] + 30.0;
    let y = editor[1] + 8.0 + 2.5 * line;
    rt.pointer_move(x, y);
    rt.button_down(x, y, PointerButton::Primary);
    rt.button_up(x, y, PointerButton::Primary);
    rt.frame(1.0 / 60.0);

    assert_eq!(
        rt.state.active_editor_mut().unwrap().cursor_line(),
        3,
        "the text field still takes plain clicks"
    );

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn ctrl_click_in_a_chapter_jumps_to_the_main_pdf() {
    isolate_settings();
    let directory = scratch();
    let paper = directory.join("paper.tex");
    let chapter = directory.join("chapter.tex");
    let pdf = directory.join("paper.pdf");
    std::fs::write(
        &paper,
        "\\documentclass{article}\n\\begin{document}\n\\input{chapter}\n\\end{document}\n",
    )
    .unwrap();
    std::fs::write(&chapter, "Chapter text.\n").unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/sample.pdf"),
        &pdf,
    )
    .unwrap();
    std::fs::write(
        directory.join("paper.synctex"),
        format!(
            "SyncTeX Version:1\nInput:1:{}\nOutput:pdf\nMagnification:1000\nUnit:1\nX Offset:0\nY Offset:0\nContent:\n{{1\n\
             [1,1:0,0:39158276,55380990,0\n\
             (1,2:3552215,18442772:15795512,444328,141880\n\
             x1,2:4571361,18442772\n\
             x1,1:5429155,18442772\n\
             x1,1:6453622,18442772\n\
             )\n\
             (1,1:3552215,33984329:15795512,541848,9432\n\
             x1,1:3945431,33984329\n\
             x1,1:6086078,33984329\n\
             )\n\
             ]\n}}1\nPostamble:\n",
            chapter.display()
        ),
    )
    .unwrap();

    let mut rt = runtime();
    rt.state.open_folder(&directory).expect("the folder opens");
    rt.state.open_path(&chapter).expect("the chapter opens");
    rt.state.primary_modifier = true;
    rt.invalidate();
    rt.frame(0.0);
    for _ in 0..10 {
        rt.state.poll();
        rt.frame(1.0 / 60.0);
    }

    // The text field puts the caret where the click is; the jump starts from
    // that caret, with the modifier held and no key pressed.
    let editor = rect(&rt, "editor-0");
    let line = rt.state.settings.line_height();
    let x = editor[0] + 60.0;
    let y = editor[1] + 8.0 + 0.5 * line;
    rt.pointer_move(x, y);
    // The pointer reports the modifiers it moves with, and the headless one
    // holds none; hold Ctrl for the press, as a hand would.
    rt.state.primary_modifier = true;
    rt.button_down(x, y, PointerButton::Primary);
    rt.button_up(x, y, PointerButton::Primary);
    rt.frame(1.0 / 60.0);
    rt.state.primary_modifier = true;
    rt.state.poll();
    rt.frame(1.0 / 60.0);

    assert!(rt.state.build.is_none(), "the click does not build");
    assert!(rt.state.notice.is_none(), "{:?}", rt.state.notice);
    assert!(rt.state.split_document.is_some(), "the PDF opened beside");
    assert_eq!(
        rt.state.active_document().and_then(|document| document.path()),
        Some(chapter.as_path()),
        "the chapter stays on the left"
    );

    // The place is the box that really holds line 1, not the box that was
    // closed on the next line.
    let pdf_index = rt.state.split_document.expect("the PDF is open");
    let pointer = match &rt.state.documents[pdf_index] {
        lide_workspace::Document::Pdf(view) => view.pointer,
        _ => None,
    };
    let (x, y) = pointer.expect("the place is known");
    assert!(x > 0.0 && x < 612.0, "x {x}");
    assert!(y > 0.0 && y < 792.0, "y {y}");

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn double_clicking_the_pdf_lands_on_the_source_line() {
    isolate_settings();
    let directory = scratch();
    let paper = directory.join("paper.tex");
    let chapter = directory.join("chapter.tex");
    let pdf = directory.join("paper.pdf");
    std::fs::write(
        &paper,
        "\\documentclass{article}\n\\begin{document}\n\\input{chapter}\n\\end{document}\n",
    )
    .unwrap();
    std::fs::write(&chapter, "Chapter text.\n").unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/sample.pdf"),
        &pdf,
    )
    .unwrap();
    // The build wrote the PDF beside the source, so the sidecar is named after
    // it.
    std::fs::write(
        directory.join("paper.synctex"),
        format!(
            "SyncTeX Version:1\nInput:1:{}\nOutput:pdf\nMagnification:1000\nUnit:1\nX Offset:0\nY Offset:0\nContent:\n{{1\n\
             [1,1:0,0:39158276,55380990,0\n\
             (1,2:3552215,18442772:15795512,444328,141880\n\
             x1,2:4571361,18442772\n\
             x1,1:5429155,18442772\n\
             x1,1:6453622,18442772\n\
             )\n\
             (1,1:3552215,33984329:15795512,541848,9432\n\
             x1,1:3945431,33984329\n\
             x1,1:6086078,33984329\n\
             )\n\
             ]\n}}1\nPostamble:\n",
            chapter.display()
        ),
    )
    .unwrap();

    let mut rt = runtime();
    rt.state.images = Some(rune_image::ImageCache::new(std::sync::Arc::new(
        || {},
    )));
    rt.state.open_folder(&directory).expect("the folder opens");
    rt.state.open_path(&chapter).expect("the chapter opens");
    rt.state.open_pdf_beside_source();
    rt.invalidate();
    for _ in 0..120 {
        rt.state.poll();
        rt.frame(1.0 / 60.0);
        if rt
            .elements()
            .iter()
            .any(|element| element.key.as_deref() == Some("pdf-page-1"))
        {
            break;
        }
    }

    // The page is fitted to the pane once its width is known; let that settle.
    for _ in 0..60 {
        if rt.state.poll() {
            rt.invalidate();
        }
        rt.frame(1.0 / 60.0);
    }
    let page = rect(&rt, "pdf-page-1");
    let scale = {
        let index = rt.state.split_document.expect("the PDF is beside");
        match &rt.state.documents[index] {
            lide_workspace::Document::Pdf(view) => view.scale(),
            _ => 1.0,
        }
    };
    let x = page[0] + 90.0 * scale;
    let y = page[1] + 279.0 * scale;
    rt.pointer_move(x, y);
    // A hand does not hold the pointer still between two clicks, so the
    // second one lands a few points away.
    for offset in [0.0f32, 6.0] {
        rt.pointer_move(x + offset, y);
        rt.button_down(x + offset, y, PointerButton::Primary);
        rt.button_up(x + offset, y, PointerButton::Primary);
    }
    rt.frame(1.0 / 60.0);
    rt.state.poll();
    rt.frame(1.0 / 60.0);

    assert!(
        rt.state.notice.is_none(),
        "{:?}",
        rt.state.notice
    );
    assert_eq!(
        rt.state.active_document().and_then(|document| document.path()),
        Some(chapter.as_path()),
        "the chapter opens"
    );
    assert_eq!(
        rt.state.active_editor_mut().map(|editor| editor.cursor_line()),
        Some(1),
        "the click found the line the text came from"
    );
    assert!(
        rt.state
            .scroll_requests
            .iter()
            .any(|(key, _, _)| key == "editor-scroll-0"),
        "the source is scrolled to the line: {:?} notice {:?}", rt.state.scroll_requests, rt.state.notice
    );

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn the_preview_button_opens_the_existing_pdf_without_building() {
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

    let mut rt = runtime();
    rt.state.open_folder(&directory).expect("the folder opens");
    rt.state.open_path(&tex).expect("the source opens");
    rt.invalidate();
    rt.frame(0.0);
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
        if rt
            .elements()
            .iter()
            .any(|element| element.key.as_deref() == Some("latex-preview"))
        {
            break;
        }
    }

    click(&mut rt, "latex-preview");

    assert!(rt.state.build.is_none(), "the button does not compile");
    assert_eq!(rt.state.documents.len(), 2, "source and PDF are open");
    assert!(
        rt.state.split_document.is_some(),
        "the PDF opens in the other pane"
    );

    click(&mut rt, "icon-button-Close split");
    assert!(
        rt.state.split_document.is_none(),
        "the X closes the preview pane"
    );

    let _ = std::fs::remove_dir_all(&directory);
}
