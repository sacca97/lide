//! The Markdown document model.
//!
//! `pulldown-cmark` produces a flat event stream. This module turns that
//! stream into a tree that is convenient to render and to navigate, and that
//! does not borrow from the parser's input.

/// A block level element.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// A heading, `#` through `######`.
    Heading {
        /// The heading level, 1 through 6.
        level: u8,
        /// The explicit heading identifier, if the document sets one.
        id: Option<String>,
        /// The heading's inline content.
        content: Vec<Inline>,
    },
    /// A paragraph of text.
    Paragraph(Vec<Inline>),
    /// A fenced or indented code block.
    CodeBlock {
        /// The language of the code, from the fence's info string.
        language: Option<String>,
        /// The code itself.
        code: String,
    },
    /// A block quote.
    BlockQuote(Vec<Block>),
    /// A bullet or numbered list.
    List {
        /// Whether the list is numbered.
        ordered: bool,
        /// The number the first item starts at.
        start: Option<u64>,
        /// The items of the list.
        items: Vec<ListItem>,
    },
    /// A GFM table.
    Table {
        /// The column alignments.
        alignments: Vec<Alignment>,
        /// The header cells.
        head: Vec<Vec<Inline>>,
        /// The body rows.
        rows: Vec<Vec<Vec<Inline>>>,
    },
    /// A thematic break (`---`).
    ThematicBreak,
    /// A raw HTML block.
    Html(String),
    /// A footnote definition.
    FootnoteDefinition {
        /// The footnote label, without the leading `^`.
        label: String,
        /// The footnote's blocks.
        blocks: Vec<Block>,
    },
}

/// An item in a list.
#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    /// `Some(true)` for a checked task, `Some(false)` for an unchecked one,
    /// `None` for an ordinary item.
    pub checked: Option<bool>,
    /// The item's blocks.
    pub blocks: Vec<Block>,
}

/// The alignment of a table column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    /// Default alignment.
    None,
    /// Left aligned.
    Left,
    /// Centred.
    Center,
    /// Right aligned.
    Right,
}

/// An inline element.
#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    /// Plain text.
    Text(String),
    /// A code span.
    Code(String),
    /// A math span.
    Math {
        /// Whether the math is displayed as a block.
        display: bool,
        /// The LaTeX source.
        source: String,
    },
    /// Emphasised text (`*text*`).
    Emphasis(Vec<Inline>),
    /// Strongly emphasised text (`**text**`).
    Strong(Vec<Inline>),
    /// Struck through text (`~~text~~`).
    Strikethrough(Vec<Inline>),
    /// A link.
    Link {
        /// The link text.
        text: Vec<Inline>,
        /// The destination.
        dest: String,
        /// The optional title.
        title: Option<String>,
    },
    /// An image.
    Image {
        /// The alternative text.
        alt: Vec<Inline>,
        /// The image source.
        dest: String,
        /// The optional title.
        title: Option<String>,
    },
    /// Raw inline HTML.
    Html(String),
    /// A soft line break.
    SoftBreak,
    /// A hard line break.
    HardBreak,
    /// A reference to a footnote.
    FootnoteReference(String),
}

/// A heading in the document outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry {
    /// The heading level, 1 through 6.
    pub level: u8,
    /// The plain text of the heading.
    pub title: String,
    /// An anchor derived from the heading text, unique within the document.
    pub anchor: String,
    /// The line the heading is on, counted from one.
    pub line: u64,
}

/// The plain text of a run of inline elements.
///
/// Inline markup contributes its text but not its markers, which is what a
/// heading in the outline should show.
pub fn inline_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    push_inline_text(inlines, &mut out);
    out
}

fn push_inline_text(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => out.push_str(text),
            Inline::Code(text) => out.push_str(text),
            Inline::Math { source, .. } => out.push_str(source),
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strikethrough(children) => push_inline_text(children, out),
            Inline::Link { text, .. } => push_inline_text(text, out),
            Inline::Image { alt, .. } => push_inline_text(alt, out),
            Inline::Html(_) => {}
            Inline::SoftBreak | Inline::HardBreak => out.push(' '),
            Inline::FootnoteReference(_) => {}
        }
    }
}

/// Collect the headings of a document into an outline.
pub fn outline(blocks: &[Block]) -> Vec<OutlineEntry> {
    let mut entries = Vec::new();
    let mut used: Vec<String> = Vec::new();
    collect_outline(blocks, &mut entries, &mut used);
    entries
}

fn collect_outline(blocks: &[Block], entries: &mut Vec<OutlineEntry>, used: &mut Vec<String>) {
    for block in blocks {
        match block {
            Block::Heading { level, content, .. } => {
                let title = inline_text(content);
                let anchor = unique_anchor(&slugify(&title), used);
                entries.push(OutlineEntry {
                    level: *level,
                    title,
                    anchor,
                    line: 1,
                });
            }
            Block::BlockQuote(blocks) => collect_outline(blocks, entries, used),
            Block::List { items, .. } => {
                for item in items {
                    collect_outline(&item.blocks, entries, used);
                }
            }
            Block::FootnoteDefinition { blocks, .. } => collect_outline(blocks, entries, used),
            _ => {}
        }
    }
}

/// Turn a heading into a GitHub style anchor.
pub fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        } else {
            pending_dash = true;
        }
    }
    out
}

pub fn unique_anchor(base: &str, used: &mut Vec<String>) -> String {
    if base.is_empty() {
        return String::new();
    }
    let mut candidate = base.to_string();
    let mut counter = 1;
    while used.contains(&candidate) {
        counter += 1;
        candidate = format!("{base}-{counter}");
    }
    used.push(candidate.clone());
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_ignores_markup() {
        let inlines = vec![
            Inline::Text("Hello ".into()),
            Inline::Strong(vec![Inline::Text("world".into())]),
            Inline::Text("!".into()),
        ];
        assert_eq!(inline_text(&inlines), "Hello world!");
    }

    #[test]
    fn slugify_produces_github_style_anchors() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  spaced  out  "), "spaced-out");
        assert_eq!(slugify(""), "");
    }
}
