//! Rasterising PDF pages and extracting positioned text with PDFium.
//!
//! PDFium is the renderer that Chrome uses. The `pdfium-render` crate loads
//! it at runtime with `libloading`, so there is no build-time dependency and
//! no vendored C++ to compile — but a `libpdfium.so` has to be present.
//!
//! The library is looked for in this order:
//!
//! 1. `$LIDE_PDFIUM_PATH` — an explicit path to the shared library.
//! 2. `$PDFIUM_LIBRARY_PATH` — the conventional environment variable.
//! 3. `third_party/pdfium/lib/libpdfium.so` next to the executable or in the
//!    working directory, which is where `scripts/fetch-pdfium.sh` puts it.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use pdfium_render::prelude::*;

use crate::document::PdfError;

/// A rasterised page.
#[derive(Debug, Clone)]
pub struct RenderedPage {
    /// The width of the bitmap in pixels.
    pub width: u32,
    /// The height of the bitmap in pixels.
    pub height: u32,
    /// The RGBA pixels, four bytes per pixel, row major.
    pub rgba: Vec<u8>,
}

/// One character of a page's text layer, with its position in PDF points.
///
/// The origin is the bottom left corner of the page and the y axis points
/// up, which is how PDF works. The editor converts to screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextChar {
    /// The character.
    pub ch: char,
    /// The left edge of the character's bounding box.
    pub left: f32,
    /// The bottom edge of the character's bounding box.
    pub bottom: f32,
    /// The right edge of the character's bounding box.
    pub right: f32,
    /// The top edge of the character's bounding box.
    pub top: f32,
}

impl TextChar {
    /// The width of the character's bounding box.
    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    /// The height of the character's bounding box.
    pub fn height(&self) -> f32 {
        self.top - self.bottom
    }
}

/// The text of a page, with the position of every character.
#[derive(Debug, Clone, Default)]
pub struct PageText {
    /// The characters, in reading order.
    pub chars: Vec<TextChar>,
}

impl PageText {
    /// The plain text of the page.
    pub fn text(&self) -> String {
        self.chars.iter().map(|c| c.ch).collect()
    }

    /// The characters that fall inside a rectangle, in reading order.
    ///
    /// The rectangle is in the same coordinate system as the characters:
    /// points, origin at the bottom left.
    pub fn chars_in_rect(&self, rect: (f32, f32, f32, f32)) -> Vec<TextChar> {
        let (left, bottom, right, top) = rect;
        self.chars
            .iter()
            .copied()
            .filter(|c| {
                let cx = (c.left + c.right) / 2.0;
                let cy = (c.bottom + c.top) / 2.0;
                cx >= left && cx <= right && cy >= bottom && cy <= top
            })
            .collect()
    }
}

/// The PDFium engine: a loaded library and the documents opened with it.
pub struct PdfiumEngine {
    pdfium: &'static Pdfium,
}

impl std::fmt::Debug for PdfiumEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfiumEngine").finish_non_exhaustive()
    }
}

/// The process's one loaded PDFium.
///
/// `pdfium-render` keeps the bindings in a global that can only be
/// initialised once; a second `bind_to_library` fails. The library also has
/// process-wide state, so it is loaded once and kept for the life of the
/// editor rather than once per document.
fn pdfium(path: Option<&Path>) -> Result<&'static Pdfium, PdfError> {
    static ENGINE: OnceLock<Result<Pdfium, String>> = OnceLock::new();
    let loaded = ENGINE.get_or_init(|| {
        let path = match path {
            Some(path) => path.to_path_buf(),
            None => locate_pdfium().ok_or_else(|| {
                "PDFium was not found. Run scripts/fetch-pdfium.sh or set LIDE_PDFIUM_PATH."
                    .to_string()
            })?,
        };
        // Binding to a shared library runs its initialisers. The library has
        // to be a PDFium build; the caller is responsible for providing it.
        let bindings = Pdfium::bind_to_library(&path).map_err(|error| {
            format!("{} is not a usable PDFium library: {error}", path.display())
        })?;
        Ok(Pdfium::new(bindings))
    });
    match loaded {
        Ok(pdfium) => Ok(pdfium),
        Err(message) => Err(PdfError::RendererUnavailable(message.clone())),
    }
}

impl PdfiumEngine {
    /// Load the PDFium library, looking for it in the usual places.
    pub fn load() -> Result<Self, PdfError> {
        Ok(Self {
            pdfium: pdfium(None)?,
        })
    }

    /// Load a specific PDFium library.
    ///
    /// The first library loaded in the process is the one every engine uses;
    /// a later path is ignored once PDFium is running.
    pub fn load_from(path: impl AsRef<Path>) -> Result<Self, PdfError> {
        Ok(Self {
            pdfium: pdfium(Some(path.as_ref()))?,
        })
    }

    /// Open a PDF document.
    pub fn open(&self, path: impl AsRef<Path>) -> Result<PdfiumDocument<'static>, PdfError> {
        let document = self
            .pdfium
            .load_pdf_from_file(path.as_ref(), None)
            .map_err(|error| PdfError::Parse {
                path: path.as_ref().to_path_buf(),
                message: error.to_string(),
            })?;
        Ok(PdfiumDocument { document })
    }
}

/// A PDF document opened with PDFium.
pub struct PdfiumDocument<'a> {
    document: PdfDocument<'a>,
}

