//! Editor-only selection helpers that aren't part of the editable-text core.
//!
//! Most `SelectionState` impl methods (primary-cursor sync, multi-cursor
//! add / remove, range queries) live in [`bevy_instanced_text_edit::edit`] and are
//! available on the re-exported `SelectionState` Component. Anything truly
//! editor-specific would go here; today that file is empty placeholder.
