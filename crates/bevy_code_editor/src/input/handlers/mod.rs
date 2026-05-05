//! Per-action handler systems. Each system reads one
//! `super::action_events::*Requested` event and applies it to the focused
//! editor entity.
//!
//! The handlers are split by concern (cursor / selection / edit / clipboard /
//! multi-cursor / folding / file / lsp). They all share the same shape:
//! resolve the focused entity via `Res<InputFocus>`, fetch the editor's
//! per-entity components, mutate them, done. They never know about
//! `EditorAction` and never know about key repeat — those are the dispatch
//! system's concern.
//!
//! `LspFollowup` runs after every Update frame to mirror the post-action
//! behavior the old `execute_action` had: send `did_change` after edits and
//! hide / refilter the completion popup on cursor moves.

pub mod clipboard;
pub mod cursor_move;
pub mod edit;
pub mod file;
pub mod folding;
#[cfg(feature = "lsp")]
pub mod lsp;
pub mod multi_cursor;
pub mod selection;

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
