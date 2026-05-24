//! Pluggable syntax highlighter for fenced code blocks.
//!
//! v1 ships [`NoHighlight`]; a host can drop in a tree-sitter or syntect
//! adapter by inserting [`MarkdownHighlighter`] as a resource. The
//! adapter lives outside this crate so we don't drag a parser dependency
//! into the markdown viewer.

use std::ops::Range;
use std::sync::Arc;

use bevy::prelude::*;

pub trait CodeHighlighter: Send + Sync + 'static {
    /// Return styled ranges over `code`. Ranges must be byte offsets into
    /// `code`, sorted by start, and may be non-overlapping. Bytes outside
    /// any returned range render with the theme's default text color.
    fn highlight(&self, lang: Option<&str>, code: &str) -> Vec<(Range<usize>, Color)>;
}

pub struct NoHighlight;

impl CodeHighlighter for NoHighlight {
    fn highlight(&self, _lang: Option<&str>, _code: &str) -> Vec<(Range<usize>, Color)> {
        Vec::new()
    }
}

#[derive(Resource, Clone)]
pub struct MarkdownHighlighter(pub Arc<dyn CodeHighlighter>);

impl Default for MarkdownHighlighter {
    fn default() -> Self {
        Self(Arc::new(NoHighlight))
    }
}
