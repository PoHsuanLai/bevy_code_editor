//! Keyboard and mouse input handling via leafwing-input-manager.

mod actions;
mod cursor;
mod editor_ops;
mod editing;
mod keybindings;
mod keyboard;
mod mouse;
mod selection_ops;

pub use keybindings::{default_input_map, EditorAction};
pub use keyboard::handle_keyboard_input;
pub use mouse::{handle_mouse_input, handle_mouse_wheel, MouseDragState};

pub use leafwing_input_manager::prelude::{ActionState, Actionlike, ButtonlikeChord, InputMap};
