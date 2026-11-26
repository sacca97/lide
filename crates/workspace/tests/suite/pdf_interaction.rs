//! The PDF viewer's handlers, driven through Rune's real event routing.
//!
//! The viewer's closures used to name `PdfView` as the event state, which is
//! the document inside the workspace rather than the workspace itself. These
//! tests click and drag the way the window does, so a wrong state type fails
//! here instead of only in a hand-driven session.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use lide_workspace::{Document, Workspace};
use rune::edit::{Key, KeyEvent, Mods};
use rune::{Env, PointerButton, Runtime, WheelEvent};

/// PDFium's engine is process-wide, and the tests share its settings
/// directory; running them one at a time keeps both simple.
fn pdf_guard() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn isolate_settings() {
    crate::isolate_settings();
}

fn runtime() -> Runtime<Workspace> {
    runtime_sized((1200.0, 800.0))
}

fn runtime_sized(size: (f32, f32)) -> Runtime<Workspace> {
    isolate_settings();
    let mut workspace = Workspace::new();
    // The viewer's render results are loaded through this cache.
    workspace.images = Some(rune_image::ImageCache::new(std::sync::Arc::new(|| {})));
    let mut rt = Runtime::new(
        workspace,
        lide_workspace::view,
        size,
        Env::default(),
    );
    rt.on_key = Some(lide_workspace::keys::handle);
    rt.frame(0.0);
    rt
}

fn open_pdf_page(rt: &mut Runtime<Workspace>, path: &std::path::Path) {
    rt.state.open_path(path).expect("the fixture opens");
    rt.invalidate();
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        rt.state.poll();
        rt.frame(1.0 / 60.0);
        if rt
            .elements()
            .iter()
            .any(|element| element.key.as_deref() == Some("pdf-page-0"))
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the page never rendered");
}

