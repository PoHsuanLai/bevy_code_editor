//! Buffer edit handlers — Insert{Newline,Tab}, Delete{Backward,Forward,
//! Word{Backward,Forward},Line}, Replace, Undo, Redo.
//!
//! All edit handlers fetch the full buffer query (six per-entity components)
//! because mutating the rope cascades into history, selection sync, syntax
//! cache invalidation, and display invalidation.

use crate::input::action_events::*;
use crate::input::actions::{delete_selection, insert_char};
use crate::input::cursor::{delete_word_backward, delete_word_forward};
use crate::settings::IndentationSettings;
use crate::types::*;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;

type EditorBufQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut SelectionState,
        &'static mut EditHistoryState,
        &'static mut SyntaxCacheState,
        &'static mut EditorDisplayState,
        &'static mut CursorState,
        &'static mut crate::text_view::TextViewState,
    ),
    With<CodeEditor>,
>;

pub fn handle_insert_newline(
    mut events: MessageReader<InsertNewlineRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorBufQuery,
    #[cfg(feature = "lsp")] mut lsp_q: Query<
        (
            &bevy_lsp::LspClient,
            Option<&mut bevy_lsp::LspDocument>,
            &mut crate::lsp_ui::state::LspCompletionPopup,
        ),
        With<CodeEditor>,
    >,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut hist, mut syntax, mut display, mut cursor, mut tv)) = q.get_mut(entity)
    else {
        return;
    };

    // LSP completion popup intercepts Enter — apply selected item instead.
    #[cfg(feature = "lsp")]
    {
        if let Ok((lsp_client, mut lsp_document, mut completion_state)) = lsp_q.get_mut(entity) {
            let filtered_count = completion_state.filtered_items().len();
            if completion_state.visible && filtered_count > 0 {
                crate::input::actions::apply_completion(
                    &mut cursor,
                    &mut tv,
                    &mut completion_state,
                );
                crate::input::actions::send_did_change(
                    &tv.rope,
                    lsp_client,
                    lsp_document.as_deref_mut(),
                );
                return;
            }
        }
    }

    insert_char(
        &mut sel,
        &mut hist,
        &mut syntax,
        &mut display,
        &mut cursor,
        &mut tv,
        '\n',
    );
}

pub fn handle_insert_tab(
    mut events: MessageReader<InsertTabRequested>,
    input_focus: Res<InputFocus>,
    indentation: Res<IndentationSettings>,
    mut q: EditorBufQuery,
    #[cfg(feature = "lsp")] mut lsp_q: Query<
        (
            &bevy_lsp::LspClient,
            Option<&mut bevy_lsp::LspDocument>,
            &mut crate::lsp_ui::state::LspCompletionPopup,
        ),
        With<CodeEditor>,
    >,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut hist, mut syntax, mut display, mut cursor, mut tv)) = q.get_mut(entity)
    else {
        return;
    };

    #[cfg(feature = "lsp")]
    {
        if let Ok((lsp_client, mut lsp_document, mut completion_state)) = lsp_q.get_mut(entity) {
            let filtered_count = completion_state.filtered_items().len();
            if completion_state.visible && filtered_count > 0 {
                crate::input::actions::apply_completion(
                    &mut cursor,
                    &mut tv,
                    &mut completion_state,
                );
                crate::input::actions::send_did_change(
                    &tv.rope,
                    lsp_client,
                    lsp_document.as_deref_mut(),
                );
                return;
            }
        }
    }

    for _ in 0..indentation.tab_width {
        insert_char(
            &mut sel,
            &mut hist,
            &mut syntax,
            &mut display,
            &mut cursor,
            &mut tv,
            ' ',
        );
    }
}

pub fn handle_delete_backward(
    mut events: MessageReader<DeleteBackwardRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorBufQuery,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut hist, mut syntax, mut display, mut cursor, mut tv)) = q.get_mut(entity)
    else {
        return;
    };
    if sel.selection_start.is_some() {
        delete_selection(
            &mut sel,
            &mut hist,
            &mut syntax,
            &mut display,
            &mut cursor,
            &mut tv,
        );
    } else {
        hist.delete_backward(&mut sel, &mut syntax, &mut display, &mut cursor, &mut tv);
    }
}

pub fn handle_delete_forward(
    mut events: MessageReader<DeleteForwardRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorBufQuery,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut hist, mut syntax, mut display, mut cursor, mut tv)) = q.get_mut(entity)
    else {
        return;
    };
    if sel.selection_start.is_some() {
        delete_selection(
            &mut sel,
            &mut hist,
            &mut syntax,
            &mut display,
            &mut cursor,
            &mut tv,
        );
    } else {
        hist.delete_forward(&mut sel, &mut syntax, &mut display, &mut cursor, &mut tv);
    }
}

pub fn handle_delete_word_backward(
    mut events: MessageReader<DeleteWordBackwardRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorBufQuery,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut hist, mut syntax, mut display, mut cursor, mut tv)) = q.get_mut(entity)
    else {
        return;
    };
    if sel.selection_start.is_some() {
        delete_selection(
            &mut sel,
            &mut hist,
            &mut syntax,
            &mut display,
            &mut cursor,
            &mut tv,
        );
    } else {
        delete_word_backward(&mut sel, &mut hist, &mut cursor, &mut tv);
    }
}

pub fn handle_delete_word_forward(
    mut events: MessageReader<DeleteWordForwardRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorBufQuery,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut hist, mut syntax, mut display, mut cursor, mut tv)) = q.get_mut(entity)
    else {
        return;
    };
    if sel.selection_start.is_some() {
        delete_selection(
            &mut sel,
            &mut hist,
            &mut syntax,
            &mut display,
            &mut cursor,
            &mut tv,
        );
    } else {
        delete_word_forward(&mut sel, &mut hist, &mut cursor, &mut tv);
    }
}

pub fn handle_delete_line(mut events: MessageReader<DeleteLineRequested>) {
    // TODO: not implemented in the original match either; drain to keep the
    // queue from accumulating.
    events.read().for_each(|_| {});
}

pub fn handle_replace(mut events: MessageReader<ReplaceRequested>) {
    // TODO: replace was a TODO pre-refactor.
    events.read().for_each(|_| {});
}

pub fn handle_undo(
    mut events: MessageReader<UndoRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorBufQuery,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((_sel, mut hist, mut syntax, mut display, mut cursor, mut tv)) = q.get_mut(entity)
    else {
        return;
    };
    let _ = hist.undo(&mut syntax, &mut display, &mut cursor, &mut tv);
}

pub fn handle_redo(
    mut events: MessageReader<RedoRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorBufQuery,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((_sel, mut hist, mut syntax, mut display, mut cursor, mut tv)) = q.get_mut(entity)
    else {
        return;
    };
    let _ = hist.redo(&mut syntax, &mut display, &mut cursor, &mut tv);
}
