use std::path::Path;

use rune::{col, el, icon, row, text, text_input, Cx, El, Live, ScrollPos, ScrollSpec, Tx, WheelEvent};
use rune_kit::markdown::{Markdown, MarkdownStyle};
use rune_kit::syntax::{highlight, lang_for_path, SyntaxTheme};
use rune_kit::{Button, ControlSize, Kit, TextInput};

use crate::view::terminal::terminal_view;
use crate::view::{empty_note, icon_button};
use crate::text_edits::{line_starts, offset_at_line};
use crate::{clamp_split_ratio, Document, Workspace};
use crate::editor::Editor;
use lide_pdf::view::PdfView;

pub(crate) fn tab_strip(s: &Workspace, k: &Kit) -> El {
    let mut strip = row()
        .w_full()
        .h(32.0)
        .px(4.0)
        .pt(4.0)
        .gap(4.0)
        .items_end()
        .fill(k.c.bg_base);

    if s.documents.is_empty() {
        return col()
            .child(strip)
            .child(el().w_full().h(1.0).fill(k.c.border_base));
    }

    for (index, document) in s.documents.iter().enumerate() {
        let in_primary = index == s.active;
        let in_secondary = s.split_document == Some(index);
        let active = if s.split_document.is_some() && s.focused_pane == 1 {
            in_secondary
        } else {
            in_primary
        };
        let dirty = document.is_dirty();
        let title = document.title();

        let mut content = row().grow(1.0).pl(10.0).pr(4.0).gap(6.0).center_y().child(
            text(title.clone())
                .font(k.ty.text_xsmall)
                .truncate()
                .min_w(0.0)
                .color(if active {
                    k.c.text_base
                } else {
                    k.c.text_muted
                }),
        );

        if dirty {
            content = content.child(el().size(6.0, 6.0).radius(3.0).fill(k.c.text_muted));
        }

        // The active tab carries the editor's one mark of "you are here": a
        // short ink rule along its top edge, like a proof mark in the margin.
        let mark = el()
            .h(2.0)
            .mx(10.0)
            .radius(1.0)
            .fill(if active { k.c.text_accent } else { k.c.text_accent.alpha(0.0) });
        let tab = col()
            .key(format!("tab-{index}"))
            .h(28.0)
            .max_w(200.0)
            .shrink()
            .radius(6.0)
            .border(1.0, if active { k.c.border_base } else { k.c.border_base.alpha(0.0) })
            .fill(if active { k.c.page } else { k.c.bg_base })
            .child(mark)
            .child(
                content.child(
                    col()
                        .key(format!("tab-close-{index}"))
                        .label(format!("Close {title}"))
                        .size(18.0, 18.0)
                        .radius(4.0)
                        .center()
                        .hover(k.c.surface_base)
                        .child(icon(&crate::icons::X, 12.0).color(if active {
                            k.c.text_muted
                        } else {
                            k.c.text_faint
                        }))
                        .on_press(move |s: &mut Workspace| s.request_close(index)),
                ),
            )
            .on_press(move |s: &mut Workspace| s.select_tab(index))
            .on_middle_press(move |s: &mut Workspace, _tx: &mut Tx| s.request_close(index))
            .on_drag(move |s: &mut Workspace, drag: &mut rune::Drag, _tx: &mut Tx| {
                if drag.phase != rune::Phase::Up || drag.cancelled() {
                    return;
                }
                let [x, y] = drag.window_point();
                let left = s.split_left_rect.get();
                let right = s.split_right_rect.get();
                if s.split_document.is_none() {
                    if inside(left, x, y) {
                        let pane = if x > left[0] + left[2] / 2.0 { 1 } else { 0 };
                        s.drop_tab_to_pane(index, pane);
                    }
                } else if inside(left, x, y) {
                    s.drop_tab_to_pane(index, 0);
                } else if inside(right, x, y) {
                    s.drop_tab_to_pane(index, 1);
                }
            })
            .hover(k.c.hover);

        strip = strip.child(tab);
    }

    let preview = s
        .documents
        .get(s.focused_index())
        .and_then(|document| document.path())
        .is_some_and(|path| path.extension().is_some_and(|extension| extension == "tex"));
    if preview || s.split_document.is_some() {
        strip = strip.child(el().grow(1.0));
    }
    if preview {
        // An eye, sized and shaped like the other icon buttons in the strip:
        // the built PDF shown beside the source, without building.
        strip = strip.child(
            icon_button(&crate::icons::EYE, "Preview PDF", k)
                .on_press(|s: &mut Workspace| s.open_pdf_beside_source())
                .key("latex-preview"),
        );
    }
    if s.split_document.is_some() {
        strip = strip.child(
            icon_button(&crate::icons::X, "Close split", k)
                .on_press(|s: &mut Workspace| s.toggle_split()),
        );
    }

    col()
        .child(strip)
        .child(el().w_full().h(1.0).fill(k.c.border_base))
}

