//! Tree-sitter syntax-highlight provider using the low-level `QueryCursor` API.

use crate::highlight::{highlight_ranges, HighlightRange};
use ropey::Rope;
use std::ops::Range;
use std::sync::Arc;
use tree_sitter::{Language, Parser, Query, QueryCursor, Tree};

/// Zero-copy rope reader for `parse_with`. Streams chunks forward; seeking
/// backwards resets the chunk iterator.
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

/// Maximum bytes to query at once (matches Zed's heuristic).
/// Exposed for `bevscode`'s `highlight_range` call site.
pub const MAX_BYTES_TO_QUERY_INTERNAL: usize = 16 * 1024;

/// Pure query executor: compiled highlight query + reusable cursor.
/// Does not own a tree or rope — callers pass those in from `SyntaxTree`.
pub struct TreeSitterProvider {
    query: Option<Query>,
    /// Intern table indexed by `capture.index`; cloning is a refcount bump.
    capture_names: Vec<Arc<str>>,
    query_cursor: QueryCursor,
}

impl TreeSitterProvider {
    pub fn new() -> Self {
        Self {
            query: None,
            capture_names: Vec::new(),
            query_cursor: QueryCursor::new(),
        }
    }

    /// Compile a highlight query for `language`.
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
        Ok(())
    }

    /// True when a highlight query is compiled and ready.
    pub fn is_available(&self) -> bool {
        self.query.is_some()
    }

    /// Access the compiled query (for callers that call `highlight_ranges` directly).
    pub fn query(&self) -> Option<&Query> {
        self.query.as_ref()
    }

    /// Interned capture names slice.
    pub fn capture_names(&self) -> &[Arc<str>] {
        &self.capture_names
    }

    /// Mutable access to the reusable query cursor.
    pub fn query_cursor_mut(&mut self) -> &mut QueryCursor {
        &mut self.query_cursor
    }

    /// Query `tree`/`rope` for highlights in `byte_range`.
    ///
    /// Returns `None` when no query is compiled. Callers fall back to plain
    /// text styling on `None`.
    pub fn highlight_range(
        &mut self,
        tree: &Tree,
        rope: &Rope,
        byte_range: Range<usize>,
    ) -> Option<Vec<HighlightRange>> {
        let query = self.query.as_ref()?;
        let query_end =
            byte_range.start + (byte_range.end - byte_range.start).min(MAX_BYTES_TO_QUERY_INTERNAL);
        Some(highlight_ranges(
            tree,
            rope,
            query,
            &self.capture_names,
            &mut self.query_cursor,
            byte_range.start..query_end,
        ))
    }
}

impl Default for TreeSitterProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a `Parser` for `language`. Used by the async parse pipeline.
pub(crate) fn build_parser(language: &Language) -> Option<Parser> {
    let mut parser = Parser::new();
    parser.set_language(language).ok()?;
    Some(parser)
}
