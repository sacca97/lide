//! The command palette.
//!
//! Every action the editor can take is a [`Command`]. The palette lists them,
//! filters them as the user types, and runs the one that is picked. Keeping
//! the list in one place is what makes "keyboard accessible" true rather than
//! aspirational: a command that is in the list is reachable without a mouse.

use crate::Workspace;

/// Something the editor can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Write the active document to disk.
    Save,
    /// Write every document to disk.
    SaveAll,
    /// Close the active document.
    CloseTab,
    /// Close every document.
    CloseAll,
    /// Reload the active document from disk.
    Reload,
    /// Find in the current document.
    Find,
    /// Search the whole workspace.
    SearchWorkspace,
    /// Show the diagnostics the language servers reported.
    ShowProblems,
    /// Show the diff of the active file.
    ShowDiff,
    /// Open a terminal in the project.
    OpenTerminal,
    /// Build the LaTeX document.
    BuildLatex,
    /// Open the built PDF beside the source.
    ShowLatexPdf,
    /// Use the next engine for LaTeX builds.
    CycleLatexEngine,
    /// Show an SVG's source, or an SVG source's preview.
    ToggleImageSource,
    /// Open an SSH terminal.
    SshConnect,
    /// Open one file through an SSH agent.
    RemoteOpen,
    /// Search the active terminal's scrollback.
    FindInTerminal,
    /// Choose the appearance: follow the system, or force light or dark.
    Preferences,
    /// Choose the shell and start directory for new terminals.
    TerminalSettings,
    /// Choose where LaTeX builds write.
    LatexOutput,
    /// Check a JSON, TOML, YAML or XML document.
    ValidateDocument,
    /// Format a JSON or XML document.
    FormatStructuredDocument,
    /// Show the source line where it lands on the PDF.
    SyncForward,
    /// Jump between the LaTeX source and its PDF.
    Sync,
    /// Show the source of the place on the PDF.
    SyncInverse,
    /// Go to the next PDF page.
    PdfNextPage,
    /// Go to the previous PDF page.
    PdfPreviousPage,
    /// Zoom the PDF in.
    PdfZoomIn,
    /// Zoom the PDF out.
    PdfZoomOut,
    /// Show the PDF at actual size.
    PdfResetZoom,
    /// Switch which pane a tab click opens into.
    FocusOtherPane,
    /// Jump to where the symbol under the caret is defined.
    GoToDefinition,
    /// Rename the symbol under the caret everywhere it is used.
    RenameSymbol,
    /// Show what the language server offers to do here.
    ShowCodeActions,
    /// Find where the symbol under the caret is used.
    FindReferences,
    /// Show what the language server says is under the caret.
    ShowHover,
    /// Ask the language server to format the document.
    FormatDocument,
    /// Find and replace in the current document.
    Replace,
    /// Open the command palette.
    Palette,
    /// Switch between the light and the dark theme.
    ToggleTheme,
    /// Split the editor in two, or close the split.
    ToggleSplit,
    /// Show or hide the outline in a Markdown preview or PDF.
    ToggleOutline,
    /// Wrap long lines to the editor width, or scroll them horizontally.
    ToggleSoftWrap,
    /// Show or hide the sidebar.
    ToggleSidebar,
    /// Create a new, empty document.
    NewFile,
    /// Rename the active document on disk.
    RenameFile,
    /// Delete the active document from disk.
    DeleteFile,
    /// Show the active document in the file manager.
    RevealFile,
    /// Refresh the file tree from disk.
    RefreshTree,
}

/// A command and how it is presented.
#[derive(Debug, Clone, Copy)]
pub struct CommandSpec {
    /// The command itself.
    pub command: Command,
    /// The label shown in the palette.
    pub label: &'static str,
    /// The group the command is listed under.
    pub group: &'static str,
    /// The shortcut, as it is written in menus.
    pub shortcut: Option<&'static str>,
}

