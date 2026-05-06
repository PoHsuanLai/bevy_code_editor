//! Keyboard → PTY bytes via `FocusedInput<KeyboardInput>` observer.
//!
//! Mouse interaction (drag-select, scroll) reuses
//! `bevy_text_editor::TextInteractionPlugin` — we get scroll/click for
//! free. Selection extraction + clipboard flow lands in Phase 4.

use alacritty_terminal::event::Notify;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::input_focus::FocusedInput;
use bevy::prelude::*;

use crate::messages::{TerminalCopySelection, TerminalPaste};
use crate::types::{TerminalInputMode, TerminalSession};

/// `FocusedInput<KeyboardInput>` observer: translate the key into a byte
/// sequence and write it to the PTY via the session's `Notifier`.
///
/// Modifier-only events (just Shift / Ctrl etc. with no other key) are
/// dropped — alacritty's input encoder pairs the modifier with a normal
/// key. Only `state == Pressed` produces output for now; held-key repeat
/// is delegated to the OS (most shells handle their own repeat).
pub fn on_focused_terminal_keyboard(
    trigger: On<FocusedInput<KeyboardInput>>,
    sessions: Query<(&TerminalSession, &TerminalInputMode)>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut copy_w: MessageWriter<TerminalCopySelection>,
    mut paste_w: MessageWriter<TerminalPaste>,
) {
    let entity = trigger.event().focused_entity;
    let Ok((session, mode)) = sessions.get(entity) else {
        return;
    };
    let event = &trigger.event().input;
    if event.state != ButtonState::Pressed {
        return;
    }

    let ctrl = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
    let alt = keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight);
    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    let cmd = keyboard.pressed(KeyCode::SuperLeft) || keyboard.pressed(KeyCode::SuperRight);

    // Cmd-shortcuts: copy / paste fire as messages so the snapshot rope
    // is the source of truth. Anything else under Cmd is unhandled (the
    // app can install its own handlers via leafwing).
    //
    // Linux/Windows convention: Ctrl+Shift+C / Ctrl+Shift+V (so plain
    // Ctrl+C still sends SIGINT to the shell). macOS convention: Cmd+C /
    // Cmd+V. Honor both.
    let copy_combo = (cmd && event.key_code == KeyCode::KeyC)
        || (ctrl && shift && event.key_code == KeyCode::KeyC);
    let paste_combo = (cmd && event.key_code == KeyCode::KeyV)
        || (ctrl && shift && event.key_code == KeyCode::KeyV);
    if copy_combo {
        copy_w.write(TerminalCopySelection { entity });
        return;
    }
    if paste_combo {
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if let Ok(text) = clipboard.get_text() {
                paste_w.write(TerminalPaste { entity, text });
            }
        }
        return;
    }
    if cmd {
        return;
    }

    let bytes = encode_key(&event.key_code, &event.logical_key, ctrl, alt, mode);
    if let Some(bytes) = bytes {
        session.notifier.notify(bytes);
    }
}

/// Translate a Bevy key event into the byte sequence the shell expects.
///
/// Returns `None` for keys we don't have an encoding for (modifiers
/// alone, function keys not handled yet, etc.).
fn encode_key(
    code: &KeyCode,
    logical: &Key,
    ctrl: bool,
    alt: bool,
    mode: &TerminalInputMode,
) -> Option<Vec<u8>> {
    use KeyCode as K;

    // Special keys first — these don't surface as printable chars.
    let special = match code {
        K::Enter | K::NumpadEnter => Some(vec![b'\r']),
        K::Backspace => Some(vec![0x7F]),
        K::Tab => Some(vec![b'\t']),
        K::Escape => Some(vec![0x1B]),
        K::Space if !ctrl && !alt => None, // fall through to logical char
        K::Space if ctrl => Some(vec![0x00]),
        K::ArrowUp => Some(arrow(b'A', mode)),
        K::ArrowDown => Some(arrow(b'B', mode)),
        K::ArrowRight => Some(arrow(b'C', mode)),
        K::ArrowLeft => Some(arrow(b'D', mode)),
        K::Home => Some(vec![0x1B, b'[', b'H']),
        K::End => Some(vec![0x1B, b'[', b'F']),
        K::PageUp => Some(vec![0x1B, b'[', b'5', b'~']),
        K::PageDown => Some(vec![0x1B, b'[', b'6', b'~']),
        K::Delete => Some(vec![0x1B, b'[', b'3', b'~']),
        K::Insert => Some(vec![0x1B, b'[', b'2', b'~']),
        K::F1 => Some(vec![0x1B, b'O', b'P']),
        K::F2 => Some(vec![0x1B, b'O', b'Q']),
        K::F3 => Some(vec![0x1B, b'O', b'R']),
        K::F4 => Some(vec![0x1B, b'O', b'S']),
        K::F5 => Some(vec![0x1B, b'[', b'1', b'5', b'~']),
        K::F6 => Some(vec![0x1B, b'[', b'1', b'7', b'~']),
        K::F7 => Some(vec![0x1B, b'[', b'1', b'8', b'~']),
        K::F8 => Some(vec![0x1B, b'[', b'1', b'9', b'~']),
        K::F9 => Some(vec![0x1B, b'[', b'2', b'0', b'~']),
        K::F10 => Some(vec![0x1B, b'[', b'2', b'1', b'~']),
        K::F11 => Some(vec![0x1B, b'[', b'2', b'3', b'~']),
        K::F12 => Some(vec![0x1B, b'[', b'2', b'4', b'~']),
        _ => None,
    };
    if let Some(bytes) = special {
        return Some(bytes);
    }

    // Printable / logical characters.
    let ch = match logical {
        Key::Character(s) => s.chars().next()?,
        Key::Space => ' ',
        _ => return None,
    };

    if ctrl {
        // Translate Ctrl+letter to control byte (Ctrl+A=0x01..Ctrl+Z=0x1A,
        // Ctrl+@=0x00, Ctrl+[=0x1B, Ctrl+\=0x1C, Ctrl+]=0x1D, Ctrl+^=0x1E,
        // Ctrl+_=0x1F).
        let upper = ch.to_ascii_uppercase();
        let byte = match upper {
            '@' => 0x00,
            'A'..='Z' => 0x01 + (upper as u8 - b'A'),
            '[' => 0x1B,
            '\\' => 0x1C,
            ']' => 0x1D,
            '^' => 0x1E,
            '_' => 0x1F,
            ' ' => 0x00,
            _ => return None,
        };
        return Some(if alt {
            vec![0x1B, byte]
        } else {
            vec![byte]
        });
    }

    // Plain character (with optional Alt prefix → ESC).
    let mut buf = if alt { vec![0x1B] } else { Vec::new() };
    let mut tmp = [0u8; 4];
    buf.extend_from_slice(ch.encode_utf8(&mut tmp).as_bytes());
    Some(buf)
}

/// Arrow-key encoding flips between CSI (`\e[A`) and SS3 (`\eOA`) based
/// on whether the application requested cursor-key application mode.
fn arrow(letter: u8, mode: &TerminalInputMode) -> Vec<u8> {
    if mode.cursor_key_application {
        vec![0x1B, b'O', letter]
    } else {
        vec![0x1B, b'[', letter]
    }
}
