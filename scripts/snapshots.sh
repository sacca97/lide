#!/usr/bin/env bash
#
# Render the editor headlessly and write PNG snapshots.
#
# The snapshots are the editor's own output: Rune renders them with the same
# renderer the window uses, without opening a window and without needing a
# display. That makes it possible to check the UI from a script, in CI, or
# over ssh.
#
# Usage:
#   scripts/snapshots.sh [output directory]
#
# The default output directory is target/snapshots.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-${ROOT}/target/snapshots}"
# The build directory is configurable; Cargo puts it in ~/.cargo/config.toml here.
BIN="${LIDE_BIN:-$(cargo metadata --manifest-path "${ROOT}/Cargo.toml" --format-version 1 --no-deps 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')/debug/lide}"

if [ ! -x "${BIN}" ]; then
    echo "building the editor" >&2
    cargo build --manifest-path "${ROOT}/Cargo.toml" -p lide
fi

mkdir -p "${OUT}"

# Snapshots write settings of their own rather than the user's.
export LIDE_CONFIG_DIR="${OUT}/config"
mkdir -p "${LIDE_CONFIG_DIR}"

snap() {
    local name="$1"
    shift
    echo "  ${name}"
    "${BIN}" --snap "${OUT}/${name}.png" --size 1200x800 "$@" >/dev/null
}

echo "rendering snapshots into ${OUT}"

# The workspace with no document open.
snap workspace .

# A source file, with the cursor in it.
snap editor crates/workspace/src/editor/text/buffer.rs

# A Markdown document: the source on the left, the preview on the right.
snap markdown README.md

# The PDF viewer.
snap pdf tests/fixtures/sample.pdf

# The command palette, over the workspace.
"${BIN}" --snap "${OUT}/palette.png" --scene palette --size 1200x800 >/dev/null
echo "  palette"

# The palette with a filter applied and the second command highlighted.
"${BIN}" --snap "${OUT}/palette-filtered.png" --scene palette-filtered --size 1200x800 >/dev/null
echo "  palette-filtered"

# The find bar, with a query in it.
"${BIN}" --snap "${OUT}/find.png" --scene find --size 1200x800 >/dev/null
echo "  find"

# The editor split in two.
"${BIN}" --snap "${OUT}/split.png" --scene split --size 1200x800 >/dev/null
echo "  split"

# The light theme.
"${BIN}" --snap "${OUT}/light.png" --scene light --size 1200x800 >/dev/null
echo "  light"

# Workspace search, with results.
"${BIN}" --snap "${OUT}/search.png" --scene search --size 1200x800 >/dev/null
echo "  search"

# A Jupyter notebook.
"${BIN}" --snap "${OUT}/notebook.png" --scene notebook --size 1200x800 >/dev/null
echo "  notebook"

# A CSV file as a table.
"${BIN}" --snap "${OUT}/csv.png" --scene csv --size 1200x800 >/dev/null
echo "  csv"

# The code actions a language server offers.
"${BIN}" --snap "${OUT}/code-actions.png" --scene code-actions --size 1200x800 >/dev/null
echo "  code-actions"

# A file's diff.
"${BIN}" --snap "${OUT}/diff.png" --scene diff --size 1200x800 >/dev/null
echo "  diff"

# An SVG's source beside its preview.
"${BIN}" --snap "${OUT}/svg-source.png" --scene svg-source --size 1400x800 >/dev/null
echo "  svg-source"

# An image preview.
"${BIN}" --snap "${OUT}/image.png" --scene image --size 1200x800 >/dev/null
echo "  image"

# A PDF with a bookmark outline.
"${BIN}" --snap "${OUT}/pdf-outline.png" --scene pdf-outline --size 1200x800 >/dev/null
echo "  pdf-outline"

# A PDF with text selected by the pointer's machinery.
"${BIN}" --snap "${OUT}/pdf-select.png" --scene pdf-select --size 1200x800 >/dev/null
echo "  pdf-select"

# A LaTeX build that failed, with its output.
"${BIN}" --snap "${OUT}/build.png" --scene build --size 1200x800 >/dev/null
echo "  build"

# A PDF with a search open.
"${BIN}" --snap "${OUT}/pdf-search.png" --scene pdf-search --size 1200x800 >/dev/null
echo "  pdf-search"

# The dialog that chooses where LaTeX builds write.
"${BIN}" --snap "${OUT}/latex-output.png" --scene latex-output >/dev/null
echo "  latex-output"

# A pointer selection in the terminal grid.
"${BIN}" --snap "${OUT}/terminal-select.png" --scene terminal-select >/dev/null
echo "  terminal-select"

# The dialog that chooses the terminal's shell and directory.
"${BIN}" --snap "${OUT}/terminal-settings.png" --scene terminal-settings >/dev/null
echo "  terminal-settings"

# The terminal's search bar over its scrollback.
"${BIN}" --snap "${OUT}/terminal-find.png" --scene terminal-find --size 1200x800 >/dev/null
echo "  terminal-find"

# The integrated terminal.
"${BIN}" --snap "${OUT}/terminal.png" --scene terminal --size 1200x800 >/dev/null
echo "  terminal"

# The completion list, anchored to the editor.
"${BIN}" --snap "${OUT}/completion.png" --scene completion --size 1200x800 >/dev/null
echo "  completion"

# Hover information from a language server.
"${BIN}" --snap "${OUT}/hover.png" --scene hover --size 1200x800 >/dev/null
echo "  hover"

# The places panel: where a symbol is used.
"${BIN}" --snap "${OUT}/places.png" --scene places --size 1200x800 >/dev/null
echo "  places"

# The problems panel.
"${BIN}" --snap "${OUT}/problems.png" --scene problems --size 1200x800 >/dev/null
echo "  problems"

# Diagnostics underlined in the editor and listed in the Problems panel.
"${BIN}" --snap "${OUT}/diagnostics.png" --scene diagnostics >/dev/null
echo "  diagnostics"

# The sidebar when it is shown; it is hidden on a fresh start.
"${BIN}" --snap "${OUT}/sidebar.png" --scene sidebar >/dev/null
echo "  sidebar"

# Opening one file through the SSH agent.
"${BIN}" --snap "${OUT}/remote-file.png" --scene remote-file >/dev/null
echo "  remote-file"

# The SSH host picker.
"${BIN}" --snap "${OUT}/ssh-hosts.png" --scene ssh-hosts >/dev/null
echo "  ssh-hosts"

# The review step before replacing across the workspace.
"${BIN}" --snap "${OUT}/replace-confirm.png" --scene replace-confirm >/dev/null
echo "  replace-confirm"

# A file that changed on disk while the editor has unsaved changes.
"${BIN}" --snap "${OUT}/conflict.png" --scene conflict --size 1200x800 >/dev/null
echo "  conflict"

# The element tree, for assertions that do not need pixels.
"${BIN}" --tree "${OUT}/workspace-tree.json" . >/dev/null

echo "done"
ls -la "${OUT}"
