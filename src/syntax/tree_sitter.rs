//! Tree-sitter syntax highlighting provider using low-level QueryCursor API
//! This approach matches Zed's implementation for better performance.

use bevy::prelude::*;
use tree_sitter::{Language, Parser, Query, QueryCursor, Tree};
use crate::types::LineSegment;
use super::highlighter::{SyntaxProvider, map_highlight_color};
use std::ops::Range;
use streaming_iterator::StreamingIterator;
use ropey::Rope;

/// Text provider for tree-sitter that wraps a Rope (like Zed does)
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

/// Maximum bytes to query at once (same as Zed)
pub const MAX_BYTES_TO_QUERY: usize = 16 * 1024;

/// Zero-copy rope reader for tree-sitter parsing
pub(crate) struct RopeReader<'a> {
    rope: &'a Rope,
    chunks: ropey::iter::Chunks<'a>,
    current_chunk: &'a [u8],
    total_byte_offset: usize,
}

impl<'a> RopeReader<'a> {
    pub(crate) fn new(rope: &'a Rope) -> Self {
        let mut chunks = rope.chunks();
        let current_chunk = chunks.next().map(|s| s.as_bytes()).unwrap_or(b"");
        Self {
            rope,
            chunks,
            current_chunk,
            total_byte_offset: 0,
        }
    }

    pub(crate) fn read(&mut self, byte_offset: usize) -> &'a [u8] {
        // If seeking backwards, reset
        if byte_offset < self.total_byte_offset {
            *self = Self::new(self.rope);
        }

        // Skip forward to the requested offset
        while self.total_byte_offset + self.current_chunk.len() <= byte_offset {
            self.total_byte_offset += self.current_chunk.len();
            self.current_chunk = self.chunks.next().map(|s| s.as_bytes()).unwrap_or(b"");
            if self.current_chunk.is_empty() {
                return b"";
            }
        }

        // Return slice from current position
        let offset_in_chunk = byte_offset.saturating_sub(self.total_byte_offset);
        &self.current_chunk[offset_in_chunk.min(self.current_chunk.len())..]
    }
}

/// Tree-sitter-based syntax highlighting provider
pub struct TreeSitterProvider {
    /// The highlight query
    query: Option<Query>,

    /// Cached tree-sitter tree for incremental parsing
    pub(crate) cached_tree: Option<Tree>,

    /// Cached tree-sitter parser (reused for performance)
    pub(crate) cached_parser: Option<Parser>,

    /// Cached tree-sitter language (for incremental parsing)
    pub(crate) cached_language: Option<Language>,

    /// Pending edits to apply to the tree before re-parsing
    pub(crate) pending_edits: Vec<tree_sitter::InputEdit>,

    /// Reusable query cursor
    query_cursor: QueryCursor,

    /// Cached full document rope (needed for TextProvider)
    pub(crate) cached_rope: Option<Rope>,
}

impl TreeSitterProvider {
    /// Create a new tree-sitter provider
    pub fn new() -> Self {
        Self {
            query: None,
            cached_tree: None,
            cached_parser: None,
            cached_language: None,
            pending_edits: Vec::new(),
            query_cursor: QueryCursor::new(),
            cached_rope: None,
        }
    }

    /// Set the highlight query and language
    pub fn set_query(&mut self, query_source: &str, language: Language) -> Result<(), tree_sitter::QueryError> {
        let query = Query::new(&language, query_source)?;
        self.query = Some(query);
        self.cached_language = Some(language);
        // Reset cached parser/tree so they get reinitialized with the new language
        self.cached_parser = None;
        self.cached_tree = None;
        Ok(())
    }

    /// Record an edit with full position information for incremental parsing
    pub fn record_edit_with_positions(
        &mut self,
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
        start_position: tree_sitter::Point,
        old_end_position: tree_sitter::Point,
        new_end_position: tree_sitter::Point,
    ) {
        let edit = tree_sitter::InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position,
            old_end_position,
            new_end_position,
        };

