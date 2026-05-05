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

use crate::text_view::TextViewState;
use crate::types::{EditHistoryState, EditorDisplayState, SyntaxCacheState};
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
            let _ = &mut syntax;
        }
    }
    if let Some(line_idx) = trigger.event().invalidate_lines_from {
        display.invalidate_lines_from = Some(line_idx);
    }
}

/// Set buffer text and eagerly propagate the edit's side effects to the
/// editor's per-entity caches.
///
/// Hosts call this at startup (before any system runs) to load file
/// contents. The normal observer flow runs at Update time, but a host
/// loading text in `Startup` and immediately rendering wouldn't see syntax
/// highlighting on the first frame without this eager drain.
#[allow(clippy::too_many_arguments)]
pub fn set_text(
    sel: &mut crate::types::SelectionState,
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
    cursor: &mut crate::types::CursorState,
    tv: &mut TextViewState,
    text: &str,
) {
    hist.set_text(sel, cursor, tv, text);
    if let Some(byte_edit) = hist.pending_byte_edit.take() {
        #[cfg(feature = "tree-sitter")]
        {
            syntax.pending_tree_sitter_edit = Some(byte_edit);
        }
        #[cfg(not(feature = "tree-sitter"))]
        {
            let _ = byte_edit;
            let _ = syntax;
        }
    }
    if let Some(line_idx) = hist.invalidate_lines_from.take() {
        display.invalidate_lines_from = Some(line_idx);
    }
}
