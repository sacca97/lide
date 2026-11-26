# 0014 — LaTeX builds through texres

- Status: accepted
- Date: 2026-11-24

## Context

The plan's reason for the editor existing is partly technical writing: a
multi-file LaTeX project should build from the editor, with errors that
navigate to the source, and the PDF previewed beside it.

## Decision

Run `texres` as a subprocess. The default command is exactly:

```text
texres main.tex
```

`texres` picks the engine from the document, writes SyncTeX, keeps the build
from stopping to ask a question nobody is there to answer, and prints a
compact error block. The editor does not add extra compiler flags: the file
name is the only argument in the default case. The editor never embeds a TeX
distribution.

## Engine selection

`texres` chooses an engine by itself, which is right for most documents. A
document that needs XeLaTeX or LuaLaTeX gets an explicit choice instead:

| Engine | Command |
| --- | --- |
| `auto` | `texres file.tex` |
| `pdflatex` | `texres -pdf file.tex` |
| `xelatex` | `texres -xelatex file.tex` |
| `lualatex` | `texres -lualatex file.tex` |

The choice lives in `settings.json` as `latex_engine`, and the palette's
"LaTeX: cycle build engine" moves to the next one and saves it. Only one
engine runs at a time, so the flags are alternatives: choosing XeLaTeX does
not leave `-pdf` in the command.

## The runner is general

`BuildOptions` is a program, its arguments and a working directory;
`lide_latex::texres` is one way to fill it in. That is deliberate: it makes
the component testable on a machine with no TeX installed. The tests run `sh`
and check that output streams, that a non-zero exit is a failed build, that
cancelling stops it, and that errors parse.

## Error parsing

`texres` prints its own block, with the message and a precise location:

```text
error: LaTeX Error: File `nope.tex' not found.
  --> /work/main.tex:3:8
```

The older forms are understood too, because a build run by hand should read
the same as one run from the editor:

* `./main.tex:12: Undefined control sequence.` — the compiler's line form.
* `! Undefined control sequence.` followed by `l.42` — plain TeX's form.

An error with no line number still counts, at line 0.

## Consequences

* A missing `texres` is a message in the build panel, not a broken editor.
* A successful build opens the PDF it produced, so the preview follows the
  source without a second command.
* Engine selection is a single remembered choice, not user-authored recipe
  lists. The build directory is a setting (`latex_output_directory`, passed
  as `-output-directory DIR`), but recipe lists and continuous builds are not
  implemented; the roadmap records them.
* SyncTeX is written and read in both directions: forward from the source
  line, inverse from a point on the page.
* Verified against `texres` on this machine: a minimal document produces
  `main.pdf` and `main.synctex.gz`, and a missing input produces a navigable
  `--> file:line:column` error.
