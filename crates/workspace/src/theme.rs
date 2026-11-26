//! The editor's kit and the monospace style.
//!
//! Rune's neutral kit is close to what an editor wants, but not flat enough;
//! this overrides its surfaces with VS Code's. A kit is built once per
//! appearance and shared, because building it per frame would allocate for no
//! reason.

use std::sync::OnceLock;

use rune::TextStyle;
use rune_kit::Kit;

/// The theme tokens for one appearance.
///
/// The kit is built once per appearance and then shared: building it on
/// every frame would allocate for no reason.
pub fn kit(dark: bool) -> &'static Kit {
    static DARK: OnceLock<Kit> = OnceLock::new();
    static LIGHT: OnceLock<Kit> = OnceLock::new();
    if dark {
        DARK.get_or_init(|| vscode(true))
    } else {
        LIGHT.get_or_init(|| vscode(false))
    }
}

/// A neutral kit with VS Code's flat, low-contrast chrome.
///
/// The kit's own palette is a soft, layered one; the editor wants the flatter
/// surfaces and tighter borders an editor's window uses, with the editor page
/// as the darkest surface and the chrome one step lighter.
fn vscode(dark: bool) -> Kit {
    let hex = |value: &str| rune::color::hex(value).expect("a literal colour");
    let mut kit = Kit::neutral(dark);

    if dark {
        kit.c.page = hex("#1e1e1e");
        kit.c.bg_base = hex("#252526");
        kit.c.bg_deep = hex("#181818");
        kit.c.layer_01 = hex("#2d2d30");
        kit.c.layer_02 = hex("#252526");
        kit.c.layer_03 = hex("#2d2d30");
        kit.c.layer_04 = hex("#333333");
        kit.c.surface_base = hex("#2a2d2e");
        kit.c.surface_weak = hex("#252526");
        kit.c.surface_hover = hex("#2a2d2e");
        kit.c.surface_raised = hex("#2d2d30");
        kit.c.surface_raised_hover = hex("#37373d");
        kit.c.text_base = hex("#cccccc");
        kit.c.text_muted = hex("#9d9d9d");
        kit.c.text_weak = hex("#8a8a8a");
        kit.c.text_faint = hex("#6e7681");
        kit.c.text_accent = hex("#4daafc");
        kit.c.border_base = hex("#3c3c3c");
        kit.c.border_muted = hex("#2b2b2b");
        kit.c.border_weak = hex("#2b2b2b");
        kit.c.border_strong = hex("#4c4c4c");
        kit.c.hover = hex("#2a2d2e");
        kit.c.pressed = hex("#37373d");
        kit.c.success.fg = hex("#89d185");
        kit.c.warning.fg = hex("#cca700");
        kit.c.danger.fg = hex("#f14c4c");
        kit.c.info.fg = hex("#3794ff");
    } else {
        kit.c.page = hex("#ffffff");
        kit.c.bg_base = hex("#f3f3f3");
        kit.c.bg_deep = hex("#ececec");
        kit.c.layer_01 = hex("#ffffff");
        kit.c.layer_02 = hex("#f3f3f3");
        kit.c.layer_03 = hex("#ececec");
        kit.c.layer_04 = hex("#e8e8e8");
        kit.c.surface_base = hex("#e8e8e8");
        kit.c.surface_weak = hex("#f3f3f3");
        kit.c.surface_hover = hex("#e8e8e8");
        kit.c.surface_raised = hex("#ffffff");
        kit.c.surface_raised_hover = hex("#e8e8e8");
        kit.c.text_base = hex("#333333");
        kit.c.text_muted = hex("#616161");
        kit.c.text_weak = hex("#717171");
        kit.c.text_faint = hex("#8c8c8c");
        kit.c.text_accent = hex("#005fb8");
        kit.c.border_base = hex("#d4d4d4");
        kit.c.border_muted = hex("#e0e0e0");
        kit.c.border_weak = hex("#e0e0e0");
        kit.c.border_strong = hex("#c4c4c4");
        kit.c.hover = hex("#e8e8e8");
        kit.c.pressed = hex("#dcdcdc");
        kit.c.success.fg = hex("#388a34");
        kit.c.warning.fg = hex("#bf8803");
        kit.c.danger.fg = hex("#e51400");
        kit.c.info.fg = hex("#0066bf");
    }

    kit
}

/// The monospace style the editor draws its text with.
pub fn mono_style(font_size: f32) -> TextStyle {
    TextStyle::new(font_size, 400.0).mono().lh(font_size * 1.55)
}

