//! The remote agent and its client.
//!
//! The agent is a small program that runs on the machine that owns the files.
//! It speaks `Content-Length` framed JSON over stdio ([`lide_rpc::transport`]),
//! the same framing the LSP client uses, so it can ride an ordinary
//! `ssh host lide-agent --stdio --root <dir>` connection without opening a
//! port or implementing any cryptography.
//!
//! Protocol 1 serves the file system: list, stat, read, write, rename, remove
//! and mkdir, all confined to the root the agent was started with. Search,
//! Git, language servers and terminals are later methods, not part of this
//! version.

pub mod agent;
pub mod client;
pub mod protocol;
pub mod store;
