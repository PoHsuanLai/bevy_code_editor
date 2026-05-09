//! Per-action handler systems for IDE-specific actions.
//!
//! The basic editing handlers (cursor movement, selection, delete / insert,
//! clipboard, undo / redo) live in [`bevy_instanced_text_edit::handlers`] and are
//! registered by [`bevy_instanced_text_edit::InstancedTextEditPlugin`]. The handlers in
//! this module cover IDE-only concerns: multi-cursor, folding, the
//! goto-line dialog, and LSP request handlers.
//!
//! `LspFollowup` runs after every Update frame to mirror the post-action
//! behavior the legacy `execute_action` had: send `did_change` after edits
//! and hide / refilter the completion popup on cursor moves.

pub mod file;
pub mod folding;
#[cfg(feature = "lsp")]
pub mod lsp;
pub mod multi_cursor;

#[cfg(feature = "lsp")]
pub mod lsp_followup;

/// Helper: resolve the focused editor entity. Returns `None` when the
/// editor isn't focused or when the focused entity isn't a `CodeEditor`.
///
/// All handler systems early-return on `None`; they never act on a
/// non-focused editor.
#[macro_export]
macro_rules! editor_focused_entity {
    ($input_focus:expr) => {
        match $input_focus.get() {
            Some(e) => e,
            None => return,
        }
    };
}
