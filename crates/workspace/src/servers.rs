//! Language servers, as the workspace uses them.
//!
//! A server is started on a background thread: the handshake can take
//! seconds with a large project, and the window must not wait for it. Once it
//! is up, documents are opened, changed and closed through it, and what it
//! publishes becomes the editor's diagnostics.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use lide_lsp::client::{CompletionItem, ServerTextEdit};
use lide_lsp::{server_for, Client, Diagnostic, Event, Position, ServerConfig};

/// What a request the editor made came back with.
#[derive(Debug)]
pub enum Outcome {
    /// Where to jump to.
    Definition(PathBuf, Position),
    /// The hover text.
    Hover(String),
    /// Where a symbol is used.
    References(Vec<(PathBuf, Position)>),
    /// Edits that format the document.
    Format(Vec<ServerTextEdit>),
    /// What can be completed here.
    Completions(Vec<CompletionItem>),
    /// The edits that rename a symbol.
    Rename(Vec<(PathBuf, Vec<ServerTextEdit>)>),
    /// What the server offers to do at the caret.
    CodeActions(Vec<lide_lsp::client::CodeAction>),
    /// The server does not offer this.
    Unsupported(&'static str),
}

/// A request that is waiting for its answer.
#[derive(Debug, Clone, Copy)]
enum Pending {
    Definition,
    Hover,
    References,
    Format,
    Completion,
    Rename,
    CodeActions,
}

/// Where a server is in its life.
enum State {
    /// The handshake is running on another thread.
    Starting(mpsc::Receiver<Result<Box<Client>, String>>),
    /// The server is up.
    Ready(Box<Client>),
    /// It could not be started, and why.
    Failed(String),
}

/// One server, and the state it is in.
struct Entry {
    config: &'static ServerConfig,
    state: State,
}

/// The language servers the workspace has started.
#[derive(Default)]
pub struct Servers {
    entries: Vec<Entry>,
    /// Every diagnostic the servers have published, by file.
    diagnostics: HashMap<PathBuf, Vec<Diagnostic>>,
    /// Documents the servers have been told about, and the text last sent.
    sent: HashMap<PathBuf, String>,
    /// The version number to send with the next change, by file.
    versions: HashMap<PathBuf, i64>,
    /// The requests waiting for an answer, by id.
    pending: HashMap<i64, Pending>,
    /// The documents each server has already been told about.
    opened: HashSet<(String, PathBuf)>,
}

impl std::fmt::Debug for Servers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Servers")
            .field("entries", &self.entries.len())
            .field("files_with_diagnostics", &self.diagnostics.len())
            .finish_non_exhaustive()
    }
}

impl Servers {
    /// Make sure a server is running for `path`, and tell it the document is
    /// open.
    pub fn open(&mut self, root: &Path, path: &Path, text: &str) {
        let Some(config) = server_for(&path.to_string_lossy()) else {
            return;
        };

        let index = match self
            .entries
            .iter()
            .position(|e| e.config.command == config.command)
        {
            Some(index) => index,
            None => {
                let (tx, rx) = mpsc::channel();
                let root = root.to_path_buf();
                let config_ptr: &'static ServerConfig = config;
                std::thread::spawn(move || {
                    let result = Client::start(config_ptr, &root).map(Box::new);
                    let _ = tx.send(result);
                });
                self.entries.push(Entry {
                    config,
                    state: State::Starting(rx),
                });
                self.entries.len() - 1
            }
        };

        // Remember what the server has been told, so that `poll` sends only
        // the changes it does not know about yet.
        self.sent.insert(path.to_path_buf(), text.to_string());
        self.versions.insert(path.to_path_buf(), 1);

        // If the server is already up, open the document now; otherwise the
        // pending documents are opened when the handshake finishes.
        if let State::Ready(client) = &mut self.entries[index].state {
            client.did_open(path, text);
        }
    }

    /// Tell the servers a document was closed.
    pub fn close(&mut self, path: &Path) {
        for entry in &mut self.entries {
            if let State::Ready(client) = &mut entry.state {
                client.did_close(path);
            }
        }
        self.sent.remove(path);
        self.versions.remove(path);
        self.diagnostics.remove(path);
        self.opened.retain(|(_, opened)| opened != path);
    }

    /// Send the changes the servers have not seen yet.
    pub fn sync(&mut self, documents: &[(PathBuf, String)]) {
        for (path, text) in documents {
            let previous = self.sent.get(path);
            if previous == Some(text) {
                continue;
            }
            let version = self.versions.entry(path.clone()).or_insert(1);
            *version += 1;
            let version = *version;

            let config = server_for(&path.to_string_lossy());
            let Some(config) = config else { continue };
            for entry in &mut self.entries {
                if entry.config.command != config.command {
                    continue;
                }
                if let State::Ready(client) = &mut entry.state {
                    if previous.is_none() {
                        client.did_open(path, text);
                    } else {
                        client.did_change(path, text, version);
                    }
                }
            }
            self.sent.insert(path.clone(), text.clone());
        }
    }