/// A banner for the active document when it changed on disk.
pub(crate) fn external_change_bar(s: &Workspace, k: &Kit) -> El {
    let Some(active) = s.active_document() else {
        return el().h(0.0);
    };
    let Some(path) = active.path() else {
        return el().h(0.0);
    };
    if !s.changed_on_disk.iter().any(|changed| changed == path) {
        return el().h(0.0);
    }

    row()
        .w_full()
        .h(32.0)
        .px(10.0)
        .gap(10.0)
        .center_y()
        .fill(k.c.warning.bg)
        .child(
            text("This file changed on disk.")
                .font(k.ty.small)
                .color(k.c.warning.fg),
        )
        .child(
            Button::new("Reload")
                .on_press(|s: &mut Workspace| s.accept_external_change())
                .build(k),
        )
        .child(
            Button::new("Keep mine")
                .on_press(|s: &mut Workspace| s.keep_local_change())
                .build(k),
        )
}

/// What the language server says is under the cursor.
pub(crate) fn hover_bar(s: &Workspace, k: &Kit) -> El {
    let Some(hover) = &s.hover else {
        return el().h(0.0);
    };
    row()
        .w_full()
        .px(10.0)
        .py(6.0)
        .gap(8.0)
        .fill(k.c.layer_01)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(text(hover.clone()).font(k.ty.small).color(k.c.text_base)),
        )
        .child(
            text("Esc")
                .font(k.ty.small)
                .color(k.c.text_weak)
                .on_press(|s: &mut Workspace| s.hover = None),
        )
}

pub(crate) fn find_bar(s: &Workspace, k: &Kit) -> El {
    if !s.find.open {
        return el().h(0.0);
    }

    let count = if s.find.query_text().is_empty() {
        String::new()
    } else if s.find.is_empty() {
        "No results".to_string()
    } else {
        format!("{} of {}", s.find.current + 1, s.find.len())
    };

    let mut bar = row()
        .w_full()
        .h(40.0)
        .px(10.0)
        .gap(6.0)
        .center_y()
        .fill(k.c.bg_base)
        .child(
            col().w(240.0).child(
                TextInput::new(&s.find.query)
                    .placeholder("Find")
                    .width(None)
                    .on_edit(|s: &mut Workspace| &mut s.find.query)
                    .build(k)
                    .key("find-query"),
            ),
        )
        .child(
            text(count)
                .font(k.ty.small)
                .color(k.c.text_weak)
                .w(60.0)
                .pl(4.0),
        )
        .child(icon_button(&crate::icons::ARROW_UP, "Previous match", k).on_press(
            |s: &mut Workspace| s.find.previous(),
        ))
        .child(icon_button(&crate::icons::ARROW_DOWN, "Next match", k).on_press(
            |s: &mut Workspace| s.find.next(),
        ));

    if s.find.replacing {
        bar = bar
            .child(el().w(1.0).h(16.0).fill(k.c.border_base).mx(4.0))
            .child(
                col().w(200.0).child(
                    TextInput::new(&s.find.replacement)
                        .placeholder("Replace")
                        .width(None)
                        .on_edit(|s: &mut Workspace| &mut s.find.replacement)
                        .build(k),
                ),
            )
            .child(
                Button::new("Replace")
                    .control_size(ControlSize::Mini)
                    .on_press(|s: &mut Workspace| s.replace_current())
                    .build(k),
            )
            .child(
                Button::new("All")
                    .control_size(ControlSize::Mini)
                    .help("Replace all matches")
                    .on_press(|s: &mut Workspace| s.replace_all())
                    .build(k),
            );
    }

    bar.child(el().grow(1.0)).child(
        icon_button(&crate::icons::X, "Close find", k)
            .on_press(|s: &mut Workspace| s.find.close()),
    )
}

