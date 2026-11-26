use rune::{icon, row, text, El};
use rune_kit::syntax::lang_for_path;
use rune_kit::Kit;

use crate::view::icon_button;
use crate::{Document, Workspace};
use lide_editor::Editor;

pub(crate) fn status_bar(s: &Workspace, k: &Kit) -> El {
    let mut left: Vec<(String, bool)> = Vec::new();
    let mut right: Vec<(String, bool)> = Vec::new();

    if let Some(notice) = &s.notice {
        left.push((notice.clone(), true));
    }
    if let Some(location) = s.active_document().and_then(|document| document.location()) {
        let path = match location {
            lide_resource::Location::Local(path) => s
                .root
                .as_deref()
                .and_then(|root| path.strip_prefix(root).ok())
                .unwrap_or(path)
                .display()
                .to_string(),
            lide_resource::Location::Remote { .. } => location.to_string(),
        };
        left.push((short_path(&path), false));
    }
    let (errors, warnings) = s.servers.problem_count();
    if errors > 0 {
        left.push((format!("{errors} errors"), false));
    }
    if warnings > 0 {
        left.push((format!("{warnings} warnings"), false));
    }

    match s.active_document() {
        Some(Document::Text(editor)) => {
            right.push((language_name(editor), false));
            right.push((
                format!(
                    "Ln {}, Col {}",
                    editor.cursor_line(),
                    editor.cursor_column()
                ),
                true,
            ));
            right.push((format!("{} lines", editor.line_count()), false));
        }
        Some(Document::Markdown { editor, .. }) => {
            right.push(("Markdown".to_string(), false));
            right.push((
                format!(
                    "Ln {}, Col {}",
                    editor.cursor_line(),
                    editor.cursor_column()
                ),
                true,
            ));
        }
        Some(Document::Pdf(pdf)) => {
            right.push(("PDF".to_string(), false));
            right.push((
                format!("Page {} / {}", pdf.page() + 1, pdf.page_count()),
                true,
            ));
        }
        Some(Document::Notebook(notebook)) => {
            right.push(("Notebook".to_string(), false));
            right.push((format!("{} cells", notebook.notebook.cells.len()), true));
        }
        Some(Document::Csv(csv)) => {
            right.push(("Table".to_string(), false));
            let (rows, columns) = csv.size();
            right.push((format!("{rows} x {columns}"), true));
        }
        Some(Document::Diff(diff)) => {
            right.push(("Diff".to_string(), false));
            let (added, removed) = diff.counts();
            right.push((format!("+{added} -{removed}"), true));
        }
        Some(Document::Image(image)) => {
            right.push(("Image".to_string(), false));
            if let Some([width, height]) = image.size() {
                right.push((format!("{width} x {height}"), true));
            }
        }
        Some(Document::Terminal(terminal)) => {
            right.push(("Terminal".to_string(), false));
            right.push((terminal.title(), true));
            right.push((
                format!(
                    "{} x {}",
                    terminal.emulator.size().cols,
                    terminal.emulator.size().rows
                ),
                false,
            ));
        }
        None => {}
    }

    if s.find.open && !s.find.is_empty() {
        right.push((format!("{} matches", s.find.len()), false));
    }
    if let Some(status) = s.servers.status() {
        right.push((status, false));
    }

    let mut bar = row()
        .w_full()
        .h(24.0)
        .px(10.0)
        .gap(14.0)
        .center_y()
        .fill(k.c.bg_base);

    let item = |label: &String, strong: bool| {
        text(label.clone())
            .font(k.ty.small)
            .color(if strong { k.c.text_base } else { k.c.text_weak })
    };

    // The sidebar is hidden by default; this is the always-visible way back.
    let (sidebar_icon, sidebar_label) = if s.settings.show_sidebar {
        (&crate::icons::CHEVRON_LEFT, "Hide sidebar")
    } else {
        (&crate::icons::CHEVRON_RIGHT, "Show sidebar")
    };
    bar = bar.child(
        icon_button(sidebar_icon, sidebar_label, k)
            .on_press(|s: &mut Workspace| s.toggle_sidebar()),
    );

    // The branch comes first, as in every editor's status bar: it is what the
    // whole window is looking at, and it does not move when the file changes.
    if let Some(git) = &s.git {
        if !git.branch.is_empty() {
            bar = bar.child(
                row()
                    .gap(4.0)
                    .center_y()
                    .child(icon(&crate::icons::BRANCH, 12.0).color(k.c.text_weak))
                    .child(item(&git.branch, false)),
            );
        }
    }

    // A one-click way between the LaTeX source and its PDF, for people who
    // do not know the shortcut yet.
    let sync = match s.documents.get(s.focused_index()) {
        Some(Document::Pdf(_)) => Some("Source"),
        Some(document)
            if document
                .path()
                .and_then(|path| path.extension())
                .is_some_and(|extension| extension == "tex") =>
        {
            Some("PDF")
        }
        _ => None,
    };
    if let Some(label) = sync {
        bar = bar.child(
            text(label)
                .key("sync-pdf")
                .label("Jump between the LaTeX source and its PDF")
                .font(k.ty.small_medium)
                .color(k.c.text_accent)
                .px(6.0)
                .py(1.0)
                .radius(4.0)
                .hover(k.c.surface_base)
                .on_press(|s: &mut Workspace| s.sync_source_and_pdf()),
        );
    }

    let has_outline = matches!(
        s.documents.get(s.focused_index()),
        Some(Document::Markdown { .. } | Document::Pdf(_))
    );
    if has_outline {
        let outline = s.show_outline();
        bar = bar.child(
            text("Outline")
                .key("toggle-outline")
                .label(if outline {
                    "Hide outline"
                } else {
                    "Show outline"
                })
                .font(k.ty.small_medium)
                .color(if outline {
                    k.c.text_accent
                } else {
                    k.c.text_weak
                })
                .px(6.0)
                .py(1.0)
                .radius(4.0)
                .hover(k.c.surface_base)
                .on_press(|s: &mut Workspace| s.toggle_outline()),
        );
    }

    // The left side may be long (a notice, a path); it is the part that gives.
    bar = bar.child(
        row()
            .grow(1.0)
            .min_w(0.0)
            .gap(14.0)
            .center_y()
            .children(left.iter().map(|(label, strong)| item(label, *strong).truncate())),
    );
    for (label, strong) in &right {
        bar = bar.child(item(label, *strong));
    }

    bar
}


