# Features

A flat inventory of what lide does, including the file formats it opens and
what each one can do. The status matrix is [`roadmap.md`](roadmap.md); the
reasoning is in [`adr/`](adr/).

lide is a native Linux editor written in Rust on
[Rune](../docs/adr/0001-ui-framework-rune.md): no Electron, no Chromium, no
embedded browser, no AI features. A macOS build is best-effort and has not been
verified.

## Window and workspace

- Open a folder as the workspace; open one or more files from the tree.
- Activity bar: Explorer, Search, Problems, Places, with an accent tick on the
  active mode. It starts hidden together with the panel; the status bar's
  leftmost button or `Ctrl+B` shows it.
- Rounded tabs with close buttons; a middle click closes one. The panel, editor
  and status bar are rounded cards. The active file is highlighted in the tree
  and the gutter has a rule against the text.
- Resizable sidebar: drag its edge (180–560 points); the width is remembered.
- Directories collapse on click (▸/▾), and the open file is highlighted. A new
  workspace starts with every folder collapsed; the open ones are remembered
  per workspace.
- Collapsible sidebar: `Ctrl+B`, the status bar's leftmost button, or the rail's
  chevron hides the whole bar; the same controls bring it back.
- Flat tab strip with dirty markers and close buttons; two split panes, each
  with its own document and a draggable divider. Clicking a tab opens it in
  the left pane; the palette's **Focus the other pane** switches which pane a
  tab opens into, dragging a tab onto the other pane moves it there, and the
  **X** in the strip closes the split. The focused pane carries an accent rule.
- Neutral status bar with branch, problems, language, cursor, size and server.
- Light and dark themes; settings persist in `$XDG_CONFIG_HOME/lide/settings.json`
  (override with `LIDE_CONFIG_DIR`).
- Recent files and projects are remembered and listed in the command palette.
- File watcher: a clean file reloads silently; a file with unsaved changes is
  reported with **Reload** / **Keep mine**, never overwritten.

## Editing

- UTF-8 text (with BOM), LF/CRLF detection and preserved line endings; editing,
  undo and selections come from Rune's text editor model.
