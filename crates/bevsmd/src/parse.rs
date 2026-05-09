//! CommonMark → Block IR.
//!
//! Walks pulldown-cmark's event stream into a tree of `Block` / `Inline`.
//! No styling decisions live here — that's `layout.rs`'s job. Keeping
//! parsing structural lets layout swap themes without re-parsing.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Clone, Debug)]
pub enum Block {
    Heading {
        level: u8,
        inlines: Vec<Inline>,
    },
    Paragraph(Vec<Inline>),
    BlockQuote(Vec<Block>),
    List {
        ordered: bool,
        /// Starting index for ordered lists; ignored when `ordered = false`.
        start: u64,
        items: Vec<Vec<Block>>,
    },
    /// Fenced or indented code block. Trailing newline already stripped.
    CodeBlock {
        lang: Option<String>,
        text: String,
    },
    /// Horizontal rule (`---`).
    Rule,
}

#[derive(Clone, Debug)]
pub enum Inline {
    Text(String),
    /// Inline `code`.
    Code(String),
    /// `*emphasis*`.
    Emph(Vec<Inline>),
    /// `**strong**`.
    Strong(Vec<Inline>),
    /// `~~strikethrough~~` (GFM).
    Strike(Vec<Inline>),
    Link {
        text: Vec<Inline>,
        url: String,
    },
    /// Hard line break (`<br>` / two trailing spaces). Soft breaks
    /// collapse to a space and aren't represented here.
    HardBreak,
}

/// Parse a markdown source string into a `Vec<Block>`.
///
/// Enables GFM strikethrough by default (rust-analyzer's hover output
/// uses it for deprecated markers). Tables parse but currently render
/// as plain paragraphs until table layout lands.
pub fn parse_markdown(source: &str) -> Vec<Block> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(source, options);
    let mut walker = Walker::new();
    walker.run(parser);
    walker.finish()
}

/// Stack-based walker. Each `Tag::*` push/pop matches a frame; inline
/// content accumulates into the current frame's `inlines`, block content
/// into `blocks`. Lists are special: items live in `list_items` so the
/// final `List` block carries `Vec<Vec<Block>>`.
struct Walker {
    stack: Vec<Frame>,
}

struct Frame {
    kind: FrameKind,
    blocks: Vec<Block>,
    inlines: Vec<Inline>,
    code: String,
    /// Items collected while inside a `FrameKind::List`. Each item closes
    /// by pushing onto its parent List frame's `list_items`.
    list_items: Vec<Vec<Block>>,
    meta: FrameMeta,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            kind: FrameKind::Root,
            blocks: Vec::new(),
            inlines: Vec::new(),
            code: String::new(),
            list_items: Vec::new(),
            meta: FrameMeta::default(),
        }
    }
}

#[derive(Default)]
enum FrameKind {
    #[default]
    Root,
    Heading,
    Paragraph,
    BlockQuote,
    List,
    Item,
    CodeBlock,
    Strong,
    Emph,
    Strike,
    Link,
    /// Generic block-level container we don't render specially (tables,
    /// images, footnote definitions). Folds its blocks straight into the
    /// parent so structure is preserved as a flat fallback.
    Passthrough,
}

#[derive(Default)]
struct FrameMeta {
    heading_level: u8,
    list_ordered: bool,
    list_start: u64,
    code_lang: Option<String>,
    link_url: String,
}

impl Walker {
    fn new() -> Self {
        Self {
            stack: vec![Frame::default()],
        }
    }

    fn finish(mut self) -> Vec<Block> {
        // The root frame's `blocks` is the result. Any straggler inlines
        // (rare — empty docs) become a final paragraph.
        let mut root = self.stack.pop().expect("root frame");
        if !root.inlines.is_empty() {
            root.blocks
                .push(Block::Paragraph(std::mem::take(&mut root.inlines)));
        }
        root.blocks
    }

    fn run(&mut self, parser: Parser<'_>) {
        for event in parser {
            self.event(event);
        }
    }

    fn top(&mut self) -> &mut Frame {
        self.stack.last_mut().expect("non-empty walker stack")
    }

    fn open(&mut self, kind: FrameKind, meta: FrameMeta) {
        self.stack.push(Frame {
            kind,
            blocks: Vec::new(),
            inlines: Vec::new(),
            code: String::new(),
            list_items: Vec::new(),
            meta,
        });
    }

