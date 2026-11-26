# Linux-First Native IDE --- Engineering Plan

## 1. Project vision

Build a lightweight, general-purpose code editor and document workspace
using **Rust and GPUI**, initially targeting Linux.

The product should offer the everyday workflow of an IDE such as VS
Code, without Electron, AI features, or a requirement to reproduce the
VS Code extension API.

It should support both software development and technical writing,
including Markdown, LaTeX, and PDF workflows.

### Product principles

-   Native Rust application; no Electron or embedded Chromium.
-   Linux-first, with architecture that can support macOS and Windows
    later.
-   Fast startup, responsive editing, and modest memory usage.
-   Keyboard-first interaction with configurable shortcuts.
-   Reuse mature upstream components where practical.
-   Prefer standard protocols and external tools over proprietary
    integrations.
-   Keep subsystems modular, but avoid premature abstraction and plugin
    infrastructure.
-   No AI, agent, LLM, or AI-chat functionality.

## 2. Initial platform scope

### Primary target

-   Linux desktop.
-   Wayland as a primary target; support X11 where the selected GPUI
    version permits it.
-   Native filesystem access and file watching.
-   PTY-based integrated terminal.
-   Shell and external-tool integration.

### Future targets

Keep platform-specific code behind narrow interfaces so macOS and
Windows can be added later. Do not spend the first milestone
implementing or testing those platforms.

### Distribution

Initially provide: - A reproducible development build. - A release
binary. - A simple packaging path, preferably AppImage or a native
package once dependencies are understood.

Do not commit to a packaging format until GPUI's runtime and
system-library requirements have been verified.

## 3. Product scope

### P0 --- Core editor and workspace

-   Open a folder as a workspace.
-   File explorer with create, rename, delete, and reveal operations.
-   Open, edit, save, and close files.
-   Multiple tabs and split panes.
-   Dirty-file indicators and save prompts.
-   Find and replace in the current file.
-   Global search and replace using ripgrep.
-   Command palette.
-   Configurable keyboard shortcuts.
-   Settings and theme selection.
-   Recent projects and files.
-   Basic status bar: language, line/column, encoding, line endings, and
    file state.
-   Large-file safeguards and sensible error handling.

### P0 --- Text editing

-   Unicode-correct text handling.
-   Efficient editing of large files.
-   Multiple selections and cursors.
-   Undo and redo.
-   Selection, navigation, and line operations.
-   Indentation and auto-indentation.
-   Bracket matching and surrounding-pair support.
-   Search and replace.
-   Syntax highlighting through Tree-sitter.
-   Basic language configuration and file-type detection.

### P0 --- Language support

-   Language Server Protocol (LSP) client.
-   Diagnostics and navigation.
-   Completion.
-   Hover information.
-   Go to definition and references.
-   Rename symbol.
-   Formatting.
-   Code actions.
-   Server lifecycle management and per-project configuration.

Use existing language servers rather than implementing language-specific
intelligence in the editor.

### P0 --- Integrated terminal

-   PTY-backed terminal sessions.
-   Multiple terminal tabs or panels.
-   Shell selection and configurable startup directory.
-   Copy, paste, scrollback, and search.
-   Terminal resizing and process lifecycle management.
-   Workspace-aware working directory.
-   Clear separation between terminal rendering and process management.

### P0 --- Markdown

-   Markdown source editing.
-   Live preview, including split source/preview mode.
-   GitHub Flavored Markdown support.
-   Tables, task lists, links, images, and fenced code blocks.
-   Heading outline and navigation.
-   Math rendering.
-   Mermaid diagrams.
-   Preview refresh without disrupting editor state.
-   Export to HTML; PDF export can use a documented external tool
    initially.

### P0 --- PDF viewing

-   Open PDFs as first-class workspace documents.
-   Page navigation, zoom, fit-to-width, and fit-to-page.
-   Text selection and copying.
-   Text search.
-   Bookmarks or outline navigation where available.
-   Multi-page rendering and sensible caching.
-   Handle large PDFs without loading every page into memory.
-   Support opening a PDF beside another document.

Start with viewing and navigation. Do not attempt arbitrary editing of
PDF page content in the initial scope.

### P1 --- LaTeX workflow

-   LaTeX syntax highlighting and LSP integration (for example, TexLab).
-   Build using `latexmk`.
-   Configurable build recipes and engine selection.
-   Build output and diagnostics panel.
-   Navigate from compiler errors to source locations.
-   Integrated PDF preview.
-   Forward and inverse SyncTeX navigation.
-   BibTeX and Biber workflows.
-   Configurable build directory and auxiliary-file handling.
-   Optional continuous build with debouncing and clear failure
    reporting.

