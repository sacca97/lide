//! The workspace: the project, its files, and the documents that are open.
//!
//! The workspace is the state of the whole editor. It knows which folder is
//! open, which documents are open, which one is active, and it draws the
//! window: the file tree on the left, the tab strip at the top, the document
//! in the middle and the status bar at the bottom.

pub mod command;
mod view;
pub use view::view;
pub mod dialog;
pub mod csv_view;
pub mod diff;
pub mod find;
pub mod icons;
pub mod images;
pub mod keys;
pub mod notebook_view;
pub mod servers;
pub mod settings;
pub mod tasks;
pub mod theme;
pub mod tree;
pub mod watch;

use std::path::{Path, PathBuf};

use lide_documents::{detect_kind, DocumentKind};
use lide_editor::Editor;
use lide_pdf::view::PdfView;
use rune_edit::TextEdit;
use rune::Live;
use rune_kit::Kit;

use crate::command::PaletteState;
use crate::find::FindState;
use crate::servers::{Outcome, Servers};
use crate::settings::Settings;
use crate::tree::FileTree;

/// How long to wait between automatic completion requests, in seconds.
const COMPLETION_DEBOUNCE: f64 = 0.15;

/// How often the repository status is refreshed, in seconds.
const GIT_REFRESH_INTERVAL: f64 = 2.0;
use crate::watch::Watch;
use lide_search::{Query, Search};

/// The rail down the left edge and the gap around the rounded cards.
const SIDEBAR_RAIL_WIDTH: f32 = 42.0;
const CARD_GAP: f32 = 4.0;

/// The narrowest and widest the sidebar can be dragged.
const MIN_SIDEBAR_WIDTH: f32 = 180.0;
const MAX_SIDEBAR_WIDTH: f32 = 560.0;

/// Keep a requested sidebar width inside its limits.
fn clamp_sidebar_width(width: f32) -> f32 {
    width.clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH)
}

/// The share of the editor each pane keeps when the split is dragged.
const MIN_SPLIT_RATIO: f32 = 0.2;
const MAX_SPLIT_RATIO: f32 = 0.8;

/// Keep a requested split ratio inside its limits.
fn clamp_split_ratio(ratio: f32) -> f32 {
    ratio.clamp(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO)
}

/// How deep the file tree is scanned.
const MAX_DEPTH: usize = 6;

/// What the sidebar is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarMode {
    /// The file tree.
    Files,
    /// Workspace search.
    Search,
    /// Diagnostics from the language servers.
    Problems,
    /// Places the language server found: definitions and references.
    Places,
}

/// The terminal's search bar.
#[derive(Debug, Default)]
pub struct TerminalFind {
    /// Whether the bar is showing.
    pub open: bool,
    /// The text typed into it.
    pub query: TextEdit,
    /// The matching lines, counted from the top of the scrollback.
    pub matches: Vec<usize>,
    /// The index of the highlighted match.
    pub current: usize,
    /// What the bar says: `3 of 12`, `no matches`, or a prompt.
    pub message: String,
    /// The query `matches` belongs to, so that typing a new one searches
    /// again rather than stepping through the old results.
    searched: String,
}

impl TerminalFind {
    /// Close the bar and let the field go.
    pub fn close(&mut self) {
        self.open = false;
        self.query.focused = false;
    }
}

/// A dialog the workspace is showing.
#[derive(Debug)]
pub enum DialogState {
    /// Ask for a new file's name.
    NewFile {
        name: TextEdit,
        error: Option<String>,
    },
    /// Ask for the active document's new name.
    Rename {
        name: TextEdit,
        error: Option<String>,
    },
    /// Ask for a symbol's new name.
    RenameSymbol {
        name: TextEdit,
        error: Option<String>,
    },
    /// Set the shell and start directory for new terminals.
    TerminalSettings {
        shell: Box<TextEdit>,
        directory: Box<TextEdit>,
    },
    /// Set where LaTeX builds write.
    LatexOutput { directory: Box<TextEdit> },
    /// Confirm a workspace-wide replacement.
    ConfirmReplace { files: usize, matches: usize },
    /// Choose an SSH host.
    SshHosts {
        hosts: Vec<String>,
        name: Box<TextEdit>,
    },
    /// Open one file through an SSH agent.
    RemoteFile {
        hosts: Vec<String>,
        host: Box<TextEdit>,
        path: Box<TextEdit>,
    },
    /// Confirm deleting the active document.
    ConfirmDelete,
}

/// The state of the editor.
pub struct Workspace {
    /// The folder the workspace is rooted at.
    pub root: Option<PathBuf>,
    /// The open documents.
    pub documents: Vec<Document>,
    /// The index of the active document.
    pub active: usize,
    /// The document shown in the second pane when the editor is split.
    pub split_document: Option<usize>,
    /// Which pane a tab click opens into: 0 for the left, 1 for the right.
    pub focused_pane: usize,
    /// The file tree of the root folder.
    pub tree: FileTree,
    /// A message shown in the status bar, e.g. after an error.
    pub notice: Option<String>,
    /// The app's clock, in seconds, updated every frame.
    now: f64,
    /// The repository the project is in, when it is in one.
    pub repo: Option<lide_git::Repo>,
    /// The repository's status, refreshed periodically.
    pub git: Option<lide_git::Status>,
    /// When the status was last refreshed, on the app's clock.
    last_git_refresh: f64,
    /// The terminal area's rect from the previous layout, so that the PTY can
    /// be told its size.
    terminal_rect: std::cell::Cell<Option<[f32; 4]>>,
    /// The LaTeX build that is running, or the last one.
    pub build: Option<lide_latex::Build>,
    /// The tasks the project offers, discovered when a folder is opened.
    pub tasks: Vec<tasks::Task>,
    /// The editor's settings.
    pub settings: Settings,
    /// An appearance override for this run only, used by headless snapshots so
    /// that rendering a state does not rewrite the user's settings.
    pub theme_override: Option<bool>,
    /// The find and replace bar.
    pub find: FindState,
    /// The command palette.
    pub palette: PaletteState,
    /// The dialog that is open, if any.
    pub dialog: Option<DialogState>,
    /// The documents that changed on disk since they were opened.
    pub changed_on_disk: Vec<PathBuf>,
    /// The file-system watcher, when a folder is open.
    pub watch: Option<Watch>,
    /// The image cache the PDF viewer decodes pages into.
    pub images: Option<rune_image::ImageCache>,
    /// What the sidebar shows.
    pub sidebar: SidebarMode,
    /// The text in the search bar.
    pub search_query: TextEdit,
    /// The search that is running, or its results.
    pub search: Option<Search>,
    /// Whether the search pattern is a regular expression.
    pub search_regex: bool,
    /// Whether the search is case sensitive.
    pub search_case_sensitive: bool,
    /// The language servers, one per language in use.
    pub servers: Servers,
    /// What the language server says is under the cursor.
    pub hover: Option<String>,
    /// Where the language server says a symbol is used.
    pub references: Vec<(PathBuf, u64, usize)>,
    /// The completions the language server offered.
    pub completions: Vec<lide_lsp::CompletionItem>,
    /// The completion the list has highlighted.
    pub completion_selected: usize,
    /// Whether the completion list is showing.
    pub completion_open: bool,
    /// The caret and text length the last completion request was made for,
    /// so that one is not made for every frame.
    last_completion: Option<(usize, usize)>,
    /// When the last completion request was sent, on the app's clock.
    last_completion_at: f64,
    /// What the language server offers to do at the caret.
    pub code_actions: Vec<lide_lsp::client::CodeAction>,
    /// Whether the code action list is showing.
    pub code_actions_open: bool,
    /// Where each Markdown pane is scrolled to, as `(offset, max)`: two lives
    /// per pane, the source first and the preview second. They are `Live`
    /// because a scroll report cannot run the view, and the wheel handler
    /// reads them to mirror the other pane.
    pub markdown_scrolls: Vec<Live<[f32; 2]>>,
    /// The terminal's search bar.
    pub terminal_find: TerminalFind,
    /// What workspace search replaces matches with.
    pub search_replacement: TextEdit,
    /// A workspace replace running on a background thread.
    pub replace: Option<lide_search::Replace>,
    /// A `.tex` file was saved, so the next frame builds it.
    latex_build_after_save: bool,
    /// A remote document being opened off the UI thread.
    remote_open: Option<std::sync::mpsc::Receiver<Result<Editor, String>>>,
    /// The width the sidebar had when a resize drag began.
    sidebar_drag_start: Option<f32>,
    /// The split ratio when a divider drag began.
    split_drag_start: Option<f32>,
    /// The Markdown source ratio when its divider drag began.
    markdown_drag_start: Option<f32>,
    /// The split area's width from the previous layout, so a drag can turn
    /// pointer travel into a ratio.
    split_width: std::cell::Cell<f32>,
    /// The width of each Markdown source-and-preview area, per pane.
    markdown_widths: [std::cell::Cell<f32>; 2],
    /// The width of each pane's PDF scroll area, from the last layout.
    pdf_widths: [std::cell::Cell<f32>; 2],
    /// The vertical scroll of each editor pane, so a Cmd/Ctrl-click maps a
    /// point to a source line.
    editor_scrolls: Vec<Live<[f32; 2]>>,
    /// The wrap width in columns of each editor pane, from the last layout.
    pub(crate) wrap_columns: [std::cell::Cell<usize>; 2],
    /// Scrolls the app still has to apply: the scroll area's key, the offset,
    /// and how many frames to keep asking. A page that has only just been
    /// shown has no height yet, so the first ask may be clamped to nothing.
    pub scroll_requests: Vec<(String, f32, u8)>,
    /// When a key was last pressed, and where the caret of the focused
    /// editor was at the last poll: a caret that moves with Cmd/Ctrl held and
    /// no key pressed is a Cmd-click.
    pub(crate) last_key_at: f64,
    caret_seen: Option<(usize, usize)>,
    /// Whether Cmd (Ctrl off macOS) is held, for source-to-PDF clicks.
    pub primary_modifier: bool,
    /// The presses on the PDF pages: when, which pane and where.
    pdf_presses: Vec<(f64, u8, f32, f32)>,
    /// The pane that was double clicked on a PDF page.
    pdf_double_click: Option<u8>,
    /// The two pane rectangles from the previous layout, for tab drops.
    split_left_rect: std::cell::Cell<[f32; 4]>,
    split_right_rect: std::cell::Cell<[f32; 4]>,
    /// An outline override for this run only, used by headless snapshots so a
    /// scene does not rewrite the user's `show_outline` setting.
    pub outline_override: Option<bool>,
    /// Directories the user collapsed in the file tree.
    pub collapsed_paths: std::collections::HashSet<PathBuf>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            root: None,
            documents: Vec::new(),
            active: 0,
            split_document: None,
            focused_pane: 0,
            tree: FileTree::default(),
            notice: None,
            now: 0.0,
            repo: None,
            git: None,
            last_git_refresh: -100.0,
            terminal_rect: std::cell::Cell::new(None),
            build: None,
            tasks: Vec::new(),
            settings: Settings::load(),
            theme_override: None,
            find: FindState::default(),
            palette: PaletteState::default(),
            dialog: None,
            changed_on_disk: Vec::new(),
            watch: None,
            images: None,
            sidebar: SidebarMode::Files,
            search_query: TextEdit::new(""),
            search: None,
            search_regex: false,
            search_case_sensitive: false,
            servers: Servers::default(),
            hover: None,
            references: Vec::new(),
            completions: Vec::new(),
            completion_selected: 0,
            completion_open: false,
            last_completion: None,
            last_completion_at: -1.0,
            code_actions: Vec::new(),
            code_actions_open: false,
            markdown_scrolls: (0..4).map(|_| Live::new([0.0, 0.0])).collect(),
            terminal_find: TerminalFind::default(),
            search_replacement: TextEdit::new(""),
            replace: None,
            latex_build_after_save: false,
            remote_open: None,
            sidebar_drag_start: None,
            split_drag_start: None,
            markdown_drag_start: None,
            split_width: std::cell::Cell::new(0.0),
            markdown_widths: [std::cell::Cell::new(0.0), std::cell::Cell::new(0.0)],
            pdf_widths: [std::cell::Cell::new(0.0), std::cell::Cell::new(0.0)],
            editor_scrolls: (0..2).map(|_| Live::new([0.0, 0.0])).collect(),
            wrap_columns: [std::cell::Cell::new(0), std::cell::Cell::new(0)],
            scroll_requests: Vec::new(),
            primary_modifier: false,
            last_key_at: -10.0,
            caret_seen: None,
            pdf_presses: Vec::new(),
            pdf_double_click: None,
            split_left_rect: std::cell::Cell::new([0.0; 4]),
            split_right_rect: std::cell::Cell::new([0.0; 4]),
            outline_override: None,
            collapsed_paths: std::collections::HashSet::new(),
        }
    }
}

/// A document that is open in the workspace.
///
/// The variants are boxed where they are large: a terminal carries a PTY and
/// a grid, and letting that size leak into every `Document` would make every
/// tab pay for it.
pub enum Document {
    /// A text or source file.
    Text(Editor),
    /// A Markdown file: the source and its preview.
    Markdown {
        /// The source text.
        editor: Editor,
        /// The headings of the document, for the outline.
        outline: Vec<lide_markdown::OutlineEntry>,
    },
    /// A PDF file.
    Pdf(Box<PdfView>),
    /// A terminal.
    Terminal(Box<lide_terminal::TerminalView>),
    /// An image.
    Image(Box<images::ImageView>),
    /// A file's diff.
    Diff(Box<diff::DiffView>),
    /// A comma- or tab-separated table.
    Csv(Box<csv_view::CsvView>),
    /// A Jupyter notebook.
    Notebook(Box<notebook_view::NotebookView>),
}

impl Document {
    /// The kind of the document.
    pub fn kind(&self) -> DocumentKind {
        match self {
            Document::Text(_) => DocumentKind::Text,
            Document::Markdown { .. } => DocumentKind::Markdown,
            Document::Pdf(_) => DocumentKind::Pdf,
            Document::Terminal(_) => DocumentKind::Text,
            Document::Image(_) => DocumentKind::Image,
            Document::Diff(_) => DocumentKind::Text,
            Document::Csv(_) => DocumentKind::Text,
            Document::Notebook(_) => DocumentKind::Text,
        }
    }

    /// The title shown on the tab.
    pub fn title(&self) -> String {
        match self {
            Document::Text(editor) => editor.title(),
            Document::Markdown { editor, .. } => editor.title(),
            Document::Pdf(pdf) => pdf.title(),
            Document::Terminal(terminal) => terminal.title(),
            Document::Image(image) => image.title(),
            Document::Diff(diff) => diff.title(),
            Document::Csv(csv) => csv.title(),
            Document::Notebook(notebook) => notebook.title(),
        }
    }

    /// Whether the document has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        match self {
            Document::Text(editor) => editor.dirty(),
            Document::Markdown { editor, .. } => editor.dirty(),
            Document::Pdf(_) => false,
            Document::Terminal(_) => false,
            Document::Image(_) => false,
            Document::Diff(_) => false,
            Document::Csv(_) => false,
            Document::Notebook(_) => false,
        }
    }

    /// Where the document lives, for local and remote text documents.
    pub fn location(&self) -> Option<&lide_resource::Location> {
        match self {
            Document::Text(editor) => editor.location.as_ref(),
            Document::Markdown { editor, .. } => editor.location.as_ref(),
            _ => None,
        }
    }

    /// The path the document was opened from.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Document::Text(editor) => editor.local_path(),
            Document::Markdown { editor, .. } => editor.local_path(),
            Document::Pdf(pdf) => Some(pdf.path()),
            Document::Terminal(_) => None,
            Document::Image(image) => Some(image.path()),
            Document::Diff(_) => None,
            Document::Csv(csv) => Some(csv.path()),
            Document::Notebook(notebook) => Some(notebook.path()),
        }
    }

    /// The text of the document, if it has any.
    pub fn text(&self) -> Option<&str> {
        match self {
            Document::Text(editor) => Some(editor.text()),
            Document::Markdown { editor, .. } => Some(editor.text()),
            Document::Pdf(_) => None,
            Document::Terminal(_) => None,
            Document::Image(_) => None,
            Document::Diff(_) => None,
            Document::Csv(_) => None,
            Document::Notebook(_) => None,
        }
    }

    /// The editor of the document, if it is editable.
    pub fn editor(&self) -> Option<&Editor> {
        match self {
            Document::Text(editor) => Some(editor),
            Document::Markdown { editor, .. } => Some(editor),
            Document::Pdf(_) => None,
            Document::Terminal(_) => None,
            Document::Image(_) => None,
            Document::Diff(_) => None,
            Document::Csv(_) => None,
            Document::Notebook(_) => None,
        }
    }

    /// The editor of the document, mutably.
    pub fn editor_mut(&mut self) -> Option<&mut Editor> {
        match self {
            Document::Text(editor) => Some(editor),
            Document::Markdown { editor, .. } => Some(editor),
            Document::Pdf(_) => None,
            Document::Terminal(_) => None,
            Document::Image(_) => None,
            Document::Diff(_) => None,
            Document::Csv(_) => None,
            Document::Notebook(_) => None,
        }
    }
}

