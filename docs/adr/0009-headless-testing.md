# 0009 — The UI is checked headlessly

- Status: accepted
- Date: 2026-11-24

## Context

This editor is being built by an agent that has to be able to check its own
work. On a machine with a busy Wayland session, opening a window means
taking the user's focus and covering their screen — which is exactly what
happened while the GPUI prototype was being tried out.

## Decision

Every view is rendered headlessly and looked at, and the renders are run as
tests.

* **`scripts/snapshots.sh`** renders the workspace, the editor, the Markdown
  preview and the PDF viewer to PNGs and dumps the element tree to JSON.
* **`crates/app/tests/snapshots.rs`** runs the same renders and fails if a
  view draws nothing.

```sh
./scripts/snapshots.sh
cargo test --workspace
```

Rune renders headlessly with the same renderer the window uses, so a snapshot
is what the user would see, not an approximation of it.

## What is checked

* Every view draws something. A blank snapshot fails the test.
* The element tree can be asserted on, for the things a picture does not
  show: that a file is open, that the status bar says where the cursor is.
* Scenarios can be replayed from a script (`--script`), which is how the
  editor's own interactions are tested without a human at the keyboard.

## Consequences

* The tests need a GPU or a software rasteriser (Mesa's `lavapipe` works).
* A test that only checks "something was drawn" is a weak test. The element
  tree is the stronger instrument, and it is used where a picture is not
  enough.
* The windowed application is still the product. The headless renders exist
  so that it can be developed without disturbing the machine it is developed
  on.
