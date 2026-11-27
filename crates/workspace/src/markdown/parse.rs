//! Turning Markdown source into the document model.
//!
//! `pulldown-cmark` emits a flat stream of events. The parser below folds
//! that stream into the tree defined in [`crate::ast`].

use pulldown_cmark::{
    Alignment as CmarkAlignment, CodeBlockKind, Event, HeadingLevel, LinkType, Options, Parser,
    Tag, TagEnd,
};

use crate::markdown::ast::{Alignment, Block, Inline, ListItem};

/// The extensions the editor enables.
///
/// This is GitHub Flavored Markdown plus tables, task lists, footnotes and
/// math. Footnotes and math are not part of GFM proper, but they are common
/// enough in technical writing to be worth supporting.
pub fn default_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM
}

/// Parse Markdown source into blocks.
pub fn parse(text: &str) -> Vec<Block> {
    parse_with(text, default_options())
}

/// Parse Markdown source into blocks with explicit options.
pub fn parse_with(text: &str, options: Options) -> Vec<Block> {
    let parser = Parser::new_ext(text, options);
    Builder::default().run(parser)
}

/// The inline context that is currently being collected.
#[derive(Debug)]
enum InlineFrame {
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Strikethrough(Vec<Inline>),
    Link {
        dest: String,
        title: Option<String>,
        text: Vec<Inline>,
    },
    Image {
        dest: String,
        title: Option<String>,
        alt: Vec<Inline>,
    },
}

/// The block container that is currently being filled.
///
/// The container stack is parallel to the frame stack: `containers[i]` holds
/// the blocks that belong to `frames[i]`.
#[derive(Debug, Clone)]
enum BlockFrame {
    /// The top level of the document.
    Root,
    /// A block quote.
    BlockQuote,
    /// A bullet or numbered list.
    List {
        ordered: bool,
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    /// An item of a list.
    Item { checked: Option<bool> },
    /// A footnote definition.
    Footnote { label: String },
}

/// The state of the table that is currently being collected.
#[derive(Debug)]
struct TableState {
    alignments: Vec<Alignment>,
    head: Vec<Vec<Inline>>,
    rows: Vec<Vec<Vec<Inline>>>,
    in_head: bool,
}

/// Folds a `pulldown-cmark` event stream into a block tree.
#[derive(Debug)]
struct Builder {
    /// The blocks of each open container.
    containers: Vec<Vec<Block>>,
    /// The metadata of each open container.
    frames: Vec<BlockFrame>,
    /// The tables that are currently open.
    tables: Vec<TableState>,
    inline_stack: Vec<InlineFrame>,
    /// The inline content of the paragraph or heading being collected.
    inlines: Vec<Inline>,
    /// The heading level of the heading being collected.
    heading_level: Option<u8>,
    /// The identifier of the heading being collected.
    heading_id: Option<String>,
    /// The code of the code block being collected.
    code: String,
    /// The language of the code block being collected.
    code_language: Option<String>,
    /// Whether the parser is currently inside a code block.
    in_code_block: bool,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            containers: vec![Vec::new()],
            frames: vec![BlockFrame::Root],
            tables: Vec::new(),
            inline_stack: Vec::new(),
            inlines: Vec::new(),
            heading_level: None,
            heading_id: None,
            code: String::new(),
            code_language: None,
            in_code_block: false,
        }
    }
}

