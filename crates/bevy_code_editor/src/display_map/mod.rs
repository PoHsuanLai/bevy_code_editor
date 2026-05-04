//! Display map — produces the editor's per-frame `DisplayLayout`.
//!
//! `build_display_layout` walks the visible window of the rope, applies
//! folding inline, shapes each row through cosmic-text, and (when soft
//! wrap is enabled) splits long lines into multiple rows on pixel-budget
//! boundaries. The output is a flat `Vec<ShapedLine>` consumed by the
//! renderer and by overlay producers (cursor, selection, brackets).
//!
//! There is no separate transform-stack abstraction (`FoldMap` /
//! `WrapMap` / `TabMap`) — that scaffolding was removed once the
//! producer landed in its current shape. Cursor/selection systems map
//! buffer positions to display rows via `DisplayLayout::buffer_to_display`,
//! which scans the visible window's rows directly.

pub mod layout;
pub mod plugin;

pub use layout::build_display_layout;
pub use plugin::{DisplayMapPlugin, DisplayMapSet};
