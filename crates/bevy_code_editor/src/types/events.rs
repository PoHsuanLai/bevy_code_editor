//! Editor events for inter-plugin communication.

use bevy::prelude::*;

/// Notifies plugins (syntax highlighting, LSP, etc.) about text changes for
/// incremental updates. Positions are captured at edit-time so consumers
/// don't need the pre-edit rope for tree-sitter style byte-keyed edits.
///
/// `pre_edit_rope` is `Some` when the editor entity has the
/// [`bevy_text_editor::SnapshotPreEdit`] marker (LSP attaches it). LSP
/// incremental sync needs the pre-edit rope to convert byte offsets
/// into LSP positions in the server's negotiated encoding.
#[derive(Message, Clone, Debug, Reflect)]
#[reflect(Clone, Debug)]
pub struct TextEditEvent {
    pub delta: bevy_text_editor::EditDelta,
    pub content_version: u64,
    #[reflect(ignore)]
    pub pre_edit_rope: Option<ropey::Rope>,
}

impl TextEditEvent {
    pub fn new(delta: bevy_text_editor::EditDelta, content_version: u64) -> Self {
        Self {
            delta,
            content_version,
            pre_edit_rope: None,
        }
    }

    pub fn with_pre_edit_rope(mut self, rope: Option<ropey::Rope>) -> Self {
        self.pre_edit_rope = rope;
        self
    }

    pub fn start_byte(&self) -> usize {
        self.delta.start_byte
    }
    pub fn old_end_byte(&self) -> usize {
        self.delta.old_end_byte
    }
    pub fn new_end_byte(&self) -> usize {
        self.delta.new_end_byte
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
