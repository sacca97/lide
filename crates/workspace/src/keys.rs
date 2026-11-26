//! The editor's keyboard handling.
//!
//! Rune routes a key to the focused control first. What arrives here is what
//! the control did not take — plus the keys a text field hands back through
//! `on_input_key`, which run before the field does (the palette's arrow keys).
//!
//! Keeping the whole mapping in one place is what makes the shortcuts
//! testable: a test can build a runtime, send a [`KeyEvent`] and look at the
//! state, without a window.

use rune::edit::{Key, KeyEvent};
use rune::Tx;

use crate::command::{self, Command};
use crate::{DialogState, Workspace};

/// Handle a key that reached the window.
///
/// Returns `true` when the key was consumed.
pub fn handle(workspace: &mut Workspace, event: &KeyEvent, tx: &mut Tx) -> bool {
    // Escape closes whatever is open, innermost first.
    if event.key == Key::Escape {
        if workspace.terminal_find.open {
            workspace.terminal_find.close();
            return true;
        }
        if workspace.code_actions_open {
            workspace.code_actions_open = false;
            return true;
        }
        if workspace.completion_open {
            workspace.close_completions();
            return true;
        }
        if workspace.palette.open {
            workspace.palette.close();
            return true;
        }
        if workspace.dialog.is_some() {
            workspace.dialog = None;
            return true;
        }
        if workspace.find.open {
            workspace.find.close();
            return true;
        }
        return false;
    }

    // A terminal owns the keyboard while it is in front. Only the palette and
    // quitting are kept back: a shell needs Ctrl+C for an interrupt and Ctrl+W
    // to delete a word, so the editor's own chords must not steal them.
    if workspace.active_terminal_mut().is_some() {
        if event.is_command('p') && event.mods.shift {
            open_palette(workspace);
            return true;
        }
        if event.is_command('q') && event.mods.shift {
            tx.quit();
            return true;
        }
        // Ctrl+Shift+F searches the terminal while it is in front; the same
        // chord searches the workspace everywhere else.
        if event.is_command('f') && event.mods.shift {
            workspace.open_terminal_find();
            return true;
        }
        // Linux terminals keep Ctrl+C and Ctrl+V for the shell and put the
        // clipboard on the Shift variants. macOS terminals use Cmd+C and
        // Cmd+V, leaving Control for the shell.
        let copy = event.is_command('c') && (cfg!(target_os = "macos") || event.mods.shift);
        let paste = event.is_command('v') && (cfg!(target_os = "macos") || event.mods.shift);
        if copy {
            if let Some(text) = workspace.terminal_text() {
                tx.copy(text);
            }
            return true;
        }
        if paste {
            tx.request_paste();
            return true;
        }
        // While the search field has the keyboard, what it declines is not
        // the shell's to take.
        if workspace.terminal_find.open {
            return false;
        }
        if let Some(key) = terminal_key(event) {
            workspace.write_terminal_key(&key);
            return true;
        }
    }

    // While the completion list is open it owns the arrows and Enter.
    if workspace.completion_open {
        match event.key {
            Key::Down => {
                workspace.completion_move(true);
                return true;
            }
            Key::Up => {
                workspace.completion_move(false);
                return true;
            }
            Key::Enter | Key::Tab => {
                workspace.accept_completion();
                return true;
            }
            _ => {}
        }
    }

    // Ctrl+Space asks for completions explicitly.
    if event.is_command(' ') {
        workspace.request_completions();
        return true;
    }

    if event.is_command('.') {
        command::run(Command::ShowCodeActions, workspace);
        return true;
    }

    if event.key == Key::Function(2) {
        command::run(Command::RenameSymbol, workspace);
        return true;
    }

    if event.key == Key::Function(12) {
        let command = if event.mods.shift {
            Command::FindReferences
        } else {
            Command::GoToDefinition
        };
        command::run(command, workspace);
        return true;
    }

    if event.is_command('b') && event.mods.shift {
        command::run(Command::BuildLatex, workspace);
        return true;
    }

    if event.is_command('j') && event.mods.shift {
        command::run(Command::Sync, workspace);
        return true;
    }

    // PDF navigation and zoom. The Ctrl+Page keys leave plain PageUp/PageDown
    // for the scroll viewport.
    if event.mods.cmd {
        let turn = match event.key {
            Key::PageDown => Some(true),
            Key::PageUp => Some(false),
            _ => None,
        };
        if let Some(next) = turn {
            let handled = if next {
                workspace.pdf_next_page()
            } else {
                workspace.pdf_previous_page()
            };
            if handled {
                let scroll = format!("pdf-scroll-{}", workspace.focused_pane);
                tx.scroll_to_instant(&scroll, 0.0, 0.0);
                return true;
            }
        }
    }
    if (event.is_command('=') || event.is_command('+')) && workspace.pdf_zoom_in() {
        return true;
    }
    if event.is_command('-') && workspace.pdf_zoom_out() {
        return true;
    }
    if event.is_command('0') && workspace.pdf_reset_zoom() {
        return true;
    }

    if event.is_command('g') {
        command::run(Command::ShowDiff, workspace);
        return true;
    }

    if event.is_command('k') {
        command::run(Command::ShowHover, workspace);
        return true;
    }

    if event.is_command('i') && event.mods.shift {
        command::run(Command::FormatDocument, workspace);
        return true;
    }

    // A dialog's Return confirms it.
    if event.key == Key::Enter && workspace.dialog.is_some() {
        match workspace.dialog {
            Some(DialogState::NewFile { .. }) => workspace.confirm_new_file(),
            Some(DialogState::Rename { .. }) => workspace.confirm_rename(),
            Some(DialogState::RenameSymbol { .. }) => workspace.confirm_rename_symbol(),
            Some(DialogState::TerminalSettings { .. }) => workspace.confirm_terminal_settings(),
            Some(DialogState::LatexOutput { .. }) => workspace.confirm_latex_output(),
            Some(DialogState::ConfirmReplace { .. }) => workspace.confirm_replace_all(),
            Some(DialogState::SshHosts { .. }) => workspace.confirm_ssh_connect(),
            Some(DialogState::RemoteFile { .. }) => workspace.confirm_remote_file(),
            Some(DialogState::ConfirmDelete) => workspace.confirm_delete(),
            None => {}
        }
        return true;
    }

    if event.is_command('p') && event.mods.shift {
        open_palette(workspace);
        return true;
    }

    // While the palette is open its field normally takes these through
    // `on_input_key`; this is the path for when nothing has focus.
    if workspace.palette.open {
        match event.key {
            Key::Down => {
                workspace.palette.down();
                return true;
            }
            Key::Up => {
                workspace.palette.up();
                return true;
            }
            Key::Enter => {
                workspace.run_selected_command();
                return true;
            }
            _ => {}
        }
    }

    // Ctrl+P / Ctrl+N move the palette's highlight, the way every command
    // palette does.
    if workspace.palette.open && (event.is_command('p') || event.is_command('n')) {
        if event.is_command('n') {
            workspace.palette.down();
        } else {
            workspace.palette.up();
        }
        return true;
    }

    if let Some(command) = shortcut(event) {
        command::run(command, workspace);
        return true;
    }

    if event.is_command('q') {
        tx.quit();
        return true;
    }

    false
}

