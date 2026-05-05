//! `EditorAction` → typed event dispatcher.
//!
//! Replaces the pre-refactor `process_editor_actions` 400-line match. This
//! system polls leafwing's `ActionState`, picks the just-pressed-or-repeating
//! action for the focused editor, and emits the corresponding
//! `super::action_events::*Requested` event. Handler systems consume those
//! events one-by-one elsewhere.
//!
//! Three responsibilities live here that don't fit individual handlers:
//!   - **Key repeat.** The `KeyRepeatState` resource tracks the held action;
//!     when its repeat timer fires we re-emit the same event.
//!   - **LSP completion popup navigation.** When the popup is visible, the
//!     up/down arrows navigate items instead of moving the cursor. The
//!     dispatcher swallows those actions silently — handlers never see them.
//!   - **Save / Open / Rename modal.** `Save` and `Open` reuse the existing
//!     host-facing events from `crate::types::events`. The rename modal eats
//!     all input until dismissed.

use super::action_events::*;
#[cfg(feature = "lsp")]
use super::handlers::lsp_followup::PendingActionFollowup;
use super::keybindings::EditorAction;
use crate::plugin::EditorInputManager;
use crate::settings::CursorSettings;
use crate::types::*;
use bevy::ecs::system::SystemParam;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use std::time::Instant;

const ALL_ACTIONS: [EditorAction; 50] = [
    EditorAction::DeleteBackward,
    EditorAction::DeleteForward,
    EditorAction::DeleteWordBackward,
    EditorAction::DeleteWordForward,
    EditorAction::DeleteLine,
    EditorAction::InsertNewline,
    EditorAction::InsertTab,
    EditorAction::MoveCursorLeft,
    EditorAction::MoveCursorRight,
    EditorAction::MoveCursorUp,
    EditorAction::MoveCursorDown,
    EditorAction::MoveCursorWordLeft,
    EditorAction::MoveCursorWordRight,
    EditorAction::MoveCursorLineStart,
    EditorAction::MoveCursorLineEnd,
    EditorAction::MoveCursorDocumentStart,
    EditorAction::MoveCursorDocumentEnd,
    EditorAction::MoveCursorPageUp,
    EditorAction::MoveCursorPageDown,
    EditorAction::SelectLeft,
    EditorAction::SelectRight,
    EditorAction::SelectUp,
    EditorAction::SelectDown,
    EditorAction::SelectWordLeft,
    EditorAction::SelectWordRight,
    EditorAction::SelectLineStart,
    EditorAction::SelectLineEnd,
    EditorAction::SelectAll,
    EditorAction::ClearSelection,
    EditorAction::Copy,
    EditorAction::Cut,
    EditorAction::Paste,
    EditorAction::Undo,
    EditorAction::Redo,
    EditorAction::Replace,
    EditorAction::GotoLine,
    EditorAction::RequestCompletion,
    EditorAction::GotoDefinition,
    EditorAction::RenameSymbol,
    EditorAction::AddCursorAtNextOccurrence,
    EditorAction::AddCursorAbove,
    EditorAction::AddCursorBelow,
    EditorAction::ClearSecondaryCursors,
    EditorAction::ToggleFold,
    EditorAction::Fold,
    EditorAction::Unfold,
    EditorAction::FoldAll,
    EditorAction::UnfoldAll,
    EditorAction::Save,
    EditorAction::Open,
];

