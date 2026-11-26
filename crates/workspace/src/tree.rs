//! The file tree shown in the sidebar.

use std::path::{Path, PathBuf};

/// A node in the file tree.
#[derive(Debug, Clone)]
pub struct TreeNode {
    /// The name shown in the tree.
    pub name: String,
    /// The full path of the entry.
    pub path: PathBuf,
    /// Whether the entry is a directory.
    pub is_dir: bool,
    /// The children of a directory.
    pub children: Vec<TreeNode>,
}

/// The file tree of a workspace.
#[derive(Debug, Clone, Default)]
pub struct FileTree {
    /// The top level entries.
    pub nodes: Vec<TreeNode>,
}

/// Directories that are never interesting in a file tree.
const IGNORED_DIRECTORIES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".cargo",
    ".idea",
    ".vscode",
    "node_modules",
    "target",
    "dist",
    "build",
    "__pycache__",
    ".venv",
    "venv",
];

impl FileTree {
    /// Scan a directory into a tree.
    ///
    /// The scan is shallow on purpose: it stops at `max_depth` and skips
    /// hidden entries and the directories that never contain source code.
    pub fn scan(root: &Path, max_depth: usize) -> Self {
        Self {
            nodes: scan_directory(root, max_depth),
        }
    }

    /// The top level entries.
    pub fn nodes(&self) -> &[TreeNode] {
        &self.nodes
    }

    /// Every directory in the tree, deepest last.
    pub fn directory_paths(&self) -> Vec<PathBuf> {
        fn walk(nodes: &[TreeNode], out: &mut Vec<PathBuf>) {
            for node in nodes {
                if node.is_dir {
                    out.push(node.path.clone());
                    walk(&node.children, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.nodes, &mut out);
        out
    }
}

fn scan_directory(directory: &Path, remaining_depth: usize) -> Vec<TreeNode> {
    if remaining_depth == 0 {
        return Vec::new();
    }

    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };

    let mut nodes: Vec<TreeNode> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let path = entry.path();
            let is_dir = path.is_dir();
            let ignored = is_dir && IGNORED_DIRECTORIES.contains(&name.as_str());
            let children = if is_dir && !ignored {
                scan_directory(&path, remaining_depth - 1)
            } else {
                Vec::new()
            };
            Some(TreeNode {
                name,
                path,
                is_dir,
                children,
            })
        })
        .collect();

    // Directories first, then files, each alphabetically.
    nodes.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });

    nodes
}
