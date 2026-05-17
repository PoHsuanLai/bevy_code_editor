//! `QueryData` groups for the most-co-queried Component bundles.
//!
//! Editor systems regularly need the same combinations of buffer/cursor
//! state, viewport metrics, and chrome settings. Pulling each as a flat
//! tuple makes systems noisy; these groups let callers write
//! `Query<(EditorBufferView, EditorLayoutView, ...), With<CodeEditor>>`
//! and dot into fields by name.

use bevy::ecs::query::QueryData;
use bevy::text::TextFont;
use bevy::ui::{ComputedNode, ScrollPosition};
use bevy_instanced_text::{DisplayLayout, MonoCellWidth, TextBuffer};
use bevy_instanced_text_editor::RopeBuffer;

use crate::types::FoldState;

/// Buffer + fold state. Every overlay producer that reads text and needs
/// to skip folded lines wants this group. CursorState / SelectionState
/// are taken as `&'static` next to it on the rare system that needs
/// them — most systems only need one or the other.
#[derive(QueryData)]
pub struct EditorBufferView {
    pub buffer: &'static TextBuffer<RopeBuffer>,
    pub fold: &'static FoldState,
}

/// Viewport + scroll + mono-cell metrics + optional display layout. The
/// shaped layout is `Option<&...>` because some systems (gutter setup,
/// scroll animator) run before it exists.
#[derive(QueryData)]
pub struct EditorLayoutView {
    pub computed: &'static ComputedNode,
    pub scroll: &'static ScrollPosition,
    pub mono: &'static MonoCellWidth,
    pub layout: Option<&'static DisplayLayout>,
}

/// Font + line-height — readers that need `resolve_line_height` /
/// font_size together (line numbers, indent guides, syntax sizing).
#[derive(QueryData)]
pub struct EditorFontView {
    pub font: &'static TextFont,
    pub line_height: &'static bevy::text::LineHeight,
}