/// All MessageWriters the dispatcher writes into. Bundling into a SystemParam
/// trims the function signature from ~50 individual writer params to one.
#[derive(SystemParam)]
pub struct ActionEventWriters<'w> {
    // Deletion
    delete_backward: MessageWriter<'w, DeleteBackwardRequested>,
    delete_forward: MessageWriter<'w, DeleteForwardRequested>,
    delete_word_backward: MessageWriter<'w, DeleteWordBackwardRequested>,
    delete_word_forward: MessageWriter<'w, DeleteWordForwardRequested>,
    delete_line: MessageWriter<'w, DeleteLineRequested>,

    // Special insertion
    insert_newline: MessageWriter<'w, InsertNewlineRequested>,
    insert_tab: MessageWriter<'w, InsertTabRequested>,

    // Cursor movement
    move_cursor_left: MessageWriter<'w, MoveCursorLeftRequested>,
    move_cursor_right: MessageWriter<'w, MoveCursorRightRequested>,
    move_cursor_up: MessageWriter<'w, MoveCursorUpRequested>,
    move_cursor_down: MessageWriter<'w, MoveCursorDownRequested>,
    move_cursor_word_left: MessageWriter<'w, MoveCursorWordLeftRequested>,
    move_cursor_word_right: MessageWriter<'w, MoveCursorWordRightRequested>,
    move_cursor_line_start: MessageWriter<'w, MoveCursorLineStartRequested>,
    move_cursor_line_end: MessageWriter<'w, MoveCursorLineEndRequested>,
    move_cursor_document_start: MessageWriter<'w, MoveCursorDocumentStartRequested>,
    move_cursor_document_end: MessageWriter<'w, MoveCursorDocumentEndRequested>,
    move_cursor_page_up: MessageWriter<'w, MoveCursorPageUpRequested>,
    move_cursor_page_down: MessageWriter<'w, MoveCursorPageDownRequested>,

    // Selection
    select_left: MessageWriter<'w, SelectLeftRequested>,
    select_right: MessageWriter<'w, SelectRightRequested>,
    select_up: MessageWriter<'w, SelectUpRequested>,
    select_down: MessageWriter<'w, SelectDownRequested>,
    select_word_left: MessageWriter<'w, SelectWordLeftRequested>,
    select_word_right: MessageWriter<'w, SelectWordRightRequested>,
    select_line_start: MessageWriter<'w, SelectLineStartRequested>,
    select_line_end: MessageWriter<'w, SelectLineEndRequested>,
    select_all: MessageWriter<'w, SelectAllRequested>,
    clear_selection: MessageWriter<'w, ClearSelectionRequested>,

    // Clipboard
    copy: MessageWriter<'w, CopyRequested>,
    cut: MessageWriter<'w, CutRequested>,
    paste: MessageWriter<'w, PasteRequested>,

    // Undo/redo
    undo: MessageWriter<'w, UndoRequested>,
    redo: MessageWriter<'w, RedoRequested>,

    // Search / Navigation
    replace: MessageWriter<'w, ReplaceRequested>,
    goto_line: MessageWriter<'w, GotoLineRequested>,

    // LSP
    request_completion: MessageWriter<'w, RequestCompletionRequested>,
    goto_definition: MessageWriter<'w, GotoDefinitionRequested>,
    rename_symbol: MessageWriter<'w, RenameSymbolRequested>,

    // Multi-cursor
    add_cursor_next: MessageWriter<'w, AddCursorAtNextOccurrenceRequested>,
    add_cursor_above: MessageWriter<'w, AddCursorAboveRequested>,
    add_cursor_below: MessageWriter<'w, AddCursorBelowRequested>,
    clear_secondary_cursors: MessageWriter<'w, ClearSecondaryCursorsRequested>,

    // Folding
    toggle_fold: MessageWriter<'w, ToggleFoldRequested>,
    fold: MessageWriter<'w, FoldRequested>,
    unfold: MessageWriter<'w, UnfoldRequested>,
    fold_all: MessageWriter<'w, FoldAllRequested>,
    unfold_all: MessageWriter<'w, UnfoldAllRequested>,

    // File operations — reuse pre-existing host-facing events.
    save: MessageWriter<'w, SaveRequested>,
    open: MessageWriter<'w, OpenRequested>,
}

