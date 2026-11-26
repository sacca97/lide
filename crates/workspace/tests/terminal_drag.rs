//! Pointer selection in the terminal, through Rune's real event routing.

use std::time::{Duration, Instant};

use lide_workspace::{Document, Workspace};
use rune::{Env, PointerButton, Runtime};

fn isolate_settings() {
    use std::sync::OnceLock;
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let directory = std::env::temp_dir().join("lide-test-config-terminal-drag");
        let _ = std::fs::create_dir_all(&directory);
        // SAFETY: this runs once, before any test touches the settings.
        unsafe { std::env::set_var("LIDE_CONFIG_DIR", &directory) };
    });
}

fn screen(workspace: &Workspace) -> String {
    workspace
        .documents
        .iter()
        .find_map(|document| match document {
            Document::Terminal(terminal) => Some(terminal.emulator.snapshot().rows.join("\n")),
            _ => None,
        })
        .unwrap_or_default()
}

#[test]
fn dragging_over_the_terminal_selects_cells() {
    isolate_settings();
    let mut rt = Runtime::new(
        Workspace::new(),
        lide_workspace::view,
        (1200.0, 800.0),
        Env::default(),
    );
    rt.state.open_terminal().expect("a terminal starts");
    rt.invalidate();

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && screen(&rt.state).is_empty() {
        rt.state.poll();
        rt.frame(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    if let Some(terminal) = rt.state.active_terminal_mut() {
        terminal.write("printf 'alpha\\nbeta\\ngamma\\n'\r");
    }
    while Instant::now() < deadline && !screen(&rt.state).contains("gamma") {
        rt.state.poll();
        rt.frame(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(screen(&rt.state).contains("gamma"), "the output arrives");

    let rect = rt
        .elements()
        .iter()
        .find(|element| element.key.as_deref() == Some("terminal-area"))
        .map(|element| element.rect)
        .expect("the terminal area is laid out");

    let line = rt.state.settings.line_height();
    let advance = rt.state.settings.font_size * 0.62;
    let start = (rect[0] + 8.0, rect[1] + 8.0);
    let end = (rect[0] + 8.0 + advance * 5.0, rect[1] + 8.0 + line * 3.0);
    rt.pointer_move(start.0, start.1);
    rt.button_down(start.0, start.1, PointerButton::Primary);
    rt.pointer_move(end.0, end.1);
    rt.button_up(end.0, end.1, PointerButton::Primary);
    rt.frame(1.0 / 60.0);

    match &rt.state.documents[rt.state.active] {
        Document::Terminal(terminal) => {
            assert!(terminal.selection.is_some(), "the drag made a selection");
            let text = terminal.selected_text();
            assert!(!text.trim().is_empty(), "the selection has text: {text:?}");
        }
        _ => panic!("the terminal is active"),
    }
}
