# lide

A lightweight, native code editor and document workspace for Linux, with a
best-effort macOS build (see [`docs/build.md`](docs/build.md)).

`lide` is written in Rust with [Rune](https://github.com/anomalyco/rune), a
native GPU-rendered UI framework. It has no Electron, no embedded browser and
no AI features — just the everyday editing workflow of an IDE, with Markdown,
LaTeX and PDF support.

> **Status: working prototype.** Phases 0–3 of [`plan.md`](plan.md) are done and
> Phase 5 is started. See [`docs/roadmap.md`](docs/roadmap.md) for exactly what
> works and what does not.

## What works

**Editing**

- Open a folder as a workspace, with a file tree in the sidebar. The sidebar
  starts hidden; the status bar's leftmost button or `Ctrl+B` shows and hides
  it. When shown, a compact activity bar runs down its left edge, with an accent
  tick.
- Rounded tabs with close buttons, and a neutral status bar with the path,
  branch, problems, language and cursor. A middle click closes a tab. The panel,
  editor and status bar are rounded cards on a darker window; the active file is
  highlighted in the tree, and the gutter has a rule against the text. The
  sidebar can be resized by dragging its edge and collapsed with `Ctrl+B` or the
  rail's chevron. Folders start collapsed, and how far each workspace was opened
  is remembered.
- Line numbers, syntax highlighting for 80 languages, undo and redo.
- Find and replace in the file, with smart case and replace-all.
- A command palette: every action is in it, `Ctrl+Shift+P`, together with the
  project's tasks and recently opened files and folders.
- Light and dark themes, settings persisted to `$XDG_CONFIG_HOME/lide`.

**Projects**

- Workspace search with `ripgrep`: literal or regex, click a result to open it
  at the match, and replace across files after a review dialog that counts the
  matches and refuses dirty buffers.
- A file watcher: a clean file reloads silently, a file with unsaved changes is
  reported and never overwritten.

**Language intelligence** (through a real LSP client)

- Diagnostics underlined in the editor and listed in a Problems panel, with
  the error and warning count in the status bar.
- Completion, hover, go to definition, find references and formatting.
- Servers start on a background thread; a missing binary costs features, never
  the editor.

**Terminal and Git**

- An integrated terminal: a real shell in a PTY, several sessions, resized with
  the window, with wheel scrollback, pointer selection, `Ctrl+Shift+F` to
  search it, `Ctrl+Shift+C` / `Ctrl+Shift+V` for the clipboard, ANSI colours and
  bold text, and "Terminal settings" for the shell and start directory.
  **SSH: connect** opens
  `ssh host` in a terminal, listing the aliases from `~/.ssh/config`.
- A read-only diff for the current file, including a two-sided conflict view
  for unmerged files. Staging, commits, branches, pull and push stay in the
  terminal.

**Documents**

- Markdown with a live preview: tables, task lists, links, fenced code, math and
  Mermaid diagrams, a navigable heading outline, and source and preview that
  scroll together.
- PDFs: previous/next page controls, zoom buttons with `Ctrl+=`, `Ctrl+-` and
  `Ctrl+0`, pinch and Ctrl+wheel zoom, smooth wheel scrolling that turns the
  page at the end of the page, text search with the matches highlighted at their
  glyphs, text selection by dragging, the document's own bookmark outline, and
  on-demand rendering.
- Images, CSV and TSV tables (sortable and filterable), and Jupyter
  notebooks, each as its own document.
- SVG opens as its preview; the palette's "Toggle SVG source / preview" shows
  the source, and saving it redraws the preview wherever it is open.
- JSON, TOML, YAML and XML are checked with the parser's line and column, and
  JSON and XML can be formatted without losing key order or comments.

**LaTeX**

- `texres` is the builder (`texres file.tex`), with the output streaming into
  a panel and every error clickable.
- A successful build opens the PDF **beside the source**, leaving the `.tex`
  in the other pane; "LaTeX: open PDF beside source" does the same from the
  palette when the PDF already exists.
- The engine is chosen from the palette (`auto`, `pdflatex`, `xelatex`, or
  `lualatex`), and the choice is remembered. Builds can write into a separate
  output directory, logs and SyncTeX file included. Saving a `.tex` file
  rebuilds it; nothing rebuilds on a timer.
- Double-click a point on the PDF to jump back to the source; the status bar's
  **PDF** / **Source** chip or `Ctrl+Shift+J` jumps the other way.

## Building

See [`docs/build.md`](docs/build.md). The short version:

```sh
sudo dnf install -y vulkan-loader mesa-vulkan-drivers \
    wayland-devel libxkbcommon-devel libxkbcommon-x11 fontconfig-devel

./scripts/fetch-pdfium.sh     # PDF rendering (runtime only)
cargo build --release
```

## Running

```bash
lide                  # empty workspace
lide .                # open the current folder
lide README.md        # open a file
```

On macOS, read every `Ctrl` in this table as `Cmd`, except the terminal's
interrupt: `Ctrl+C` still goes to the shell, and the terminal's clipboard is
`Cmd+C` / `Cmd+V`.

| Shortcut | Action |
| --- | --- |
| `Ctrl+Shift+P` | Command palette |
| `Ctrl+P` / `Ctrl+N` | Move the palette's highlight |
| `Ctrl+S` / `Ctrl+Shift+S` | Save / save all |
| `Ctrl+W` / `Ctrl+Q` | Close tab / quit |
| `Ctrl+F` / `Ctrl+H` | Find / replace in file |
| `Ctrl+Shift+F` | Search the workspace |
| `Ctrl+B` | Toggle the sidebar |
| `Ctrl+\` | Toggle the split |
| `F12` / `Shift+F12` | Go to definition / find references |
| `Ctrl+K` / `Ctrl+Shift+I` | Hover / format document |
| `Ctrl+G` | Diff the current file |
| `` Ctrl+` `` | Terminal |
| `Ctrl+Shift+B` | Build the LaTeX document |
| `Ctrl+Shift+J` | Jump between the LaTeX source and its PDF (SyncTeX) |
| `Ctrl+PageUp` / `Ctrl+PageDown` | Previous / next PDF page |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | PDF zoom in / out / actual size |

## Checking the UI without a display

The editor renders itself headlessly with the same renderer the window uses, so
the views can be inspected without a display server — which is also how the
test suite checks them:

```sh
./scripts/snapshots.sh          # a PNG of every view and state
cargo test --workspace          # the same renders, plus the unit tests
```

## Layout

```text
crates/
├── app/          # the binary: startup, the window, the global keys
├── workspace/    # the project, files, tabs, panes, window and theme
├── editor/       # the source editor view
├── text/         # UTF-8 and line-ending helpers (no UI)
├── documents/    # document kinds and the parsers behind the viewers
├── markdown/     # the Markdown model, the outline and the HTML export
├── pdf/          # PDF structure, rendering, text and search
├── lsp/          # the Language Server Protocol client
├── terminal/     # the PTY session and the VT emulator
├── git/          # status and diff parsing through the git CLI
├── latex/        # texres builds and the error parsing
├── search/       # workspace search through ripgrep
└── platform/     # Linux specifics: XDG paths, the session, the tools
```

[`docs/adr/0008-crate-layout.md`](docs/adr/0008-crate-layout.md) explains the
dependency direction.

## Documentation

- [`plan.md`](plan.md) — the engineering plan this project follows.
- [`docs/features.md`](docs/features.md) — a flat inventory of features, file
  formats and shortcuts.
- [`docs/remote.md`](docs/remote.md) — the SSH agent, its protocol, and the
  remaining remote-workspace work.
- [`docs/roadmap.md`](docs/roadmap.md) — the closed status matrix.
- [`docs/audit/upstream-audit.md`](docs/audit/upstream-audit.md) — the audit that
  chose the framework and the dependencies.
- [`docs/adr/`](docs/adr/) — the architecture decision records.
- [`docs/build.md`](docs/build.md) — the build environment and instructions.

## Licence

MIT OR Apache-2.0.
