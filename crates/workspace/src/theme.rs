//! The editor's kit and the monospace style.
//!
//! Rune's neutral kit is close to what an editor wants, but not flat enough;
//! this overrides its surfaces with the editor's own ink and paper ramps. A kit is built once per
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

/// The editor's kit: a writing desk at night, or paper by day.
///
/// The surfaces are near greys, leaning a touch toward blue-black ink in the
/// dark theme and toward off-white paper in the light one, so the documents
/// carry the colour and the chrome does not. The editor page is the calmest
/// surface; the window behind the cards is one step deeper and the chrome one
/// step raised. There is one accent, an ink blue, kept for "you are here":
/// the active tab, the focused pane, the activity bar and a SyncTeX jump.
fn vscode(dark: bool) -> Kit {
    let hex = |value: &str| rune::color::hex(value).expect("a literal colour");
    let mut kit = Kit::neutral(dark);

    if dark {
        kit.c.bg_deep = hex("#0f1014");
        kit.c.page = hex("#15161b");
        kit.c.bg_base = hex("#191a20");
        kit.c.layer_01 = hex("#1d1f26");
        kit.c.layer_02 = hex("#191a20");
        kit.c.layer_03 = hex("#1d1f26");
        kit.c.layer_04 = hex("#24262e");
        kit.c.surface_base = hex("#22242b");
        kit.c.surface_weak = hex("#191a20");
        kit.c.surface_hover = hex("#22242b");
        kit.c.surface_raised = hex("#1d1f26");
        kit.c.surface_raised_hover = hex("#272a33");
        kit.c.text_strong = hex("#eef0f5");
        kit.c.text_base = hex("#dfe2ea");
        kit.c.text_muted = hex("#a3a9b8");
        kit.c.text_weak = hex("#8b91a1");
        kit.c.text_faint = hex("#6c7284");
        kit.c.text_accent = hex("#8aa9f2");
        kit.c.border_base = hex("#2a2c34");
        kit.c.border_muted = hex("#22242b");
        kit.c.border_weak = hex("#22242b");
        kit.c.border_strong = hex("#383b45");
        kit.c.hover = hex("#1f2128");
        kit.c.pressed = hex("#272a33");
        kit.c.success.fg = hex("#4cc38a");
        kit.c.warning.fg = hex("#d9a54a");
        kit.c.danger.fg = hex("#e5484d");
        kit.c.info.fg = hex("#8aa9f2");
    } else {
        kit.c.bg_deep = hex("#ebe8e1");
        kit.c.page = hex("#fdfcf9");
        kit.c.bg_base = hex("#f5f3ee");
        kit.c.layer_01 = hex("#fdfcf9");
        kit.c.layer_02 = hex("#f5f3ee");
        kit.c.layer_03 = hex("#efece6");
        kit.c.layer_04 = hex("#e8e5de");
        kit.c.surface_base = hex("#ebe8e1");
        kit.c.surface_weak = hex("#f5f3ee");
        kit.c.surface_hover = hex("#ebe8e1");
        kit.c.surface_raised = hex("#fdfcf9");
        kit.c.surface_raised_hover = hex("#ebe8e1");
        kit.c.text_strong = hex("#14130f");
        kit.c.text_base = hex("#24221d");
        kit.c.text_muted = hex("#5c5850");
        kit.c.text_weak = hex("#6f6a61");
        kit.c.text_faint = hex("#8d887e");
        kit.c.text_accent = hex("#2c4ea8");
        kit.c.border_base = hex("#dedad1");
        kit.c.border_muted = hex("#e7e4dc");
        kit.c.border_weak = hex("#e7e4dc");
        kit.c.border_strong = hex("#cdc8bd");
        kit.c.hover = hex("#efece6");
        kit.c.pressed = hex("#e6e2da");
        kit.c.success.fg = hex("#16803c");
        kit.c.warning.fg = hex("#a86a12");
        kit.c.danger.fg = hex("#c8262b");
        kit.c.info.fg = hex("#2c4ea8");
    }

    kit
}

/// The monospace style the editor draws its text with.
pub fn mono_style(font_size: f32) -> TextStyle {
    TextStyle::new(font_size, 400.0).mono().lh(font_size * 1.55)
}

