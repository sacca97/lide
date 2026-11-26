//! The editor's icons.
//!
//! Every icon is drawn on the same 16-point grid with the same 1.25 stroke, so
//! a row of them reads as one set. The kit's own glyphs (search, close, the
//! file and folder shapes) already use that grid; the ones it lacks are here.

use rune::Icon;

pub use rune_kit::glyphs::{
    CHEVRON_DOWN, CLOSE, FILE, FILE_CODE, FILE_IMAGE, FILE_TEXT, FOLDER, FOLDER_OPEN, SEARCH,
};

/// Two stacked sheets: the explorer.
pub static FILES: Icon = Icon {
    name: "lide-files",
    view: 16.0,
    body: r#"<path d="M5.5 2.5H10.5L13 5V11.5H5.5V2.5Z" stroke="currentColor" stroke-width="1.25" stroke-linejoin="round"/><path d="M3 5V13.5H10" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

/// A triangle with a bang: problems.
pub static PROBLEMS: Icon = Icon {
    name: "lide-problems",
    view: 16.0,
    body: r#"<path d="M8 2.5L14 13H2L8 2.5Z" stroke="currentColor" stroke-width="1.25" stroke-linejoin="round"/><path d="M8 6.5V9.5" stroke="currentColor" stroke-width="1.25" stroke-linecap="round"/><circle cx="8" cy="11.25" r="0.75" fill="currentColor"/>"#,
};

/// A target: where a symbol is used.
pub static PLACES: Icon = Icon {
    name: "lide-places",
    view: 16.0,
    body: r#"<circle cx="8" cy="8" r="3.5" stroke="currentColor" stroke-width="1.25"/><path d="M8 1.5V4M8 12V14.5M1.5 8H4M12 8H14.5" stroke="currentColor" stroke-width="1.25" stroke-linecap="round"/>"#,
};

/// A branch: source control.
pub static BRANCH: Icon = Icon {
    name: "lide-branch",
    view: 16.0,
    body: r#"<circle cx="5" cy="3.75" r="1.5" stroke="currentColor" stroke-width="1.25"/><circle cx="5" cy="12.25" r="1.5" stroke="currentColor" stroke-width="1.25"/><circle cx="11.5" cy="5.75" r="1.5" stroke="currentColor" stroke-width="1.25"/><path d="M5 5.25V10.75M11.5 7.25C11.5 9.5 5 8.5 5 10.75" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

pub static CHEVRON_LEFT: Icon = Icon {
    name: "lide-chevron-left",
    view: 16.0,
    body: r#"<path d="M10 3.5L5.5 8L10 12.5" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

pub static CHEVRON_RIGHT: Icon = Icon {
    name: "lide-chevron-right",
    view: 16.0,
    body: r#"<path d="M6 3.5L10.5 8L6 12.5" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

pub static ARROW_DOWN: Icon = Icon {
    name: "lide-arrow-down",
    view: 16.0,
    body: r#"<path d="M8 3V12.5M4 8.75L8 12.75L12 8.75" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

pub static ARROW_UP: Icon = Icon {
    name: "lide-arrow-up",
    view: 16.0,
    body: r#"<path d="M8 13V3.5M4 7.25L8 3.25L12 7.25" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

/// A thin cross, for the tab close button.
pub static X: Icon = Icon {
    name: "lide-x",
    view: 16.0,
    body: r#"<path d="M4.5 4.5L11.5 11.5M11.5 4.5L4.5 11.5" stroke="currentColor" stroke-width="1.25" stroke-linecap="round"/>"#,
};

/// The icon a file in the tree gets, from its name.
pub fn for_file(name: &str) -> &'static Icon {
    let extension = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match extension.as_deref() {
        Some(
            "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "c" | "h" | "cpp" | "hpp" | "go" | "java"
            | "rb" | "sh" | "lua" | "css" | "html" | "tex" | "sty" | "cls",
        ) => &FILE_CODE,
        Some("png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" | "ico") => &FILE_IMAGE,
        Some("md" | "txt" | "toml" | "yaml" | "yml" | "json" | "csv" | "lock" | "pdf" | "ipynb") => {
            &FILE_TEXT
        }
        _ => &FILE,
    }
}
