# 0010 — Workspace search runs ripgrep

- Status: accepted
- Date: 2026-11-24

## Context

The editor needs to search a project: literal and regular-expression
patterns, case control, and results that open the file at the match.

## Decision

Run `rg` as a subprocess, ask for `--json`, and parse the stream on a
background thread.

## Why

* `ripgrep` is faster than anything written here would be, and it is already
  installed on most Linux systems.
* It already handles `.gitignore`, binary files, encodings and symlinks —
  every one of which is a bug waiting to happen in a hand-written walker.
* The plan asks to "invoke as a subprocess initially", and this is that.

The cost is a dependency on an external binary. A missing `rg` is reported as
a normal user-facing state rather than a failure; the editor works without it.

## Consequences

* `lide-search` owns the protocol: argument building, the JSON stream,
  base64 fallbacks for non-UTF-8 paths, and the worker thread.
* Results carry the line, the line's text and byte offsets, so opening one puts
  the caret on the match.
* Typing restarts the search and kills the previous `rg`, so a large tree
  cannot pile up processes.
* Replace across files is deliberately not offered yet: it needs a review
  step before it is safe.