    /// Ask for the definition at a position.
    pub fn request_definition(&mut self, path: &Path, position: Position) -> bool {
        self.request(path, Pending::Definition, |client| {
            client.definition(path, position)
        })
    }

    /// Ask what is at a position.
    pub fn request_hover(&mut self, path: &Path, position: Position) -> bool {
        self.request(path, Pending::Hover, |client| client.hover(path, position))
    }

    /// Ask where a symbol is used.
    pub fn request_references(&mut self, path: &Path, position: Position) -> bool {
        self.request(path, Pending::References, |client| {
            client.references(path, position, true)
        })
    }

    /// Ask what the server offers to do at a range.
    pub fn request_code_actions(&mut self, path: &Path, start: Position, end: Position) -> bool {
        self.request(path, Pending::CodeActions, |client| {
            client.code_actions(path, start, end, &[])
        })
    }

    /// Ask the server to rename a symbol.
    pub fn request_rename(&mut self, path: &Path, position: Position, new_name: &str) -> bool {
        self.request(path, Pending::Rename, |client| {
            client.rename(path, position, new_name)
        })
    }

    /// Ask what can be completed at a position.
    pub fn request_completion(&mut self, path: &Path, position: Position) -> bool {
        self.request(path, Pending::Completion, |client| {
            client.completion(path, position)
        })
    }

    /// Ask the server to format a document.
    pub fn request_format(&mut self, path: &Path) -> bool {
        self.request(path, Pending::Format, |client| client.formatting(path))
    }

    /// Send a request to the first server that accepts it.
    fn request(
        &mut self,
        _path: &Path,
        kind: Pending,
        send: impl Fn(&mut Client) -> Option<i64>,
    ) -> bool {
        for entry in &mut self.entries {
            if let State::Ready(client) = &mut entry.state {
                if let Some(id) = send(client) {
                    self.pending.insert(id, kind);
                    return true;
                }
            }
        }
        false
    }

    /// Collect everything the servers said, and finish any handshakes.
    pub fn poll(&mut self) -> Vec<Outcome> {
        // Finish the handshakes that completed.
        for entry in &mut self.entries {
            let finished = match &entry.state {
                State::Starting(rx) => match rx.try_recv() {
                    Ok(Ok(client)) => Some(Ok(client)),
                    Ok(Err(error)) => Some(Err(error)),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some(Err("the server thread stopped".to_string()))
                    }
                },
                _ => None,
            };
            match finished {
                Some(Ok(client)) => {
                    log::info!("{} is ready", entry.config.command);
                    entry.state = State::Ready(client);
                }
                Some(Err(error)) => {
                    // A language server is optional: a missing binary is a
                    // normal configuration, not an editor warning.
                    if error.contains("No such file or directory") {
                        log::info!("{error}");
                    } else {
                        log::warn!("{error}");
                    }
                    entry.state = State::Failed(error);
                }
                None => {}
            }
        }

        // Open the documents a server has not been told about yet — those it
        // was starting for, and anything opened while the handshake ran.
        let documents: Vec<(PathBuf, String)> = self
            .sent
            .iter()
            .map(|(path, text)| (path.clone(), text.clone()))
            .collect();
        for entry in &mut self.entries {
            let State::Ready(client) = &mut entry.state else {
                continue;
            };
            for (path, text) in &documents {
                let Some(config) = server_for(&path.to_string_lossy()) else {
                    continue;
                };
                if config.command != entry.config.command {
                    continue;
                }
                let key = (entry.config.command.to_string(), path.clone());
                if self.opened.contains(&key) {
                    continue;
                }
                client.did_open(path, text);
                self.opened.insert(key);
            }
        }

