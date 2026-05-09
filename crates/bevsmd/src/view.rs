//! Per-entity components for a markdown viewer.

use bevy::prelude::*;
use bevy::text::Font;
use std::ops::Range;
use std::sync::Arc;

/// Source markdown text. The plugin rebuilds the layout whenever this changes.
/// Spawn as part of [`crate::plugin::MarkdownViewerBundle`] — the plugin fills the rest.
/// [`crate::theme::MarkdownTheme`] cascades automatically (defaults to dark).
#[derive(Component, Clone, Debug, Default)]
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

/// Optional code font for inline code and fenced code blocks. Add this
/// component alongside [`MarkdownDoc`] to use a different font for code runs.
/// When absent, code renders with the body font from [`crate::plugin::MarkdownFont`].
#[derive(Component, Clone, Debug)]
pub struct MarkdownCodeFont(pub Handle<Font>);

/// All links in the rendered document. Updated by the plugin whenever the
/// layout rebuilds. Query this to handle link clicks — the plugin does not
/// navigate links.
///
/// Each span carries the display row, byte range within that row, and the URL.
#[derive(Component, Clone, Default, Debug)]
pub struct MarkdownLinks {
    pub spans: Vec<LinkSpan>,
}

#[derive(Clone, Debug)]
pub struct LinkSpan {
    pub display_row: u32,
    pub byte_range: Range<usize>,
    pub url: Arc<str>,
}
