//! Tree-sitter syntax-highlight provider using the low-level `QueryCursor` API.
//!
//! Modeled on Zed's pattern: parse with a `RopeReader` for zero-copy reads,
//! run highlight queries against a cached tree, and emit `HighlightRange`s
//! whose capture names are `Arc<str>` clones from a per-query intern table.

use crate::highlight::{highlight_ranges, HighlightRange};
use ropey::Rope;
use std::ops::Range;
use std::sync::Arc;
use tree_sitter::{Language, Parser, Query, QueryCursor, Tree};

/// Maximum bytes to query at once (matches Zed's heuristic).
pub const MAX_BYTES_TO_QUERY: usize = 16 * 1024;

/// Buffer-size threshold above which `apply_sync_edit` skips its synchronous
/// `parse_with` call and only does the cheap `tree.edit()` byte-offset shift.
/// Larger buffers let the async `parse_dirty` pipeline handle the reparse
/// off the main thread. ~64 KB covers typical source files; sqlite3.c-scale
/// (~7 MB) defers.
pub const SYNC_REPARSE_BYTE_LIMIT: usize = 64 * 1024;

/// Zero-copy rope reader for `parse_with`. Streams chunks forward; seeking
/// backwards resets the chunk iterator.
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

/// Tree-sitter-based highlight provider. Use [`highlight_range`] to query
/// highlight ranges from a cached tree; heavy parse operations are driven by
/// [`apply_sync_edit`], [`update_tree`], and [`invalidate_tree`].
pub struct TreeSitterProvider {
    query: Option<Query>,
    /// Intern table indexed by `capture.index`; cloning is a refcount bump.
    capture_names: Vec<Arc<str>>,
    pub cached_tree: Option<Tree>,
    pub cached_parser: Option<Parser>,
    /// Kept so async tasks can recreate a parser without the `Language` Component.
    pub cached_language: Option<Language>,
    query_cursor: QueryCursor,
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

    /// Compile a highlight query. Resets cached parser/tree so the next parse
    /// picks up the new grammar.
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

    /// True when a highlight query is compiled and ready.
    pub fn is_available(&self) -> bool {
        self.query.is_some()
    }

    /// Query the cached tree for highlights in `byte_range`.
    ///
    /// Returns `None` when any prerequisite (query, tree, rope) is absent or
    /// the tree's byte offsets are stale after an edit. Callers fall back to
    /// plain text styling on `None`.
    pub fn highlight_range(&mut self, byte_range: Range<usize>) -> Option<Vec<HighlightRange>> {
        let query = self.query.as_ref()?;
        let tree = self.cached_tree.as_ref()?;
        let rope = self.cached_rope.as_ref()?;

        // Tree offsets can lag the rope momentarily after an edit; skip that frame.
        if tree.root_node().end_byte() > rope.len_bytes() {
            return None;
        }

        let query_end =
            byte_range.start + (byte_range.end - byte_range.start).min(MAX_BYTES_TO_QUERY);

        Some(highlight_ranges(
            tree,
            rope,
            query,
            &self.capture_names,
            &mut self.query_cursor,
            byte_range.start..query_end,
        ))
    }

    /// Apply an edit synchronously ("tree interpolation"). `tree.edit()` shifts
    /// byte offsets in O(log n) so highlights stay valid while the async
    /// re-parse runs; also re-parses synchronously for small edits.
    ///
    /// The synchronous reparse is skipped when *either* side of the edit
    /// (old text removed OR new text inserted) exceeds
    /// [`SYNC_REPARSE_BYTE_LIMIT`], or the old tree itself was large.
    /// Without all three checks, a select-all+delete on a 7 MB file looks
    /// like "tiny new buffer" but tree-sitter still does O(old size)
    /// reconciliation work — checking just `rope.len_bytes()` post-edit
    /// misses this case. The async `parse_dirty` pipeline picks up the
    /// edit a frame later; cost of skipping is one frame of slightly stale
    /// highlights, barely perceptible vs. dropping a frame on every edit.
    pub fn apply_sync_edit(&mut self, edit: tree_sitter::InputEdit, rope: &Rope) {
        if let Some(ref mut tree) = self.cached_tree {
            let old_size = tree.root_node().end_byte();
            tree.edit(&edit);

            let removed = edit.old_end_byte.saturating_sub(edit.start_byte);
            let inserted = edit.new_end_byte.saturating_sub(edit.start_byte);
            let small_edit = removed <= SYNC_REPARSE_BYTE_LIMIT
                && inserted <= SYNC_REPARSE_BYTE_LIMIT
                && old_size <= SYNC_REPARSE_BYTE_LIMIT
                && rope.len_bytes() <= SYNC_REPARSE_BYTE_LIMIT;

            if small_edit {
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
        }
        self.cached_rope = Some(rope.clone());
    }

    pub fn tree(&self) -> Option<&Tree> {
        self.cached_tree.as_ref()
    }

    /// Drop cached tree and rope when content shifts would leave byte offsets stale.
    pub fn invalidate_tree(&mut self) {
        self.cached_tree = None;
        self.cached_rope = None;
    }

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