impl Builder {
    fn run<'a>(mut self, events: impl Iterator<Item = Event<'a>>) -> Vec<Block> {
        for event in events {
            self.handle(event);
        }
        self.finish();
        self.containers.pop().unwrap_or_default()
    }

    fn handle(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => {
                if self.in_code_block {
                    self.code.push_str(&text);
                } else {
                    self.push_inline(Inline::Text(text.to_string()));
                }
            }
            Event::Code(code) => self.push_inline(Inline::Code(code.to_string())),
            Event::InlineMath(source) => self.push_inline(Inline::Math {
                display: false,
                source: source.to_string(),
            }),
            Event::DisplayMath(source) => self.push_inline(Inline::Math {
                display: true,
                source: source.to_string(),
            }),
            Event::Html(html) => self.push_inline(Inline::Html(html.to_string())),
            Event::InlineHtml(html) => self.push_inline(Inline::Html(html.to_string())),
            Event::FootnoteReference(label) => {
                self.push_inline(Inline::FootnoteReference(label.to_string()))
            }
            Event::SoftBreak => self.push_inline(Inline::SoftBreak),
            Event::HardBreak => self.push_inline(Inline::HardBreak),
            Event::Rule => self.push_block(Block::ThematicBreak),
            Event::TaskListMarker(checked) => {
                if let Some(BlockFrame::Item { checked: slot }) = self.frames.last_mut() {
                    *slot = Some(checked);
                }
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.inlines.clear(),
            Tag::Heading { level, id, .. } => {
                self.heading_level = Some(heading_level(level));
                self.heading_id = id.map(|id| id.to_string()).filter(|id| !id.is_empty());
                self.inlines.clear();
            }
            Tag::CodeBlock(kind) => {
                self.code.clear();
                self.in_code_block = true;
                self.code_language = match kind {
                    CodeBlockKind::Indented => None,
                    CodeBlockKind::Fenced(language) => {
                        let language = language.trim().to_string();
                        if language.is_empty() {
                            None
                        } else {
                            Some(language)
                        }
                    }
                };
            }
            Tag::BlockQuote(_) => self.push_container(BlockFrame::BlockQuote),
            Tag::List(start) => self.push_container(BlockFrame::List {
                ordered: start.is_some(),
                start,
                items: Vec::new(),
            }),
            Tag::Item => self.push_container(BlockFrame::Item { checked: None }),
            Tag::FootnoteDefinition(label) => self.push_container(BlockFrame::Footnote {
                label: label.to_string(),
            }),
            Tag::Table(alignments) => {
                self.tables.push(TableState {
                    alignments: alignments.into_iter().map(convert_alignment).collect(),
                    head: Vec::new(),
                    rows: Vec::new(),
                    in_head: false,
                });
            }
            Tag::TableHead => {
                if let Some(table) = self.tables.last_mut() {
                    table.in_head = true;
                }
            }
            Tag::TableRow => {
                if let Some(table) = self.tables.last_mut() {
                    table.in_head = false;
                    table.rows.push(Vec::new());
                }
            }
            Tag::TableCell => self.inlines.clear(),
            Tag::Emphasis => self.inline_stack.push(InlineFrame::Emphasis(Vec::new())),
            Tag::Strong => self.inline_stack.push(InlineFrame::Strong(Vec::new())),
            Tag::Strikethrough => self.inline_stack.push(InlineFrame::Strikethrough(Vec::new())),
            Tag::Link { dest_url, title, .. } => {
                self.inline_stack.push(InlineFrame::Link {
                    dest: dest_url.to_string(),
                    title: non_empty(title),
                    text: Vec::new(),
                });
            }
            Tag::Image { dest_url, title, .. } => {
                self.inline_stack.push(InlineFrame::Image {
                    dest: dest_url.to_string(),
                    title: non_empty(title),
                    alt: Vec::new(),
                });
            }
            Tag::Superscript | Tag::Subscript => {}
            Tag::HtmlBlock | Tag::MetadataBlock(_) => {}
            Tag::DefinitionList | Tag::DefinitionListTitle | Tag::DefinitionListDefinition => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                let inlines = std::mem::take(&mut self.inlines);
                if !inlines.is_empty() {
                    self.push_block(Block::Paragraph(inlines));
                }
            }
            TagEnd::Heading(_) => {
                let inlines = std::mem::take(&mut self.inlines);
                let level = self.heading_level.take().unwrap_or(1);
                let id = self.heading_id.take();
                self.push_block(Block::Heading {
                    level,
                    id,
                    content: inlines,
                });
            }
            TagEnd::CodeBlock => {
                self.in_code_block = false;
                let code = std::mem::take(&mut self.code);
                let language = self.code_language.take();
                self.push_block(Block::CodeBlock { language, code });
            }
            TagEnd::BlockQuote(_) => {
                let (blocks, _) = self.pop_container();
                self.push_block(Block::BlockQuote(blocks));
            }
            TagEnd::List(_) => {
                let (_, frame) = self.pop_container();
                if let BlockFrame::List {
                    ordered,
                    start,
                    items,
                } = frame
                {
                    self.push_block(Block::List {
                        ordered,
                        start,
                        items,
                    });
                }
            }
            TagEnd::Item => {
                let (blocks, frame) = self.pop_container();
                if let BlockFrame::Item { checked } = frame {
                    if let Some(BlockFrame::List { items, .. }) = self.frames.last_mut() {
                        items.push(ListItem { checked, blocks });
                    }
                }
            }
            TagEnd::FootnoteDefinition => {
                let (blocks, frame) = self.pop_container();
                if let BlockFrame::Footnote { label } = frame {
                    self.push_block(Block::FootnoteDefinition { label, blocks });
                }
            }
            TagEnd::Table => {
                if let Some(table) = self.tables.pop() {
                    self.push_block(Block::Table {
                        alignments: table.alignments,
                        head: table.head,
                        rows: table.rows,
                    });
                }
            }
            TagEnd::TableHead | TagEnd::TableRow => {}
            TagEnd::TableCell => self.finish_table_cell(),
            TagEnd::Emphasis => self.pop_inline(InlineFrameKind::Emphasis),
            TagEnd::Strong => self.pop_inline(InlineFrameKind::Strong),
            TagEnd::Strikethrough => self.pop_inline(InlineFrameKind::Strikethrough),
            TagEnd::Link => self.pop_inline(InlineFrameKind::Link),
            TagEnd::Image => self.pop_inline(InlineFrameKind::Image),
            TagEnd::Superscript | TagEnd::Subscript => {}
            TagEnd::DefinitionList | TagEnd::DefinitionListTitle | TagEnd::DefinitionListDefinition => {}
            TagEnd::HtmlBlock | TagEnd::MetadataBlock(_) => {}
        }
    }

    /// Open a new block container.
    fn push_container(&mut self, frame: BlockFrame) {
        self.containers.push(Vec::new());
        self.frames.push(frame);
    }

    /// Close the innermost block container.
    ///
    /// The container stack and the frame stack are parallel, so both are
    /// popped together.
    fn pop_container(&mut self) -> (Vec<Block>, BlockFrame) {
        let blocks = self.containers.pop().unwrap_or_default();
        let frame = self.frames.pop().unwrap_or(BlockFrame::Root);
        (blocks, frame)
    }

    /// Collect a table cell's inline content when the cell ends.
    fn finish_table_cell(&mut self) {
        let inlines = std::mem::take(&mut self.inlines);
        let Some(table) = self.tables.last_mut() else {
            return;
        };
        if table.in_head {
            table.head.push(inlines);
        } else if let Some(row) = table.rows.last_mut() {
            row.push(inlines);
        }
    }

    fn push_inline(&mut self, inline: Inline) {
        match self.inline_stack.last_mut() {
            Some(InlineFrame::Emphasis(children)) => children.push(inline),
            Some(InlineFrame::Strong(children)) => children.push(inline),
            Some(InlineFrame::Strikethrough(children)) => children.push(inline),
            Some(InlineFrame::Link { text, .. }) => text.push(inline),
            Some(InlineFrame::Image { alt, .. }) => alt.push(inline),
            None => self.inlines.push(inline),
        }
    }

    fn pop_inline(&mut self, kind: InlineFrameKind) {
        let Some(frame) = self.inline_stack.pop() else {
            return;
        };
        let inline = match (kind, frame) {
            (InlineFrameKind::Emphasis, InlineFrame::Emphasis(children)) => {
                Inline::Emphasis(children)
            }
            (InlineFrameKind::Strong, InlineFrame::Strong(children)) => Inline::Strong(children),
            (InlineFrameKind::Strikethrough, InlineFrame::Strikethrough(children)) => {
                Inline::Strikethrough(children)
            }
            (InlineFrameKind::Link, InlineFrame::Link { dest, title, text }) => Inline::Link {
                text,
                dest,
                title,
            },
            (InlineFrameKind::Image, InlineFrame::Image { dest, title, alt }) => Inline::Image {
                alt,
                dest,
                title,
            },
            // Mismatched frames cannot happen with a well formed event stream.
            (_, other) => {
                self.inline_stack.push(other);
                return;
            }
        };
        self.push_inline(inline);
    }

    fn push_block(&mut self, block: Block) {
        if let Some(container) = self.containers.last_mut() {
            container.push(block);
        }
    }

    /// Flush any partially collected state at the end of the stream.
    fn finish(&mut self) {
        while let Some(frame) = self.inline_stack.pop() {
            let inline = match frame {
                InlineFrame::Emphasis(children) => Inline::Emphasis(children),
                InlineFrame::Strong(children) => Inline::Strong(children),
                InlineFrame::Strikethrough(children) => Inline::Strikethrough(children),
                InlineFrame::Link { dest, title, text } => Inline::Link { text, dest, title },
                InlineFrame::Image { dest, title, alt } => Inline::Image { alt, dest, title },
            };
            self.push_inline(inline);
        }
        if !self.code.is_empty() {
            let code = std::mem::take(&mut self.code);
            let language = self.code_language.take();
            self.push_block(Block::CodeBlock { language, code });
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum InlineFrameKind {
    Emphasis,
    Strong,
    Strikethrough,
    Link,
    Image,
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn convert_alignment(alignment: CmarkAlignment) -> Alignment {
    match alignment {
        CmarkAlignment::None => Alignment::None,
        CmarkAlignment::Left => Alignment::Left,
        CmarkAlignment::Center => Alignment::Center,
        CmarkAlignment::Right => Alignment::Right,
    }
}

/// Convert an optional attribute into an `Option<String>`, dropping empty
/// values so that `None` and `Some("")` mean the same thing.
fn non_empty(value: pulldown_cmark::CowStr<'_>) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// The headings of a document, with the line each one is on.
///
/// The tree in [`parse`] does not carry positions, so the event stream is
/// walked again with its byte ranges: a heading's range starts at the `#`, and
/// the line is however many newlines came before it.
pub fn outline(source: &str) -> Vec<crate::markdown::ast::OutlineEntry> {
    use crate::markdown::ast::{slugify, OutlineEntry};

    let parser = Parser::new_ext(source, default_options()).into_offset_iter();
    let mut entries = Vec::new();
    let mut used = std::collections::HashSet::new();
    let mut heading: Option<(u8, usize)> = None;
    let mut text = String::new();
    let mut last_offset = 0;
    let mut line = 1;

    for (event, range) in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some((heading_level(level), range.start));
                text.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, offset)) = heading.take() {
                    let offset = offset.min(source.len()).max(last_offset);
                    line += source[last_offset..offset].matches('\n').count() as u64;
                    last_offset = offset;
                    let anchor =
                        crate::markdown::ast::unique_anchor(&slugify(&text), &mut used);
                    entries.push(OutlineEntry {
                        level,
                        title: text.clone(),
                        anchor,
                        line,
                    });
                }
            }
            Event::Text(fragment) | Event::Code(fragment) if heading.is_some() => {
                text.push_str(&fragment);
            }
            _ => {}
        }
    }

    entries
}