        self.pending_edits.push(edit);
    }

    /// Get readonly access to the cached tree
    pub fn tree(&self) -> Option<&tree_sitter::Tree> {
        self.cached_tree.as_ref()
    }

    /// Invalidate the tree-sitter tree (like Zed does when content changes)
    /// This clears the cached tree to prevent using stale data with mismatched byte positions
    pub fn invalidate_tree(&mut self) {
        self.cached_tree = None;
        self.cached_rope = None;
        self.pending_edits.clear();
    }

    /// Update the parse tree from a rope (zero-copy, like Zed)
    pub fn update_tree(&mut self, rope: &Rope) {
        // Cache the rope for use in highlighting (clone is cheap - Rope uses Arc internally)
        self.cached_rope = Some(rope.clone());

        // Use rope reader for zero-copy parsing
        let mut reader = RopeReader::new(rope);
        let mut callback = |byte_offset: usize, _position: tree_sitter::Point| -> &[u8] {
            reader.read(byte_offset)
        };

        // Try incremental parsing first
        if let Some(ref mut tree) = self.cached_tree {
            // Apply any pending edits to the tree
            for edit in self.pending_edits.drain(..) {
                tree.edit(&edit);
            }

            // Re-parse incrementally using the edited tree
            if let Some(ref mut parser) = self.cached_parser {
                if let Some(new_tree) = parser.parse_with(&mut callback, Some(tree)) {
                    *tree = new_tree;
                }
            }
        } else if let Some(ref language) = self.cached_language {
            // First parse - initialize parser and tree using the cached language
            if self.cached_parser.is_none() {
                let mut parser = tree_sitter::Parser::new();
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
        theme: &crate::settings::SyntaxTheme,
        default_color: Color,
    ) -> Vec<Vec<LineSegment>> {
        let query = match &self.query {
            Some(q) => q,
            None => {
                // Return plain text without highlighting
                return text
                    .lines()
                    .map(|line| {
                        if line.trim().is_empty() {
                            vec![]
                        } else {
                            vec![LineSegment {
                                text: line.to_string(),
                                color: default_color,
                            }]
                        }
                    })
                    .collect();
            }
        };

        let tree = match &self.cached_tree {
            Some(t) => t,
            None => {
                // No tree available, return plain text
                return text
                    .lines()
                    .map(|line| {
                        if line.trim().is_empty() {
                            vec![]
                        } else {
                            vec![LineSegment {
                                text: line.to_string(),
                                color: default_color,
                            }]
                        }
                    })
                    .collect();
            }
        };

        let rope = match &self.cached_rope {
            Some(r) => r,
            None => {
                // No cached rope, return plain text
                return text
                    .lines()
                    .map(|line| {
                        if line.trim().is_empty() {
                            vec![]
                        } else {
                            vec![LineSegment {
                                text: line.to_string(),
                                color: default_color,
                            }]
                        }
                    })
                    .collect();
            }
        };

        // Safety check: tree might be temporarily out of sync after edit
        // This can happen when highlighting runs before tree update completes
        if tree.root_node().end_byte() > rope.len_bytes() {
            // Tree is stale, return plain text for now
            return text
                .lines()
                .map(|line| {
                    if line.trim().is_empty() {
                        vec![]
                    } else {
                        vec![LineSegment {
                            text: line.to_string(),
                            color: default_color,
                        }]
                    }
                })
                .collect();
        }

        let text_bytes = text.as_bytes();
        let end_byte = start_byte + text_bytes.len();

        // Limit query range to MAX_BYTES_TO_QUERY (like Zed does)
        let query_end = start_byte + text_bytes.len().min(MAX_BYTES_TO_QUERY);
        let byte_range = start_byte..query_end;

        // Set byte range on query cursor (like Zed does)
        self.query_cursor.set_byte_range(byte_range.clone());

        // Execute query using captures with RopeProvider (like Zed does)
        let mut captures = self.query_cursor.captures(
            query,
            tree.root_node(),
            RopeProvider(rope),
        );

        // Build a map of byte ranges to highlight names
        // Using streaming_iterator pattern like Zed
        let mut highlights: Vec<(Range<usize>, &str)> = Vec::new();

        while let Some((match_ref, capture_index)) = captures.next() {
            let capture = &match_ref.captures[*capture_index];
            let capture_name = &query.capture_names()[capture.index as usize];
            let node = capture.node;
            let node_range = node.byte_range();

            // Convert node byte range (relative to full document) to slice-relative
            if node_range.start >= start_byte && node_range.end <= end_byte {
                let slice_start = node_range.start - start_byte;
                let slice_end = node_range.end - start_byte;
                highlights.push((slice_start..slice_end, capture_name));
            }
        }

        // Sort by start position
        highlights.sort_by_key(|(range, _)| range.start);

        // Convert highlights to per-line segments
        let mut lines: Vec<Vec<LineSegment>> = Vec::new();
        let mut current_line_segments: Vec<LineSegment> = Vec::new();
        let mut byte_pos = 0;
        let mut current_highlight_idx = 0;

        for line in text.lines() {
            current_line_segments.clear();
            let line_start = byte_pos;
            let line_end = byte_pos + line.len();
            let mut char_pos = 0;

            while char_pos < line.len() {
                let current_byte = line_start + char_pos;

                // Find active highlight at this position
                let mut active_highlight: Option<&str> = None;
                for i in current_highlight_idx..highlights.len() {
                    let (range, name) = &highlights[i];
                    if range.start > current_byte {
                        break;
                    }
                    if range.contains(&current_byte) {
                        active_highlight = Some(name);
                        current_highlight_idx = i;
                        break;
                    }
                }

                // Find the end of this segment (either end of line, or next highlight change)
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
                    let segment_text = &line[char_pos..segment_end];
                    let color = map_highlight_color(active_highlight, theme, default_color);

                    if !segment_text.is_empty() {
                        current_line_segments.push(LineSegment {
                            text: segment_text.to_string(),
                            color,
                        });
                    }
                    char_pos = segment_end;
                } else {
                    char_pos += 1;
                }
            }

            lines.push(current_line_segments.clone());
            byte_pos = line_end + 1; // +1 for newline
        }

        // Pad to match expected line count
        let expected_lines = end_line - start_line;
        while lines.len() < expected_lines {
            lines.push(Vec::new());
        }

        lines
    }

    fn notify_edit(&mut self, start_byte: usize, old_end_byte: usize, new_end_byte: usize) {
        // Note: This simple version doesn't have Point info
        // The caller should use record_edit_with_positions instead
        let edit = tree_sitter::InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position: tree_sitter::Point::new(0, 0),
            old_end_position: tree_sitter::Point::new(0, 0),
            new_end_position: tree_sitter::Point::new(0, 0),
        };

        self.pending_edits.push(edit);
    }

    fn is_available(&self) -> bool {
        // Only check if query is configured, not if tree exists
        // Tree will be built on first update_tree() call
        self.query.is_some()
    }
}
