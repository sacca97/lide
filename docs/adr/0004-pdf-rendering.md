# 0004 — PDF rendering with PDFium

- Status: accepted
- Date: 2026-11-24

## Context

The editor opens PDFs as first-class documents: page navigation, zoom, text
selection, text search, and rendering large documents without loading every
page at once.

## Decision

Two libraries, with different jobs:

* **`pdf` (pdf-rs) 0.10, MIT, pure Rust** reads the *structure* of a document:
  the page count and the page sizes. It always works, with no native library
  and nothing to ship.
* **PDFium** rasterises pages and extracts positioned text. It is reached
  through `pdfium-render` (MIT or Apache-2.0), which binds the shared library
  at runtime with `libloading`.

The library is fetched by `scripts/fetch-pdfium.sh`, which checks it against a
pinned SHA-256, and written to `third_party/pdfium/lib/`. A native build embeds
those bytes in the executable and extracts them to the user's cache on first
use, so a distributed binary does not need a loose library. The runtime search
still accepts an external copy at `$LIDE_PDFIUM_PATH`, at
`$PDFIUM_LIBRARY_PATH`, or beside the executable.

## Why PDFium

| Option | Licence | Verdict |
| --- | --- | --- |
| **PDFium** via `pdfium-render` + the prebuilt library | BSD-3 + MIT/Apache-2.0 | **chosen** |
| `mupdf` | AGPL-3.0 | rejected: copyleft |
| `poppler-rs` | MIT bindings over a GPL-2.0 library, plus a system dependency | rejected |
| `pdf-extract` | MIT | text only, no raster |
| `pdfluent`, `pdf_render` | AGPL, or ambiguous between the metadata and the repository | rejected |

PDFium is what Chromium uses, so its output matches what people expect from a
PDF. It gives both rasterisation and a text layer with per-character boxes,
which is what text selection and search need.

## Consequences

* PDFium is **BSD-3 licensed** and ships with third-party notices (FreeType,
  libpng, zlib, ICU, HarfBuzz and others). Those notices have to travel with
  any distribution that includes the library.
* The library is about 8 MiB and is not vendored into the repository. The
  fetch script and the checksum are the reproducible way to get it. A native
  build embeds the fetched library, which grows the executable by about the
  same amount; `LIDE_EMBED_PDFIUM=0` keeps the library external.
* `pdfium-render` 0.9 does not expose its dynamic loader publicly, so the
  binding goes through the public `Pdfium::bind_to_library`.
* PDF support is a default feature. If no library was embedded and none is
  found at runtime, the editor opens PDFs, reports the page count and says
  that PDFium is missing, rather than refusing the file.
* Pages are rendered on a background thread; the first page is rendered when
  the document is opened so that something appears immediately. The rendered
  bitmap is handed to the renderer as pixels — there is no PNG round trip.

## What is not done yet

* Text selection and search need the text layer, which is extracted with
  positions but not yet wired to the pointer.
* The page cache is bounded by the number of pages visited, not by a byte
  budget.
* Annotations are out of scope, as the plan says.
