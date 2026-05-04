//! Reusable text view module — generic text rendering in a scrollable viewport.
//!
//! Most types (`DisplayLayout`, `ShapedLine`, `StyleRun`, `RectOverlay`,
//! `TextViewState`, `TextViewViewport`, `render_layout`, …) live in
//! [`bevy_text_engine`] and are re-exported here so existing
//! `use bevy_code_editor::text_view::…;` paths keep working through the
//! workspace split. Editor-specific bits (`interaction`, `plugin`) remain
//! in this crate; subsequent phases will move `interaction` to a
//! `bevy_text_interaction` peer crate and slim `plugin` into a thin editor
//! adapter.

pub mod interaction;
pub mod plugin;

// Re-export the engine view layer so the rest of the editor crate keeps
// using `crate::text_view::…`. Re-exporting submodules (not just symbols)
// preserves paths like `crate::text_view::render::GlyphInstance`.
pub use bevy_text_engine::view::{
    layout, line_width, overlay, render, snapshot, state, viewport, DisplayLayout,
    GlyphBatchComponent, GlyphInstance, LineWidthTracker, RectOverlay, RowVertical, ShapedLine,
    SimpleTheme, StyleRun, TextViewBatch, TextViewOverlays, TextViewState, TextViewViewport,
    ViewportOrigin,
};

pub use bevy_text_engine::view::snapshot::trivial_layout;

pub use interaction::{
    copy_selection, screen_to_char_pos, TextViewDragState, TextViewSelectionState,
};
pub use plugin::{TextView, TextViewBatchEntity, TextViewPlugin, TextViewRenderSet};
