//! Tab gestures, driven through the real view.

use std::path::{Path, PathBuf};

use lide_workspace::Workspace;
use rune::{Env, PointerButton, Runtime};

fn isolate_settings() {
    crate::isolate_settings();
}

fn scratch() -> PathBuf {
    let directory = std::env::temp_dir().join(format!("lide-tabs-{}", std::process::id()));
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

fn middle_click(rt: &mut Runtime<Workspace>, key: &str) {
    let rect = rect(rt, key);
    let (x, y) = (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
    rt.pointer_move(x, y);
    rt.button_down(x, y, PointerButton::Middle);
    rt.button_up(x, y, PointerButton::Middle);
    rt.frame(1.0 / 60.0);
}

#[test]
fn a_middle_click_closes_a_tab() {
    let directory = scratch();
    let a = directory.join("a.txt");
    let b = directory.join("b.txt");
    std::fs::write(&a, "a\n").unwrap();
    std::fs::write(&b, "b\n").unwrap();

    let mut rt = runtime();
    rt.state.open_path(&a).expect("the first file opens");
    rt.state.open_path(&b).expect("the second file opens");
    rt.invalidate();
    rt.frame(0.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
        let settled = rt
            .elements()
            .iter()
            .filter(|element| {
                element
                    .key
                    .as_deref()
                    .is_some_and(|key| key.starts_with("tab-"))
            })
            .all(|element| !element.moving);
        if settled {
            break;
        }
    }
    assert_eq!(rt.state.documents.len(), 2);

    middle_click(&mut rt, "tab-1");
    assert_eq!(rt.state.documents.len(), 1, "the wheel click closed the tab");
    assert_eq!(
        rt.state.active_document().and_then(|d| d.path()),
        Some(Path::new(&a)),
        "the other tab is left standing"
    );

    let _ = std::fs::remove_dir_all(&directory);
}