/// Pick the source line a clicked word is on, among those near the line SyncTeX
/// named, and the column of the word in it.
///
/// `words` are the folded words before, at and after the click. The longest
/// run of them that one line holds wins, then the nearest line.
fn refine_line(text: &str, line: u64, column: usize, words: &(String, String, String)) -> (u64, usize) {
    let (before, word, after) = words;
    if word.is_empty() {
        return (line, column);
    }
    let needles = [
        format!("{before}{word}{after}"),
        format!("{before}{word}"),
        format!("{word}{after}"),
        word.clone(),
    ];
    let first = line.saturating_sub(8).max(1);
    let folded: Vec<(u64, String, &str)> = text
        .lines()
        .enumerate()
        .map(|(i, l)| (i as u64 + 1, lide_pdf::view::fold_text(l), l))
        .filter(|(n, _, _)| *n >= first && *n <= line + 8)
        .collect();
    for needle in needles.iter().filter(|n| n.chars().count() >= 3) {
        let best = folded
            .iter()
            .filter(|(_, f, _)| f.contains(needle.as_str()))
            .min_by_key(|(n, _, _)| n.abs_diff(line));
        if let Some((n, _, raw)) = best {
            let column = raw.to_lowercase().find(word.as_str()).unwrap_or(0);
            return (*n, column);
        }
    }
    (line, column)
}

impl Workspace {
    /// Pick the source line a click on the PDF came from, among the lines near
    /// the one SyncTeX named.
    ///
    /// SyncTeX's edit query answers with a single node, and its boxes are
    /// narrower than the glyphs they cover, so a click on the last words of a
    /// line can come back as the next line down. The click itself says better:
    /// among the candidate lines, the one whose text holds the clicked word
    /// and whose own SyncTeX position on the clicked page is nearest the
    /// click's height wins. `y_top` is the click's height in points from the
    /// top of the page, `page` the zero-based page.
    fn pick_source_line(
        &self,
        synctex: &lide_latex::SyncTex,
        pdf: &Path,
        page: u32,
        y_top: f32,
        source: &lide_latex::SyncSource,
        words: Option<&(String, String, String)>,
    ) -> (u64, usize) {
        let fallback = (source.line, source.column as usize);
        let Some(text) = self.source_text(&source.path) else {
            return fallback;
        };
        let needles: Vec<String> = match words {
            Some((before, word, after)) => [
                format!("{before}{word}{after}"),
                format!("{before}{word}"),
                format!("{word}{after}"),
                word.clone(),
            ]
            .into_iter()
            .filter(|needle| needle.chars().count() >= 3)
            .collect(),
            None => Vec::new(),
        };

        // Candidate lines: every line holding the clicked phrase, wherever it
        // is in the file — SyncTeX's single answer can be a section away when
        // its boxes do not cover the glyphs that were clicked — capped to the
        // ones nearest its answer to bound the query. Without a clicked word,
        // just the lines near its answer.
        let folded: Vec<(u64, String)> = text
            .lines()
            .enumerate()
            .map(|(index, line)| (index as u64 + 1, lide_pdf::view::fold_text(line)))
            .collect();
        let near = |n: u64| {
            n >= source.line.saturating_sub(8).max(1) && n <= source.line + 8
        };
        let mut candidates: Vec<u64> = if needles.is_empty() {
            folded.iter().map(|(n, _)| *n).filter(|n| near(*n)).collect()
        } else {
            folded
                .iter()
                .filter(|(_, f)| needles.iter().any(|needle| f.contains(needle.as_str())))
                .map(|(n, _)| *n)
                .collect()
        };
        if candidates.is_empty() {
            candidates = folded.iter().map(|(n, _)| *n).filter(|n| near(*n)).collect();
        }
        candidates.sort_by_key(|n| n.abs_diff(source.line));
        candidates.truncate(64);
        if candidates.is_empty() {
            return fallback;
        }

        // Score each candidate by the height of its own SyncTeX position on
        // the clicked page; the nearest line to SyncTeX's answer breaks ties.
        let positions = synctex.forward_many(&source.path, &candidates, pdf).unwrap_or_default();
        let mut best: Option<(u64, f32, u64)> = None;
        for (n, position) in &positions {
            let distance = position
                .as_ref()
                .filter(|point| point.page == page + 1)
                .map(|point| (point.y - y_top).abs())
                .unwrap_or(f32::MAX);
            let line_distance = n.abs_diff(source.line);
            let better = match &best {
                None => true,
                Some((_, best_distance, best_line_distance)) => {
                    distance < *best_distance
                        || (distance == *best_distance && line_distance < *best_line_distance)
                }
            };
            if better {
                best = Some((*n, distance, line_distance));
            }
        }
        let Some((line, distance, _)) = best else {
            return fallback;
        };
        if distance == f32::MAX {
            // No candidate has a position on this page: the text alone decides.
            return match words {
                Some(words) => refine_line(&text, source.line, source.column as usize, words),
                None => fallback,
            };
        }
        let column = words
            .filter(|(_, word, _)| !word.is_empty())
            .and_then(|(_, word, _)| {
                text.lines()
                    .nth(line.saturating_sub(1) as usize)
                    .and_then(|raw| raw.to_lowercase().find(word.as_str()))
            })
            .unwrap_or(source.column as usize);
        (line, column)
    }
}

impl Workspace {
    /// An empty workspace.
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a folder as the workspace root.
    pub fn open_folder(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let path = path.as_ref();
        // Canonicalise so that the title shows the real folder name rather
        // than ".".
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.root = Some(path.clone());
        self.tree = FileTree::scan(&path, MAX_DEPTH);
        // A workspace visited before opens the way it was left. A new one
        // starts with every folder collapsed, so the first view is the
        // project's shape rather than a wall of files.
        self.collapsed_paths = match self.settings.collapsed_directories.get(&path) {
            Some(saved) => saved.iter().cloned().collect(),
            None => self.tree.directory_paths().into_iter().collect(),
        };
        self.watch = Watch::new(&path);
        self.repo = lide_git::Repo::discover(&path).ok();
        self.refresh_tasks();
        self.refresh_git(true);
        self.settings.remember_project(&path);
        self.settings.save();
        Ok(())
    }

    /// Whether the dark theme is in use.
    pub fn dark(&self) -> bool {
        self.theme_override.unwrap_or(self.settings.dark)
    }

    /// The folder the workspace is rooted at.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// The active document.
    pub fn active_document(&self) -> Option<&Document> {
        self.documents.get(self.active)
    }

    /// The active document, mutably.
    pub fn active_document_mut(&mut self) -> Option<&mut Document> {
        let index = self.active;
        self.documents.get_mut(index)
    }

    /// The editor of the active document, if it is a text document.
    pub fn active_editor_mut(&mut self) -> Option<&mut Editor> {
        self.active_document_mut()?.editor_mut()
    }

    /// The terminal of the active document, if it is a terminal.
    /// What copy takes from the active terminal: its selection, or the whole
    /// visible grid when there is none.
    pub fn terminal_text(&self) -> Option<String> {
        match self.active_document() {
            Some(Document::Terminal(terminal)) => Some(terminal.selected_text()),
            _ => None,
        }
    }

    pub fn active_terminal_mut(&mut self) -> Option<&mut lide_terminal::TerminalView> {
        match self.active_document_mut()? {
            Document::Terminal(terminal) => Some(terminal),
            _ => None,
        }
    }

    /// Open a terminal in the workspace root, or wherever the editor started.
    /// Ask which SSH host to open a terminal to.
    pub fn begin_ssh_connect(&mut self) {
        let hosts = lide_platform::ssh::hosts();
        let first = hosts.first().cloned().unwrap_or_default();
        self.dialog = Some(DialogState::SshHosts {
            hosts,
            name: Box::new(TextEdit::new(first)),
        });
    }

    /// Open the SSH terminal the dialog chose.
    pub fn confirm_ssh_connect(&mut self) {
        let Some(DialogState::SshHosts { name, .. }) = &self.dialog else {
            return;
        };
        let host = name.text.trim().to_string();
        if host.is_empty() {
            self.notice = Some("type an SSH host".to_string());
            return;
        }
        self.dialog = None;
        self.open_ssh_terminal(&host);
    }

