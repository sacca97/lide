# 0012 — Git goes through the `git` command line

- Status: accepted
- Date: 2026-11-24

## Context

The editor needs the current branch, a file's diff, and a readable view of an
unmerged file. Staging, commits, branch switching and the network operations
are deliberately left to the terminal; the editor is not a Git client.

## Decision

Run the installed `git` as a subprocess and parse what it prints.

## Why

* `git status --porcelain=v1 -b -z` is a stable, documented, machine-readable
  format. `-z` is what makes a path containing a space or a newline survive.
* The parts that are hard to reimplement are the ones nobody wants to
  reimplement: the index format, `.gitignore` matching, submodules, and what
  "modified" means for a file whose mode changed.
* The plan says to prefer the CLI unless a concrete requirement justifies
  `libgit2`. No such requirement has appeared: the status query responds in
  milliseconds on this repository.

## Consequences

* `git` must be installed. A missing binary is a normal user-facing state — the
  status bar simply has no branch, and the editor works.
* The status is refreshed at most every two seconds because `git status` over a
  large tree is not something to run on every frame.
* `lide-git` owns the parsing: the porcelain records, the branch header forms
  (`main...origin/main [ahead 1]`, `No commits yet on main`, `HEAD (no branch)`),
  the two-column staged/unstaged distinction, and unified diffs.
* `Ctrl+G` opens the active file's diff; an unmerged file gets a two-sided
  conflict view built from its `<<<<<<<` / `=======` / `>>>>>>>` markers.
* The read paths are covered by tests that create a real repository in a
  temporary directory: status, diff, and a merge conflict.
