//! The split divider, dragged through Rune's real event routing.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use lide_workspace::Workspace;
use rune::{Env, PointerButton, Runtime};

fn isolate_settings() {
    crate::isolate_settings();
}

fn scratch() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let directory = std::env::temp_dir().join(format!(
        "lide-split-{}-{}",
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

fn settle(rt: &mut Runtime<Workspace>, key: &str) {
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
        let settled = rt
            .elements()
            .iter()
            .find(|element| element.key.as_deref() == Some(key))
            .is_some_and(|element| !element.moving);
        if settled {
            break;
        }
    }
}

fn drag(rt: &mut Runtime<Workspace>, from: (f32, f32), to: (f32, f32)) {
    rt.pointer_move(from.0, from.1);
    rt.button_down(from.0, from.1, PointerButton::Primary);
    rt.frame(1.0 / 60.0);
    rt.pointer_move(to.0, to.1);
    rt.frame(1.0 / 60.0);
    rt.button_up(to.0, to.1, PointerButton::Primary);
    rt.frame(1.0 / 60.0);
}

#[test]
fn dragging_the_split_divider_changes_the_ratio() {
    let directory = scratch();
    let a = directory.join("a.txt");
    let b = directory.join("b.txt");
    std::fs::write(&a, "a\n").unwrap();
    std::fs::write(&b, "b\n").unwrap();

    let mut rt = runtime();
    rt.state.open_path(&a).expect("the first file opens");
    rt.state.open_path(&b).expect("the second file opens");
    rt.state.settings.split_ratio = 0.5;
    rt.state.split_document = Some(1);
    rt.state.active = 0;
    rt.invalidate();
    rt.frame(0.0);
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
        let settled = rt
            .elements()
            .iter()
            .find(|element| element.key.as_deref() == Some("split-resizer"))
            .is_some_and(|element| !element.moving);
        if settled {
            break;
        }
    }

    let before = rt.state.settings.split_ratio;
    let before_width = rect(&rt, "split-left")[2];
    let divider = rect(&rt, "split-resizer");
    let start = (divider[0] + divider[2] / 2.0, divider[1] + divider[3] / 2.0);
    rt.pointer_move(start.0, start.1);
    rt.button_down(start.0, start.1, PointerButton::Primary);
    rt.frame(1.0 / 60.0);
    rt.pointer_move(start.0 + 120.0, start.1);
    rt.frame(1.0 / 60.0);
    rt.button_up(start.0 + 120.0, start.1, PointerButton::Primary);
    rt.frame(1.0 / 60.0);

    assert!(
        rt.state.settings.split_ratio > before,
        "dragging right gave the left pane more room: {} -> {}",
        before,
        rt.state.settings.split_ratio
    );
    assert!(rt.state.settings.split_ratio <= 0.8);
    assert!(
        rect(&rt, "split-left")[2] > before_width,
        "the pane follows the new ratio: {} -> {}",
        before_width,
        rect(&rt, "split-left")[2]
    );

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn dragging_the_markdown_divider_changes_the_source_and_preview() {
    let directory = scratch();
    let document = directory.join("notes.md");
    std::fs::write(&document, "# Title\n\nSome text.\n\n## Section\n\nMore.\n").unwrap();

    let mut rt = runtime();
    rt.state.open_path(&document).expect("the Markdown file opens");
    rt.state.settings.markdown_ratio = 0.5;
    rt.state.settings.show_outline = false;
    rt.invalidate();
    rt.frame(0.0);
    settle(&mut rt, "markdown-resizer-0");

    let before = rt.state.settings.markdown_ratio;
    let before_width = rect(&rt, "markdown-scroll-0")[2];
    let divider = rect(&rt, "markdown-resizer-0");
    let start = (divider[0] + divider[2] / 2.0, divider[1] + divider[3] / 2.0);
    drag(&mut rt, start, (start.0 + 120.0, start.1));

    assert!(
        rt.state.settings.markdown_ratio > before,
        "dragging right gave the source more room: {} -> {}",
        before,
        rt.state.settings.markdown_ratio
    );
    assert!(
        rect(&rt, "markdown-scroll-0")[2] > before_width,
        "the source follows the new ratio: {} -> {}",
        before_width,
        rect(&rt, "markdown-scroll-0")[2]
    );

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn dragging_a_tab_to_the_right_half_creates_a_split() {
    let directory = scratch();
    let a = directory.join("a.txt");
    let b = directory.join("b.txt");
    std::fs::write(&a, "a\n").unwrap();
    std::fs::write(&b, "b\n").unwrap();

    let mut rt = runtime();
    rt.state.open_path(&a).expect("the first file opens");
    rt.state.open_path(&b).expect("the second file opens");
    rt.state.split_document = None;
    rt.state.settings.split = false;
    rt.invalidate();
    rt.frame(0.0);
    settle(&mut rt, "editor-primary");

    let tab = rect(&rt, "tab-0");
    let from = (tab[0] + tab[2] / 2.0, tab[1] + tab[3] / 2.0);
    let editor = rect(&rt, "editor-primary");
    let to = (
        editor[0] + editor[2] * 0.75,
        editor[1] + editor[3] / 2.0,
    );
    drag(&mut rt, from, to);

    assert_eq!(rt.state.split_document, Some(0), "the tab opened on the right");
    assert_eq!(rt.state.active, 1, "the other document stays on the left");
    assert_eq!(rt.state.focused_pane, 1);

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn dragging_a_tab_into_the_other_pane_swaps_the_documents() {
    let directory = scratch();
    let a = directory.join("a.txt");
    let b = directory.join("b.txt");
    std::fs::write(&a, "a\n").unwrap();
    std::fs::write(&b, "b\n").unwrap();

    let mut rt = runtime();
    rt.state.open_path(&a).expect("the first file opens");
    rt.state.open_path(&b).expect("the second file opens");
    rt.state.active = 0;
    rt.state.split_document = Some(1);
    rt.state.settings.split = true;
    rt.invalidate();
    rt.frame(0.0);
    settle(&mut rt, "split-resizer");

    let tab = rect(&rt, "tab-0");
    let from = (tab[0] + tab[2] / 2.0, tab[1] + tab[3] / 2.0);
    let right = rect(&rt, "split-right");
    let to = (right[0] + right[2] / 2.0, right[1] + right[3] / 2.0);
    drag(&mut rt, from, to);

    assert_eq!(rt.state.split_document, Some(0), "the dragged tab is on the right");
    assert_eq!(rt.state.active, 1, "the old right document moved left");
    assert_eq!(rt.state.focused_pane, 1, "the drop target has focus");

    let _ = std::fs::remove_dir_all(&directory);
}
