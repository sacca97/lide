//! The wire format: `Content-Length` framed JSON over a byte stream.
//!
//! LSP puts a header block in front of every message:
//!
//! ```text
//! Content-Length: 123\r\n
//! \r\n
//! {"jsonrpc":"2.0", ...}
//! ```
//!
//! The length is in **bytes** of the UTF-8 body, not characters, which is a
//! classic source of bugs when a message contains a multi-byte string.

use std::io::{self, BufRead, Write};

/// The largest message this client accepts.
///
/// A server that announces more than this is either confused or hostile;
/// either way the editor should not try to allocate it.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

/// Errors from the transport.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// The underlying stream failed.
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    /// The header block did not contain a usable `Content-Length`.
    #[error("missing or invalid Content-Length header")]
    BadHeader,
    /// The announced length was larger than [`MAX_MESSAGE_BYTES`].
    #[error("message of {0} bytes is too large")]
    TooLarge(usize),
}

/// Write one message.
pub fn write_message(writer: &mut impl Write, body: &str) -> Result<(), TransportError> {
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(body.as_bytes())?;
    writer.flush()?;
    Ok(())
}

/// Read one message, returning `None` at end of stream.
pub fn read_message(reader: &mut impl BufRead) -> Result<Option<String>, TransportError> {
    let mut length: Option<usize> = None;

    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            return Ok(None); // the server closed the stream
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break; // end of the header block
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = value.trim().parse::<usize>().ok();
        }
        // Other headers (Content-Type) are ignored.
    }

    let length = length.ok_or(TransportError::BadHeader)?;
    if length > MAX_MESSAGE_BYTES {
        return Err(TransportError::TooLarge(length));
    }

    let mut body = vec![0u8; length];
    std::io::Read::read_exact(reader, &mut body)?;
    Ok(Some(String::from_utf8_lossy(&body).into_owned()))
}

/// Turn a `file://` URI into a path.
///
/// The URI is percent-encoded, so `%20` has to become a space and so on.
/// Anything that is not a `file://` URI is left alone as a path-shaped
/// string, which is what servers that send odd URIs get.
pub fn uri_to_path(uri: &str) -> String {
    let rest = match uri.strip_prefix("file://") {
        Some(rest) => rest,
        None => return uri.to_string(),
    };
    // `file:///a/b` -> `/a/b`; `file://host/a/b` -> `/a/b`.
    let path = match rest.find('/') {
        Some(index) => &rest[index..],
        None => rest,
    };
    percent_decode(path)
}

/// Turn a path into a `file://` URI.
pub fn path_to_uri(path: &str) -> String {
    let mut out = String::from("file://");
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &input[index + 1..index + 3];
            if let Ok(value) = u8::from_str_radix(hex, 16) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip() {
        let mut buffer = Vec::new();
        write_message(&mut buffer, r#"{"jsonrpc":"2.0"}"#).unwrap();

        let mut reader = std::io::Cursor::new(buffer);
        let message = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(message, r#"{"jsonrpc":"2.0"}"#);
        assert!(read_message(&mut reader).unwrap().is_none());
    }

    #[test]
    fn the_length_counts_bytes_not_characters() {
        let body = r#"{"text":"héllo"}"#;
        let mut buffer = Vec::new();
        write_message(&mut buffer, body).unwrap();

        let header = String::from_utf8_lossy(&buffer);
        let announced: usize = header
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length:"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(announced, body.len());
        assert!(
            announced > body.chars().count(),
            "multibyte text is longer in bytes"
        );
    }

    #[test]
    fn a_missing_length_is_an_error() {
        let mut reader = std::io::Cursor::new(b"Content-Type: x\r\n\r\n{}".to_vec());
        assert!(matches!(
            read_message(&mut reader),
            Err(TransportError::BadHeader)
        ));
    }

    #[test]
    fn an_oversized_message_is_refused() {
        let mut reader = std::io::Cursor::new(
            format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_BYTES + 1).into_bytes(),
        );
        assert!(matches!(
            read_message(&mut reader),
            Err(TransportError::TooLarge(_))
        ));
    }

    #[test]
    fn uris_become_paths() {
        assert_eq!(uri_to_path("file:///home/a/b.rs"), "/home/a/b.rs");
        assert_eq!(uri_to_path("file:///home/a%20b/c.rs"), "/home/a b/c.rs");
        assert_eq!(uri_to_path("/not/a/uri"), "/not/a/uri");
    }

    #[test]
    fn paths_become_uris() {
        assert_eq!(path_to_uri("/home/a/b.rs"), "file:///home/a/b.rs");
        assert_eq!(path_to_uri("/home/a b/c.rs"), "file:///home/a%20b/c.rs");
    }

    #[test]
    fn a_path_survives_the_round_trip() {
        let path = "/tmp/with space/and-ünïcode.rs";
        assert_eq!(uri_to_path(&path_to_uri(path)), path);
    }
}
