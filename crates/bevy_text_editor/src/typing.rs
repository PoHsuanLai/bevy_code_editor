//! Typed-character observer for the focused [`crate::TextEditor`].
//!
//! Runs on `bevy::input_focus::FocusedInput<KeyboardInput>`. When the focused
//! entity carries `TextEditor` and the keystroke is a printable character
//! (no Ctrl / Cmd / Alt held), the character is inserted via
//! [`crate::handlers::edit::insert_char`].
//!
//! Modifier keys (Ctrl+C, Cmd+S, …) skip insertion — they're meant to be
//! handled by the host's editing-event dispatcher (e.g. the code editor's
//! leafwing keymap fan-out).

use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::input_focus::FocusedInput;
use bevy::prelude::*;
use bevy_text_engine::TextViewState;

use crate::handlers::edit::insert_char;
use crate::state::{CursorState, EditHistoryState, SelectionState, TextEditor};

/// `true` when any modifier key is held — used by the typing observer to
/// skip shortcut keystrokes (Ctrl+C, Cmd+S, …) that should reach the
/// editing-event dispatcher unchanged.
fn modifier_held(keyboard: &ButtonInput<KeyCode>) -> bool {
    keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight)
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight)
        || keyboard.pressed(KeyCode::AltLeft)
        || keyboard.pressed(KeyCode::AltRight)
}

/// Observer: insert printable characters typed in the focused text editor.
pub fn on_focused_keyboard_typing(
    trigger: On<FocusedInput<KeyboardInput>>,
    mut q: Query<
        (
            &mut SelectionState,
            &mut EditHistoryState,
            &mut CursorState,
            &mut TextViewState,
        ),
        With<TextEditor>,
    >,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    let entity = trigger.event().focused_entity;
    let Ok((mut sel, mut hist, mut cursor, mut tv)) = q.get_mut(entity) else {
        return;
    };

    let event = &trigger.event().input;
    if !event.state.is_pressed() {
        return;
    }

    if modifier_held(&keyboard) {
        return;
    }

    match &event.logical_key {
        Key::Character(text) => {
            for c in text.chars() {
                if c.is_control() {
                    continue;
                }
                insert_char(&mut sel, &mut hist, &mut cursor, &mut tv, c);
            }
        }
        Key::Space => {
            insert_char(&mut sel, &mut hist, &mut cursor, &mut tv, ' ');
        }
        _ => {}
    }
}