/// A path short enough for the status bar.
fn short_path(path: &str) -> String {
    const LIMIT: usize = 60;
    let count = path.chars().count();
    if count <= LIMIT {
        return path.to_string();
    }
    let start = path
        .char_indices()
        .nth(count - LIMIT + 1)
        .map(|(index, _)| index)
        .unwrap_or(0);
    format!("…{}", &path[start..])
}

/// The language's name for the status bar, from the file's extension.
fn language_name(editor: &Editor) -> String {
    let Some(location) = &editor.location else {
        return "Plain Text".to_string();
    };
    let Some(language) = lang_for_path(&location.path_for_kind().to_string_lossy()) else {
        return "Plain Text".to_string();
    };
    match language {
        "cpp" => "C++".to_string(),
        "csharp" => "C#".to_string(),
        "javascript" => "JavaScript".to_string(),
        "typescript" => "TypeScript".to_string(),
        "python" => "Python".to_string(),
        "rust" => "Rust".to_string(),
        "toml" => "TOML".to_string(),
        "yaml" | "yml" => "YAML".to_string(),
        "json" => "JSON".to_string(),
        "html" => "HTML".to_string(),
        "css" => "CSS".to_string(),
        "markdown" => "Markdown".to_string(),
        "bash" => "Shell Script".to_string(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => "Plain Text".to_string(),
            }
        }
    }
}

