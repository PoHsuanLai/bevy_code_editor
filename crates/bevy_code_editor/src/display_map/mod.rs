//! Display map — produces the editor's per-frame `DisplayLayout`.
//!
//! The wrap-aware layout walker now lives engine-side in
//! [`bevy_text_engine::view::layout_builder`]. This module is the editor's
//! adapter on top of it: [`build_display_layout`] resolves the editor's
//! settings (wrapping, indentation, performance budget) and captures the
//! editor's fold state + syntax highlighter into closures, then defers to
//! the engine. Output is the same flat `Vec<ShapedLine>` consumed by the
//! renderer and by overlay producers (cursor, selection, brackets).
//!
//! Cursor/selection systems map buffer positions to display rows via
//! `DisplayLayout::buffer_to_display`, which scans the visible window's
//! rows directly — there is no separate transform-stack abstraction.

pub mod layout;
pub mod plugin;

pub use layout::build_display_layout;
pub use plugin::{DisplayMapPlugin, DisplayMapSet};
