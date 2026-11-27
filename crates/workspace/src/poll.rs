//! The per-frame work.
//!
//! Everything the editor collects while it waits — the pages the PDF viewer
//! rendered, the files that changed on disk, the answers the language servers
//! sent, the results a search found — arrives here, once per frame.
//!
//! These are apart from the rest of the workspace because they are all "ask
//! the outside world what happened" methods: they read a channel, a
//! subprocess or a watcher, and they change nothing but the state they
//! report.

use super::{Document, Workspace};

impl Workspace {
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

    /// Collect the remote open, if it has finished.
    pub(crate) fn poll_remote_open(&mut self) -> bool {
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

    /// Let the image viewer decode what it has been given.
    pub(crate) fn poll_images(&mut self) -> bool {
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
    pub(crate) fn poll_build(&mut self) -> bool {
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
    pub(crate) fn poll_terminals(&mut self) -> bool {
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

    /// Ask for completions when typing moved the caret to the end of a word.
    ///
    /// This is what makes completion feel automatic without asking the server
    /// on every frame: a request goes out once per *edit*, only when the
    /// character before the caret is part of a word, and no more often than
    /// [`COMPLETION_DEBOUNCE`]. Moving the caret by clicking is not an edit and
    /// does not open the list.
    pub(crate) fn poll_completions(&mut self) -> bool {
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

    /// Keep the language servers in step with the open documents.
    pub(crate) fn poll_servers(&mut self) -> bool {
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

    /// Collect the replace worker's progress, and put the results into the
    /// open documents when it finishes.
    pub(crate) fn poll_replace(&mut self) -> bool {
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

    pub(crate) fn poll_pdfs(&mut self) -> bool {
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
    pub(crate) fn poll_source_click(&mut self) -> bool {
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
    pub(crate) fn poll_pdf_double_click(&mut self) -> bool {
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
    pub(crate) fn poll_files(&mut self) -> bool {
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
}
