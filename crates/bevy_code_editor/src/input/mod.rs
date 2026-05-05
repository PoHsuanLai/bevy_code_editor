//! Keyboard and mouse input handling via leafwing-input-manager.
//!
//! Two layers:
//!   1. [`keybindings::EditorAction`] — the leafwing `Actionlike` enum that
//!      keymaps target. Stays the keymap currency; not consumed directly by
//!      handlers.
//!   2. [`action_events`] — one typed `*Requested` event per `EditorAction`
//!      variant. The [`dispatch::dispatch_action_events`] system fans the
//!      enum out into events; per-action handler systems under [`handlers`]
//!      consume them.

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
pub use keybindings::{default_input_map, EditorAction};
pub use keyboard::on_focused_keyboard;
pub use mouse::{handle_mouse_input, handle_mouse_wheel, MouseDragState};

pub use leafwing_input_manager::prelude::{ActionState, Actionlike, ButtonlikeChord, InputMap};