    /// Open a terminal running `ssh host`, starting in the configured
    /// terminal directory.
    pub fn open_ssh_terminal(&mut self, host: &str) {
        let directory = self.terminal_start_directory();
        let size = self.terminal_size();
        match lide_terminal::TerminalView::open_ssh(host, &directory.to_string_lossy(), size) {
            Ok(terminal) => {
                self.documents.push(Document::Terminal(Box::new(terminal)));
                self.active = self.documents.len() - 1;
            }
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    /// Ask for a host and a remote file to open.
    pub fn begin_remote_file(&mut self) {
        let hosts = lide_platform::ssh::hosts();
        let first = hosts.first().cloned().unwrap_or_default();
        self.dialog = Some(DialogState::RemoteFile {
            hosts,
            host: Box::new(TextEdit::new(first)),
            path: Box::new(TextEdit::new("")),
        });
    }

    /// Connect and open the remote file named in the dialog.
    ///
    /// Connecting is network work, so it runs on a thread; the frame that
    /// follows adds the document. Only text-like files are opened: the PDF,
    /// image, table and notebook viewers still read local paths.
    pub fn confirm_remote_file(&mut self) {
        let Some(DialogState::RemoteFile { host, path, .. }) = &self.dialog else {
            return;
        };
        let host = host.text.trim().to_string();
        let requested = path.text.trim().to_string();
        if host.is_empty() {
            self.notice = Some("type an SSH host".to_string());
            return;
        }
        let path = PathBuf::from(&requested);
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            self.notice = Some("type an absolute file path".to_string());
            return;
        };
        if !path.is_absolute() {
            self.notice = Some("the remote path must be absolute".to_string());
            return;
        }
        let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
            self.notice = Some("the remote file needs an extension".to_string());
            return;
        };
        if matches!(
            extension.to_ascii_lowercase().as_str(),
            "pdf"
                | "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "webp"
                | "bmp"
                | "ico"
                | "avif"
                | "tiff"
                | "csv"
                | "tsv"
                | "ipynb"
        ) {
            self.notice = Some(
                "remote viewers (PDF, images, tables, notebooks) are not wired yet".to_string(),
            );
            return;
        }

        let root = parent.to_path_buf();
        let location =
            lide_resource::Location::remote(host.clone(), root.clone(), PathBuf::from(name));
        self.dialog = None;

        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = lide_remote::store::AgentStore::connect(host, &root)
                .map_err(|error| error.to_string())
                .and_then(|store| {
                    Editor::open_with(std::sync::Arc::new(store), location)
                        .map_err(|error| error.to_string())
                });
            let _ = sender.send(result);
        });
        self.remote_open = Some(receiver);
        self.notice = Some("connecting over SSH…".to_string());
    }

    /// Collect the remote open, if it has finished.
    fn poll_remote_open(&mut self) -> bool {
        let Some(receiver) = self.remote_open.as_ref() else {
            return false;
        };
        match receiver.try_recv() {
            Ok(Ok(editor)) => {
                self.remote_open = None;
                match editor
                    .location
                    .as_ref()
                    .and_then(lide_resource::Location::extension)
                {
                    Some(extension) if extension.eq_ignore_ascii_case("md") => {
                        let outline = lide_markdown::outline(editor.text());
                        self.documents.push(Document::Markdown { editor, outline });
                    }
                    _ => self.documents.push(Document::Text(editor)),
                }
                self.active = self.documents.len() - 1;
                self.notice = None;
                true
            }
            Ok(Err(error)) => {
                self.remote_open = None;
                self.notice = Some(format!("could not open the remote file: {error}"));
                true
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.remote_open = None;
                self.notice = Some("the remote open stopped unexpectedly".to_string());
                true
            }
        }
    }

    pub fn open_terminal(&mut self) -> Result<(), String> {
        let directory = self.terminal_start_directory();
        let shell = if self.settings.terminal_shell.trim().is_empty() {
            lide_terminal::default_shell()
        } else {
            self.settings.terminal_shell.trim().to_string()
        };
        let size = self.terminal_size();
        let terminal =
            lide_terminal::TerminalView::open(&shell, &directory.to_string_lossy(), size)
                .map_err(|error| error.to_string())?;
        self.documents.push(Document::Terminal(Box::new(terminal)));
        self.active = self.documents.len() - 1;
        Ok(())
    }

    /// The directory a new terminal starts in.
    ///
    /// Empty means the project root. `~/` expands to the home directory, and
    /// any other relative path is taken from the project root, which is what
    /// makes a per-project setting portable.
    fn terminal_start_directory(&self) -> PathBuf {
        let configured = self.settings.terminal_directory.trim();
        if configured.is_empty() {
            return self
                .root
                .clone()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        }
        let path = match configured.strip_prefix("~/") {
            Some(rest) => std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(rest))
                .unwrap_or_else(|| PathBuf::from(configured)),
            None => PathBuf::from(configured),
        };
        if path.is_absolute() {
            path
        } else {
            self.root
                .clone()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
                .join(path)
        }
    }

    /// Where a LaTeX build for `tex` writes, from the settings.
    ///
    /// Empty means beside the source; a relative path is taken from the
    /// source's directory, and `~/` from home.
    fn latex_output_directory(&self, tex: &Path) -> Option<PathBuf> {
        let configured = self.settings.latex_output_directory.trim();
        if configured.is_empty() {
            return None;
        }
        let path = match configured.strip_prefix("~/") {
            Some(rest) => std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(rest))
                .unwrap_or_else(|| PathBuf::from(configured)),
            None => PathBuf::from(configured),
        };
        Some(if path.is_absolute() {
            path
        } else {
            tex.parent().unwrap_or_else(|| Path::new(".")).join(path)
        })
    }

    /// Ask where LaTeX builds should write.
    pub fn begin_latex_output(&mut self) {
        self.dialog = Some(DialogState::LatexOutput {
            directory: Box::new(TextEdit::new(self.settings.latex_output_directory.clone())),
        });
    }

    /// Save the LaTeX output directory from the dialog.
    pub fn confirm_latex_output(&mut self) {
        let Some(DialogState::LatexOutput { directory }) = &self.dialog else {
            return;
        };
        self.settings.latex_output_directory = directory.text.trim().to_string();
        self.settings.save();
        self.dialog = None;
        self.notice = Some("LaTeX output directory saved".to_string());
    }

    /// Ask for the shell and start directory new terminals use.
    pub fn begin_terminal_settings(&mut self) {
        self.dialog = Some(DialogState::TerminalSettings {
            shell: Box::new(TextEdit::new(self.settings.terminal_shell.clone())),
            directory: Box::new(TextEdit::new(self.settings.terminal_directory.clone())),
        });
    }

    /// Save the terminal settings from the dialog.
    pub fn confirm_terminal_settings(&mut self) {
        let Some(DialogState::TerminalSettings { shell, directory }) = &self.dialog else {
            return;
        };
        self.settings.terminal_shell = shell.text.trim().to_string();
        self.settings.terminal_directory = directory.text.trim().to_string();
        self.settings.save();
        self.dialog = None;
        self.notice = Some("terminal settings saved; new terminals use them".to_string());
    }

    /// Open the terminal's search bar, if a terminal is in front.
    pub fn open_terminal_find(&mut self) {
        if self.active_terminal_mut().is_none() {
            self.notice = Some("open a terminal to search it".to_string());
            return;
        }
        self.terminal_find = TerminalFind::default();
        self.terminal_find.open = true;
        self.terminal_find.query.focused = true;
        self.terminal_find.message = "type to search".to_string();
    }

    /// Go to the next match in the terminal, searching again if the query
    /// changed.
    pub fn terminal_find_next(&mut self) {
        self.terminal_find_step(1);
    }

    /// Go to the previous match in the terminal.
    pub fn terminal_find_previous(&mut self) {
        self.terminal_find_step(-1);
    }

    fn terminal_find_step(&mut self, step: i32) {
        let query = self.terminal_find.query.text.clone();
        if query.is_empty() {
            self.terminal_find.message = "type to search".to_string();
            return;
        }

        if query != self.terminal_find.searched {
            let Some(terminal) = self.active_terminal_mut() else {
                return;
            };
            let matches = terminal.search(&query);
            self.terminal_find.matches = matches;
            self.terminal_find.searched = query;
            self.terminal_find.current = 0;
        } else if !self.terminal_find.matches.is_empty() {
            let len = self.terminal_find.matches.len() as i32;
            self.terminal_find.current =
                (self.terminal_find.current as i32 + step).rem_euclid(len) as usize;
            let line = self.terminal_find.matches[self.terminal_find.current];
            if let Some(terminal) = self.active_terminal_mut() {
                terminal.scroll_to_line(line);
            }
        }

        let len = self.terminal_find.matches.len();
        self.terminal_find.message = if len == 0 {
            "no matches".to_string()
        } else {
            format!("{} of {len}", self.terminal_find.current + 1)
        };
    }

    /// Send a key to the active terminal.
    pub fn write_terminal_key(&mut self, key: &lide_terminal::Key) {
        if let Some(terminal) = self.active_terminal_mut() {
            terminal.write_key(key);
        }
    }

    /// The terminal's size in cells, from the area it was last drawn in.
    fn terminal_size(&self) -> lide_terminal::Size {
        let advance = self.settings.font_size * 0.62;
        let line_height = self.settings.line_height();
        let rect = self.terminal_rect.get();
        let (width, height) = rect
            .map(|rect| (rect[2], rect[3]))
            .unwrap_or((900.0, 500.0));
        lide_terminal::Size {
            cols: ((width - 12.0) / advance).max(20.0) as usize,
            rows: ((height - 12.0) / line_height).max(5.0) as usize,
        }
    }

    /// Let the image viewer decode what it has been given.
    fn poll_images(&mut self) -> bool {
        let Some(cache) = self.images.as_ref() else {
            return false;
        };
        let mut changed = false;
        for document in &mut self.documents {
            if let Document::Image(image) = document {
                changed |= image.poll(cache);
            }
        }
        changed
    }

    /// Collect the build's output, and open the PDF when it succeeds.
    fn poll_build(&mut self) -> bool {
        let Some(build) = self.build.as_mut() else {
            return false;
        };
        let was_running = !build.finished;
        let changed = build.poll();

        if was_running && build.finished {
            if build.success {
                let pdf = build.options.pdf_path();
                let tex = build.options.tex_path();
                if pdf.exists() {
                    // The preview is reloaded before it is focused: the build
                    // wrote a new file, and an open view still holds the old
                    // pages and text.
                    self.reload_pdf_previews(&pdf);
                    self.show_pdf_beside(&tex, &pdf);
                }
            } else {
                let count = build.errors().len();
                self.notice = Some(if count == 0 {
                    "the build failed".to_string()
                } else {
                    format!("the build failed with {count} errors")
                });
                self.sidebar = SidebarMode::Problems;
                self.settings.show_sidebar = true;
            }
        }
        changed
    }

    /// Keep the terminals the size of the area they are drawn in.
    fn poll_terminals(&mut self) -> bool {
        let size = self.terminal_size();
        let mut changed = false;
        for document in &mut self.documents {
            if let Document::Terminal(terminal) = document {
                if terminal.emulator.size() != size {
                    terminal.resize(size);
                    changed = true;
                }
                changed |= terminal.poll();
            }
        }
        changed
    }

    /// Make a document active.
    ///
    /// When the editor is split, the document that was active moves into the
    /// other pane, so the two panes always show different documents.
    pub fn set_active(&mut self, index: usize) {
        if index >= self.documents.len() || index == self.active {
            return;
        }
        if self.split_document == Some(index) {
            self.split_document = Some(self.active);
        }
        self.active = index;
        // Programmatic navigation (the tree, search, the palette) shows the
        // document in the left pane; a tab click may aim at either pane and
        // goes through `select_tab` instead.
        self.focused_pane = 0;
    }

    /// Focus the left (0) or right (1) pane.
    pub fn focus_pane(&mut self, pane: usize) {
        self.focused_pane = if self.split_document.is_some() {
            pane.min(1)
        } else {
            0
        };
    }

    /// Focus the other pane, switching which one a tab click opens into.
    pub fn focus_other_pane(&mut self) {
        if self.split_document.is_some() {
            self.focused_pane = 1 - self.focused_pane.min(1);
        } else {
            self.notice = Some("split the editor to have two panes".to_string());
        }
    }

    /// The document in the focused pane.
    pub fn focused_index(&self) -> usize {
        if self.focused_pane == 1 {
            self.split_document.unwrap_or(self.active)
        } else {
            self.active
        }
    }

    /// The document in the focused pane, mutably.
    pub fn focused_document_mut(&mut self) -> Option<&mut Document> {
        let index = self.focused_index();
        self.documents.get_mut(index)
    }

    /// Click a tab: show that document in the focused pane.
    ///
    /// Without a split this is the ordinary active document. With one, a tab
    /// picked while the right pane is focused loads the right pane; picking
    /// the document that is already on the left swaps the two panes, since a
    /// document is shown once.
    pub fn select_tab(&mut self, index: usize) {
        if index >= self.documents.len() {
            return;
        }
        let Some(split) = self.split_document else {
            self.set_active(index);
            return;
        };
        if self.focused_pane == 0 {
            self.set_active(index);
            return;
        }
        if index == split {
            return;
        }
        if index == self.active {
            self.active = split;
            self.split_document = Some(index);
        } else {
            self.split_document = Some(index);
        }
    }

    /// Move a tab into the left or right pane after a drag.
    ///
    /// Dropping a tab on the pane that already shows it only focuses that
    /// pane. Dropping it on the other pane swaps the two documents, so the
    /// same document is never shown twice.
    pub fn drop_tab_to_pane(&mut self, index: usize, pane: usize) {
        if index >= self.documents.len() {
            return;
        }
        if self.split_document.is_none() {
            if pane == 1 && self.documents.len() > 1 {
                if index == self.active {
                    self.active = (index + 1) % self.documents.len();
                }
                self.split_document = Some(index);
                self.focused_pane = 1;
                self.settings.split = true;
                self.settings.save();
            } else {
                self.focused_pane = 0;
                self.select_tab(index);
            }
            return;
        }
        self.focused_pane = pane.min(1);
        self.select_tab(index);
    }

    /// Show a message in the status bar.
    pub fn set_notice(&mut self, notice: impl Into<String>) {
        self.notice = Some(notice.into());
    }

    /// Open a file, choosing the right kind of view for it.
    pub fn open_path(&mut self, path: impl AsRef<Path>) -> Result<(), String> {
        // `./a/../b` and `b` are the same file; keep one spelling so the tab,
        // the status bar and the "already open" check all agree.
        let path = clean_path(path.as_ref());
        let kind = detect_kind(&path);

        // Already open? Just focus it.
        if let Some(index) = self
            .documents
            .iter()
            .position(|document| document.path() == Some(path.as_path()))
        {
            self.set_active(index);
            return Ok(());
        }

        let document = match kind {
            DocumentKind::Pdf => {
                Document::Pdf(Box::new(PdfView::open(&path).map_err(|e| e.to_string())?))
            }
            DocumentKind::Markdown => {
                let editor = Editor::open(&path).map_err(|error| error.to_string())?;
                let outline = lide_markdown::outline(editor.text());
                Document::Markdown { editor, outline }
            }
            DocumentKind::Image => Document::Image(Box::new(images::ImageView::open(&path))),
            DocumentKind::Text if is_table(&path) => {
                Document::Csv(Box::new(csv_view::CsvView::open(&path)))
            }
            DocumentKind::Text if is_notebook(&path) => {
                Document::Notebook(Box::new(notebook_view::NotebookView::open(&path)))
            }
            _ => Document::Text(Editor::open(&path).map_err(|error| error.to_string())?),
        };

        self.documents.push(document);
        if let Some(watch) = self.watch.as_mut() {
            watch.watch_file(&path);
        }
        let previous = self.active;

        // Tell a language server about the file, if one handles it. A server
        // needs the *project* root, not the file's directory: rust-analyzer
        // will not start without the Cargo.toml above the file.
        let root = self.root.clone().unwrap_or_else(|| project_root(&path));
        let text = self
            .documents
            .last()
            .and_then(|document| document.text())
            .unwrap_or_default()
            .to_string();
        if !text.is_empty() || self.documents.last().and_then(|d| d.text()).is_some() {
            self.servers.open(&root, &path, &text);
        }
        let index = self.documents.len() - 1;
        self.settings.remember_file(&path);
        self.settings.save();
        self.set_active(index);
        if self.active != index {
            self.active = index;
        }
        // A workspace that was left split opens the new file on the left and
        // keeps the file that was there on the right.
        if self.settings.split && self.split_document.is_none() && self.documents.len() > 1 {
            self.split_document = Some(previous.min(self.documents.len() - 1));
        }
        Ok(())
    }

    /// Close a document, asking first when it has unsaved changes.
    pub fn request_close(&mut self, index: usize) {
        let Some(document) = self.documents.get(index) else {
            return;
        };
        if document.is_dirty() {
            // Keep the document; the save prompt is the dialog.
            self.dialog = Some(DialogState::ConfirmDelete);
            return;
        }
        self.close(index);
    }

    /// Close every document that has no unsaved changes.
    pub fn request_close_all(&mut self) {
        let index = 0;
        while index < self.documents.len() {
            if self.documents[index].is_dirty() {
                self.set_notice("some files have unsaved changes");
                return;
            }
            self.documents.remove(index);
        }
        self.active = 0;
        self.split_document = None;
        self.focused_pane = 0;
    }

    /// Close a document without asking.
    pub fn close(&mut self, index: usize) {
        if index >= self.documents.len() {
            return;
        }
        if let Some(path) = self.documents[index].path().map(|path| path.to_path_buf()) {
            self.servers.close(&path);
        }
        self.documents.remove(index);
        if self.documents.is_empty() {
            self.active = 0;
            self.split_document = None;
            self.focused_pane = 0;
            return;
        }
        if self.active >= self.documents.len() {
            self.active = self.documents.len() - 1;
        }
        match self.split_document {
            Some(open) if open == index => {
                self.split_document = None;
            }
            Some(open) if open > index => {
                self.split_document = Some(open - 1);
            }
            _ => {}
        }
        // A split cannot show the same document twice, and a workspace that is
        // meant to be split picks another file rather than quietly collapsing.
        if self.split_document == Some(self.active) {
            self.split_document = None;
        }
        if self.split_document.is_none() && self.settings.split && self.documents.len() > 1 {
            self.split_document = Some((self.active + 1) % self.documents.len());
        }
        if self.split_document.is_none() {
            self.focused_pane = 0;
        }
    }

    /// Save a document.
    pub fn save(&mut self, index: usize) {
        let Some(document) = self.documents.get_mut(index) else {
            return;
        };
        // There is nothing to write. Skipping the write also keeps a save with
        // no delta from scheduling another LaTeX build.
        if !document.is_dirty() {
            return;
        }
        let saved_path = document.path().map(|path| path.to_path_buf());
        let result = match document {
            Document::Text(editor) => editor.save(),
            Document::Markdown { editor, .. } => editor.save(),
            Document::Pdf(_)
            | Document::Terminal(_)
            | Document::Image(_)
            | Document::Diff(_)
            | Document::Csv(_)
            | Document::Notebook(_) => return,
        };
        match result {
            Ok(()) => {
                self.notice = None;
                if let Some(path) = saved_path {
                    // An image preview of the same file should show what was
                    // just saved, not what it looked like when it was opened.
                    self.refresh_image_previews(&path);
                    // LaTeX builds on save, and only on save: no timer and no
                    // watcher-driven rebuild.
                    if path
                        .extension()
                        .map(|extension| extension == "tex")
                        .unwrap_or(false)
                    {
                        self.latex_build_after_save = true;
                    }
                }
            }
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    /// Save a document the way an explicit Save does: a `.tex` file is built
    /// even when there was nothing to write.
    ///
    /// The shortcut is the save-and-typeset key, so pressing it after opening
    /// a document, or on one that already matches disk, still compiles. A
    /// save that failed leaves the document dirty and schedules nothing.
    pub fn save_and_build(&mut self, index: usize) {
        self.save(index);
        if self.latex_build_after_save {
            return;
        }
        let is_tex = self
            .documents
            .get(index)
            .and_then(|document| document.path())
            .and_then(Path::extension)
            .is_some_and(|extension| extension == "tex");
        let clean = self
            .documents
            .get(index)
            .is_some_and(|document| !document.is_dirty());
        if is_tex && clean {
            self.latex_build_after_save = true;
        }
    }

    /// Reopen every image view of `path`, so that it decodes the bytes on disk
    /// again. Used when the source of an SVG (or its file) changed.
    fn refresh_image_previews(&mut self, path: &Path) {
        for document in &mut self.documents {
            if let Document::Image(image) = document {
                if image.path() == path {
                    **image = images::ImageView::open(path);
                }
            }
        }
    }

    /// Reopen every PDF view of `path`, so a rebuilt document shows its new
    /// pages instead of the ones it had when it was opened.
    fn reload_pdf_previews(&mut self, path: &Path) {
        for document in &mut self.documents {
            if let Document::Pdf(pdf) = document {
                if pdf.path() == path {
                    let _ = pdf.reload();
                }
            }
        }
        self.changed_on_disk.retain(|changed| changed != path);
    }

    /// Show the source of an image, or the preview of the source.
    ///
    /// Only SVG has both: the other formats are binary, so opening their
    /// source as text would show noise. When the other view is already open,
    /// this focuses it rather than opening a second copy.
    pub fn toggle_image_source(&mut self) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };

        let path = match document {
            Document::Image(image) => image.path().to_path_buf(),
            _ => match document.path() {
                Some(path) if is_svg(path) => path.to_path_buf(),
                _ => {
                    self.notice = Some("the active document has no SVG source".to_string());
                    return;
                }
            },
        };

        let want_source = matches!(document, Document::Image(_));
        let existing = self.documents.iter().position(|document| {
            document.path() == Some(path.as_path())
                && if want_source {
                    matches!(document, Document::Text(_) | Document::Markdown { .. })
                } else {
                    matches!(document, Document::Image(_))
                }
        });
        if let Some(index) = existing {
            self.set_active(index);
            return;
        }

        if want_source {
            match Editor::open(&path) {
                Ok(editor) => {
                    self.documents.push(Document::Text(editor));
                    let index = self.documents.len() - 1;
                    if let Some(watch) = self.watch.as_mut() {
                        watch.watch_file(&path);
                    }
                    self.set_active(index);
                }
                Err(error) => self.notice = Some(error.to_string()),
            }
        } else {
            self.documents
                .push(Document::Image(Box::new(images::ImageView::open(&path))));
            let index = self.documents.len() - 1;
            self.set_active(index);
        }
    }

    /// Save every document.
    pub fn save_all(&mut self) {
        for index in 0..self.documents.len() {
            self.save(index);
        }
    }

    /// Reload the active document from disk.
    pub fn reload_active(&mut self) {
        let active = self.active;
        let Some(document) = self.documents.get_mut(active) else {
            return;
        };
        let reloaded_path = document.path().map(|path| path.to_path_buf());
        let result = match document {
            Document::Text(editor) => editor.reload(),
            Document::Markdown { editor, .. } => editor.reload(),
            Document::Pdf(_)
            | Document::Terminal(_)
            | Document::Image(_)
            | Document::Diff(_)
            | Document::Csv(_)
            | Document::Notebook(_) => return,
        };
        match result {
            Ok(()) => {
                self.notice = None;
                if let Some(path) = reloaded_path {
                    self.refresh_image_previews(&path);
                }
            }
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    /// Whether any document has unsaved changes.
    pub fn has_unsaved_changes(&self) -> bool {
        self.documents.iter().any(|document| document.is_dirty())
    }

    // --- View state ------------------------------------------------------

    /// Switch between the light and the dark theme.
    pub fn toggle_theme(&mut self) {
        self.settings.dark = !self.settings.dark;
        self.settings.save();
    }

    /// Split the editor in two, or close the split.
    pub fn toggle_split(&mut self) {
        if self.split_document.is_some() {
            self.split_document = None;
            self.focused_pane = 0;
            self.settings.split = false;
        } else if self.documents.len() > 1 {
            self.split_document = Some((self.active + 1) % self.documents.len());
            self.focused_pane = 0;
            self.settings.split = true;
        } else {
            self.notice = Some("open a second file to split the editor".to_string());
            return;
        }
        self.settings.save();
    }

    /// Collapse or expand a directory in the file tree.
    pub fn toggle_directory(&mut self, path: &Path) {
        if !self.collapsed_paths.remove(path) {
            self.collapsed_paths.insert(path.to_path_buf());
        }
        self.remember_tree_state();
    }

    /// Save the collapsed directories for this workspace.
    fn remember_tree_state(&mut self) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let mut paths: Vec<PathBuf> = self.collapsed_paths.iter().cloned().collect();
        paths.sort();
        self.settings.collapsed_directories.insert(root, paths);
        self.settings.save();
    }

    /// Whether a directory is expanded.
    pub fn directory_expanded(&self, path: &Path) -> bool {
        !self.collapsed_paths.contains(path)
    }

    /// Show or hide the sidebar.
    pub fn toggle_sidebar(&mut self) {
        self.settings.show_sidebar = !self.settings.show_sidebar;
        self.settings.save();
    }

    /// Whether Markdown previews and PDFs show their outline.
    pub fn show_outline(&self) -> bool {
        self.outline_override.unwrap_or(self.settings.show_outline)
    }

    /// Show or hide the outline in Markdown previews and PDFs.
    pub fn toggle_outline(&mut self) {
        self.settings.show_outline = !self.show_outline();
        self.outline_override = None;
        self.settings.save();
    }

    /// Remember whether the primary modifier is held.
    ///
    /// The app hears every change of the held modifiers, so a click with
    /// Cmd (Ctrl off macOS) can be told from a plain one.
    pub fn set_primary_modifier(&mut self, held: bool) {
        self.primary_modifier = held;
    }

    /// Note a press on a PDF page, in PDF points.
    ///
    /// Two presses close together on the same page are a double click. The
    /// editor's own double press is stricter, and a hand rarely holds the
    /// pointer still enough for it.
    pub fn note_pdf_press(&mut self, pane: usize, x: f32, y: f32) {
        let now = self.now;
        let pane = pane as u8;
        self.pdf_presses
            .retain(|(at, other, _, _)| now - *at < 0.8 && *other == pane);
        if let Some((at, _, before_x, before_y)) = self.pdf_presses.last().copied() {
            if now - at < 0.45
                && (before_x - x).abs() < 14.0
                && (before_y - y).abs() < 14.0
            {
                self.pdf_double_click = Some(pane);
            }
        }
        self.pdf_presses.push((now, pane, x, y));
        if self.pdf_presses.len() > 8 {
            self.pdf_presses.remove(0);
        }
    }

    /// Wrap long lines, or scroll them horizontally.
    pub fn toggle_soft_wrap(&mut self) {
        self.settings.soft_wrap = !self.settings.soft_wrap;
        self.settings.save();
    }

    /// Jump between the LaTeX source and its PDF.
    ///
    /// A `.tex` source goes to the line in the PDF; a PDF goes back to the
    /// source at the point last under the pointer.
    pub fn sync_source_and_pdf(&mut self) {
        match self.documents.get(self.focused_index()) {
            Some(Document::Pdf(_)) => self.sync_inverse(),
            Some(document)
                if document
                    .path()
                    .and_then(|path| path.extension())
                    .is_some_and(|extension| extension == "tex") =>
            {
                self.sync_forward();
            }
            _ => {
                self.notice = Some("open a .tex source or its PDF to sync".to_string());
            }
        }
    }

    /// Show the current source line where it lands on the PDF.
    pub fn sync_forward(&mut self) {
        let Some(tex) = self.latex_document() else {
            self.notice = Some("no .tex file to sync".to_string());
            return;
        };
        let line = self
            .documents
            .get(self.focused_index())
            .and_then(|document| document.editor())
            .map(|editor| editor.cursor_line() as u64)
            .unwrap_or(1);
        let output = self.latex_output_directory(&tex);
        let pdf = lide_latex::texres_in(&tex, self.latex_engine(), output.as_deref()).pdf_path();
        if !pdf.exists() {
            self.notice = Some("build the document first".to_string());
            return;
        }

        // The cursor may be in an included chapter, not in the main file.
        let source = self
            .documents
            .get(self.focused_index())
            .and_then(|document| document.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "tex"))
            .map(Path::to_path_buf)
            .unwrap_or_else(|| tex.clone());

        // The build directory is where the source lives: the sidecar of a
        // PDF that was moved stays there.
        let build_directory = tex
            .parent()
            .or_else(|| pdf.parent())
            .unwrap_or_else(|| std::path::Path::new("."));
        let phrases = self.source_phrases(&source, line);
        let synctex = lide_latex::SyncTex::new(build_directory);
        match synctex.forward(&source, line, 0, &pdf) {
            Ok(Some(point)) => {
                self.show_pdf_beside(&source, &pdf);
                let pdf_index = self.split_document.unwrap_or(self.active);
                if let Some(Document::Pdf(view)) = self.documents.get_mut(pdf_index) {
                    let page = point.page.saturating_sub(1);
                    let height = view.page_height(page).unwrap_or(792.0);
                    view.go_to_page(page);                    // SyncTeX's y is measured from the top; the viewer's
                    // pointer and text layer use the PDF's bottom-left origin.
                    view.pointer = Some((point.x, (height - point.y).max(0.0)));
                    // SyncTeX is exact to the line of the page; the words of the
                    // source line find the place within it, once the page's
                    // text is there.
                    view.set_sync_goal(phrases.clone(), point.y);
                    let offset = view.scroll_to_height(point.y);
                    let pane = if self.split_document == Some(pdf_index) { 1 } else { 0 };
                    self.request_scroll(format!("pdf-scroll-{pane}"), offset);
                }
            }
            Ok(None) => self.notice = Some(format!("SyncTeX knows nothing about line {line}")),
            Err(error) => self.notice = Some(error),
        }
    }

    /// The words to look for on the PDF page for a source line, longest
    /// phrase first, starting at the caret when it is on that line.
    fn source_phrases(&self, path: &Path, line: u64) -> Vec<String> {
        let Some(text) = self.source_text(path) else {
            return Vec::new();
        };
        let Some(line_text) = text.lines().nth(line.saturating_sub(1) as usize) else {
            return Vec::new();
        };
        let mut start = 0usize;
        if let Some(editor) = self.documents.get(self.focused_index()).and_then(|d| d.editor()) {
            if editor.cursor_line() as u64 == line {
                let column = editor.cursor_column().saturating_sub(1);
                start = line_text
                    .char_indices()
                    .nth(column)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
            }
        }
        let mut words = lide_latex::plain_words(&line_text[start..]);
        if words.is_empty() && start > 0 {
            words = lide_latex::plain_words(line_text);
        }
        let mut phrases = Vec::new();
        for count in [5usize, 3, 2] {
            if words.len() >= count {
                phrases.push(words[..count].join(" "));
            }
        }
        if let Some(longest) = words.iter().filter(|w| w.chars().count() >= 4).max_by_key(|w| w.len()) {
            phrases.push(longest.clone());
        }
        phrases
    }

    /// The text of a source file: the open buffer, or the file on disk.
    fn source_text(&self, path: &Path) -> Option<String> {
        self.documents
            .iter()
            .find(|document| document.path() == Some(path))
            .and_then(|document| document.text().map(str::to_string))
            .or_else(|| std::fs::read_to_string(path).ok())
    }

    /// Ask the app to scroll the area with this key to `offset`.
    fn request_scroll(&mut self, key: String, offset: f32) {
        self.scroll_requests.retain(|(other, _, _)| *other != key);
        self.scroll_requests.push((key, offset, 12));
    }

    /// Scroll the left editor pane so that `line` is in view.
    fn reveal_source_line(&mut self, line: u64) {
        let line_height = self.settings.line_height().max(1.0);
        let row = if self.settings.soft_wrap {
            let columns = self.wrap_columns[0].get();
            let text = self.active_document().and_then(|d| d.text()).unwrap_or_default();
            view::editor::visual_row_of(text, line, columns)
        } else {
            line.saturating_sub(1) as usize
        };
        let offset = (row as f32 * line_height - 4.0 * line_height).max(0.0);
        self.request_scroll("editor-scroll-0".to_string(), offset);
    }

    /// Show the source of the place the pointer was last at on the PDF.
    pub fn sync_inverse(&mut self) {
        let words;
        let (pdf, page, x, y) = match self.documents.get(self.focused_index()) {
            Some(Document::Pdf(view)) => {
                let page = view.page();
                let height = view.page_height(page).unwrap_or(792.0);
                let (x, bottom) = view.pointer.unwrap_or((306.0, height / 2.0));
                words = view.words_around(x, bottom);
                // The pointer is in bottom-left PDF points; SyncTeX's y is
                // measured from the top of the page.
                (
                    view.path().to_path_buf(),
                    page,
                    x,
                    (height - bottom).max(0.0),
                )
            }
            _ => {
                self.notice = Some("open a PDF to sync from".to_string());
                return;
            }
        };

        let synctex = lide_latex::SyncTex::new(pdf.parent().unwrap_or(std::path::Path::new(".")));
        match synctex.inverse(&pdf, page + 1, x, y) {
            Ok(Some(source)) => {
                // The sidecar may spell the source through a symlink (macOS's
                // /var for /private/var); if the file is already open under the
                // workspace's spelling, use that, so the open tab is focused
                // instead of a second copy opening.
                let source = lide_latex::SyncSource {
                    path: self
                        .documents
                        .iter()
                        .find_map(|document| {
                            document
                                .path()
                                .filter(|path| fs_key(path) == fs_key(&source.path))
                                .map(Path::to_path_buf)
                        })
                        .unwrap_or(source.path),
                    ..source
                };
                self.show_pdf_beside(&source.path, &pdf);
                // SyncTeX names one line of the page's text; the clicked word
                // and the clicked row say which of the nearby lines it was.
                let (line, column) =
                    self.pick_source_line(&synctex, &pdf, page, y, &source, words.as_ref());
                if let Some(editor) = self.active_editor_mut() {
                    editor.go_to(line, column);
                }
                self.reveal_source_line(line);
            }
            Ok(None) => self.notice = Some("SyncTeX knows nothing about that place".to_string()),
            Err(error) => self.notice = Some(error),
        }
    }

    /// Go to the next page of the PDF in the focused pane.
    pub fn pdf_next_page(&mut self) -> bool {
        match self.focused_document_mut() {
            Some(Document::Pdf(pdf)) => {
                pdf.next_page();
                true
            }
            _ => false,
        }
    }

    /// Go to the previous page of the PDF in the focused pane.
    pub fn pdf_previous_page(&mut self) -> bool {
        match self.focused_document_mut() {
            Some(Document::Pdf(pdf)) => {
                pdf.previous_page();
                true
            }
            _ => false,
        }
    }

    /// Zoom the PDF in the focused pane in.
    pub fn pdf_zoom_in(&mut self) -> bool {
        match self.focused_document_mut() {
            Some(Document::Pdf(pdf)) => {
                pdf.zoom_in();
                true
            }
            _ => false,
        }
    }

    /// Zoom the PDF in the focused pane out.
    pub fn pdf_zoom_out(&mut self) -> bool {
        match self.focused_document_mut() {
            Some(Document::Pdf(pdf)) => {
                pdf.zoom_out();
                true
            }
            _ => false,
        }
    }

    /// Return the PDF in the focused pane to actual size.
    pub fn pdf_reset_zoom(&mut self) -> bool {
        match self.focused_document_mut() {
            Some(Document::Pdf(pdf)) => {
                pdf.reset_zoom();
                true
            }
            _ => false,
        }
    }

    /// Build the LaTeX document the editor is looking at.
    ///
    /// The document is the active `.tex` file if there is one, else the only
    /// `.tex` file in the project. When the build succeeds the PDF opens
    /// beside the source, so the preview follows the source without losing it.
    pub fn build_latex(&mut self) {
        if let Some(build) = self.build.as_mut() {
            if !build.finished {
                self.notice = Some("a build is already running".to_string());
                return;
            }
        }
        let Some(tex) = self.latex_document() else {
            self.notice = Some("no .tex file to build".to_string());
            return;
        };

        let output = self.latex_output_directory(&tex);
        let options = lide_latex::texres_in(&tex, self.latex_engine(), output.as_deref());
        match lide_latex::Build::start(options) {
            Ok(build) => {
                self.build = Some(build);
                self.notice = None;
            }
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    /// Open the built PDF beside the `.tex` source.
    ///
    /// The source stays in the left pane and the PDF opens in the right, so a
    /// build or save keeps both in view. Build the document first if the PDF
    /// does not exist yet.
    pub fn open_pdf_beside_source(&mut self) {
        let Some(tex) = self.latex_document() else {
            self.notice = Some("no .tex file to preview".to_string());
            return;
        };
        let output = self.latex_output_directory(&tex);
        let pdf = lide_latex::texres_in(&tex, self.latex_engine(), output.as_deref()).pdf_path();
        if !pdf.exists() {
            self.notice = Some("build the document first".to_string());
            return;
        }
        let source = self
            .documents
            .get(self.focused_index())
            .and_then(|document| document.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "tex"))
            .map(Path::to_path_buf)
            .unwrap_or_else(|| tex.clone());
        self.show_pdf_beside(&source, &pdf);
    }

    /// Put `tex` in the left pane and `pdf` in the right.
    fn show_pdf_beside(&mut self, tex: &Path, pdf: &Path) {
        if let Err(error) = self.open_path(tex) {
            self.set_notice(error);
            return;
        }
        let tex_index = self.active;
        if let Err(error) = self.open_path(pdf) {
            self.set_notice(error);
            return;
        }
        let pdf_index = self.active;
        if pdf_index == tex_index || pdf_index >= self.documents.len() {
            return;
        }
        self.active = tex_index;
        self.split_document = Some(pdf_index);
        self.focused_pane = 0;
        if !self.settings.split {
            self.settings.split = true;
            self.settings.save();
        }
    }

    /// The main `.tex` file to build.
    ///
    /// An included chapter is not the build target. Prefer `main.tex`, then
    /// the active file when it is a root document, then the shallowest file
    /// that nothing else includes.
    fn latex_document(&self) -> Option<PathBuf> {
        let active = self
            .documents
            .get(self.focused_index())
            .and_then(|document| document.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "tex"))
            .map(Path::to_path_buf);
        let Some(root) = self.root.as_ref() else {
            return active;
        };

        let main = root.join("main.tex");
        if main.is_file() {
            return Some(main);
        }

        let mut found: Vec<PathBuf> = Vec::new();
        collect_tex(root, &mut found, 0);
        found.sort();
        let roots = tex_roots(&found);
        // The user opened the file and the tree walks the workspace, so the
        // two spellings of the same path can differ (a symlink, macOS's
        // canonical case). Same file means the same document.
        if let Some(active) = active
            .as_ref()
            .filter(|path| is_one_of_roots(path, &roots))
        {
            return Some(active.clone());
        }
        roots
            .iter()
            .find(|path| path.file_stem() == root.file_name())
            .cloned()
            .or_else(|| roots.first().cloned())
            .or(active)
            .or_else(|| found.into_iter().next())
    }

    /// Stop the running build.
    pub fn cancel_build(&mut self) {
        if let Some(build) = self.build.as_mut() {
            build.cancel();
        }
    }

    /// The LaTeX engine builds use, from the settings.
    pub fn latex_engine(&self) -> lide_latex::Engine {
        lide_latex::Engine::from_name(&self.settings.latex_engine).unwrap_or_default()
    }

    /// Move to the next LaTeX engine and remember it.
    pub fn cycle_latex_engine(&mut self) {
        let next = self.latex_engine().next();
        self.settings.latex_engine = next.name().to_string();
        self.settings.save();
        self.refresh_tasks();
        self.notice = Some(format!("LaTeX engine: {}", next.name()));
    }

    /// Re-discover the tasks the project offers.
    pub fn refresh_tasks(&mut self) {
        let engine = self.latex_engine();
        self.tasks = match &self.root {
            Some(root) => {
                let mut tasks = tasks::discover(root);
                if let Some(latex) = tasks::latex_task(root, engine) {
                    tasks.push(latex);
                }
                tasks
            }
            None => Vec::new(),
        };
    }

    /// Run the task at `index` in the build panel.
    pub fn run_task(&mut self, index: usize) {
        let Some(task) = self.tasks.get(index).cloned() else {
            return;
        };
        let Some(root) = self.root.clone() else {
            self.notice = Some("open a folder before running a task".to_string());
            return;
        };
        if let Some(build) = self.build.as_mut() {
            if !build.finished {
                self.notice = Some("a task is already running".to_string());
                return;
            }
        }
        match lide_latex::Build::start(task.options(&root)) {
            Ok(build) => {
                self.build = Some(build);
                self.notice = None;
            }
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    /// Re-read the repository status.
    ///
    /// `force` skips the interval; otherwise the status is refreshed at most
    /// every couple of seconds, because `git status` over a large tree is not
    /// something to run on every frame.
    pub fn refresh_git(&mut self, force: bool) {
        let Some(repo) = &self.repo else {
            self.git = None;
            return;
        };
        if !force && self.now - self.last_git_refresh < GIT_REFRESH_INTERVAL {
            return;
        }
        self.last_git_refresh = self.now;
        match repo.status() {
            Ok(status) => self.git = Some(status),
            Err(error) => {
                log::warn!("git status: {error}");
                self.git = None;
            }
        }
    }

    /// Open the diff of the file in the active document.
    pub fn open_diff_for_active(&mut self) {
        let Some(path) = self
            .active_document()
            .and_then(|document| document.path())
            .map(Path::to_path_buf)
        else {
            self.set_notice("open a file to diff".to_string());
            return;
        };
        let Some(repo) = &self.repo else {
            self.set_notice("this folder is not a git repository".to_string());
            return;
        };
        let Ok(relative) = path.strip_prefix(&repo.root) else {
            self.set_notice("the file is outside the repository".to_string());
            return;
        };
        self.open_diff(relative.to_path_buf());
    }

    /// Open the diff of a changed file.
    pub fn open_diff(&mut self, path: PathBuf) {
        let Some(repo) = &self.repo else {
            return;
        };
        let entry = self
            .git
            .as_ref()
            .and_then(|status| status.entries.iter().find(|entry| entry.path == path));
        let conflicted = entry
            .map(|entry| entry.index.is_unmerged() || entry.worktree.is_unmerged())
            .unwrap_or(false);
        let staged = entry.map(|entry| entry.staged()).unwrap_or(false) && !conflicted;

        // Already showing this diff? Focus it instead of opening another.
        if let Some(index) = self.documents.iter().position(|document| {
            matches!(
                document,
                Document::Diff(diff)
                    if diff.path == path && diff.staged == staged && diff.conflict == conflicted
            )
        }) {
            self.set_active(index);
            return;
        }

        let diff = if conflicted {
            diff::DiffView::open_conflict(&repo.root, &path)
        } else {
            diff::DiffView::open(&repo.root, &path, staged)
        };
        self.documents.push(Document::Diff(Box::new(diff)));
        self.active = self.documents.len() - 1;
    }

    /// Re-read the file tree from disk.
    pub fn refresh_tree(&mut self) {
        if let Some(root) = self.root.clone() {
            self.tree = FileTree::scan(&root, MAX_DEPTH);
        }
    }

    // --- File operations -------------------------------------------------

    /// Open the dialog for a new file.
    pub fn begin_new_file(&mut self) {
        let mut name = TextEdit::new("");
        name.focused = true;
        self.dialog = Some(DialogState::NewFile { name, error: None });
    }

    /// Open the dialog for renaming the active document.
    pub fn begin_rename(&mut self) {
        let Some(document) = self.active_document() else {
            return;
        };
        let Some(path) = document.path() else {
            return;
        };
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut edit = TextEdit::new(name);
        edit.focused = true;
        self.dialog = Some(DialogState::Rename {
            name: edit,
            error: None,
        });
    }

    /// Open the dialog for deleting the active document.
    pub fn begin_delete(&mut self) {
        if self.active_document().is_some() {
            self.dialog = Some(DialogState::ConfirmDelete);
        }
    }

    /// Show the active document in the system file manager.
    pub fn reveal_active(&mut self) {
        let Some(path) = self
            .active_document()
            .and_then(|d| d.path())
            .map(|p| p.to_path_buf())
        else {
            return;
        };
        let Some(directory) = path.parent() else {
            return;
        };
        // `xdg-open` on Linux, `open` on macOS; lide-platform picks one.
        if let Err(error) = lide_platform::reveal(directory) {
            self.notice = Some(format!("could not reveal the file: {error}"));
        }
    }

    /// Create the file named in the new-file dialog.
    fn create_file(&mut self, name: &str) {
        let Some(root) = self.root.clone() else {
            self.set_notice("open a folder before creating files");
            return;
        };
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let path = root.join(name);
        if path.exists() {
            self.dialog = Some(DialogState::NewFile {
                name: TextEdit::new(name),
                error: Some(format!("{name} already exists")),
            });
            return;
        }
        match std::fs::write(&path, "") {
            Ok(()) => {
                self.dialog = None;
                self.refresh_tree();
                if let Err(error) = self.open_path(&path) {
                    self.set_notice(error);
                }
            }
            Err(error) => {
                self.dialog = Some(DialogState::NewFile {
                    name: TextEdit::new(name),
                    error: Some(error.to_string()),
                });
            }
        }
    }

    /// Rename the active document.
    fn rename_active(&mut self, new_name: &str) {
        let Some(path) = self
            .active_document()
            .and_then(|d| d.path())
            .map(|p| p.to_path_buf())
        else {
            return;
        };
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return;
        }
        let Some(parent) = path.parent() else {
            return;
        };
        let target = parent.join(new_name);
        if target == path {
            self.dialog = None;
            return;
        }
        if target.exists() {
            self.dialog = Some(DialogState::Rename {
                name: TextEdit::new(new_name),
                error: Some(format!("{new_name} already exists")),
            });
            return;
        }
        match std::fs::rename(&path, &target) {
            Ok(()) => {
                self.dialog = None;
                if let Some(editor) = self.active_editor_mut() {
                    editor.location = Some(lide_resource::Location::local(target.clone()));
                }
                self.refresh_tree();
            }
            Err(error) => {
                self.dialog = Some(DialogState::Rename {
                    name: TextEdit::new(new_name),
                    error: Some(error.to_string()),
                });
            }
        }
    }

    /// Delete the active document from disk.
    fn delete_active(&mut self) {
        let Some(path) = self
            .active_document()
            .and_then(|d| d.path())
            .map(|p| p.to_path_buf())
        else {
            return;
        };
        match std::fs::remove_file(&path) {
            Ok(()) => {
                self.dialog = None;
                let active = self.active;
                self.close(active);
                self.refresh_tree();
            }
            Err(error) => {
                self.dialog = None;
                self.set_notice(format!("could not delete {}: {error}", path.display()));
            }
        }
    }

    // --- Frames -----------------------------------------------------------

    /// Collect the pages the PDF viewer rendered in the background, the files
    /// that changed on disk, and the search results that arrived.
    pub fn poll(&mut self) -> bool {
        self.poll_at(0.0)
    }

    /// [`Workspace::poll`], with the app's clock for the completion debounce.
    pub fn poll_at(&mut self, now: f64) -> bool {
        self.now = now;
        let mut changed = self.poll_pdfs();
        changed |= self.poll_pdf_double_click();
        changed |= self.poll_source_click();
        changed |= self.poll_files();
        if let Some(search) = self.search.as_mut() {
            changed |= search.poll();
        }
        self.refresh_git(false);
        changed |= self.poll_terminals();
        changed |= self.poll_images();
        changed |= self.poll_build();
        changed |= self.poll_servers();
        changed |= self.poll_completions();
        changed |= self.poll_replace();
        changed |= self.poll_remote_open();
        // A save during a build is not lost: the request waits for the running
        // build to finish, then rebuilds with what was saved.
        if self.latex_build_after_save
            && self.build.as_ref().is_none_or(|build| build.finished)
        {
            self.latex_build_after_save = false;
            self.build_latex();
            changed = true;
        }
        changed
    }

    /// Ask for completions when typing moved the caret to the end of a word.
    ///
    /// This is what makes completion feel automatic without asking the server
    /// on every frame: a request goes out once per *edit*, only when the
    /// character before the caret is part of a word, and no more often than
    /// [`COMPLETION_DEBOUNCE`]. Moving the caret by clicking is not an edit and
    /// does not open the list.
    fn poll_completions(&mut self) -> bool {
        if self.completion_open {
            // The list belongs where the request was made. A click, or more
            // typing, moves the caret and dismisses it.
            let current = self
                .active_document()
                .and_then(|document| document.editor())
                .map(|editor| (editor.edit.focus, editor.edit.text.len()));
            if current.is_some() && self.last_completion != current {
                self.close_completions();
            }
            return false;
        }
        let Some((path, position)) = self.caret() else {
            return false;
        };
        let Some((caret, length, preceded_by_word)) = self
            .active_document()
            .and_then(|d| d.editor())
            .map(|editor| {
                let caret = editor.edit.focus;
                let length = editor.edit.text.len();
                let preceded_by_word = editor.edit.text[..caret]
                    .chars()
                    .next_back()
                    .map(|ch| ch.is_alphanumeric() || ch == '_')
                    .unwrap_or(false);
                (caret, length, preceded_by_word)
            })
        else {
            return false;
        };

        let previous = self.last_completion;
        if previous == Some((caret, length)) {
            return false;
        }
        // Only a text change earns an automatic request. Clicking somewhere
        // in a word moves the caret but must not pop a list of insertions.
        let typed = previous.is_none_or(|(_, seen_length)| seen_length != length);
        self.last_completion = Some((caret, length));
        if !typed {
            return false;
        }
        // Only after a word character: completing after a space or a bracket
        // is usually noise, and the user can ask for it explicitly.
        if !preceded_by_word {
            return false;
        }
        if self.now - self.last_completion_at < COMPLETION_DEBOUNCE {
            return false;
        }

        self.last_completion_at = self.now;
        self.servers.request_completion(&path, position);
        false
    }

    /// Ask for completions at the caret, whatever precedes it.
    pub fn request_completions(&mut self) {
        let Some((path, position)) = self.caret() else {
            return;
        };
        if let Some(editor) = self.active_document().and_then(|document| document.editor()) {
            self.last_completion = Some((editor.edit.focus, editor.edit.text.len()));
        }
        self.servers.request_completion(&path, position);
    }

    /// Move the completion highlight.
    pub fn completion_move(&mut self, down: bool) {
        if self.completions.is_empty() {
            return;
        }
        let count = self.completions.len();
        self.completion_selected = if down {
            (self.completion_selected + 1) % count
        } else if self.completion_selected == 0 {
            count - 1
        } else {
            self.completion_selected - 1
        };
    }

    /// Insert the highlighted completion.
    pub fn accept_completion(&mut self) {
        let Some(item) = self.completions.get(self.completion_selected).cloned() else {
            self.close_completions();
            return;
        };
        if let Some(editor) = self.active_editor_mut() {
            let caret = editor.edit.focus;
            // Replace the word fragment before the caret, not just insert.
            let text = &editor.edit.text;
            let start = text[..caret]
                .char_indices()
                .rev()
                .take_while(|(_, ch)| ch.is_alphanumeric() || *ch == '_')
                .last()
                .map(|(index, _)| index)
                .unwrap_or(caret);
            let mut updated = String::with_capacity(text.len() + item.insert.len());
            updated.push_str(&text[..start]);
            updated.push_str(&item.insert);
            updated.push_str(&text[caret..]);
            editor.edit.replace_text(updated);
            let target = start + item.insert.len();
            editor.edit.set_caret(target);
            editor.edit.set_selection(target, target);
        }
        // Accepting is not typing: keep the freshly inserted text from asking
        // for another completion on the next frame.
        if let Some(editor) = self.active_document().and_then(|document| document.editor()) {
            self.last_completion = Some((editor.edit.focus, editor.edit.text.len()));
        }
        self.close_completions();
    }

    /// Hide the completion list.
    pub fn close_completions(&mut self) {
        self.completion_open = false;
        self.completions.clear();
        self.completion_selected = 0;
    }

    /// Keep the language servers in step with the open documents.
    fn poll_servers(&mut self) -> bool {
        // Send the text of every open document that changed.
        let documents: Vec<(PathBuf, String)> = self
            .documents
            .iter()
            .filter_map(|document| {
                let path = document.path()?.to_path_buf();
                let text = document.text()?.to_string();
                Some((path, text))
            })
            .collect();
        self.servers.sync(&documents);

        let outcomes = self.servers.poll();
        let mut changed = !outcomes.is_empty();
        for outcome in outcomes {
            match outcome {
                Outcome::Definition(path, position) => {
                    let line = position.line + 1;
                    let column = position.character as usize;
                    self.open_match(path, line, column);
                }
                Outcome::Hover(text) => {
                    self.hover = if text.is_empty() { None } else { Some(text) };
                }
                Outcome::References(locations) => {
                    self.references = locations
                        .into_iter()
                        .map(|(path, position)| {
                            (path, position.line + 1, position.character as usize)
                        })
                        .collect();
                    self.sidebar = SidebarMode::Places;
                    self.settings.show_sidebar = true;
                }
                Outcome::Completions(items) => {
                    if items.is_empty() {
                        self.close_completions();
                    } else {
                        self.completions = items;
                        self.completion_selected = 0;
                        self.completion_open = true;
                    }
                }
                Outcome::CodeActions(actions) => {
                    if actions.is_empty() {
                        self.code_actions_open = false;
                        self.set_notice("no code actions here".to_string());
                    } else {
                        self.code_actions = actions;
                        self.code_actions_open = true;
                    }
                }
                Outcome::Rename(files) => {
                    self.apply_workspace_edit(&files);
                }
                Outcome::Format(edits) => {
                    self.apply_formatting(&edits);
                    changed = true;
                }
                Outcome::Unsupported(what) => {
                    self.notice = Some(format!("the language server does not support {what}"));
                }
            }
        }
        changed
    }

    /// Apply a formatting response to the active document.
    ///
    /// The edits arrive in document order and must be applied from the end
    /// so that earlier offsets stay valid.
    fn apply_formatting(&mut self, edits: &[lide_lsp::client::ServerTextEdit]) {
        let Some(editor) = self.active_editor_mut() else {
            return;
        };
        let text = editor.edit.text.clone();
        let formatted = apply_text_edits(&text, edits);
        if formatted != text {
            editor.edit.replace_text(formatted);
        }
    }

    /// Check the active document as JSON, TOML, YAML or XML.
    pub fn validate_document(&mut self) {
        let Some(document) = self.active_document() else {
            self.notice = Some("no document is active".to_string());
            return;
        };
        let Some(path) = document.path() else {
            self.notice = Some("the active document has no file".to_string());
            return;
        };
        let Some(kind) = lide_documents::structured::Kind::for_path(path) else {
            self.notice = Some("no JSON, TOML, YAML or XML checker for this file".to_string());
            return;
        };
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".to_string());
        let Some(text) = document.text() else {
            self.notice = Some("the active document is not text".to_string());
            return;
        };

        match lide_documents::structured::validate(text, kind) {
            Ok(()) => self.notice = Some(format!("{name}: valid {}", kind.name())),
            Err(problem) => self.notice = Some(format!("{name}: {problem}")),
        }
    }

    /// Format the active document if its format can be rewritten safely.
    pub fn format_structured_document(&mut self) {
        let index = self.active;
        let Some(document) = self.documents.get(index) else {
            return;
        };
        let Some(path) = document.path() else {
            self.notice = Some("the active document has no file".to_string());
            return;
        };
        let Some(kind) = lide_documents::structured::Kind::for_path(path) else {
            self.notice = Some("no JSON or XML formatter for this file".to_string());
            return;
        };
        let Some(text) = document.text().map(|text| text.to_string()) else {
            self.notice = Some("the active document is not text".to_string());
            return;
        };

        match lide_documents::structured::format(&text, kind) {
            Ok(formatted) if formatted == text => {
                self.notice = Some(format!("{} is already formatted", kind.name()));
            }
            Ok(formatted) => {
                if let Some(editor) = self.documents[index].editor_mut() {
                    editor.edit.replace_text(formatted);
                }
                self.notice = Some(format!("{} formatted", kind.name()));
            }
            Err(problem) => self.notice = Some(problem.to_string()),
        }
    }

    /// Ask what the language server offers to do at the caret.
    pub fn show_code_actions(&mut self) {
        let Some((path, position)) = self.caret() else {
            return;
        };
        let line_end = lide_lsp::Position {
            line: position.line,
            character: u32::MAX as u64,
        };
        if !self.servers.request_code_actions(&path, position, line_end) {
            self.notice = Some("no language server for this file".to_string());
        }
    }

    /// Run the code action at `index`.
    pub fn apply_code_action(&mut self, index: usize) {
        let Some(action) = self.code_actions.get(index).cloned() else {
            return;
        };
        self.code_actions_open = false;
        self.code_actions.clear();
        if action.edits.is_empty() {
            self.set_notice(format!(
                "'{}' is a command the editor cannot run yet",
                action.title
            ));
            return;
        }
        self.apply_workspace_edit(&action.edits);
    }

    /// Ask the language server to rename the symbol under the caret.
    pub fn begin_rename_symbol(&mut self) {
        let Some((path, position)) = self.caret() else {
            return;
        };
        let _ = (&path, position);
        // Prefill with the word under the caret, so the dialog starts from
        // what the user is looking at.
        let word = self
            .active_document()
            .and_then(|document| document.text())
            .map(|text| {
                let caret = self.caret_offset();
                word_at(text, caret)
            })
            .unwrap_or_default();
        let mut name = TextEdit::new(word);
        name.focused = true;
        self.dialog = Some(DialogState::RenameSymbol { name, error: None });
    }

    /// Send the rename the dialog asked for.
    pub fn confirm_rename_symbol(&mut self) {
        let Some(DialogState::RenameSymbol { name, .. }) = &self.dialog else {
            return;
        };
        let new_name = name.text.trim().to_string();
        if new_name.is_empty() {
            return;
        }
        let Some((path, position)) = self.caret() else {
            return;
        };
        if !self.servers.request_rename(&path, position, &new_name) {
            self.dialog = None;
            self.set_notice("no language server can rename this file".to_string());
            return;
        }
        self.dialog = None;
    }

    /// The active document's caret as a byte offset.
    fn caret_offset(&self) -> usize {
        self.active_document()
            .and_then(|document| document.editor())
            .map(|editor| editor.edit.focus)
            .unwrap_or(0)
    }

    /// Apply a rename across the files it touches.
    ///
    /// Open documents are edited in place so that the change is undoable; files
    /// that are not open are read, edited and written back.
    fn apply_workspace_edit(&mut self, files: &[(PathBuf, Vec<lide_lsp::client::ServerTextEdit>)]) {
        if files.is_empty() {
            self.set_notice("the server suggested no edits".to_string());
            return;
        }

        let mut written = 0usize;
        for (path, edits) in files {
            if let Some(index) = self
                .documents
                .iter()
                .position(|document| document.path() == Some(path.as_path()))
            {
                if let Some(editor) = self.documents[index].editor_mut() {
                    let text = editor.edit.text.clone();
                    let updated = apply_text_edits(&text, edits);
                    if updated != text {
                        editor.edit.replace_text(updated);
                    }
                }
                written += 1;
                continue;
            }

            // Not open: edit the file on disk.
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            let Some(buffer) = lide_text::TextBuffer::from_bytes(&bytes) else {
                continue;
            };
            let text = buffer.to_text();
            let updated = apply_text_edits(&text, edits);
            if updated != text
                && std::fs::write(path, buffer.line_ending().apply(&updated).as_bytes()).is_ok()
            {
                written += 1;
            }
        }

        self.notice = Some(format!("renamed in {written} files"));
        self.refresh_git(true);
    }

    /// The active document's path and the caret as an LSP position.
    fn caret(&self) -> Option<(PathBuf, lide_lsp::Position)> {
        let document = self.active_document()?;
        let path = document.path()?.to_path_buf();
        let editor = document.editor()?;
        let caret = editor.edit.focus.min(editor.edit.text.len());
        let line = editor.edit.line_of_byte(caret) as u64;
        let line_start = editor.edit.line_range(line as usize).start;
        // LSP counts characters in UTF-16 code units, not bytes.
        let line_text = &editor.edit.text[line_start.min(caret)..caret];
        let character: u64 = line_text.chars().map(|ch| ch.len_utf16() as u64).sum();
        Some((path, lide_lsp::Position { line, character }))
    }

    /// Ask the language server where the symbol under the caret is defined.
    pub fn go_to_definition(&mut self) {
        let Some((path, position)) = self.caret() else {
            return;
        };
        if !self.servers.request_definition(&path, position) {
            self.notice = Some("no language server for this file".to_string());
        }
    }

    /// Ask the language server what is under the caret.
    pub fn show_hover(&mut self) {
        let Some((path, position)) = self.caret() else {
            return;
        };
        if !self.servers.request_hover(&path, position) {
            self.notice = Some("no language server for this file".to_string());
        }
    }

    /// Ask the language server where the symbol under the caret is used.
    pub fn find_references(&mut self) {
        let Some((path, position)) = self.caret() else {
            return;
        };
        if !self.servers.request_references(&path, position) {
            self.notice = Some("no language server for this file".to_string());
        }
    }

    /// Ask the language server to format the active document.
    pub fn format_document(&mut self) {
        let Some(path) = self
            .active_document()
            .and_then(|d| d.path())
            .map(|p| p.to_path_buf())
        else {
            return;
        };
        if !self.servers.request_format(&path) {
            self.notice = Some("no formatter for this file".to_string());
        }
    }

    /// Run the workspace search for the current query.
    ///
    /// The previous search is cancelled first: typing "abc" should not leave
    /// the results for "ab" in the list.
    pub fn run_search(&mut self) {
        self.sidebar = SidebarMode::Search;
        if let Some(mut previous) = self.search.take() {
            previous.cancel();
        }

        let query = Query {
            pattern: self.search_query.text.clone(),
            regex: self.search_regex,
            case_sensitive: self.search_case_sensitive,
            whole_word: false,
        };
        if !query.is_searchable() {
            return;
        }
        let Some(root) = self.root.clone() else {
            self.notice = Some("open a folder to search in".to_string());
            return;
        };
        self.search = Some(Search::start(&root, query));
    }

    /// Ask to replace every workspace match, after checking what it would
    /// touch.
    pub fn begin_replace_all(&mut self) {
        let Some(search) = &self.search else {
            self.notice = Some("search the workspace first".to_string());
            return;
        };
        if search.running {
            self.notice = Some("wait for the search to finish".to_string());
            return;
        }
        if search.query.pattern.is_empty() || search.matches.is_empty() {
            self.notice = Some("the search found nothing to replace".to_string());
            return;
        }
        if self.search_replacement.text.is_empty() {
            self.notice = Some("type what to replace it with".to_string());
            return;
        }

        // A line can hold several matches; the dialog counts occurrences,
        // which is what the replace worker will write.
        let occurrences: usize = search
            .matches
            .iter()
            .map(|found| found.ranges.len().max(1))
            .sum();
        let paths: Vec<&Path> = unique_paths(&search.matches);
        // A dirty buffer is unsaved work; writing the file underneath it
        // would make the two disagree.
        for document in &self.documents {
            if document.is_dirty() && document.path().is_some_and(|path| paths.contains(&path)) {
                let name = document
                    .path()
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "a file".to_string());
                self.notice = Some(format!("save {name} before replacing across files"));
                return;
            }
        }

        self.dialog = Some(DialogState::ConfirmReplace {
            files: paths.len(),
            matches: occurrences,
        });
    }

    /// Run the replacement the dialog confirmed.
    pub fn confirm_replace_all(&mut self) {
        let Some(DialogState::ConfirmReplace { .. }) = &self.dialog else {
            return;
        };
        self.dialog = None;
        let Some(search) = &self.search else {
            return;
        };
        let paths: Vec<PathBuf> = unique_paths(&search.matches)
            .into_iter()
            .map(Path::to_path_buf)
            .collect();
        let query = search.query.clone();
        let replacement = self.search_replacement.text.clone();
        self.replace = Some(lide_search::Replace::start(paths, query, replacement));
        self.notice = Some("replacing…".to_string());
    }

    /// Collect the replace worker's progress, and put the results into the
    /// open documents when it finishes.
    fn poll_replace(&mut self) -> bool {
        let Some(replace) = self.replace.as_mut() else {
            return false;
        };
        let changed = replace.poll();
        if replace.running {
            return changed;
        }

        let files = replace.files.clone();
        let file_count = files.len();
        let matches = replace.matches;
        let skipped = replace.skipped;
        let error = replace.error.clone();
        self.replace = None;

        for path in files {
            for document in &mut self.documents {
                if document.path() == Some(path.as_path()) && !document.is_dirty() {
                    if let Some(editor) = document.editor_mut() {
                        let _ = editor.reload();
                    }
                }
            }
            self.refresh_image_previews(&path);
        }

        self.notice = Some(match error {
            Some(error) => format!("replace stopped: {error}"),
            None if skipped > 0 => format!(
                "replaced {matches} matches in {file_count} files; skipped {skipped} that were not text"
            ),
            None => format!("replaced {matches} matches in {file_count} files"),
        });

        // Show the tree as it is now, not as it was before replacing.
        self.run_search();
        true
    }

    /// Open the file a search result is in and put the caret on the match.
    pub fn open_match(&mut self, path: PathBuf, line: u64, column: usize) {
        if let Err(error) = self.open_path(&path) {
            self.set_notice(error);
            return;
        }
        if let Some(editor) = self.active_editor_mut() {
            editor.go_to(line, column);
        }
    }

    /// The number of matches found so far.
    pub fn match_count(&self) -> usize {
        self.search
            .as_ref()
            .map(|search| search.matches.len())
            .unwrap_or(0)
    }

    fn poll_pdfs(&mut self) -> bool {
        let Some(cache) = self.images.as_ref() else {
            return false;
        };
        let mut changed = false;
        let mut scrolls = Vec::new();
        for (index, document) in self.documents.iter_mut().enumerate() {
            if let Document::Pdf(pdf) = document {
                let pane = if self.split_document == Some(index) { 1 } else { 0 };
                pdf.fit_to_width(self.pdf_widths[pane].get());
                changed |= pdf.poll(cache);
                if let Some(offset) = pdf.take_scroll_wanted() {
                    scrolls.push((index, offset));
                }
            }
        }
        for (index, offset) in scrolls {
            let pane = if self.split_document == Some(index) { 1 } else { 0 };
            self.request_scroll(format!("pdf-scroll-{pane}"), offset);
        }
        changed
    }

    /// A Cmd/Ctrl-click on a LaTeX source shows the same place in the PDF.
    ///
    /// The text field places the caret exactly where it was clicked, wrapped
    /// lines included, so the jump starts from the caret instead of from a
    /// guess about the layout. A caret that moves while no key was pressed is
    /// a click; arrows and typing arrive with a key event just before.
    fn poll_source_click(&mut self) -> bool {
        let index = self.focused_index();
        let seen = self
            .documents
            .get(index)
            .filter(|document| {
                document
                    .path()
                    .is_some_and(|path| path.extension().is_some_and(|e| e == "tex"))
            })
            .and_then(|document| document.editor())
            .map(|editor| (index, editor.edit.focus));
        let before = std::mem::replace(&mut self.caret_seen, seen);
        let moved = matches!((before, seen), (Some(a), Some(b)) if a.0 == b.0 && a.1 != b.1);
        if moved && self.primary_modifier && self.now - self.last_key_at > 0.25 {
            self.sync_forward();
            return true;
        }
        false
    }

    /// A double click on a PDF page asks for its source, through SyncTeX.
    fn poll_pdf_double_click(&mut self) -> bool {
        // The framework's double press, as a second way in.
        let mut requested = None;
        for (index, document) in self.documents.iter_mut().enumerate() {
            if let Document::Pdf(pdf) = document {
                if pdf.double_click {
                    pdf.double_click = false;
                    requested = Some(index);
                }
            }
        }

        // The editor's own double click, with a hand-sized tolerance.
        if let Some(pane) = self.pdf_double_click.take() {
            let index = if pane == 1 {
                self.split_document
            } else {
                Some(self.active)
            };
            if index.is_some() {
                self.focused_pane = pane as usize;
                self.sync_inverse();
                return true;
            }
        }

        let Some(index) = requested else {
            return false;
        };
        self.focused_pane = if self.split_document == Some(index) {
            1
        } else {
            0
        };
        self.sync_inverse();
        true
    }

    /// Apply the file-system changes the watcher collected.
    ///
    /// A file whose content the editor already has is not a change: that is
    /// the editor's own save coming back. A file nobody has edited is
    /// reloaded. A file with unsaved changes is *reported*, never overwritten.
    fn poll_files(&mut self) -> bool {
        let Some(watch) = self.watch.as_mut() else {
            return false;
        };
        let changes = watch.drain();
        if changes.is_empty() {
            return false;
        }

        let mut changed = false;
        for change in changes {
            // A path can be open twice: once as text and once as a preview.
            // The text is what reloads; the preview is refreshed after.
            let index = self
                .documents
                .iter()
                .position(|document| {
                    document.path() == Some(change.path.as_path()) && document.editor().is_some()
                })
                .or_else(|| {
                    self.documents
                        .iter()
                        .position(|document| document.path() == Some(change.path.as_path()))
                });
            let Some(index) = index else {
                // A file in the tree that is not open: refresh the tree.
                if change.exists {
                    self.tree = self
                        .root
                        .clone()
                        .map(|root| FileTree::scan(&root, MAX_DEPTH))
                        .unwrap_or_default();
                    changed = true;
                }
                continue;
            };

            // A PDF is binary: reading it back as text would call a rebuild a
            // deletion. The build reloads the view itself; the watcher only
            // has to forget any earlier report for it.
            if change.exists && matches!(self.documents.get(index), Some(Document::Pdf(_))) {
                self.changed_on_disk.retain(|path| path != &change.path);
                continue;
            }

            let Some(on_disk) = std::fs::read(&change.path)
                .ok()
                .and_then(|bytes| lide_text::TextBuffer::from_bytes(&bytes))
                .map(|buffer| buffer.to_text())
            else {
                // The file is gone.
                if !self.changed_on_disk.contains(&change.path) {
                    self.changed_on_disk.push(change.path.clone());
                }
                changed = true;
                continue;
            };

            let dirty = self.documents[index].is_dirty();
            let same = self.documents[index]
                .text()
                .map(|text| text == on_disk)
                .unwrap_or(false);

            if same {
                // The editor's own write, or a rewrite with identical content.
                self.changed_on_disk.retain(|path| path != &change.path);
                continue;
            }

            if dirty {
                if !self.changed_on_disk.contains(&change.path) {
                    self.changed_on_disk.push(change.path.clone());
                }
                changed = true;
            } else {
                let mut reloaded = false;
                if let Some(editor) = self.documents[index].editor_mut() {
                    if editor.reload().is_ok() {
                        self.changed_on_disk.retain(|path| path != &change.path);
                        changed = true;
                        reloaded = true;
                    }
                }
                if reloaded {
                    self.refresh_image_previews(&change.path);
                }
            }
        }
        changed
    }

    /// Take the text from disk for the active document, discarding what the
    /// editor holds.
    pub fn accept_external_change(&mut self) {
        let active = self.active;
        let Some(document) = self.documents.get_mut(active) else {
            return;
        };
        let Some(path) = document.path().map(|path| path.to_path_buf()) else {
            return;
        };
        if let Some(editor) = document.editor_mut() {
            if editor.reload().is_ok() {
                self.changed_on_disk.retain(|other| other != &path);
                self.refresh_image_previews(&path);
            }
        }
    }

    /// Dismiss the "changed on disk" warning and keep the editor's text.
    pub fn keep_local_change(&mut self) {
        let Some(path) = self
            .active_document()
            .and_then(|d| d.path())
            .map(|p| p.to_path_buf())
        else {
            return;
        };
        self.changed_on_disk.retain(|other| other != &path);
    }

    /// Create the file named in the new-file dialog.
    pub fn confirm_new_file(&mut self) {
        let Some(DialogState::NewFile { name, .. }) = &self.dialog else {
            return;
        };
        let name = name.text.clone();
        self.create_file(&name);
    }

    /// Rename the active document to the name in the rename dialog.
    pub fn confirm_rename(&mut self) {
        let Some(DialogState::Rename { name, .. }) = &self.dialog else {
            return;
        };
        let name = name.text.clone();
        self.rename_active(&name);
    }

    /// Delete the active document from disk, as the confirm dialog asks.
    pub fn confirm_delete(&mut self) {
        self.delete_active();
    }

    /// Run what the palette has highlighted.
    pub fn run_selected_command(&mut self) {
        let Some(entry) = self.palette.selected_entry() else {
            return;
        };
        self.palette.close();
        match entry {
            command::Entry::Command(command) => command::run(command, self),
            command::Entry::Task(index, _) => self.run_task(index),
            command::Entry::RecentFile(index, _) => {
                if let Some(path) = self.settings.recent_files.get(index).cloned() {
                    if let Err(error) = self.open_path(&path) {
                        self.set_notice(error);
                    }
                }
            }
            command::Entry::RecentProject(index, _) => {
                if let Some(path) = self.settings.recent_projects.get(index).cloned() {
                    if let Err(error) = self.open_folder(&path) {
                        self.set_notice(error.to_string());
                    }
                }
            }
        }
    }

    /// Replace the current match and move to the next one.
    pub fn replace_current(&mut self) {
        let Some((range, replacement)) = self.find.replacement_edit() else {
            return;
        };
        let Some(editor) = self.active_editor_mut() else {
            return;
        };
        let text = replace_range(&editor.edit.text, range.clone(), &replacement);
        editor.edit.replace_text(text);
        let updated = editor.edit.text.clone();
        self.find.refresh(&updated);
        self.find.next();
    }

    /// Replace every match.
    pub fn replace_all(&mut self) {
        let edits = self.find.replace_all_edits();
        if edits.is_empty() {
            return;
        }
        let Some(editor) = self.active_editor_mut() else {
            return;
        };
        let mut text = editor.edit.text.clone();
        // Apply from the end so that earlier offsets stay valid.
        for (range, replacement) in edits.iter().rev() {
            text = replace_range(&text, range.clone(), replacement);
        }
        editor.edit.replace_text(text);
        let updated = editor.edit.text.clone();
        self.find.matches.clear();
        self.find.refresh(&updated);
    }
}

