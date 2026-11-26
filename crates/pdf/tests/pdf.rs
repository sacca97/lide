//! Integration tests for the PDF document support.
//!
//! The tests that need PDFium are only compiled when the `pdfium` feature is
//! enabled, because they need a `libpdfium.so` at runtime. Run them with:
//!
//! ```text
//! cargo test -p lide-pdf --features pdfium
//! ```

use std::path::PathBuf;

use lide_pdf::PdfDocument;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

#[test]
fn reads_the_page_count_and_size() {
    let document = PdfDocument::open(fixture("sample.pdf")).expect("the fixture should parse");
    assert_eq!(document.page_count(), 1);

    let page = document.page(0).expect("page 0 exists");
    assert_eq!(page.width, 612.0);
    assert_eq!(page.height, 792.0);
    assert!((page.aspect_ratio() - 612.0 / 792.0).abs() < 0.001);
}

#[test]
fn reports_pages_out_of_range() {
    let document = PdfDocument::open(fixture("sample.pdf")).expect("the fixture should parse");
    assert!(document.page(7).is_err());
}

/// The vendored PDFium, so that the tests run instead of skipping.
///
/// `locate_pdfium` looks next to the executable and in the working directory;
/// a test binary lives in `target/debug/deps` and runs in the crate's
/// directory, so neither finds `third_party/pdfium/lib` without help.
#[cfg(feature = "pdfium")]
fn vendored_pdfium() -> Option<PathBuf> {
    let name = format!(
        "{}pdfium{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/pdfium/lib")
        .join(name);
    if !path.exists() {
        return None;
    }
    // The tests are one process, so setting this once is enough.
    unsafe { std::env::set_var("LIDE_PDFIUM_PATH", &path) };
    Some(path)
}

/// A loaded PDFium, or a printed skip when the library is not on this
/// machine.
#[cfg(feature = "pdfium")]
fn engine() -> Option<lide_pdf::render::PdfiumEngine> {
    let _ = vendored_pdfium();
    if lide_pdf::render::locate_pdfium().is_none() {
        eprintln!("skipping: no PDFium library found");
        return None;
    }
    Some(lide_pdf::render::PdfiumEngine::load().expect("PDFium should load"))
}

#[cfg(feature = "pdfium")]
mod pdfium {
    use super::*;

    #[test]
    fn renders_a_page_to_a_bitmap() {
        let Some(engine) = engine() else {
            return;
        };
        let document = engine.open(fixture("sample.pdf")).expect("the fixture should open");
        assert_eq!(document.page_count(), 1);

        let page = document.render_page(0, 1.0).expect("the page should render");
        assert_eq!(page.width, 612);
        assert_eq!(page.height, 792);
        assert_eq!(page.rgba.len(), (612 * 792 * 4) as usize);

        // The page is not blank: at least one pixel is not white.
        assert!(
            page.rgba.chunks(4).any(|pixel| pixel[0] < 240),
            "the rendered page should contain text"
        );
    }

    #[test]
    fn extracts_positioned_text() {
        let Some(engine) = engine() else {
            return;
        };
        let document = engine.open(fixture("sample.pdf")).expect("the fixture should open");
        let text = document.page_text(0).expect("the text layer should load");

        let plain = text.to_string();
        assert!(
            plain.contains("Hello lide"),
            "expected the greeting, got {plain:?}"
        );
        assert!(plain.contains("Second line of text"));

        // Every character has a position, and the characters are ordered from
        // the top of the page downwards.
        let first = text.chars[0];
        assert!(first.top > first.bottom);
        assert!(first.right > first.left);
    }

