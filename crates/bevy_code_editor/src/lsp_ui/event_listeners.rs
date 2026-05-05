//! LSP event listener systems.
//!
//! These systems read editor events ([`crate::types::events::*`]) and translate
//! them into LSP request sends through each editor entity's
//! [`bevy_lsp::LspClient`] Component.
//!
//! Position conversion goes through [`bevy_lsp::rope_char_to_lsp_position`]
//! with [`PositionEncoding::Utf16`] (LSP spec default). Once we read the
//! server's negotiated `position_encoding` from `initialize.serverInfo.
//! capabilities` we'll thread that through here instead.

use super::snippet;
use super::state::{
    LspCodeActionsPopup, LspCompletionPopup, LspDebounceTimers, LspRenamePopup,
    LspSignatureHelpPopup, PendingLspRequest, SessionTabstop, TabstopSession,
    UnifiedCompletionItem,
};
use crate::settings::LspSettings;
use crate::text_view::TextViewState;
use crate::types::events::{
    ApplyCompletionEvent, DismissCompletionEvent, RequestCompletionEvent, RequestHoverEvent,
    RequestRenameEvent, RequestSignatureHelpEvent, TextEditEvent,
};
use crate::types::{CodeEditor, CursorState};
use bevy::prelude::*;
use bevy_lsp::{
    rope_byte_to_lsp_position, rope_char_to_lsp_position, LspClient, LspDocument, LspMessage,
};
use lsp_types::{Range, TextDocumentContentChangeEvent};

/// Listen for text edits and send `textDocument/didChange` to the server.
///
/// Sends incremental change events when each edit carries a pre-edit rope
/// snapshot (the editor entity has [`bevy_text_editor::SnapshotPreEdit`]).
/// Falls back to full-document sync when the snapshot is missing or when
/// `LspSettings::full_document_sync` is set — the spec guarantees full-doc
/// is always valid.
pub fn listen_text_edit_events(
    mut events: MessageReader<TextEditEvent>,
    mut query: Query<
        (
            &TextViewState,
            &LspClient,
            Option<&mut LspDocument>,
            &bevy_lsp::ServerCapabilities,
        ),
        With<CodeEditor>,
    >,
    settings: Res<LspSettings>,
) {
    let Ok((tv, lsp_client, lsp_document, caps)) = query.single_mut() else {
        return;
    };
    let Some(mut lsp_document) = lsp_document else {
        return;
    };

    let enc = caps.position_encoding();
    let collected: Vec<TextEditEvent> = events.read().cloned().collect();
    if collected.is_empty() {
        return;
    }

    // Build incremental change events when every event has a pre-edit rope
    // and full-doc-sync override is off. Otherwise fall back to a single
    // full-document sync.
    let can_incremental = !settings.full_document_sync
        && collected.iter().all(|e| e.pre_edit_rope.is_some());

    let uri = lsp_document.uri.clone();
    let version = lsp_document.bump_version();

    if can_incremental {
        let mut changes: Vec<TextDocumentContentChangeEvent> = Vec::with_capacity(collected.len());
        for event in &collected {
            let pre = event.pre_edit_rope.as_ref().expect("checked above");
            let delta = &event.delta;
            let start = rope_byte_to_lsp_position(pre, delta.start_byte, enc);
            let end = rope_byte_to_lsp_position(pre, delta.old_end_byte, enc);
            // The new text is the slice of the *post-edit* rope from
            // `start_byte` to `new_end_byte`. We use the current rope here
            // — for a single edit per frame this is correct; for batched
            // edits in one frame, callers should be aware that only the
            // last event's `tv.rope` matches `new_end_byte`. Today the
            // editor produces one edit per frame, so this is fine.
            let new_text = if delta.start_byte == delta.new_end_byte {
                String::new()
            } else {
                let new_start_char = tv.rope.byte_to_char(delta.start_byte);
                let new_end_char = tv.rope.byte_to_char(delta.new_end_byte);
                tv.rope.slice(new_start_char..new_end_char).chars().collect()
            };
            changes.push(TextDocumentContentChangeEvent {
                range: Some(Range { start, end }),
                range_length: None,
                text: new_text,
            });
        }
        lsp_client.send(LspMessage::DidChange { uri, version, changes });
    } else {
        let text: String = tv.rope.chunks().collect();
        lsp_client.send(LspMessage::DidChange {
            uri,
            version,
            changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text,
            }],
        });
    }
}

