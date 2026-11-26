//! PDF document support.
//!
//! The crate is split so that the parts that always work do not depend on
//! the parts that need a native library:
//!
//! * [`document`] reads the structure of a PDF — page count, page sizes —
//!   with the pure Rust [`pdf`] crate. It always works.
//! * [`render`] (behind the `pdfium` feature) rasterises pages and extracts
//!   positioned text with PDFium. It needs a `libpdfium.so` at runtime; see
//!   `scripts/fetch-pdfium.sh` and `docs/adr/0004-pdf-rendering.md`.
//!
//! Keeping the two apart means the editor can still show a PDF's page count
//! and report a clear diagnostic when PDFium is missing, instead of failing
//! to open the file at all.

pub mod document;

#[cfg(feature = "pdfium")]
pub mod render;

pub mod view;

pub use document::{PageInfo, PdfDocument, PdfError};
