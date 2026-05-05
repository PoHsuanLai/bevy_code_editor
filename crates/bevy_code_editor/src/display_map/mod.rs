//! Display map — wires the editor's fold / syntax / wrap state into the
//! engine's per-frame layout system via trait Components.
//!
//! The wrap-aware layout walker lives engine-side in
//! [`bevy_text_engine::view::layout_builder`], driven by `produce_layouts`.
//! This module's [`DisplayMapPlugin`] inserts the editor's
//! [`bevy_text_engine::LineFilter`] / [`bevy_text_engine::LineStyleSource`]
//! / [`bevy_text_engine::LayoutWrap`] Components on each `CodeEditor`
//! entity and refreshes their interior state from `FoldState`,
//! `SyntaxResource`, `WrappingSettings`, etc. The engine's system then
//! reads those Components on each layout pass.
//!
//! Cursor/selection systems map buffer positions to display rows via
//! `DisplayLayout::buffer_to_display`, which scans the visible window's
//! rows directly — there is no separate transform-stack abstraction.

pub mod plugin;
pub mod styling;

pub use plugin::{DisplayMapPlugin, DisplayMapSet, LayoutSyncSet};