/// Open the palette with the project's tasks in it.
fn open_palette(workspace: &mut Workspace) {
    let tasks: Vec<String> = workspace
        .tasks
        .iter()
        .map(|task| task.label.clone())
        .collect();
    workspace.palette.open(
        &tasks,
        &command::Recents {
            files: &workspace.settings.recent_files,
            projects: &workspace.settings.recent_projects,
        },
    );
}

/// Translate a key into the terminal's terms.
fn terminal_key(event: &KeyEvent) -> Option<lide_terminal::Key> {
    use lide_terminal::Key as TerminalKey;
    let key = match &event.key {
        Key::Text(text) if cfg!(target_os = "macos") && event.mods.ctrl => {
            // On macOS Control is separate from Command, so Ctrl+C and
            // friends arrive as text with the Control flag. A terminal wants
            // the corresponding control byte, including Ctrl+[ (Esc),
            // Ctrl+\ (SIGQUIT) and Ctrl+Space (NUL).
            match text.chars().next() {
                Some(ch) if text.chars().count() == 1 && ch.is_ascii() => TerminalKey::Control(ch),
                _ => TerminalKey::Text(text.clone()),
            }
        }
        Key::Text(text) if !event.mods.cmd => TerminalKey::Text(text.clone()),
        Key::Text(text) => TerminalKey::Text(text.clone()),
        Key::Command(_) if cfg!(target_os = "macos") => return None,
        Key::Command(letter) => TerminalKey::Control(*letter),
        Key::Enter => TerminalKey::Enter,
        Key::Tab => TerminalKey::Tab,
        Key::Backspace => TerminalKey::Backspace,
        Key::Escape => TerminalKey::Escape,
        Key::Up => TerminalKey::Up,
        Key::Down => TerminalKey::Down,
        Key::Left => TerminalKey::Left,
        Key::Right => TerminalKey::Right,
        Key::Home => TerminalKey::Home,
        Key::End => TerminalKey::End,
        Key::PageUp => TerminalKey::PageUp,
        Key::PageDown => TerminalKey::PageDown,
        Key::Delete => TerminalKey::Delete,
        _ => TerminalKey::Unsupported,
    };
    Some(key)
}

