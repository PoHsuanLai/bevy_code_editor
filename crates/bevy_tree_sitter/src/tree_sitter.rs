//! Tree-sitter syntax-highlight provider using the low-level `QueryCursor` API.
//!
//! Modeled on Zed's pattern: parse with a `RopeReader` for zero-copy reads,
//! run highlight queries against a cached tree, and emit `HighlightRange`s
//! whose capture names are `Arc<str>` clones from a per-query intern table.

use crate::highlight::{HighlightRange, SyntaxProvider};
use ropey::Rope;
use std::ops::Range;
use std::sync::Arc;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Parser, Query, QueryCursor, Tree};

/// Wraps a `Rope` so tree-sitter can pull text bytes through the
/// `TextProvider` trait (used inside query execution).
struct RopeProvider<'a>(&'a Rope);

struct RopeChunks<'a> {
    chunks: ropey::iter::Chunks<'a>,
}

impl<'a> tree_sitter::TextProvider<&'a [u8]> for RopeProvider<'a> {
    type I = RopeChunks<'a>;

    fn text(&mut self, node: tree_sitter::Node) -> Self::I {
        let byte_range = node.byte_range();
        let start_char = self.0.byte_to_char(byte_range.start);
        let end_char = self.0.byte_to_char(byte_range.end.min(self.0.len_bytes()));
        RopeChunks {
            chunks: self.0.slice(start_char..end_char).chunks(),
        }
    }
}

impl<'a> Iterator for RopeChunks<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        self.chunks.next().map(|s| s.as_bytes())
    }
}

/// Maximum bytes to query at once (matches Zed's heuristic).
pub const MAX_BYTES_TO_QUERY: usize = 16 * 1024;

/// Zero-copy rope reader for tree-sitter parsing.
///
/// `parse_with` calls this with arbitrary byte offsets; the reader streams
/// rope chunks forward and returns a slice starting at the requested offset
/// without allocating. Seeking backwards resets the chunk iterator.
pub struct RopeReader<'a> {
    rope: &'a Rope,
    chunks: ropey::iter::Chunks<'a>,
    current_chunk: &'a [u8],
    total_byte_offset: usize,
}

impl<'a> RopeReader<'a> {
    pub fn new(rope: &'a Rope) -> Self {
        let mut chunks = rope.chunks();
        let current_chunk = chunks.next().map(|s| s.as_bytes()).unwrap_or(b"");
        Self {
            rope,
            chunks,
            current_chunk,
            total_byte_offset: 0,
        }
    }

    pub fn read(&mut self, byte_offset: usize) -> &'a [u8] {
        if byte_offset < self.total_byte_offset {
            *self = Self::new(self.rope);
        }

        while self.total_byte_offset + self.current_chunk.len() <= byte_offset {
            self.total_byte_offset += self.current_chunk.len();
            self.current_chunk = self.chunks.next().map(|s| s.as_bytes()).unwrap_or(b"");
            if self.current_chunk.is_empty() {
                return b"";
            }
        }

        let offset_in_chunk = byte_offset.saturating_sub(self.total_byte_offset);
        &self.current_chunk[offset_in_chunk.min(self.current_chunk.len())..]
    }
}

/// Tree-sitter-based `SyntaxProvider`.
pub struct TreeSitterProvider {
    /// The compiled highlight query.
    query: Option<Query>,

    /// Capture-name intern table for `query`. Indexed by `capture.index`.
    /// Populated alongside `query` in `set_query`; cloning an entry is a
    /// refcount bump rather than a string allocation.
    capture_names: Vec<Arc<str>>,

    /// Cached parse tree (incremental).
    pub cached_tree: Option<Tree>,

    /// Reused parser, keyed to `cached_language`.
    pub cached_parser: Option<Parser>,

    /// Language used for parsing — kept so async tasks can recreate a parser.
    pub cached_language: Option<Language>,

    /// Reusable query cursor (avoids per-call allocation).
    query_cursor: QueryCursor,

    /// Last rope handed to `update_tree` / `apply_sync_edit`.
    /// `Rope` is `Arc`-backed, so cloning is cheap.
    pub cached_rope: Option<Rope>,
}

