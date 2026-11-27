# 0008 — The crate layout

- Status: accepted
- Date: 2026-11-24
- Amended: 2026-11-27 — the single-consumer crates were folded into the
  workspace; the crate boundaries are now the shared contracts.

## Context

The plan proposes a Cargo workspace with one crate per subsystem. It also
warns against a crate for every small helper.

## Decision

Eight crates, split where the ownership, the dependencies or the testing
boundary justify it.

```
crates/
├── app/          # the binary: the command line, the window, the global keys
├── workspace/    # everything the editor owns: the state, the documents, the views
├── pdf/          # PDF structure, rendering and text extraction
├── latex/        # LaTeX builds and the SyncTeX map
├── lsp/          # the language server client
├── remote/       # the agent, its protocol and the client that talks to it
├── resource/     # where a document lives: a local path or an SSH agent
└── rpc/          # Content-Length framed JSON (the LSP client and the agent)
```

## The rules

* **A crate is a boundary someone else needs.** A helper with one consumer
  is a module, not a crate: it costs a manifest, a dependency edge and a
  place to look, and buys nothing. The workspace holds the editor, the
  document registry, the parsers behind the viewers, the Markdown model, the
  search, the git reader, the terminal and the platform specifics.
* **`app` composes; it does not implement.** It parses the command line, opens
  the window and owns the global shortcuts.
* **A crate that another crate depends on stays a crate.** `resource` is the
  contract between the editor and the remote agent: both implement the same
  `Store`, and the agent cannot depend on the workspace without a cycle.
  `rpc` is the contract between the LSP client and the agent, which speak the
  same framing.
* **A crate with a binary or a native library stays a crate.** `remote` builds
  the `lide-agent` that runs on the other machine; `pdf` loads PDFium at
  runtime.
* **The document crates never depend on each other.** `pdf` does not know
  about `markdown`.

## The dependency graph

```
        app
         │
     workspace ── pdf
         ├──────── latex ── lsp ── rpc
         ├──────── lsp
         └──────── remote ── resource
                          └──── rpc
```

There are no cycles, and nothing depends on `app`.

## Consequences

* The workspace is one crate with many modules; `cargo test -p lide-workspace`
  covers them together, and the parsers that had no UI still have none.
* The crates that draw can be tested headlessly: `cargo test -p lide` renders
  the views to PNGs.
* A framework change touches `workspace`, `pdf` and `app`. The parsers and
  the models sit beside them, but they do not import the framework.
* Moving a helper between crates is a `git mv` and an import change, not a
  new package to publish.
