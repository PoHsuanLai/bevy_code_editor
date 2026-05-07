//! Per-entity components for a markdown viewer.

use bevy::prelude::*;
use std::ops::Range;
use std::sync::Arc;

/// Source markdown text. The plugin watches `Changed<MarkdownDoc>` and
/// rebuilds the entity's `DisplayLayout` on edit. Spawn alongside the
/// usual text-engine bundle (`TextView`, `TextBuffer`, `ScrollState`,
/// `ContentMetrics`, `FontConfig`, `TextViewViewport`) — the plugin fills the rest.
/// `MarkdownTheme` cascades automatically (defaults to dark); override it
/// at spawn for per-viewer styling.
#[derive(Component, Clone, Debug)]
#[require(crate::theme::MarkdownTheme)]
pub struct MarkdownDoc {
    pub source: String,
}

impl MarkdownDoc {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }
}

impl Default for MarkdownDoc {
    fn default() -> Self {
        Self {
            source: String::new(),
        }
    }
}

/// Side-channel mapping link byte ranges back to their URL. The renderer
/// doesn't read this — it's exposed for click-handler systems that want
/// to dispatch on `(display_row, byte_range_within_row)` from a hit test.
///
/// Keys index into the `DisplayLayout`'s rendered text after layout +
/// soft-wrap; resolving back to source markdown is the host's job.
#[derive(Component, Clone, Default, Debug)]
pub struct MarkdownLinks {
    pub spans: Vec<LinkSpan>,
}

#[derive(Clone, Debug)]
pub struct LinkSpan {
    /// `display_row` in the entity's `DisplayLayout`.
    pub display_row: u32,
    /// Byte range within that row's `text`.
    pub byte_range: Range<usize>,
    pub url: Arc<str>,
}
