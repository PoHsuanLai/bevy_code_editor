//! Editor-side text-view module.
//!
//! The generic primitives (`TextView`, `DisplayLayout`, `ShapedLine`,
//! `StyleRun`, `RectOverlay`, `TextBuffer`, `ScrollState`, `ContentMetrics`,
//! `TextViewViewport`, `render_layout`, `InstancedTextPlugin`,
//! `InstancedTextPlugins`, …) live in [`bevy_instanced_text`] and are re-exported
//! here so existing `use bevy_code_editor::text_view::…;` paths keep resolving.
//!
//! Interaction (`TextViewDragState`, `ScrollConfig`,
//! `InstancedTextInteractionPlugin`, `screen_to_char_pos`, `copy_selection`)
//! lives in [`bevy_instanced_text_edit`] and is re-exported here so the same
//! `use bevy_code_editor::text_view::…;` paths continue to resolve.
//!
//! Selection state for editor entities lives on `SelectionState` (also in
//! `bevy_instanced_text_edit`). Pre-Phase 30A there was a parallel
//! `TextViewSelectionState` Component for picking-driven selection; both
//! stores have been collapsed into the unified `SelectionState`.

pub use bevy_instanced_text::view::{
    layout, overlay, render, snapshot, state, viewport, ContentMetrics, DisplayLayout,
    GlyphBatchComponent, GlyphInstance, RectOverlay, RowVertical, ScrollState, ShapedLine,
    StyleRun, TextBuffer, TextView, TextViewBatch, TextViewBatchEntity, TextViewOverlays,
    TextViewRenderSet, TextViewViewport, ViewportOrigin,
};

pub use bevy_instanced_text::view::snapshot::trivial_layout;

pub use bevy_instanced_text_edit::{
    copy_selection, screen_to_char_pos, ScrollConfig, InstancedTextInteractionPlugin, TextViewDragState,
};
