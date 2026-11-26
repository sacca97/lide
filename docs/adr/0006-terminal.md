# 0006 — The terminal: alacritty_terminal and pty-process

- Status: accepted (not yet implemented)
- Date: 2026-11-24

## Context

The editor needs an integrated terminal: a PTY-backed shell, several sessions,
copy and paste, scrollback and search, and resizing.

## Decision

* **`alacritty_terminal` 0.26.0** (Apache-2.0) does the terminal emulation:
  the parser, the grid, the selection and the scrollback.
* **`pty-process` 0.5.4** (MIT) does the process side: it opens the PTY,
  spawns the shell and hands back the master side.

The two are kept apart, as the plan asks: the terminal view never touches a
file descriptor, and the process layer never knows about the grid.

## Why

| Option | Verdict |
| --- | --- |
| `alacritty_terminal` + `pty-process` | **chosen** — the maintained emulator, an actively maintained PTY layer |
| `alacritty_terminal` + `portable-pty` | `portable-pty` has had no release since February 2025 |
| `alacritty_terminal` + `nix` `openpty` | more code in the editor, no gain |
| Zed's `terminal` and `terminal_view` crates | GPL-3.0 and unpublished |

Zed uses a *fork* of `alacritty_terminal` and its own PTY layer. The editor
uses the upstream crate, so it does not have to carry a fork.

## Consequences

* The shell is started in the workspace directory, and the environment is the
  editor's, with `TERM` set for the emulator.
* The terminal renders into the same GPU surface as the rest of the editor;
  the grid is drawn from the emulator's cells rather than being a terminal
  emulator of its own.
* Resizing has to go both ways: the window resize reaches the PTY, and the
  process's answer reaches the grid.
* Nothing here is implemented yet. The plan puts it in Phase 3, after the
  editor shell is solid.