fn open_pdf_settled(rt: &mut Runtime<Workspace>, settled_path: &std::path::Path) {
    open_pdf_page(rt, settled_path);
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        rt.state.poll();
        rt.frame(1.0 / 60.0);
        let elements = rt.elements();
        // A new view fades and springs into place. Click at its settled
        // position, not where the first frame drew it.
        let settled = elements
            .iter()
            .filter(|element| {
                element
                    .key
                    .as_deref()
                    .is_some_and(|key| key.starts_with("pdf-"))
            })
            .all(|element| !element.moving);
        if settled {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the page never settled");
}

fn open_pdf(rt: &mut Runtime<Workspace>) {
    open_pdf_settled(
        rt,
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/sample.pdf"),
    );
}

fn open_outline_pdf(rt: &mut Runtime<Workspace>) {
    open_pdf_settled(
        rt,
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/outline.pdf"),
    );
}

fn open_pdf_for_keys(rt: &mut Runtime<Workspace>) {
    open_pdf_page(
        rt,
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/sample.pdf"),
    );
}

fn rect_of(rt: &Runtime<Workspace>, key: &str) -> [f32; 4] {
    rt.elements()
        .iter()
        .find(|element| element.key.as_deref() == Some(key))
        .map(|element| element.rect)
        .unwrap_or_else(|| panic!("{key} is drawn"))
}

fn page_rect(rt: &Runtime<Workspace>) -> [f32; 4] {
    rect_of(rt, "pdf-page-0")
}

fn click(rt: &mut Runtime<Workspace>, key: &str) {
    let rect = rect_of(rt, key);
    let (x, y) = (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
    rt.pointer_move(x, y);
    rt.button_down(x, y, PointerButton::Primary);
    rt.button_up(x, y, PointerButton::Primary);
    rt.frame(1.0 / 60.0);
}

fn press(rt: &mut Runtime<Workspace>, event: KeyEvent) {
    rt.key(event);
    rt.frame(1.0 / 60.0);
}

fn command(key: char) -> KeyEvent {
    KeyEvent {
        key: Key::Command(key),
        mods: Mods {
            cmd: true,
            ctrl: true,
            ..Mods::default()
        },
    }
}

/// Wait for the page to be drawn at least `width` points wide.
fn wait_for_page_width(rt: &mut Runtime<Workspace>, width: f32) -> f32 {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        rt.state.poll();
        rt.frame(1.0 / 60.0);
        let drawn = page_rect(rt)[2];
        if drawn >= width {
            return drawn;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    page_rect(rt)[2]
}

#[test]
fn dragging_on_the_page_selects_text() {
    let _pdf = pdf_guard();
    let mut rt = runtime();
    open_pdf(&mut rt);
    let rect = page_rect(&rt);

    // Across the first line of the fixture, in window points.
    let start = (rect[0] + 20.0, rect[1] + 40.0);
    let end = (rect[0] + 220.0, rect[1] + 55.0);
    rt.pointer_move(start.0, start.1);
    rt.button_down(start.0, start.1, PointerButton::Primary);
    rt.pointer_move(end.0, end.1);
    rt.button_up(end.0, end.1, PointerButton::Primary);
    rt.frame(1.0 / 60.0);

    match &rt.state.documents[rt.state.active] {
        Document::Pdf(pdf) => {
            assert!(pdf.selection.is_some(), "the drag selected something");
        }
        _ => panic!("the PDF is active"),
    }
}

fn pdf(rt: &Runtime<Workspace>) -> &lide_pdf::view::PdfView {
    match &rt.state.documents[rt.state.active] {
        Document::Pdf(pdf) => pdf,
        _ => panic!("the PDF is active"),
    }
}

#[test]
fn the_page_buttons_move_between_pages() {
    let _pdf = pdf_guard();
    let mut rt = runtime();
    open_outline_pdf(&mut rt);
    assert_eq!(pdf(&rt).page(), 0);
    assert_eq!(pdf(&rt).page_count(), 2);

    click(&mut rt, "pdf-next-page-0");
    assert_eq!(pdf(&rt).page(), 1, "the next-page button moved on");

    click(&mut rt, "pdf-prev-page-0");
    assert_eq!(pdf(&rt).page(), 0, "the previous-page button moved back");
}

#[test]
fn ctrl_page_keys_switch_pages() {
    let _pdf = pdf_guard();
    let mut rt = runtime();
    open_outline_pdf(&mut rt);

    press(
        &mut rt,
        KeyEvent {
            key: Key::PageDown,
            mods: Mods {
                cmd: true,
                ctrl: true,
                ..Mods::default()
            },
        },
    );
    assert_eq!(pdf(&rt).page(), 1, "Ctrl+PageDown goes to the next page");

    press(
        &mut rt,
        KeyEvent {
            key: Key::PageUp,
            mods: Mods {
                cmd: true,
                ctrl: true,
                ..Mods::default()
            },
        },
    );
    assert_eq!(pdf(&rt).page(), 0, "Ctrl+PageUp goes back");
}

#[test]
fn zoom_controls_change_the_scale_and_the_page_size() {
    let _pdf = pdf_guard();
    let mut rt = runtime();
    open_pdf(&mut rt);
    assert_eq!(pdf(&rt).scale(), 1.0, "a PDF opens at actual size");

    let before = page_rect(&rt)[2];
    click(&mut rt, "pdf-zoom-in-0");
    assert_eq!(pdf(&rt).scale(), 1.25);
    let after = wait_for_page_width(&mut rt, before * 1.2);
    assert!(
        after > before * 1.2,
        "zooming in draws the page larger: {before} -> {after}"
    );

    click(&mut rt, "pdf-zoom-reset-0");
    assert_eq!(pdf(&rt).scale(), 1.0);

    click(&mut rt, "pdf-zoom-out-0");
    assert_eq!(pdf(&rt).scale(), 0.8);
}

#[test]
fn double_clicking_a_pdf_page_asks_for_the_source() {
    let _pdf = pdf_guard();
    let mut rt = runtime();
    open_pdf(&mut rt);
    let rect = page_rect(&rt);
    let (x, y) = (rect[0] + 120.0, rect[1] + 60.0);
    rt.pointer_move(x, y);
    for _ in 0..2 {
        rt.button_down(x, y, PointerButton::Primary);
        rt.button_up(x, y, PointerButton::Primary);
    }
    rt.frame(1.0 / 60.0);
    rt.state.poll();

    assert!(!pdf(&rt).double_click, "the request was handled");
    assert!(
        pdf(&rt).pointer.is_some(),
        "the page records where the double click landed"
    );
    assert!(
        rt.state.notice.is_some(),
        "the inverse SyncTeX attempt reports its result"
    );
}

#[test]
fn the_wheel_turns_the_page_when_the_page_fits() {
    let _pdf = pdf_guard();
    let mut rt = runtime_sized((1200.0, 1200.0));
    open_outline_pdf(&mut rt);

    let rect = page_rect(&rt);
    let (x, y) = (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
    rt.pointer_move(x, y);
    rt.frame(0.0);

    // The page fits this window, so the wheel has nowhere to scroll and must
    // turn the page instead.
    rt.wheel_event(WheelEvent::new(0.0, -240.0).precise(true).at(x, y));
    rt.frame(1.0 / 60.0);
    assert_eq!(pdf(&rt).page(), 1, "wheel down turns to the next page");

    let rect = page_rect(&rt);
    rt.pointer_move(rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
    rt.frame(0.0);
    rt.wheel_event(WheelEvent::new(0.0, 240.0).precise(true).at(x, y));
    rt.frame(1.0 / 60.0);
    assert_eq!(pdf(&rt).page(), 0, "wheel up turns back");
}

#[test]
fn the_wheel_scrolls_a_tall_page() {
    let _pdf = pdf_guard();
    let mut rt = runtime();
    open_pdf(&mut rt);

    let before = page_rect(&rt);
    let (x, y) = (before[0] + before[2] / 2.0, before[1] + before[3] / 2.0);
    rt.pointer_move(x, y);
    rt.frame(0.0);

    rt.wheel_event(WheelEvent::new(0.0, -80.0).precise(true).at(x, y));
    for _ in 0..20 {
        rt.frame(1.0 / 60.0);
    }

    let after = page_rect(&rt);
    assert!(
        after[1] < before[1] - 1.0,
        "the wheel moved the page up: {} -> {}",
        before[1],
        after[1]
    );
    assert_eq!(pdf(&rt).page(), 0, "a tall page scrolls before it turns");
}

#[test]
fn zoom_shortcuts_change_the_scale() {
    let _pdf = pdf_guard();
    let mut rt = runtime();
    open_pdf_for_keys(&mut rt);

    press(&mut rt, command('='));
    assert_eq!(pdf(&rt).scale(), 1.25, "Ctrl+= zooms in");
    press(&mut rt, command('-'));
    assert_eq!(pdf(&rt).scale(), 1.0, "Ctrl+- zooms out");
    press(&mut rt, command('0'));
    assert_eq!(pdf(&rt).scale(), 1.0, "Ctrl+0 is actual size");
}