/// Every command, in the order the palette lists them.
pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        command: Command::Palette,
        label: "Show all commands",
        group: "General",
        shortcut: Some("Ctrl+Shift+P"),
    },
    CommandSpec {
        command: Command::Save,
        label: "Save file",
        group: "File",
        shortcut: Some("Ctrl+S"),
    },
    CommandSpec {
        command: Command::SaveAll,
        label: "Save all files",
        group: "File",
        shortcut: Some("Ctrl+Shift+S"),
    },
    CommandSpec {
        command: Command::CloseTab,
        label: "Close file",
        group: "File",
        shortcut: Some("Ctrl+W"),
    },
    CommandSpec {
        command: Command::CloseAll,
        label: "Close all files",
        group: "File",
        shortcut: None,
    },
    CommandSpec {
        command: Command::Reload,
        label: "Reload from disk",
        group: "File",
        shortcut: Some("Ctrl+R"),
    },
    CommandSpec {
        command: Command::NewFile,
        label: "New file",
        group: "File",
        shortcut: Some("Ctrl+N"),
    },
    CommandSpec {
        command: Command::RenameFile,
        label: "Rename file",
        group: "File",
        shortcut: None,
    },
    CommandSpec {
        command: Command::DeleteFile,
        label: "Delete file",
        group: "File",
        shortcut: None,
    },
    CommandSpec {
        command: Command::RevealFile,
        label: "Reveal in file manager",
        group: "File",
        shortcut: None,
    },
    CommandSpec {
        command: Command::Find,
        label: "Find in file",
        group: "Edit",
        shortcut: Some("Ctrl+F"),
    },
    CommandSpec {
        command: Command::Replace,
        label: "Replace in file",
        group: "Edit",
        shortcut: Some("Ctrl+H"),
    },
    CommandSpec {
        command: Command::SearchWorkspace,
        label: "Search in workspace",
        group: "Edit",
        shortcut: Some("Ctrl+Shift+F"),
    },
    CommandSpec {
        command: Command::ShowProblems,
        label: "Show problems",
        group: "View",
        shortcut: None,
    },
    CommandSpec {
        command: Command::SyncForward,
        label: "SyncTeX: show source line in PDF",
        group: "LaTeX",
        shortcut: None,
    },
    CommandSpec {
        command: Command::Sync,
        label: "SyncTeX: jump between source and PDF",
        group: "LaTeX",
        shortcut: Some("Ctrl+Shift+J"),
    },
    CommandSpec {
        command: Command::SyncInverse,
        label: "SyncTeX: jump PDF back to source",
        group: "LaTeX",
        shortcut: None,
    },
    CommandSpec {
        command: Command::PdfNextPage,
        label: "PDF: next page",
        group: "PDF",
        shortcut: Some("Ctrl+PageDown"),
    },
    CommandSpec {
        command: Command::PdfPreviousPage,
        label: "PDF: previous page",
        group: "PDF",
        shortcut: Some("Ctrl+PageUp"),
    },
    CommandSpec {
        command: Command::PdfZoomIn,
        label: "PDF: zoom in",
        group: "PDF",
        shortcut: Some("Ctrl+="),
    },
    CommandSpec {
        command: Command::PdfZoomOut,
        label: "PDF: zoom out",
        group: "PDF",
        shortcut: Some("Ctrl+-"),
    },
    CommandSpec {
        command: Command::PdfResetZoom,
        label: "PDF: actual size",
        group: "PDF",
        shortcut: Some("Ctrl+0"),
    },
    CommandSpec {
        command: Command::FocusOtherPane,
        label: "Focus the other pane",
        group: "View",
        shortcut: None,
    },
    CommandSpec {
        command: Command::LatexOutput,
        label: "LaTeX: output directory",
        group: "LaTeX",
        shortcut: None,
    },
    CommandSpec {
        command: Command::Preferences,
        label: "Preferences",
        group: "View",
        shortcut: None,
    },
    CommandSpec {
        command: Command::TerminalSettings,
        label: "Terminal settings",
        group: "Terminal",
        shortcut: None,
    },
    CommandSpec {
        command: Command::FindInTerminal,
        label: "Find in terminal",
        group: "Navigation",
        shortcut: None,
    },
    CommandSpec {
        command: Command::RemoteOpen,
        label: "SSH: open remote file",
        group: "File",
        shortcut: None,
    },
    CommandSpec {
        command: Command::SshConnect,
        label: "SSH: connect",
        group: "Terminal",
        shortcut: None,
    },
    CommandSpec {
        command: Command::ValidateDocument,
        label: "Validate JSON / TOML / YAML / XML",
        group: "File",
        shortcut: None,
    },
    CommandSpec {
        command: Command::FormatStructuredDocument,
        label: "Format JSON or XML",
        group: "File",
        shortcut: None,
    },
    CommandSpec {
        command: Command::ToggleImageSource,
        label: "Toggle SVG source / preview",
        group: "View",
        shortcut: None,
    },
    CommandSpec {
        command: Command::CycleLatexEngine,
        label: "LaTeX: cycle build engine",
        group: "LaTeX",
        shortcut: None,
    },
    CommandSpec {
        command: Command::BuildLatex,
        label: "Build LaTeX document",
        group: "LaTeX",
        shortcut: Some("Ctrl+Shift+B"),
    },
    CommandSpec {
        command: Command::ShowLatexPdf,
        label: "LaTeX: preview PDF beside source",
        group: "LaTeX",
        shortcut: None,
    },
    CommandSpec {
        command: Command::OpenTerminal,
        label: "Open terminal",
        group: "Terminal",
        shortcut: Some("Ctrl+`"),
    },
    CommandSpec {
        command: Command::ShowDiff,
        label: "Git: diff current file",
        group: "Git",
        shortcut: Some("Ctrl+G"),
    },
    CommandSpec {
        command: Command::GoToDefinition,
        label: "Go to definition",
        group: "Navigation",
        shortcut: Some("F12"),
    },
    CommandSpec {
        command: Command::ShowCodeActions,
        label: "Show code actions",
        group: "Navigation",
        shortcut: Some("Ctrl+."),
    },
    CommandSpec {
        command: Command::RenameSymbol,
        label: "Rename symbol",
        group: "Navigation",
        shortcut: Some("F2"),
    },
    CommandSpec {
        command: Command::FindReferences,
        label: "Find references",
        group: "Navigation",
        shortcut: Some("Shift+F12"),
    },
    CommandSpec {
        command: Command::ShowHover,
        label: "Show hover information",
        group: "Navigation",
        shortcut: Some("Ctrl+K"),
    },
    CommandSpec {
        command: Command::FormatDocument,
        label: "Format document",
        group: "Edit",
        shortcut: Some("Ctrl+Shift+I"),
    },
    CommandSpec {
        command: Command::ToggleTheme,
        label: "Cycle theme: system, light, dark",
        group: "View",
        shortcut: None,
    },
    CommandSpec {
        command: Command::ToggleSplit,
        label: "Toggle split editor",
        group: "View",
        shortcut: Some("Ctrl+\\"),
    },
    CommandSpec {
        command: Command::ToggleOutline,
        label: "Toggle document outline",
        group: "View",
        shortcut: Some("Ctrl+Shift+O"),
    },
    CommandSpec {
        command: Command::ToggleSoftWrap,
        label: "Toggle soft wrap",
        group: "View",
        shortcut: None,
    },
    CommandSpec {
        command: Command::ToggleSidebar,
        label: "Toggle sidebar",
        group: "View",
        shortcut: Some("Ctrl+B"),
    },
    CommandSpec {
        command: Command::RefreshTree,
        label: "Refresh file tree",
        group: "View",
        shortcut: None,
    },
];

