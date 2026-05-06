//! Editor-side observers that react to [`bevy_text_editor::OnEdit`] triggers.
//!
//! The editable-text core lives in `bevy_text_editor`. After every edit op,
//! its `emit_edit_triggers` system fires an [`OnEdit`] event on the entity.
//! Editor-tier consumers (incremental tree-sitter reparse, line-keyed entity
//! invalidation) observe this event and update their per-entity caches.
//!
//! This decouples the cross-crate state propagation: `bevy_text_editor`
//! knows nothing about `SyntaxCacheState` or `EditorDisplayState`; the
//! editor crate adds behavior via observers, never by reading the lower
//! crate's mutable fields.

use crate::types::{EditorDisplayState, events::TextEditEvent};
use bevy::prelude::*;
use bevy_text_editor::OnEdit;
use bevy_text_engine::TextBuffer;

/// Observer: emit [`TextEditEvent`] for downstream consumers (tree-sitter
/// incremental reparse, LSP `did_change`) and forward the line-invalidation
/// index to [`EditorDisplayState`].
pub fn on_edit_invalidate_caches(
    trigger: On<OnEdit>,
    mut q: Query<(&TextBuffer, &mut EditorDisplayState), With<crate::types::CodeEditor>>,
    mut writer: MessageWriter<TextEditEvent>,
) {
    let entity = trigger.event().entity;
    let Ok((buffer, mut display)) = q.get_mut(entity) else {
        return;
    };

    if let Some(byte_edit) = trigger.event().byte_edit {
        writer.write(
            TextEditEvent::new(byte_edit, buffer.content_version)
                .with_pre_edit_rope(trigger.event().pre_edit_rope.clone()),
        );
    }
    if let Some(line_idx) = trigger.event().invalidate_lines_from {
        display.invalidate_lines_from = Some(line_idx);
    }
}

