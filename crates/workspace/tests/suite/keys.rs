//! The keyboard, driven through Rune's headless runtime.
//!
//! These tests build the real view at the real size, send real key events and
//! look at the state afterwards. No window, no display, no user.

use lide_workspace::Workspace;
use rune::edit::{Key, KeyEvent, Modifier, Mods};
use rune::{Env, Runtime};

/// Keep the tests from reading or writing the real settings file.
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
    rt.on_key = Some(lide_workspace::keys::handle);
    rt.frame(0.0);
    rt
}

fn press(rt: &mut Runtime<Workspace>, event: KeyEvent) {
    rt.key(event);
    rt.frame(0.0);
}

fn key(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        mods: Mods::default(),
    }
}

fn chord(key: char, shift: bool) -> KeyEvent {
    KeyEvent {
        key: Key::Command(key),
        mods: Mods {
            cmd: true,
            ctrl: true,
            shift,
            ..Mods::default()
        },
    }
}

#[test]
fn ctrl_shift_p_opens_the_palette() {
    let mut rt = runtime();
    assert!(!rt.state.palette.open);
    press(&mut rt, chord('p', true));
    assert!(rt.state.palette.open, "Ctrl+Shift+P should open the palette");
}

#[test]
fn escape_closes_the_palette() {
    let mut rt = runtime();
    rt.state.palette.open(&[], &lide_workspace::command::Recents::default());
    press(&mut rt, key(Key::Escape));
    assert!(!rt.state.palette.open);
}

#[test]
fn the_arrow_keys_move_the_highlight() {
    let mut rt = runtime();
    rt.state.palette.open(&[], &lide_workspace::command::Recents::default());
    rt.frame(0.0);

    let first = rt.state.palette.active;
    press(&mut rt, key(Key::Down));
    assert_eq!(
        rt.state.palette.active,
        first + 1,
        "Down moves the highlight, not the caret"
    );

    press(&mut rt, key(Key::Up));
    assert_eq!(rt.state.palette.active, first, "Up moves it back");
}

#[test]
fn down_wraps_from_the_last_command_to_the_first() {
    let mut rt = runtime();
    rt.state.palette.open(&[], &lide_workspace::command::Recents::default());
    let count = rt.state.palette.matches.len();
    rt.state.palette.active = count - 1;
    rt.frame(0.0);

    press(&mut rt, key(Key::Down));
    assert_eq!(rt.state.palette.active, 0);
}

#[test]
fn enter_runs_the_highlighted_command() {
    let mut rt = runtime();
    rt.state.palette.open(&[], &lide_workspace::command::Recents::default());
    // Narrow the list to one command so the highlight is unambiguous.
    rt.state.palette.query = rune_edit::TextEdit::new("toggle light");
    rt.state.palette.refresh();
    rt.frame(0.0);
    assert_eq!(
        rt.state.palette.selected(),
        Some(lide_workspace::command::Command::ToggleTheme)
    );

    let before = rt.state.settings.dark;
    press(&mut rt, key(Key::Enter));

    assert!(!rt.state.palette.open, "running a command closes the palette");
    assert_eq!(rt.state.settings.dark, !before, "the theme toggled");
}

#[test]
fn typing_filters_the_palette() {
    let mut rt = runtime();
    rt.state.palette.open(&[], &lide_workspace::command::Recents::default());
    rt.state.palette.query = rune_edit::TextEdit::new("save");
    rt.state.palette.refresh();
    rt.frame(0.0);

    let labels: Vec<String> = rt
        .state
        .palette
        .visible()
        .into_iter()
        .map(|entry| entry.label())
        .collect();
    assert!(labels.iter().any(|label| label == "Save file"));
    assert!(!labels
        .iter()
        .any(|label| label == "Toggle light / dark theme"));
}

#[test]
fn modifier_keys_track_the_primary_click_state() {
    let mut rt = runtime();
    rt.set_modifier_keys(true);
    press(
        &mut rt,
        KeyEvent {
            key: Key::Modifier {
                modifier: Modifier::ControlLeft,
                pressed: true,
            },
            mods: Mods {
                cmd: true,
                ctrl: true,
                ..Mods::default()
            },
        },
    );
    assert!(rt.state.primary_modifier, "Ctrl is held");

    press(
        &mut rt,
        KeyEvent {
            key: Key::Modifier {
                modifier: Modifier::ControlLeft,
                pressed: false,
            },
            mods: Mods::default(),
        },
    );
    assert!(!rt.state.primary_modifier, "Ctrl was released");
}

#[test]
fn ctrl_shift_o_toggles_the_outline() {
    let mut rt = runtime();
    rt.state.settings.show_outline = false;
    press(&mut rt, chord('o', true));
    assert!(rt.state.settings.show_outline, "the outline opens");
    press(&mut rt, chord('o', true));
    assert!(!rt.state.settings.show_outline, "the outline closes again");
}

#[test]
fn ctrl_f_opens_find() {
    let mut rt = runtime();
    press(&mut rt, chord('f', false));
    assert!(rt.state.find.open);
}

#[test]
fn ctrl_b_toggles_the_sidebar() {
    let mut rt = runtime();
    let before = rt.state.settings.show_sidebar;
    press(&mut rt, chord('b', false));
    assert_ne!(rt.state.settings.show_sidebar, before);
    // Put it back so the test does not write a surprising settings file.
    rt.state.settings.show_sidebar = before;
}

#[test]
fn an_unknown_chord_is_not_consumed() {
    let mut rt = runtime();
    let before = rt.state.settings.dark;
    press(&mut rt, chord('z', false));
    assert_eq!(rt.state.settings.dark, before);
    assert!(!rt.state.palette.open);
}

