//! Core types for the code editor

pub mod anchor;
pub mod display_map;
pub mod editor;
pub mod events;
pub mod fold;
pub mod history;
pub mod selection;

// Re-export everything so `use crate::types::*` still works
pub use anchor::*;
pub use display_map::*;
pub use editor::*;
pub use fold::*;
pub use history::*;
pub use selection::*;