LaTeX compilation should run as an external process. Do not embed a TeX
distribution.

### P1 --- Git

-   Repository status and changed-file indicators.
-   Diff viewer.
-   Stage and unstage files and hunks.
-   Commit and branch operations.
-   Diff navigation and conflict visibility.
-   Prefer Git CLI initially unless a concrete requirement justifies
    libgit2.

### P1 --- Structured data and web files

-   JSON, YAML, and TOML formatting and validation.
-   Schema-aware validation where available.
-   XML formatting and validation.
-   HTML/CSS/JavaScript syntax support.
-   SVG source editing and visual preview.
-   Image preview.
-   CSV viewer with a table-like presentation.

### P2 --- Advanced development features

-   Debug Adapter Protocol (DAP).
-   Task and build configurations.
-   Jupyter notebook support.
-   More advanced CSV editing.
-   Remote development, only if there is a clear use case.
-   Extension/plugin system.

## 4. Architecture

Use a Cargo workspace with clear subsystem boundaries. Avoid creating a
crate for every small helper; split where ownership, dependencies, or
testing boundaries justify it.

``` text
editor/
├── Cargo.toml
├── crates/
│   ├── app/              # Application entry point and lifecycle
│   ├── workspace/        # Projects, tabs, panes, layout
│   ├── ui/                # Shared GPUI components and styling
│   ├── editor/            # Editor view and editing interactions
│   ├── text/              # Buffers, selections, transactions, history
│   ├── syntax/            # Tree-sitter integration
│   ├── lsp/               # LSP client and server lifecycle
│   ├── terminal/          # Terminal view, PTY, process lifecycle
│   ├── documents/         # Document types and provider registry
│   ├── markdown/          # Markdown editing and preview
│   ├── pdf/               # PDF rendering and navigation
│   ├── latex/             # Build orchestration and SyncTeX
│   ├── git/               # Git status and operations
│   ├── search/             # Workspace search
│   ├── settings/          # Settings, keymaps, themes
│   └── platform/          # Linux-specific integrations
├── assets/
├── docs/
└── tests/
```

The proposed structure is a starting point, not a mandate. Consolidate
crates if separation creates unnecessary complexity.

### Dependency direction

-   `app` composes the application and owns startup/shutdown.
-   `workspace` coordinates views and documents.
-   `ui` contains reusable GPUI components; it should not own domain
    logic.
-   `editor` depends on text and syntax abstractions.
-   `documents` defines document lifecycle and capabilities.
-   Markdown, PDF, and LaTeX implement document-specific behavior.
-   LSP, terminal, Git, and search expose services to the workspace.
-   Platform-specific functionality stays behind narrow interfaces.

Avoid circular dependencies. Define interfaces at the boundary where
they are needed rather than introducing a large generic framework.

## 5. Technology choices to investigate

These are candidates, not assumptions. Verify current maintenance
status, licensing, Linux support, API stability, and compatibility
before committing.

  -----------------------------------------------------------------------
  Area                    Candidate               Notes
  ----------------------- ----------------------- -----------------------
  UI                      GPUI                    Pin a known-good
                                                  revision; expect
                                                  pre-1.0 API changes.

  Editor infrastructure   Zed crates              Audit individual crates
                                                  and their dependency
                                                  graph; do not blindly
                                                  fork all of Zed.

  Text buffer             Zed text infrastructure Benchmark editing,
                          or Ropey                memory, Unicode, and
                                                  large-file behavior
                                                  before choosing.

  Syntax                  Tree-sitter             Incremental parsing and
                                                  language grammars.

  Language intelligence   LSP                     Reuse external language
                                                  servers.

  Debugging               DAP                     Defer until core editor
                                                  is stable.

  Terminal                Alacritty terminal      Separate terminal
                          components or another   emulation from
                          maintained terminal     PTY/process management.
                          core                    

  PTY                     Portable-PTY candidate  Verify Linux behavior
                          or platform-specific    and future portability.
                          PTY layer               

  Search                  ripgrep                 Invoke as a subprocess
                                                  initially.

  Filesystem watching     notify                  Handle event coalescing
                                                  and dropped events.

  Markdown                pulldown-cmark          Check extension support
                                                  and rendering needs.

  Math                    KaTeX-compatible or     Decide based on GPUI
                          native rendering        rendering capabilities;
                          strategy                avoid embedding
                                                  Chromium.

  Mermaid                 External renderer or a  Keep optional; do not
                          native implementation   make it a P0
                                                  dependency.

  PDF                     PDFium via Rust         Compare licensing,
                          bindings, or MuPDF      packaging, text
                                                  selection, annotations,
                                                  and Linux deployment.

  LaTeX                   latexmk + SyncTeX +     Use installed system
                          TexLab                  tools.

  Git                     Git CLI initially       Simple, predictable,
                                                  and widely available.
  -----------------------------------------------------------------------