impl TreeSitterProvider {
    pub fn new() -> Self {
        Self {
            query: None,
            capture_names: Vec::new(),
            cached_tree: None,
            cached_parser: None,
            cached_language: None,
            query_cursor: QueryCursor::new(),
            cached_rope: None,
        }
    }

    /// Compile and store a highlight query for the given language.
    ///
    /// Resets the cached parser/tree so the next parse picks up the new grammar.
    pub fn set_query(
        &mut self,
        query_source: &str,
        language: Language,
    ) -> Result<(), tree_sitter::QueryError> {
        let query = Query::new(&language, query_source)?;
        self.capture_names = query
            .capture_names()
            .iter()
            .map(|name| Arc::<str>::from(*name))
            .collect();
        self.query = Some(query);
        self.cached_language = Some(language);
        self.cached_parser = None;
        self.cached_tree = None;
        Ok(())
    }

    /// Apply an edit synchronously (Zed's "tree interpolation").
    ///
    /// `tree.edit()` shifts the existing tree's byte offsets in O(log n)
    /// without re-parsing — the tree stays structurally valid for highlight
    /// queries while the full async re-parse runs in the background. For
    /// small documents we also re-parse synchronously so highlights are
    /// immediately correct.
    pub fn apply_sync_edit(&mut self, edit: tree_sitter::InputEdit, rope: &Rope) {
        if let Some(ref mut tree) = self.cached_tree {
            tree.edit(&edit);

            if let Some(ref mut parser) = self.cached_parser {
                let rope_clone = rope.clone();
                if let Some(new_tree) = parser.parse_with(
                    &mut |byte_offset, _| {
                        let (chunk, start_byte, _, _) = rope_clone.chunk_at_byte(byte_offset);
                        &chunk.as_bytes()[(byte_offset - start_byte)..]
                    },
                    Some(tree),
                ) {
                    *tree = new_tree;
                }
            }
        }
        self.cached_rope = Some(rope.clone());
    }

    /// Read-only access to the cached tree (for folding, outline, etc.).
    pub fn tree(&self) -> Option<&Tree> {
        self.cached_tree.as_ref()
    }

    /// Drop the cached tree and rope. Used when content shifts in a way that
    /// would leave byte offsets stale.
    pub fn invalidate_tree(&mut self) {
        self.cached_tree = None;
        self.cached_rope = None;
    }

    /// Re-parse the tree from `rope` (zero-copy via `RopeReader`).
    pub fn update_tree(&mut self, rope: &Rope) {
        self.cached_rope = Some(rope.clone());

        let mut reader = RopeReader::new(rope);
        let mut callback = |byte_offset: usize, _position: tree_sitter::Point| -> &[u8] {
            reader.read(byte_offset)
        };

        if let Some(ref mut tree) = self.cached_tree {
            if let Some(ref mut parser) = self.cached_parser {
                if let Some(new_tree) = parser.parse_with(&mut callback, Some(tree)) {
                    *tree = new_tree;
                }
            }
        } else if let Some(ref language) = self.cached_language {
            if self.cached_parser.is_none() {
                let mut parser = Parser::new();
                if parser.set_language(language).is_ok() {
                    self.cached_parser = Some(parser);
                }
            }

            if let Some(ref mut parser) = self.cached_parser {
                self.cached_tree = parser.parse_with(&mut callback, None);
            }
        }
    }
}

