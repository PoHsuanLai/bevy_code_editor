//! Editor events for inter-plugin communication

use bevy::prelude::*;

/// Notifies plugins (syntax highlighting, LSP, etc.) about text changes for incremental updates
#[derive(Message, Clone, Debug)]
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

/// Fired when user presses Ctrl+Space or types a trigger character
#[derive(Message, Clone, Debug)]
pub struct RequestCompletionEvent {
    pub line: usize,
    pub character: usize,
}

impl RequestCompletionEvent {
    pub fn new(line: usize, character: usize) -> Self {
        Self { line, character }
    }
}

/// Fired when user hovers over a symbol
#[derive(Message, Clone, Debug)]
pub struct RequestHoverEvent {
    pub line: usize,
    pub character: usize,
}

impl RequestHoverEvent {
    pub fn new(line: usize, character: usize) -> Self {
        Self { line, character }
    }
}

/// Fired when user initiates a rename (F2)
#[derive(Message, Clone, Debug)]
pub struct RequestRenameEvent {
    pub line: usize,
    pub character: usize,
}

impl RequestRenameEvent {
    pub fn new(line: usize, character: usize) -> Self {
        Self { line, character }
    }
}

/// Fired when user types '(' or ','
#[derive(Message, Clone, Debug)]
pub struct RequestSignatureHelpEvent {
    pub line: usize,
    pub character: usize,
}

impl RequestSignatureHelpEvent {
    pub fn new(line: usize, character: usize) -> Self {
        Self { line, character }
    }
}

#[derive(Message, Clone, Debug, Default)]
pub struct DismissCompletionEvent;

#[derive(Message, Clone, Debug)]
pub struct ApplyCompletionEvent {
    pub item_index: usize,
}

impl ApplyCompletionEvent {
    pub fn new(item_index: usize) -> Self {
        Self { item_index }
    }
}