/// The theme tokens for the current appearance.
pub fn kit(dark: bool) -> &'static Kit {
    crate::theme::kit(dark)
}



fn is_table(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("csv") | Some("tsv")
    )
}

/// Whether a file has an SVG source.
fn is_svg(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("svg"))
        .unwrap_or(false)
}

/// Whether a file is a Jupyter notebook.
fn is_notebook(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("ipynb")
    )
}

/// Collect the `.tex` files under `directory`, shallowly.
fn collect_tex(directory: &Path, found: &mut Vec<PathBuf>, depth: usize) {
    // Deep and wide enough for a paper project with its listings and
    // chapters; capped so a huge repository cannot make a save slow.
    if depth > 4 || found.len() > 128 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            collect_tex(&path, found, depth + 1);
        } else if path
            .extension()
            .map(|extension| extension == "tex")
            .unwrap_or(false)
        {
            found.push(path);
        }
    }
}

/// The `.tex` files that other files do not include: the build roots.
fn tex_roots(files: &[PathBuf]) -> Vec<PathBuf> {
    use std::collections::HashSet;

    let mut included = HashSet::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        for name in tex_includes(&text) {
            let mut path = file
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(name);
            if path.extension().is_none() {
                path.set_extension("tex");
            }
            included.insert(fs_key(&path));
        }
    }
    files
        .iter()
        .filter(|path| !included.contains(&fs_key(path)))
        .cloned()
        .collect()
}