impl Default for TreeSitterProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxProvider for TreeSitterProvider {
    fn highlight_range(
        &mut self,
        text: &str,
        start_line: usize,
        end_line: usize,
        start_byte: usize,
    ) -> Vec<Vec<HighlightRange>> {
        // Bail-out helper: empty highlight runs per line. The consumer renders
        // the line with whatever default styling it picks for un-captured text.
        let plain_lines = || -> Vec<Vec<HighlightRange>> {
            let mut lines: Vec<Vec<HighlightRange>> = text.lines().map(|_| Vec::new()).collect();
            let expected_lines = end_line.saturating_sub(start_line);
            while lines.len() < expected_lines {
                lines.push(Vec::new());
            }
            lines
        };

        let query = match &self.query {
            Some(q) => q,
            None => return plain_lines(),
        };

        let tree = match &self.cached_tree {
            Some(t) => t,
            None => return plain_lines(),
        };

        let rope = match &self.cached_rope {
            Some(r) => r,
            None => return plain_lines(),
        };

        // Tree's byte offsets can lag the rope momentarily after an edit;
        // skip highlighting that frame rather than emitting wrong ranges.
        if tree.root_node().end_byte() > rope.len_bytes() {
            return plain_lines();
        }

        let text_bytes = text.as_bytes();
        let end_byte = start_byte + text_bytes.len();

        let query_end = start_byte + text_bytes.len().min(MAX_BYTES_TO_QUERY);
        let byte_range = start_byte..query_end;

        self.query_cursor.set_byte_range(byte_range.clone());

        let mut captures = self
            .query_cursor
            .captures(query, tree.root_node(), RopeProvider(rope));

        // Slice-relative byte ranges paired with their interned capture name.
        let mut highlights: Vec<(Range<usize>, Arc<str>)> = Vec::new();

        while let Some((match_ref, capture_index)) = captures.next() {
            let capture = &match_ref.captures[*capture_index];
            let capture_name = match self.capture_names.get(capture.index as usize) {
                Some(name) => Arc::clone(name),
                None => continue,
            };
            let node = capture.node;
            let node_range = node.byte_range();

            // Convert from document-absolute to slice-relative, clamping to the
            // queried text (interpolated trees can have nodes that span across
            // the queried window).
            if node_range.end > start_byte && node_range.start < end_byte {
                let slice_start = node_range.start.saturating_sub(start_byte);
                let slice_end = (node_range.end - start_byte).min(text_bytes.len());
                if slice_start < slice_end {
                    highlights.push((slice_start..slice_end, capture_name));
                }
            }
        }

        highlights.sort_by_key(|(range, _)| range.start);

        // Walk the text line-by-line, emitting one HighlightRange per
        // contiguous run of identical-capture (or no-capture) bytes. Byte
        // ranges are document-absolute (start_byte + per-line byte_pos)
        // so consumers can index into the original document directly.
        let mut lines: Vec<Vec<HighlightRange>> = Vec::new();
        let mut byte_pos = 0;
        let mut current_highlight_idx = 0;

        for line in text.lines() {
            let mut current_line: Vec<HighlightRange> = Vec::new();
            let line_start = byte_pos;
            let line_end = byte_pos + line.len();
            let mut char_pos = 0;

            while char_pos < line.len() {
                let current_byte = line_start + char_pos;

                let mut active_capture: Option<Arc<str>> = None;
                for i in current_highlight_idx..highlights.len() {
                    let (range, name) = &highlights[i];
                    if range.start > current_byte {
                        break;
                    }
                    if range.contains(&current_byte) {
                        active_capture = Some(Arc::clone(name));
                        current_highlight_idx = i;
                        break;
                    }
                }

                let mut segment_end = line.len();
                for (range, _) in &highlights[current_highlight_idx..] {
                    if range.start > current_byte && range.start < line_end {
                        segment_end = range.start - line_start;
                        break;
                    }
                    if range.end > current_byte && range.end < line_end {
                        segment_end = range.end - line_start;
                        break;
                    }
                }

                if segment_end > char_pos {
                    let abs_start = start_byte + line_start + char_pos;
                    let abs_end = start_byte + line_start + segment_end;
                    if let Some(name) = active_capture {
                        current_line.push(HighlightRange {
                            byte_range: abs_start..abs_end,
                            capture_name: name,
                        });
                    }
                    char_pos = segment_end;
                } else {
                    char_pos += 1;
                }
            }

            lines.push(current_line);
            byte_pos = line_end + 1;
        }

        let expected_lines = end_line.saturating_sub(start_line);
        while lines.len() < expected_lines {
            lines.push(Vec::new());
        }

        lines
    }

    fn notify_edit(&mut self, start_byte: usize, old_end_byte: usize, new_end_byte: usize) {
        // Approximate Points — async parse will correct them.
        let edit = tree_sitter::InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position: tree_sitter::Point::new(0, 0),
            old_end_position: tree_sitter::Point::new(0, 0),
            new_end_position: tree_sitter::Point::new(0, 0),
        };
        if let Some(ref mut tree) = self.cached_tree {
            tree.edit(&edit);
        }
    }

    fn is_available(&self) -> bool {
        self.query.is_some()
    }
}
