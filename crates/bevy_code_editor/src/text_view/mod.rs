//! Editor-side text-view module.
//!
//! The generic primitives (`TextView`, `DisplayLayout`, `ShapedLine`,
//! `StyleRun`, `RectOverlay`, `TextViewState`, `TextViewViewport`,
//! `render_layout`, `TextEnginePlugin`, `TextEnginePlugins`, …) live in
//! [`bevy_text_engine`] and are re-exported here so existing
//! `use bevy_code_editor::text_view::…;` paths keep resolving. Editor-only
//! pieces — input interaction (drag/copy state) and the
//! [`TextInteractionPlugin`] that registers them — live in the
//! [`interaction`] and [`plugin`] submodules.

pub mod interaction;
pub mod plugin;

// Re-exporting submodules (not just symbols) preserves paths like
// `crate::text_view::render::GlyphInstance` for downstream code.
pub use bevy_text_engine::view::{
    layout, line_width, overlay, render, snapshot, state, viewport, DisplayLayout,
    GlyphBatchComponent, GlyphInstance, LineWidthTracker, RectOverlay, RowVertical, ShapedLine,
    SimpleTheme, StyleRun, TextView, TextViewBatch, TextViewBatchEntity, TextViewOverlays,
    TextViewRenderSet, TextViewState, TextViewViewport, ViewportOrigin,
};

pub use bevy_text_engine::view::snapshot::trivial_layout;

pub use interaction::{
    copy_selection, screen_to_char_pos, TextViewDragState, TextViewSelectionState,
};
pub use plugin::TextInteractionPlugin;
