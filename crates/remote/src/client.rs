//! The local side of the agent protocol: launch it, optionally over SSH, and
//! turn requests into typed answers.

use std::io::{BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use base64::Engine as _;
use serde_json::{json, Value};

use crate::protocol::{Entry, Hello, MAX_WRITE_BYTES, READ_CHUNK};

/// Errors from the remote agent.
#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    /// The agent could not be started.
    #[error("could not start the agent: {0}")]
    Spawn(String),
    /// The pipe failed.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// Framing failed.
    #[error("transport error: {0}")]
    Transport(#[from] lide_rpc::transport::TransportError),
    /// The agent went away before answering.
    #[error("the agent closed the connection")]
    Closed,
    /// The agent answered with an error.
    #[error("agent error {code}: {message}")]
    Agent {
        /// JSON-RPC error code.
        code: i64,
        /// The agent's message.
        message: String,
    },
    /// The agent's answer did not have the expected shape.
    #[error("protocol error: {0}")]
    Protocol(String),
    /// A response was not JSON.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// A connection to an agent.
pub struct Remote {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl std::fmt::Debug for Remote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Remote")
            .field("pid", &self.child.id())
            .finish_non_exhaustive()
    }
}

impl Remote {
    /// Start an agent binary directly. Used by tests and by a local agent.
    pub fn spawn_local(
        agent: impl AsRef<Path>,
        root: impl AsRef<Path>,
    ) -> Result<Self, RemoteError> {
        let child = Command::new(agent.as_ref())
            .arg("--stdio")
            .arg("--root")
            .arg(root.as_ref())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| RemoteError::Spawn(error.to_string()))?;
        Self::from_child(child)
    }

    /// Start an agent over SSH.
    ///
    /// `lide-agent` must be on the remote `PATH`. Credential prompts are
    /// disabled (`BatchMode=yes`): the connection uses keys or an agent, so a
    /// window cannot wait forever on a password.
    pub fn ssh(host: &str, root: &str) -> Result<Self, RemoteError> {
        let child = Command::new("ssh")
            .args(ssh_arguments(host, root))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| RemoteError::Spawn(error.to_string()))?;
        Self::from_child(child)
    }

    fn from_child(mut child: Child) -> Result<Self, RemoteError> {
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| RemoteError::Spawn("no stdin pipe".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RemoteError::Spawn("no stdout pipe".to_string()))?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 0,
        })
    }

    /// Ask the agent what it is.
    pub fn hello(&mut self) -> Result<Hello, RemoteError> {
        let hello: Hello = serde_json::from_value(self.request("initialize", json!({}))?)?;
        if hello.protocol != crate::protocol::PROTOCOL_VERSION {
            return Err(RemoteError::Protocol(format!(
                "the agent speaks protocol {}, this client speaks {}",
                hello.protocol,
                crate::protocol::PROTOCOL_VERSION
            )));
        }
        Ok(hello)
    }

    /// List a directory, with `""` for the root.
    pub fn list(&mut self, path: &str) -> Result<Vec<Entry>, RemoteError> {
        let value = self.request("list", json!({ "path": path }))?;
        Ok(serde_json::from_value(
            value.get("entries").cloned().unwrap_or_else(|| json!([])),
        )?)
    }

    /// Stat one path.
    pub fn stat(&mut self, path: &str) -> Result<Entry, RemoteError> {
        let value = self.request("stat", json!({ "path": path }))?;
        Ok(serde_json::from_value(
            value.get("entry").cloned().unwrap_or(Value::Null),
        )?)
    }

    /// Read a whole file, in chunks small enough for one framed message.
    pub fn read(&mut self, path: &str) -> Result<Vec<u8>, RemoteError> {
        let mut data = Vec::new();
        let mut offset = 0u64;
        loop {
            let value = self.request(
                "read",
                json!({ "path": path, "offset": offset, "length": READ_CHUNK }),
            )?;
            let encoded = value
                .get("data")
                .and_then(Value::as_str)
                .ok_or_else(|| RemoteError::Protocol("read has no data".to_string()))?;
            let chunk = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| RemoteError::Protocol("read data is not base64".to_string()))?;
            data.extend_from_slice(&chunk);
            if value.get("eof").and_then(Value::as_bool).unwrap_or(true) {
                break;
            }
            let next = value
                .get("next")
                .and_then(Value::as_u64)
                .unwrap_or(offset + chunk.len() as u64);
            if next <= offset {
                return Err(RemoteError::Protocol(
                    "the read offset did not advance".to_string(),
                ));
            }
            offset = next;
        }
        Ok(data)
    }

    /// Write a whole file. A save above [`MAX_WRITE_BYTES`] is refused.
    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<usize, RemoteError> {
        if data.len() > MAX_WRITE_BYTES {
            return Err(RemoteError::Protocol(format!(
                "a write is limited to {} MiB",
                MAX_WRITE_BYTES / 1024 / 1024
            )));
        }
        let encoded = base64::engine::general_purpose::STANDARD.encode(data);
        let value = self.request("write", json!({ "path": path, "data": encoded }))?;
        Ok(value
            .get("written")
            .and_then(Value::as_u64)
            .unwrap_or(data.len() as u64) as usize)
    }

    /// Rename a path.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), RemoteError> {
        self.request("rename", json!({ "from": from, "to": to }))?;
        Ok(())
    }

    /// Remove a file, or a directory with `recursive`.
    pub fn remove(&mut self, path: &str, recursive: bool) -> Result<(), RemoteError> {
        self.request("remove", json!({ "path": path, "recursive": recursive }))?;
        Ok(())
    }

    /// Create one directory.
    pub fn mkdir(&mut self, path: &str) -> Result<(), RemoteError> {
        self.request("mkdir", json!({ "path": path }))?;
        Ok(())
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, RemoteError> {
        self.next_id += 1;
        let id = self.next_id;
        let body = serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))?;
        lide_rpc::transport::write_message(&mut self.stdin, &body)?;
        let _ = self.stdin.flush();

        loop {
            let body =
                lide_rpc::transport::read_message(&mut self.stdout)?.ok_or(RemoteError::Closed)?;
            let response: Value = serde_json::from_str(&body)?;
            // Ignore anything that is not the answer to this request; future
            // versions may send notifications.
            if response.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = response.get("error") {
                return Err(RemoteError::Agent {
                    code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
                    message: error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown agent error")
                        .to_string(),
                });
            }
            return response
                .get("result")
                .cloned()
                .ok_or_else(|| RemoteError::Protocol("the response has no result".to_string()));
        }
    }
}

impl Drop for Remote {
    fn drop(&mut self) {
        // Closing stdin ends the agent's loop; kill it if it does not.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The SSH command line that starts the agent.
pub fn ssh_arguments(host: &str, root: &str) -> Vec<String> {
    vec![
        "-T".to_string(),
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "--".to_string(),
        host.to_string(),
        "lide-agent".to_string(),
        "--stdio".to_string(),
        "--root".to_string(),
        root.to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ssh_command_quotes_nothing_it_should_not() {
        let args = ssh_arguments("build-box", "/srv/project");
        assert!(args.contains(&"BatchMode=yes".to_string()));
        assert_eq!(args.last(), Some(&"/srv/project".to_string()));
        assert_eq!(args[args.len() - 4], "lide-agent");
    }
}
