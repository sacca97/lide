//! The client: one running server, and the conversation with it.

use std::collections::VecDeque;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::server::ServerConfig;
use crate::transport::{path_to_uri, read_message, uri_to_path, write_message};

/// How long to wait for a server to answer `initialize`.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// A compile error.
    Error,
    /// Something that is probably wrong.
    Warning,
    /// A suggestion.
    Information,
    /// A hint.
    Hint,
}

impl Severity {
    fn from_lsp(value: Option<&Value>) -> Severity {
        match value.and_then(Value::as_u64) {
            Some(1) => Severity::Error,
            Some(2) => Severity::Warning,
            Some(3) => Severity::Information,
            _ => Severity::Hint,
        }
    }

    /// Whether this diagnostic should count as a problem in the status bar.
    pub fn is_problem(self) -> bool {
        matches!(self, Severity::Error | Severity::Warning)
    }
}

/// A position in a document, in LSP's terms: a line and a UTF-16 column,
/// both counted from zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// The line.
    pub line: u64,
    /// The character offset within the line, in UTF-16 code units.
    pub character: u64,
}

/// One diagnostic from a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The file the diagnostic is about.
    pub path: PathBuf,
    /// Where it starts.
    pub start: Position,
    /// Where it ends.
    pub end: Position,
    /// How serious it is.
    pub severity: Severity,
    /// What the server says.
    pub message: String,
    /// The tool that reported it.
    pub source: String,
}

/// Something the server said that the editor cares about.
#[derive(Debug)]
pub enum Event {
    /// The server published diagnostics for a file.
    Diagnostics {
        /// The file.
        path: PathBuf,
        /// Everything it reported for that file.
        diagnostics: Vec<Diagnostic>,
    },
    /// A response to a request the editor made.
    Response {
        /// The request id.
        id: i64,
        /// The result, as sent.
        result: Value,
    },
    /// The server said something went wrong.
    Error(String),
    /// The server's stream ended.
    Exited,
}

/// A running language server.
pub struct Client {
    /// The server this client talks to.
    pub config: &'static ServerConfig,
    /// The folder the server was started in.
    pub root: PathBuf,
    /// What the server says its capabilities are.
    pub capabilities: Value,

    outgoing: mpsc::Sender<String>,
    incoming: mpsc::Receiver<Value>,
    next_id: i64,
    /// Messages that arrived before the caller started polling.
    pending: VecDeque<Value>,
    child: Option<Child>,
    writer: Option<std::thread::JoinHandle<()>>,
    reader: Option<std::thread::JoinHandle<()>>,
    /// Whether the server has exited.
    pub dead: bool,
    /// The server's standard error, collected when the stream ends.
    stderr: mpsc::Receiver<String>,
    /// The last thing the server said on standard error.
    last_stderr: std::sync::Mutex<String>,
}

/// The `textDocument`/`position` pair several requests share.
fn text_document_position(path: &Path, position: Position) -> Value {
    json!({
        "textDocument": { "uri": path_to_uri(&path.to_string_lossy()) },
        "position": { "line": position.line, "character": position.character }
    })
}

/// Parse one LSP position.
fn parse_position(value: &Value) -> Option<Position> {
    Some(Position {
        line: value.get("line")?.as_u64()?,
        character: value.get("character")?.as_u64()?,
    })
}

/// Parse an LSP `range` into its two positions.
fn parse_range(range: &Value) -> Option<(Position, Position)> {
    Some((parse_position(range.get("start")?)?, parse_position(range.get("end")?)?))
}

impl Client {
    /// An error message with whatever the server said on standard error.
    fn exit_error(&mut self, reason: &str) -> String {
        // The stderr reader sends when the stream closes, which happens as
        // the process exits; wait once rather than polling on a timer.
        if let Ok(text) = self.stderr.recv_timeout(Duration::from_millis(200)) {
            if let Ok(mut stored) = self.last_stderr.lock() {
                *stored = text;
            }
        }
        let stderr = self.stderr();
        let first = stderr
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_string();
        if first.is_empty() {
            format!("{} {reason}", self.config.command)
        } else {
            format!("{} {reason}: {first}", self.config.command)
        }
    }

