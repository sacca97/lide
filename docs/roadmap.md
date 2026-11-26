# Roadmap

Where the project stands against [`plan.md`](../plan.md).

**Status: closed (2026-11-24).** Every item below is a result: delivered, or
deliberately not built with the reason beside it. There is no open work left in
this document; anything new belongs in a new roadmap.

## Phase 0 — feasibility (done, with one exception)

| Deliverable | State |
| --- | --- |
| Upstream audit of GPUI and the Zed crates | [done](../audit/upstream-audit.md) |
| A Linux build environment and reproducible instructions | [done](build.md) |
| A minimal application | done — a window with a file tree, tabs, a status bar |
| A text editor that opens, edits and saves a file | done |
| A Markdown rendering prototype | done — source and preview side by side |
| A PDF rendering prototype with text extraction | done — PDFium renders the page; the text layer is extracted with positions |
| A dependency and licence comparison and an ADR | [done](adr/) |
| A Cargo workspace with dependency boundaries | [done](adr/0008-crate-layout.md) |

The exception: the framework is [Rune](../adr/0001-ui-framework-rune.md), not
GPUI. The audit explains why, and the prototypes are better for it.

### The gaps the prototypes had

When the prototypes were first measured the editor had no line numbers,
syntax highlighting, find and replace, palette or file watching, and its
buffer was a plain `String`. Those are now Phase 1 and Phase 2 rows below;
the section is kept so that the starting point is not mistaken for the
current one.

## Phase 1 — usable editor shell

| Deliverable | State |
| --- | --- |
| Application window and workspace layout | done |
| Project/folder opening | done |
| File explorer: open files from the tree | done |
| File explorer: create, rename, delete, reveal | done, through the command palette |
| Tabs | done, with a dirty marker and a close action |
| Split panes | done: two panes, each with its own document |
| Basic text editor | done |
| Save, reload and dirty-state handling | done; closing a dirty file asks first |
| Find | done: case-insensitive with smart case, match count, next and previous |
| Replace | done: replace one, replace all |
| Command palette | done: filter, arrow keys, Enter to run, Escape to close |
| Settings and themes | done: light and dark, persisted to `$XDG_CONFIG_HOME/lide/settings.json` |
| Recent projects and files | done: recorded in the settings file and listed in the palette under their own groups |
| File watching and external-change handling | done: a clean file reloads silently, a dirty one is reported with Reload / Keep mine |
| Line numbers | done |
| Syntax highlighting | done, through `rune-kit`'s 80 built-in lexers |

### Deliberate limits in Phase 1

* **Tabs are per window, not per pane.** The two panes show the active
  document and the previous one; a second tab strip per pane is not built.
* **Long lines scroll horizontally rather than wrapping.** That is what keeps
  the gutter aligned with the text; soft wrap is a different editing model and
  is not built.

## Phase 2 — editing and language support