/// Whether `path` is one of the given roots, whatever spelling the caller
/// used: the opened file and the walk over the workspace can disagree about
/// symlinks and a filesystem's canonical case, and they name one file.
fn is_one_of_roots(path: &Path, roots: &[PathBuf]) -> bool {
    roots
        .iter()
        .any(|root| root == &path.to_path_buf() || fs_key(root) == fs_key(path))
}

/// The names in `\input{...}`, `\include{...}` and `\subfile{...}`.
fn tex_includes(text: &str) -> Vec<&str> {
    let mut names = Vec::new();
    for command in ["\\input", "\\include", "\\subfile"] {
        let mut rest = text;
        while let Some(index) = rest.find(command) {
            let after = rest[index + command.len()..].trim_start();
            if let Some(after) = after.strip_prefix('{') {
                if let Some(end) = after.find('}') {
                    let name = after[..end].trim();
                    if !name.is_empty() {
                        names.push(name);
                    }
                }
            }
            rest = &rest[index + command.len()..];
        }
    }
    names
}

fn fs_key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The project a file belongs to.
///
/// Walks up from the file looking for something that marks a project root,
/// and falls back to the file's own directory.
pub fn project_root(path: &Path) -> PathBuf {
    const MARKERS: &[&str] = &[
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "go.mod",
        "CMakeLists.txt",
        "Makefile",
        "pom.xml",
        "build.gradle",
        ".git",
        ".hg",
    ];

    let mut directory = path.parent();
    while let Some(current) = directory {
        if MARKERS.iter().any(|marker| current.join(marker).exists()) {
            return current.to_path_buf();
        }
        directory = current.parent();
    }

    path.parent()
        .map(|parent| parent.to_path_buf())
        .unwrap_or_default()
}

