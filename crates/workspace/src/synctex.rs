//! Moving between a LaTeX source and its PDF.
//!
//! SyncTeX writes, beside the PDF, where each line of the source landed on
//! which page. The editor reads that map to jump both ways: from the caret
//! to the matching place in the PDF, and from a double click on the page back
//! to the line that produced it.
//!
//! The map is approximate — its boxes are narrower than the glyphs, and a
//! line of TeX can become several lines of type — so each jump is corrected
//! with the text itself: the words of the source line find the place on the
//! page, and the clicked word picks the line.

use std::path::Path;

use super::{Document, Workspace};
use crate::latex_discovery::fs_key;

/// Pick the source line a clicked word is on, among those near the line SyncTeX
/// named, and the column of the word in it.
///
/// `words` are the folded words before, at and after the click. The longest
/// run of them that one line holds wins, then the nearest line.
pub(crate) fn refine_line(text: &str, line: u64, column: usize, words: &(String, String, String)) -> (u64, usize) {
    let (before, word, after) = words;
    if word.is_empty() {
        return (line, column);
    }
    let needles = [
        format!("{before}{word}{after}"),
        format!("{before}{word}"),
        format!("{word}{after}"),
        word.clone(),
    ];
    let first = line.saturating_sub(8).max(1);
    let folded: Vec<(u64, String, &str)> = text
        .lines()
        .enumerate()
        .map(|(i, l)| (i as u64 + 1, lide_pdf::view::fold_text(l), l))
        .filter(|(n, _, _)| *n >= first && *n <= line + 8)
        .collect();
    for needle in needles.iter().filter(|n| n.chars().count() >= 3) {
        let best = folded
            .iter()
            .filter(|(_, f, _)| f.contains(needle.as_str()))
            .min_by_key(|(n, _, _)| n.abs_diff(line));
        if let Some((n, _, raw)) = best {
            let column = raw.to_lowercase().find(word.as_str()).unwrap_or(0);
            return (*n, column);
        }
    }
    (line, column)
}

impl Workspace {
    /// Jump between the LaTeX source and its PDF.
    ///
    /// A `.tex` source goes to the line in the PDF; a PDF goes back to the
    /// source at the point last under the pointer.
    pub fn sync_source_and_pdf(&mut self) {
        match self.documents.get(self.focused_index()) {
            Some(Document::Pdf(_)) => self.sync_inverse(),
            Some(document)
                if document
                    .path()
                    .and_then(|path| path.extension())
                    .is_some_and(|extension| extension == "tex") =>
            {
                self.sync_forward();
            }
            _ => {
                self.notice = Some("open a .tex source or its PDF to sync".to_string());
            }
        }
    }

    /// Show the current source line where it lands on the PDF.
    pub fn sync_forward(&mut self) {
        let Some(tex) = self.latex_document() else {
            self.notice = Some("no .tex file to sync".to_string());
            return;
        };
        let line = self
            .documents
            .get(self.focused_index())
            .and_then(|document| document.editor())
            .map(|editor| editor.cursor_line() as u64)
            .unwrap_or(1);
        let output = self.latex_output_directory(&tex);
        let pdf = lide_latex::texres_in(&tex, self.latex_engine(), output.as_deref()).pdf_path();
        if !pdf.exists() {
            self.notice = Some("build the document first".to_string());
            return;
        }

        // The cursor may be in an included chapter, not in the main file.
        let source = self
            .documents
            .get(self.focused_index())
            .and_then(|document| document.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "tex"))
            .map(Path::to_path_buf)
            .unwrap_or_else(|| tex.clone());