    /// What the server wrote to standard error, if anything.
    pub fn stderr(&self) -> String {
        if let Ok(text) = self.stderr.try_recv() {
            if let Ok(mut stored) = self.last_stderr.lock() {
                *stored = text;
            }
        }
        self.last_stderr
            .lock()
            .map(|text| text.clone())
            .unwrap_or_default()
    }
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("server", &self.config.command)
            .field("root", &self.root)
            .field("dead", &self.dead)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Start the server for `config` in `root` and perform the handshake.
    pub fn start(config: &'static ServerConfig, root: &Path) -> Result<Self, String> {
        let mut child = Command::new(config.command)
            .args(config.arguments())
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("could not start {}: {error}", config.command))?;

        let stdin = child.stdin.take().ok_or("the server has no stdin")?;
        let stdout = child.stdout.take().ok_or("the server has no stdout")?;
        let stderr = child.stderr.take();

        // Keep the server's own complaints: when it exits during the
        // handshake, its stderr is the only thing that says why.
        let (error_tx, error_rx) = mpsc::channel::<String>();
        if let Some(stderr) = stderr {
            std::thread::spawn(move || {
                use std::io::BufRead;
                let mut collected = String::new();
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if collected.len() < 4096 {
                        collected.push_str(&line);
                        collected.push('\n');
                    }
                }
                let _ = error_tx.send(collected);
            });
        }

        let (out_tx, out_rx) = mpsc::channel::<String>();
        let (in_tx, in_rx) = mpsc::channel::<Value>();

        let writer = std::thread::spawn(move || {
            let mut stdin = stdin;
            while let Ok(body) = out_rx.recv() {
                if write_message(&mut stdin, &body).is_err() {
                    break;
                }
                let _ = stdin.flush();
            }
        });

        let reader = std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_message(&mut reader) {
                    Ok(Some(body)) => match serde_json::from_str::<Value>(&body) {
                        Ok(value) => {
                            if in_tx.send(value).is_err() {
                                break;
                            }
                        }
                        Err(_) => continue, // a message we cannot parse is not fatal
                    },
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
            // Dropping the sender tells the client the stream ended.
        });

        let mut client = Self {
            config,
            root: root.to_path_buf(),
            capabilities: Value::Null,
            outgoing: out_tx,
            incoming: in_rx,
            next_id: 1,
            pending: VecDeque::new(),
            child: Some(child),
            writer: Some(writer),
            reader: Some(reader),
            last_stderr: std::sync::Mutex::new(String::new()),
            stderr: error_rx,
            dead: false,
        };

        client.handshake()?;
        Ok(client)
    }

    /// The `initialize` / `initialized` exchange.
    fn handshake(&mut self) -> Result<(), String> {
        let root_uri = path_to_uri(&self.root.to_string_lossy());
        let id = self.send_request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "clientInfo": { "name": "lide", "version": env!("CARGO_PKG_VERSION") },
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "synchronization": { "dynamicRegistration": false },
                        "definition": { "linkSupport": false },
                        "publishDiagnostics": { "relatedInformation": false }
                    }
                },
                "workspaceFolders": [{ "uri": root_uri, "name": "workspace" }]
            }),
        );

        let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(self.exit_error("did not answer the handshake"));
            }
            match self.incoming.recv_timeout(remaining.min(Duration::from_millis(200))) {
                Ok(value) => {
                    if value.get("id").and_then(Value::as_i64) == Some(id) {
                        if let Some(error) = value.get("error") {
                            return Err(format!("{} refused to start: {error}", self.config.command));
                        }
                        self.capabilities = value
                            .get("result")
                            .and_then(|result| result.get("capabilities"))
                            .cloned()
                            .unwrap_or(Value::Null);
                        break;
                    }
                    // Anything else that arrives during the handshake is kept
                    // for the next poll.
                    self.pending.push_back(value);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.dead = true;
                    return Err(self.exit_error("exited during the handshake"));
                }
            }
        }

        self.send_notification("initialized", json!({}));
        Ok(())
    }

    /// Whether the server said it supports going to a definition.
    fn supports_definition(&self) -> bool {
        self.supports("definitionProvider")
    }

    /// Whether the server advertises `capability`.
    fn supports(&self, capability: &str) -> bool {
        self.capabilities
            .get(capability)
            .map(|value| value.as_bool().unwrap_or(!value.is_null()))
            .unwrap_or(false)
    }

    /// Ask what is under the cursor.
    pub fn hover(&mut self, path: &Path, position: Position) -> Option<i64> {
        if !self.supports("hoverProvider") {
            return None;
        }
        Some(self.send_request(
            "textDocument/hover",
            text_document_position(path, position),
        ))
    }

    /// Ask where a symbol is used.
    pub fn references(
        &mut self,
        path: &Path,
        position: Position,
        include_declaration: bool,
    ) -> Option<i64> {
        if !self.supports("referencesProvider") {
            return None;
        }
        Some(self.send_request(
            "textDocument/references",
            {
                let mut value = text_document_position(path, position);
                value["context"] = json!({ "includeDeclaration": include_declaration });
                value
            },
        ))
    }

    /// Ask what can be completed at a position.
    pub fn completion(&mut self, path: &Path, position: Position) -> Option<i64> {
        if !self.supports("completionProvider") {
            return None;
        }
        Some(self.send_request(
            "textDocument/completion",
            text_document_position(path, position),
        ))
    }

    /// Ask what can be done at a range — the quick fixes for a line.
    pub fn code_actions(
        &mut self,
        path: &Path,
        start: Position,
        end: Position,
        diagnostics: &[Value],
    ) -> Option<i64> {
        if !self.supports("codeActionProvider") {
            return None;
        }
        Some(self.send_request(
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": path_to_uri(&path.to_string_lossy()) },
                "range": {
                    "start": { "line": start.line, "character": start.character },
                    "end": { "line": end.line, "character": end.character }
                },
                "context": { "diagnostics": diagnostics }
            }),
        ))
    }

    /// Ask the server to rename the symbol at a position.
    pub fn rename(&mut self, path: &Path, position: Position, new_name: &str) -> Option<i64> {
        if !self.supports("renameProvider") {
            return None;
        }
        Some(self.send_request(
            "textDocument/rename",
            {
                let mut value = text_document_position(path, position);
                value["newName"] = json!(new_name);
                value
            },
        ))
    }

    /// Ask the server to format a whole document.
    pub fn formatting(&mut self, path: &Path) -> Option<i64> {
        if !self.supports("documentFormattingProvider") {
            return None;
        }
        Some(self.send_request(
            "textDocument/formatting",
            json!({
                "textDocument": { "uri": path_to_uri(&path.to_string_lossy()) },
                "options": { "tabSize": 4, "insertSpaces": true }
            }),
        ))
    }

    /// Tell the server a document is open.
    pub fn did_open(&mut self, path: &Path, text: &str) {
        self.send_notification(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": path_to_uri(&path.to_string_lossy()),
                    "languageId": self.config.language_id,
                    "version": 1,
                    "text": text,
                }
            }),
        );
    }

    /// Tell the server a document changed.
    ///
    /// The whole text is sent rather than a range: the protocol allows it
    /// whatever sync kind the server asked for, and it cannot drift.
    pub fn did_change(&mut self, path: &Path, text: &str, version: i64) {
        self.send_notification(
            "textDocument/didChange",
            json!({
                "textDocument": {
                    "uri": path_to_uri(&path.to_string_lossy()),
                    "version": version,
                },
                "contentChanges": [{ "text": text }]
            }),
        );
    }

    /// Tell the server a document was closed.
    pub fn did_close(&mut self, path: &Path) {
        self.send_notification(
            "textDocument/didClose",
            json!({
                "textDocument": { "uri": path_to_uri(&path.to_string_lossy()) }
            }),
        );
    }

    /// Ask where a position is defined.
    ///
    /// Returns the request id, which the response carries back.
    pub fn definition(&mut self, path: &Path, position: Position) -> Option<i64> {
        if !self.supports_definition() {
            return None;
        }
        Some(self.send_request(
            "textDocument/definition",
            text_document_position(path, position),
        ))
    }

    /// Ask the server to stop.
    pub fn shutdown(&mut self) {
        let id = self.send_request("shutdown", Value::Null);
        self.send_notification("exit", Value::Null);
        let _ = id;

        if let Some(mut child) = self.child.take() {
            // A server that ignores `exit` is killed after a grace period.
            let deadline = Instant::now() + Duration::from_millis(500);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                }
            }
        }
        self.dead = true;
    }

    /// Collect everything the server said since the last call.
    pub fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();

        while let Some(value) = self.pending.pop_front() {
            if let Some(event) = self.classify(&value) {
                events.push(event);
            }
        }

        loop {
            match self.incoming.try_recv() {
                Ok(value) => {
                    if let Some(event) = self.classify(&value) {
                        events.push(event);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if !self.dead {
                        self.dead = true;
                        events.push(Event::Exited);
                    }
                    break;
                }
            }
        }

        events
    }

    /// Turn one message from the server into an [`Event`].
    fn classify(&self, value: &Value) -> Option<Event> {
        // A response to something the editor asked for.
        if let Some(id) = value.get("id").and_then(Value::as_i64) {
            if value.get("method").is_none() {
                if let Some(error) = value.get("error") {
                    return Some(Event::Error(error.to_string()));
                }
                return Some(Event::Response {
                    id,
                    result: value.get("result").cloned().unwrap_or(Value::Null),
                });
            }
            // A request *from* the server. None are needed yet; answering
            // nothing is better than answering wrongly.
            return None;
        }

        let method = value.get("method")?.as_str()?;
        if method != "textDocument/publishDiagnostics" {
            return None;
        }

        let params = value.get("params")?;
        let path = PathBuf::from(uri_to_path(params.get("uri")?.as_str()?));
        let raw = params.get("diagnostics")?.as_array()?;

        let diagnostics = raw
            .iter()
            .filter_map(|diagnostic| {
                let (start, end) = parse_range(diagnostic.get("range")?)?;
                Some(Diagnostic {
                    path: path.clone(),
                    start,
                    end,
                    severity: Severity::from_lsp(diagnostic.get("severity")),
                    message: diagnostic
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    source: diagnostic
                        .get("source")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                })
            })
            .collect();

        Some(Event::Diagnostics { path, diagnostics })
    }

    fn send_request(&mut self, method: &str, params: Value) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let _ = self.outgoing.send(message.to_string());
        id
    }

    fn send_notification(&mut self, method: &str, params: Value) {
        let message = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        let _ = self.outgoing.send(message.to_string());
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if !self.dead {
            self.shutdown();
        }
        // The channels close with the client, which ends both threads.
        self.writer.take();
        self.reader.take();
    }
}