pub(crate) fn editor_area(s: &Workspace, k: &Kit, cx: &mut Cx<Workspace>) -> El {
    let Some(document) = s.active_document() else {
        return col()
            .grow(1.0)
            .min_h(0.0)
            .center()
            .fill(k.c.page)
            .child(
                col()
                    .gap(14.0)
                    .center()
                    .child(
                        text("Open a file to start editing")
                            .font(k.ty.ui_medium)
                            .color(k.c.text_muted),
                    )
                    .child(
                        col()
                            .gap(8.0)
                            .child(hint_row("Command palette", "Ctrl+Shift+P", k))
                            .child(hint_row("Terminal", "Ctrl+`", k)),
                    ),
            );
    };

    let advance = char_advance(s, cx);
    let primary = document_view(document, s, k, cx, 0, advance);

    let Some(secondary) = s.split_document.and_then(|index| s.documents.get(index)) else {
        s.split_left_rect
            .set(cx.layout("editor-primary").unwrap_or([0.0; 4]));
        s.split_right_rect.set([0.0; 4]);
        return col()
            .key("editor-primary")
            .grow(1.0)
            .min_h(0.0)
            .child(primary);
    };

    let focus_border = |focused: bool| {
        if focused {
            [2.0, 0.0, 0.0, 0.0]
        } else {
            [0.0; 4]
        }
    };

    // The divider uses the previous frame's width to turn pointer travel into
    // a ratio; a first frame without it still gives the panes their default
    // halves.
    s.split_width
        .set(cx.layout("editor-split").map(|rect| rect[2]).unwrap_or(0.0));
    s.split_left_rect
        .set(cx.layout("split-left").unwrap_or([0.0; 4]));
    s.split_right_rect
        .set(cx.layout("split-right").unwrap_or([0.0; 4]));
    let ratio = clamp_split_ratio(s.settings.split_ratio);

    row()
        .key("editor-split")
        .grow(1.0)
        .min_h(0.0)
        .child(
            col()
                .key("split-left")
                .grow(ratio)
                .min_w(0.0)
                .border_sides(focus_border(s.focused_pane == 0), k.c.text_accent)
                .child(primary),
        )
        .child(split_resizer(k))
        .child(
            col()
                .key("split-right")
                .grow(1.0 - ratio)
                .min_w(0.0)
                .border_sides(focus_border(s.focused_pane == 1), k.c.text_accent)
                .child(document_view(secondary, s, k, cx, 1, advance)),
        )
}

/// The draggable divider between the two editor panes.
fn split_resizer(k: &Kit) -> El {
    col()
        .key("split-resizer")
        .label("Resize split")
        .w(7.0)
        .h_full()
        .center()
        .fill(k.c.bg_deep)
        .cursor(rune::Cursor::ResizeColumn)
        .hit()
        .hover(k.c.surface_hover)
        .child(el().w(2.0).h(36.0).radius(1.0).fill(k.c.border_strong))
        .on_drag(
            |s: &mut Workspace, drag: &mut rune::Drag, _tx: &mut rune::Tx| {
                if drag.phase == rune::Phase::Up {
                    s.split_drag_start = None;
                    s.settings.save();
                    return;
                }
                if s.split_drag_start.is_none() {
                    s.split_drag_start = Some(s.settings.split_ratio);
                }
                if let Some(start) = s.split_drag_start {
                    let width = s.split_width.get();
                    let width = if width > 1.0 { width } else { 1000.0 };
                    s.settings.split_ratio = clamp_split_ratio(start + drag.dx / width);
                }
            },
        )
}

/// The PDF in `pane`, for the viewer's handlers.
/// One line of the empty state: what a shortcut does, and the shortcut as a key cap.
fn hint_row(action: &str, shortcut: &str, k: &Kit) -> El {
    row()
        .w(260.0)
        .gap(12.0)
        .center_y()
        .child(
            text(action.to_string())
                .font(k.ty.text_xsmall)
                .color(k.c.text_weak)
                .grow(1.0),
        )
        .child(
            text(shortcut.to_string())
                .font(k.ty.small)
                .color(k.c.text_muted)
                .px(6.0)
                .py(2.0)
                .radius(4.0)
                .border(1.0, k.c.border_base)
                .fill(k.c.bg_base),
        )
}

/// Whether a window point falls inside a laid-out rectangle.
fn inside(rect: [f32; 4], x: f32, y: f32) -> bool {
    rect[2] > 0.0
        && rect[3] > 0.0
        && x >= rect[0]
        && x <= rect[0] + rect[2]
        && y >= rect[1]
        && y <= rect[1] + rect[3]
}

fn pdf_at(s: &mut Workspace, pane: usize) -> &mut PdfView {
    let index = if pane == 0 {
        s.active
    } else {
        s.split_document.unwrap_or(s.active)
    };
    match s.documents.get_mut(index) {
        Some(Document::Pdf(pdf)) => pdf,
        _ => unreachable!("the pane shows a PDF"),
    }
}

