# Upstream audit

This is the Phase 0 audit of the components the editor was going to be built
from. It was carried out on 2026-11-24 against the live registries and
repositories.

Every version below was read from crates.io or from the upstream repository on
that date. Where a fact could not be verified it is marked as such.

## Summary

| Area | Chosen | Why |
| --- | --- | --- |
| UI framework | [Rune](https://github.com/anomalyco/rune) at `ac452a9c` | headless rendering, element dumps, scripted input, and the editor components the plan needs |
| Text editing | [`rune-edit`](https://github.com/anomalyco/rune) + our own buffer | selections, undo, IME and key handling already exist |
| Text buffer (documents) | [`ropey`](https://github.com/cessen/ropey) 2.0.0-beta.1 | byte-indexed, the active development line |
| Syntax highlighting | [`rune-highlights`](https://github.com/anomalyco/rune) | 80 languages, no runtime, no grammars to ship |
| Markdown | [`rune-kit`](https://github.com/anomalyco/rune) + [`pulldown-cmark`](https://github.com/raphlinus/pulldown-cmark) | GFM, tables, task lists, math and Mermaid |
| PDF | [PDFium](https://github.com/bblanchon/pdfium-binaries) via `pdfium-render` | permissive, rasterises and extracts positioned text |
| Terminal | `alacritty_terminal` + `pty-process` | the maintained emulator, an actively maintained PTY layer |
| LSP | a client of our own over `lsp-types` | Zed's client is GPL and unpublished |
| Search | `ripgrep` as a subprocess | the fastest thing available, and already installed |
| Git | the `git` CLI | predictable, always present |

## The UI framework

### GPUI

The plan named GPUI, the UI framework from the Zed repository.

| Fact | Finding |
| --- | --- |
| Latest release | `gpui` 0.2.2, published 2025-10-22, not yanked |
| Repository state | Zed's tree has moved on: 684 commits touched `crates/gpui` between 2025-10-20 and 2026-11-23, and the crate has been split into `gpui_platform`, `gpui_linux`, `gpui_wgpu`, `gpui_macos` and more |
| Published split crates | `gpui_platform`, `gpui_linux`, `gpui_wgpu` are **not published**. The published crate's README tells users to depend on `gpui_platform` |
| Licence | Apache-2.0 (the framework), GPL-3.0 (the Zed repository) |
| Linux | Wayland (since 2024-02) and X11, Vulkan only via `blade` |
| IME | `zwp_text_input_v3` on Wayland, XIM on X11 |
| Wayland activation | `xdg_activation_v1` is implemented, but `WindowOptions::app_id` is dropped in `Window::new`, so the token is built without an application id and the window never receives focus |
| Headless rendering | none |

A prototype was built against GPUI 0.2.2: it compiles, runs on Wayland and
draws. Two problems showed up. The first was the activation token: without an
application id the compositor never focuses the window, so the editor opens
behind whatever was already on screen. The second was that there is no way to
render or test the UI without a display server — every check meant taking over
the user's screen.

### Rune

[Rune](https://github.com/anomalyco/rune) is a native, GPU-rendered Rust UI
framework with its own renderer, text engine, layout and animation, built on
`winit`.

| Fact | Finding |
| --- | --- |
| Revision | `ac452a9c2b75c462c5eb1659024178f1fbbd416a`, 2026-11-24 |
| Licence | MIT |
| Activity | 3,022 commits, 7.9k stars, commits on the day of the audit |
| Release state | pre-release; the `dev` branch is never rewritten, so a revision can be pinned |
| Linux | Wayland and X11 through `winit` 0.30; Vulkan or GLES through `wgpu` |
| Headless | `--snap out.png`, `--tree tree.json`, `--script scene.json`, `--trace run.jsonl`, and a `Runtime` API for unit tests |
| Editor components | `rune-edit` (text fields, undo, IME), `rune-highlights` (80 languages), `rune-kit` (`markdown`, `math`, `mermaid`, `diff`, `line_selection`, `search`) |

A spike confirmed that a Rune application builds and renders headlessly on
this machine, with the same renderer the window uses.

Rune is now the framework. See [ADR 0001](../adr/0001-ui-framework-rune.md).

## The Zed crates

None of Zed's editor crates are usable without taking on GPL-3.0.

| Crate | Published | Licence | Verdict |
| --- | --- | --- | --- |
| `text` | no | GPL-3.0-or-later | **avoid** — it is a collaborative buffer with undo, but GPL |
| `rope` | no | GPL-3.0-or-later | **avoid** |
| `multi_buffer` | no | GPL-3.0-or-later | **avoid** |
| `lsp` | no | GPL-3.0-or-later | **avoid** |
| `terminal`, `terminal_view` | no | GPL-3.0-or-later | **avoid** |
| `grammars` | no | GPL-3.0-or-later | **avoid** |
| `sum_tree`, `collections` | no | Apache-2.0 | usable, but only through a mirror |
| `markdown` | no | GPL-3.0-or-later | **avoid** |

Zed's own `Cargo.toml` sets `publish = false` for the workspace, so none of
these can be taken as a dependency. Vendoring them would make the editor GPL.

Note the name collisions: `text`, `rope`, `terminal` and `collections` on
crates.io are unrelated crates by other authors.

**Consequence.** The document buffer, the undo history and the selections in
`lide-text` are our own. They are covered by unit tests and have no UI
dependency.

## Text buffers

| Crate | Version | Indexing | Notes |
| --- | --- | --- | --- |
| `ropey` 1.6.1 | 2023-10-18 | character | the stable line; dormant since 2023 |
| `ropey` 2.0.0-beta.1 | 2025-08-02 | **byte** | the active line; byte indexing, UTF-16 metrics behind a feature |
| `crop` 0.4.3 | 2025-04-25 | byte | fast; byte offsets with an opt-in UTF-16 metric |
| Zed `text` | — | byte + UTF-16 | best-in-class, but GPL |
| `xi-rope` 0.3.0 | 2019 | byte | an order of magnitude slower in `crop`'s benchmark |

`ropey` 2.0.0-beta.1 is used: byte offsets are what an editor wants (the
operating system, the clipboard and the input method all address text in
bytes or UTF-16 code units), and it is the line the author is still working on.
The risk is that it is a pre-release; the mitigation is that the buffer sits
behind `lide-text`'s own API, so a future swap is contained.

`crop` is the alternative to keep an eye on. Its benchmark advantage is
self-reported, on 2018 hardware, from 2023 — directional, not authoritative.

## Syntax highlighting

The plan assumed Tree-sitter.

`rune-highlights` (in the Rune repository) translates Pierre's Highlights
WebAssembly highlighter to Rust at build time. It covers 80 languages with no
runtime and no grammars to ship, and it is already wired into `rune-kit`'s
Markdown renderer. Tree-sitter would mean vendoring or fetching grammars per
language, which is a supply-chain and packaging problem the plan flagged.

`tree-sitter` on crates.io is 0.27.2 (MIT) and remains the right answer if the
editor ever needs an incremental parse tree for something other than
highlighting (folding, semantic tokens, structural search).

## PDF

| Option | Licence | Verdict |
| --- | --- | --- |
| **PDFium** via `pdfium-render` (MIT/Apache-2.0) + the prebuilt library (BSD-3) | permissive | **use** |
| `mupdf` | AGPL-3.0 | avoid |
| `poppler-rs` | MIT bindings, GPL-2.0 library, and a system dependency | avoid |
| `pdf-extract` | MIT | text only, no raster |
| `pdf` (pdf-rs) | MIT | **use** for the document structure |
| `pdfluent`, `pdf_render` | AGPL or ambiguous | avoid |

The editor reads a PDF's structure (page count, page sizes) with `pdf-rs`,
which is pure Rust and always available, and rasterises and extracts positioned
text with PDFium. The library is fetched by `scripts/fetch-pdfium.sh` and
checked against a pinned SHA-256.

See [ADR 0004](../adr/0004-pdf-rendering.md).

## Markdown

`pulldown-cmark` 0.13.6 (MIT) is the parser. It supports GFM, tables, task
lists, footnotes and math (`Options::ENABLE_MATH`).

The rendering is `rune-kit`'s `Markdown`, which draws tables, task lists,
fenced code with syntax colouring, block quotes, links, `diff` fences and —
with the `mermaid` feature — diagrams. With the `math` feature it runs LaTeX
display math in a sandboxed WebAssembly interpreter, which avoids embedding a
browser or a JavaScript runtime.

The editor's own `lide-markdown` crate keeps the document model, the heading
outline and the HTML export; the live preview is drawn by `rune-kit`.

## Terminal and PTY

| Option | Version | Licence | Verdict |
| --- | --- | --- | --- |
| `alacritty_terminal` | 0.26.0 (2026-04-06) | Apache-2.0 | **use** — the maintained emulator |
| `pty-process` | 0.5.4 (2026-10-19) | MIT | **use** — active, `rustix`-based, no `nix` boilerplate |
| `portable-pty` | 0.9.0 (2025-02-11) | MIT | use with caution — no release in 21 months |
| `nix` (`openpty`) | 0.32.0 | MIT | the building block `pty-process` uses |

Zed uses a fork of `alacritty_terminal` and its own PTY layer. The editor
will use the upstream `alacritty_terminal` for the emulation and `pty-process`
for the process side, so the two layers stay separate.

## LSP

Zed's client is GPL and unpublished, and it depends on a fork of `lsp-types`
that is also unpublished. The crates.io `lsp-types` (0.97.0, last published
2024-06-04) is stale.

There is no maintained, permissively licensed Rust LSP *client*. The editor
will need its own, over `lsp-types` or over a fork of it. That is real work,
and it is the largest single piece left in Phase 2.

## Search

`ripgrep` as a subprocess. It is the fastest thing available, it is already
installed on most Linux systems, and it handles binary files, ignore files and
encodings better than anything written from scratch. The editor reports it as
missing when it is not installed.

## Git

The `git` CLI. It is predictable, it is always present, and every behaviour the
editor needs — status, diff, stage, commit — is one command away. `libgit2`
would be a large dependency for no gain at this stage.

## File watching

`notify` 9.0.0. The events have to be coalesced and de-duplicated, because
editors write files in several steps and a naive watcher sees three events for
one save.

## Risks and unknowns

* **Rune is pre-release.** Its API moves. The revision is pinned and the
  patches are mirrored in the workspace root, but a bump is a migration.
* **`pdfium-render` 0.9** does not expose its dynamic loader publicly
  (`dynamic_bindings` is `pub(crate)`), so the library is bound through the
  public `Pdfium::bind_to_library`.
* **The `crop` benchmark** is the crate author's own, on 2018 hardware.
* **Rune's `mermaid` and `math` features** bring their own dependencies
  (`rune-mermaid`, `wasmi`, `tiny-skia`); they are enabled, and the build cost
  is paid.
* **No permissively licensed LSP client exists.** Writing one is on the
  critical path for Phase 2.