/// The group a command is listed under.
pub fn command_group(command: Command) -> &'static str {
    COMMANDS
        .iter()
        .find(|spec| spec.command == command)
        .map(|spec| spec.group)
        .unwrap_or("General")
}

impl Command {
    /// The label shown in the palette.
    pub fn label(self) -> &'static str {
        COMMANDS
            .iter()
            .find(|spec| spec.command == self)
            .map(|spec| spec.label)
            .unwrap_or("Unknown command")
    }

    /// The shortcut, as it is written in menus, when the command has one.
    pub fn shortcut(self) -> Option<&'static str> {
        COMMANDS
            .iter()
            .find(|spec| spec.command == self)
            .and_then(|spec| spec.shortcut)
    }
}

/// The recent lists the palette offers.
#[derive(Debug, Clone, Copy, Default)]
pub struct Recents<'a> {
    /// Recently opened files.
    pub files: &'a [std::path::PathBuf],
    /// Recently opened folders.
    pub projects: &'a [std::path::PathBuf],
}

/// One row of the palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// An editor command.
    Command(Command),
    /// A task the project offers, by index into the workspace's task list.
    Task(usize, String),
    /// A file from the recent list, by index into the settings.
    RecentFile(usize, String),
    /// A folder from the recent list, by index into the settings.
    RecentProject(usize, String),
}

impl Entry {
    /// The label shown and matched against.
    pub fn label(&self) -> String {
        match self {
            Entry::Command(command) => command.label().to_string(),
            Entry::Task(_, label) => format!("Run: {label}"),
            Entry::RecentFile(_, path) => format!("Recent file: {path}"),
            Entry::RecentProject(_, path) => format!("Recent project: {path}"),
        }
    }

    /// The group it is listed under.
    pub fn group(&self) -> &'static str {
        match self {
            Entry::Command(command) => command_group(*command),
            Entry::Task(..) => "Tasks",
            Entry::RecentFile(..) => "Recent files",
            Entry::RecentProject(..) => "Recent projects",
        }
    }
}