/// The link type of an inline link, for callers that need to distinguish
/// external links from internal ones.
pub fn is_external_link(link_type: LinkType) -> bool {
    matches!(
        link_type,
        LinkType::Inline | LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::ast::inline_text;

    #[test]
    fn the_outline_gives_each_heading_its_line() {
        let source = "# Title\n\ntext\n\n## Section one\n\nmore\n\n## Section two\n";
        let outline = outline(source);
        assert_eq!(outline.len(), 3);
        assert_eq!(outline[0].title, "Title");
        assert_eq!(outline[0].line, 1);
        assert_eq!(outline[1].title, "Section one");
        assert_eq!(outline[1].line, 5);
        assert_eq!(outline[2].line, 9);
        assert_eq!(outline[1].level, 2);
    }

    #[test]
    fn outline_anchors_are_unique() {
        let source = "# Same\n\n# Same\n";
        let outline = outline(source);
        assert_eq!(outline[0].anchor, "same");
        assert_eq!(outline[1].anchor, "same-2");
    }

    #[test]
    fn parses_a_heading() {
        let blocks = parse("# Title\n");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            Block::Heading { level, content, .. } => {
                assert_eq!(*level, 1);
                assert_eq!(inline_text(content), "Title");
            }
            other => panic!("expected a heading, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_paragraph_with_emphasis() {
        let blocks = parse("Hello *world* and **bold**.\n");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            Block::Paragraph(inlines) => {
                assert_eq!(inline_text(inlines), "Hello world and bold.");
                assert!(inlines.iter().any(|i| matches!(i, Inline::Emphasis(_))));
                assert!(inlines.iter().any(|i| matches!(i, Inline::Strong(_))));
            }
            other => panic!("expected a paragraph, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_fenced_code_block() {
        let blocks = parse("```rust\nfn main() {}\n```\n");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            Block::CodeBlock { language, code } => {
                assert_eq!(language.as_deref(), Some("rust"));
                assert_eq!(code, "fn main() {}\n");
            }
            other => panic!("expected a code block, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_task_list() {
        let blocks = parse("- [x] done\n- [ ] todo\n");
        match &blocks[0] {
            Block::List { ordered, items, .. } => {
                assert!(!ordered);
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].checked, Some(true));
                assert_eq!(items[1].checked, Some(false));
            }
            other => panic!("expected a list, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_table() {
        let blocks = parse("| a | b |\n| --- | ---: |\n| 1 | 2 |\n");
        match &blocks[0] {
            Block::Table { head, rows, alignments } => {
                assert_eq!(head.len(), 2);
                assert_eq!(rows.len(), 1);
                assert_eq!(alignments[1], Alignment::Right);
            }
            other => panic!("expected a table, got {other:?}"),
        }
    }

    #[test]
    fn parses_math() {
        let blocks = parse("Inline $x^2$ and display:\n\n$$\\int f$$\n");
        let found: Vec<&Inline> = blocks
            .iter()
            .filter_map(|block| match block {
                Block::Paragraph(inlines) => Some(inlines),
                _ => None,
            })
            .flatten()
            .filter(|inline| matches!(inline, Inline::Math { .. }))
            .collect();
        assert_eq!(found.len(), 2);
        match found[0] {
            Inline::Math { display, source } => {
                assert!(!display);
                assert_eq!(source, "x^2");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn parses_nested_lists() {
        let blocks = parse("- one\n  - nested\n- two\n");
        match &blocks[0] {
            Block::List { items, .. } => {
                assert_eq!(items.len(), 2);
                assert!(matches!(items[0].blocks[0], Block::List { .. }));
            }
            other => panic!("expected a list, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_block_quote() {
        let blocks = parse("> quoted\n");
        match &blocks[0] {
            Block::BlockQuote(inner) => assert_eq!(inner.len(), 1),
            other => panic!("expected a block quote, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_link() {
        let blocks = parse("See [the docs](https://example.com).\n");
        match &blocks[0] {
            Block::Paragraph(inlines) => {
                let link = inlines
                    .iter()
                    .find_map(|i| match i {
                        Inline::Link { dest, text, .. } => Some((dest.clone(), inline_text(text))),
                        _ => None,
                    })
                    .expect("a link");
                assert_eq!(link.0, "https://example.com");
                assert_eq!(link.1, "the docs");
            }
            other => panic!("expected a paragraph, got {other:?}"),
        }
    }
}
