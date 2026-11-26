use rune::{col, row, text, El, ScrollSpec};
use rune_kit::{Kit, List, ListItem, TextInput};

use crate::dialog::Dialog;
use crate::Workspace;

pub(crate) fn palette(s: &Workspace, k: &Kit, layer: &mut Vec<El>) {
    if !s.palette.open {
        return;
    }

    let items: Vec<ListItem> = s
        .palette
        .visible()
        .into_iter()
        .map(|entry| {
            let entry = entry.clone();
            let label = entry.label();
            let group = entry.group();

            // A task runs; a command is run. Both close the palette first, so
            // that a command which opens another dialog is not fighting it.
            ListItem::new(label)
                .group(group)
                .on_select(move |s: &mut Workspace, _tx| {
                    s.palette.close();
                    match entry.clone() {
                        crate::command::Entry::Command(command) => crate::command::run(command, s),
                        crate::command::Entry::Task(index, _) => s.run_task(index),
                        crate::command::Entry::RecentFile(index, _) => {
                            if let Some(path) = s.settings.recent_files.get(index).cloned() {
                                if let Err(error) = s.open_path(&path) {
                                    s.set_notice(error);
                                }
                            }
                        }
                        crate::command::Entry::RecentProject(index, _) => {
                            if let Some(path) = s.settings.recent_projects.get(index).cloned() {
                                if let Err(error) = s.open_folder(&path) {
                                    s.set_notice(error.to_string());
                                }
                            }
                        }
                    }
                })
        })
        .collect();

    let mut body = List::new();
    for item in items {
        body = body.item(item);
    }

    let footer = row()
        .w_full()
        .px(16.0)
        .py(8.0)
        .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_muted)
        .child(
            text("↑↓ move   ↵ run   esc close")
                .font(k.ty.small)
                .color(k.c.text_weak),
        );

    Dialog::new("palette", "Commands")
        .bare()
        .open(true)
        .fit()
        .click_outside(true)
        .width(520.0)
        .on_close(|s: &mut Workspace| s.palette.close())
        .child(col().w_full().px(10.0).py(6.0).child(
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
        ))
        .child(
            col()
                .key("palette-results")
                .w_full()
                .px(6.0)
                .max_h(320.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(body.build(k)),
        )
        .child(footer)
        .build(k, layer);
}