        // The build directory is where the source lives: the sidecar of a
        // PDF that was moved stays there.
        let build_directory = tex
            .parent()
            .or_else(|| pdf.parent())
            .unwrap_or_else(|| std::path::Path::new("."));
        let phrases = self.source_phrases(&source, line);
        let synctex = lide_latex::SyncTex::new(build_directory);
        match synctex.forward(&source, line, 0, &pdf) {
            Ok(Some(point)) => {
                self.show_pdf_beside(&source, &pdf);
                let pdf_index = self.split_document.unwrap_or(self.active);
                if let Some(Document::Pdf(view)) = self.documents.get_mut(pdf_index) {
                    let page = point.page.saturating_sub(1);
                    let height = view.page_height(page).unwrap_or(792.0);
                    view.go_to_page(page);                    // SyncTeX's y is measured from the top; the viewer's
                    // pointer and text layer use the PDF's bottom-left origin.
                    view.pointer = Some((point.x, (height - point.y).max(0.0)));
                    // SyncTeX is exact to the line of the page; the words of the
                    // source line find the place within it, once the page's
                    // text is there.
                    view.set_sync_goal(phrases.clone(), point.y);
                    let offset = view.scroll_to_height(point.y);
                    let pane = if self.split_document == Some(pdf_index) { 1 } else { 0 };
                    self.request_scroll(format!("pdf-scroll-{pane}"), offset);
                }
            }
            Ok(None) => self.notice = Some(format!("SyncTeX knows nothing about line {line}")),
            Err(error) => self.notice = Some(error),
        }
    }

    /// The words to look for on the PDF page for a source line, longest
    /// phrase first, starting at the caret when it is on that line.
    pub(crate) fn source_phrases(&self, path: &Path, line: u64) -> Vec<String> {
        let Some(text) = self.source_text(path) else {
            return Vec::new();
        };
        let Some(line_text) = text.lines().nth(line.saturating_sub(1) as usize) else {
            return Vec::new();
        };
        let mut start = 0usize;
        if let Some(editor) = self.documents.get(self.focused_index()).and_then(|d| d.editor()) {
            if editor.cursor_line() as u64 == line {
                let column = editor.cursor_column().saturating_sub(1);
                start = line_text
                    .char_indices()
                    .nth(column)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
            }
        }
        let mut words = lide_latex::plain_words(&line_text[start..]);
        if words.is_empty() && start > 0 {
            words = lide_latex::plain_words(line_text);
        }
        let mut phrases = Vec::new();
        for count in [5usize, 3, 2] {
            if words.len() >= count {
                phrases.push(words[..count].join(" "));
            }
        }
        if let Some(longest) = words.iter().filter(|w| w.chars().count() >= 4).max_by_key(|w| w.len()) {
            phrases.push(longest.clone());
        }
        phrases
    }

    /// The text of a source file: the open buffer, or the file on disk.
    pub(crate) fn source_text(&self, path: &Path) -> Option<String> {
        self.documents
            .iter()
            .find(|document| document.path() == Some(path))
            .and_then(|document| document.text().map(str::to_string))
            .or_else(|| std::fs::read_to_string(path).ok())
    }

    /// Show the source of the place the pointer was last at on the PDF.
    pub fn sync_inverse(&mut self) {
        let words;
        let (pdf, page, x, y) = match self.documents.get(self.focused_index()) {
            Some(Document::Pdf(view)) => {
                let page = view.page();
                let height = view.page_height(page).unwrap_or(792.0);
                let (x, bottom) = view.pointer.unwrap_or((306.0, height / 2.0));
                words = view.words_around(x, bottom);
                // The pointer is in bottom-left PDF points; SyncTeX's y is
                // measured from the top of the page.
                (
                    view.path().to_path_buf(),
                    page,
                    x,
                    (height - bottom).max(0.0),
                )
            }
            _ => {
                self.notice = Some("open a PDF to sync from".to_string());
                return;
            }
        };

        let synctex = lide_latex::SyncTex::new(pdf.parent().unwrap_or(std::path::Path::new(".")));
        match synctex.inverse(&pdf, page + 1, x, y) {
            Ok(Some(source)) => {
                // The sidecar may spell the source through a symlink (macOS's
                // /var for /private/var); if the file is already open under the
                // workspace's spelling, use that, so the open tab is focused
                // instead of a second copy opening.
                let source = lide_latex::SyncSource {
                    path: self
                        .documents
                        .iter()
                        .find_map(|document| {
                            document
                                .path()
                                .filter(|path| fs_key(path) == fs_key(&source.path))
                                .map(Path::to_path_buf)
                        })
                        .unwrap_or(source.path),
                    ..source
                };
                self.show_pdf_beside(&source.path, &pdf);
                // SyncTeX names one line of the page's text; the clicked word
                // and the clicked row say which of the nearby lines it was.
                let (line, column) =
                    self.pick_source_line(&synctex, &pdf, page, y, &source, words.as_ref());
                if let Some(editor) = self.active_editor_mut() {
                    editor.go_to(line, column);
                }
                self.reveal_source_line(line);
            }
            Ok(None) => self.notice = Some("SyncTeX knows nothing about that place".to_string()),
            Err(error) => self.notice = Some(error),
        }
    }

    /// Build the LaTeX document the editor is looking at.
    ///
    /// The document is the active `.tex` file if there is one, else the only
    /// `.tex` file in the project. When the build succeeds the PDF opens
    /// beside the source, so the preview follows the source without losing it.
    pub fn build_latex(&mut self) {
        if let Some(build) = self.build.as_mut() {
            if !build.finished {
                self.notice = Some("a build is already running".to_string());
                return;
            }
        }
        let Some(tex) = self.latex_document() else {
            self.notice = Some("no .tex file to build".to_string());
            return;
        };

        let output = self.latex_output_directory(&tex);
        let options = lide_latex::texres_in(&tex, self.latex_engine(), output.as_deref());
        match lide_latex::Build::start(options) {
            Ok(build) => {
                self.build = Some(build);
                self.notice = None;
            }
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    /// Open the built PDF beside the `.tex` source.
    ///
    /// The source stays in the left pane and the PDF opens in the right, so a
    /// build or save keeps both in view. Build the document first if the PDF
    /// does not exist yet.
    pub fn open_pdf_beside_source(&mut self) {
        let Some(tex) = self.latex_document() else {
            self.notice = Some("no .tex file to preview".to_string());
            return;
        };
        let output = self.latex_output_directory(&tex);
        let pdf = lide_latex::texres_in(&tex, self.latex_engine(), output.as_deref()).pdf_path();
        if !pdf.exists() {
            self.notice = Some("build the document first".to_string());
            return;
        }
        let source = self
            .documents
            .get(self.focused_index())
            .and_then(|document| document.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "tex"))
            .map(Path::to_path_buf)
            .unwrap_or_else(|| tex.clone());
        self.show_pdf_beside(&source, &pdf);
    }

    /// Put `tex` in the left pane and `pdf` in the right.
    pub(crate) fn show_pdf_beside(&mut self, tex: &Path, pdf: &Path) {
        if let Err(error) = self.open_path(tex) {
            self.set_notice(error);
            return;
        }
        let tex_index = self.active;
        if let Err(error) = self.open_path(pdf) {
            self.set_notice(error);
            return;
        }
        let pdf_index = self.active;
        if pdf_index == tex_index || pdf_index >= self.documents.len() {
            return;
        }
        self.active = tex_index;
        self.split_document = Some(pdf_index);
        self.focused_pane = 0;
        if !self.settings.split {
            self.settings.split = true;
            self.settings.save();
        }
    }

    /// Stop the running build.
    pub fn cancel_build(&mut self) {
        if let Some(build) = self.build.as_mut() {
            build.cancel();
        }
    }

    /// The LaTeX engine builds use, from the settings.
    pub fn latex_engine(&self) -> lide_latex::Engine {
        lide_latex::Engine::from_name(&self.settings.latex_engine).unwrap_or_default()
    }

    /// Move to the next LaTeX engine and remember it.
    pub fn cycle_latex_engine(&mut self) {
        let next = self.latex_engine().next();
        self.settings.latex_engine = next.name().to_string();
        self.settings.save();
        self.refresh_tasks();
        self.notice = Some(format!("LaTeX engine: {}", next.name()));
    }

    /// Pick the source line a click on the PDF came from, among the lines near
    /// the one SyncTeX named.
    ///
    /// SyncTeX's edit query answers with a single node, and its boxes are
    /// narrower than the glyphs they cover, so a click on the last words of a
    /// line can come back as the next line down. The click itself says better:
    /// among the candidate lines, the one whose text holds the clicked word
    /// and whose own SyncTeX position on the clicked page is nearest the
    /// click's height wins. `y_top` is the click's height in points from the
    /// top of the page, `page` the zero-based page.
    pub(crate) fn pick_source_line(
        &self,
        synctex: &lide_latex::SyncTex,
        pdf: &Path,
        page: u32,
        y_top: f32,
        source: &lide_latex::SyncSource,
        words: Option<&(String, String, String)>,
    ) -> (u64, usize) {
        let fallback = (source.line, source.column as usize);
        let Some(text) = self.source_text(&source.path) else {
            return fallback;
        };
        let needles: Vec<String> = match words {
            Some((before, word, after)) => [
                format!("{before}{word}{after}"),
                format!("{before}{word}"),
                format!("{word}{after}"),
                word.clone(),
            ]
            .into_iter()
            .filter(|needle| needle.chars().count() >= 3)
            .collect(),
            None => Vec::new(),
        };

        // Candidate lines: every line holding the clicked phrase, wherever it
        // is in the file — SyncTeX's single answer can be a section away when
        // its boxes do not cover the glyphs that were clicked — capped to the
        // ones nearest its answer to bound the query. Without a clicked word,
        // just the lines near its answer.
        let folded: Vec<(u64, String)> = text
            .lines()
            .enumerate()
            .map(|(index, line)| (index as u64 + 1, lide_pdf::view::fold_text(line)))
            .collect();
        let near = |n: u64| {
            n >= source.line.saturating_sub(8).max(1) && n <= source.line + 8
        };
        let mut candidates: Vec<u64> = if needles.is_empty() {
            folded.iter().map(|(n, _)| *n).filter(|n| near(*n)).collect()
        } else {
            folded
                .iter()
                .filter(|(_, f)| needles.iter().any(|needle| f.contains(needle.as_str())))
                .map(|(n, _)| *n)
                .collect()
        };
        if candidates.is_empty() {
            candidates = folded.iter().map(|(n, _)| *n).filter(|n| near(*n)).collect();
        }
        candidates.sort_by_key(|n| n.abs_diff(source.line));
        candidates.truncate(64);
        if candidates.is_empty() {
            return fallback;
        }

        // Score each candidate by the height of its own SyncTeX position on
        // the clicked page; the nearest line to SyncTeX's answer breaks ties.
        let positions = synctex.forward_many(&source.path, &candidates, pdf).unwrap_or_default();
        let mut best: Option<(u64, f32, u64)> = None;
        for (n, position) in &positions {
            let distance = position
                .as_ref()
                .filter(|point| point.page == page + 1)
                .map(|point| (point.y - y_top).abs())
                .unwrap_or(f32::MAX);
            let line_distance = n.abs_diff(source.line);
            let better = match &best {
                None => true,
                Some((_, best_distance, best_line_distance)) => {
                    distance < *best_distance
                        || (distance == *best_distance && line_distance < *best_line_distance)
                }
            };
            if better {
                best = Some((*n, distance, line_distance));
            }
        }
        let Some((line, distance, _)) = best else {
            return fallback;
        };
        if distance == f32::MAX {
            // No candidate has a position on this page: the text alone decides.
            return match words {
                Some(words) => refine_line(&text, source.line, source.column as usize, words),
                None => fallback,
            };
        }
        let column = words
            .filter(|(_, word, _)| !word.is_empty())
            .and_then(|(_, word, _)| {
                text.lines()
                    .nth(line.saturating_sub(1) as usize)
                    .and_then(|raw| raw.to_lowercase().find(word.as_str()))
            })
            .unwrap_or(source.column as usize);
        (line, column)
    }
}