impl<'a> PdfiumDocument<'a> {
    /// The number of pages.
    pub fn page_count(&self) -> u32 {
        self.document.pages().len().max(0) as u32
    }

    /// The size of a page in points.
    pub fn page_size(&self, index: u32) -> Result<(f32, f32), PdfError> {
        let rect = self
            .document
            .pages()
            .page_size(index as PdfPageIndex)
            .map_err(|_error| PdfError::PageOutOfRange {
                index,
                page_count: self.page_count(),
            })?;
        Ok((rect.width().value.abs(), rect.height().value.abs()))
    }

    /// The document's bookmarks, flattened to `(title, page, depth)`.
    ///
    /// The tree is walked depth-first so that the order matches what a reader
    /// sees. A bookmark with no destination keeps page 0: it is a heading, not
    /// a link.
    pub fn outline(&self) -> Vec<(String, u32, usize)> {
        let mut out = Vec::new();
        let bookmarks = self.document.bookmarks();
        // `root` is the first top-level bookmark, not a synthetic node: it is
        // walked itself, and the rest of that level are its siblings.
        if let Some(first) = bookmarks.root() {
            walk_bookmarks(&first, 0, &mut out);
            for bookmark in first.iter_siblings() {
                walk_bookmarks(&bookmark, 0, &mut out);
            }
        }
        out
    }

    /// Rasterise a page to an RGBA bitmap.
    ///
    /// `scale` is the number of pixels per point; 2.0 renders at roughly
    /// 144 dpi.
    pub fn render_page(&self, index: u32, scale: f32) -> Result<RenderedPage, PdfError> {
        let page = self
            .document
            .pages()
            .get(index as PdfPageIndex)
            .map_err(|_| PdfError::PageOutOfRange {
                index,
                page_count: self.page_count(),
            })?;

        let config = PdfRenderConfig::new().scale_page_by_factor(scale);
        let bitmap = page
            .render_with_config(&config)
            .map_err(|error| PdfError::RendererUnavailable(error.to_string()))?;

        Ok(RenderedPage {
            width: bitmap.width().max(0) as u32,
            height: bitmap.height().max(0) as u32,
            rgba: bitmap.as_rgba_bytes(),
        })
    }

    /// Extract the text of a page, with the position of every character.
    pub fn page_text(&self, index: u32) -> Result<PageText, PdfError> {
        let page = self
            .document
            .pages()
            .get(index as PdfPageIndex)
            .map_err(|_| PdfError::PageOutOfRange {
                index,
                page_count: self.page_count(),
            })?;

        let text_page = page
            .text()
            .map_err(|error| PdfError::RendererUnavailable(error.to_string()))?;

        let mut chars = Vec::new();
        for character in text_page.chars().iter() {
            let Some(ch) = character.unicode_char() else {
                continue;
            };
            let Ok(bounds) = character.tight_bounds() else {
                continue;
            };
            chars.push(TextChar {
                ch,
                left: bounds.left().value,
                bottom: bounds.bottom().value,
                right: bounds.right().value,
                top: bounds.top().value,
            });
        }

        Ok(PageText { chars })
    }
}

/// Walk a bookmark tree depth-first.
fn walk_bookmarks(bookmark: &PdfBookmark<'_>, depth: usize, out: &mut Vec<(String, u32, usize)>) {
    if depth > 8 || out.len() > 500 {
        return;
    }
    let title = bookmark.title().unwrap_or_default();
    let page = bookmark
        .destination()
        .and_then(|destination| destination.page_index().ok())
        .unwrap_or(0) as u32;
    if !title.is_empty() {
        out.push((title, page, depth));
    }
    for child in bookmark.iter_direct_children() {
        walk_bookmarks(&child, depth + 1, out);
    }
}

/// Find the PDFium shared library.
pub fn locate_pdfium() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("LIDE_PDFIUM_PATH") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    if let Some(path) = std::env::var_os("PDFIUM_LIBRARY_PATH") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    let name = Pdfium::pdfium_platform_library_name();
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.to_path_buf());
            // `<exe>/../third_party/pdfium/lib` for a build in `target/debug`.
            if let Some(parent) = dir.parent() {
                roots.push(parent.join("third_party/pdfium/lib"));
                roots.push(parent.join("../third_party/pdfium/lib"));
                // `make install` puts it beside the binary's prefix:
                // /usr/local/bin/lide -> /usr/local/lib/lide/libpdfium.so.
                roots.push(parent.join("lib/lide"));
            }
        }
    }
    roots.push(PathBuf::from("third_party/pdfium/lib"));
    roots.push(PathBuf::from("../third_party/pdfium/lib"));

    for root in roots {
        let candidate = root.join(&name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

impl std::fmt::Display for PageText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for character in &self.chars {
            write!(f, "{}", character.ch)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_selection_inside_a_rectangle() {
        let page = PageText {
            chars: vec![
                TextChar {
                    ch: 'a',
                    left: 0.0,
                    bottom: 0.0,
                    right: 5.0,
                    top: 10.0,
                },
                TextChar {
                    ch: 'b',
                    left: 10.0,
                    bottom: 0.0,
                    right: 15.0,
                    top: 10.0,
                },
            ],
        };
        assert_eq!(page.text(), "ab");
        let inside = page.chars_in_rect((0.0, 0.0, 6.0, 10.0));
        assert_eq!(inside.len(), 1);
        assert_eq!(inside[0].ch, 'a');
    }
}
