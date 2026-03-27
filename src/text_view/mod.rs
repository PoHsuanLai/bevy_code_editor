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

pub mod line_width;
pub mod plugin;
pub mod render;
pub mod state;
pub mod viewport;

pub use plugin::{TextView, TextViewBatchEntity, TextViewPlugin, TextViewRenderSet};
pub use render::{GlyphBatchComponent, GlyphInstance, TextViewBatch};
pub use state::TextViewState;
pub use viewport::TextViewViewport;