/// The width of one character of the editor's font, in points.
///
/// The view cannot ask the text system directly, so it renders an invisible
/// run of characters and reads its width back from the previous layout. Until
/// that has happened once, an estimate from the font size is used; it is
/// deliberately a little generous, because underestimating would make a long
/// line wrap and put the gutter out of step.
/// A path with its `.` and `..` components resolved lexically, without
/// touching the file system (so it works for files that do not exist yet).
fn clean_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !matches!(out.components().next_back(), Some(Component::Normal(_))) || !out.pop()
                {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// Apply a server's edits to `text`.
///
/// The edits arrive in document order; they are applied from the end so that
/// earlier offsets stay valid. An edit that overlaps one already applied, or
/// that points outside the text, is dropped rather than corrupting the file.
fn apply_text_edits(text: &str, edits: &[lide_lsp::client::ServerTextEdit]) -> String {
    let lines = text.lines().count().max(1) as u64;
    let mut offsets: Vec<(usize, usize, &str)> = Vec::with_capacity(edits.len());
    for edit in edits {
        // A position past the last line is a server bug; clamping it into an
        // append would quietly move text, so the edit is dropped.
        if edit.start.line >= lines || edit.end.line >= lines {
            continue;
        }
        let start = offset_of(text, edit.start.line, edit.start.character);
        let end = offset_of(text, edit.end.line, edit.end.character);
        offsets.push((start, end, edit.new_text.as_str()));
    }
    offsets.sort_by_key(|(start, _, _)| *start);

    let mut out = String::with_capacity(text.len() + 64);
    let mut last = 0usize;
    for (start, end, new_text) in offsets {
        if start < last || end < start || end > text.len() || !text.is_char_boundary(start) {
            continue;
        }
        out.push_str(&text[last..start]);
        out.push_str(new_text);
        last = end;
    }
    out.push_str(&text[last..]);
    out
}

/// The identifier around `caret`, for prefilling a rename.
fn word_at(text: &str, caret: usize) -> String {
    let caret = caret.min(text.len());
    let mut start = caret;
    for (index, ch) in text[..caret].char_indices().rev() {
        if ch.is_alphanumeric() || ch == '_' {
            start = index;
        } else {
            break;
        }
    }
    let mut end = caret;
    for (index, ch) in text[caret..].char_indices() {
        if ch.is_alphanumeric() || ch == '_' {
            end = caret + index + ch.len_utf8();
        } else {
            break;
        }
    }
    text[start..end].to_string()
}

/// The byte offset of an LSP position in `text`.
///
/// LSP counts lines from zero and characters in UTF-16 code units, which is
/// not the same as bytes whenever the text is not plain ASCII.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = Vec::with_capacity(text.lines().count() + 1);
    starts.push(0);
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

/// The byte offset of a UTF-16 `(line, character)` position, with line starts
/// already computed.
fn offset_at_line(text: &str, starts: &[usize], line: u64, character: u64) -> usize {
    let start = starts
        .get(line as usize)
        .copied()
        .unwrap_or(text.len());
    let end = text[start..]
        .find('\n')
        .map(|index| start + index)
        .unwrap_or(text.len());
    let mut units = 0u64;
    for (index, ch) in text[start..end].char_indices() {
        if units >= character {
            return start + index;
        }
        units += ch.len_utf16() as u64;
    }
    end
}

fn offset_of(text: &str, line: u64, character: u64) -> usize {
    let mut offset = 0usize;
    for (current_line, current) in text.split_inclusive('\n').enumerate() {
        let current_line = current_line as u64;
        if current_line == line {
            let mut units = 0u64;
            for (index, ch) in current.char_indices() {
                if units >= character {
                    return offset + index;
                }
                units += ch.len_utf16() as u64;
            }
            return offset + current.trim_end_matches('\n').len();
        }
        offset += current.len();
    }
    offset.min(text.len())
}

/// The files a set of matches touched, once each, in the order first seen.
fn unique_paths(matches: &[lide_search::Match]) -> Vec<&Path> {
    let mut paths: Vec<&Path> = Vec::new();
    for found in matches {
        let path = found.path.as_path();
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

/// Replace `range` in `text`, returning the new string.
fn replace_range(text: &str, range: std::ops::Range<usize>, replacement: &str) -> String {
    let mut out = String::with_capacity(text.len() + replacement.len());
    out.push_str(&text[..range.start]);
    out.push_str(replacement);
    out.push_str(&text[range.end.min(text.len())..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::editor::diagnostic_spans;

    fn edit(start: (u64, u64), end: (u64, u64), text: &str) -> lide_lsp::client::ServerTextEdit {
        lide_lsp::client::ServerTextEdit {
            start: lide_lsp::Position {
                line: start.0,
                character: start.1,
            },
            end: lide_lsp::Position {
                line: end.0,
                character: end.1,
            },
            new_text: text.to_string(),
        }
    }

    #[test]
    fn positions_become_byte_offsets() {
        let text = "one\ntwo\nthree";
        assert_eq!(offset_of(text, 0, 0), 0);
        assert_eq!(offset_of(text, 0, 2), 2);
        assert_eq!(offset_of(text, 1, 0), 4);
        assert_eq!(offset_of(text, 2, 3), 11);
    }

    #[test]
    fn positions_count_utf16_units_not_bytes() {
        // "é" is two bytes but one UTF-16 unit.
        let text = "é x\nz";
        assert_eq!(offset_of(text, 0, 1), 2, "after the é");
        assert_eq!(offset_of(text, 0, 3), 4, "after the x");
        // An astral character is two UTF-16 units.
        let text = "😀a";
        assert_eq!(offset_of(text, 0, 2), 4, "after the emoji");
        assert_eq!(offset_of(text, 0, 3), 5);
    }

    #[test]
    fn a_position_past_the_end_clamps() {
        assert_eq!(offset_of("abc", 9, 0), 3);
        assert_eq!(offset_of("abc", 0, 99), 3);
    }

    fn diagnostic(start: (u64, u64), end: (u64, u64)) -> lide_lsp::Diagnostic {
        lide_lsp::Diagnostic {
            path: PathBuf::from("/tmp/a.rs"),
            start: lide_lsp::Position {
                line: start.0,
                character: start.1,
            },
            end: lide_lsp::Position {
                line: end.0,
                character: end.1,
            },
            severity: lide_lsp::Severity::Error,
            message: "unused".to_string(),
            source: "test".to_string(),
        }
    }

    #[test]
    fn saving_a_tex_file_asks_for_a_build_and_other_files_do_not() {
        let directory =
            std::env::temp_dir().join(format!("lide-build-on-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let tex = directory.join("main.tex");
        let notes = directory.join("notes.txt");
        std::fs::write(&tex, "\\documentclass{article}\n").unwrap();
        std::fs::write(&notes, "hello\n").unwrap();

        let mut workspace = Workspace::new();
        workspace.open_folder(&directory).expect("the folder opens");
        workspace.open_path(&notes).expect("the text file opens");
        workspace.active_editor_mut().unwrap().edit.insert("x");
        let index = workspace.active;
        workspace.save(index);
        assert!(
            !workspace.latex_build_after_save,
            "saving a .txt does not build LaTeX"
        );

        workspace.open_path(&tex).expect("the .tex file opens");
        workspace.active_editor_mut().unwrap().edit.insert("x");
        let index = workspace.active;
        workspace.save(index);
        assert!(
            workspace.latex_build_after_save,
            "saving a .tex schedules a build"
        );

        workspace.latex_build_after_save = false;
        workspace.save(index);
        assert!(
            !workspace.latex_build_after_save,
            "saving an unchanged .tex does not schedule another build"
        );

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn an_explicit_save_builds_a_clean_tex_file_but_not_a_text_file() {
        let directory =
            std::env::temp_dir().join(format!("lide-save-and-build-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let tex = directory.join("main.tex");
        let notes = directory.join("notes.txt");
        std::fs::write(&tex, "\\documentclass{article}\n").unwrap();
        std::fs::write(&notes, "hello\n").unwrap();

        let mut workspace = Workspace::new();
        workspace.open_folder(&directory).expect("the folder opens");
        workspace.open_path(&tex).expect("the .tex file opens");
        let index = workspace.active;
        assert!(!workspace.documents[index].is_dirty());

        // Cmd+S is save-and-typeset: an unchanged .tex still builds.
        workspace.save_and_build(index);
        assert!(
            workspace.latex_build_after_save,
            "an explicit save builds a clean .tex"
        );

        workspace.latex_build_after_save = false;
        workspace.open_path(&notes).expect("the text file opens");
        let index = workspace.active;
        workspace.save_and_build(index);
        assert!(
            !workspace.latex_build_after_save,
            "saving a .txt does not build LaTeX"
        );

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn the_document_being_edited_is_what_a_build_targets() {
        let directory =
            std::env::temp_dir().join(format!("lide-build-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        // Many unrelated root documents, so a wrong choice is loud: the save
        // must build the paper being edited, not the first root in the tree.
        let extra = directory.join("extra");
        std::fs::create_dir_all(&extra).unwrap();
        for index in 0..40 {
            let name = extra.join(format!("chapter{:02}.tex", index));
            std::fs::write(&name, "\\documentclass{article}\n").unwrap();
        }
        let paper = directory.join("paper");
        std::fs::create_dir_all(&paper).unwrap();
        let tex = paper.join("paper.tex");
        std::fs::write(
            &tex,
            "\\documentclass{acmart}\n\\begin{document}\nBlueBrothers\n\\end{document}\n",
        )
        .unwrap();

        let mut workspace = Workspace::new();
        workspace.open_folder(&directory).expect("the folder opens");
        workspace.open_path(&tex).expect("paper.tex opens");

        let target = workspace.latex_document().expect("a document to build");
        assert!(
            target.ends_with("paper.tex"),
            "the file being edited is what a save builds, not {target:?}"
        );

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_diagnostic_becomes_an_underlined_range() {
        let k = rune_kit::Kit::neutral(true);
        let text = "let x = 1;\nlet y = 2;\n";
        let found = diagnostic((1, 4), (1, 5));
        let spans = diagnostic_spans(text, Path::new("/tmp/a.rs"), &[found], &k);
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].start, spans[0].end), (15, 16), "the y of line 2");
        assert!(spans[0].underline.is_some());
    }

    #[test]
    fn a_zero_length_diagnostic_covers_one_character() {
        let k = rune_kit::Kit::neutral(true);
        let found = diagnostic((0, 1), (0, 1));
        let spans = diagnostic_spans("abc", Path::new("/tmp/a.rs"), &[found], &k);
        assert_eq!((spans[0].start, spans[0].end), (1, 2));
    }

    #[test]
    fn a_diagnostic_for_another_file_is_ignored() {
        let k = rune_kit::Kit::neutral(true);
        let mut other = diagnostic((0, 0), (0, 1));
        other.path = PathBuf::from("/tmp/b.rs");
        let spans = diagnostic_spans("abc", Path::new("/tmp/a.rs"), &[other], &k);
        assert!(spans.is_empty());
    }

    #[test]
    fn edits_are_applied_from_the_end() {
        // "fn  main( ) {}" -> collapse the double space, then the pair's space.
        let text = "fn  main( ) {}\n";
        let edits = vec![edit((0, 2), (0, 4), " "), edit((0, 9), (0, 10), "")];
        assert_eq!(apply_text_edits(text, &edits), "fn main() {}\n");
    }

    #[test]
    fn an_overlapping_edit_is_dropped() {
        let text = "abcdef";
        let edits = vec![edit((0, 0), (0, 4), "X"), edit((0, 2), (0, 5), "Y")];
        let out = apply_text_edits(text, &edits);
        assert!(out.starts_with('X'), "the first edit wins: {out}");
        assert!(!out.contains('Y'), "the overlapping edit is dropped: {out}");
    }

    #[test]
    fn an_out_of_range_edit_is_dropped() {
        let out = apply_text_edits("abc", &[edit((5, 0), (5, 1), "X")]);
        assert_eq!(out, "abc");
    }

    fn workspace_with_text(text: &str) -> Workspace {
        let mut workspace = Workspace::new();
        let mut editor = lide_editor::Editor::empty();
        editor.edit.replace_text(text);
        editor.edit.set_caret(text.len());
        workspace.documents.push(Document::Text(editor));
        workspace
    }

    #[test]
    fn accepting_a_completion_replaces_the_word_before_the_caret() {
        let mut workspace = workspace_with_text("fn mai");
        workspace.completions = vec![lide_lsp::CompletionItem {
            label: "main".to_string(),
            detail: None,
            kind: Some(3),
            insert: "main()".to_string(),
        }];
        workspace.completion_open = true;

        workspace.accept_completion();

        assert_eq!(
            workspace.active_document().and_then(|d| d.text()),
            Some("fn main()"),
            "the typed fragment is replaced, not appended to"
        );
        assert!(!workspace.completion_open);
        assert_eq!(
            workspace.active_editor_mut().unwrap().edit.focus,
            "fn main()".len()
        );
    }

    #[test]
    fn clicking_in_a_word_does_not_open_the_completion_list() {
        let path = std::env::temp_dir().join("lide-completion-click.txt");
        std::fs::write(&path, "fn main").unwrap();
        let mut workspace = workspace_with_text("fn main");
        if let Some(editor) = workspace.active_editor_mut() {
            editor.location = Some(lide_resource::Location::local(path.clone()));
        }
        workspace.last_completion = Some((7, 7));

        // A click inside `main` moves the caret without changing the text.
        workspace.active_editor_mut().unwrap().edit.set_caret(6);
        assert!(!workspace.poll_completions());
        assert_eq!(
            workspace.last_completion,
            Some((6, 7)),
            "the click is remembered, not treated as typing"
        );
        assert!(!workspace.completion_open);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn moving_the_caret_closes_an_open_completion_list() {
        let mut workspace = workspace_with_text("fn main");
        workspace.last_completion = Some((7, 7));
        workspace.completion_open = true;
        workspace.completions = vec![lide_lsp::CompletionItem {
            label: "main".to_string(),
            detail: None,
            kind: None,
            insert: "main".to_string(),
        }];

        workspace.active_editor_mut().unwrap().edit.set_caret(6);
        assert!(!workspace.poll_completions());
        assert!(!workspace.completion_open, "the list belongs to the caret");
    }

    #[test]
    fn accepting_after_a_bracket_only_inserts() {
        let mut workspace = workspace_with_text("call(");
        workspace.completions = vec![lide_lsp::CompletionItem {
            label: "x".to_string(),
            detail: None,
            kind: None,
            insert: "x".to_string(),
        }];
        workspace.completion_open = true;

        workspace.accept_completion();
        assert_eq!(
            workspace.active_document().and_then(|d| d.text()),
            Some("call(x")
        );
    }

    #[test]
    fn the_completion_highlight_wraps() {
        let mut workspace = workspace_with_text("");
        workspace.completions = vec![
            lide_lsp::CompletionItem {
                label: "a".into(),
                detail: None,
                kind: None,
                insert: "a".into(),
            },
            lide_lsp::CompletionItem {
                label: "b".into(),
                detail: None,
                kind: None,
                insert: "b".into(),
            },
        ];
        assert_eq!(workspace.completion_selected, 0);
        workspace.completion_move(true);
        assert_eq!(workspace.completion_selected, 1);
        workspace.completion_move(true);
        assert_eq!(workspace.completion_selected, 0, "down wraps");
        workspace.completion_move(false);
        assert_eq!(workspace.completion_selected, 1, "up wraps");
    }

    #[test]
    fn closing_completions_clears_the_list() {
        let mut workspace = workspace_with_text("");
        workspace.completions = vec![lide_lsp::CompletionItem {
            label: "a".into(),
            detail: None,
            kind: None,
            insert: "a".into(),
        }];
        workspace.completion_open = true;
        workspace.close_completions();
        assert!(!workspace.completion_open);
        assert!(workspace.completions.is_empty());
    }

    #[test]
    fn the_word_under_the_caret_is_found() {
        assert_eq!(word_at("fn main() {}", 4), "main");
        assert_eq!(word_at("fn main() {}", 3), "main", "inside the word");
        assert_eq!(
            word_at("fn main() {}", 2),
            "fn",
            "a caret just after a word takes that word, which is what a rename wants"
        );
        assert_eq!(word_at("", 0), "");
    }

    #[test]
    fn a_workspace_edit_touches_every_file_it_names() {
        let directory = std::env::temp_dir().join("lide-rename-test");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let a = directory.join("a.txt");
        let b = directory.join("b.txt");
        std::fs::write(&a, "old name\n").unwrap();
        std::fs::write(&b, "name and old\n").unwrap();

        let mut workspace = Workspace::new();
        let edits = vec![
            (
                a.clone(),
                vec![lide_lsp::client::ServerTextEdit {
                    start: lide_lsp::Position {
                        line: 0,
                        character: 0,
                    },
                    end: lide_lsp::Position {
                        line: 0,
                        character: 3,
                    },
                    new_text: "new".to_string(),
                }],
            ),
            (
                b.clone(),
                vec![lide_lsp::client::ServerTextEdit {
                    start: lide_lsp::Position {
                        line: 0,
                        character: 0,
                    },
                    end: lide_lsp::Position {
                        line: 0,
                        character: 4,
                    },
                    new_text: "renamed".to_string(),
                }],
            ),
        ];
        workspace.apply_workspace_edit(&edits);

        assert_eq!(std::fs::read_to_string(&a).unwrap(), "new name\n");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "renamed and old\n");

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_workspace_edit_edits_open_documents_in_place() {
        let mut workspace = workspace_with_text("let old = 1;");
        let path = std::env::temp_dir().join("lide-rename-open.txt");
        std::fs::write(&path, "let old = 1;").unwrap();
        if let Some(editor) = workspace.active_editor_mut() {
            editor.location = Some(lide_resource::Location::local(path.clone()));
        }

        workspace.apply_workspace_edit(&[(
            path.clone(),
            vec![lide_lsp::client::ServerTextEdit {
                start: lide_lsp::Position {
                    line: 0,
                    character: 4,
                },
                end: lide_lsp::Position {
                    line: 0,
                    character: 7,
                },
                new_text: "renamed".to_string(),
            }],
        )]);

        assert_eq!(
            workspace.active_document().and_then(|d| d.text()),
            Some("let renamed = 1;"),
            "the open document is changed, not reloaded from disk"
        );
        assert!(workspace.active_document().unwrap().is_dirty());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_latex_document_prefers_main_tex() {
        let directory = std::env::temp_dir().join("lide-latex-pick");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("chapters")).unwrap();
        std::fs::write(directory.join("appendix.tex"), "").unwrap();
        std::fs::write(directory.join("chapters/one.tex"), "").unwrap();
        std::fs::write(directory.join("main.tex"), "").unwrap();

        let mut workspace = Workspace::new();
        workspace.root = Some(directory.clone());
        assert_eq!(
            workspace.latex_document(),
            Some(directory.join("main.tex")),
            "main.tex wins over the alphabetically earlier files"
        );

        std::fs::remove_file(directory.join("main.tex")).unwrap();
        assert!(
            workspace.latex_document().is_some(),
            "with no main.tex any .tex file will do"
        );

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn the_clicked_word_picks_the_source_line_among_the_nearby_ones() {
        let text = "Alpha beta gamma\ndelta epsilon zeta\neta theta iota\nkappa lambda mu\n";
        let words = ("epsilon".to_string(), "zeta".to_string(), String::new());
        // SyncTeX said line 4 (the paragraph's last line); the word is on 2.
        let (line, column) = refine_line(text, 4, 0, &words);
        assert_eq!(line, 2);
        assert_eq!(column, "delta epsilon zeta".find("zeta").unwrap());
        // A word that is nowhere near keeps SyncTeX's answer.
        let none = (String::new(), "omega".to_string(), String::new());
        assert_eq!(refine_line(text, 3, 0, &none), (3, 0));
    }

    #[test]
    fn two_presses_close_together_are_a_double_click() {
        let mut workspace = Workspace::new();
        workspace.now = 1.0;
        workspace.note_pdf_press(0, 100.0, 200.0);
        assert_eq!(workspace.pdf_double_click, None, "one press is not a pair");
        workspace.now = 1.1;
        workspace.note_pdf_press(0, 104.0, 202.0);
        assert_eq!(
            workspace.pdf_double_click,
            Some(0),
            "a second press nearby is a double click"
        );
    }

    #[test]
    fn two_presses_apart_are_not_a_double_click() {
        let mut workspace = Workspace::new();
        workspace.now = 1.0;
        workspace.note_pdf_press(0, 100.0, 200.0);
        workspace.now = 2.0;
        workspace.note_pdf_press(0, 104.0, 202.0);
        assert_eq!(workspace.pdf_double_click, None, "the presses were seconds apart");
    }

    #[test]
    fn an_included_chapter_builds_its_root_document() {
        let directory = std::env::temp_dir().join("lide-latex-root-pick");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("chapters")).unwrap();
        std::fs::write(
            directory.join("paper.tex"),
            "\\documentclass{article}\n\\begin{document}\n\\input{chapters/one}\n\\end{document}\n",
        )
        .unwrap();
        let chapter = directory.join("chapters/one.tex");
        std::fs::write(&chapter, "Chapter text.\n").unwrap();

        let mut workspace = Workspace::new();
        workspace.root = Some(directory.clone());
        workspace.open_path(&chapter).expect("the chapter opens");
        assert_eq!(
            workspace.latex_document(),
            Some(directory.join("paper.tex")),
            "the included chapter is not the build target"
        );

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_project_with_no_tex_has_nothing_to_build() {
        let directory = std::env::temp_dir().join("lide-latex-none");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("notes.md"), "").unwrap();

        let mut workspace = Workspace::new();
        workspace.root = Some(directory.clone());
        assert_eq!(workspace.latex_document(), None);

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn replacing_a_range_rebuilds_the_text() {
        assert_eq!(
            replace_range("hello world", 0..5, "goodbye"),
            "goodbye world"
        );
        assert_eq!(replace_range("hello", 5..5, "!"), "hello!");
    }

    #[test]
    fn a_new_workspace_has_no_documents() {
        let workspace = Workspace::new();
        assert!(workspace.documents.is_empty());
        assert!(workspace.active_document().is_none());
        assert!(!workspace.has_unsaved_changes());
    }

    #[test]
    fn splitting_needs_a_second_document() {
        let mut workspace = Workspace::new();
        workspace.toggle_split();
        assert!(workspace.split_document.is_none());
        assert!(workspace.notice.is_some());
    }

    fn push_text(workspace: &mut Workspace, text: &str) {
        let mut editor = lide_editor::Editor::empty();
        editor.edit.replace_text(text);
        workspace.documents.push(Document::Text(editor));
    }

    #[test]
    fn a_tab_click_can_fill_the_right_pane() {
        let mut workspace = workspace_with_text("left");
        push_text(&mut workspace, "right");
        push_text(&mut workspace, "other");
        workspace.active = 0;
        workspace.split_document = Some(1);
        workspace.focused_pane = 1;

        workspace.select_tab(2);
        assert_eq!(workspace.active, 0, "the left pane stays put");
        assert_eq!(
            workspace.split_document,
            Some(2),
            "the tab opened in the right pane"
        );

        // Picking the document that is already on the left swaps the panes,
        // because one document is shown once.
        workspace.select_tab(0);
        assert_eq!(workspace.active, 2);
        assert_eq!(workspace.split_document, Some(0));
    }

    #[test]
    fn a_tab_click_on_the_left_replaces_the_left_pane() {
        let mut workspace = workspace_with_text("left");
        push_text(&mut workspace, "right");
        push_text(&mut workspace, "other");
        workspace.active = 0;
        workspace.split_document = Some(1);
        workspace.focused_pane = 0;

        workspace.select_tab(2);
        assert_eq!(workspace.active, 2);
        assert_eq!(
            workspace.split_document,
            Some(1),
            "the right pane is not disturbed"
        );
    }

    #[test]
    fn focusing_a_pane_without_a_split_stays_left() {
        let mut workspace = workspace_with_text("only");
        workspace.focus_pane(1);
        assert_eq!(workspace.focused_pane, 0);
    }

    #[test]
    fn closing_the_right_pane_falls_back_to_the_left() {
        let mut workspace = workspace_with_text("left");
        push_text(&mut workspace, "right");
        workspace.settings.split = false;
        workspace.active = 0;
        workspace.split_document = Some(1);
        workspace.focused_pane = 1;

        workspace.close(1);
        assert_eq!(workspace.split_document, None);
        assert_eq!(workspace.focused_pane, 0, "a lost pane is not focused");
    }

    #[test]
    fn the_theme_toggles() {
        let mut workspace = Workspace::new();
        let before = workspace.settings.dark;
        workspace.toggle_theme();
        assert_eq!(workspace.settings.dark, !before);
        workspace.settings.dark = before;
    }
}