/// One completion the server offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    /// What the list shows.
    pub label: String,
    /// Extra context, such as the type.
    pub detail: Option<String>,
    /// The LSP completion kind, as a number.
    pub kind: Option<u64>,
    /// What to insert when it is picked.
    pub insert: String,
}

impl CompletionItem {
    /// A short description of the kind, for the list.
    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            Some(2) | Some(3) => "fn",
            Some(4) => "fn",
            Some(5) => "field",
            Some(6) => "var",
            Some(7) => "class",
            Some(8) => "iface",
            Some(9) => "module",
            Some(10) => "prop",
            Some(13) => "enum",
            Some(14) => "keyword",
            Some(15) => "snippet",
            Some(21) => "const",
            Some(22) => "struct",
            Some(23) => "event",
            Some(25) => "type",
            _ => "",
        }
    }
}

/// One thing a server offers to do with a range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeAction {
    /// What the list shows.
    pub title: String,
    /// The edits it makes, per file. Empty when the action is a command the
    /// editor cannot run yet.
    pub edits: Vec<(PathBuf, Vec<ServerTextEdit>)>,
}

/// Parse a code action response.
pub fn code_actions(result: &Value) -> Vec<CodeAction> {
    let Some(items) = result.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let title = item.get("title")?.as_str()?.to_string();
            let edits = item
                .get("edit")
                .map(workspace_edit)
                .unwrap_or_default();
            Some(CodeAction { title, edits })
        })
        .collect()
}

