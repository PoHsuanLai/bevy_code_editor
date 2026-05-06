//! Keyboard → wezterm key encoding via `FocusedInput<KeyboardInput>` observer.
//!
//! Bevy gives us logical / physical key plus a modifier snapshot; we
//! translate to [`backend::KeyCode`] + [`backend::KeyModifiers`] and call
//! `Terminal::key_down`. Wezterm handles the mode-aware encoding (cursor-
//! key application, kitty keyboard, CSI-u, alt-screen mouse, …) — so the
//! encoder we used to maintain by hand goes away. Mouse interaction
//! (drag-select, scroll) reuses `bevy_text_editor::TextInteractionPlugin`.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::input_focus::FocusedInput;
use bevy::prelude::*;

use crate::backend;
use crate::messages::{TerminalCopySelection, TerminalPaste};
use crate::types::TerminalSession;

/// `FocusedInput<KeyboardInput>` observer: translate the key + modifiers
/// into a wezterm `key_down` call. The `Terminal`'s writer flushes the
/// encoded bytes through to the PTY.
///
/// Only `state == Pressed` produces output for now; held-key repeat is
/// delegated to the OS (most shells handle their own repeat).
pub fn on_focused_terminal_keyboard(
    trigger: On<FocusedInput<KeyboardInput>>,
    sessions: Query<&TerminalSession>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut copy_w: MessageWriter<TerminalCopySelection>,
    mut paste_w: MessageWriter<TerminalPaste>,
) {
    let entity = trigger.event().focused_entity;
    let Ok(session) = sessions.get(entity) else {
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

    let Some((wezterm_key, mods)) = bevy_to_wezterm(&event.key_code, &event.logical_key, ctrl, alt, shift) else {
        return;
    };

    let mut term = session.terminal.lock();
    let _ = term.key_down(wezterm_key, mods);
}

/// Translate Bevy's `(KeyCode, logical Key)` pair plus modifier flags
/// into a [`backend::KeyCode`] + [`backend::KeyModifiers`] suitable for
/// `Terminal::key_down`. Returns `None` for keys we don't have an
/// encoding for (raw modifier presses, etc.) — the wezterm parser knows
/// how to do the rest.
fn bevy_to_wezterm(
    code: &KeyCode,
    logical: &Key,
    ctrl: bool,
    alt: bool,
    shift: bool,
) -> Option<(backend::KeyCode, backend::KeyModifiers)> {
    use backend::KeyCode as W;
    use KeyCode as K;

    let mut mods = backend::KeyModifiers::NONE;
    if shift {
        mods |= backend::KeyModifiers::SHIFT;
    }
    if alt {
        mods |= backend::KeyModifiers::ALT;
    }
    if ctrl {
        mods |= backend::KeyModifiers::CTRL;
    }

    let key = match code {
        K::Enter | K::NumpadEnter => W::Enter,
        K::Backspace => W::Backspace,
        K::Tab => W::Tab,
        K::Escape => W::Escape,
        K::ArrowUp => W::UpArrow,
        K::ArrowDown => W::DownArrow,
        K::ArrowLeft => W::LeftArrow,
        K::ArrowRight => W::RightArrow,
        K::Home => W::Home,
        K::End => W::End,
        K::PageUp => W::PageUp,
        K::PageDown => W::PageDown,
        K::Delete => W::Delete,
        K::Insert => W::Insert,
        K::F1 => W::Function(1),
        K::F2 => W::Function(2),
        K::F3 => W::Function(3),
        K::F4 => W::Function(4),
        K::F5 => W::Function(5),
        K::F6 => W::Function(6),
        K::F7 => W::Function(7),
        K::F8 => W::Function(8),
        K::F9 => W::Function(9),
        K::F10 => W::Function(10),
        K::F11 => W::Function(11),
        K::F12 => W::Function(12),
        _ => {
            let ch = match logical {
                Key::Character(s) => s.chars().next()?,
                Key::Space => ' ',
                _ => return None,
            };
            W::Char(ch)
        }
    };
    Some((key, mods))
}