    #[test]
    fn selects_characters_inside_a_rectangle() {
        let Some(engine) = engine() else {
            return;
        };
        let document = engine.open(fixture("sample.pdf")).expect("the fixture should open");
        let text = document.page_text(0).expect("the text layer should load");

        // A rectangle that spans the first line vertically, from the lowest
        // descender to the highest ascender.
        let bottom = text.chars[0].bottom - 2.0;
        let top = text.chars[0].top + 2.0;
        let selected = text.chars_in_rect((0.0, bottom, 612.0, top));
        assert!(
            selected.iter().any(|c| c.ch == 'H'),
            "the first line should be inside the rectangle"
        );
    }
}

#[cfg(feature = "pdfium")]
mod search {
    use super::*;
    use lide_pdf::view::find_matches;

    #[test]
    fn a_word_on_the_page_is_found_at_its_glyphs() {
        let Some(engine) = engine() else { return };
        let document = engine.open(fixture("sample.pdf")).expect("the fixture should open");
        let text = document.page_text(0).expect("the text layer should load");

        let matches = find_matches(&text.chars, "line");
        assert_eq!(matches.len(), 1, "the fixture has one 'line'");

        let (left, bottom, width, height) = matches[0];
        assert!(width > 0.0 && height > 0.0, "the box has a size");
        assert!(left > 0.0 && bottom > 0.0, "the box is on the page");

        // The match's box should cover the characters it matched.
        let first = text
            .chars
            .windows(4)
            .find(|window| window.iter().map(|c| c.ch).collect::<String>() == "line")
            .expect("the characters are on the page");
        let expected_left = first.iter().map(|c| c.left).fold(f32::MAX, f32::min);
        assert!((left - expected_left).abs() < 0.01);
    }

    #[test]
    fn searching_is_case_insensitive() {
        let Some(engine) = engine() else { return };
        let document = engine.open(fixture("sample.pdf")).expect("the fixture should open");
        let text = document.page_text(0).expect("the text layer should load");

        assert_eq!(find_matches(&text.chars, "HELLO").len(), 1);
        assert_eq!(find_matches(&text.chars, "hello").len(), 1);
    }

    #[test]
    fn a_query_that_is_not_there_finds_nothing() {
        let Some(engine) = engine() else { return };
        let document = engine.open(fixture("sample.pdf")).expect("the fixture should open");
        let text = document.page_text(0).expect("the text layer should load");

        assert!(find_matches(&text.chars, "zebra").is_empty());
        assert!(find_matches(&text.chars, "").is_empty());
    }
}

#[cfg(feature = "pdfium")]
mod selection {
    use super::*;
    use lide_pdf::view::{char_at, selection_boxes};

    fn page_text() -> Option<lide_pdf::render::PageText> {
        let engine = engine()?;
        let document = engine.open(fixture("sample.pdf")).expect("the fixture should open");
        Some(document.page_text(0).expect("the text layer should load"))
    }

    /// The point at the centre of the character that starts `word`.
    fn centre_of(text: &lide_pdf::render::PageText, word: &str) -> (f32, f32, usize) {
        let chars = &text.chars;
        let found = chars
            .windows(word.chars().count())
            .position(|window| window.iter().map(|c| c.ch).collect::<String>() == word)
            .expect("the word is on the page");
        let character = &chars[found];
        (
            (character.left + character.right) / 2.0,
            (character.bottom + character.top) / 2.0,
            found,
        )
    }

    #[test]
    fn a_point_inside_a_glyph_picks_that_glyph() {
        let Some(text) = page_text() else { return };
        let (x, y, index) = centre_of(&text, "Second");
        assert_eq!(char_at(&text.chars, x, y), Some(index));
    }

    #[test]
    fn a_point_in_the_margin_picks_the_nearest_line() {
        let Some(text) = page_text() else { return };
        let (_, y, first_of_line) = centre_of(&text, "Second");
        // Far to the right of the line, on the same baseline.
        let picked = char_at(&text.chars, 500.0, y).expect("a character");
        let picked_line = text.chars[picked].bottom;
        let wanted_line = text.chars[first_of_line].bottom;
        assert!(
            (picked_line - wanted_line).abs() < 1.0,
            "the nearest character should be on the same line"
        );
    }