### Upstream reuse policy

Before implementing editor infrastructure:

1.  Inspect the current Zed repository and its crate documentation.
2.  Identify the smallest set of reusable crates for GPUI, text editing,
    terminal, and LSP.
3.  Check licenses and transitive dependencies.
4.  Build a minimal Linux prototype against pinned revisions.
5.  Record any patches and upstream divergence.
6.  Avoid copying large modules until their ownership and maintenance
    cost are understood.

Do not assume Zed's internal APIs are stable public APIs. Keep
integration points small enough that upstream changes can be handled
deliberately.

## 6. Document provider design

Documents should be opened through a registry that selects the
appropriate view based on file type and user choice.

A document provider should define only the capabilities it needs, such
as:

-   Open and close lifecycle.
-   Save or export, if applicable.
-   Reload and external-change handling.
-   Title and dirty state.
-   Search or text extraction, where supported.
-   Navigation and outline.
-   Preview or source/preview pairing.
-   Contextual commands.

Do not force every document type into a text-buffer interface. A PDF
viewer, image viewer, Markdown preview, and source editor have different
interaction models.

For Markdown and LaTeX, support paired views: - Source editor. -
Rendered preview. - Synchronized navigation where practical.

For LaTeX, the PDF is an output artifact, not the editable source
document.

## 7. Linux-specific requirements

-   Test on a mainstream Wayland desktop and at least one X11 session if
    supported.
-   Verify clipboard integration, keyboard input, IME behavior,
    drag-and-drop, and file dialogs.
-   Respect XDG directories for configuration, cache, and data.
-   Use desktop portals where appropriate.
-   Verify font fallback, ligatures, emoji, and Unicode rendering.
-   Handle filesystem events from common Linux filesystems.
-   Avoid blocking the UI thread on filesystem, process, parsing, or
    rendering work.
-   Provide clear diagnostics when optional system tools are missing.
-   Document required system libraries and runtime dependencies.

## 8. Performance and reliability goals

Establish measurements before optimizing. Record baseline startup time,
idle memory, file-open latency, typing responsiveness, and PDF rendering
behavior.

Initial engineering targets:

-   UI remains responsive during builds, searches, LSP startup, and file
    watching.
-   No synchronous filesystem or process operations on the UI thread.
-   Incremental parsing and rendering for normal editing.
-   Search and build output streamed incrementally.
-   Large files have a documented fallback strategy.
-   PDF pages render on demand with bounded caching.
-   Crashes in an external language server or build process do not crash
    the editor.
-   Recover gracefully from deleted, renamed, or externally modified
    files.

Do not promise fixed memory or latency numbers until representative
benchmarks exist.

## 9. Development roadmap

### Phase 0 --- Feasibility and upstream audit

Deliverables: - Audit GPUI Linux support and build requirements. - Audit
Zed crates and identify reusable editor components. - Compare candidate
text-buffer implementations. - Prototype PDF rendering and Markdown
rendering without Chromium. - Verify terminal and PTY options. - Verify
PDF library licensing and packaging implications. - Create an
architecture decision record for each major dependency.

Exit criteria: - A minimal GPUI application builds and runs on the
target Linux environment. - A text editing prototype can open, edit, and
save a file. - A PDF prototype can render pages and extract/select
text. - A Markdown prototype can render the required syntax. -
Dependency and licensing decisions are documented.

### Phase 1 --- Usable editor shell

Deliverables: - Application window and workspace layout. -
Project/folder opening. - File explorer. - Tabs and split panes. - Basic
text editor. - Save/reload and dirty-state handling. - Command
palette. - Settings and themes. - Find and replace.

Exit criteria: - A user can open a project, edit multiple files, split
views, save, close, and reopen the workspace. - Common operations are
keyboard accessible. - UI remains responsive during ordinary file
operations.

### Phase 2 --- Editing and language support

Deliverables: - Tree-sitter syntax highlighting. - LSP lifecycle and
client. - Diagnostics, completion, hover, navigation, rename,
formatting, and code actions. - Workspace search using ripgrep. - File
watching and external-change handling.

Exit criteria: - At least two representative language servers work
end-to-end. - Diagnostics and completion work without blocking typing. -
Search results navigate reliably to source locations.

### Phase 3 --- Terminal and Git