/// A workspace edit: the edits a server wants made, per file.
///
/// The protocol offers two shapes — `changes` keyed by URI, and
/// `documentChanges`, which can also carry file renames — and both are read.
pub fn workspace_edit(result: &Value) -> Vec<(PathBuf, Vec<ServerTextEdit>)> {
    let mut out: Vec<(PathBuf, Vec<ServerTextEdit>)> = Vec::new();

    if let Some(changes) = result.get("changes").and_then(Value::as_object) {
        for (uri, edits) in changes {
            let edits = text_edits(edits);
            if !edits.is_empty() {
                out.push((PathBuf::from(uri_to_path(uri)), edits));
            }
        }
    }

    if let Some(changes) = result.get("documentChanges").and_then(Value::as_array) {
        for change in changes {
            let Some(document) = change.get("textDocument") else {
                continue;
            };
            let Some(uri) = document.get("uri").and_then(Value::as_str) else {
                continue;
            };
            let Some(edits) = change.get("edits") else {
                continue;
            };
            let edits = text_edits(edits);
            if !edits.is_empty() {
                out.push((PathBuf::from(uri_to_path(uri)), edits));
            }
        }
    }

    out
}

/// Parse a completion response.
///
/// The protocol allows a bare array or a `CompletionList` object, and an item
/// may carry its insertion text in `textEdit` rather than `insertText`.
pub fn completion_items(result: &Value) -> Vec<CompletionItem> {
    let items: &[Value] = match result {
        Value::Array(items) => items,
        Value::Object(map) => map
            .get("items")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        _ => return Vec::new(),
    };

    items
        .iter()
        .filter_map(|item| {
            let label = item.get("label")?.as_str()?.to_string();
            let insert = item
                .get("textEdit")
                .and_then(|edit| edit.get("newText"))
                .and_then(Value::as_str)
                .or_else(|| item.get("insertText").and_then(Value::as_str))
                .unwrap_or(&label)
                .to_string();
            Some(CompletionItem {
                label,
                detail: item
                    .get("detail")
                    .and_then(Value::as_str)
                    .map(|detail| detail.to_string()),
                kind: item.get("kind").and_then(Value::as_u64),
                insert,
            })
        })
        .collect()
}

