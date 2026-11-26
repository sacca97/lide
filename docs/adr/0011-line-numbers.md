# 0011 — Line numbers without a wrapped field

- Status: accepted
- Date: 2026-11-24

## Context

A code editor needs a gutter. Rune's text field always wraps
(`text_input` sets `wrap = true` and `El.text` is `pub(crate)`), so a gutter
with one row per *logical* line drifts out of step the moment a line wraps.

## Decision

Make the field wide enough that nothing wraps, and let the row scroll
horizontally.

* The field's width is `longest_line_chars × char_advance + padding`.
* The gutter is **one** text element containing every line number, right
  aligned by padding with spaces — which works because the font is
  monospace.
* Both use the same `TextStyle`, so their line heights match by construction.

`char_advance` is measured, not assumed: the view renders an invisible run of
64 characters and reads its width back from the previous layout
(`Cx::layout_with`). Until that lands, an estimate of `0.62 × font size` is
used, deliberately erring high — underestimating would reintroduce wrapping.

## Consequences

* A 2,000-line file adds one text element, not 2,000.
* Long lines scroll sideways. Soft wrap becomes a separate feature with its
  own gutter logic (count visual lines) rather than being the default.
* The measurement is one extra invisible element per window.
* Line numbers can be turned off in the settings.
