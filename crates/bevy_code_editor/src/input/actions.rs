//! Buffer-edit primitives shared by handler systems and the on-focus
//! keyboard observer.
//!
//! Pre-refactor this file owned a 400-line `execute_action_core` match plus
//! a wrapper that handled LSP completion popup interception. After the
//! event-dispatch refactor, the action match is gone — its body lives in
//! per-action handler systems under `super::handlers`. What remains here
//! are the small helpers each handler reuses (insert_char, delete_selection,
//! bracket-skip predicates, LSP completion helpers) plus the LSP follow-up
//! glue called from `super::handlers::lsp_followup`.

use crate::text_view::TextViewState;
#[cfg(feature = "lsp")]
use crate::settings::LspSettings;
use crate::types::*;
use ropey::Rope;

#[cfg(feature = "lsp")]
use crate::lsp_ui::state::LspCompletionPopup;
#[cfg(feature = "lsp")]
use bevy::log::trace;
#[cfg(feature = "lsp")]
use bevy_lsp::{LspClient, LspDocument, LspMessage};

/// Bundled refs to the four LSP pieces that previously co-traveled through
/// `execute_action`: settings, transport client, completion popup state, and
/// the per-editor `LspDocument` (URI / version). Retained here for the
/// keyboard observer that still passes them down its `insert_typed_char`
/// helper. `document` is `Option` because a freshly spawned editor may not
/// have an `LspDocument` inserted yet.
#[cfg(feature = "lsp")]
pub struct LspBuf<'a> {
    pub settings: &'a LspSettings,
    pub client: &'a LspClient,
    pub completion: &'a mut LspCompletionPopup,
    pub document: Option<&'a mut LspDocument>,
}

/// Insert a character at cursor position
pub fn insert_char(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
    c: char,
) {
    if sel.selection_start.is_some() && sel.selection_end.is_some() {
        delete_selection(sel, hist, syntax, display, cursor, tv);
    }
    hist.insert_char(sel, syntax, display, cursor, tv, c);
}

/// Insert a closing bracket / quote without moving the cursor (auto-close).
pub fn insert_closing_char(cursor: &CursorState, tv: &mut TextViewState, c: char) {
    let cursor_pos = cursor.cursor_pos.min(tv.rope.len_chars());
    tv.rope.insert_char(cursor_pos, c);
    tv.content_version += 1;
}

/// Get the closing bracket for an opening bracket
pub fn get_closing_bracket(open: char, pairs: &[(char, char)]) -> Option<char> {
    pairs.iter().find(|(o, _)| *o == open).map(|(_, c)| *c)
}

/// Get the matching quote character (quotes are self-closing)
pub fn get_closing_quote(c: char) -> Option<char> {
    match c {
        '"' | '\'' | '`' => Some(c),
        _ => None,
    }
}

/// Skip auto-close when the cursor already has the closing char in front of
/// it — typing the close key just steps over it.
pub fn should_skip_auto_close(cursor: &CursorState, rope: &Rope, closing: char) -> bool {
    let cursor_pos = cursor.cursor_pos;
    if cursor_pos >= rope.len_chars() {
        return false;
    }
    rope.char(cursor_pos) == closing
}

/// Delete selected text (with undo recording).
pub fn delete_selection(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
) {
    delete_selection_with_history(sel, hist, syntax, display, cursor, tv, true);
}

fn delete_selection_with_history(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    _syntax: &mut SyntaxCacheState,
    _display: &mut EditorDisplayState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
    record_history: bool,
) {
    if let (Some(start), Some(end)) = (sel.selection_start, sel.selection_end) {
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };

        let cursor_before = cursor.cursor_pos;

        let deleted_text: String = tv.rope.slice(start..end).chars().collect();

        let start_byte = tv.rope.char_to_byte(start);
        let end_byte = tv.rope.char_to_byte(end);

        tv.rope.remove(start_byte..end_byte);

        cursor.cursor_pos = start;

        if record_history && !deleted_text.is_empty() {
            hist.history.record(EditOperation {
                removed_text: deleted_text,
                inserted_text: String::new(),
                position: start,
                cursor_before,
                cursor_after: start,
                kind: EditKind::Other,
            });
        }

        sel.selection_start = None;
        sel.selection_end = None;

        tv.content_version += 1;
    }
}

