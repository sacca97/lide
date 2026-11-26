//! The messages the agent and client exchange.

use serde::{Deserialize, Serialize};

/// The protocol version this build speaks.
pub const PROTOCOL_VERSION: u64 = 1;

/// The largest file the client reads in one request. Larger files are
/// requested in chunks, so this is a message-size limit, not a file limit.
pub const READ_CHUNK: u64 = 8 * 1024 * 1024;

/// The largest file the agent accepts in one write. Saves above this are
/// refused rather than split, for now.
pub const MAX_WRITE_BYTES: usize = 32 * 1024 * 1024;

/// One directory entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The file name.
    pub name: String,
    /// Path relative to the agent's root, with `/` separators.
    pub path: String,
    /// Whether it is a directory.
    pub is_dir: bool,
    /// Size in bytes for files, zero for directories.
    pub size: u64,
    /// Modification time in seconds since the Unix epoch, when known.
    pub modified: Option<u64>,
}

/// What `initialize` returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// The protocol version.
    pub protocol: u64,
    /// The agent's version.
    pub version: String,
    /// The root's last path component, for display.
    pub root: String,
}