Deliverables: - PTY-backed terminal. - Multiple terminal sessions. -
Workspace-aware shell startup. - Git status and diff viewer. - Stage,
unstage, and commit operations.

Exit criteria: - Terminal processes survive ordinary panel navigation. -
Terminal resizing and shutdown are reliable. - Git changes are reflected
after editor and external operations.

### Phase 4 --- Markdown and PDF

Deliverables: - Markdown source editor. - Live preview and split mode. -
GFM support, math, and Mermaid strategy. - PDF document provider. - PDF
navigation, search, text selection, and caching. - Image preview.

Exit criteria: - Markdown documents render correctly for the supported
feature set. - Preview updates without losing editor state. - Large PDFs
render pages on demand without unbounded memory growth.

### Phase 5 --- LaTeX environment

Deliverables: - TexLab or another LSP integration. - latexmk build
recipes. - Build diagnostics and output. - Integrated PDF preview. -
Forward and inverse SyncTeX. - BibTeX/Biber support.

Exit criteria: - A multi-file LaTeX project can be built from the
editor. - Build errors navigate to source. - Source-to-PDF and
PDF-to-source navigation work on a representative project. - Failed
builds do not disrupt editing.

### Phase 6 --- Advanced IDE features

Deliverables: - DAP debugging. - Task runner. - CSV viewer. -
Structured-data tooling. - Jupyter support if justified. - Extension
system only after stable internal APIs exist.

Exit criteria: - Each advanced feature has independent tests and can be
omitted without destabilizing the core editor.

## 10. Testing strategy

### Unit tests

-   Text-buffer operations, Unicode, selections, and undo/redo.
-   Document-provider lifecycle.
-   Settings parsing and keymap resolution.
-   Search result parsing.
-   LSP message and lifecycle handling.
-   Terminal process lifecycle.
-   Markdown parsing and rendering behavior.
-   PDF navigation and page-cache behavior.
-   LaTeX command construction and SyncTeX parsing.

### Integration tests

-   Open/edit/save/reopen workspace.
-   External file modification and rename.
-   LSP startup, diagnostics, completion, and shutdown.
-   Search across a sample project.
-   Terminal startup, input, resize, and exit.
-   Git status and diff updates.
-   Markdown preview refresh.
-   PDF page rendering and search.
-   LaTeX build and SyncTeX navigation.

### Manual validation

Maintain a small set of representative projects: - Rust project. -
Python project. - C/C++ project. - Multi-file LaTeX paper with
bibliography and figures. - Markdown documentation with tables, math,
and diagrams. - Large source file. - Large PDF.

## 11. Explicit non-goals

Do not implement these in the initial project:

-   AI assistants, LLM integrations, or coding agents.
-   VS Code extension API compatibility.
-   A browser engine or Chromium embedding.
-   A custom compiler or language-specific analysis engine.
-   Arbitrary PDF content editing.
-   A full office suite.
-   Remote development.
-   A general-purpose plugin marketplace.
-   macOS and Windows release support before the Linux product is
    stable.

## 12. Engineering workflow and project rules

-   Keep `main` buildable.
-   Use pinned dependency revisions for fast-moving upstream projects.
-   Record architecture decisions and dependency rationale.
-   Prefer small, reviewable changes.
-   Require tests for core editing and document lifecycle behavior.
-   Benchmark before and after performance-sensitive changes.
-   Avoid speculative abstractions and premature plugin APIs.
-   Do not silently replace upstream components; document forks and
    patches.
-   Keep UI code separate from blocking I/O and process management.
-   Treat errors from external tools as normal user-facing states.
-   Provide useful logs without flooding the UI.

## 13. First implementation task

Start with **Phase 0**, not the full editor.

The first coding task should produce:

1.  A short upstream audit of GPUI and relevant Zed crates.
2.  A Linux build environment and reproducible build instructions.
3.  A minimal GPUI application.
4.  A prototype text editor that opens, edits, and saves a file.
5.  A Markdown rendering prototype.
6.  A PDF rendering prototype with text selection or extraction.
7.  A dependency/license comparison and architecture decision record.
8.  A proposed initial Cargo workspace with dependency boundaries.

Do not begin implementing the complete workspace until these prototypes
have been evaluated.

## Definition of success

The first meaningful release is a stable Linux application that can
replace a lightweight VS Code workflow for everyday coding and technical
writing: editing source files, using LSP, working in a terminal,
reviewing Git changes, writing Markdown, and reading/building LaTeX with
an integrated PDF preview.

The product should remain smaller in scope and simpler to maintain than
VS Code, while delivering a polished native experience.