- Line numbers, syntax highlighting, undo/redo.
- Find and replace in the file, smart case, next/previous, replace-all.
- Save, save all, reload from disk, dirty-state prompt on close.
- Split editing; each pane follows its own document.
- Horizontal scrolling for long lines (no soft wrap; see [limits](#not-included)).

## Language intelligence

A hand-written LSP client, verified against real servers. A missing server
costs features, never the editor.

| Language | Server | Extensions |
| --- | --- | --- |
| Rust | `rust-analyzer` | `.rs` |
| Python | `pylsp` | `.py`, `.pyi` |
| C | `clangd` | `.c`, `.h` |
| C++ | `clangd` | `.cc`, `.cpp`, `.cxx`, `.hpp`, `.hh` |
| Go | `gopls` | `.go` |
| TypeScript / JavaScript | `typescript-language-server` | `.ts`, `.tsx`, `.js`, `.jsx` |
| LaTeX | `texlab` | `.tex`, `.bib` |

Features: diagnostics (underlined in the text and listed in Problems),
completion, hover, go to definition, find references, document formatting,
rename across files, and code actions.

## Search and replace

- Workspace search through `ripgrep`: literal or regex, case-sensitive or not.
- Results grouped by file; click one to open at the match.
- Replace across the workspace behind a review dialog that counts matches and
  files; open dirty buffers stop it; writes run on a background thread.

## Files and formats

| Format | Extensions | How it opens | Capabilities |
| --- | --- | --- | --- |
| Markdown | `.md`, `.markdown`, `.mdown`, `.mkd` | source + outline + live preview | GFM tables, task lists, links, fenced code, math, Mermaid; synchronised scrolling; heading outline navigation; HTML export available in `lide-markdown` |
| PDF | `.pdf` | rendered document | previous/next page controls, zoom buttons, `Ctrl+=` / `Ctrl+-` / `Ctrl+0`, pinch and Ctrl+wheel zoom, smooth wheel scrolling that turns the page at either end, text search with glyph highlights, pointer text selection, bookmark outline, on-demand rendering and caching |
| Images | `.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.bmp`, `.ico`, `.avif`, `.tiff` | preview scaled to fit | pixel size in the status bar |
| SVG | `.svg` | preview | **Toggle SVG source / preview**; saving the source redraws the preview |
| CSV / TSV | `.csv`, `.tsv` | table | RFC 4180 quoting/newlines, header row, sort by column, filter by text, row/column caps with a note |
| Jupyter notebook | `.ipynb` | column of cells | Markdown rendered, code with its last output, errors marked; read-only |
| JSON | `.json` | text | validation with line/column; two-space formatting that keeps key order |
| TOML | `.toml` | text | validation with line/column |
| YAML | `.yaml`, `.yml` | text | validation with line/column |
| XML | `.xml` | text | validation; indented formatting that keeps comments, refuses mixed content |
| Text / code | see below | text editor | highlighting where `rune-kit` has a lexer |
| Diff | generated | side-by-side-ish diff document | old/new line numbers, added/removed tinting, counts |
| Terminal | not a file | terminal document | see [Terminal](#terminal) |

**Text extensions recognised explicitly:** `.txt`, `.log`, `.text`, `.rst`,
`.adoc`, `.org`, `.toml`, `.yaml`, `.yml`, `.json`, `.jsonc`, `.xml`, `.csv`,
`.tsv`, `.ini`, `.cfg`, `.conf`, `.env`, `.gitignore`, `.gitattributes`,
`.bash`, `.sh`, `.zsh`, `.fish`, `.ps1`, `.bat`, `.cmd`, `.lua`, `.rb`, `.py`,
`.pyi`, `.rs`, `.go`, `.js`, `.jsx`, `.ts`, `.tsx`, `.c`, `.h`, `.cc`, `.cpp`,
`.cxx`, `.hpp`, `.hh`, `.java`, `.kt`, `.kts`, `.scala`, `.swift`, `.m`, `.mm`,
`.cs`, `.fs`, `.fsx`, `.clj`, `.cljs`, `.ex`, `.exs`, `.erl`, `.hrl`, `.hs`,
`.ml`, `.mli`, `.nim`, `.zig`, `.dart`, `.r`, `.jl`, `.jl2`, `.tex`, `.sty`,
`.cls`, `.bib`, `.sql`, `.vue`, `.svelte`, `.asm`, `.s`, `.make`, `.mk`,
`.cmake`, `.gradle`, `.proto`, `.graphql`, `.gql`, `.diff`, `.patch`,
`.desktop`, `.service`, `.timer`, `.socket`.

Other files open as text when their first bytes are valid UTF-8; known PDF and
image signatures are recognised without an extension. Non-UTF-8 files that are
not recognised report an error rather than being guessed at.

## Terminal

- Real shell in a PTY, several sessions, resized to its pane.
- Scrollback by wheel and `Ctrl+Shift+F` search across it.
- Pointer selection of grid cells; copy takes the selection.
- `Ctrl+Shift+C` / `Ctrl+Shift+V` clipboard; ANSI colours and bold output are
  rendered, and the cursor is drawn at its actual grid column.
- Configurable shell and start directory; empty means `$SHELL` and the project
  root. Relative start directories are resolved from the project root, `~/`
  from home.
- **SSH: connect** opens `ssh host` in a terminal. It lists the `Host` aliases
  from `~/.ssh/config` and accepts a typed `user@host`; `Include` directives
  are not followed.
- **SSH: open remote file** opens one remote text or Markdown file through
  `lide-agent`; edits save back over the agent. It requires `lide-agent` on the
  remote `PATH`. Remote PDF, image, table and notebook viewers are not wired
  yet.

## Git

- `Ctrl+G` opens a read-only diff for the file in the active editor.
- An unmerged file opens as a two-sided conflict view: the lines between
  `<<<<<<<` and `=======` are shown as ours, the lines between `=======` and
  `>>>>>>>` as theirs, with the markers kept between them.
- Staging, commits, branch switching, pull and push are deliberately left to
  the terminal. The status bar still shows the current branch.

## LaTeX

- `texres` builds the document with exactly `texres file.tex` by default; the
  editor passes the file and nothing else. `texres` writes SyncTeX and keeps
  non-stop output of its own.
- Engine selection: `auto`, `pdflatex`, `xelatex`, or `lualatex`. An explicit
  engine becomes `-pdf`, `-xelatex`, or `-lualatex`; `auto` is the bare command.
- Output directory (`-output-directory DIR`), so the final PDF goes where the
  project asks.
- Streaming output panel; `texres`'s `error:` / `--> path:line:column` blocks
  and the classic `path:line:` / `! … / l.N` forms are clickable.
- A successful build opens the PDF **beside the source** (the source stays in
  the left pane and the PDF takes the right), and an already open preview is
  reloaded when the build writes a new PDF. The tab strip's **Preview** button
  opens an existing PDF without building; the palette command does the same.
  **`Ctrl+S` saves and builds the active `.tex`** even when the file has no
  changes, and any changed `.tex` builds when it is saved (no timers, no
  watcher rebuilds). `Ctrl+Shift+B` builds explicitly.
- Forward SyncTeX (source line → PDF page) and inverse (PDF point → source),
  read directly from the `.synctex.gz` the build already writes; no separate
  `synctex` executable is needed. Double-click a point on the PDF to go to the
  source, and `Cmd`/`Ctrl`-click a source line to go to the PDF. The status
  bar's **PDF** / **Source** chip or `Ctrl+Shift+J` does the same.
- BibTeX/Biber handled by `texres`.

## Settings

`settings.json`, or `$LIDE_CONFIG_DIR/settings.json` when set.

| Key | Meaning |
| --- | --- |
| `dark` | dark or light theme |
| `font_size` | editor font size in points |
| `sidebar_width` | sidebar width in points |
| `show_sidebar` | sidebar visibility |
| `split` | whether the editor starts split |
| `split_ratio` | share of the width the left split pane takes |
| `markdown_ratio` | share of a Markdown tab the source takes |
| `show_outline` | Markdown/PDF outline visibility |
| `soft_wrap` | wrap text to the editor width (on by default) |
| `line_numbers` | gutter visibility (hidden while soft wrap is on) |
| `recent_files`, `recent_projects` | recent lists shown in the palette |
| `latex_engine` | `auto`, `pdflatex`, `xelatex`, `lualatex` |
| `latex_output_directory` | build output directory |
| `terminal_shell` | shell for new terminals (empty: `$SHELL`) |
| `terminal_directory` | start directory (empty: project root) |

## Keyboard shortcuts

On macOS, the platform delivers these as `Cmd` chords; the labels below keep
the Linux spelling. The terminal is the exception: `Cmd+C` / `Cmd+V` are its
clipboard, while `Ctrl+C` still sends the shell an interrupt.

| Shortcut | Action |
| --- | --- |
| `Ctrl+Shift+P` | Command palette |
| `Ctrl+P` / `Ctrl+N` | Move the palette's highlight |
| `Enter` / `Esc` | Run / close the palette, dialog or bar |
| `Ctrl+S` / `Ctrl+Shift+S` | Save (and build the active `.tex`) / save all |
| `Ctrl+W` | Close tab |
| `Ctrl+R` | Reload from disk |
| `Ctrl+N` | New file (outside the palette) |
| `Ctrl+F` / `Ctrl+H` | Find / replace in file |
| `Ctrl+Shift+F` | Search the workspace (search the terminal when it is in front) |
| `Ctrl+B` / `Ctrl+\` | Toggle sidebar / split |
| `Ctrl+Shift+O` | Toggle the Markdown/PDF outline |
| `F12` / `Shift+F12` | Go to definition / find references |
| `Ctrl+K` | Hover information |
| `Ctrl+.` | Code actions |
| `F2` | Rename symbol |
| `Ctrl+Shift+I` | Document formatting (LSP) |
| `Ctrl+G` | Diff the current file |
| `` Ctrl+` `` | Open terminal |
| `Ctrl+Shift+C` / `Ctrl+Shift+V` | Terminal copy / paste |
| `Ctrl+Shift+B` | Build the LaTeX document |
| `Ctrl+Shift+J` | Jump between the LaTeX source and its PDF (SyncTeX) |
| `Ctrl+PageDown` / `Ctrl+PageUp` | Next / previous PDF page |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | PDF zoom in / out / actual size |

## Command line and environment

```sh
lide [path ...]        # files open; the first folder becomes the workspace root
```

| Variable | Effect |
| --- | --- |
| `LIDE_CONFIG_DIR` | settings directory override (tests and snapshots use it) |
| `LIDE_PDFIUM_PATH` / `PDFIUM_LIBRARY_PATH` | PDFium library location |
| `LIDE_LOG` | log filter for stderr (e.g. `debug`) |

Rune's tooling flags are available for headless checking: `--snap <png>`,
`--tree <json>`, `--scene <name>`, `--size <WxH>`.

## Headless checks

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
./scripts/snapshots.sh /tmp/out     # one PNG per view, no display server
```

## Not included

Deliberate scope decisions, recorded here so they are not mistaken for bugs:

- **No debugger (DAP).** gdb can speak DAP, but the client is not built.
- **No extension system.** The plan conditions it on stable internal APIs.
- **No AI features, no Electron/Chromium.**
- **No YAML/TOML formatting.** A parse-and-print would drop comments and key
  order; validation is offered instead.
- **No schema-aware validation.**
- **No CSV cell editing.** Viewing, sorting and filtering only.
- **No soft wrap.** Long lines scroll horizontally so the gutter stays aligned.
- **No per-pane tab strips.** Tabs are per window; panes show the active and
  previous document.
- **No terminal match highlighting.** Search scrolls the matching line to the
  top rather than tinting characters.
- **No PDF editing.** Viewing and navigation only.
