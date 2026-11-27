//! The diff viewer.
//!
//! A changed file opens as its own document showing `git diff` for it: the
//! old and new line numbers, and a tint per kind of line. An unmerged file
//! opens as a conflict view instead, with its two sides and markers. It is
//! read-only; Git operations stay in the terminal.

use std::path::{Path, PathBuf};

use crate::git::{DiffKind, DiffLine};
use rune::{col, el, row, text, El, ScrollSpec};
use rune_kit::Kit;

/// A file's diff, ready to draw.
pub struct DiffView {
    /// The repository this diff belongs to.
    pub root: PathBuf,
    /// The file, relative to the repository root.
    pub path: PathBuf,
    /// Whether the staged diff is shown.
    pub staged: bool,
    /// Whether this is a merge conflict view rather than a git diff.
    pub conflict: bool,
    /// The parsed lines.
    pub lines: Vec<DiffLine>,
}

impl std::fmt::Debug for DiffView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiffView")
            .field("path", &self.path)
            .field("staged", &self.staged)
            .field("conflict", &self.conflict)
            .field("lines", &self.lines.len())
            .finish_non_exhaustive()
    }
}

impl DiffView {
    /// Open the diff of `path`.
    pub fn open(root: &Path, path: impl AsRef<Path>, staged: bool) -> Self {
        let path = path.as_ref().to_path_buf();
        let repo = crate::git::Repo {
            root: root.to_path_buf(),
        };
        let text = repo.diff_file(&path, staged).unwrap_or_default();
        Self {
            root: root.to_path_buf(),
            path,
            staged,
            conflict: false,
            lines: crate::git::parse_diff(&text),
        }
    }

    /// Open a conflicted file's working copy, with its two sides marked.
    pub fn open_conflict(root: &Path, path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let text = std::fs::read_to_string(root.join(&path)).unwrap_or_default();
        Self {
            root: root.to_path_buf(),
            path,
            staged: false,
            conflict: true,
            lines: parse_conflict(&text),
        }
    }

    /// The title shown on the tab.
    pub fn title(&self) -> String {
        let name = self
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "diff".to_string());
        if self.conflict {
            format!("{name} (conflict)")
        } else if self.staged {
            format!("{name} (staged)")
        } else {
            format!("{name} (diff)")
        }
    }

    /// The path the diff is for, as a real file path.
    pub fn file_path(&self) -> PathBuf {
        self.root.join(&self.path)
    }

    /// How many lines were added and removed.
    pub fn counts(&self) -> (usize, usize) {
        let added = self
            .lines
            .iter()
            .filter(|line| line.kind == DiffKind::Added)
            .count();
        let removed = self
            .lines
            .iter()
            .filter(|line| line.kind == DiffKind::Removed)
            .count();
        (added, removed)
    }

    /// Draw the diff.
    pub fn view(&self, k: &Kit) -> El {
        let (added, removed) = self.counts();

        let mut body = col().w_full().gap(0.0);
        if self.lines.is_empty() {
            body = body.child(text("No changes").font(k.ty.small).color(k.c.text_weak));
        }
        for (index, line) in self.lines.iter().enumerate() {
            let (tint, ink) = match line.kind {
                DiffKind::Added => (Some(k.c.success.bg), k.c.success.fg),
                DiffKind::Removed => (Some(k.c.danger.bg), k.c.danger.fg),
                DiffKind::Hunk => (Some(k.c.info.bg), k.c.text_accent),
                DiffKind::Header => (Some(k.c.surface_base), k.c.text_weak),
                _ => (None, k.c.text_base),
            };

            let mut row = row()
                .key(format!("diff-{index}"))
                .w_full()
                .gap(8.0)
                .py(1.0)
                .child(
                    text(match line.old_line {
                        Some(number) => number.to_string(),
                        None => String::new(),
                    })
                    .font(k.ty.mono)
                    .color(k.c.text_weak)
                    .w(44.0),
                )
                .child(
                    text(match line.new_line {
                        Some(number) => number.to_string(),
                        None => String::new(),
                    })
                    .font(k.ty.mono)
                    .color(k.c.text_weak)
                    .w(44.0),
                )
                .child(text(line.text.clone()).font(k.ty.mono).color(ink).grow(1.0));
            if let Some(tint) = tint {
                row = row.fill(tint);
            }
            body = body.child(row);
        }

        col()
            .w_full()
            .h_full()
            .fill(k.c.page)
            .child(
                row()
                    .w_full()
                    .h(40.0)
                    .px(10.0)
                    .gap(10.0)
                    .center_y()
                    .fill(k.c.bg_base)
                    .child(
                        text(self.path.display().to_string())
                            .font(k.ty.small)
                            .color(k.c.text_weak)
                            .grow(1.0),
                    )
                    .child(if self.conflict {
                        text("CONFLICT")
                            .font(k.ty.small_medium)
                            .color(k.c.danger.fg)
                    } else {
                        el().w(0.0)
                    })
                    .child(
                        text(format!("+{added}"))
                            .font(k.ty.small_medium)
                            .color(k.c.success.fg),
                    )
                    .child(
                        text(format!("-{removed}"))
                            .font(k.ty.small_medium)
                            .color(k.c.danger.fg),
                    ),
            )
            .child(
                col()
                    .key("diff-scroll")
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .scroll(ScrollSpec {
                        stick_bottom: false,
                    })
                    .px(6.0)
                    .child(body),
            )
    }
}

/// Turn a file's conflict markers into diff lines.
///
/// The two sides are read-only and tinted like a diff: the lines between
/// `<<<<<<<` and `=======` are ours (removed), and the lines between
/// `=======` and `>>>>>>>` are theirs (added). The markers themselves are
/// kept as hunk rows so the boundaries stay visible.
fn parse_conflict(text: &str) -> Vec<DiffLine> {
    let mut lines = Vec::new();
    let mut kind = DiffKind::Context;
    let mut number = 0u64;

    for raw in text.lines() {
        if raw.starts_with("<<<<<<<") {
            lines.push(DiffLine {
                kind: DiffKind::Hunk,
                text: raw.to_string(),
                old_line: None,
                new_line: None,
            });
            kind = DiffKind::Removed;
            continue;
        }
        if raw.starts_with("=======") {
            lines.push(DiffLine {
                kind: DiffKind::Hunk,
                text: raw.to_string(),
                old_line: None,
                new_line: None,
            });
            kind = DiffKind::Added;
            continue;
        }
        if raw.starts_with(">>>>>>>") {
            lines.push(DiffLine {
                kind: DiffKind::Hunk,
                text: raw.to_string(),
                old_line: None,
                new_line: None,
            });
            kind = DiffKind::Context;
            continue;
        }

        number += 1;
        let (old_line, new_line) = match kind {
            DiffKind::Removed => (Some(number), None),
            DiffKind::Added => (None, Some(number)),
            _ => (Some(number), Some(number)),
        };
        lines.push(DiffLine {
            kind,
            text: raw.to_string(),
            old_line,
            new_line,
        });
    }

    lines
}
