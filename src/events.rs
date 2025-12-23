//! Editor events for inter-plugin communication

use bevy::prelude::*;

/// Event fired when text content is edited
///
/// This event is used to notify plugins (syntax highlighting, LSP, etc.)
/// about text changes for incremental updates
#[derive(Message, Clone, Debug)]
pub struct TextEditEvent {
    /// Byte offset where the edit started
    pub start_byte: usize,
    /// Byte offset where the old text ended (before edit)
    pub old_end_byte: usize,
    /// Byte offset where the new text ends (after edit)
    pub new_end_byte: usize,
    /// Content version after this edit
    pub content_version: u64,
}

impl TextEditEvent {
    /// Create a new text edit event
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

/// Event requesting code completion at current cursor position
///
/// This event is typically fired when user presses Ctrl+Space or types a trigger character
#[derive(Message, Clone, Debug)]
pub struct RequestCompletionEvent {
    /// Line number where completion is requested
    pub line: usize,
    /// Character position on the line
    pub character: usize,
}

impl RequestCompletionEvent {
    pub fn new(line: usize, character: usize) -> Self {
        Self { line, character }
    }
}

/// Event requesting hover information at cursor position
///
/// This event is typically fired when user hovers over a symbol
#[derive(Message, Clone, Debug)]
pub struct RequestHoverEvent {
    /// Line number where hover is requested
    pub line: usize,
    /// Character position on the line
    pub character: usize,
}

impl RequestHoverEvent {
    pub fn new(line: usize, character: usize) -> Self {
        Self { line, character }
    }
}

/// Event requesting rename operation
///
/// This event is typically fired when user initiates a rename (F2)
#[derive(Message, Clone, Debug)]
pub struct RequestRenameEvent {
    /// Line number where rename is requested
    pub line: usize,
    /// Character position on the line
    pub character: usize,
}

impl RequestRenameEvent {
    pub fn new(line: usize, character: usize) -> Self {
        Self { line, character }
    }
}

/// Event requesting signature help at cursor position
///
/// This event is typically fired when user types '(' or ','
#[derive(Message, Clone, Debug)]
pub struct RequestSignatureHelpEvent {
    /// Line number where signature help is requested
    pub line: usize,
    /// Character position on the line
    pub character: usize,
}

impl RequestSignatureHelpEvent {
    pub fn new(line: usize, character: usize) -> Self {
        Self { line, character }
    }
}

/// Event fired when completion is dismissed/cancelled
#[derive(Message, Clone, Debug, Default)]
pub struct DismissCompletionEvent;

/// Event fired when a completion item is selected
#[derive(Message, Clone, Debug)]
pub struct ApplyCompletionEvent {
    /// Index of the selected completion item
    pub item_index: usize,
}

impl ApplyCompletionEvent {
    pub fn new(item_index: usize) -> Self {
        Self { item_index }
    }
}
