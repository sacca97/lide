//! Document locations and the store that reads and writes them.
//!
//! Local documents keep their `PathBuf`. Remote documents name a host, the
//! root the agent serves, and a path relative to that root; they never hold a
//! local path. The editor uses [`Store`] rather than `std::fs`, so a remote
//! store can be substituted without changing the editing model.

use std::fmt;
use std::path::{Path, PathBuf};

/// Where a document lives.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Location {
    /// A file on this machine.
    Local(PathBuf),
    /// A file served by an agent over SSH.
    Remote {
        /// The SSH destination, as `ssh` accepts it (`host`, `user@host`).
        host: String,
        /// The directory the agent was started with.
        root: PathBuf,
        /// A path relative to `root`.
        path: PathBuf,
    },
}

impl Location {
    /// A local file.
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Self::Local(path.into())
    }

    /// A remote file.
    pub fn remote(host: impl Into<String>, root: impl Into<PathBuf>, path: impl Into<PathBuf>) -> Self {
        Self::Remote {
            host: host.into(),
            root: root.into(),
            path: path.into(),
        }
    }

    /// The local path, when this is a local document.
    pub fn as_local(&self) -> Option<&Path> {
        match self {
            Self::Local(path) => Some(path),
            Self::Remote { .. } => None,
        }
    }

    /// The last path component, for a tab title.
    pub fn file_name(&self) -> Option<&str> {
        match self {
            Self::Local(path) => path.file_name().and_then(|name| name.to_str()),
            Self::Remote { path, .. } => path.file_name().and_then(|name| name.to_str()),
        }
    }

    /// The extension, for choosing a syntax and a viewer.
    pub fn extension(&self) -> Option<&str> {
        match self {
            Self::Local(path) => path.extension().and_then(|extension| extension.to_str()),
            Self::Remote { path, .. } => path.extension().and_then(|extension| extension.to_str()),
        }
    }

    /// The path used to choose a language: the local path or the remote one.
    pub fn path_for_kind(&self) -> &Path {
        match self {
            Self::Local(path) => path,
            Self::Remote { path, .. } => path,
        }
    }

    /// Whether the document is on another machine.
    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote { .. })
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local(path) => write!(f, "{}", path.display()),
            Self::Remote { host, root, path } => {
                let root = root.to_string_lossy();
                let path = path.to_string_lossy();
                write!(
                    f,
                    "ssh://{host}/{}/{}",
                    root.trim_matches('/'),
                    path.trim_start_matches('/')
                )
            }
        }
    }
}

/// Errors from a store.
#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    /// The local file system failed.
    #[error("{0}")]
    Io(String),
    /// The store does not handle this kind of location.
    #[error("this store cannot handle {0}")]
    Unsupported(Location),
    /// The remote agent or transport failed.
    #[error("{0}")]
    Remote(String),
}

/// Read and write documents.
///
/// Implementations are shared by the editor with `Arc`, so `&self`; a remote
/// implementation serialises requests internally.
pub trait Store: fmt::Debug + Send + Sync {
    /// Read a whole document.
    fn read(&self, location: &Location) -> Result<Vec<u8>, ResourceError>;

    /// Write a whole document.
    fn write(&self, location: &Location, bytes: &[u8]) -> Result<(), ResourceError>;
}

/// The local file system.
#[derive(Debug, Default)]
pub struct LocalStore;

impl Store for LocalStore {
    fn read(&self, location: &Location) -> Result<Vec<u8>, ResourceError> {
        let path = location
            .as_local()
            .ok_or_else(|| ResourceError::Unsupported(location.clone()))?;
        std::fs::read(path).map_err(|error| ResourceError::Io(error.to_string()))
    }

    fn write(&self, location: &Location, bytes: &[u8]) -> Result<(), ResourceError> {
        let path = location
            .as_local()
            .ok_or_else(|| ResourceError::Unsupported(location.clone()))?;
        std::fs::write(path, bytes).map_err(|error| ResourceError::Io(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remote_location_displays_as_a_uri() {
        let location = Location::remote("build-box", "/srv/project", "src/main.rs");
        assert_eq!(location.to_string(), "ssh://build-box/srv/project/src/main.rs");
        assert_eq!(location.file_name(), Some("main.rs"));
        assert_eq!(location.extension(), Some("rs"));
        assert!(location.is_remote());
        assert!(location.as_local().is_none());
    }

    #[test]
    fn a_local_location_keeps_its_path_for_kinds() {
        let location = Location::local("/tmp/a.rs");
        assert_eq!(location.as_local(), Some(Path::new("/tmp/a.rs")));
        assert_eq!(location.extension(), Some("rs"));
        assert!(!location.is_remote());
    }

    #[test]
    fn the_local_store_round_trips_and_refuses_remote_paths() {
        let directory = std::env::temp_dir().join(format!("lide-resource-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("a.txt");
        let store = LocalStore;
        let location = Location::local(&path);
        store.write(&location, b"hello").unwrap();
        assert_eq!(store.read(&location).unwrap(), b"hello");
        assert!(store
            .read(&Location::remote("host", "/root", "a.txt"))
            .is_err());
        let _ = std::fs::remove_dir_all(&directory);
    }
}
