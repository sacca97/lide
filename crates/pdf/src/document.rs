//! Reading the structure of a PDF.

use std::path::{Path, PathBuf};

use pdf::file::FileOptions;

/// Errors raised while working with a PDF.
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    /// The file could not be read.
    #[error("could not read {path}: {source}")]
    Io {
        /// The offending path.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The file is not a PDF, or is damaged beyond what the parser can
    /// recover from.
    #[error("{path} could not be parsed as a PDF: {message}")]
    Parse {
        /// The offending path.
        path: PathBuf,
        /// A description of the problem.
        message: String,
    },
    /// The requested page does not exist.
    #[error("page {index} is out of range (the document has {page_count} pages)")]
    PageOutOfRange {
        /// The requested page index.
        index: u32,
        /// The number of pages in the document.
        page_count: u32,
    },
    /// A native library needed for rendering is not available.
    #[error("{0}")]
    RendererUnavailable(String),
}

/// The size of a page, in PostScript points (1/72 inch).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageInfo {
    /// The zero based index of the page.
    pub index: u32,
    /// The width of the page in points.
    pub width: f32,
    /// The height of the page in points.
    pub height: f32,
}

impl PageInfo {
    /// The aspect ratio (width divided by height).
    pub fn aspect_ratio(&self) -> f32 {
        if self.height <= 0.0 {
            1.0
        } else {
            self.width / self.height
        }
    }
}

/// The structure of a PDF document.
#[derive(Debug)]
pub struct PdfDocument {
    path: PathBuf,
    pages: Vec<PageInfo>,
}

impl PdfDocument {
    /// Read the structure of the PDF at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PdfError> {
        let path = path.as_ref().to_path_buf();
        let file = FileOptions::uncached()
            .open(&path)
            .map_err(|source| PdfError::Parse {
                path: path.clone(),
                message: source.to_string(),
            })?;

        let mut pages = Vec::new();
        for (index, page) in file.pages().enumerate() {
            let page = page.map_err(|source| PdfError::Parse {
                path: path.clone(),
                message: source.to_string(),
            })?;
            let box_ = page.media_box().map_err(|source| PdfError::Parse {
                path: path.clone(),
                message: source.to_string(),
            })?;
            let width = (box_.right - box_.left).abs();
            let height = (box_.top - box_.bottom).abs();
            pages.push(PageInfo {
                index: index as u32,
                width,
                height,
            });
        }

        Ok(Self { path, pages })
    }

    /// An empty document, used when a file could not be opened.
    pub fn empty(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            pages: Vec::new(),
        }
    }

    /// The path the document was opened from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The number of pages.
    pub fn page_count(&self) -> u32 {
        self.pages.len() as u32
    }

    /// The information about every page.
    pub fn pages(&self) -> &[PageInfo] {
        &self.pages
    }

    /// The information about a single page.
    pub fn page(&self, index: u32) -> Result<&PageInfo, PdfError> {
        self.pages.get(index as usize).ok_or(PdfError::PageOutOfRange {
            index,
            page_count: self.page_count(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_a_missing_file_reports_a_parse_error() {
        let error = PdfDocument::open("/nonexistent/definitely-not-here.pdf").unwrap_err();
        assert!(matches!(error, PdfError::Parse { .. }));
    }

    #[test]
    fn page_aspect_ratio_is_never_zero() {
        let info = PageInfo {
            index: 0,
            width: 0.0,
            height: 0.0,
        };
        assert_eq!(info.aspect_ratio(), 1.0);
    }
}