    fn close(&mut self) -> Frame {
        self.stack.pop().expect("close on empty stack")
    }

    fn emit_block(&mut self, block: Block) {
        self.top().blocks.push(block);
    }

    fn push_inline(&mut self, inline: Inline) {
        self.top().inlines.push(inline);
    }

    fn fold_inline_frame(&mut self, into: impl FnOnce(Vec<Inline>) -> Inline) {
        let f = self.close();
        let inline = into(f.inlines);
        self.push_inline(inline);
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => self.text(t.into_string()),
            Event::Code(t) => self.push_inline(Inline::Code(t.into_string())),
            Event::Html(_) | Event::InlineHtml(_) => {
                // Drop raw HTML — render nothing rather than dump source.
            }
            Event::SoftBreak => self.push_inline(Inline::Text(" ".into())),
            Event::HardBreak => self.push_inline(Inline::HardBreak),
            Event::Rule => self.emit_block(Block::Rule),
            Event::FootnoteReference(_)
            | Event::TaskListMarker(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_) => {
                // Out of scope this milestone.
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.open(FrameKind::Paragraph, FrameMeta::default()),
            Tag::Heading { level, .. } => self.open(
                FrameKind::Heading,
                FrameMeta {
                    heading_level: heading_level_to_u8(level),
                    ..Default::default()
                },
            ),
            Tag::BlockQuote(_) => self.open(FrameKind::BlockQuote, FrameMeta::default()),
            Tag::List(start) => self.open(
                FrameKind::List,
                FrameMeta {
                    list_ordered: start.is_some(),
                    list_start: start.unwrap_or(1),
                    ..Default::default()
                },
            ),
            Tag::Item => self.open(FrameKind::Item, FrameMeta::default()),
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(s) => {
                        let s = s.into_string();
                        if s.is_empty() {
                            None
                        } else {
                            // pulldown-cmark passes the whole info string;
                            // markdown convention is the language token
                            // up to the first whitespace.
                            Some(s.split_whitespace().next().unwrap_or("").to_string())
                                .filter(|t| !t.is_empty())
                        }
                    }
                    pulldown_cmark::CodeBlockKind::Indented => None,
                };
                self.open(
                    FrameKind::CodeBlock,
                    FrameMeta {
                        code_lang: lang,
                        ..Default::default()
                    },
                );
            }
            Tag::Strong => self.open(FrameKind::Strong, FrameMeta::default()),
            Tag::Emphasis => self.open(FrameKind::Emph, FrameMeta::default()),
            Tag::Strikethrough => self.open(FrameKind::Strike, FrameMeta::default()),
            Tag::Link { dest_url, .. } => self.open(
                FrameKind::Link,
                FrameMeta {
                    link_url: dest_url.into_string(),
                    ..Default::default()
                },
            ),
            // Tables / images / footnotes / math / html-block: keep the
            // stack balanced and let inline content flow up to the
            // parent. Image `<alt>` becomes text; tables become a flat
            // sequence of cell paragraphs.
            _ => self.open(FrameKind::Passthrough, FrameMeta::default()),
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                let f = self.close();
                self.emit_block(Block::Paragraph(f.inlines));
            }
            TagEnd::Heading(_) => {
                let f = self.close();
                self.emit_block(Block::Heading {
                    level: f.meta.heading_level,
                    inlines: f.inlines,
                });
            }
            TagEnd::BlockQuote(_) => {
                let f = self.close();
                self.emit_block(Block::BlockQuote(f.blocks));
            }
            TagEnd::List(_) => {
                let f = self.close();
                self.emit_block(Block::List {
                    ordered: f.meta.list_ordered,
                    start: f.meta.list_start,
                    items: f.list_items,
                });
            }
            TagEnd::Item => {
                let f = self.close();
                let mut item_blocks = f.blocks;
                // List items frequently contain bare inline content (no
                // surrounding `<p>`). Wrap any leftover inlines into a
                // paragraph so layout sees uniform Block-typed content.
                if !f.inlines.is_empty() {
                    item_blocks.insert(0, Block::Paragraph(f.inlines));
                }
                // Push onto the parent List's items collection.
                self.top().list_items.push(item_blocks);
            }
            TagEnd::CodeBlock => {
                let f = self.close();
                let mut text = f.code;
                if text.ends_with('\n') {
                    text.pop();
                }
                self.emit_block(Block::CodeBlock {
                    lang: f.meta.code_lang,
                    text,
                });
            }
            TagEnd::Strong => self.fold_inline_frame(Inline::Strong),
            TagEnd::Emphasis => self.fold_inline_frame(Inline::Emph),
            TagEnd::Strikethrough => self.fold_inline_frame(Inline::Strike),
            TagEnd::Link => {
                let f = self.close();
                let url = f.meta.link_url;
                self.push_inline(Inline::Link {
                    text: f.inlines,
                    url,
                });
            }
            _ => {
                // Passthrough close: lift child blocks into the parent
                // and discard frame metadata. Inlines bubble up too.
                let f = self.close();
                if !f.inlines.is_empty() {
                    self.emit_block(Block::Paragraph(f.inlines));
                }
                for b in f.blocks {
                    self.emit_block(b);
                }
            }
        }
    }

    fn text(&mut self, t: String) {
        let frame = self.top();
        match frame.kind {
            FrameKind::CodeBlock => frame.code.push_str(&t),
            _ => frame.inlines.push(Inline::Text(t)),
        }
    }
}

fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(blocks: &[Block]) -> &Block {
        blocks.first().expect("at least one block")
    }

    #[test]
    fn parses_simple_paragraph() {
        let blocks = parse_markdown("hello world");
        assert_eq!(blocks.len(), 1);
        match first(&blocks) {
            Block::Paragraph(inlines) => {
                assert!(matches!(&inlines[..], [Inline::Text(t)] if t == "hello world"));
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn parses_heading_with_emphasis() {
        let blocks = parse_markdown("# Hello *world*");
        match first(&blocks) {
            Block::Heading { level, inlines } => {
                assert_eq!(*level, 1);
                assert_eq!(inlines.len(), 2);
                assert!(matches!(&inlines[0], Inline::Text(t) if t == "Hello "));
                assert!(matches!(&inlines[1], Inline::Emph(_)));
            }
            other => panic!("expected heading, got {other:?}"),
        }
    }

    #[test]
    fn parses_fenced_code_block() {
        let blocks = parse_markdown("```rust\nfn main() {}\n```");
        match first(&blocks) {
            Block::CodeBlock { lang, text } => {
                assert_eq!(lang.as_deref(), Some("rust"));
                assert_eq!(text, "fn main() {}");
            }
            other => panic!("expected code block, got {other:?}"),
        }
    }

    #[test]
    fn parses_unordered_list_items() {
        let blocks = parse_markdown("- alpha\n- beta\n- gamma");
        match first(&blocks) {
            Block::List { ordered, items, .. } => {
                assert!(!ordered);
                assert_eq!(items.len(), 3);
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn parses_ordered_list_with_start() {
        let blocks = parse_markdown("3. third\n4. fourth");
        match first(&blocks) {
            Block::List {
                ordered,
                start,
                items,
            } => {
                assert!(ordered);
                assert_eq!(*start, 3);
                assert_eq!(items.len(), 2);
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn parses_link() {
        let blocks = parse_markdown("[click](https://example.com)");
        let para = match first(&blocks) {
            Block::Paragraph(inlines) => inlines,
            other => panic!("expected paragraph, got {other:?}"),
        };
        match &para[0] {
            Inline::Link { text, url } => {
                assert_eq!(url, "https://example.com");
                assert!(matches!(&text[..], [Inline::Text(t)] if t == "click"));
            }
            other => panic!("expected link, got {other:?}"),
        }
    }

    #[test]
    fn parses_blockquote_containing_paragraph() {
        let blocks = parse_markdown("> quoted text");
        match first(&blocks) {
            Block::BlockQuote(inner) => {
                assert!(matches!(&inner[..], [Block::Paragraph(_)]));
            }
            other => panic!("expected blockquote, got {other:?}"),
        }
    }

    #[test]
    fn parses_horizontal_rule() {
        let blocks = parse_markdown("---");
        assert!(matches!(first(&blocks), Block::Rule));
    }
}