/// One edit a server asked the editor to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerTextEdit {
    /// Where the replaced text starts.
    pub start: Position,
    /// Where it ends.
    pub end: Position,
    /// What replaces it.
    pub new_text: String,
}

/// The text of a hover response.
///
/// The protocol allows `MarkupContent { kind, value }`, a bare string, or an
/// array of marked strings; all of them are flattened to plain text.
pub fn hover_text(result: &Value) -> Option<String> {
    let contents = result.get("contents")?;
    let text = flatten_markup(contents)?;
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn flatten_markup(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().filter_map(flatten_markup).collect();
            Some(parts.join("\n\n"))
        }
        Value::Object(map) => {
            if let Some(value) = map.get("value").and_then(Value::as_str) {
                return Some(value.to_string());
            }
            // `MarkedString { language, value }` is already covered by `value`.
            None
        }
        _ => None,
    }
}

/// The locations a references response points at.
pub fn reference_locations(result: &Value) -> Vec<(PathBuf, Position)> {
    definition_locations(result)
}

/// Parse a formatting response into edits.
pub fn text_edits(result: &Value) -> Vec<ServerTextEdit> {
    let Some(items) = result.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let (start, end) = parse_range(item.get("range")?)?;
            Some(ServerTextEdit {
                start,
                end,
                new_text: item.get("newText")?.as_str()?.to_string(),
            })
        })
        .collect()
}

