//! Input handling for the code editor
//!
//! This module provides keyboard and mouse input handling using
//! leafwing-input-manager for action-based keybindings.

mod keybindings;
mod actions;
mod keyboard;
mod mouse;
mod cursor;

// Re-export public types
pub use keybindings::{EditorAction, default_input_map};
pub use keyboard::handle_keyboard_input;
pub use mouse::{handle_mouse_input, handle_mouse_wheel, MouseDragState};

// Re-export leafwing types for user customization
pub use leafwing_input_manager::prelude::{InputMap, ButtonlikeChord, ActionState, Actionlike};
