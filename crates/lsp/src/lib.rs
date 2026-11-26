//! A Language Server Protocol client.
//!
//! The client starts a server process, performs the handshake, keeps it told
//! about the documents the editor has open, and forwards what it says —
//! diagnostics, and the answers to the requests the editor makes.
//!
//! The process is *not* trusted: a server that crashes, hangs or writes
//! nonsense produces an error the editor shows and nothing else. Starting a
//! server is best-effort, so a missing binary costs language features and
//! never the editor.

pub mod client;
pub mod server;
pub use lide_rpc::transport;

pub use client::{Client, CompletionItem, Diagnostic, Event, Position, Severity};
pub use server::{ServerConfig, SERVERS, server_for};