    #[test]
    fn selecting_two_words_covers_both() {
        let Some(text) = page_text() else { return };
        let (start_x, start_y, start) = centre_of(&text, "Second");
        let (end_x, end_y, end) = centre_of(&text, "text");
        let first = char_at(&text.chars, start_x, start_y).unwrap();
        let last = char_at(&text.chars, end_x, end_y).unwrap();
        let boxes = selection_boxes(&text.chars, first.min(last), first.max(last) + 1);
        assert_eq!(boxes.len(), 1, "one line, one box");

        // The box starts where the first selected glyph starts and ends where
        // the last one ends.
        let (left, bottom, width, height) = boxes[0];
        assert!(width > 0.0 && height > 0.0);
        assert!((left - text.chars[start].left).abs() < 0.01);
        assert!((left + width - text.chars[end].right).abs() < 0.01);
        assert!(bottom > 0.0);
    }

    #[test]
    fn a_selection_box_spans_the_characters_it_covers() {
        let Some(text) = page_text() else { return };
        let boxes = selection_boxes(&text.chars, 0, 4);
        let expected = selection_boxes(&text.chars, 0, 4);
        assert_eq!(boxes, expected);

        let left = text.chars[0].left;
        let right = text.chars[3].right;
        assert!((boxes[0].0 - left).abs() < 0.01);
        assert!((boxes[0].0 + boxes[0].2 - right).abs() < 0.01);
    }

    #[test]
    fn an_empty_or_reversed_selection_has_no_boxes() {
        let Some(text) = page_text() else { return };
        assert!(selection_boxes(&text.chars, 3, 3).is_empty());
        assert!(selection_boxes(&text.chars, 5, 2).is_empty());
    }
}

#[cfg(feature = "pdfium")]
mod outline {
    use super::*;

    #[test]
    fn the_bookmarks_are_extracted_with_their_pages() {
        let Some(engine) = engine() else { return };
        let document = engine.open(fixture("outline.pdf")).expect("the fixture should open");
        assert_eq!(document.page_count(), 2, "the fixture has two pages");

        let outline = document.outline();
        let titles: Vec<&str> = outline.iter().map(|(title, _, _)| title.as_str()).collect();
        assert!(
            titles.iter().any(|title| title.contains("Chapter One")),
            "got {titles:?}"
        );
        assert!(titles.iter().any(|title| title.contains("Chapter Two")));
        assert!(
            titles.iter().any(|title| title.contains("A section")),
            "the nested entry is flattened in"
        );

        // The second chapter points at the second page.
        let chapter_two = outline
            .iter()
            .find(|(title, _, _)| title.contains("Chapter Two"))
            .expect("Chapter Two");
        assert_eq!(chapter_two.1, 1, "page index 1 is the second page");
    }

    #[test]
    fn a_document_without_bookmarks_has_no_outline() {
        let Some(engine) = engine() else { return };
        let document = engine.open(fixture("sample.pdf")).expect("the fixture should open");
        assert!(document.outline().is_empty());
    }
}

#[test]
fn a_phrase_is_found_nearest_to_the_wanted_height() {
    use lide_pdf::render::TextChar;
    use lide_pdf::view::find_phrase;
    let mut chars = Vec::new();
    for bottom in [700.0f32, 300.0] {
        for (i, ch) in "the \u{FB01}nal word".chars().enumerate() {
            let left = 50.0 + i as f32 * 6.0;
            chars.push(TextChar { ch, left, right: left + 5.0, bottom, top: bottom + 10.0 });
        }
    }
    let (low, _) = find_phrase(&chars, "the final", 305.0).expect("found");
    assert!(chars[low].bottom < 400.0, "the lower copy is the near one");
    let (high, _) = find_phrase(&chars, "the final", 695.0).expect("found");
    assert!(chars[high].bottom > 400.0);
}
