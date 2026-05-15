//! Editor-side text-view module.
//!
//! The generic primitives (`TextView`, `DisplayLayout`, `ShapedLine`,
//! `StyleRun`, `RectOverlay`, `TextBuffer<RopeBuffer>`, `ScrollState`, `ContentMetrics`,
//! `render_layout`, `InstancedTextPlugin`,
//! `InstancedTextPlugins`, …) live in [`bevy_instanced_text`] and are re-exported
//! here so existing `use bevy_code_editor::text_view::…;` paths keep resolving.
//!
//! Interaction (`TextViewDragState`, `ScrollConfig`,
//! `InstancedTextInteractionPlugin`, `screen_to_char_pos`, `copy_selection`)
//! lives in [`bevy_text_editor`] and is re-exported here so the same
//! `use bevy_code_editor::text_view::…;` paths continue to resolve.
//!
//! Selection state for editor entities lives on `SelectionState` (also in
//! `bevy_text_editor`). Pre-Phase 30A there was a parallel
//! `TextViewSelectionState` Component for picking-driven selection; both
//! stores have been collapsed into the unified `SelectionState`.

pub use bevy_instanced_text::view::{
    pipeline, overlay, render, glyph, text, ContentMetrics, DisplayLayout,
    GlyphBatchComponent, HorizontalScroll, RectOverlay, RowVertical, ShapedLine, StyleRun,
    TextBuffer, TextOverlays, TextUnderlays, TextViewBatchEntity, TextViewRenderSet,
    VerticalScroll,
};

pub use bevy_text_editor::{RopeBuffer, 
    copy_selection, screen_to_char_pos, InstancedTextInteractionPlugin, ScrollConfig,
    TextViewDragState,
};
