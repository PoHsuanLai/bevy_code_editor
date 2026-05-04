//! Reusable text view module — generic text rendering in a scrollable viewport.
//!
//! This module provides the core abstraction for rendering styled text using the GPU
//! instanced rendering pipeline. It is editor-agnostic: it knows nothing about cursors,
//! selections, syntax highlighting, or keybindings.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use bevy_code_editor::text_view::*;
//!
//! app.add_plugins(TextViewPlugin);
//!
//! // Spawn a text view entity
//! commands.spawn((
//!     TextView,
//!     TextViewState::with_text("Hello, World!"),
//!     TextViewViewport::default(),
//! ));
//! ```

pub mod interaction;
pub mod layout;
pub mod line_width;
pub mod overlay;
pub mod plugin;
pub mod render;
pub mod snapshot;
pub mod state;
pub mod viewport;

pub use interaction::{
    copy_selection, screen_to_char_pos, TextViewDragState, TextViewSelectionState,
};
pub use layout::DisplayLayout;
pub use overlay::{RectOverlay, RowVertical, TextViewOverlays};
pub use plugin::{TextView, TextViewBatchEntity, TextViewPlugin, TextViewRenderSet};
pub use render::{GlyphBatchComponent, GlyphInstance, TextViewBatch};
pub use snapshot::{trivial_layout, ShapedLine, SimpleTheme, StyleRun};
pub use state::TextViewState;
pub use viewport::{TextViewViewport, ViewportOrigin};
