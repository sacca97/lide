//! Resizing and collapsing the sidebar through real pointer/key input.

use lide_workspace::Workspace;
use rune::{Env, PointerButton, Runtime};

fn isolate_settings() {
    crate::isolate_settings();
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
fn dragging_the_edge_resizes_the_sidebar() {
    let mut rt = runtime();
    // The settings directory is shared between runs; start from a known width
    // so the clamp cannot hide a no-op drag.
    rt.state.settings.sidebar_width = 240.0;
    rt.state.settings.show_sidebar = true;
    rt.invalidate();
    rt.frame(0.0);
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
        let settled = rt
            .elements()
            .iter()
            .find(|element| element.key.as_deref() == Some("sidebar-resizer"))
            .is_some_and(|element| !element.moving);
        if settled {
            break;
        }
    }
    let before = rt.state.settings.sidebar_width;
    let rect = rect(&rt, "sidebar-resizer");

    let start = (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
    rt.pointer_move(start.0, start.1);
    rt.button_down(start.0, start.1, PointerButton::Primary);
    rt.frame(1.0 / 60.0);
    rt.pointer_move(start.0 + 80.0, start.1);
    rt.frame(1.0 / 60.0);
    rt.button_up(start.0 + 80.0, start.1, PointerButton::Primary);
    rt.frame(1.0 / 60.0);

    assert!(
        rt.state.settings.sidebar_width > before,
        "dragging right widened the sidebar: {} -> {}",
        before,
        rt.state.settings.sidebar_width
    );
}

#[test]
fn the_status_bar_shows_the_sidebar_and_the_rail_closes_it() {
    let mut rt = runtime();
    rt.state.settings.show_sidebar = false;
    rt.invalidate();
    rt.frame(0.0);
    assert!(!rt.state.settings.show_sidebar, "the sidebar starts hidden");
    assert!(
        !rt.elements()
            .iter()
            .any(|element| element.key.as_deref() == Some("activity-Files") && !element.exiting),
        "the rail is hidden with the panel"
    );

    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
        let settled = rt
            .elements()
            .iter()
            .find(|element| element.key.as_deref() == Some("icon-button-Show sidebar"))
            .is_some_and(|element| !element.moving && element.opacity > 0.99);
        if settled {
            break;
        }
    }
    click(&mut rt, "icon-button-Show sidebar");
    assert!(rt.state.settings.show_sidebar, "the status bar shows it");
    assert!(
        rt.elements()
            .iter()
            .any(|element| element.key.as_deref() == Some("activity-Files") && !element.exiting),
        "the activity rail is visible with the panel"
    );

    click(&mut rt, "sidebar-toggle");
    assert!(!rt.state.settings.show_sidebar, "the rail chevron hides it");
    assert!(
        !rt.elements()
            .iter()
            .any(|element| element.key.as_deref() == Some("activity-Files") && !element.exiting),
        "the rail goes with the panel"
    );
}
