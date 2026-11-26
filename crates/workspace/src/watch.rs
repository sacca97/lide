//! Watching the files on disk.
//!
//! A file can change while the editor has it open: a formatter runs, a build
//! writes next to it, git checks out a branch. The watcher reports those
//! changes so that the editor can reload a file nobody has touched, and say
//! something when a file *has* been touched.
//!
//! Events are coalesced by content rather than by time. A save produces
//! several events, and there is no useful debounce interval that is right for
//! both a fast editor and a slow build; comparing what is on disk with what
//! the editor holds answers the only question that matters — did the text
//! really change?

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use notify::{RecursiveMode, Watcher as _};

/// A file that changed on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The file that changed.
    pub path: PathBuf,
    /// Whether the file still exists.
    pub exists: bool,
}

/// The file-system watcher.
pub struct Watch {
    watcher: notify::RecommendedWatcher,
    events: mpsc::Receiver<notify::Result<notify::Event>>,
    /// The directories being watched, so that a file can be added once.
    watched: HashSet<PathBuf>,
}

impl std::fmt::Debug for Watch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watch")
            .field("watched", &self.watched.len())
            .finish_non_exhaustive()
    }
}

impl Watch {
    /// Start watching `root` and everything under it.
    pub fn new(root: &Path) -> Option<Self> {
        let (tx, events) = mpsc::channel();
        let watcher = notify::recommended_watcher(move |event| {
            // A closed receiver means the editor is going away; dropping the
            // event is the right thing to do.
            let _ = tx.send(event);
        })
        .ok()?;

        let mut watch = Self {
            watcher,
            events,
            watched: HashSet::new(),
        };
        if watch.watch_directory(root, RecursiveMode::Recursive) {
            Some(watch)
        } else {
            // A watcher that watches nothing would only cost a thread.
            None
        }
    }

    /// Watch one more file, by watching the directory that holds it.
    ///
    /// A file outside the workspace root (or one opened before a folder was)
    /// still has to be seen. Watching the parent rather than the file
    /// survives the editor that saves by writing a new file and renaming it
    /// over the old one.
    pub fn watch_file(&mut self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = self.watch_directory(parent, RecursiveMode::Recursive);
        }
    }

    fn watch_directory(&mut self, directory: &Path, mode: RecursiveMode) -> bool {
        if self.watched.contains(directory) {
            return true;
        }
        match self.watcher.watch(directory, mode) {
            Ok(()) => {
                self.watched.insert(directory.to_path_buf());
                true
            }
            Err(error) => {
                log::warn!("could not watch {}: {error}", directory.display());
                false
            }
        }
    }

    /// The files that changed since the last call, without duplicates.
    pub fn drain(&mut self) -> Vec<Change> {
        let mut seen: HashSet<PathBuf> = HashSet::new();
        let mut changes = Vec::new();

        while let Ok(event) = self.events.try_recv() {
            let Ok(event) = event else {
                continue;
            };
            // Only content changes matter; opening a file is not a change.
            if !matches!(
                event.kind,
                notify::EventKind::Modify(_)
                    | notify::EventKind::Create(_)
                    | notify::EventKind::Remove(_)
            ) {
                continue;
            }
            for path in event.paths {
                if !seen.insert(path.clone()) {
                    continue;
                }
                changes.push(Change {
                    exists: path.exists(),
                    path,
                });
            }
        }

        changes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_root_is_not_fatal() {
        // Watching a path that does not exist fails, but the editor carries on
        // without a watcher rather than refusing to start.
        let watch = Watch::new(Path::new("/definitely/not/here"));
        assert!(watch.is_none());
    }

    #[test]
    fn a_real_directory_can_be_watched() {
        let directory = std::env::temp_dir().join("lide-watch-test");
        std::fs::create_dir_all(&directory).unwrap();
        let mut watch = Watch::new(&directory).expect("the directory should be watchable");
        assert!(watch.drain().is_empty(), "nothing has changed yet");

        std::fs::write(directory.join("a.txt"), "hello").unwrap();

        // The event arrives asynchronously; give the watcher a moment.
        let mut changes = Vec::new();
        for _ in 0..100 {
            changes = watch.drain();
            if !changes.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            changes.iter().any(|change| change.path.ends_with("a.txt")),
            "the new file should be reported"
        );

        std::fs::remove_dir_all(&directory).ok();
    }
}
