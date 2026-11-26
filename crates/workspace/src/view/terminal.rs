use rune::{col, el, row, text, text_input, El, ScrollSpec, Tx, WheelEvent};
use rune_kit::{Button, Kit};

use crate::{Document, Workspace, CARD_GAP};

pub(crate) fn terminal_view(
    terminal: &lide_terminal::TerminalView,
    s: &Workspace,
    k: &Kit,
    pane: usize,
    advance: f32,
) -> El {
    let advance = advance.max(1.0);
    let snapshot = terminal.emulator.snapshot();
    let line_height = s.settings.line_height();
    let style = crate::theme::mono_style(s.settings.font_size);
    let mut rows = col().w_full().gap(0.0);

    let selection = terminal.selection.map(|(start, end)| {
        if start <= end {
            (start, end)
        } else {
            (end, start)
        }
    });

    for (index, line_text) in snapshot.rows.iter().enumerate() {
        let is_cursor_row = snapshot.cursor_visible && index == snapshot.cursor.0;
        let mut line = row().h(line_height).items_center();

        if let Some(((start_row, start_column), (end_row, end_column))) = selection {
            if index >= start_row && index <= end_row {
                let chars = line_text.chars().count();
                let from = if index == start_row {
                    start_column.min(chars)
                } else {
                    0
                };
                let to = if index == end_row {
                    (end_column + 1).min(chars)
                } else {
                    chars
                };
                if to > from {
                    line = line.child(
                        el()
                            .absolute(from as f32 * advance_for(s), 0.0)
                            .w((to - from) as f32 * advance_for(s))
                            .h_full()
                            .fill(k.c.text_accent.alpha(0.25)),
                    );
                }
            }
        }

        line = line.child(text(line_text.clone()).font(style).color(k.c.text_base));

        if is_cursor_row {
            // The block belongs at the cursor's column, not at the end of the
            // row: an empty line otherwise puts it after the text.
            line = line.child(
                el()
                    .absolute(snapshot.cursor.1 as f32 * advance, line_height * 0.1)
                    .w(advance)
                    .h(line_height * 0.8)
                    .fill(k.c.text_accent)
                    .opacity(0.5),
            );
        }
        rows = rows.child(line);
    }

    let area = col()
        .key("terminal-area")
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: true })
        .hit()
        .fill(k.c.page)
        .padding(CARD_GAP)
        .on_wheel(move |s: &mut Workspace, ev: WheelEvent, _tx: &mut Tx| {
            if ev.dy == 0.0 {
                return false;
            }
            let line_height = s.settings.line_height().max(1.0);
            let lines = (-ev.dy / line_height).round() as i32;
            let lines = if lines == 0 {
                if ev.dy < 0.0 {
                    1
                } else {
                    -1
                }
            } else {
                lines
            };
            let index = if pane == 0 {
                s.active
            } else {
                s.split_document.unwrap_or(s.active)
            };
            if let Some(Document::Terminal(terminal)) = s.documents.get_mut(index) {
                terminal.scroll(lines);
            }
            true
        })
        .on_drag(
            move |s: &mut Workspace, drag: &mut rune::Drag, tx: &mut Tx| {
                let line_height = s.settings.line_height().max(1.0);
                let advance = advance_for(s).max(1.0);
                let point = |x: f32, y: f32| {
                    (
                        ((y - 6.0).max(0.0) / line_height) as usize,
                        ((x - 6.0).max(0.0) / advance) as usize,
                    )
                };
                let start = point(drag.start_x, drag.start_y);
                let end = point(drag.x, drag.y);
                let index = if pane == 0 {
                    s.active
                } else {
                    s.split_document.unwrap_or(s.active)
                };
                if let Some(Document::Terminal(terminal)) = s.documents.get_mut(index) {
                    terminal.select(start, end);
                    if drag.phase == rune::Phase::Up {
                        let text = terminal.selected_text();
                        if !text.trim().is_empty() {
                            tx.copy(text);
                        }
                    }
                }
            },
        )
        .child(rows);

    if !s.terminal_find.open {
        return area;
    }

    col()
        .grow(1.0)
        .min_h(0.0)
        .child(terminal_find_bar(s, k))
        .child(area)
}

/// The terminal's search bar.
fn terminal_find_bar(s: &Workspace, k: &Kit) -> El {
    row()
        .w_full()
        .h(34.0)
        .px(8.0)
        .gap(6.0)
        .center_y()
        .fill(k.c.bg_base)
        .child(
            text_input(&s.terminal_find.query, k.ty.ui)
                .key("terminal-find-query")
                .label("Find in terminal")
                .on_edit(|s: &mut Workspace| &mut s.terminal_find.query)
                .on_submit(|s: &mut Workspace, _| s.terminal_find_next())
                .w(220.0)
                .px(6.0)
                .py(3.0)
                .radius(5.0)
                .fill(k.c.surface_base),
        )
        .child(
            text(s.terminal_find.message.clone())
                .font(k.ty.small)
                .color(k.c.text_weak),
        )
        .child(
            Button::new("Previous")
                .on_press(|s: &mut Workspace| s.terminal_find_previous())
                .build(k),
        )
        .child(
            Button::new("Next")
                .on_press(|s: &mut Workspace| s.terminal_find_next())
                .build(k),
        )
        .child(
            Button::new("Close")
                .on_press(|s: &mut Workspace| s.terminal_find.close())
                .build(k),
        )
}

fn advance_for(s: &Workspace) -> f32 {
    s.settings.font_size * 0.62
}