/// Apply selected completion item.
#[cfg(feature = "lsp")]
pub fn apply_completion(
    cursor: &mut CursorState,
    tv: &mut TextViewState,
    completion_state: &mut LspCompletionPopup,
) {
    let filtered = completion_state.filtered_items();
    if let Some(item) = filtered.get(completion_state.selected_index) {
        let start = completion_state.start_char_index;
        let end = cursor.cursor_pos;
        let insert_text = item.insert_text().to_string();

        if start <= end && end <= tv.rope.len_chars() {
            let start_byte = tv.rope.char_to_byte(start);
            let end_byte = tv.rope.char_to_byte(end);

            tv.rope.remove(start_byte..end_byte);
            tv.rope.insert(start, &insert_text);

            cursor.cursor_pos = start + insert_text.chars().count();
            tv.content_version += 1;
        }
    }
    completion_state.visible = false;
    completion_state.filter.clear();
    completion_state.scroll_offset = 0;
}

/// Find the start of the current word (for auto-triggering completion).
#[cfg(feature = "lsp")]
pub fn find_word_start(rope: &ropey::Rope, cursor_pos: usize) -> usize {
    if cursor_pos == 0 {
        return 0;
    }

    let mut pos = cursor_pos;
    while pos > 0 {
        let prev_char = rope.char(pos - 1);
        if prev_char.is_alphanumeric() || prev_char == '_' {
            pos -= 1;
        } else {
            break;
        }
    }
    pos
}

/// Update the completion filter based on text typed since `start_char_index`.
#[cfg(feature = "lsp")]
pub fn update_completion_filter(
    cursor: &CursorState,
    rope: &Rope,
    completion_state: &mut LspCompletionPopup,
) {
    let cursor_pos = cursor.cursor_pos.min(rope.len_chars());
    let start = completion_state.start_char_index;

    if cursor_pos > start && start <= rope.len_chars() {
        let filter_text: String = rope.slice(start..cursor_pos).chars().collect();
        completion_state.filter = filter_text;
        completion_state.selected_index = 0;
        completion_state.scroll_offset = 0;

        trace!("[LSP] Filter updated: '{}'", completion_state.filter);
    } else {
        completion_state.filter.clear();
        completion_state.scroll_offset = 0;
    }
}

/// Request completion from LSP.
#[cfg(feature = "lsp")]
pub fn request_completion(
    cursor: &CursorState,
    rope: &Rope,
    lsp_client: &LspClient,
    completion_state: &mut LspCompletionPopup,
    lsp_document: Option<&LspDocument>,
) {
    let cursor_pos = cursor.cursor_pos.min(rope.len_chars());
    let lsp_position =
        bevy_lsp::rope_char_to_lsp_position(rope, cursor_pos, bevy_lsp::PositionEncoding::Utf16);

    if let Some(doc) = lsp_document {
        trace!(
            "[LSP] Requesting completion at line={}, char={}, visible={}, start_idx={}",
            lsp_position.line,
            lsp_position.character,
            completion_state.visible,
            completion_state.start_char_index
        );

        lsp_client.send(LspMessage::Completion {
            uri: doc.uri.clone(),
            position: lsp_position,
        });

        if !completion_state.visible {
            completion_state.start_char_index = cursor_pos;
            completion_state.items.clear();
            completion_state.selected_index = 0;
            completion_state.filter.clear();
        }

        completion_state.update_word_completions(rope, cursor_pos);
        completion_state.visible = true;
    } else {
        if !completion_state.visible {
            completion_state.start_char_index = cursor_pos;
            completion_state.items.clear();
            completion_state.selected_index = 0;
            completion_state.filter.clear();
        }

        completion_state.update_word_completions(rope, cursor_pos);
        completion_state.visible = true;

        trace!(
            "[bevy_code_editor] No LSP document URI - using word completions only ({} words)",
            completion_state.word_items.len()
        );
    }
}

/// Send `textDocument/didChange` notification to LSP.
#[cfg(feature = "lsp")]
pub fn send_did_change(rope: &Rope, lsp_client: &LspClient, lsp_document: Option<&mut LspDocument>) {
    let Some(doc) = lsp_document else {
        return;
    };
    let version = doc.bump_version();

    let change = lsp_types::TextDocumentContentChangeEvent {
        range: None,
        range_length: None,
        text: rope.chunks().collect(),
    };

    lsp_client.send(LspMessage::DidChange {
        uri: doc.uri.clone(),
        version,
        changes: vec![change],
    });

    trace!("[LSP] DidChange sent, version={}", version);
}

