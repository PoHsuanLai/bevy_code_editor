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
//!      handled by [`bevy_text_editor`]; IDE-only events (multi-cursor,
//!      folding, LSP, goto-line, save / open) are handled by the per-action
//!      handler systems under [`handlers`].

pub mod action_events;
pub mod actions;
pub mod cursor;
pub mod dispatch;
pub mod editing;
pub mod editor_ops;
pub mod handlers;
pub mod keybindings;
pub mod keyboard;
pub mod mouse;
pub mod selection_ops;

pub use dispatch::dispatch_action_events;
pub use editing::drain_edit_side_effects;
pub use keybindings::{default_input_map, EditorAction};
pub use keyboard::on_focused_keyboard;
pub use mouse::{handle_mouse_input, handle_mouse_wheel};

pub use leafwing_input_manager::prelude::{ActionState, Actionlike, ButtonlikeChord, InputMap};