/// The locations a definition response points at.
pub fn definition_locations(result: &Value) -> Vec<(PathBuf, Position)> {
    let mut out = Vec::new();
    let mut push = |item: &Value| {
        // `Location` has `uri`/`range`; `LocationLink` has `targetUri` / `targetRange`.
        let (uri, range) = if let Some(uri) = item.get("uri") {
            (uri, item.get("range"))
        } else {
            let Some(uri) = item.get("targetUri") else {
                return;
            };
            (
                uri,
                item.get("targetSelectionRange")
                    .or_else(|| item.get("targetRange")),
            )
        };
        let Some(range) = range else { return };
        let Some(start) = range.get("start").and_then(parse_position) else {
            return;
        };
        out.push((
            PathBuf::from(uri_to_path(uri.as_str().unwrap_or_default())),
            start,
        ));
    };
    match result {
        Value::Array(items) => items.iter().for_each(&mut push),
        Value::Null => {}
        other => push(other),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_maps_from_lsp_numbers() {
        assert_eq!(Severity::from_lsp(Some(&json!(1))), Severity::Error);
        assert_eq!(Severity::from_lsp(Some(&json!(2))), Severity::Warning);
        assert_eq!(Severity::from_lsp(Some(&json!(3))), Severity::Information);
        assert_eq!(Severity::from_lsp(Some(&json!(4))), Severity::Hint);
        assert_eq!(Severity::from_lsp(None), Severity::Hint);
    }

    #[test]
    fn only_errors_and_warnings_are_problems() {
        assert!(Severity::Error.is_problem());
        assert!(Severity::Warning.is_problem());
        assert!(!Severity::Information.is_problem());
        assert!(!Severity::Hint.is_problem());
    }

    #[test]
    fn a_definition_response_yields_locations() {
        let result = json!([
            { "uri": "file:///a/b.rs", "range": { "start": { "line": 4, "character": 2 }, "end": { "line": 4, "character": 9 } } }
        ]);
        let locations = definition_locations(&result);
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].0, PathBuf::from("/a/b.rs"));
        assert_eq!(locations[0].1.line, 4);
        assert_eq!(locations[0].1.character, 2);
    }

    #[test]
    fn a_single_location_is_accepted() {
        let result = json!({ "uri": "file:///a/b.rs", "range": { "start": { "line": 1, "character": 0 } } });
        assert_eq!(definition_locations(&result).len(), 1);
    }

    #[test]
    fn a_null_definition_yields_nothing() {
        assert!(definition_locations(&Value::Null).is_empty());
    }

    #[test]
    fn hover_text_is_flattened() {
        let markup = json!({ "contents": { "kind": "markdown", "value": "```rust\nfn main()\n```" } });
        assert!(hover_text(&markup).unwrap().contains("fn main()"));

        let plain = json!({ "contents": "just text" });
        assert_eq!(hover_text(&plain).as_deref(), Some("just text"));

        let several = json!({ "contents": [{ "language": "rust", "value": "a" }, "b"] });
        assert_eq!(hover_text(&several).as_deref(), Some("a\n\nb"));

        let empty = json!({ "contents": "" });
        assert!(hover_text(&empty).is_none());
    }

    #[test]
    fn completion_items_are_parsed_from_an_array() {
        let result = json!([
            { "label": "main", "kind": 3, "detail": "fn()" },
            { "label": "map", "kind": 2, "insertText": "map()" }
        ]);
        let items = completion_items(&result);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].label, "main");
        assert_eq!(items[0].detail.as_deref(), Some("fn()"));
        assert_eq!(items[0].insert, "main", "label is the fallback insert text");
        assert_eq!(items[1].insert, "map()");
        assert_eq!(items[1].kind_label(), "fn");
    }

    #[test]
    fn completion_items_are_parsed_from_a_list() {
        let result = json!({ "isIncomplete": false, "items": [{ "label": "x" }] });
        assert_eq!(completion_items(&result).len(), 1);
    }

    #[test]
    fn a_text_edit_wins_over_insert_text() {
        let result = json!([{
            "label": "add",
            "insertText": "add",
            "textEdit": { "newText": "add(a, b)" }
        }]);
        assert_eq!(completion_items(&result)[0].insert, "add(a, b)");
    }

    #[test]
    fn a_null_completion_response_yields_nothing() {
        assert!(completion_items(&Value::Null).is_empty());
    }

    #[test]
    fn code_actions_carry_their_edits() {
        let result = json!([
            {
                "title": "Import `std::fmt`",
                "kind": "quickfix",
                "edit": {
                    "changes": {
                        "file:///a.rs": [
                            { "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "newText": "use std::fmt;\n" }
                        ]
                    }
                }
            },
            { "title": "Do nothing", "kind": "refactor" }
        ]);
        let actions = code_actions(&result);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].title, "Import `std::fmt`");
        assert_eq!(actions[0].edits.len(), 1);
        assert!(actions[1].edits.is_empty(), "an action with no edit still lists");
    }

    #[test]
    fn a_null_code_action_response_has_nothing() {
        assert!(code_actions(&Value::Null).is_empty());
    }

    #[test]
    fn a_workspace_edit_is_read_from_changes() {
        let result = json!({
            "changes": {
                "file:///a.rs": [
                    { "range": { "start": { "line": 0, "character": 3 }, "end": { "line": 0, "character": 6 } }, "newText": "bar" }
                ],
                "file:///b.rs": []
            }
        });
        let edits = workspace_edit(&result);
        assert_eq!(edits.len(), 1, "an empty file edit is dropped");
        assert_eq!(edits[0].0, PathBuf::from("/a.rs"));
        assert_eq!(edits[0].1[0].new_text, "bar");
    }

    #[test]
    fn a_workspace_edit_is_read_from_document_changes() {
        let result = json!({
            "documentChanges": [{
                "textDocument": { "uri": "file:///c.rs", "version": 3 },
                "edits": [
                    { "range": { "start": { "line": 1, "character": 0 }, "end": { "line": 1, "character": 2 } }, "newText": "zz" }
                ]
            }]
        });
        let edits = workspace_edit(&result);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].0, PathBuf::from("/c.rs"));
    }

    #[test]
    fn a_null_workspace_edit_has_nothing() {
        assert!(workspace_edit(&Value::Null).is_empty());
    }

    #[test]
    fn formatting_edits_are_parsed() {
        let result = json!([{
            "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 3 } },
            "newText": "  "
        }]);
        let edits = text_edits(&result);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "  ");
        assert_eq!(edits[0].end.character, 3);
    }

    #[test]
    fn a_null_formatting_response_yields_no_edits() {
        assert!(text_edits(&Value::Null).is_empty());
    }

    #[test]
    fn location_links_are_accepted() {
        let result = json!([{
            "targetUri": "file:///a/c.rs",
            "targetRange": { "start": { "line": 0, "character": 0 } },
            "targetSelectionRange": { "start": { "line": 7, "character": 3 } }
        }]);
        let locations = definition_locations(&result);
        assert_eq!(locations[0].0, PathBuf::from("/a/c.rs"));
        assert_eq!(locations[0].1.line, 7);
    }
}
