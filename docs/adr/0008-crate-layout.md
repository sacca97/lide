# 0008 — The crate layout

- Status: accepted
- Date: 2026-11-24

## Context

The plan proposes a Cargo workspace with one crate per subsystem. It also
warns against a crate for every small helper.

## Decision

Nine crates, split where the ownership, the dependencies or the testing
boundary justify it.

```
crates/
├── app/          # the binary: the command line, the window, the global keys
├── workspace/    # the project, its files, the tabs, and the window's layout
├── editor/       # the source editor: a document backed by a text field
├── text/         # UTF-8 and line-ending helpers (no UI)
├── documents/    # document kinds and the parsers behind the viewers
├── markdown/     # the Markdown model, the outline and the HTML export
├── pdf/          # PDF structure, rendering and text extraction
└── platform/     # Linux specifics: XDG paths, the session, the tools
```

## The rules

* **`app` composes; it does not implement.** It parses the command line, opens
  the window and owns the global shortcuts. Everything else lives in a crate.
* **`workspace` coordinates.** It knows which documents are open and which one
  is active, and it draws the window. It depends on the document crates and
  on nothing else.
* **`ui` holds no domain logic.** If a component needs to know what a
  document is, that knowledge belongs to the crate that owns the document.
* **`text`, `documents`, `markdown` and `platform` have no UI dependency.**
  They are unit tested without a display, and they would survive a change of
  framework.
* **`editor` depends on `text` and on the editing model**; it does not depend
  on `workspace`.
* **The document crates never depend on each other.** `pdf` does not know
  about `markdown`.

## The dependency graph

```
        app
         │
     workspace ── editor ── text
         │          │
         │          └──── documents
         ├──────── markdown
         ├──────── pdf
         └──────── ui
```

There are no cycles, and nothing depends on `app`.

## Consequences

* The crates that matter can be tested in isolation: `cargo test -p lide-text`
  needs nothing but a terminal.
* The crates that draw can be tested headlessly: `cargo test -p lide` renders
  the views to PNGs.
* A framework change touches `ui`, `editor`, `workspace`, `pdf` and `app`. It
  does not touch `text`, `documents`, `markdown` or `platform`.
