#[path = "suite/external_changes.rs"]
mod external_changes;
#[path = "suite/git_diff.rs"]
mod git_diff;
#[path = "suite/images.rs"]
mod images;
#[path = "suite/keys.rs"]
mod keys;
#[path = "suite/latex_preview.rs"]
mod latex_preview;
#[path = "suite/lsp.rs"]
mod lsp;
#[path = "suite/pdf_interaction.rs"]
mod pdf_interaction;
#[path = "suite/replace.rs"]
mod replace;
#[path = "suite/sidebar.rs"]
mod sidebar;
#[path = "suite/split.rs"]
mod split;
#[path = "suite/structured.rs"]
mod structured;
#[path = "suite/sync_text.rs"]
mod sync_text;
#[path = "suite/tabs.rs"]
mod tabs;
#[path = "suite/terminal_drag.rs"]
mod terminal_drag;
#[path = "suite/terminal.rs"]
mod terminal;
#[path = "suite/tree_state.rs"]
mod tree_state;

use std::sync::OnceLock;

/// Every test shares one settings directory, set once before any of them
/// reads it: the environment is process-wide.
pub fn isolate_settings() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let directory = std::env::temp_dir().join("lide-test-config-suite");
        let _ = std::fs::create_dir_all(&directory);
        // SAFETY: this runs once, before any test touches the settings.
        unsafe { std::env::set_var("LIDE_CONFIG_DIR", &directory) };
    });
}
