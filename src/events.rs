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