pub fn listen_completion_requests(
    mut events: MessageReader<RequestCompletionEvent>,
    mut query: Query<
        (
            &TextViewState,
            Option<&LspDocument>,
            &bevy_lsp::ServerCapabilities,
            &mut LspDebounceTimers,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((tv, lsp_document, caps, mut debounce)) = query.single_mut() else {
        return;
    };
    let Some(lsp_document) = lsp_document else {
        return;
    };
    let enc = caps.position_encoding();
    for event in events.read() {
        debounce.pending_completion = Some(PendingLspRequest {
            uri: lsp_document.uri.clone(),
            position: rope_char_to_lsp_position(&tv.rope, event.cursor_char, enc),
        });
        debounce.completion_timer.reset();
    }
}

pub fn listen_hover_requests(
    mut events: MessageReader<RequestHoverEvent>,
    mut query: Query<
        (
            &TextViewState,
            Option<&LspDocument>,
            &bevy_lsp::ServerCapabilities,
            &mut LspDebounceTimers,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((tv, lsp_document, caps, mut debounce)) = query.single_mut() else {
        return;
    };
    let Some(lsp_document) = lsp_document else {
        return;
    };
    let enc = caps.position_encoding();
    for event in events.read() {
        debounce.pending_hover = Some(PendingLspRequest {
            uri: lsp_document.uri.clone(),
            position: rope_char_to_lsp_position(&tv.rope, event.cursor_char, enc),
        });
        debounce.hover_timer.reset();
    }
}

pub fn listen_rename_requests(
    mut events: MessageReader<RequestRenameEvent>,
    mut query: Query<
        (
            &TextViewState,
            Option<&LspDocument>,
            &LspClient,
            &bevy_lsp::ServerCapabilities,
            &mut LspRenamePopup,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((tv, lsp_document, lsp_client, caps, mut rename_state)) = query.single_mut() else {
        return;
    };
    let Some(lsp_document) = lsp_document else {
        return;
    };
    let enc = caps.position_encoding();
    for event in events.read() {
        let position = rope_char_to_lsp_position(&tv.rope, event.cursor_char, enc);
        rename_state.start_prepare(position);
        lsp_client.send(LspMessage::PrepareRename {
            uri: lsp_document.uri.clone(),
            position,
        });
    }
}

pub fn listen_signature_help_requests(
    mut events: MessageReader<RequestSignatureHelpEvent>,
    mut query: Query<
        (
            &TextViewState,
            Option<&LspDocument>,
            &LspClient,
            &bevy_lsp::ServerCapabilities,
            &mut LspSignatureHelpPopup,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((tv, lsp_document, lsp_client, caps, mut sig_help_state)) = query.single_mut() else {
        return;
    };
    let Some(lsp_document) = lsp_document else {
        return;
    };
    let enc = caps.position_encoding();
    for event in events.read() {
        sig_help_state.dismiss();
        lsp_client.send(LspMessage::SignatureHelp {
            uri: lsp_document.uri.clone(),
            position: rope_char_to_lsp_position(&tv.rope, event.cursor_char, enc),
            id: sig_help_state.request_id,
        });
    }
}

pub fn listen_dismiss_completion(
    mut events: MessageReader<DismissCompletionEvent>,
    mut query: Query<&mut LspCompletionPopup, With<CodeEditor>>,
) {
    let Ok(mut completion_state) = query.single_mut() else {
        return;
    };
    for _ in events.read() {
        completion_state.items.clear();
        completion_state.dismiss();
    }
}

/// Selection-change driver for `completionItem/resolve`. Owns the `&mut`
/// access to the popup; reads the local "last selected" cursor and fires
/// the resolve request when the selection has moved to an item we don't
/// have cached docs for.
pub fn drive_completion_resolve(
    mut query: Query<
        (
            &mut LspCompletionPopup,
            &bevy_lsp::LspClient,
            &bevy_lsp::ServerCapabilities,
        ),
        With<CodeEditor>,
    >,
    mut last_selected: Local<Option<usize>>,
) {
    let Ok((mut popup, lsp_client, caps)) = query.single_mut() else {
        *last_selected = None;
        return;
    };
    if !popup.visible {
        *last_selected = None;
        return;
    }
    if !caps.supports_completion_resolve() {
        return;
    }
    let current = popup.selected_index;
    if Some(current) == *last_selected {
        return;
    }
    *last_selected = Some(current);

    let filtered = popup.filtered_items();
    let Some(item) = filtered.get(current).cloned() else {
        return;
    };
    let UnifiedCompletionItem::Lsp(lsp_item) = item else {
        return;
    };
    if popup.resolved.contains_key(&lsp_item.label) {
        return;
    }
    if let Some((label, _)) = &popup.pending_resolve {
        if label == &lsp_item.label {
            return;
        }
    }
    popup.resolve_request_id = popup.resolve_request_id.wrapping_add(1);
    let id = popup.resolve_request_id;
    popup.pending_resolve = Some((lsp_item.label.clone(), id));
    lsp_client.send(LspMessage::ResolveCompletionItem {
        item: lsp_item,
        id,
    });
}

/// Dismiss the completion popup when the cursor moves out of a position
/// where completions make sense. Mirrors Zed's logic: keep the menu
/// only when (a) the cursor is at-or-after the menu's anchor and (b) the
/// character just before the cursor is a word character. Anything else
/// (clicked elsewhere, typed `;` / `(` / space, hit Backspace past the
/// anchor) hides the menu immediately.
pub fn dismiss_completion_on_cursor_move(
    mut query: Query<
        (Ref<CursorState>, &TextViewState, &mut LspCompletionPopup),
        With<CodeEditor>,
    >,
) {
    let Ok((cursor, tv, mut completion_state)) = query.single_mut() else {
        return;
    };
    if !completion_state.visible || !cursor.is_changed() {
        return;
    }
    let start = completion_state.start_char_index;
    let pos = cursor.cursor_pos;
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let in_anchor_range = pos >= start && pos <= tv.rope.len_chars();
    let prev_is_word = pos > 0 && is_word(tv.rope.char(pos - 1));
    if !in_anchor_range || !prev_is_word {
        completion_state.dismiss();
    }
}

/// Tick debounce timers and fire LSP requests when they expire.
///
/// Kept as a free function (not in this file's `pub use` set) and named
/// `tick_lsp_debounce_timers` so callers in `plugin/lsp_plugin.rs` find it
/// at the same path as before the refactor.
pub fn tick_lsp_debounce_timers(
    time: Res<Time>,
    mut query: Query<
        (
            &LspClient,
            &mut LspDebounceTimers,
            &mut LspCompletionPopup,
            &mut LspCodeActionsPopup,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((lsp_client, mut debounce, mut completion_state, mut code_actions_state)) =
        query.single_mut()
    else {
        return;
    };

    if debounce.pending_completion.is_some() {
        debounce.completion_timer.tick(time.delta());
        if debounce.completion_timer.just_finished() {
            if let Some(req) = debounce.pending_completion.take() {
                completion_state.request_id = completion_state.request_id.wrapping_add(1);
                lsp_client.send(LspMessage::Completion {
                    uri: req.uri,
                    position: req.position,
                    id: completion_state.request_id,
                });
            }
        }
    }

    if debounce.pending_hover.is_some() {
        debounce.hover_timer.tick(time.delta());
        if debounce.hover_timer.just_finished() {
            if let Some(req) = debounce.pending_hover.take() {
                lsp_client.send(LspMessage::Hover {
                    uri: req.uri,
                    position: req.position,
                });
            }
        }
    }

    if debounce.pending_highlight.is_some() {
        debounce.highlight_timer.tick(time.delta());
        if debounce.highlight_timer.just_finished() {
            if let Some(req) = debounce.pending_highlight.take() {
                lsp_client.send(LspMessage::DocumentHighlight {
                    uri: req.uri,
                    position: req.position,
                });
            }
        }
    }

    if debounce.pending_code_action.is_some() {
        debounce.code_action_timer.tick(time.delta());
        if debounce.code_action_timer.just_finished() {
            if let Some(req) = debounce.pending_code_action.take() {
                code_actions_state.request_id =
                    code_actions_state.request_id.wrapping_add(1);
                lsp_client.send(LspMessage::CodeAction {
                    uri: req.uri,
                    range: req.range,
                    diagnostics: Vec::new(),
                    id: code_actions_state.request_id,
                });
            }
        }
    }
}

/// Advance the tabstop session on `Tab` / `Shift+Tab`, and end it on
/// `Escape`. Runs **before** `bevy_text_editor::handlers::edit::handle_insert_tab`
/// so we drain `InsertTabRequested` events when a session is active —
/// the underlying handler then sees no events and inserts no tabs.
///
/// `Escape` is intercepted via `ClearSelectionRequested` since that's
/// the action the dispatcher emits for Esc.
pub fn advance_tabstop_session(
    mut tab_events: MessageReader<bevy_text_editor::InsertTabRequested>,
    mut clear_events: MessageReader<bevy_text_editor::ClearSelectionRequested>,
    mut query: Query<
        (
            &mut bevy_text_editor::SelectionState,
            &mut bevy_text_editor::EditHistoryState,
            &mut CursorState,
            &TextViewState,
            &mut TabstopSession,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((mut sel, hist, mut cursor, tv, mut session)) = query.single_mut() else {
        // Drain events even when there's no session entity (avoid leaks).
        let _ = tab_events.read().count();
        let _ = clear_events.read().count();
        return;
    };

    if !session.is_active() {
        // Session dormant — let the events flow through to the normal
        // handlers untouched.
        let _ = tab_events.read().count();
        let _ = clear_events.read().count();
        return;
    }

    // Esc ends the session.
    if clear_events.read().next().is_some() {
        session.end();
        // Drain remaining tab events so they don't trigger the underlying
        // tab handler this frame on a dead session.
        let _ = tab_events.read().count();
        return;
    }

    let tab_pressed = tab_events.read().next().is_some();
    if !tab_pressed {
        return;
    }

    // Drain any remaining Tab events — we consume the whole burst.
    let _ = tab_events.read().count();

    let next = session.current + 1;
    if next >= session.stops.len() {
        // Last stop was just visited; end the session and let cursor
        // remain wherever the user moved it. Final stop ($0) typically
        // sits where they want the caret to land.
        session.end();
        return;
    }
    session.current = next;
    let stop = session.stops[next].clone();
    let s = hist.resolve_anchor(&tv.rope, &stop.start);
    let e = hist.resolve_anchor(&tv.rope, &stop.end);
    cursor.cursor_pos = e;
    if s != e {
        sel.selections = bevy_text_editor::SelectionCollection::with_selection(e, s);
    } else {
        sel.selections = bevy_text_editor::SelectionCollection::with_cursor(e);
    }
}

/// End an active tabstop session when the cursor moves outside the
/// covered range (e.g. user clicked elsewhere) or when a non-snippet
/// edit happens. Cheap when no session is active.
pub fn end_tabstop_session_on_cursor_leave(
    mut query: Query<
        (
            Ref<CursorState>,
            &TextViewState,
            &mut bevy_text_editor::EditHistoryState,
            &mut TabstopSession,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((cursor, tv, hist, mut session)) = query.single_mut() else {
        return;
    };
    if !session.is_active() || !cursor.is_changed() {
        return;
    }
    // Compute the covered range as [min(start), max(end)] across all
    // remaining stops. If the cursor leaves it, end.
    let mut min_start = usize::MAX;
    let mut max_end = 0;
    for stop in session.stops.iter().skip(session.current) {
        let s = hist.resolve_anchor(&tv.rope, &stop.start);
        let e = hist.resolve_anchor(&tv.rope, &stop.end);
        min_start = min_start.min(s);
        max_end = max_end.max(e);
    }
    let pos = cursor.cursor_pos;
    if pos < min_start || pos > max_end {
        session.end();
    }
}

/// Listens to ApplyCompletionEvent. Applies the edit synchronously via
/// `EditHistoryState::replace_range` (rather than emitting
/// `ReplaceRangeRequested`) so that, when the inserted item carries
/// snippet syntax, we can immediately create anchors for the tabstops
/// from the post-edit rope and start a `TabstopSession`.
pub fn listen_apply_completion(
    mut events: MessageReader<ApplyCompletionEvent>,
    mut query: Query<
        (
            &mut bevy_text_editor::SelectionState,
            &mut bevy_text_editor::EditHistoryState,
            &mut CursorState,
            &mut TextViewState,
            &mut LspCompletionPopup,
            &mut TabstopSession,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((
        mut sel,
        mut hist,
        mut cursor_state,
        mut tv,
        mut completion_state,
        mut session,
    )) = query.single_mut()
    else {
        return;
    };
    for event in events.read() {
        let filtered = completion_state.filtered_items();
        if event.item_index >= filtered.len() {
            continue;
        }
        let item = &filtered[event.item_index];

        let cursor_pos = cursor_state.cursor_pos.min(tv.rope.len_chars());
        let line = tv.rope.char_to_line(cursor_pos);
        let line_start = tv.rope.line_to_char(line);
        let start_pos = completion_state.start_char_index.max(line_start).min(cursor_pos);

        // Decide whether the item carries snippet syntax. LSP marks this
        // explicitly via `insert_text_format`; we only treat snippet items
        // (rust-analyzer marks function calls / for-loops / etc.) — word
        // completions go through verbatim.
        let parsed = match item {
            UnifiedCompletionItem::Lsp(lsp_item)
                if lsp_item.insert_text_format
                    == Some(lsp_types::InsertTextFormat::SNIPPET) =>
            {
                Some(snippet::parse(lsp_item.insert_text.as_deref().unwrap_or(&lsp_item.label)))
            }
            _ => None,
        };
        let plain_text = match &parsed {
            Some(p) => p.text.clone(),
            None => item.insert_text().to_string(),
        };

        let outcome = hist.replace_range(
            &mut tv,
            start_pos,
            cursor_pos,
            &plain_text,
            bevy_text_editor::EditKind::Other,
            true,
        );

        // Build a tabstop session from the parsed snippet.
        if let Some(parsed) = parsed {
            if parsed.has_tabstops() {
                session.end();
                let mut stops_sorted = parsed.tabstops.clone();
                // LSP semantics: walk in ascending id, with `0` (final
                // stop) at the end.
                stops_sorted.sort_by_key(|t| if t.id == 0 { u32::MAX } else { t.id });
                let inserted_start = outcome.start_char;
                let mut session_stops = Vec::with_capacity(stops_sorted.len());
                for stop in stops_sorted {
                    let abs_start = inserted_start + stop.start;
                    let abs_end = inserted_start + stop.end;
                    let start_anchor = hist.create_anchor(
                        &tv.rope,
                        abs_start,
                        bevy_text_editor::AnchorBias::Left,
                    );
                    let end_anchor = hist.create_anchor(
                        &tv.rope,
                        abs_end,
                        bevy_text_editor::AnchorBias::Right,
                    );
                    session_stops.push(SessionTabstop {
                        id: stop.id,
                        start: start_anchor,
                        end: end_anchor,
                    });
                }
                session.stops = session_stops;
                session.current = 0;
                // Move cursor to first tabstop and select its placeholder
                // range (if any).
                if let Some(first) = session.stops.first() {
                    let s = hist.resolve_anchor(&tv.rope, &first.start);
                    let e = hist.resolve_anchor(&tv.rope, &first.end);
                    cursor_state.cursor_pos = e;
                    if s != e {
                        sel.selections =
                            bevy_text_editor::SelectionCollection::with_selection(e, s);
                    } else {
                        sel.selections =
                            bevy_text_editor::SelectionCollection::with_cursor(e);
                    }
                }
            } else {
                cursor_state.cursor_pos = outcome.new_cursor_pos;
                sel.apply_primary_cursor(&cursor_state);
            }
        } else {
            cursor_state.cursor_pos = outcome.new_cursor_pos;
            sel.apply_primary_cursor(&cursor_state);
        }
        completion_state.dismiss();
    }
}
