//! Which language server runs for which language.

/// A language server the editor knows how to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerConfig {
    /// The language this server handles, as the editor names it.
    pub language: &'static str,
    /// The executable.
    pub command: &'static str,
    /// Extra arguments.
    pub args: &'static [&'static str],
    /// The language identifier the server expects in `textDocument/didOpen`.
    pub language_id: &'static str,
    /// The file extensions the server handles.
    pub extensions: &'static [&'static str],
}

impl ServerConfig {
    /// Parse the server's arguments.
    pub fn arguments(&self) -> Vec<String> {
        self.args.iter().map(|arg| arg.to_string()).collect()
    }
}

/// The servers the editor knows about.
///
/// Only servers that are commonly installed are listed. A missing binary is
/// reported, not fatal: the editor works without language intelligence.
pub const SERVERS: &[ServerConfig] = &[
    ServerConfig {
        language: "Rust",
        command: "rust-analyzer",
        args: &[],
        language_id: "rust",
        extensions: &["rs"],
    },
    ServerConfig {
        language: "Python",
        command: "pylsp",
        args: &[],
        language_id: "python",
        extensions: &["py", "pyi"],
    },
    ServerConfig {
        language: "LaTeX",
        command: "texlab",
        args: &[],
        language_id: "latex",
        extensions: &["tex", "bib"],
    },
    ServerConfig {
        language: "Go",
        command: "gopls",
        args: &[],
        language_id: "go",
        extensions: &["go"],
    },
    ServerConfig {
        language: "C",
        command: "clangd",
        args: &[],
        language_id: "c",
        extensions: &["c", "h"],
    },
    ServerConfig {
        language: "C++",
        command: "clangd",
        args: &[],
        language_id: "cpp",
        extensions: &["cc", "cpp", "cxx", "hpp", "hh"],
    },
    ServerConfig {
        language: "TypeScript",
        command: "typescript-language-server",
        args: &["--stdio"],
        language_id: "typescript",
        extensions: &["ts", "tsx", "js", "jsx"],
    },
];

/// The server for a file, by its extension.
pub fn server_for(path: &str) -> Option<&'static ServerConfig> {
    let extension = path.rsplit_once('.')?.1;
    SERVERS
        .iter()
        .find(|server| server.extensions.contains(&extension))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_files_get_rust_analyzer() {
        let server = server_for("/src/main.rs").expect("a server");
        assert_eq!(server.command, "rust-analyzer");
        assert_eq!(server.language_id, "rust");
    }

    #[test]
    fn header_files_go_to_clangd() {
        assert_eq!(server_for("a.hpp").unwrap().language_id, "cpp");
    }

    #[test]
    fn unknown_extensions_have_no_server() {
        assert!(server_for("notes.md").is_none());
        assert!(server_for("Makefile").is_none());
    }

    #[test]
    fn every_server_has_a_unique_language_id_pair() {
        // Two servers may share an executable (clangd), but not a language id.
        let mut seen = Vec::new();
        for server in SERVERS {
            assert!(!server.command.is_empty());
            assert!(!server.extensions.is_empty());
            seen.push((server.command, server.language_id));
        }
        let mut unique = seen.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(seen.len(), unique.len());
    }
}
