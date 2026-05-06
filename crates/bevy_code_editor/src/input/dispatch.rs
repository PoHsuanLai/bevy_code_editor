//! `EditorAction` → typed event dispatcher.
//!
//! Polls leafwing's `ActionState`, picks the just-pressed-or-repeating
//! action for the focused editor, and emits the corresponding
//! `*Requested` event. Per-action handler systems consume those events
//! one-by-one elsewhere (basic editing handlers in `bevy_text_editor`,
//! IDE-specific handlers in this crate).
//!
//! The dispatcher orchestrates a small chain of responsibilities:
//!
//! 1. **Rename modal early-return.** When the rename modal is visible the
//!    dispatcher exits — input flows through `crate::input::keyboard`'s
//!    typed-char observer for the modal instead.
//! 2. **Pick the action** from leafwing's `ActionState` (just-pressed or
//!    key-repeat).
//! 3. **Per-feature interceptors** get first crack at consuming the action.
//!    Each lives in its feature module, returns `true` when consumed:
//!    - [`crate::lsp_ui::interceptors::completion_popup_intercept`]: arrow
//!      keys / Enter / Tab / Escape on a visible completion popup.
//!    - [`crate::types::fold::goto_line_intercept`]: Escape on an active
//!      goto-line dialog.
//!    Adding a new popup feature means adding its interceptor module and
//!    one more `if` at this point — no scattered popup state in the
//!    dispatcher itself.
//! 4. **Save / Open** are special transforms: these build host-facing events
//!    with payloads constructed from the editor's current state.
//! 5. **LSP follow-up snapshot** captures pre-edit state so the post-edit
//!    follow-up system can detect what changed and refresh popups / send
//!    `did_change`.
//! 6. **Emit the typed `*Requested` event** via `ActionEventWriters::emit`.

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

    // Programmatic edits (LSP completion application, etc.)
    #[cfg(feature = "lsp")]
    replace_range: MessageWriter<'w, bevy_text_editor::ReplaceRangeRequested>,
}

impl<'w> ActionEventWriters<'w> {
    /// Emit the event corresponding to `action`.
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
#[allow(clippy::too_many_arguments)]
pub fn dispatch_action_events(
    input_focus: Res<InputFocus>,
    mut action_query: Query<
        (&ActionState<EditorAction>, &mut KeyRepeatState),
        With<EditorInputManager>,
    >,
    cursor_settings: Res<CursorSettings>,
    #[cfg(feature = "lsp")] mut pending: ResMut<PendingActionFollowup>,
    #[cfg(feature = "lsp")] mut editor_q: Query<
        (
            &mut CursorState,
            &mut crate::text_view::TextBuffer,
            &mut GotoLineState,
        ),
        With<CodeEditor>,
    >,
    #[cfg(not(feature = "lsp"))] mut editor_q: Query<
        (
            &CursorState,
            &crate::text_view::TextBuffer,
            &mut GotoLineState,
        ),
        With<CodeEditor>,
    >,
    #[cfg(feature = "lsp")] mut lsp_q: Query<
        (
            &bevy_lsp::LspClient,
            Option<&mut bevy_lsp::LspDocument>,
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
    let Ok((action_state, mut key_repeat_state)) = action_query.single_mut() else {
        warn!("No EditorInputManager entity found with ActionState");
        return;
    };

    // Rename modal eats all action input until dismissed (input flows
    // through `crate::input::keyboard` instead).
    #[cfg(feature = "lsp")]
    if let Ok((_, _, _, rename_state)) = lsp_q.get(focused) {
        if rename_state.visible {
            return;
        }
    }
    // (We could route this through `lsp_ui::interceptors::rename_modal_active`,
    // but the inline check is two lines and avoids an extra Query — the
    // single `lsp_q` already carries the rename state.)

    let now = Instant::now();
    let mut action_to_execute: Option<EditorAction> = None;

    for action in ALL_ACTIONS {
        if action_state.just_pressed(&action) {
            action_to_execute = Some(action);
            if action.is_repeatable() {
                key_repeat_state.arm(action, now);
            }
            break;
        }
    }

    // Key repeat on held actions.
    if action_to_execute.is_none() {
        if let Some(current_action) = key_repeat_state.current_action {
            if action_state.pressed(&current_action) {
                if let Some(action) = key_repeat_state.tick(now, &cursor_settings.key_repeat) {
                    action_to_execute = Some(action);
                }
            } else {
                key_repeat_state.release();
            }
        }
    }

    let Some(action) = action_to_execute else {
        return;
    };

    // Feature-owned interceptors get first crack at the action. Each returns
    // `true` if it consumed the action; the dispatcher early-returns and the
    // bevy_text_editor / IDE handlers never see the event.
    #[cfg(feature = "lsp")]
    if let Ok((lsp_client, mut lsp_document, mut completion_state, _)) =
        lsp_q.get_mut(focused)
    {
        if crate::lsp_ui::interceptors::completion_popup_intercept(
            action,
            focused,
            &mut completion_state,
            lsp_client,
            lsp_document.as_deref_mut(),
            &mut editor_q,
            &lsp_settings,
            &mut writers.replace_range,
        ) {
            return;
        }
    }

    if let Ok((_, _, mut goto_line_state)) = editor_q.get_mut(focused) {
        if crate::types::fold::goto_line_intercept(action, &mut goto_line_state) {
            return;
        }
    }

    // Save / Open are special — they carry payloads constructed from the
    // editor's current state.
    match action {
        EditorAction::Save => {
            if let Ok((_cursor, buffer, _)) = editor_q.get(focused) {
                let content: String = buffer.rope.chars().collect();
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

    // Snapshot for the LSP follow-up system before handlers run.
    #[cfg(feature = "lsp")]
    {
        if let Ok((cursor, tv, _)) = editor_q.get(focused) {
            pending.pre_cursor_pos = cursor.cursor_pos;
            pending.pre_content_version = tv.content_version;
        }
        pending.was_delete_backward = matches!(action, EditorAction::DeleteBackward);
        pending.was_horizontal_move = is_horizontal_move(action);
        pending.action_fired = true;
    }

    writers.emit(action);
}
