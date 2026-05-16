//! Per-action handler systems. Each system reads one
//! [`crate::editing_events`]`::*Requested` event and applies it to the
//! focused [`crate::TextEditor`] entity.

pub mod clipboard;
pub mod cursor_move;
pub mod text_input;
pub mod selection;