        // Drain the servers.
        let mut outcomes = Vec::new();
        for entry in &mut self.entries {
            let State::Ready(client) = &mut entry.state else {
                continue;
            };
            for event in client.poll() {
                match event {
                    Event::Diagnostics { path, diagnostics } => {
                        self.diagnostics.insert(path, diagnostics);
                    }
                    Event::Response { id, result } => {
                        let Some(kind) = self.pending.remove(&id) else {
                            continue;
                        };
                        match kind {
                            Pending::Definition => {
                                if let Some((path, position)) =
                                    lide_lsp::client::definition_locations(&result)
                                        .into_iter()
                                        .next()
                                {
                                    outcomes.push(Outcome::Definition(path, position));
                                }
                            }
                            Pending::Hover => match lide_lsp::client::hover_text(&result) {
                                Some(text) => outcomes.push(Outcome::Hover(text)),
                                None => outcomes.push(Outcome::Hover(String::new())),
                            },
                            Pending::References => {
                                let locations = lide_lsp::client::reference_locations(&result);
                                outcomes.push(Outcome::References(locations));
                            }
                            Pending::Format => {
                                outcomes
                                    .push(Outcome::Format(lide_lsp::client::text_edits(&result)));
                            }
                            Pending::CodeActions => {
                                outcomes.push(Outcome::CodeActions(
                                    lide_lsp::client::code_actions(&result),
                                ));
                            }
                            Pending::Rename => {
                                outcomes.push(Outcome::Rename(lide_lsp::client::workspace_edit(
                                    &result,
                                )));
                            }
                            Pending::Completion => {
                                outcomes.push(Outcome::Completions(
                                    lide_lsp::client::completion_items(&result),
                                ));
                            }
                        }
                    }
                    Event::Error(error) => log::warn!("language server: {error}"),
                    Event::Exited => {
                        log::warn!("{} exited", entry.config.command);
                    }
                }
            }
        }
        outcomes
    }

    /// The diagnostics for a file.
    pub fn diagnostics_for(&self, path: &Path) -> &[Diagnostic] {
        self.diagnostics
            .get(path)
            .map(|items| items.as_slice())
            .unwrap_or_default()
    }

    /// Every diagnostic, in file order.
    pub fn all_diagnostics(&self) -> Vec<Diagnostic> {
        let mut all: Vec<Diagnostic> = self
            .diagnostics
            .values()
            .flat_map(|items| items.iter().cloned())
            .collect();
        all.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.start.line.cmp(&b.start.line))
                .then(a.start.character.cmp(&b.start.character))
        });
        all
    }

    /// Put diagnostics in as if a server had published them. For headless
    /// snapshots and tests, which have no server to talk to.
    #[doc(hidden)]
    pub fn seed_diagnostics(&mut self, path: PathBuf, items: Vec<Diagnostic>) {
        self.diagnostics.insert(path, items);
    }

    /// How many errors and warnings the servers have reported.
    pub fn problem_count(&self) -> (usize, usize) {
        let mut errors = 0;
        let mut warnings = 0;
        for items in self.diagnostics.values() {
            for diagnostic in items {
                match diagnostic.severity {
                    lide_lsp::Severity::Error => errors += 1,
                    lide_lsp::Severity::Warning => warnings += 1,
                    _ => {}
                }
            }
        }
        (errors, warnings)
    }

    /// A one-line description of the server state, for the status bar.
    pub fn status(&self) -> Option<String> {
        for entry in &self.entries {
            if let State::Failed(error) = &entry.state {
                return Some(format!("{}: {error}", entry.config.language));
            }
        }
        let ready = self
            .entries
            .iter()
            .filter(|entry| matches!(entry.state, State::Ready(_)))
            .count();
        if ready > 0 {
            Some(if ready == 1 {
                "1 language server".to_string()
            } else {
                format!("{ready} language servers")
            })
        } else {
            None
        }
    }

    /// Stop every server.
    pub fn shutdown(&mut self) {
        for entry in &mut self.entries {
            if let State::Ready(client) = &mut entry.state {
                client.shutdown();
            }
        }
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_no_server_handles_starts_nothing() {
        let mut servers = Servers::default();
        servers.open(Path::new("/tmp"), Path::new("/tmp/notes.md"), "# hi");
        assert!(
            servers.status().is_none(),
            "no server should have been started"
        );
        assert!(servers.all_diagnostics().is_empty());
        assert_eq!(servers.problem_count(), (0, 0));
    }

    #[test]
    fn polling_an_empty_set_is_harmless() {
        let mut servers = Servers::default();
        assert!(servers.poll().is_empty());
    }

    #[test]
    fn syncing_with_no_servers_records_nothing() {
        let mut servers = Servers::default();
        servers.sync(&[(PathBuf::from("/tmp/a.md"), "text".to_string())]);
        assert!(
            servers.sent.is_empty(),
            "a file no server handles is not tracked"
        );
    }

    #[test]
    fn closing_a_file_forgets_its_diagnostics() {
        let mut servers = Servers::default();
        let path = PathBuf::from("/tmp/a.rs");
        servers.diagnostics.insert(path.clone(), Vec::new());
        servers.sent.insert(path.clone(), "text".to_string());
        servers.close(&path);
        assert!(servers.diagnostics_for(&path).is_empty());
        assert!(!servers.sent.contains_key(&path));
    }
}