/// The state of the palette.
#[derive(Debug, Default)]
pub struct PaletteState {
    /// Whether the palette is open.
    pub open: bool,
    /// The text typed into it.
    pub query: rune_edit::TextEdit,
    /// Everything the palette can do: the commands, then the project's tasks.
    pub entries: Vec<Entry>,
    /// Indices into `entries` that match the query.
    pub matches: Vec<usize>,
    /// The index into `matches` of the highlighted row.
    pub active: usize,
}

impl PaletteState {
    /// Open the palette with an empty query and the project's tasks.
    ///
    /// The field takes focus so that typing filters the list; the arrow keys
    /// are intercepted by the field's key handler before it sees them.
    pub fn open(&mut self, tasks: &[String], recents: &Recents<'_>) {
        self.open = true;
        self.query = rune_edit::TextEdit::new("");
        self.query.focused = true;
        self.active = 0;
        self.entries =
            COMMANDS
                .iter()
                .map(|spec| Entry::Command(spec.command))
                .chain(
                    tasks
                        .iter()
                        .enumerate()
                        .map(|(index, label)| Entry::Task(index, label.clone())),
                )
                .chain(recents.files.iter().enumerate().map(|(index, path)| {
                    Entry::RecentFile(index, path.to_string_lossy().into_owned())
                }))
                .chain(recents.projects.iter().enumerate().map(|(index, path)| {
                    Entry::RecentProject(index, path.to_string_lossy().into_owned())
                }))
                .collect();
        self.refresh();
    }

    /// Close the palette.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Recompute the matches for the current query.
    pub fn refresh(&mut self) {
        self.matches = filter_entries(&self.entries, &self.query.text);
        if self.active >= self.matches.len() {
            self.active = 0;
        }
    }

    /// The highlighted entry.
    pub fn selected_entry(&self) -> Option<Entry> {
        let index = *self.matches.get(self.active)?;
        self.entries.get(index).cloned()
    }

    /// The labels of the matching entries, for the view.
    pub fn visible(&self) -> Vec<&Entry> {
        self.matches
            .iter()
            .filter_map(|index| self.entries.get(*index))
            .collect()
    }

    /// Move the highlight down.
    pub fn down(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.active = (self.active + 1) % self.matches.len();
    }

    /// Move the highlight up.
    pub fn up(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.active = if self.active == 0 {
            self.matches.len() - 1
        } else {
            self.active - 1
        };
    }

    /// The highlighted command, when the highlight is on a command.
    pub fn selected(&self) -> Option<Command> {
        match self.selected_entry()? {
            Entry::Command(command) => Some(command),
            Entry::Task(..) | Entry::RecentFile(..) | Entry::RecentProject(..) => None,
        }
    }
}

/// The entries that match `query`.
pub fn filter_entries(entries: &[Entry], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return (0..entries.len()).collect();
    }
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            let label = entry.label().to_lowercase();
            let group = entry.group().to_lowercase();
            subsequence(&label, &query) || subsequence(&group, &query)
        })
        .map(|(index, _)| index)
        .collect()
}

/// The commands that match `query`.
///
/// Matching is a case-insensitive subsequence test, so "sf" finds "Save file"
/// and "togth" finds "Toggle light / dark theme".
pub fn filter(query: &str) -> Vec<Command> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return COMMANDS.iter().map(|spec| spec.command).collect();
    }
    COMMANDS
        .iter()
        .filter(|spec| {
            subsequence(&spec.label.to_lowercase(), &query)
                || subsequence(&spec.group.to_lowercase(), &query)
        })
        .map(|spec| spec.command)
        .collect()
}

/// Whether `needle` appears in `haystack` in order, with gaps allowed.
fn subsequence(haystack: &str, needle: &str) -> bool {
    let mut chars = haystack.chars();
    for wanted in needle.chars() {
        if !chars.any(|candidate| candidate == wanted) {
            return false;
        }
    }
    true
}