fn document_view(
    document: &Document,
    s: &Workspace,
    k: &Kit,
    cx: &mut Cx<Workspace>,
    pane: usize,
    advance: f32,
) -> El {
    let view = match document {
        Document::Text(editor) => editor_view(editor, s, k, cx, pane, advance),
        Document::Markdown { editor, .. } => markdown_view(editor, s, k, cx, pane, advance),
        Document::Pdf(pdf) => {
            // Tell the viewer how dense this display is, so page bitmaps are
            // rendered sharp instead of at one pixel per point.
            pdf.set_display_scale(cx.env().scale);
            s.pdf_widths[pane].set(cx.layout(&format!("pdf-scroll-{pane}")).map(|r| r[2]).unwrap_or(0.0));
            pdf.view(
            k,
            pane,
            s.show_outline(),
            move |s: &mut Workspace, x: f32, y: f32| s.note_pdf_press(pane, x, y),
            move |s: &mut Workspace| pdf_at(s, pane),
            )
        }
        Document::Terminal(terminal) => terminal_view(terminal, s, k, pane, advance),
        Document::Image(image) => image.view(k),
        Document::Diff(diff) => diff.view(k),
        Document::Csv(csv) => csv.view(k, pane),
        Document::Notebook(notebook) => notebook.view(k),
    };

    view
}

fn char_advance(s: &Workspace, cx: &mut Cx<Workspace>) -> f32 {
    const CELLS: f32 = 64.0;
    cx.layout_with("ruler", |rect| rect[2])
        .map(|width| width / CELLS)
        .filter(|advance| *advance > 1.0)
        .unwrap_or(s.settings.font_size * 0.62)
}

/// The source line under a visual row in a softly wrapped document.
///
/// The editor uses a monospace font, so a greedy word wrap at the known
/// column count is a close match for the layout Rune draws.
/// The visual row a source line starts at, for scrolling to it.
pub(crate) fn visual_row_of(text: &str, line: u64, columns: usize) -> usize {
    if columns == 0 {
        return line.saturating_sub(1) as usize;
    }
    text.lines()
        .take(line.saturating_sub(1) as usize)
        .map(|l| wrapped_rows(l, columns))
        .sum()
}

pub(crate) fn wrapped_rows(line: &str, columns: usize) -> usize {
    if line.is_empty() {
        return 1;
    }
    let mut rows = 1usize;
    let mut used = 0usize;
    for word in line.split(' ') {
        let width = word.chars().count();
        if used == 0 {
            used = width;
        } else if used + 1 + width <= columns {
            used += 1 + width;
        } else {
            rows += 1;
            used = width;
        }
        while used > columns {
            rows += 1;
            used = used.saturating_sub(columns);
        }
    }
    rows
}

/// The line numbers, one per line, right-aligned by padding with spaces.
///
/// A monospace font makes the padding work, and one text element with as many
/// lines as the document is far cheaper than one element per number.
fn gutter_text(lines: usize) -> String {
    use std::fmt::Write;

    let digits = lines.to_string().len();
    let mut out = String::with_capacity(lines * (digits + 1));
    for line in 1..=lines {
        let _ = writeln!(out, "{line:>digits$}");
    }
    out
}

/// The coloured spans for a document, in the editor's current theme.
///
/// The highlighter caches its output per language and source text, so a frame
/// that draws unchanged text highlights nothing.
fn syntax_spans(editor: &Editor, s: &Workspace, k: &Kit) -> Vec<rune::Span> {
    let mut spans = match editor
        .location
        .as_ref()
        .map(lide_resource::Location::path_for_kind)
        .and_then(|path| lang_for_path(&path.to_string_lossy()))
    {
        Some(language) => {
            let found = highlight(Some(language), editor.text());
            SyntaxTheme::from_kit(k).spans(&found, 0)
        }
        None => Vec::new(),
    };

    if let Some(path) = editor
        .location
        .as_ref()
        .map(lide_resource::Location::path_for_kind)
    {
        let diagnostics = s.servers.diagnostics_for(path);
        spans.extend(diagnostic_spans(editor.text(), path, diagnostics, k));
    }
    spans
}

