//! Re-exports of cursor / word-boundary helpers from `bevy_text_editor`.
//!
//! The pure rope/cursor functions moved down to the editable-text widget
//! crate. The editor's mouse / handler systems still import from
//! `crate::input::cursor::…` paths.

pub use bevy_text_editor::cursor_movement::{
    find_word_boundary_left, find_word_boundary_right, move_cursor_down, move_cursor_line_end,
    move_cursor_line_start, move_cursor_up, move_cursor_word_left, move_cursor_word_right,
};

// Word-delete helpers re-exported as well — they live in
// `bevy_text_editor::handlers::edit` since they're keyboard-handler-shaped.
pub use bevy_text_editor::handlers::edit::{delete_word_backward, delete_word_forward};
