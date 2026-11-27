# 0001 — The UI framework is Rune

- Status: accepted
- Date: 2026-11-24

> **Amended (2026-11-27).** The crate that held the crates that do not draw anything was folded into
> `lide-workspace`; see [0008](0008-crate-layout.md) for the current
> layout. The decision itself stands.
- Supersedes: the original plan's choice of GPUI

## Context

The plan names GPUI (from the Zed repository) as the UI framework. Before
writing any code against it, Phase 0 asks for an audit of the upstream
components. Two things came out of that audit and from building a prototype
against GPUI:

1. **GPUI 0.2.2 is frozen.** The crates.io release dates from October 2025 and
   Zed's tree has moved on by hundreds of commits, splitting the crate into
   `gpui_platform`, `gpui_linux`, `gpui_wgpu` and more — none of which are
   published. The published crate's own README tells users to depend on
   `gpui_platform`, which does not exist on crates.io.
2. **GPUI has no headless rendering.** There is no way to draw a window's
   content without a display server, and no way to drive synthetic input in a
   test.

The second point turned out to matter a great deal. This editor is being built
by an agent that has to be able to check its own work; on a machine with a
busy Wayland session, opening a window means stealing the user's focus and
covering their screen.

## Decision

Use [Rune](https://github.com/anomalyco/rune) as the UI framework, pinned to
revision `ac452a9c2b75c462c5eb1659024178f1fbbd416a` (2026-11-24).

Rune is a native, GPU-rendered Rust UI framework with its own renderer, text
engine, layout (Taffy) and animation system, built on `winit`. It is MIT
licensed.

## Why Rune

| Requirement | GPUI 0.2.2 | Rune |
| --- | --- | --- |
| Licence | Apache-2.0 | MIT |
| Release state | 0.2.2, frozen since 2025-10-22 | pre-release `dev`, commits on 2026-11-24 |
| Linux | Wayland + X11, Vulkan only (blade) | Wayland + X11 via `winit`, Vulkan or GLES via `wgpu` |
| Window activation on Wayland | the application id is dropped on the floor, so the activation token is invalid and the window never gets focus | `xdg_activation_v1` is handled, with the token bound to the window |
| Headless render to PNG | not possible | `--snap out.png` |
| Element tree dump | not possible | `--tree tree.json` |
| Scripted input in tests | not possible | `Runtime` API in unit tests, `--script` for scenarios |
| Text editing | none | `rune-edit`: selections, undo, IME, key handling |
| Syntax highlighting | none | `rune-highlights`: 80 languages |
| Markdown, math, Mermaid | none | `rune-kit`: `markdown`, `math`, `mermaid` |
| Diff views | none | `rune-kit`: `diff` |

The last four rows are the ones that decide it. The plan asks for Markdown
preview with math and Mermaid diagrams, syntax highlighting, a diff viewer and a
code editor with undo and IME. With Rune those are existing, tested components;
with GPUI every one of them would have to be written from scratch.

The headless rendering is what makes the rest of the project workable: the
editor can be rendered to a PNG and inspected without a display, the views can
be checked by `cargo test`, and a scenario can be replayed from a script.

### What it costs

* **The API moves.** Rune is pre-release and its API is not stable. Every
  upstream bump is a migration. The revision is pinned exactly, and the
  `[patch.crates-io]` table in the workspace root mirrors Rune's own, because
  Cargo does not inherit patches from a dependency.
* **The vendored crates have to be patched too.** Rune ships patched copies of
  `winit`, `wgpu-hal`, `naga`, `parley_engine`, `sctk-adwaita` and the
  accesskit crates. The workspace has to patch the same crates at the same
  revision, or the build breaks.
* **It is a young project.** It does not take pull requests; issues are the
  way in. The upside is that it is being developed alongside the work.

### Alternatives considered

* **Stay with GPUI and write the missing pieces.** That means a text editor
  with undo and IME, a Markdown renderer with math and Mermaid, a syntax
  highlighter for 80 languages and a diff view — all from scratch — and no way
  to check the UI without taking over the user's screen.
* **Iced, egui, Slint.** All three can render offscreen, but none of them
  brings the editor-specific components, and none of them is as close to the
  "native, GPU-rendered, agent-testable" shape as Rune.
* **Write the UI framework.** Not a reasonable use of the time.

## Consequences

* The editor's views are Rune elements. The crates that do not draw anything
  (`lide-text`, `lide-markdown`, `lide-documents`, `lide-platform`) stay free
  of a UI dependency and are tested without one.
* `scripts/snapshots.sh` renders the editor's views to PNGs, and
  `crates/app/tests/snapshots.rs` runs the same renders as a test.
* Bumping Rune means bumping the revision in `Cargo.toml` *and* re-checking
  Rune's own `[patch.crates-io]` list.

## Notes on the audit

The wider upstream audit — Zed's crates, PDF rendering, Markdown, PTYs, text
buffers — is in [`../audit/upstream-audit.md`](../audit/upstream-audit.md).
The findings that shaped this decision:

* Zed's `text`, `rope`, `multi_buffer`, `lsp`, `terminal` and `grammar`
  crates are **GPL-3.0-or-later** and unpublished. Vendoring them would make
  the editor GPL. The text buffer in `lide-text` is therefore our own.
* Tree-sitter is not needed for syntax highlighting: `rune-highlights` covers
  80 languages with no runtime and no grammars to ship.
* PDFium is the right PDF renderer (BSD-3, permissive bindings); see
  [0004](0004-pdf-rendering.md).