/// The underlines for the diagnostics a server reported for `path`.
///
/// A server's columns are UTF-16 units, so they go through [`offset_of`]
/// rather than being used as byte offsets. A zero-length range underlines the
/// character it starts at, which is what a server means by "here".
pub(crate) fn diagnostic_spans(
    text: &str,
    path: &Path,
    diagnostics: &[lide_lsp::Diagnostic],
    k: &Kit,
) -> Vec<rune::Span> {
    let mut spans = Vec::new();
    let starts = line_starts(text);
    for diagnostic in diagnostics {
        if diagnostic.path != path {
            continue;
        }
        let start = offset_at_line(text, &starts, diagnostic.start.line, diagnostic.start.character);
        if start >= text.len() {
            continue;
        }
        let end = offset_at_line(text, &starts, diagnostic.end.line, diagnostic.end.character);
        let end = if end > start {
            end.min(text.len())
        } else {
            text[start..]
                .chars()
                .next()
                .map(|ch| start + ch.len_utf8())
                .unwrap_or(start)
        };
        if end <= start {
            continue;
        }
        let color = match diagnostic.severity {
            lide_lsp::Severity::Error => k.c.danger.fg,
            lide_lsp::Severity::Warning => k.c.warning.fg,
            lide_lsp::Severity::Information | lide_lsp::Severity::Hint => k.c.text_accent,
        };
        spans.push(rune::Span::new(start, end).underline(color));
    }
    spans
}

fn editor_view(
    editor: &Editor,
    s: &Workspace,
    k: &Kit,
    cx: &mut Cx<Workspace>,
    pane: usize,
    advance: f32,
) -> El {
    let font_size = s.settings.font_size;
    let style = crate::theme::mono_style(font_size);
    let spans = syntax_spans(editor, s, k);
    let viewport_width = cx
        .layout(&format!("editor-scroll-{pane}"))
        .map(|rect| rect[2])
        .unwrap_or(900.0);
    let wrap_width = (viewport_width - 28.0).max(120.0);
    let wrap_columns = ((wrap_width - 28.0) / advance.max(1.0)).floor().max(1.0) as usize;
    s.wrap_columns[pane].set(wrap_columns);

    let field = text_input(&editor.edit, style)
        .spans(spans)
        .key(format!("editor-{pane}"))
        .on_edit(move |s: &mut Workspace| {
            let index = if pane == 0 {
                s.active
            } else {
                s.split_document.unwrap_or(s.active)
            };
            &mut s.documents[index]
                .editor_mut()
                .expect("the pane shows a text document")
                .edit
        })
        .on_submit(move |s: &mut Workspace, _| {
            let index = if pane == 0 {
                s.active
            } else {
                s.split_document.unwrap_or(s.active)
            };
            if let Some(editor) = s.documents[index].editor_mut() {
                editor.edit.insert("\n");
            }
        })
        .py(8.0)
        .color(k.c.text_base)
        // The pointer reports the modifiers it moves with, so a Cmd-click can
        // be told from a plain one even when the key event went elsewhere.
        .on_pointer_move_local(|s: &mut Workspace, _event, tx: &mut Tx| {
            let mods = tx.mods();
            s.primary_modifier = mods.primary() || mods.ctrl;
        });
    let completion = if s.completion_open {
        Some(completion_list(s, k, pane, advance))
    } else if s.code_actions_open {
        Some(code_action_list(s, k, pane, advance))
    } else {
        None
    };

    let body = if s.settings.soft_wrap {
        // Wrapped lines and a single gutter line per source line do not line
        // up, so the gutter is hidden in wrap mode. The field is pinned to the
        // viewport, which is also what makes the wrap follow a resized split.
        row()
            .items_start()
            .child(field.w(wrap_width).min_w(0.0).px(14.0))
    } else if s.settings.line_numbers {
        let (lines, longest) = editor.line_stats();
        // The field is made wide enough for its longest line so that nothing
        // wraps: a wrapped line would take two rows in the text and one in
        // the gutter.
        let padding = 24.0;
        let field_width = ((longest as f32) * advance + padding + 14.0).max(120.0);
        let digits = lines.to_string().len();
        // Padding is added outside the width, so this is the digits alone.
        let gutter_width = digits as f32 * advance + 2.0;

        let gutter = text(gutter_text(lines))
            .key("gutter")
            .font(style)
            .color(k.c.text_faint)
            .w(gutter_width)
            .shrink()
            .pl(14.0)
            .pr(12.0)
            .py(8.0);

        row()
            .items_start()
            .child(gutter)
            .child(el().w(1.0).h_full().fill(k.c.border_base))
            .child(field.w(field_width).pl(14.0))
    } else {
        row().items_start().child(field.w_full().min_w(0.0))
    };

    let scroll_offset = s.editor_scrolls[pane].clone();
    let scroll = col()
        .key(format!("editor-scroll-{pane}"))
        .grow(1.0)
        .min_h(0.0)
        .scroll_xy()
        .fill(k.c.page)
        .on_scroll_offset(move |at: ScrollPos| {
            scroll_offset.set([at.offset[1], at.max[1]]);
        })
        .child(body)
        .children(completion);

    scroll
}

