//! Structural highlight types and the `SyntaxProvider` trait.
//!
//! `HighlightRange` exposes byte ranges and capture names — no colors.
//! Capture names are `Arc<str>` so emitting one per token doesn't allocate.

use std::ops::Range;
use std::sync::Arc;

/// One contiguous run of text sharing a single tree-sitter capture.
/// `byte_range` is document-absolute; `capture_name` is the raw query capture
/// (e.g. `"keyword"`, `"function.method"`). Mapping to color is the consumer's job.
#[derive(Clone, Debug)]
pub struct HighlightRange {
    pub byte_range: Range<usize>,
    pub capture_name: Arc<str>,
}

/// Rendering-agnostic highlight backend. Returns capture names, not colors.
pub trait SyntaxProvider: Send + Sync {
    /// Return one `Vec<HighlightRange>` per line in `start_line..end_line`.
    /// `start_byte` is the document-absolute offset for tree-sitter's range
    /// queries. Lines with no captures get an empty vec.
    fn highlight_range(
        &mut self,
        text: &str,
        start_line: usize,
        end_line: usize,
        start_byte: usize,
    ) -> Vec<Vec<HighlightRange>>;

    /// Notify the provider of a text change so it can reuse the previous tree.
    fn notify_edit(&mut self, start_byte: usize, old_end_byte: usize, new_end_byte: usize);

    fn is_available(&self) -> bool;
}
