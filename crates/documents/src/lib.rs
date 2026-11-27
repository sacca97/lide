//! Document kinds and the parsers behind the viewers.
//!
//! A document is anything the editor can open: source code, Markdown, a PDF,
//! an image. This crate decides what a path *is* and owns the parsers that
//! can be tested without a window: CSV tables, Jupyter notebooks and
//! structured-data validation. The workspace owns the document lifecycle
//! itself.

pub mod csv;
pub mod notebook;
pub mod structured;

use std::path::Path;

/// The kind of a document.
///
/// The kind decides which view opens a file, so it names the views the editor
/// has, not just the broad shapes: a `.csv` file and a `.ipynb` file are
/// both text on disk, but they open as a table and as a column of cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocumentKind {
    /// Plain text or source code.
    Text,
    /// A Markdown document, which can be rendered as a preview.
    Markdown,
    /// A PDF document.
    Pdf,
    /// A raster or vector image.
    Image,
    /// A comma- or tab-separated table.
    Csv,
    /// A Jupyter notebook.
    Notebook,
    /// A file the editor can show but not meaningfully edit.
    Binary,
}

/// Decide what kind of document a path holds.
///
/// The decision is made from the file extension first, because that is cheap
/// and works for files that are not on disk. When the extension is unknown,
/// [`sniff_kind`] falls back to looking at the first bytes of the file.
pub fn detect_kind(path: &Path) -> DocumentKind {
    if let Some(kind) = kind_from_extension(path) {
        return kind;
    }
    match sniff_kind(path) {
        Some(kind) => kind,
        None => DocumentKind::Binary,
    }
}

/// Decide the kind of a document from its file extension alone.
pub fn kind_from_extension(path: &Path) -> Option<DocumentKind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let kind = match extension.as_str() {
        "md" | "markdown" | "mdown" | "mkd" => DocumentKind::Markdown,
        "pdf" => DocumentKind::Pdf,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "svg" | "avif" | "tiff" => {
            DocumentKind::Image
        }
        "csv" | "tsv" => DocumentKind::Csv,
        "ipynb" => DocumentKind::Notebook,
        "txt" | "log" | "text" | "rst" | "adoc" | "org" | "toml" | "yaml" | "yml" | "json" | "jsonc"
        | "xml" | "ini" | "cfg" | "conf" | "env" | "gitignore" | "gitattributes"
        | "bash" | "sh" | "zsh" | "fish" | "ps1" | "bat" | "cmd" | "lua" | "rb" | "py" | "pyi"
        | "rs" | "go" | "js" | "jsx" | "ts" | "tsx" | "c" | "h" | "cc" | "cpp" | "cxx" | "hpp"
        | "hh" | "java" | "kt" | "kts" | "scala" | "swift" | "m" | "mm" | "cs" | "fs" | "fsx"
        | "clj" | "cljs" | "ex" | "exs" | "erl" | "hrl" | "hs" | "ml" | "mli" | "nim" | "zig"
        | "dart" | "r" | "jl" | "jl2" | "tex" | "sty" | "cls" | "bib" | "sql" | "vue" | "svelte"
        | "asm" | "s" | "make" | "mk" | "cmake" | "gradle" | "proto" | "graphql" | "gql" | "diff"
        | "patch" | "desktop" | "service" | "timer" | "socket" => DocumentKind::Text,
        _ => return None,
    };
    Some(kind)
}

/// Decide the kind of a document by looking at its first bytes.
///
/// Returns `None` when the file does not exist or cannot be read, in which
/// case the caller should fall back to the extension.
pub fn sniff_kind(path: &Path) -> Option<DocumentKind> {
    use std::io::Read;

    let mut file = std::fs::File::open(path).ok()?;
    let mut header = [0u8; 12];
    let read = file.read(&mut header).ok()?;
    let header = &header[..read];

    if header.starts_with(b"%PDF-") {
        return Some(DocumentKind::Pdf);
    }
    if header.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some(DocumentKind::Image);
    }
    if header.starts_with(b"GIF8") {
        return Some(DocumentKind::Image);
    }
    if header.starts_with(b"\xFF\xD8\xFF") {
        return Some(DocumentKind::Image);
    }
    if header.starts_with(b"RIFF") && header.len() >= 12 {
        return Some(DocumentKind::Image);
    }
    if std::str::from_utf8(header).is_ok() {
        return Some(DocumentKind::Text);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_extensions_are_recognised() {
        assert_eq!(
            kind_from_extension(Path::new("README.md")),
            Some(DocumentKind::Markdown)
        );
        assert_eq!(
            kind_from_extension(Path::new("notes.MARKDOWN")),
            Some(DocumentKind::Markdown)
        );
    }

    #[test]
    fn pdf_extension_is_recognised() {
        assert_eq!(
            kind_from_extension(Path::new("paper.pdf")),
            Some(DocumentKind::Pdf)
        );
    }

    #[test]
    fn source_files_are_text() {
        assert_eq!(
            kind_from_extension(Path::new("src/main.rs")),
            Some(DocumentKind::Text)
        );
        assert_eq!(
            kind_from_extension(Path::new("Makefile")),
            None,
            "files without an extension need content sniffing"
        );
    }

    #[test]
    fn unknown_extensions_are_not_guessed() {
        assert_eq!(kind_from_extension(Path::new("a.qqq")), None);
    }

    #[test]
    fn tables_and_notebooks_have_their_own_kinds() {
        // The kind decides the viewer, so a table must not open as text.
        assert_eq!(
            kind_from_extension(Path::new("data.csv")),
            Some(DocumentKind::Csv)
        );
        assert_eq!(
            kind_from_extension(Path::new("data.tsv")),
            Some(DocumentKind::Csv)
        );
        assert_eq!(
            kind_from_extension(Path::new("book.ipynb")),
            Some(DocumentKind::Notebook)
        );
    }

    #[test]
    fn a_table_is_still_a_table_when_the_extension_is_unknown() {
        // Without an extension the bytes decide; a CSV file is text, and the
        // extension is what says it is a table.
        let directory = std::env::temp_dir().join(format!("lide-kinds-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("data");
        std::fs::write(&path, "a,b\n1,2\n").unwrap();
        assert_eq!(detect_kind(&path), DocumentKind::Text);
        let path = directory.join("data.csv");
        std::fs::write(&path, "a,b\n1,2\n").unwrap();
        assert_eq!(detect_kind(&path), DocumentKind::Csv);
        let _ = std::fs::remove_dir_all(&directory);
    }

}
