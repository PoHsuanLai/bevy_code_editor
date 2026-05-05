//! Structural highlight types and the `SyntaxProvider` trait.
//!
//! `HighlightRange` exposes byte ranges and tree-sitter capture names — no
//! colors. Consumers map capture names to colors / fonts / weights / whatever
//! their renderer cares about. A capture name like `"keyword"` or
//! `"function.method"` is stored as `Arc<str>` so emitting one highlight per
//! token doesn't allocate (the same `"keyword"` string is shared across
//! thousands of ranges in a typical file).

use std::ops::Range;
use std::sync::Arc;

/// One contiguous run of source text that shares a single tree-sitter capture.
///
/// `byte_range` is in document bytes (the same numbering tree-sitter uses);
/// `capture_name` is the raw query capture (e.g. `"keyword"`,
/// `"function.method"`, `"string"`). Mapping that to a color or other
/// rendering property is the consumer's job.
#[derive(Clone, Debug)]
pub struct HighlightRange {
    pub byte_range: Range<usize>,
    pub capture_name: Arc<str>,
}

/// Pluggable structural-highlight backend.
///
/// The trait is intentionally rendering-agnostic: it returns capture names,
/// not colors. Consumers (an editor, a code-outline panel, an AI agent
/// reasoning about syntax) decide what to do with those.
pub trait SyntaxProvider: Send + Sync {
    /// Highlight a range of lines and return capture-name runs per line.
    ///
    /// # Arguments
    /// * `text` - The text content to highlight (for the specified line range)
    /// * `start_line` - Starting line index (inclusive)
    /// * `end_line` - Ending line index (exclusive)
    /// * `start_byte` - Starting byte offset in the full document (used by
    ///   tree-sitter's range-restricted queries)
    ///
    /// # Returns
    /// One `Vec<HighlightRange>` per line in `start_line..end_line`. Lines
    /// with no captures get an empty vec.
    fn highlight_range(
        &mut self,
        text: &str,
        start_line: usize,
        end_line: usize,
        start_byte: usize,
    ) -> Vec<Vec<HighlightRange>>;

    /// Notify the provider that the source text changed (incremental parsing).
    ///
    /// Tree-sitter uses this to reuse the previous parse where possible.
    fn notify_edit(&mut self, start_byte: usize, old_end_byte: usize, new_end_byte: usize);

    /// Whether this provider is wired up and ready to highlight.
    fn is_available(&self) -> bool;
}
