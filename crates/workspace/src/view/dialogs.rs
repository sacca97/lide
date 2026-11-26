use rune::{col, row, text, El};
use rune_kit::{Button, Kit, TextInput};

use crate::dialog::Dialog;
use crate::{DialogState, Workspace};

pub(crate) fn dialogs(s: &Workspace, k: &Kit, layer: &mut Vec<El>) {
    match &s.dialog {
        None => {}
        Some(DialogState::NewFile { name, error }) => {
            let mut dialog = Dialog::new("new-file", "New file")
                .open(true)
                .fit()
                .width(420.0)
                .on_close(|s: &mut Workspace| s.dialog = None)
                .child(
                    TextInput::new(name)
                        .placeholder("name.rs")
                        .width(None)
                        .on_edit(|s: &mut Workspace| match &mut s.dialog {
                            Some(DialogState::NewFile { name, .. }) => name,
                            _ => unreachable!("the new-file dialog is open"),
                        })
                        .on_submit(|s: &mut Workspace, _| s.confirm_new_file())
                        .build(k),
                );
            if let Some(error) = error {
                dialog = dialog.child(text(error.clone()).font(k.ty.small).color(k.c.danger.fg));
            }
            dialog
                .child(
                    row()
                        .w_full()
                        .gap(8.0)
                        .justify_end()
                        .child(
                            Button::new("Cancel")
                                .on_press(|s: &mut Workspace| s.dialog = None)
                                .build(k),
                        )
                        .child(
                            Button::new("Create")
                                .on_press(|s: &mut Workspace| s.confirm_new_file())
                                .build(k),
                        ),
                )
                .build(k, layer);
        }
        Some(DialogState::Rename { name, error }) => {
            let mut dialog = Dialog::new("rename", "Rename file")
                .open(true)
                .fit()
                .width(420.0)
                .on_close(|s: &mut Workspace| s.dialog = None)
                .child(
                    TextInput::new(name)
                        .placeholder("new name")
                        .width(None)
                        .on_edit(|s: &mut Workspace| match &mut s.dialog {
                            Some(DialogState::Rename { name, .. }) => name,
                            _ => unreachable!("the rename dialog is open"),
                        })
                        .on_submit(|s: &mut Workspace, _| s.confirm_rename())
                        .build(k),
                );
            if let Some(error) = error {
                dialog = dialog.child(text(error.clone()).font(k.ty.small).color(k.c.danger.fg));
            }
            dialog
                .child(
                    row()
                        .w_full()
                        .gap(8.0)
                        .justify_end()
                        .child(
                            Button::new("Cancel")
                                .on_press(|s: &mut Workspace| s.dialog = None)
                                .build(k),
                        )
                        .child(
                            Button::new("Rename")
                                .on_press(|s: &mut Workspace| s.confirm_rename())
                                .build(k),
                        ),
                )
                .build(k, layer);
        }
        Some(DialogState::RenameSymbol { name, error }) => {
            let mut dialog = Dialog::new("rename-symbol", "Rename symbol")
                .open(true)
                .fit()
                .width(420.0)
                .on_close(|s: &mut Workspace| s.dialog = None)
                .child(
                    TextInput::new(name)
                        .placeholder("new name")
                        .width(None)
                        .on_edit(|s: &mut Workspace| match &mut s.dialog {
                            Some(DialogState::RenameSymbol { name, .. }) => name,
                            _ => unreachable!("the rename-symbol dialog is open"),
                        })
                        .on_submit(|s: &mut Workspace, _| s.confirm_rename_symbol())
                        .build(k),
                );
            if let Some(error) = error {
                dialog = dialog.child(text(error.clone()).font(k.ty.small).color(k.c.danger.fg));
            }
            dialog
                .child(
                    row()
                        .w_full()
                        .gap(8.0)
                        .justify_end()
                        .child(
                            Button::new("Cancel")
                                .on_press(|s: &mut Workspace| s.dialog = None)
                                .build(k),
                        )
                        .child(
                            Button::new("Rename")
                                .on_press(|s: &mut Workspace| s.confirm_rename_symbol())
                                .build(k),
                        ),
                )
                .build(k, layer);
        }
        Some(DialogState::RemoteFile { hosts, host, path }) => {
            let mut list = col().w_full().gap(1.0);
            for (index, entry) in hosts.iter().take(8).enumerate() {
                let entry = entry.clone();
                list = list.child(
                    text(entry.clone())
                        .key(format!("remote-host-{index}"))
                        .font(k.ty.ui)
                        .color(k.c.text_base)
                        .w_full()
                        .px(6.0)
                        .py(2.0)
                        .radius(4.0)
                        .hover(k.c.hover)
                        .on_press(move |s: &mut Workspace| {
                            if let Some(DialogState::RemoteFile { host, .. }) = &mut s.dialog {
                                host.replace_text(entry.clone());
                            }
                        }),
                );
            }

            Dialog::new("remote-file", "Open a remote file")
                .open(true)
                .fit()
                .width(480.0)
                .on_close(|s: &mut Workspace| s.dialog = None)
                .child(list)
                .child(
                    TextInput::new(host)
                        .placeholder("host or Host alias")
                        .width(None)
                        .on_edit(|s: &mut Workspace| match &mut s.dialog {
                            Some(DialogState::RemoteFile { host, .. }) => host,
                            _ => unreachable!("the remote-file dialog is open"),
                        })
                        .build(k),
                )
                .child(
                    TextInput::new(path)
                        .placeholder("/absolute/path/on/the/host.rs")
                        .width(None)
                        .on_edit(|s: &mut Workspace| match &mut s.dialog {
                            Some(DialogState::RemoteFile { path, .. }) => path,
                            _ => unreachable!("the remote-file dialog is open"),
                        })
                        .on_submit(|s: &mut Workspace, _| s.confirm_remote_file())
                        .build(k),
                )
                .child(
                    row()
                        .w_full()
                        .gap(8.0)
                        .justify_end()
                        .child(
                            Button::new("Cancel")
                                .on_press(|s: &mut Workspace| s.dialog = None)
                                .build(k),
                        )
                        .child(
                            Button::new("Open")
                                .on_press(|s: &mut Workspace| s.confirm_remote_file())
                                .build(k),
                        ),
                )
                .build(k, layer);
        }
        Some(DialogState::SshHosts { hosts, name }) => {
            let mut list = col().w_full().gap(1.0);
            for (index, host) in hosts.iter().take(12).enumerate() {
                let host = host.clone();
                list = list.child(
                    text(host.clone())
                        .key(format!("ssh-host-{index}"))
                        .font(k.ty.ui)
                        .color(k.c.text_base)
                        .w_full()
                        .px(6.0)
                        .py(2.0)
                        .radius(4.0)
                        .hover(k.c.hover)
                        .on_press(move |s: &mut Workspace| {
                            if let Some(DialogState::SshHosts { name, .. }) = &mut s.dialog {
                                name.replace_text(host.clone());
                            }
                        }),
                );
            }

            Dialog::new("ssh-hosts", "Connect over SSH")
                .open(true)
                .fit()
                .width(420.0)
                .on_close(|s: &mut Workspace| s.dialog = None)
                .child(list)
                .child(
                    TextInput::new(name)
                        .placeholder("user@host, or a Host alias")
                        .width(None)
                        .on_edit(|s: &mut Workspace| match &mut s.dialog {
                            Some(DialogState::SshHosts { name, .. }) => name,
                            _ => unreachable!("the SSH dialog is open"),
                        })
                        .on_submit(|s: &mut Workspace, _| s.confirm_ssh_connect())
                        .build(k),
                )
                .child(
                    row()
                        .w_full()
                        .gap(8.0)
                        .justify_end()
                        .child(
                            Button::new("Cancel")
                                .on_press(|s: &mut Workspace| s.dialog = None)
                                .build(k),
                        )
                        .child(
                            Button::new("Connect")
                                .on_press(|s: &mut Workspace| s.confirm_ssh_connect())
                                .build(k),
                        ),
                )
                .build(k, layer);
        }
        Some(DialogState::ConfirmReplace { files, matches }) => {
            Dialog::new("confirm-replace", "Replace across the workspace")
                .open(true)
                .fit()
                .width(460.0)
                .on_close(|s: &mut Workspace| s.dialog = None)
                .child(
                    text(format!(
                        "Replace {matches} matches in {files} files with \"{}\"?",
                        s.search_replacement.text
                    ))
                    .font(k.ty.ui)
                    .color(k.c.text_base),
                )
                .child(
                    text("Files are written on a background thread; open buffers are reloaded when it finishes.")
                        .font(k.ty.small)
                        .color(k.c.text_weak),
                )
                .child(
                    row()
                        .w_full()
                        .gap(8.0)
                        .justify_end()
                        .child(
                            Button::new("Cancel")
                                .on_press(|s: &mut Workspace| s.dialog = None)
                                .build(k),
                        )
                        .child(
                            Button::new("Replace")
                                .on_press(|s: &mut Workspace| s.confirm_replace_all())
                                .build(k),
                        ),
                )
                .build(k, layer);
        }
        Some(DialogState::LatexOutput { directory }) => {
            Dialog::new("latex-output", "LaTeX output")
                .open(true)
                .fit()
                .width(460.0)
                .on_close(|s: &mut Workspace| s.dialog = None)
                .child(
                    text("Output directory (empty writes beside the .tex file)")
                        .font(k.ty.small)
                        .color(k.c.text_weak),
                )
                .child(
                    TextInput::new(directory)
                        .placeholder("build")
                        .width(None)
                        .on_edit(|s: &mut Workspace| match &mut s.dialog {
                            Some(DialogState::LatexOutput { directory }) => directory,
                            _ => unreachable!("the LaTeX output dialog is open"),
                        })
                        .build(k),
                )
                .child(
                    row()
                        .w_full()
                        .gap(8.0)
                        .justify_end()
                        .child(
                            Button::new("Cancel")
                                .on_press(|s: &mut Workspace| s.dialog = None)
                                .build(k),
                        )
                        .child(
                            Button::new("Save")
                                .on_press(|s: &mut Workspace| s.confirm_latex_output())
                                .build(k),
                        ),
                )
                .build(k, layer);
        }
        Some(DialogState::TerminalSettings { shell, directory }) => {
            Dialog::new("terminal-settings", "Terminal")
                .open(true)
                .fit()
                .width(480.0)
                .on_close(|s: &mut Workspace| s.dialog = None)
                .child(
                    text("Shell (empty uses $SHELL)")
                        .font(k.ty.small)
                        .color(k.c.text_weak),
                )
                .child(
                    TextInput::new(shell)
                        .placeholder("/bin/zsh")
                        .width(None)
                        .on_edit(|s: &mut Workspace| match &mut s.dialog {
                            Some(DialogState::TerminalSettings { shell, .. }) => shell,
                            _ => unreachable!("the terminal-settings dialog is open"),
                        })
                        .build(k),
                )
                .child(
                    text("Start directory (empty uses the project root)")
                        .font(k.ty.small)
                        .color(k.c.text_weak),
                )
                .child(
                    TextInput::new(directory)
                        .placeholder("~/work")
                        .width(None)
                        .on_edit(|s: &mut Workspace| match &mut s.dialog {
                            Some(DialogState::TerminalSettings { directory, .. }) => directory,
                            _ => unreachable!("the terminal-settings dialog is open"),
                        })
                        .build(k),
                )
                .child(
                    row()
                        .w_full()
                        .gap(8.0)
                        .justify_end()
                        .child(
                            Button::new("Cancel")
                                .on_press(|s: &mut Workspace| s.dialog = None)
                                .build(k),
                        )
                        .child(
                            Button::new("Save")
                                .on_press(|s: &mut Workspace| s.confirm_terminal_settings())
                                .build(k),
                        ),
                )
                .build(k, layer);
        }
        Some(DialogState::ConfirmDelete) => {
            Dialog::new("confirm-delete", "Discard changes?")
                .open(true)
                .fit()
                .width(420.0)
                .on_close(|s: &mut Workspace| s.dialog = None)
                .child(
                    text("This file has unsaved changes.")
                        .font(k.ty.ui)
                        .color(k.c.text_base),
                )
                .child(
                    row()
                        .w_full()
                        .gap(8.0)
                        .justify_end()
                        .child(
                            Button::new("Cancel")
                                .on_press(|s: &mut Workspace| s.dialog = None)
                                .build(k),
                        )
                        .child(
                            Button::new("Discard")
                                .on_press(|s: &mut Workspace| {
                                    s.dialog = None;
                                    let active = s.active;
                                    s.close(active);
                                })
                                .build(k),
                        ),
                )
                .build(k, layer);
        }
    }
}


