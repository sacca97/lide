# 0002 — The text buffer is ours, on top of ropey

- Status: superseded (2026-11-25)
- Date: 2026-11-24

> **Amended (2026-11-27).** The crate that held the text buffer was folded into
> `lide-workspace`; see [0008](0008-crate-layout.md) for the current
> layout. The decision itself stands.

> **Superseded.** Rune's `TextEdit` owns editing, undo and selections in the
> editor. The rope-backed buffer, `SelectionSet`, `History` and `Edit` were
> never used outside their own tests and have been removed. `lide-text` now
> keeps only the UTF-8/byte-order-mark and line-ending helpers. The decision
> below records why the rope was built and is kept for history.

## Context

The editor needs a document buffer: Unicode-correct text, cursors and
selections, an undo history, and edits that stay cheap in the middle of a
large file.

Zed's `text` crate does all of this well, but it is GPL-3.0-or-later and
unpublished, so it cannot be used (see the
[upstream audit](../audit/upstream-audit.md)).

## Decision

Two pieces, with different jobs:

* **`rune-edit`'s `TextEdit`** owns the *editing model*: the text of the
  field, the selection, the undo history, IME composition and the key
  handling. It is what the editor view binds to, and it is what makes undo,
  redo and the caret work without any code here.
* **`lide-text`** owns the *document*: a rope-backed buffer with byte offsets,
  character and UTF-16 conversions, line and column arithmetic, and the line
  ending and encoding of the file on disk.

Today `lide-text` provides the line ending and byte order mark handling that
`lide-editor` needs to write a file back byte for byte. Its buffer, selection
and history machinery is tested and ready for the large-file work.

## The rope

`ropey` 2.0.0-beta.1 rather than 1.6.1:

* 1.6.1 indexes by **character**; every conversion to a byte offset (which is
  what the operating system, the clipboard and the input method use) costs a
  lookup. 2.0 indexes by **byte** and offers UTF-16 as a metric behind a
  feature.
* 1.6.1 was published in October 2023 and has had no release since. 2.0 is
  the line the author works on.
* 2.0 is a pre-release. The risk is contained: the buffer sits behind
  `lide-text`'s own API, so replacing it later touches one crate.

`crop` is the alternative to keep an eye on. Its benchmark advantage over
`ropey` is the author's own, measured on 2018 hardware in 2023.

## Consequences

* The editing model and the document model are separate. A document can be
  edited through a field (what the editor does now) or directly (what a
  large-file mode will do), without either knowing about the other.
* `TextEdit` holds the text in a `String`. Opening a very large file costs
  memory in proportion to its size; the rope-backed path is what a large-file
  mode will use.
* Line endings are normalised to LF while editing and reapplied when the file
  is written, so a CRLF file is written back byte for byte.
