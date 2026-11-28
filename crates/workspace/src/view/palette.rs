//! The command palette: a field, the matching commands under their groups,
//! and each command's shortcut as keycaps.
//!
//! It is drawn here rather than with the kit's dialog and list: those give
//! every dialog a title row and a close button, and set list rows at a size
//! and weight above the rest of the editor's chrome. A palette needs neither.

use rune::{col, el, row, text, El, ScrollSpec};
use rune_kit::{Kit, TextInput};

use crate::command::Entry;
use crate::Workspace;

pub(crate) fn palette(s: &Workspace, k: &Kit, layer: &mut Vec<El>) {
    if !s.palette.open {
        return;
    }

    let mut results = col().w_full().gap(2.0);
    let mut last_group = "";
    let visible = s.palette.visible();
    if visible.is_empty() {
        results = results.child(
            text("No matching commands")
                .font(k.ty.text_small)
                .color(k.c.text_weak)
                .px(10.0)
                .py(8.0),
        );
    }
    for (index, entry) in visible.into_iter().enumerate() {
        let group = entry.group();
        if group != last_group {
            // A group label sits closer to its own rows than to the group
            // above it.
            results = results.child(
                text(group)
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_faint)
                    .px(10.0)
                    .pt(if index == 0 { 4.0 } else { 12.0 })
                    .pb(4.0),
            );
            last_group = group;
        }
        results = results.child(palette_row(index, entry, index == s.palette.active, k));
    }

    let footer = row()
        .w_full()
        .px(16.0)
        .py(8.0)
        .gap(16.0)
        .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_muted)
        .child(hint(&["↑", "↓"], "move", k))
        .child(hint(&["↵"], "run", k))
        .child(hint(&["Esc"], "close", k));

    let field = col().w_full().px(8.0).pt(8.0).pb(4.0).child(
        TextInput::new(&s.palette.query)
            .placeholder("Type a command")
            .width(None)
            .on_edit(|s: &mut Workspace| &mut s.palette.query)
            .on_change(|s: &mut Workspace, _| s.palette.refresh())
            .on_key(|s: &mut Workspace, event, _tx| match event.key {
                rune::edit::Key::Down => {
                    s.palette.down();
                    true
                }
                rune::edit::Key::Up => {
                    s.palette.up();
                    true
                }
                rune::edit::Key::Enter => {
                    s.run_selected_command();
                    true
                }
                _ => false,
            })
            .build(k),
    );

    let panel = k
        .elevate(
            col()
                .key("palette-dialog")
                .role("dialog")
                .label("Commands")
                .w_full()
                .max_w(560.0)
                .shrink()
                .min_h(0.0)
                .radius(10.0)
                .fill(k.c.layer_01)
                .border(1.0, k.c.border_muted)
                .child(field)
                .child(
                    col()
                        .key("palette-results")
                        .w_full()
                        .px(4.0)
                        .pb(4.0)
                        .max_h(336.0)
                        .scroll(ScrollSpec {
                            stick_bottom: false,
                        })
                        .child(results),
                )
                .child(footer)
                .token_motion(k.motion.overlay),
            &k.elevation.overlay,
        )
        .modal(true, |s: &mut Workspace| s.palette.close())
        .modal_click_outside(true);

    layer.push(
        el().key("palette-scrim")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .fill(k.c.scrim)
            .token_motion(k.motion.overlay)
            .a11y_hidden(),
    );
    // A palette opens in the upper part of the window, where the eye already
    // is after the tab strip, not in its middle.
    layer.push(
        col()
            .key("palette-layer")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .px(16.0)
            .pt(96.0)
            .items_center()
            .child(panel),
    );
}

/// One command: its label, and its shortcut as keycaps on the right.
fn palette_row(index: usize, entry: &Entry, active: bool, k: &Kit) -> El {
    let label = entry.label();
    let shortcut = match entry {
        Entry::Command(command) => command.shortcut(),
        _ => None,
    };
    let mut item = row()
        .key(format!("palette-row-{index}"))
        .label(label.clone())
        .w_full()
        .h(32.0)
        .px(10.0)
        .gap(8.0)
        .center_y()
        .radius(6.0)
        .fill(if active {
            k.c.surface_raised_hover
        } else {
            k.c.layer_01
        })
        .hover(if active {
            k.c.surface_raised_hover
        } else {
            k.c.hover
        })
        .on_press(move |s: &mut Workspace| {
            s.palette.active = index;
            s.run_selected_command();
        })
        .child(
            text(label)
                .font(k.ty.text_small)
                .color(if active { k.c.text_base } else { k.c.text_muted })
                .grow(1.0)
                .min_w(0.0)
                .truncate(),
        );
    if let Some(shortcut) = shortcut {
        item = item.child(keycaps(shortcut, k));
    }
    item
}

/// A shortcut like `Ctrl+Shift+P`, one small key per part.
fn keycaps(shortcut: &str, k: &Kit) -> El {
    let mut keys = row().gap(4.0).center_y();
    for part in shortcut.split('+') {
        keys = keys.child(keycap(part, k));
    }
    keys
}

fn keycap(face: &str, k: &Kit) -> El {
    col()
        .h(20.0)
        .min_w(20.0)
        .px(6.0)
        .center()
        .radius(4.0)
        .fill(k.c.bg_base)
        .border(1.0, k.c.border_base)
        .child(
            text(face.to_string())
                .font(k.ty.text_xsmall)
                .color(k.c.text_weak),
        )
}

/// A key or two and what they do, for the footer.
fn hint(faces: &[&str], action: &str, k: &Kit) -> El {
    let mut item = row().gap(4.0).center_y();
    for face in faces {
        item = item.child(keycap(face, k));
    }
    item.child(
        text(action.to_string())
            .font(k.ty.text_xsmall)
            .color(k.c.text_weak)
            .pl(2.0),
    )
}