/// Run a command.
pub fn run(command: Command, workspace: &mut Workspace) {
    match command {
        Command::Save => {
            let active = workspace.active;
            workspace.save_and_build(active);
        }
        Command::SaveAll => workspace.save_all(),
        Command::CloseTab => {
            let active = workspace.active;
            workspace.request_close(active);
        }
        Command::CloseAll => workspace.request_close_all(),
        Command::Reload => workspace.reload_active(),
        Command::Find => workspace.find.open(),
        Command::BuildLatex => workspace.build_latex(),
        Command::ShowLatexPdf => workspace.open_pdf_beside_source(),
        Command::CycleLatexEngine => workspace.cycle_latex_engine(),
        Command::ToggleImageSource => workspace.toggle_image_source(),
        Command::SshConnect => workspace.begin_ssh_connect(),
        Command::RemoteOpen => workspace.begin_remote_file(),
        Command::FindInTerminal => workspace.open_terminal_find(),
        Command::Preferences => workspace.begin_preferences(),
        Command::TerminalSettings => workspace.begin_terminal_settings(),
        Command::LatexOutput => workspace.begin_latex_output(),
        Command::ValidateDocument => workspace.validate_document(),
        Command::FormatStructuredDocument => workspace.format_structured_document(),
        Command::SyncForward => workspace.sync_forward(),
        Command::Sync => workspace.sync_source_and_pdf(),
        Command::SyncInverse => workspace.sync_inverse(),
        Command::PdfNextPage => {
            workspace.pdf_next_page();
        }
        Command::PdfPreviousPage => {
            workspace.pdf_previous_page();
        }
        Command::PdfZoomIn => {
            workspace.pdf_zoom_in();
        }
        Command::PdfZoomOut => {
            workspace.pdf_zoom_out();
        }
        Command::PdfResetZoom => {
            workspace.pdf_reset_zoom();
        }
        Command::FocusOtherPane => workspace.focus_other_pane(),
        Command::OpenTerminal => {
            if let Err(error) = workspace.open_terminal() {
                workspace.set_notice(error);
            }
        }
        Command::ShowDiff => workspace.open_diff_for_active(),
        Command::ShowProblems => {
            workspace.sidebar = crate::SidebarMode::Problems;
            workspace.settings.show_sidebar = true;
            workspace.settings.save();
        }
        Command::GoToDefinition => workspace.go_to_definition(),
        Command::ShowCodeActions => workspace.show_code_actions(),
        Command::RenameSymbol => workspace.begin_rename_symbol(),
        Command::FindReferences => workspace.find_references(),
        Command::ShowHover => workspace.show_hover(),
        Command::FormatDocument => workspace.format_document(),
        Command::SearchWorkspace => {
            workspace.sidebar = crate::SidebarMode::Search;
            workspace.settings.show_sidebar = true;
            workspace.settings.save();
            workspace.run_search();
        }
        Command::Replace => workspace.find.open_replace(),
        Command::Palette => {
            let tasks: Vec<String> = workspace
                .tasks
                .iter()
                .map(|task| task.label.clone())
                .collect();
            workspace.palette.open(
                &tasks,
                &Recents {
                    files: &workspace.settings.recent_files,
                    projects: &workspace.settings.recent_projects,
                },
            );
        }
        Command::ToggleTheme => workspace.toggle_theme(),
        Command::ToggleSplit => workspace.toggle_split(),
        Command::ToggleOutline => workspace.toggle_outline(),
        Command::ToggleSoftWrap => workspace.toggle_soft_wrap(),
        Command::ToggleSidebar => workspace.toggle_sidebar(),
        Command::NewFile => workspace.begin_new_file(),
        Command::RenameFile => workspace.begin_rename(),
        Command::DeleteFile => workspace.begin_delete(),
        Command::RevealFile => workspace.reveal_active(),
        Command::RefreshTree => workspace.refresh_tree(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_a_spec() {
        for spec in COMMANDS {
            assert!(!spec.label.is_empty());
            assert!(!spec.group.is_empty());
        }
    }

    #[test]
    fn an_empty_query_lists_everything() {
        assert_eq!(filter("").len(), COMMANDS.len());
    }

    #[test]
    fn matching_is_a_subsequence_of_the_label() {
        let matches = filter("save");
        assert!(matches.contains(&Command::Save));
        assert!(matches.contains(&Command::SaveAll));
        assert!(!matches.contains(&Command::ToggleTheme));
    }

    #[test]
    fn matching_skips_letters() {
        let matches = filter("sf");
        assert!(matches.contains(&Command::Save), "'s' and 'f' in Save file");
    }

    #[test]
    fn matching_ignores_case() {
        assert_eq!(filter("SAVE"), filter("save"));
    }

    #[test]
    fn navigation_wraps_around() {
        let mut palette = PaletteState::default();
        palette.open(&[], &Recents::default());
        let count = palette.matches.len();
        palette.up();
        assert_eq!(
            palette.active,
            count - 1,
            "up from the first wraps to the last"
        );
        palette.down();
        assert_eq!(palette.active, 0);
    }

    #[test]
    fn a_query_that_matches_nothing_selects_nothing() {
        let mut palette = PaletteState::default();
        palette.open(&[], &Recents::default());
        palette.query = rune_edit::TextEdit::new("zzzzzz");
        palette.refresh();
        assert!(palette.matches.is_empty());
        assert_eq!(palette.selected(), None);
    }
}
