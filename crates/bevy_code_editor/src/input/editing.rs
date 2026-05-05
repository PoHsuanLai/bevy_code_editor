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

use crate::types::{EditorDisplayState, SyntaxCacheState};
use bevy::prelude::*;
use bevy_text_editor::OnEdit;

/// Observer: copy the edit's byte range into `SyntaxCacheState` for
/// incremental tree-sitter reparse, and the line-invalidation index into
/// `EditorDisplayState` for line-keyed entity re-spawning.
pub fn on_edit_invalidate_caches(
    trigger: On<OnEdit>,
    mut q: Query<
        (&mut SyntaxCacheState, &mut EditorDisplayState),
        With<crate::types::CodeEditor>,
    >,
) {
    let entity = trigger.event().entity;
    let Ok((mut syntax, mut display)) = q.get_mut(entity) else {
        return;
    };

    if let Some(byte_edit) = trigger.event().byte_edit {
        #[cfg(feature = "tree-sitter")]
        {
            syntax.pending_tree_sitter_edit = Some(byte_edit);
        }
        #[cfg(not(feature = "tree-sitter"))]
        {
            let _ = byte_edit;
        }
        // Forward the pre-edit rope (when SnapshotPreEdit was on the
        // entity) so LSP incremental sync can resolve byte offsets in
        // the negotiated wire encoding without re-decoding the
        // post-edit rope.
        syntax.pending_pre_edit_rope = trigger.event().pre_edit_rope.clone();
    }
    if let Some(line_idx) = trigger.event().invalidate_lines_from {
        display.invalidate_lines_from = Some(line_idx);
    }
}