impl<'w> ActionEventWriters<'w> {
    /// Emit the event corresponding to `action`. The grand fan-out table:
    /// 50 arms, one per `EditorAction` variant. Save/Open need extra
    /// payload-construction context (Save carries the buffer text), so they
    /// are emitted by the dispatcher directly, not through this helper.
    fn emit(&mut self, action: EditorAction) {
        match action {
            EditorAction::DeleteBackward => {
                self.delete_backward.write(DeleteBackwardRequested);
            }
            EditorAction::DeleteForward => {
                self.delete_forward.write(DeleteForwardRequested);
            }
            EditorAction::DeleteWordBackward => {
                self.delete_word_backward.write(DeleteWordBackwardRequested);
            }
            EditorAction::DeleteWordForward => {
                self.delete_word_forward.write(DeleteWordForwardRequested);
            }
            EditorAction::DeleteLine => {
                self.delete_line.write(DeleteLineRequested);
            }
            EditorAction::InsertNewline => {
                self.insert_newline.write(InsertNewlineRequested);
            }
            EditorAction::InsertTab => {
                self.insert_tab.write(InsertTabRequested);
            }
            EditorAction::MoveCursorLeft => {
                self.move_cursor_left.write(MoveCursorLeftRequested);
            }
            EditorAction::MoveCursorRight => {
                self.move_cursor_right.write(MoveCursorRightRequested);
            }
            EditorAction::MoveCursorUp => {
                self.move_cursor_up.write(MoveCursorUpRequested);
            }
            EditorAction::MoveCursorDown => {
                self.move_cursor_down.write(MoveCursorDownRequested);
            }
            EditorAction::MoveCursorWordLeft => {
                self.move_cursor_word_left.write(MoveCursorWordLeftRequested);
            }
            EditorAction::MoveCursorWordRight => {
                self.move_cursor_word_right
                    .write(MoveCursorWordRightRequested);
            }
            EditorAction::MoveCursorLineStart => {
                self.move_cursor_line_start
                    .write(MoveCursorLineStartRequested);
            }
            EditorAction::MoveCursorLineEnd => {
                self.move_cursor_line_end.write(MoveCursorLineEndRequested);
            }
            EditorAction::MoveCursorDocumentStart => {
                self.move_cursor_document_start
                    .write(MoveCursorDocumentStartRequested);
            }
            EditorAction::MoveCursorDocumentEnd => {
                self.move_cursor_document_end
                    .write(MoveCursorDocumentEndRequested);
            }
            EditorAction::MoveCursorPageUp => {
                self.move_cursor_page_up.write(MoveCursorPageUpRequested);
            }
            EditorAction::MoveCursorPageDown => {
                self.move_cursor_page_down.write(MoveCursorPageDownRequested);
            }
            EditorAction::SelectLeft => {
                self.select_left.write(SelectLeftRequested);
            }
            EditorAction::SelectRight => {
                self.select_right.write(SelectRightRequested);
            }
            EditorAction::SelectUp => {
                self.select_up.write(SelectUpRequested);
            }
            EditorAction::SelectDown => {
                self.select_down.write(SelectDownRequested);
            }
            EditorAction::SelectWordLeft => {
                self.select_word_left.write(SelectWordLeftRequested);
            }
            EditorAction::SelectWordRight => {
                self.select_word_right.write(SelectWordRightRequested);
            }
            EditorAction::SelectLineStart => {
                self.select_line_start.write(SelectLineStartRequested);
            }
            EditorAction::SelectLineEnd => {
                self.select_line_end.write(SelectLineEndRequested);
            }
            EditorAction::SelectAll => {
                self.select_all.write(SelectAllRequested);
            }
            EditorAction::ClearSelection => {
                self.clear_selection.write(ClearSelectionRequested);
            }
            EditorAction::Copy => {
                self.copy.write(CopyRequested);
            }
            EditorAction::Cut => {
                self.cut.write(CutRequested);
            }
            EditorAction::Paste => {
                self.paste.write(PasteRequested);
            }
            EditorAction::Undo => {
                self.undo.write(UndoRequested);
            }
            EditorAction::Redo => {
                self.redo.write(RedoRequested);
            }
            EditorAction::Replace => {
                self.replace.write(ReplaceRequested);
            }
            EditorAction::GotoLine => {
                self.goto_line.write(GotoLineRequested);
            }
            EditorAction::RequestCompletion => {
                self.request_completion.write(RequestCompletionRequested);
            }
            EditorAction::GotoDefinition => {
                self.goto_definition.write(GotoDefinitionRequested);
            }
            EditorAction::RenameSymbol => {
                self.rename_symbol.write(RenameSymbolRequested);
            }
            EditorAction::AddCursorAtNextOccurrence => {
                self.add_cursor_next.write(AddCursorAtNextOccurrenceRequested);
            }
            EditorAction::AddCursorAbove => {
                self.add_cursor_above.write(AddCursorAboveRequested);
            }
            EditorAction::AddCursorBelow => {
                self.add_cursor_below.write(AddCursorBelowRequested);
            }
            EditorAction::ClearSecondaryCursors => {
                self.clear_secondary_cursors
                    .write(ClearSecondaryCursorsRequested);
            }
            EditorAction::ToggleFold => {
                self.toggle_fold.write(ToggleFoldRequested);
            }
            EditorAction::Fold => {
                self.fold.write(FoldRequested);
            }
            EditorAction::Unfold => {
                self.unfold.write(UnfoldRequested);
            }
            EditorAction::FoldAll => {
                self.fold_all.write(FoldAllRequested);
            }
            EditorAction::UnfoldAll => {
                self.unfold_all.write(UnfoldAllRequested);
            }
            // Save and Open are emitted directly by the dispatcher because
            // they need editor-state context (the buffer content for Save).
            EditorAction::Save | EditorAction::Open => {}
        }
    }
}

/// Per-action flag: whether this action moves the cursor horizontally.
/// Used by `lsp_followup` to decide if the completion popup should hide.
#[cfg(feature = "lsp")]
fn is_horizontal_move(action: EditorAction) -> bool {
    matches!(
        action,
        EditorAction::MoveCursorLeft
            | EditorAction::MoveCursorRight
            | EditorAction::MoveCursorWordLeft
            | EditorAction::MoveCursorWordRight
    )
}