#[test]
fn the_latex_engine_can_be_cycled() {
    let mut rt = runtime();
    let before = rt.state.latex_engine();
    let next = before.next();

    lide_workspace::command::run(
        lide_workspace::command::Command::CycleLatexEngine,
        &mut rt.state,
    );
    rt.frame(0.0);

    assert_eq!(rt.state.latex_engine(), next);
    assert_eq!(rt.state.settings.latex_engine, next.name());
    assert!(
        rt.state
            .notice
            .as_deref()
            .unwrap_or_default()
            .contains(next.name()),
        "the notice names the engine: {:?}",
        rt.state.notice
    );
}

#[test]
fn scrolling_the_markdown_source_scrolls_the_preview() {
    let mut rt = runtime();
    let directory = std::env::temp_dir().join("lide-markdown-sync");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("long.md");
    let mut text = String::from("# Long\n\n");
    for index in 0..300 {
        text.push_str(&format!(
            "## Heading {index}\n\nParagraph {index} with enough words to wrap.\n\n"
        ));
    }
    std::fs::write(&path, text).unwrap();

    rt.state.open_path(&path).expect("the file opens");
    rt.invalidate();
    rt.frame(0.0);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);

    // The window is 1200x800: a 240pt sidebar, a 190pt outline, then the
    // source and preview panes. (620, 400) is inside the source pane.
    let (x, y) = (620.0, 400.0);

    let before_source = rt.state.markdown_scrolls[0].get();
    let before_preview = rt.state.markdown_scrolls[1].get();
    assert!(
        before_source[1] > 0.0,
        "the source is taller than its pane: {before_source:?}"
    );
    assert!(
        before_preview[1] > 0.0,
        "the preview is taller than its pane: {before_preview:?}"
    );

    // `Runtime::wheel` scrolls the viewport directly; a real wheel arrives as
    // an event, and events are what the pane's handler sees.
    rt.wheel_event(rune::WheelEvent::new(0.0, -120.0).at(x, y));
    for _ in 0..20 {
        rt.frame(1.0 / 60.0);
    }

    let source = rt.state.markdown_scrolls[0].get();
    let preview = rt.state.markdown_scrolls[1].get();
    assert!(
        source[0] > before_source[0],
        "the source scrolled down: {source:?} was {before_source:?}"
    );
    assert!(
        preview[0] > before_preview[0],
        "the preview followed: {preview:?} was {before_preview:?}"
    );

    // Both are the same fraction of the way through their content, which is
    // what "the same place" means when the two hold different amounts of text.
    let source_fraction = source[0] / source[1];
    let preview_fraction = preview[0] / preview[1];
    assert!(
        (source_fraction - preview_fraction).abs() < 0.05,
        "source at {source_fraction:.3}, preview at {preview_fraction:.3}"
    );

    // And the other way: a wheel over the preview moves the source.
    let source_before = source[0];
    let preview_before = preview[0];
    rt.wheel_event(rune::WheelEvent::new(0.0, -120.0).at(1000.0, 400.0));
    for _ in 0..20 {
        rt.frame(1.0 / 60.0);
    }
    let source = rt.state.markdown_scrolls[0].get();
    let preview = rt.state.markdown_scrolls[1].get();
    assert!(
        preview[0] > preview_before,
        "the preview scrolled down: {preview:?} was {preview_before:?}"
    );
    assert!(
        source[0] > source_before,
        "the source followed: {source:?} was {source_before:?}"
    );
    assert!(
        (source[0] / source[1] - preview[0] / preview[1]).abs() < 0.05,
        "the panes stay together in both directions"
    );

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn the_palette_offers_recent_files_and_runs_them() {
    let mut rt = runtime();
    let directory = std::env::temp_dir().join(format!("lide-recents-file-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("notes.txt");
    std::fs::write(&path, "hello\n").unwrap();

    rt.state.settings.recent_files = vec![path.clone()];
    press(&mut rt, chord('p', true));

    let labels: Vec<String> = rt
        .state
        .palette
        .visible()
        .into_iter()
        .map(|entry| entry.label())
        .collect();
    let wanted = format!("Recent file: {}", path.display());
    let index = labels.iter().position(|label| label == &wanted).unwrap_or_else(|| {
        panic!("the recent file is listed: {labels:?}");
    });

    rt.state.palette.active = index;
    press(&mut rt, key(Key::Enter));

    assert!(!rt.state.palette.open, "running an entry closes the palette");
    assert_eq!(
        rt.state.active_document().and_then(|document| document.path()),
        Some(path.as_path())
    );

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_recent_project_opens_its_folder() {
    let mut rt = runtime();
    let directory = std::env::temp_dir().join(format!("lide-recents-folder-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();

    rt.state.settings.recent_projects = vec![directory.clone()];
    press(&mut rt, chord('p', true));

    let labels: Vec<String> = rt
        .state
        .palette
        .visible()
        .into_iter()
        .map(|entry| entry.label())
        .collect();
    let wanted = format!("Recent project: {}", directory.display());
    let index = labels.iter().position(|label| label == &wanted).unwrap_or_else(|| {
        panic!("the recent project is listed: {labels:?}");
    });

    rt.state.palette.active = index;
    press(&mut rt, key(Key::Enter));

    assert_eq!(rt.state.root.as_deref(), Some(directory.as_path()));
    assert!(
        rt.state.tasks.iter().any(|task| task.label == "cargo test"),
        "opening the folder rediscovers its tasks"
    );

    let _ = std::fs::remove_dir_all(&directory);
}
