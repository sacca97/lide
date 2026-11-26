# 0005 — Markdown: our model, Rune's renderer

- Status: accepted
- Date: 2026-11-24

## Context

The editor renders Markdown as a live preview next to the source: GitHub
Flavored Markdown, tables, task lists, links, images, fenced code, heading
outline, mathematics and Mermaid diagrams.

## Decision

* **`rune-kit`'s `Markdown`** draws the preview. It covers headings,
  paragraphs, emphasis, inline code, code blocks, lists with task checkboxes,
  block quotes, rules, links, GFM tables, `diff` fences, and — with the
  features enabled — Mermaid diagrams and sandboxed LaTeX display math.
* **`pulldown-cmark`** 0.13 is the parser underneath, with GFM, tables,
  footnotes and math enabled.
* **`lide-markdown`** keeps the document model, the heading outline and the
  HTML export. It has no UI dependency and is unit tested on its own.

## Why not one or the other

Rune's renderer alone would do for the preview, but the editor also needs:

* a **heading outline** for the navigation pane, which is a property of the
  parsed document rather than of the drawing;
* an **HTML export** for "export this document", which the preview cannot
  provide;
* a **document model** that a future document-wide search can walk.

Writing a second parser for those would mean two Markdown implementations that
can disagree. Keeping the model means the preview and the outline come from
the same parse.

## Consequences

* Mathematics is rendered by running LaTeX in a sandboxed WebAssembly
  interpreter (`wasmi`, with fuel and memory caps) on a background worker.
  There is no browser and no JavaScript runtime.
* Mermaid diagrams are drawn by `rune-mermaid`. Both features are optional in
  Rune and are enabled here.
* The preview re-parses when the source changes. `rune-kit` caches the parse of
  unchanged text, so a frame that draws the same source parses nothing.
* The preview and the source are separate scroll viewports. Each reports its
  offset and content height through a `Live` value, and a wheel over either
  asks the other to show the same fraction of its content. The fraction is
  the common measure because the two hold different amounts of text — a
  wrapped source line is not a rendered paragraph. The pane the pointer is
  over keeps its native wheel scrolling, so momentum and the scrollbar still
  behave as usual; only the other pane is moved.
