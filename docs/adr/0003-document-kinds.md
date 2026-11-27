# 0003 — Document kinds and the provider registry

- Status: amended (2026-11-25)
- Date: 2026-11-24

> **Amended (2026-11-27).** The crate that held the document kinds was folded into
> `lide-workspace`; see [0008](0008-crate-layout.md) for the current
> layout. The decision itself stands.

> **Amended.** Only `DocumentKind` remains. The workspace owns the document
> lifecycle as an enum, and `Capabilities`, `Document`, `Provider` and the
> registry were unused outside their tests and have been removed. The text
> below records the original design; the seam a plugin system would use is
> no longer present.

## Context

The editor opens more than text: source code, Markdown, PDFs and images. A PDF
viewer, a text editor and a Markdown preview have nothing in common in the
way they are interacted with, so forcing them into one interface produces
either a very wide interface or a lot of special cases.

## Decision

`lide-documents` defines three things and nothing else:

* **`DocumentKind`** — what a document *is*: `Text`, `Markdown`, `Pdf`,
  `Image`, `Binary`. It is decided by the file extension, and by the first
  bytes of the file when the extension says nothing.
* **`Capabilities`** — what a document *can do*: editable, savable,
  searchable, has a preview, has an outline, read-only. The UI asks the
  capabilities rather than the kind, so a new kind does not mean a new
  `match` in every command.
* **`Document`** and **`Provider`** — the lifecycle: open, save, close,
  reload, and the title and dirty state.

The registry is a list of providers; the first one that claims a path wins.
That keeps the order meaningful without a plugin system: the Markdown and PDF
providers go before the text provider.

## Consequences

* `lide-documents` has no UI dependency and is unit tested on its own.
* The workspace crate holds the concrete documents as an enum — `Text`,
  `Markdown`, `Pdf` — rather than as trait objects. With three kinds an enum
  is simpler to read and to match on; it becomes a trait object when a
  fourth kind needs different behaviour rather than different content.
* A PDF is not a text document. It does not pretend to be one, and the
  editor never asks it for text.
* The document *provider* registry is the seam a plugin system would grow
  from, but nothing about it depends on one existing.
