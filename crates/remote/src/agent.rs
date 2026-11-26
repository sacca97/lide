//! The agent that runs on the host that owns the files.

use std::fs;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use base64::Engine as _;
use serde_json::{json, Value};

use crate::protocol::{Entry, Hello, MAX_WRITE_BYTES, PROTOCOL_VERSION};

/// The root a request must stay inside.
#[derive(Debug, Clone)]
pub struct Agent {
    root: PathBuf,
}

/// A failed request, ready to become a JSON-RPC error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentError {
    /// The JSON-RPC error code.
    pub code: i64,
    /// What went wrong.
    pub message: String,
}

impl AgentError {
    fn bad(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
        }
    }

    fn io(error: io::Error) -> Self {
        Self {
            code: -32000,
            message: error.to_string(),
        }
    }

    fn unknown(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("unknown method: {method}"),
        }
    }
}

impl Agent {
    /// Open a root. It is canonicalised once so later containment checks
    /// cannot be fooled by a relative path.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, String> {
        let root = fs::canonicalize(root.as_ref())
            .map_err(|error| format!("cannot open {}: {error}", root.as_ref().display()))?;
        if !root.is_dir() {
            return Err(format!("{} is not a directory", root.display()));
        }
        Ok(Self { root })
    }

    /// The canonical root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Handle one method call.
    pub fn handle(&self, method: &str, params: &Value) -> Result<Value, AgentError> {
        match method {
            "initialize" => Ok(json!(Hello {
                protocol: PROTOCOL_VERSION,
                version: env!("CARGO_PKG_VERSION").to_string(),
                root: self
                    .root
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "/".to_string()),
            })),
            "list" => self.list(params),
            "stat" => self.stat(params),
            "read" => self.read(params),
            "write" => self.write(params),
            "rename" => self.rename(params),
            "remove" => self.remove(params),
            "mkdir" => self.mkdir(params),
            other => Err(AgentError::unknown(other)),
        }
    }

    fn param<'a>(params: &'a Value, name: &str) -> Result<&'a str, AgentError> {
        params
            .get(name)
            .and_then(Value::as_str)
            .ok_or_else(|| AgentError::bad(format!("missing string parameter: {name}")))
    }

    /// A path relative to the root, rejecting absolute paths and `..`.
    fn relative(&self, request: &str) -> Result<PathBuf, AgentError> {
        let mut relative = PathBuf::new();
        for component in Path::new(request).components() {
            match component {
                Component::Normal(part) => relative.push(part),
                Component::CurDir => {}
                Component::RootDir | Component::ParentDir | Component::Prefix(_) => {
                    return Err(AgentError::bad("paths must stay inside the workspace root"));
                }
            }
        }
        Ok(relative)
    }

    /// An existing path, with symlinks resolved and the root checked after
    /// resolution.
    fn existing(&self, request: &str) -> Result<PathBuf, AgentError> {
        let path = self.root.join(self.relative(request)?);
        let canonical = fs::canonicalize(&path).map_err(AgentError::io)?;
        if !canonical.starts_with(&self.root) {
            return Err(AgentError::bad("path escapes the workspace root"));
        }
        Ok(canonical)
    }

    /// Where a new file or directory may be created. The parent must exist
    /// and resolve inside the root; the leaf itself need not exist.
    fn for_write(&self, request: &str) -> Result<PathBuf, AgentError> {
        let relative = self.relative(request)?;
        let name = relative
            .file_name()
            .ok_or_else(|| AgentError::bad("the workspace root cannot be written"))?;
        let parent = relative.parent().unwrap_or_else(|| Path::new(""));
        let parent = self.root.join(parent);
        let canonical_parent = fs::canonicalize(&parent).map_err(AgentError::io)?;
        if !canonical_parent.starts_with(&self.root) {
            return Err(AgentError::bad("path escapes the workspace root"));
        }
        if !canonical_parent.is_dir() {
            return Err(AgentError::bad("the parent is not a directory"));
        }
        let target = canonical_parent.join(name);
        // An existing leaf may be a symlink. Writing through one that points
        // outside the root would escape the workspace, so resolve it and
        // check the root again. A dangling symlink is refused too: following
        // it is not safe to guess at.
        match fs::symlink_metadata(&target) {
            Ok(_) => {
                let canonical = fs::canonicalize(&target)
                    .map_err(|_| AgentError::bad("the target is a dangling symlink"))?;
                if !canonical.starts_with(&self.root) {
                    return Err(AgentError::bad("path escapes the workspace root"));
                }
                Ok(canonical)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(target),
            Err(error) => Err(AgentError::io(error)),
        }
    }

    fn list(&self, params: &Value) -> Result<Value, AgentError> {
        let directory = self.existing(Self::param(params, "path").unwrap_or(""))?;
        let mut entries = Vec::new();
        for entry in fs::read_dir(&directory).map_err(AgentError::io)? {
            // A directory that vanishes while it is being listed should not
            // fail the rest of the listing.
            let Ok(entry) = entry else { continue };
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry
                .path()
                .strip_prefix(&self.root)
                .unwrap_or(&entry.path())
                .to_string_lossy()
                .replace('\\', "/");
            let modified = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs());
            entries.push(Entry {
                name,
                path,
                is_dir: metadata.is_dir(),
                size: if metadata.is_file() {
                    metadata.len()
                } else {
                    0
                },
                modified,
            });
            if entries.len() >= 10_000 {
                break;
            }
        }
        entries.sort_by(|left, right| {
            right
                .is_dir
                .cmp(&left.is_dir)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        Ok(json!({ "entries": entries }))
    }

    fn stat(&self, params: &Value) -> Result<Value, AgentError> {
        let path = self.existing(Self::param(params, "path")?)?;
        let metadata = fs::metadata(&path).map_err(AgentError::io)?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let relative = path
            .strip_prefix(&self.root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        Ok(json!({
            "entry": Entry {
                name,
                path: relative,
                is_dir: metadata.is_dir(),
                size: if metadata.is_file() { metadata.len() } else { 0 },
                modified: metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs()),
            }
        }))
    }

    fn read(&self, params: &Value) -> Result<Value, AgentError> {
        let path = self.existing(Self::param(params, "path")?)?;
        let metadata = fs::metadata(&path).map_err(AgentError::io)?;
        if !metadata.is_file() {
            return Err(AgentError::bad("not a regular file"));
        }
        let size = metadata.len();
        let offset = params
            .get("offset")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(size);
        let length = params
            .get("length")
            .and_then(Value::as_u64)
            .unwrap_or(crate::protocol::READ_CHUNK)
            .min(crate::protocol::READ_CHUNK);
        let end = offset.saturating_add(length).min(size);
        let take = end.saturating_sub(offset) as usize;

        let mut file = fs::File::open(&path).map_err(AgentError::io)?;
        file.seek(SeekFrom::Start(offset)).map_err(AgentError::io)?;
        let mut buffer = vec![0u8; take];
        file.read_exact(&mut buffer).map_err(AgentError::io)?;

        Ok(json!({
            "data": base64::engine::general_purpose::STANDARD.encode(&buffer),
            "next": end,
            "size": size,
            "eof": end >= size,
        }))
    }

    fn write(&self, params: &Value) -> Result<Value, AgentError> {
        let path = self.for_write(Self::param(params, "path")?)?;
        let encoded = Self::param(params, "data")?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| AgentError::bad("data is not valid base64"))?;
        if decoded.len() > MAX_WRITE_BYTES {
            return Err(AgentError::bad(format!(
                "a write is limited to {} MiB",
                MAX_WRITE_BYTES / 1024 / 1024
            )));
        }
        fs::write(&path, &decoded).map_err(AgentError::io)?;
        Ok(json!({ "written": decoded.len() }))
    }

    fn rename(&self, params: &Value) -> Result<Value, AgentError> {
        let from = self.existing(Self::param(params, "from")?)?;
        if from == self.root {
            return Err(AgentError::bad("the workspace root cannot be renamed"));
        }
        let to = self.for_write(Self::param(params, "to")?)?;
        fs::rename(&from, &to).map_err(AgentError::io)?;
        Ok(json!({ "renamed": true }))
    }

    fn remove(&self, params: &Value) -> Result<Value, AgentError> {
        let path = self.existing(Self::param(params, "path")?)?;
        if path == self.root {
            return Err(AgentError::bad("the workspace root cannot be removed"));
        }
        let recursive = params
            .get("recursive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let metadata = fs::metadata(&path).map_err(AgentError::io)?;
        if metadata.is_dir() {
            if recursive {
                fs::remove_dir_all(&path).map_err(AgentError::io)?;
            } else {
                fs::remove_dir(&path).map_err(AgentError::io)?;
            }
        } else {
            fs::remove_file(&path).map_err(AgentError::io)?;
        }
        Ok(json!({ "removed": true }))
    }

    fn mkdir(&self, params: &Value) -> Result<Value, AgentError> {
        let path = self.for_write(Self::param(params, "path")?)?;
        fs::create_dir(&path).map_err(AgentError::io)?;
        Ok(json!({ "created": true }))
    }
}

/// Run the agent on stdio after parsing its arguments. Returns a process exit
/// code.
pub fn main() -> i32 {
    let mut root = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--stdio" => {}
            "--root" => {
                root = args.next();
            }
            "--version" => {
                println!("lide-agent {}", env!("CARGO_PKG_VERSION"));
                return 0;
            }
            "--help" | "-h" => {
                eprintln!("usage: lide-agent --stdio --root <directory>");
                return 0;
            }
            other => {
                eprintln!("lide-agent: unknown argument: {other}");
                return 2;
            }
        }
    }
    let Some(root) = root else {
        eprintln!("lide-agent: --root <directory> is required");
        return 2;
    };
    let agent = match Agent::new(&root) {
        Ok(agent) => agent,
        Err(error) => {
            eprintln!("lide-agent: {error}");
            return 2;
        }
    };

    let mut stdin = BufReader::new(io::stdin());
    let mut stdout = io::stdout();
    loop {
        let body = match lide_rpc::transport::read_message(&mut stdin) {
            Ok(Some(body)) => body,
            Ok(None) => break,
            Err(error) => {
                eprintln!("lide-agent: {error}");
                return 1;
            }
        };
        let request: Value = match serde_json::from_str(&body) {
            Ok(request) => request,
            Err(error) => {
                eprintln!("lide-agent: malformed request: {error}");
                continue;
            }
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = request.get("params").cloned().unwrap_or(Value::Null);

        let response = match agent.handle(method, &params) {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": error.code, "message": error.message }
            }),
        };
        let encoded = match serde_json::to_string(&response) {
            Ok(encoded) => encoded,
            Err(error) => {
                eprintln!("lide-agent: could not encode a response: {error}");
                return 1;
            }
        };
        if let Err(error) = lide_rpc::transport::write_message(&mut stdout, &encoded) {
            eprintln!("lide-agent: {error}");
            return 1;
        }
    }
    0
}
