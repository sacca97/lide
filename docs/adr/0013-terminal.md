# 0013 — The terminal: portable-pty and alacritty_terminal

- Status: accepted
- Date: 2026-11-24

## Context

The editor needs an integrated terminal: a PTY-backed shell, several sessions,
and resizing.

## Decision

Two layers in one crate, kept apart:

* **`portable-pty` 0.9** opens the PTY and starts the shell.
* **`alacritty_terminal` 0.26** does the emulation: the parser, the grid and
  the cursor.

[ADR 0006](0006-terminal.md) predates the work and names `pty-process`;
`portable-pty` was preferred in the end because its `MasterPty` keeps the
resize handle, which `pty-process` leaves to the caller. Both are MIT and
permissively licensed; the difference is ergonomic, not architectural.

## Why this shape

The plan asks for "a clear separation between terminal rendering and process
management", and that is what the split buys:

* `Session` knows about file descriptors, bytes and the child process. It has
  no idea what an escape sequence is. It is tested against a real `sh`.
* `Emulator` knows about escape sequences and the grid. It has no idea a
  process exists. It is tested by feeding it bytes.

Each can be replaced without the other: a different emulator would keep
`Session`, and a different PTY layer would keep `Emulator`.

## Consequences

* The shell starts as a login shell (`-l`) so that the user's aliases and
  `PATH` are there, with `TERM=xterm-256color`.
* Closing a terminal kills its child. Leaving shells behind every time a tab
  closes would be a leak the user cannot see.
* The PTY is resized when the area it is drawn in changes size, which is what
  makes a full-screen program (`vim`, `htop`) lay out correctly.
* A terminal owns the keyboard while it is in front: only the palette and
  quitting are kept back, because a shell needs `Ctrl+C` to interrupt and
  `Ctrl+W` to delete a word.
* Rendering is text-only for now: `alacritty_terminal` gives cells with
  colours, but painting them needs per-cell spans the view does not build yet.
  The roadmap records it.
