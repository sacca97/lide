//! The terminal documents, and the keys and text they carry.
//!
//! A terminal is a document like any other: it opens in a tab, it is sized
//! to the area it is drawn in, and it takes the keyboard while it is in
//! front. What is particular to it is the PTY behind it and the search over
//! its scrollback.

use std::path::PathBuf;

use super::{Document, TerminalFind, Workspace};

impl Workspace {
    /// Open a terminal running `ssh host`, starting in the configured
    /// terminal directory.
    pub fn open_ssh_terminal(&mut self, host: &str) {
        let directory = self.terminal_start_directory();
        let size = self.terminal_size();
        match crate::terminal::TerminalView::open_ssh(host, &directory.to_string_lossy(), size) {
            Ok(terminal) => {
                self.documents.push(Document::Terminal(Box::new(terminal)));
                self.active = self.documents.len() - 1;
            }
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    pub fn open_terminal(&mut self) -> Result<(), String> {
        let directory = self.terminal_start_directory();
        let shell = if self.settings.terminal_shell.trim().is_empty() {
            crate::terminal::default_shell()
        } else {
            self.settings.terminal_shell.trim().to_string()
        };
        let size = self.terminal_size();
        let terminal =
            crate::terminal::TerminalView::open(&shell, &directory.to_string_lossy(), size)
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
    pub(crate) fn terminal_start_directory(&self) -> PathBuf {
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

    pub(crate) fn terminal_find_step(&mut self, step: i32) {
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
    pub fn write_terminal_key(&mut self, key: &crate::terminal::Key) {
        if let Some(terminal) = self.active_terminal_mut() {
            terminal.write_key(key);
        }
    }

    /// The terminal's size in cells, from the area it was last drawn in.
    pub(crate) fn terminal_size(&self) -> crate::terminal::Size {
        let advance = self.settings.font_size * 0.62;
        let line_height = self.settings.line_height();
        let rect = self.terminal_rect.get();
        let (width, height) = rect
            .map(|rect| (rect[2], rect[3]))
            .unwrap_or((900.0, 500.0));
        crate::terminal::Size {
            cols: ((width - 12.0) / advance).max(20.0) as usize,
            rows: ((height - 12.0) / line_height).max(5.0) as usize,
        }
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

    pub fn active_terminal_mut(&mut self) -> Option<&mut crate::terminal::TerminalView> {
        match self.active_document_mut()? {
            Document::Terminal(terminal) => Some(terminal),
            _ => None,
        }
    }
}