/// The code actions the server offered, at the caret.
fn code_action_list(s: &Workspace, k: &Kit, pane: usize, advance: f32) -> El {
    let Some(editor) = s.active_document().and_then(|document| document.editor()) else {
        return el();
    };
    let line = editor.edit.line_of_byte(editor.edit.focus);
    let line_start = editor.edit.line_range(line).start;
    let column = editor
        .edit
        .text
        .get(line_start..editor.edit.focus)
        .map(|text| text.chars().count())
        .unwrap_or(0);

    let line_height = s.settings.line_height();
    let gutter = if s.settings.line_numbers && !s.settings.soft_wrap {
        let digits = editor.line_count().to_string().len();
        digits as f32 * advance + 16.0
    } else {
        0.0
    };
    let x = gutter + 12.0 + column as f32 * advance;
    let y = 8.0 + (line as f32 + 1.0) * line_height;

    let mut list = col().w_full().gap(1.0);
    for (index, action) in s.code_actions.iter().take(10).enumerate() {
        list = list.child(
            row()
                .key(format!("action-{index}"))
                .w_full()
                .px(8.0)
                .py(3.0)
                .radius(3.0)
                .fill(k.c.layer_01)
                .hover(k.c.surface_hover)
                .on_press(move |s: &mut Workspace| s.apply_code_action(index))
                .child(
                    text(action.title.clone())
                        .font(k.ty.ui_compact)
                        .color(k.c.text_base),
                ),
        );
    }

    el().key(format!("code-actions-{pane}"))
        .absolute(x, y)
        .w(360.0)
        .radius(6.0)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_base)
        .padding(4.0)
        .child(list)
}

/// The completion list, placed at the caret.
///
/// It is a child of the editor's scrolling content rather than an overlay, so
/// it scrolls with the text and sits on the line the caret is on. Positioning
/// it exactly needs the caret's rect, which the text field does not publish;
/// this uses the caret's line and column, which is the same thing as long as
/// nothing wraps — and nothing wraps, because the field is as wide as its
/// longest line.
fn completion_list(s: &Workspace, k: &Kit, pane: usize, advance: f32) -> El {
    let Some(editor) = s.active_document().and_then(|document| document.editor()) else {
        return el();
    };
    let line = editor.edit.line_of_byte(editor.edit.focus);
    let line_start = editor.edit.line_range(line).start;
    let column = editor
        .edit
        .text
        .get(line_start..editor.edit.focus)
        .map(|text| text.chars().count())
        .unwrap_or(0);

    let line_height = s.settings.line_height();
    let gutter = if s.settings.line_numbers && !s.settings.soft_wrap {
        let digits = editor.line_count().to_string().len();
        digits as f32 * advance + 16.0
    } else {
        0.0
    };

    let x = gutter + 12.0 + column as f32 * advance;
    let y = 8.0 + (line as f32 + 1.0) * line_height;

    let mut list = col().w_full().gap(1.0);
    for (index, item) in s.completions.iter().take(10).enumerate() {
        let selected = index == s.completion_selected;
        let kind = item.kind_label();
        let mut row = row()
            .w_full()
            .px(8.0)
            .py(3.0)
            .gap(8.0)
            .center_y()
            .radius(3.0)
            .fill(if selected {
                k.c.surface_hover
            } else {
                k.c.layer_01
            })
            .child(
                text(item.label.clone())
                    .font(k.ty.ui_compact)
                    .color(k.c.text_base)
                    .grow(1.0),
            );
        if !kind.is_empty() {
            row = row.child(text(kind).font(k.ty.small).color(k.c.text_weak));
        }
        if let Some(detail) = &item.detail {
            row = row.child(text(detail.clone()).font(k.ty.small).color(k.c.text_muted));
        }
        list = list.child(row.on_press(move |s: &mut Workspace| {
            s.completion_selected = index;
            s.accept_completion();
        }));
    }

    el().key(format!("completion-{pane}"))
        .absolute(x, y)
        .w(380.0)
        .radius(6.0)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_base)
        .padding(4.0)
        .child(list)
}