/// `EditorAction` → typed event dispatcher.
///
/// Polls `ActionState`, finds the just-pressed-or-repeating action, then
/// emits the matching `*Requested` event. The followup snapshot is set
/// in `Res<PendingActionFollowup>` for the LSP follow-up system.
#[allow(clippy::too_many_arguments)]
pub fn dispatch_action_events(
    input_focus: Res<InputFocus>,
    action_query: Query<&ActionState<EditorAction>, With<EditorInputManager>>,
    cursor_settings: Res<CursorSettings>,
    mut key_repeat_state: ResMut<KeyRepeatState>,
    #[cfg(feature = "lsp")] mut pending: ResMut<PendingActionFollowup>,
    editor_q: Query<(&CursorState, &crate::text_view::TextViewState), With<CodeEditor>>,
    #[cfg(feature = "lsp")] mut lsp_q: Query<
        (
            &mut crate::lsp_ui::state::LspCompletionPopup,
            &crate::lsp_ui::state::LspRenamePopup,
        ),
        With<CodeEditor>,
    >,
    #[cfg(feature = "lsp")] lsp_settings: Res<crate::settings::LspSettings>,
    mut writers: ActionEventWriters,
) {
    let Some(focused) = input_focus.get() else {
        return;
    };
    let Ok(action_state) = action_query.single() else {
        warn!("No EditorInputManager entity found with ActionState");
        return;
    };

    // Rename modal eats all action input until dismissed.
    #[cfg(feature = "lsp")]
    if let Ok((_, rename_state)) = lsp_q.get(focused) {
        if rename_state.visible {
            return;
        }
    }

    let now = Instant::now();
    let mut action_to_execute: Option<EditorAction> = None;

    for action in ALL_ACTIONS {
        if action_state.just_pressed(&action) {
            action_to_execute = Some(action);
            if action.is_repeatable() {
                key_repeat_state.current_action = Some(action);
                key_repeat_state.press_start = Some(now);
                key_repeat_state.last_repeat = None;
            }
            break;
        }
    }

    // Key repeat on held actions.
    if action_to_execute.is_none() {
        if let Some(current_action) = key_repeat_state.current_action {
            if action_state.pressed(&current_action) {
                let initial_delay = cursor_settings.key_repeat.initial_delay_ms as f64 / 1000.0;
                let repeat_interval = cursor_settings.key_repeat.repeat_delay_ms as f64 / 1000.0;
                if let Some(press_start) = key_repeat_state.press_start {
                    let elapsed = now.duration_since(press_start).as_secs_f64();
                    if elapsed >= initial_delay {
                        let should_repeat = match key_repeat_state.last_repeat {
                            Some(last) => {
                                now.duration_since(last).as_secs_f64() >= repeat_interval
                            }
                            None => true,
                        };
                        if should_repeat {
                            action_to_execute = Some(current_action);
                            key_repeat_state.last_repeat = Some(now);
                        }
                    }
                }
            } else {
                key_repeat_state.current_action = None;
                key_repeat_state.press_start = None;
                key_repeat_state.last_repeat = None;
            }
        }
    }

    let Some(action) = action_to_execute else {
        return;
    };

    // LSP completion popup intercepts up/down arrow navigation. Enter / Tab
    // / Escape pass through to the corresponding handlers, which check the
    // popup themselves before applying the buffer-edit behavior.
    #[cfg(feature = "lsp")]
    if let Ok((mut completion_state, _)) = lsp_q.get_mut(focused) {
        let filtered_count = completion_state.filtered_items().len();
        let max_visible = lsp_settings.completion.max_items;
        if completion_state.visible && filtered_count > 0 {
            match action {
                EditorAction::MoveCursorUp => {
                    if completion_state.selected_index > 0 {
                        completion_state.selected_index -= 1;
                    } else {
                        completion_state.selected_index = filtered_count.saturating_sub(1);
                    }
                    completion_state.ensure_selected_visible_with_max(max_visible);
                    return;
                }
                EditorAction::MoveCursorDown => {
                    if completion_state.selected_index + 1 < filtered_count {
                        completion_state.selected_index += 1;
                    } else {
                        completion_state.selected_index = 0;
                    }
                    completion_state.ensure_selected_visible_with_max(max_visible);
                    return;
                }
                _ => {}
            }
        }
    }

    // Save / Open are special — they carry payloads constructed from the
    // editor's current state.
    match action {
        EditorAction::Save => {
            if let Ok((_cursor, tv)) = editor_q.get(focused) {
                let content: String = tv.rope.chars().collect();
                writers.save.write(SaveRequested { content });
            }
            return;
        }
        EditorAction::Open => {
            writers.open.write(OpenRequested);
            return;
        }
        _ => {}
    }

    // Snapshot for the LSP follow-up system before handlers run. The
    // followup runs only when LSP is enabled; without it the snapshot is a
    // no-op.
    #[cfg(feature = "lsp")]
    {
        if let Ok((cursor, tv)) = editor_q.get(focused) {
            pending.pre_cursor_pos = cursor.cursor_pos;
            pending.pre_content_version = tv.content_version;
        }
        pending.was_delete_backward = matches!(action, EditorAction::DeleteBackward);
        pending.was_horizontal_move = is_horizontal_move(action);
        pending.action_fired = true;
    }

    writers.emit(action);
}
