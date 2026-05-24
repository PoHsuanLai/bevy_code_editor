//! Keyboard and mouse input handling via leafwing-input-manager.
//!
//! Two layers:
//!   1. [`keybindings::EditorAction`] — the leafwing `Actionlike` enum that
//!      keymaps target. Stays the keymap currency; not consumed directly by
//!      handlers.
//!   2. [`action_events`] — one typed `*Requested` event per `EditorAction`
//!      variant. The [`dispatch::dispatch_action_events`] system fans the
//!      enum out into events. Editing events (cursor movement, selection,
//!      delete / insert / clipboard / undo / redo) are defined in and
//!      handled by [`bevy_instanced_text_editor`]; IDE-only events (multi-cursor,
//!      folding, LSP, goto-line, save / open) are handled by the per-action
//!      handler systems under [`handlers`].

pub mod action_events;
pub mod actions;
pub mod auto_indent;
pub mod dispatch;
pub mod editing;
pub mod handlers;
pub mod keybindings;
pub mod keyboard;
pub mod mouse;
pub mod picking_backend;
pub mod selection_ops;
pub mod word_boundary;

pub(crate) use dispatch::dispatch_action_events;
pub use editing::on_edit_invalidate_caches;
pub use keybindings::{default_input_map, EditorAction};
pub use keyboard::on_focused_keyboard;
pub use mouse::{
    on_alt_click, on_click_past_eol_unfold, on_fold_gutter_press, on_pointer_move_for_gutter_hover,
};
#[cfg(feature = "lsp")]
pub use mouse::{
    on_ctrl_click_goto_definition, on_pointer_move_for_hover, on_pointer_out_for_hover,
    tick_lsp_hover_timer,
};

pub use leafwing_input_manager::prelude::{ActionState, Actionlike, ButtonlikeChord, InputMap};