fn markdown_view(
    editor: &Editor,
    s: &Workspace,
    k: &Kit,
    cx: &mut Cx<Workspace>,
    pane: usize,
    advance: f32,
) -> El {
    let _ = advance;
    let style = crate::theme::mono_style(s.settings.font_size);
    let source_scroll = s.markdown_scrolls[pane * 2].clone();
    let preview_scroll = s.markdown_scrolls[pane * 2 + 1].clone();
    let document_index = if pane == 0 {
        s.active
    } else {
        s.split_document.unwrap_or(s.active)
    };
    let outline = s
        .documents
        .get(document_index)
        .and_then(|document| match document {
            Document::Markdown { outline, .. } => Some(outline.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let outline_open = s.show_outline() && !outline.is_empty();
    let area_key = format!("markdown-split-{pane}");
    s.markdown_widths[pane].set(cx.layout(&area_key).map(|rect| rect[2]).unwrap_or(0.0));
    let markdown_ratio = clamp_split_ratio(s.settings.markdown_ratio);

    let source = text_input(&editor.edit, style)
        .spans(syntax_spans(editor, s, k))
        .key(format!("markdown-source-{pane}"))
        .on_edit(move |s: &mut Workspace| {
            let index = if pane == 0 {
                s.active
            } else {
                s.split_document.unwrap_or(s.active)
            };
            &mut s.documents[index]
                .editor_mut()
                .expect("the pane shows a text document")
                .edit
        })
        .on_submit(move |s: &mut Workspace, _| {
            let index = if pane == 0 {
                s.active
            } else {
                s.split_document.unwrap_or(s.active)
            };
            if let Some(editor) = s.documents[index].editor_mut() {
                editor.edit.insert("\n");
            }
        })
        .w_full()
        .px(12.0)
        .py(8.0)
        .color(k.c.text_base);

    let preview = Markdown::new(editor.text()).render(&MarkdownStyle::from_kit(k));

    // The outline navigates the source: clicking a heading puts the caret on
    // the line it is on, which is what makes the preview and the source feel
    // like one document.
    let mut outline_pane = col().w(190.0).shrink().gap(1.0).px(6.0).py(8.0).child(
        text("Outline")
            .font(k.ty.small_medium)
            .color(k.c.text_muted)
            .px(6.0)
            .pb(4.0),
    );
    if outline.is_empty() {
        outline_pane =
            outline_pane.child(empty_note("No headings", k));
    }
    for (index, entry) in outline.iter().enumerate() {
        let line = entry.line;
        outline_pane = outline_pane.child(
            text(entry.title.clone())
                .key(format!("outline-{pane}-{index}"))
                .font(k.ty.text_xsmall)
                .min_w(0.0)
                .truncate()
                .color(if entry.level <= 2 {
                    k.c.text_base
                } else {
                    k.c.text_muted
                })
                .w_full()
                .px(6.0)
                .py(2.0)
                .pl(6.0 + 10.0 * entry.level.saturating_sub(1) as f32)
                .radius(5.0)
                .hover(k.c.hover)
                .on_press(move |s: &mut Workspace| {
                    if let Some(editor) = s.active_editor_mut() {
                        editor.go_to(line, 0);
                    }
                }),
        );
    }

    let outline_pane = if outline_open {
        outline_pane
    } else {
        el().h(0.0)
    };
    let source_area = row()
        .key(area_key)
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0)
        .child({
            let live = source_scroll.clone();
            let reported = live.clone();
            let other = preview_scroll.clone();
            let other_key = format!("preview-scroll-{pane}");
            col()
                .key(format!("markdown-scroll-{pane}"))
                .grow(markdown_ratio)
                .min_w(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .on_scroll_offset(move |at: ScrollPos| {
                    reported.set([at.offset[1], at.max[1]]);
                })
                .on_wheel(move |_s: &mut Workspace, ev: WheelEvent, tx: &mut Tx| {
                    mirror_scroll(&live, &other, &other_key, ev.dy, tx);
                    // Leave the native scroll to the pane the pointer is over.
                    false
                })
                .child(source)
        })
        .child(markdown_resizer(k, pane))
        .child({
            let live = preview_scroll.clone();
            let reported = live.clone();
            let other = source_scroll.clone();
            let other_key = format!("markdown-scroll-{pane}");
            col()
                .key(format!("preview-scroll-{pane}"))
                .grow(1.0 - markdown_ratio)
                .min_w(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .on_scroll_offset(move |at: ScrollPos| {
                    reported.set([at.offset[1], at.max[1]]);
                })
                .on_wheel(move |_s: &mut Workspace, ev: WheelEvent, tx: &mut Tx| {
                    mirror_scroll(&live, &other, &other_key, ev.dy, tx);
                    false
                })
                .padding(16.0)
                .child(preview)
        });

    row()
        .grow(1.0)
        .min_h(0.0)
        .child(outline_pane)
        .child(if outline_open {
            el().w(1.0).h_full().fill(k.c.border_base)
        } else {
            el().h(0.0)
        })
        .child(source_area)
}

/// The draggable divider between a Markdown tab's source and preview.
fn markdown_resizer(k: &Kit, pane: usize) -> El {
    col()
        .key(format!("markdown-resizer-{pane}"))
        .label("Resize Markdown source and preview")
        .w(6.0)
        .h_full()
        .center()
        .fill(k.c.bg_base)
        .cursor(rune::Cursor::ResizeColumn)
        .hit()
        .hover(k.c.surface_hover)
        .child(
            el()
                .w(2.0)
                .h(36.0)
                .radius(1.0)
                .fill(k.c.border_strong),
        )
        .on_drag(move |s: &mut Workspace, drag: &mut rune::Drag, _tx: &mut rune::Tx| {
            if drag.phase == rune::Phase::Up {
                s.markdown_drag_start = None;
                s.settings.save();
                return;
            }
            if s.markdown_drag_start.is_none() {
                s.markdown_drag_start = Some(s.settings.markdown_ratio);
            }
            if let Some(start) = s.markdown_drag_start {
                let width = s.markdown_widths[pane].get();
                let width = if width > 1.0 { width } else { 1000.0 };
                s.settings.markdown_ratio = clamp_split_ratio(start + drag.dx / width);
            }
        })
}

/// Move `other` to the same fraction of its content that `this` will be at
/// after a wheel of `dy` points, leaving `this` to scroll naturally.
///
/// The two panes hold different amounts of content — a wrapped source line is
/// not a rendered paragraph — so the common measure is the fraction scrolled,
/// which is what a reader means by "the same place".
fn mirror_scroll(
    this: &Live<[f32; 2]>,
    other: &Live<[f32; 2]>,
    other_key: &str,
    dy: f32,
    tx: &mut Tx,
) {
    let [offset, max] = this.get();
    let [_, other_max] = other.get();
    if max <= 0.0 || other_max <= 0.0 {
        return;
    }
    // A positive wheel delta moves the content down, towards its start.
    let projected = (offset - dy).clamp(0.0, max);
    tx.scroll_to_y(other_key.to_string(), projected / max * other_max);
}

/// The LaTeX build's output, at the bottom of the editor.
pub(crate) fn build_panel(s: &Workspace, k: &Kit) -> El {
    let Some(build) = &s.build else {
        return el().h(0.0);
    };

    let (status, colour) = if !build.finished {
        ("building…".to_string(), k.c.text_accent)
    } else if build.success {
        ("build succeeded".to_string(), k.c.success.fg)
    } else {
        (
            format!("build failed, {} errors", build.errors().len()),
            k.c.danger.fg,
        )
    };

    let mut lines = col().w_full().gap(0.0);

    // The errors first, each one a jump to the line that caused it. The key
    // carries the index, because a document can report two problems on one
    // line and a duplicated key would make one row unclickable.
    for (index, error) in build.errors().iter().take(8).enumerate() {
        let name = error
            .path
            .strip_prefix(s.root.as_deref().unwrap_or(std::path::Path::new("")))
            .unwrap_or(&error.path)
            .display()
            .to_string();
        let path = error.path.clone();
        let line = error.line;
        lines = lines.child(
            row()
                .key(format!("build-error-{name}-{line}-{index}"))
                .w_full()
                .gap(8.0)
                .radius(3.0)
                .hover(k.c.hover)
                .on_press(move |s: &mut Workspace| {
                    s.open_match(path.clone(), line, 0);
                })
                .child(
                    text(format!("{name}:{line}"))
                        .font(k.ty.small_medium)
                        .color(k.c.text_accent),
                )
                .child(
                    text(error.message.clone())
                        .font(k.ty.small)
                        .color(k.c.text_base)
                        .grow(1.0),
                ),
        );
    }

    for line in build.tail(8) {
        lines = lines.child(text(line.clone()).font(k.ty.small).color(k.c.text_muted));
    }

    col()
        .key("build-panel")
        .w_full()
        .max_h(220.0)
        .shrink()
        .fill(k.c.bg_base)
        .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base)
        .child(
            row()
                .w_full()
                .h(30.0)
                .pl(12.0)
                .pr(6.0)
                .gap(10.0)
                .center_y()
                .child(
                    text(build.options.command_line())
                        .font(k.ty.small)
                        .color(k.c.text_weak)
                        .grow(1.0)
                        .min_w(0.0)
                        .truncate(),
                )
                .child(text(status).font(k.ty.small_medium).color(colour))
                .child(
                    icon_button(&crate::icons::X, "Close build output", k).on_press(
                        |s: &mut Workspace| {
                            if let Some(build) = s.build.as_mut() {
                                build.cancel();
                            }
                            s.build = None;
                        },
                    ),
                ),
        )
        .child(
            col()
                .key("build-output")
                .w_full()
                .h(120.0)
                .scroll(ScrollSpec { stick_bottom: true })
                .px(12.0)
                .py(6.0)
                .child(lines),
        )
}
