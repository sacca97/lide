//! Finding the LaTeX document a build should target.
//!
//! A paper is usually several files: a main document that includes chapters,
//! with the chapters in subdirectories. The build has to pick the one file
//! that is the root — the one nothing else includes — and it has to pick the
//! same file the user is editing, not the first one the tree happens to find.
//!
//! The paths here come in two spellings: the one the user opened the file
//! with, and the one a walk over the workspace finds. A symlink, or macOS's
//! canonical case, makes them differ, so the comparison goes through
//! [`fs_key`], which resolves a path to the file it names.

use std::path::{Path, PathBuf};

use super::Workspace;

impl Workspace {
    /// Where a LaTeX build for `tex` writes, from the settings.
    ///
    /// Empty means beside the source; a relative path is taken from the
    /// source's directory, and `~/` from home.
    pub(crate) fn latex_output_directory(&self, tex: &Path) -> Option<PathBuf> {
        let configured = self.settings.latex_output_directory.trim();
        if configured.is_empty() {
            return None;
        }
        let path = match configured.strip_prefix("~/") {
            Some(rest) => std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(rest))
                .unwrap_or_else(|| PathBuf::from(configured)),
            None => PathBuf::from(configured),
        };
        Some(if path.is_absolute() {
            path
        } else {
            tex.parent().unwrap_or_else(|| Path::new(".")).join(path)
        })
    }

    /// The main `.tex` file to build.
    ///
    /// An included chapter is not the build target. Prefer `main.tex`, then
    /// the active file when it is a root document, then the shallowest file
    /// that nothing else includes.
    pub(crate) fn latex_document(&self) -> Option<PathBuf> {
        let active = self
            .documents
            .get(self.focused_index())
            .and_then(|document| document.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "tex"))
            .map(Path::to_path_buf);
        let Some(root) = self.root.as_ref() else {
            return active;
        };

        let main = root.join("main.tex");
        if main.is_file() {
            return Some(main);
        }

        let mut found: Vec<PathBuf> = Vec::new();
        collect_tex(root, &mut found, 0);
        found.sort();
        let roots = tex_roots(&found);
        // The user opened the file and the tree walks the workspace, so the
        // two spellings of the same path can differ (a symlink, macOS's
        // canonical case). Same file means the same document.
        if let Some(active) = active
            .as_ref()
            .filter(|path| is_one_of_roots(path, &roots))
        {
            return Some(active.clone());
        }
        roots
            .iter()
            .find(|path| path.file_stem() == root.file_name())
            .cloned()
            .or_else(|| roots.first().cloned())
            .or(active)
            .or_else(|| found.into_iter().next())
    }
}

/// Collect the `.tex` files under `directory`, shallowly.
pub(crate) fn collect_tex(directory: &Path, found: &mut Vec<PathBuf>, depth: usize) {
    // Deep and wide enough for a paper project with its listings and
    // chapters; capped so a huge repository cannot make a save slow.
    if depth > 4 || found.len() > 128 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            collect_tex(&path, found, depth + 1);
        } else if path
            .extension()
            .map(|extension| extension == "tex")
            .unwrap_or(false)
        {
            found.push(path);
        }
    }
}

/// The `.tex` files that other files do not include: the build roots.
pub(crate) fn tex_roots(files: &[PathBuf]) -> Vec<PathBuf> {
    use std::collections::HashSet;

    let mut included = HashSet::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        for name in tex_includes(&text) {
            let mut path = file
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(name);
            if path.extension().is_none() {
                path.set_extension("tex");
            }
            included.insert(fs_key(&path));
        }
    }
    files
        .iter()
        .filter(|path| !included.contains(&fs_key(path)))
        .cloned()
        .collect()
}

/// Whether `path` is one of the given roots, whatever spelling the caller
/// used: the opened file and the walk over the workspace can disagree about
/// symlinks and a filesystem's canonical case, and they name one file.
pub(crate) fn is_one_of_roots(path: &Path, roots: &[PathBuf]) -> bool {
    roots
        .iter()
        .any(|root| root == &path.to_path_buf() || fs_key(root) == fs_key(path))
}

/// The names in `\input{...}`, `\include{...}` and `\subfile{...}`.
pub(crate) fn tex_includes(text: &str) -> Vec<&str> {
    let mut names = Vec::new();
    for command in ["\\input", "\\include", "\\subfile"] {
        let mut rest = text;
        while let Some(index) = rest.find(command) {
            let after = rest[index + command.len()..].trim_start();
            if let Some(after) = after.strip_prefix('{') {
                if let Some(end) = after.find('}') {
                    let name = after[..end].trim();
                    if !name.is_empty() {
                        names.push(name);
                    }
                }
            }
            rest = &rest[index + command.len()..];
        }
    }
    names
}

pub(crate) fn fs_key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
