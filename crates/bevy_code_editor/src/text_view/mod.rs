//! Editor-side text-view module.
//!
//! The generic primitives (`TextView`, `DisplayLayout`, `ShapedLine`,
//! `StyleRun`, `RectOverlay`, `TextViewState`, `TextViewViewport`,
//! `render_layout`, `TextEnginePlugin`, `TextEnginePlugins`, …) live in
//! [`bevy_text_engine`] and are re-exported here so existing
//! `use bevy_code_editor::text_view::…;` paths keep resolving.
//!
//! Interaction (`TextViewSelectionState`, `TextViewDragState`,
//! `ScrollConfig`, `TextInteractionPlugin`, `screen_to_char_pos`,
//! `copy_selection`) lives in [`bevy_text_editor`] and is re-exported
//! here so the same `use bevy_code_editor::text_view::…;` paths continue
//! to resolve after the Phase 6 split.

pub use bevy_text_engine::view::{
    layout, overlay, render, snapshot, state, viewport, DisplayLayout, GlyphBatchComponent,
    GlyphInstance, RectOverlay, RowVertical, ShapedLine, StyleRun, TextView, TextViewBatch,
    TextViewBatchEntity, TextViewOverlays, TextViewRenderSet, TextViewState, TextViewViewport,
    ViewportOrigin,
};

pub use bevy_text_engine::view::snapshot::trivial_layout;

pub use bevy_text_editor::{
    copy_selection, screen_to_char_pos, ScrollConfig, TextInteractionPlugin, TextViewDragState,
    TextViewSelectionState,
};
