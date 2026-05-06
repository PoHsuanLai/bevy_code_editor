//! Editor-side text-view module.
//!
//! The generic primitives (`TextView`, `DisplayLayout`, `ShapedLine`,
//! `StyleRun`, `RectOverlay`, `TextBuffer`, `ScrollState`, `ContentMetrics`,
//! `TextViewViewport`, `render_layout`, `TextEnginePlugin`,
//! `TextEnginePlugins`, …) live in [`bevy_text_engine`] and are re-exported
//! here so existing `use bevy_code_editor::text_view::…;` paths keep resolving.
//!
//! Interaction (`TextViewDragState`, `ScrollConfig`,
//! `TextInteractionPlugin`, `screen_to_char_pos`, `copy_selection`)
//! lives in [`bevy_text_editor`] and is re-exported here so the same
//! `use bevy_code_editor::text_view::…;` paths continue to resolve.
//!
//! Selection state for editor entities lives on `SelectionState` (also in
//! `bevy_text_editor`). Pre-Phase 30A there was a parallel
//! `TextViewSelectionState` Component for picking-driven selection; both
//! stores have been collapsed into the unified `SelectionState`.

pub use bevy_text_engine::view::{
    layout, overlay, render, snapshot, state, viewport, ContentMetrics, DisplayLayout,
    GlyphBatchComponent, GlyphInstance, RectOverlay, RowVertical, ScrollState, ShapedLine,
    StyleRun, TextBuffer, TextView, TextViewBatch, TextViewBatchEntity, TextViewOverlays,
    TextViewRenderSet, TextViewViewport, ViewportOrigin,
};

pub use bevy_text_engine::view::snapshot::trivial_layout;

pub use bevy_text_editor::{
    copy_selection, screen_to_char_pos, ScrollConfig, TextInteractionPlugin, TextViewDragState,
};
