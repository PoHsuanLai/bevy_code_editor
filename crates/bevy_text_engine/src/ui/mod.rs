//! Reusable UI widgets that consume the engine's view primitives.
//!
//! Currently: a generic scrollbar driven by [`crate::view::TextViewState`]
//! or any [`ScrollState`]. Lives here (not in `bevy_text_editor`) because
//! it's a pure viewport-scroll widget — it has no edit/cursor/input
//! semantics and the terminal needs it just as much as the editor does.

pub mod scrollbar;

pub use scrollbar::*;
