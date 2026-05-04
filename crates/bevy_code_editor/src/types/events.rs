//! Editor events for inter-plugin communication.

use bevy::prelude::*;

/// Notifies plugins (syntax highlighting, LSP, etc.) about text changes for
/// incremental updates.
#[derive(Message, Clone, Debug, Reflect)]
#[reflect(Clone, Debug)]
pub struct TextEditEvent {
    pub start_byte: usize,
    pub old_end_byte: usize,
    pub new_end_byte: usize,
    pub content_version: u64,
}

impl TextEditEvent {
    pub fn new(
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
        content_version: u64,
    ) -> Self {
        Self {
            start_byte,
            old_end_byte,
            new_end_byte,
            content_version,
        }
    }
}

// LSP request events.
//
// `cursor_char` is a rope char offset; the listener resolves it to an LSP
// `Position` in the negotiated wire encoding (UTF-16 by spec default). This
// keeps producers honest about non-ASCII content — no inline char-counting
// at the construction site.

/// Fired when user presses Ctrl+Space or types a trigger character.
#[derive(Message, Clone, Debug, Reflect)]
#[reflect(Clone, Debug)]
pub struct RequestCompletionEvent {
    pub cursor_char: usize,
}

impl RequestCompletionEvent {
    pub fn new(cursor_char: usize) -> Self {
        Self { cursor_char }
    }
}

/// Fired when user hovers over a symbol.
#[derive(Message, Clone, Debug, Reflect)]
#[reflect(Clone, Debug)]
pub struct RequestHoverEvent {
    pub cursor_char: usize,
}

impl RequestHoverEvent {
    pub fn new(cursor_char: usize) -> Self {
        Self { cursor_char }
    }
}

/// Fired when user initiates a rename (F2).
#[derive(Message, Clone, Debug, Reflect)]
#[reflect(Clone, Debug)]
pub struct RequestRenameEvent {
    pub cursor_char: usize,
}

impl RequestRenameEvent {
    pub fn new(cursor_char: usize) -> Self {
        Self { cursor_char }
    }
}

/// Fired when user types '(' or ','.
#[derive(Message, Clone, Debug, Reflect)]
#[reflect(Clone, Debug)]
pub struct RequestSignatureHelpEvent {
    pub cursor_char: usize,
}

impl RequestSignatureHelpEvent {
    pub fn new(cursor_char: usize) -> Self {
        Self { cursor_char }
    }
}

#[derive(Message, Clone, Debug, Default, Reflect)]
#[reflect(Clone, Debug, Default)]
pub struct DismissCompletionEvent;

#[derive(Message, Clone, Debug, Reflect)]
#[reflect(Clone, Debug)]
pub struct ApplyCompletionEvent {
    pub item_index: usize,
}

impl ApplyCompletionEvent {
    pub fn new(item_index: usize) -> Self {
        Self { item_index }
    }
}
