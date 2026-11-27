//! The documents that are open.
//!
//! A document is one of a few kinds, and the kind decides the view it opens
//! in. The variants are boxed where they are large: a terminal carries a PTY
//! and a grid, and letting that size leak into every `Document` would make
//! every tab pay for it.

use std::path::Path;

use crate::documents::DocumentKind;
use crate::editor::Editor;
use crate::{csv_view, diff, images, notebook_view};
use lide_pdf::view::PdfView;

/// A document that is open in the workspace.
///
/// The variants are boxed where they are large: a terminal carries a PTY and
/// a grid, and letting that size leak into every `Document` would make every
/// tab pay for it.
pub enum Document {
    /// A text or source file.
    Text(Editor),
    /// A Markdown file: the source and its preview.
    Markdown {
        /// The source text.
        editor: Editor,
        /// The headings of the document, for the outline.
        outline: Vec<crate::markdown::OutlineEntry>,
    },
    /// A PDF file.
    Pdf(Box<PdfView>),
    /// A terminal.
    Terminal(Box<crate::terminal::TerminalView>),
    /// An image.
    Image(Box<images::ImageView>),
    /// A file's diff.
    Diff(Box<diff::DiffView>),
    /// A comma- or tab-separated table.
    Csv(Box<csv_view::CsvView>),
    /// A Jupyter notebook.
    Notebook(Box<notebook_view::NotebookView>),
}

impl Document {
    /// The kind of the document.
    pub fn kind(&self) -> DocumentKind {
        match self {
            Document::Text(_) => DocumentKind::Text,
            Document::Markdown { .. } => DocumentKind::Markdown,
            Document::Pdf(_) => DocumentKind::Pdf,
            Document::Terminal(_) => DocumentKind::Text,
            Document::Image(_) => DocumentKind::Image,
            Document::Diff(_) => DocumentKind::Text,
            Document::Csv(_) => DocumentKind::Csv,
            Document::Notebook(_) => DocumentKind::Notebook,
        }
    }

    /// The title shown on the tab.
    pub fn title(&self) -> String {
        match self {
            Document::Text(editor) => editor.title(),
            Document::Markdown { editor, .. } => editor.title(),
            Document::Pdf(pdf) => pdf.title(),
            Document::Terminal(terminal) => terminal.title(),
            Document::Image(image) => image.title(),
            Document::Diff(diff) => diff.title(),
            Document::Csv(csv) => csv.title(),
            Document::Notebook(notebook) => notebook.title(),
        }
    }

    /// Whether the document has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        match self {
            Document::Text(editor) => editor.dirty(),
            Document::Markdown { editor, .. } => editor.dirty(),
            Document::Pdf(_) => false,
            Document::Terminal(_) => false,
            Document::Image(_) => false,
            Document::Diff(_) => false,
            Document::Csv(_) => false,
            Document::Notebook(_) => false,
        }
    }

    /// Where the document lives, for local and remote text documents.
    pub fn location(&self) -> Option<&lide_resource::Location> {
        match self {
            Document::Text(editor) => editor.location.as_ref(),
            Document::Markdown { editor, .. } => editor.location.as_ref(),
            _ => None,
        }
    }

    /// The path the document was opened from.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Document::Text(editor) => editor.local_path(),
            Document::Markdown { editor, .. } => editor.local_path(),
            Document::Pdf(pdf) => Some(pdf.path()),
            Document::Terminal(_) => None,
            Document::Image(image) => Some(image.path()),
            Document::Diff(_) => None,
            Document::Csv(csv) => Some(csv.path()),
            Document::Notebook(notebook) => Some(notebook.path()),
        }
    }

    /// The text of the document, if it has any.
    pub fn text(&self) -> Option<&str> {
        match self {
            Document::Text(editor) => Some(editor.text()),
            Document::Markdown { editor, .. } => Some(editor.text()),
            Document::Pdf(_) => None,
            Document::Terminal(_) => None,
            Document::Image(_) => None,
            Document::Diff(_) => None,
            Document::Csv(_) => None,
            Document::Notebook(_) => None,
        }
    }

    /// The editor of the document, if it is editable.
    pub fn editor(&self) -> Option<&Editor> {
        match self {
            Document::Text(editor) => Some(editor),
            Document::Markdown { editor, .. } => Some(editor),
            Document::Pdf(_) => None,
            Document::Terminal(_) => None,
            Document::Image(_) => None,
            Document::Diff(_) => None,
            Document::Csv(_) => None,
            Document::Notebook(_) => None,
        }
    }

    /// The editor of the document, mutably.
    pub fn editor_mut(&mut self) -> Option<&mut Editor> {
        match self {
            Document::Text(editor) => Some(editor),
            Document::Markdown { editor, .. } => Some(editor),
            Document::Pdf(_) => None,
            Document::Terminal(_) => None,
            Document::Image(_) => None,
            Document::Diff(_) => None,
            Document::Csv(_) => None,
            Document::Notebook(_) => None,
        }
    }
}
