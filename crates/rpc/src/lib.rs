//! The wire format shared by the LSP client and the remote agent.
//!
//! Both speak `Content-Length` framed JSON over stdio; the framing lives here
//! so that the two cannot drift apart.

pub mod transport;
