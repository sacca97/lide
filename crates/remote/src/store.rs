//! A [`Store`] backed by the remote agent, so the editor can open and save
//! remote documents with the same API it uses for local ones.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lide_resource::{Location, ResourceError, Store};

use crate::client::{Remote, RemoteError};

/// A store connected to one host and one root.
#[derive(Debug)]
pub struct AgentStore {
    remote: Mutex<Remote>,
    host: String,
    root: PathBuf,
}

impl AgentStore {
    /// Connect over SSH. The agent must be on the remote `PATH`.
    pub fn connect(host: impl Into<String>, root: impl AsRef<Path>) -> Result<Self, RemoteError> {
        let host = host.into();
        let root = root.as_ref().to_path_buf();
        let remote = Remote::ssh(&host, &root.to_string_lossy())?;
        Ok(Self {
            remote: Mutex::new(remote),
            host,
            root,
        })
    }

    /// A store over a locally spawned agent, for tests and local use.
    pub fn local(agent: impl AsRef<Path>, root: impl AsRef<Path>) -> Result<Self, RemoteError> {
        let root = root.as_ref().to_path_buf();
        let remote = Remote::spawn_local(agent, &root)?;
        Ok(Self {
            remote: Mutex::new(remote),
            host: "local".to_string(),
            root,
        })
    }

    /// The host and root this store serves.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The root this store serves.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The location of `path` (relative to the root) through this store.
    pub fn location(&self, path: impl Into<PathBuf>) -> Location {
        Location::remote(self.host.clone(), self.root.clone(), path)
    }

    fn relative<'a>(&self, location: &'a Location) -> Result<&'a Path, ResourceError> {
        match location {
            Location::Remote { host, root, path }
                if host == &self.host && root == &self.root =>
            {
                Ok(path)
            }
            other => Err(ResourceError::Unsupported(other.clone())),
        }
    }

    fn remote(&self) -> Result<std::sync::MutexGuard<'_, Remote>, ResourceError> {
        self.remote
            .lock()
            .map_err(|_| ResourceError::Remote("the agent connection was poisoned".to_string()))
    }
}

impl Store for AgentStore {
    fn read(&self, location: &Location) -> Result<Vec<u8>, ResourceError> {
        let path = self.relative(location)?;
        let mut remote = self.remote()?;
        remote
            .read(&path.to_string_lossy())
            .map_err(|error| ResourceError::Remote(error.to_string()))
    }

    fn write(&self, location: &Location, bytes: &[u8]) -> Result<(), ResourceError> {
        let path = self.relative(location)?;
        let mut remote = self.remote()?;
        remote
            .write(&path.to_string_lossy(), bytes)
            .map(|_| ())
            .map_err(|error| ResourceError::Remote(error.to_string()))
    }
}