/// The command a keystroke runs, if any.
fn shortcut(event: &KeyEvent) -> Option<Command> {
    if event.is_command('s') {
        return Some(if event.mods.shift {
            Command::SaveAll
        } else {
            Command::Save
        });
    }
    if event.is_command('w') {
        return Some(Command::CloseTab);
    }
    if event.is_command('r') {
        return Some(Command::Reload);
    }
    if event.is_command('f') {
        return Some(if event.mods.shift {
            Command::SearchWorkspace
        } else {
            Command::Find
        });
    }
    if event.is_command('h') {
        return Some(Command::Replace);
    }
    if event.is_command('n') {
        return Some(Command::NewFile);
    }
    if event.is_command('b') {
        return Some(Command::ToggleSidebar);
    }
    if event.is_command('\\') {
        return Some(Command::ToggleSplit);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::edit::Mods;

    fn key(key: Key) -> KeyEvent {
        KeyEvent {
            key,
            mods: Mods::default(),
        }
    }

    fn chord(key: char, shift: bool) -> KeyEvent {
        KeyEvent {
            key: Key::Command(key),
            mods: Mods {
                cmd: true,
                ctrl: true,
                shift,
                ..Mods::default()
            },
        }
    }

    #[test]
    fn the_shortcut_table_covers_the_expected_commands() {
        assert_eq!(shortcut(&chord('s', false)), Some(Command::Save));
        assert_eq!(shortcut(&chord('s', true)), Some(Command::SaveAll));
        assert_eq!(shortcut(&chord('f', false)), Some(Command::Find));
        assert_eq!(shortcut(&chord('b', false)), Some(Command::ToggleSidebar));
        assert_eq!(shortcut(&chord('z', false)), None);
    }

    #[test]
    fn plain_keys_are_not_shortcuts() {
        assert_eq!(shortcut(&key(Key::Text("s".into()))), None);
    }

    #[test]
    fn terminal_control_keys_follow_the_platform() {
        // The primary chord is Ctrl on Linux and Cmd on macOS.
        let primary_c = KeyEvent {
            key: Key::Command('c'),
            mods: Mods {
                cmd: true,
                ctrl: !cfg!(target_os = "macos"),
                ..Mods::default()
            },
        };
        let mapped = terminal_key(&primary_c);
        if cfg!(target_os = "macos") {
            assert!(
                mapped.is_none(),
                "Cmd+C is the clipboard, not an interrupt to the shell"
            );
            // Control is separate from Command on macOS; Ctrl+C still reaches
            // the shell as ^C.
            for character in ['c', '[', '\\', ' '] {
                let control = KeyEvent {
                    key: Key::Text(character.to_string()),
                    mods: Mods {
                        ctrl: true,
                        ..Mods::default()
                    },
                };
                assert_eq!(
                    terminal_key(&control),
                    Some(lide_terminal::Key::Control(character)),
                    "Ctrl+{character:?} becomes a control byte"
                );
            }
        } else {
            assert_eq!(
                mapped,
                Some(lide_terminal::Key::Control('c')),
                "Ctrl+C interrupts on Linux"
            );
        }
    }
}