| Deliverable | State |
| --- | --- |
| Syntax highlighting in the editor | done (`rune-kit`'s 80 lexers) |
| LSP client | done: handshake, document sync, shutdown, server lifecycle |
| Diagnostics | done: a Problems panel, status-bar counts, click to jump |
| Go to definition | done (F12) |
| Workspace search with `ripgrep` | done: literal and regex, case, results that open at the match |
| Workspace replace | done: a review dialog counts the matches and files, dirty buffers stop it, and the writes run on a background thread |
| Hover | done: `Ctrl+K`, shown in a bar with Escape to dismiss |
| References | done: `Shift+F12`, listed in the Places panel |
| Formatting | done: `Ctrl+Shift+I`, the server's edits applied from the end |
| Completion | done: automatic after a word character (debounced) and on Ctrl+Space, a caret-anchored list, arrows to move, Enter to accept |
| Rename | done: F2, a prompt prefilled with the word under the caret, applied across every file the server names |
| Code actions | done: `Ctrl+.` lists what the server offers at the caret and applies the chosen edit |
| Diagnostics in the text | done: underlined in the editor in their severity's colour |

The client is [ADR 0007](adr/0007-lsp.md)'s answer: written here, because no
permissively licensed Rust LSP client exists. It is verified against real
clangd and rust-analyzer.

### Deliberate limits in Phase 2

* **The completion list is anchored by line and column**, not by the caret's
  rect: the text field does not publish it. It lands correctly because
  nothing wraps; it would drift if soft wrap were enabled.

## Phase 3 — terminal and Git

| Deliverable | State |
| --- | --- |
| Diff viewer | done: `Ctrl+G` opens the active file as its own document with old and new line numbers, tinted by kind |
| Conflict viewer | done: an unmerged file opens with ours and theirs marked and the conflict markers kept |
| PTY-backed terminal | done: a real shell in a PTY, several sessions, resized to its pane, scrollback by wheel, pointer selection, `Ctrl+Shift+F` search across the scrollback, `Ctrl+Shift+C` / `Ctrl+Shift+V` for the clipboard, and a dialog for the shell and start directory |

Git goes through the `git` command line. The graphical panel, staging, commits,
branch switching and the network operations were deliberately removed: the
editor's Git surface is the branch in the status bar and the read-only diff and
conflict views.

### Deliberate limits in Phase 3

* **A terminal search result is shown by scrolling its line to the top**, not
  by tinting the matching characters: the grid is drawn row by row, and
  per-cell highlighting would mean rebuilding its renderer.

## Phase 4 — Markdown and PDF

| Deliverable | State |
| --- | --- |
| Markdown source editor | done |
| Live preview, split mode | done |
| GFM: tables, task lists, links, fenced code | done |
| Math and Mermaid | done, through `rune-kit` |
| Heading outline and navigation | done: the outline is a pane beside the source, and clicking a heading puts the caret on its line |
| Synchronised scrolling between source and preview | done: a wheel over either pane moves the other to the same fraction of its content |
| Export to HTML | done (`lide-markdown`) |
| PDF page navigation, zoom, fit | done |
| PDF text search | done: a field in the header, every match on the page highlighted at its glyphs, next/previous |
| PDF bookmark navigation | done: the document's own outline is a pane beside the page, clickable to the page each entry names, nested entries indented |
| PDF text selection with the pointer | done: dragging over the page selects the characters under it and copies them on release |
| Image preview | done: an image opens as its own document, drawn scaled to fit |
| SVG source and preview | done: an SVG opens as its preview; "Toggle SVG source / preview" shows its source in the editor, and saving the source redraws the preview |
| CSV viewer | done: `.csv` and `.tsv` open as a table, with a header row, columns sized to their content, sorting by column and filtering by text |
| Large PDFs render on demand | done: pages are rendered when visited and cached |

## Phase 5 — LaTeX

| Deliverable | State |
| --- | --- |
| TexLab through the LSP client | configured; exercise it with texres available |
| `texres` builds | done: the default command is exactly `texres file.tex` |
| Build output and errors | done: a panel with the command, the log and the error count; `texres`'s `error:` / `--> file:line:column` blocks and the classic `file:line:` / `! …` / `l.N` forms are parsed |
| Build errors navigate to source | done: each error is a row in the build panel that opens the file at its line |
| Integrated PDF preview | done, through the PDF viewer |
| Forward and inverse SyncTeX | done: the `.synctex.gz` is parsed directly, with no `synctex` executable |
| Build recipes and engine selection | done: `texres` by default, with `-pdf`, `-xelatex` or `-lualatex` when a document needs an explicit engine; the palette cycles the engine and the choice is remembered |
| Build directory and auxiliary files | done: "LaTeX: output directory" sets `latex_output_directory`; the build passes `-output-directory DIR`, and the PDF and SyncTeX file go there |
| BibTeX and Biber | done through `texres`, which runs the bibliography tool the document asks for |
| Build on save | done: saving a changed `.tex` file schedules its build for the next frame; an unchanged save is a no-op, and there is no timer or watcher-driven rebuild |

The build runner is a general "program, arguments, directory" runner with
`texres` as the LaTeX command, which is what makes it testable on a machine
with no TeX: the tests run `sh` and check the streaming, the failure reporting,
the cancellation and the error parsing.

## Structured data and web files

| Deliverable | State |
| --- | --- |
| Syntax and highlighting for JSON, YAML, TOML, XML, HTML, CSS and JavaScript | done: `rune-kit`'s `lang_for_path` maps all of them, and the editor highlights through it |
| JSON, YAML and TOML validation | done: a palette command reports the parser's line and column; JSON through `serde_json`, TOML through `toml`, YAML through `yaml-rust2` |
| JSON formatting | done: two-space indentation, key order preserved |
| YAML and TOML formatting | **not offered**: rewriting them through a parser would drop comments and key order, and a lossy formatter is worse than none |
| XML formatting and validation | done: indented children, comments kept; a document that mixes text with child elements is refused rather than changed |
| Schema-aware validation | **not offered**: resolving a schema needs a network and file-access policy the plan does not set; the plan says "where available" |
| More advanced CSV editing | **not offered**: sorting and filtering cover reading; rewriting cells is a spreadsheet's job |
| SVG source editing and visual preview | done: see Phase 4 |

## Phase 6 — advanced features

| Deliverable | State |
| --- | --- |
| Task runner | done: the project's tasks are discovered and listed in the palette, and their output streams into the build panel |
| Notebook viewing | done: `.ipynb` opens as a column of cells, Markdown rendered, code with the output of its last run |
| DAP debugging | **dropped by request**: the user asked not to add debugging for now. The adapter is available (gdb 17.2 speaks DAP via `gdb --interpreter=dap`) if it returns |
| Extension system | **deferred by the plan's own condition**: only after stable internal APIs exist |

The task runner discovers what the project can already run — `cargo build`,
`cargo test`, `cargo run`, `npm test`, `make`, `cmake --build`, `pytest`,
`go test` — and the LaTeX build, rather than inventing tasks. Its runner is
`lide_latex::Build`, which is a general "program, arguments, directory, stream
the output" runner that happens to be named after the first thing that used
it.

## Closed

The roadmap is finished. The editor builds, opens files, edits, searches,
replaces, debugs nothing, previews Markdown/PDF/images/tables/notebooks/SVG,
integrates a shell, drives git, builds LaTeX with SyncTeX, checks structured
files, and is verified headlessly. The entries marked "not offered" and the
deliberate limits above are decisions, not unfinished work.

## How the work is checked

Every view is rendered headlessly and looked at:

```sh
./scripts/snapshots.sh     # PNGs of the workspace, editor, Markdown and PDF views
cargo test --workspace     # the same renders, plus the unit tests
```

That is the main reason for the framework change: the UI can be checked
without a display server, so the work does not need the user's screen.
