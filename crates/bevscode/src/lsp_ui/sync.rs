//! Systems that sync per-editor LSP state Components into render-data Components.
//!
//! Each system queries the editor entity for one popup-state Component
//! (e.g. [`LspCompletionPopup`]) and produces / updates an entity with
//! the matching `*PopupData` Component (e.g. [`CompletionPopupData`]).
//!
//! All popup data is **semantic** — it carries `(line, character)`
//! anchors and content/sizing, not pre-baked screen positions. Hosts
//! query the `*Data` Components and render however they want, composing
//! the world position from `(line, character)` + the editor's
//! `RowMetrics` at render time. That way a scroll, viewport resize, or
//! font change doesn't need to invalidate the popup data — the renderer
//! reads live engine state.

use bevy::prelude::*;
use bevy_instanced_text_editor::RopeBuffer;

use crate::settings::*;
use crate::text_view::TextBuffer;
use crate::types::{CodeEditor, CursorState};
use bevy_instanced_text::MonoCellWidth;

use super::components::*;
use super::state::{
    LspCodeActionsPopup, LspCompletionPopup, LspDocumentHighlights, LspHoverPopup, LspInlayHints,
    LspRenamePopup, LspSignatureHelpPopup,
};
use super::systems::DiagnosticMarker;
use bevy_lsp::{CodeActionOrCommand, ServerCapabilities};
use lsp_types::DiagnosticSeverity;

/// Hard cap on the hover popup's outer height. Markdown content longer
/// than this is reachable via vertical scroll inside the popup chrome
/// — see [`crate::lsp_ui_tempera::hover::update_hover_popup`].
const MAX_HOVER_HEIGHT: f32 = 320.0;

/// Resolve a char index into `(line, character)`.
fn buffer_position(buffer: &TextBuffer<RopeBuffer>, char_index: usize) -> (u32, u32) {
    let char_index = char_index.min(buffer.len_chars());
    let line = buffer.char_to_line(char_index);
    let line_start = buffer.line_to_char(line);
    let col = char_index - line_start;
    (line as u32, col as u32)
}

/// Sync completion state to marker entity
pub fn sync_completion_popup(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &LspCompletionPopup,
            &CursorState,
            &TextBuffer<RopeBuffer>,
            &TextFont,
            &bevy::text::LineHeight,
            &MonoCellWidth,
            &LspConfig,
        ),
        With<CodeEditor>,
    >,
    existing: Query<Entity, With<CompletionPopupData>>,
) {
    let Ok((editor, completion_state, cursor_state, buffer, font, lh, mono, lsp)) = query.single()
    else {
        return;
    };
    let line_height = bevy_instanced_text::resolve_line_height(*lh, font.font_size);
    let filtered_items = completion_state.filtered_items();

    if !completion_state.visible || filtered_items.is_empty() {
        for entity in existing.iter() {
            commands
                .entity(entity)
                .queue_silenced(bevy::ecs::system::entity_command::despawn());
        }
        return;
    }

    let (line, character) = buffer_position(buffer, cursor_state.cursor_pos);

    let max_char_count = filtered_items
        .iter()
        .take(10)
        .map(|item| {
            let label_len = item.label().chars().count();
            let detail_len = item.detail().map(|d| d.chars().count()).unwrap_or(0);
            label_len + detail_len + 7
        })
        .max()
        .unwrap_or(20);

    let calculated_width = (max_char_count as f32 * mono.px) + 20.0;
    let box_width = calculated_width.clamp(200.0, 600.0);

    let max_visible = lsp.completion.max_items;
    let total_items = filtered_items.len();
    let visible_count = total_items.min(max_visible);
    let box_height = (visible_count as f32 * line_height) + 10.0;

    let items: Vec<CompletionItemData> = filtered_items
        .iter()
        .map(CompletionItemData::from)
        .collect();

    let selected_documentation = filtered_items
        .get(completion_state.selected_index)
        .and_then(|item| {
            let label = item.label();
            completion_state.resolved.get(label).and_then(|resolved| {
                resolved
                    .documentation
                    .as_ref()
                    .map(|doc| match doc {
                        lsp_types::Documentation::String(s) => s.clone(),
                        lsp_types::Documentation::MarkupContent(m) => m.value.clone(),
                    })
                    .filter(|s| !s.is_empty())
            })
        });

    let popup_data = CompletionPopupData {
        editor,
        line,
        character,
        items,
        selected_index: completion_state.selected_index,
        scroll_offset: completion_state.scroll_offset,
        max_visible,
        width: box_width,
        height: box_height,
        selected_documentation,
    };

    if let Some(entity) = existing.iter().next() {
        commands.entity(entity).insert(popup_data);
    } else {
        commands.spawn((popup_data, LspUiElement, Name::new("CompletionPopup")));
    }

    for entity in existing.iter().skip(1) {
        commands
            .entity(entity)
            .queue_silenced(bevy::ecs::system::entity_command::despawn());
    }
}

/// Sync hover state to marker entity.
///
/// Renders the popup from two sources, merged: any [`DiagnosticMarker`]
/// whose range covers the trigger position is prepended (severity-tagged)
/// to the LSP `textDocument/hover` content. This makes hovering a squiggle
/// surface the error/warning message even when the server has no hover
/// content for that token (matches VSCode behavior).
pub fn sync_hover_popup(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &LspHoverPopup,
            &TextBuffer<RopeBuffer>,
            &TextFont,
            &MonoCellWidth,
            &ServerCapabilities,
            Option<&bevy_lsp::LspDocument>,
        ),
        With<CodeEditor>,
    >,
    existing: Query<Entity, With<HoverPopupData>>,
    diagnostics: Query<&DiagnosticMarker>,
) {
    let Ok((editor, hover_state, buffer, font, mono, caps, doc)) = query.single() else {
        return;
    };

    if !hover_state.visible {
        for entity in existing.iter() {
            commands
                .entity(entity)
                .queue_silenced(bevy::ecs::system::entity_command::despawn());
        }
        return;
    }

    let (line, character) = buffer_position(buffer, hover_state.trigger_char_index);
    let trigger_lsp_pos = bevy_lsp::rope_char_to_lsp_position(
        buffer.rope(),
        hover_state.trigger_char_index.min(buffer.len_chars()),
        caps.position_encoding(),
    );

    let diagnostic_md = collect_diagnostics_md(&diagnostics, doc, trigger_lsp_pos);

    let content = match (diagnostic_md.is_empty(), hover_state.content.is_empty()) {
        (true, true) => {
            for entity in existing.iter() {
                commands
                    .entity(entity)
                    .queue_silenced(bevy::ecs::system::entity_command::despawn());
            }
            return;
        }
        (true, false) => hover_state.content.clone(),
        (false, true) => diagnostic_md,
        (false, false) => format!("{diagnostic_md}\n\n---\n\n{}", hover_state.content),
    };

    let font_size = font.font_size * 0.9;
    let padding = 10.0;

    let max_line_chars = content
        .lines()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0);
    let hover_char_width = mono.px * 0.9;

    let calculated_width = (max_line_chars as f32 * hover_char_width) + padding * 2.0;
    let box_width = calculated_width.clamp(100.0, 600.0);

    let line_count = content.lines().count().max(1);
    // Plain-text-equivalent height — markdown adds block gaps, code-
    // block padding, etc., so this underestimates. The renderer caps
    // and scrolls instead of trying to measure markdown ahead of time.
    let raw_height = (line_count as f32 * font_size * 1.2) + padding * 2.0;
    let box_height = raw_height.min(MAX_HOVER_HEIGHT);

    let popup_data = HoverPopupData {
        editor,
        line,
        character,
        content,
        width: box_width,
        height: box_height,
    };

    if let Some(entity) = existing.iter().next() {
        commands.entity(entity).insert(popup_data);
    } else {
        commands.spawn((popup_data, LspUiElement, Name::new("HoverPopup")));
    }

    for entity in existing.iter().skip(1) {
        commands
            .entity(entity)
            .queue_silenced(bevy::ecs::system::entity_command::despawn());
    }
}

/// Build the markdown block for any diagnostics whose range contains
/// `position`. Empty when no diagnostic covers the position.
///
/// Scope is restricted to diagnostics whose URI matches the current
/// document so multi-editor setups don't bleed messages across files.
fn collect_diagnostics_md(
    diagnostics: &Query<&DiagnosticMarker>,
    doc: Option<&bevy_lsp::LspDocument>,
    position: lsp_types::Position,
) -> String {
    let mut hits: Vec<&DiagnosticMarker> = diagnostics
        .iter()
        .filter(|d| doc.is_none_or(|doc| doc.uri == d.uri))
        .filter(|d| range_contains(d.range, position))
        .collect();
    if hits.is_empty() {
        return String::new();
    }
    // Most-severe first (Error < Warning < Information < Hint by LSP's
    // numeric ordering — `DiagnosticSeverity` implements `Ord` on the
    // wrapped i32).
    hits.sort_by_key(|d| d.severity);

    hits.iter()
        .map(|d| {
            let label = severity_label(d.severity);
            // Trim trailing newlines from the server-supplied message
            // so the joined block has consistent spacing.
            format!("**{label}:** {}", d.message.trim_end())
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Does `range` cover position `p` for the purposes of diagnostic
/// hover-hit-testing?
///
/// Mirrors the widening in
/// [`crate::plugin::diagnostic_underlines::update_diagnostic_underlines`]
/// for zero-width ranges (start == end, e.g. rust-analyzer's
/// `expected SEMICOLON` at col 15..15): without widening, the user
/// would have to land the pointer *exactly* on the empty cell. Treat
/// zero-width ranges as covering the entire line the diagnostic is on
/// so hovering anywhere on the line surfaces the message.
///
/// LSP ranges are half-open (end-exclusive). The standard branch
/// rejects `p == end`.
fn range_contains(range: lsp_types::Range, p: lsp_types::Position) -> bool {
    if range.start == range.end {
        return p.line == range.start.line;
    }
    let after_start = (p.line, p.character) >= (range.start.line, range.start.character);
    let before_end = (p.line, p.character) < (range.end.line, range.end.character);
    after_start && before_end
}

fn severity_label(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::ERROR => "Error",
        DiagnosticSeverity::WARNING => "Warning",
        DiagnosticSeverity::INFORMATION => "Info",
        DiagnosticSeverity::HINT => "Hint",
        _ => "Diagnostic",
    }
}

/// Sync signature help state to marker entity
pub fn sync_signature_help_popup(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &LspSignatureHelpPopup,
            &CursorState,
            &TextBuffer<RopeBuffer>,
            &TextFont,
            &MonoCellWidth,
        ),
        With<CodeEditor>,
    >,
    existing: Query<Entity, With<SignatureHelpPopupData>>,
) {
    let Ok((editor, sig_state, cursor_state, buffer, font, mono)) = query.single() else {
        return;
    };

    if !sig_state.visible || sig_state.signatures.is_empty() {
        for entity in existing.iter() {
            commands
                .entity(entity)
                .queue_silenced(bevy::ecs::system::entity_command::despawn());
        }
        return;
    }

    let Some(signature) = sig_state.current_signature() else {
        for entity in existing.iter() {
            commands
                .entity(entity)
                .queue_silenced(bevy::ecs::system::entity_command::despawn());
        }
        return;
    };

    let (line, character) = buffer_position(buffer, cursor_state.cursor_pos);

    let font_size = font.font_size * 0.9;
    let padding = 8.0;

    let sig_label = &signature.label;
    let box_width =
        (sig_label.chars().count() as f32 * mono.px * 0.9 + padding * 2.0).clamp(100.0, 600.0);
    let box_height = font_size * 1.4 + padding * 2.0;

    let parameter_ranges = signature
        .parameters
        .as_ref()
        .map(|params| {
            params
                .iter()
                .filter_map(|p| match &p.label {
                    lsp_types::ParameterLabel::LabelOffsets(offsets) => {
                        Some((offsets[0] as usize, offsets[1] as usize))
                    }
                    lsp_types::ParameterLabel::Simple(s) => sig_label
                        .find(s.as_str())
                        .map(|start| (start, start + s.len())),
                })
                .collect()
        })
        .unwrap_or_default();

    let popup_data = SignatureHelpPopupData {
        editor,
        line,
        character,
        label: sig_label.clone(),
        active_parameter: sig_state.active_parameter,
        parameter_ranges,
        total_signatures: sig_state.signatures.len(),
        current_index: sig_state.active_signature,
        width: box_width,
        height: box_height,
    };

    if let Some(entity) = existing.iter().next() {
        commands.entity(entity).insert(popup_data);
    } else {
        commands.spawn((popup_data, LspUiElement, Name::new("SignatureHelpPopup")));
    }

    for entity in existing.iter().skip(1) {
        commands
            .entity(entity)
            .queue_silenced(bevy::ecs::system::entity_command::despawn());
    }
}

/// Sync code action state to marker entity
pub fn sync_code_actions_popup(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &LspCodeActionsPopup,
            &CursorState,
            &TextBuffer<RopeBuffer>,
            &TextFont,
            &bevy::text::LineHeight,
            &MonoCellWidth,
        ),
        With<CodeEditor>,
    >,
    existing: Query<Entity, With<CodeActionsPopupData>>,
) {
    let Ok((editor, action_state, cursor_state, buffer, font, lh, mono)) = query.single() else {
        return;
    };
    let line_height = bevy_instanced_text::resolve_line_height(*lh, font.font_size);

    if !action_state.visible || action_state.actions.is_empty() {
        for entity in existing.iter() {
            commands
                .entity(entity)
                .queue_silenced(bevy::ecs::system::entity_command::despawn());
        }
        return;
    }

    let (line, character) = buffer_position(buffer, cursor_state.cursor_pos);

    let max_label_len = action_state
        .actions
        .iter()
        .map(|a| match a {
            CodeActionOrCommand::Action(action) => action.title.chars().count(),
            CodeActionOrCommand::Command(cmd) => cmd.title.chars().count(),
        })
        .max()
        .unwrap_or(20);

    let box_width = (max_label_len as f32 * mono.px + 20.0).clamp(200.0, 400.0);
    let visible_count = action_state.actions.len().min(10);
    let box_height = (visible_count as f32 * line_height) + 10.0;

    let actions: Vec<CodeActionItemData> = action_state
        .actions
        .iter()
        .take(10)
        .map(|a| {
            let (icon, title, is_preferred) = match a {
                CodeActionOrCommand::Action(action) => {
                    let icon = match &action.kind {
                        Some(kind) if kind.as_str().starts_with("quickfix") => "🔧",
                        Some(kind) if kind.as_str().starts_with("refactor") => "✨",
                        Some(kind) if kind.as_str().starts_with("source") => "📁",
                        _ => "💡",
                    };
                    (
                        icon,
                        action.title.as_str(),
                        action.is_preferred.unwrap_or(false),
                    )
                }
                CodeActionOrCommand::Command(c) => ("⚡", c.title.as_str(), false),
            };
            CodeActionItemData {
                title: title.to_string(),
                icon: icon.to_string(),
                is_preferred,
            }
        })
        .collect();

    let popup_data = CodeActionsPopupData {
        editor,
        line,
        character,
        actions,
        selected_index: action_state.selected_index,
        width: box_width,
        height: box_height,
    };

    if let Some(entity) = existing.iter().next() {
        commands.entity(entity).insert(popup_data);
    } else {
        commands.spawn((popup_data, LspUiElement, Name::new("CodeActionsPopup")));
    }

    for entity in existing.iter().skip(1) {
        commands
            .entity(entity)
            .queue_silenced(bevy::ecs::system::entity_command::despawn());
    }
}

/// Sync rename state to marker entity
pub fn sync_rename_input(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &LspRenamePopup,
            &TextFont,
            &bevy::text::LineHeight,
            &MonoCellWidth,
        ),
        With<CodeEditor>,
    >,
    existing: Query<Entity, With<RenameInputData>>,
) {
    let Ok((editor, rename_state, font, lh, mono)) = query.single() else {
        return;
    };
    let line_height = bevy_instanced_text::resolve_line_height(*lh, font.font_size);

    if !rename_state.visible {
        for entity in existing.iter() {
            commands
                .entity(entity)
                .queue_silenced(bevy::ecs::system::entity_command::despawn());
        }
        return;
    }

    let Some(range) = &rename_state.range else {
        for entity in existing.iter() {
            commands
                .entity(entity)
                .queue_silenced(bevy::ecs::system::entity_command::despawn());
        }
        return;
    };

    let line = range.start.line;
    let character = range.start.character;

    let padding_x = 4.0;
    let padding_y = 2.0;

    let display_text = if rename_state.new_name.is_empty() {
        &rename_state.original_text
    } else {
        &rename_state.new_name
    };

    let text_width = (display_text.chars().count().max(8) as f32 * mono.px) + padding_x * 2.0 + 4.0;
    let box_width = text_width.clamp(100.0, 300.0);
    let box_height = line_height + padding_y * 2.0;

    let popup_data = RenameInputData {
        editor,
        line,
        character,
        text: display_text.to_string(),
        original_text: rename_state.original_text.clone(),
        cursor_position: display_text.chars().count(),
        width: box_width,
        height: box_height,
    };

    if let Some(entity) = existing.iter().next() {
        commands.entity(entity).insert(popup_data);
    } else {
        commands.spawn((popup_data, LspUiElement, Name::new("RenameInput")));
    }

    for entity in existing.iter().skip(1) {
        commands
            .entity(entity)
            .queue_silenced(bevy::ecs::system::entity_command::despawn());
    }
}

/// Sync inlay hints to marker entities.
///
/// Each entity carries a `InlayHintData` with semantic data only
/// `(line, character, label, kind)`. Renderers compose the world
/// position from those fields + the editor's `RowMetrics`, so this
/// system doesn't need to invalidate on viewport / scroll / font
/// changes — the renderer reads live state.
pub fn sync_inlay_hints(
    mut commands: Commands,
    query: Query<(Ref<LspInlayHints>, Option<&crate::settings::Suggest>), With<CodeEditor>>,
    existing: Query<Entity, With<InlayHintData>>,
) {
    let Ok((hint_state, suggest)) = query.single() else {
        return;
    };

    if !hint_state.is_changed() {
        return;
    }

    for entity in existing.iter() {
        commands
            .entity(entity)
            .queue_silenced(bevy::ecs::system::entity_command::despawn());
    }

    if hint_state.hints.is_empty() {
        return;
    }

    let max_len = suggest.map(|s| s.inlay_hints.maximum_length as usize);

    for hint in &hint_state.hints {
        let mut label_text = match &hint.label {
            lsp_types::InlayHintLabel::String(s) => s.clone(),
            lsp_types::InlayHintLabel::LabelParts(parts) => parts
                .iter()
                .map(|p| p.value.as_str())
                .collect::<Vec<_>>()
                .join(""),
        };

        if let Some(max) = max_len {
            if max > 0 && label_text.chars().count() > max {
                let keep = max.saturating_sub(1);
                let truncated: String = label_text.chars().take(keep).collect();
                label_text = format!("{truncated}\u{2026}");
            }
        }

        let kind = match hint.kind {
            Some(lsp_types::InlayHintKind::TYPE) => InlayHintKind::Type,
            Some(lsp_types::InlayHintKind::PARAMETER) => InlayHintKind::Parameter,
            _ => InlayHintKind::Other,
        };

        commands.spawn((
            InlayHintData {
                label: label_text,
                kind,
                line: hint.position.line,
                character: hint.position.character,
            },
            LspUiElement,
            Name::new("InlayHint"),
        ));
    }
}

/// Sync document highlights to marker entities.
///
/// Each entity carries a `DocumentHighlightData` describing *what* to
/// highlight (line + character range + read/write kind). The renderer
/// (in the host crate) composes the world rectangle from these fields
/// and the editor's `RowMetrics`, so this system doesn't need to re-run
/// on viewport or scroll changes — the renderer reads live state.
pub fn sync_document_highlights(
    mut commands: Commands,
    query: Query<Ref<LspDocumentHighlights>, With<CodeEditor>>,
    existing: Query<Entity, With<DocumentHighlightData>>,
) {
    let Ok(highlight_state) = query.single() else {
        return;
    };

    if !highlight_state.is_changed() {
        return;
    }

    for entity in existing.iter() {
        commands
            .entity(entity)
            .queue_silenced(bevy::ecs::system::entity_command::despawn());
    }

    if !highlight_state.visible || highlight_state.highlights.is_empty() {
        return;
    }

    for highlight in &highlight_state.highlights {
        let is_write = matches!(
            highlight.kind,
            Some(lsp_types::DocumentHighlightKind::WRITE),
        );

        let start_line = highlight.range.start.line;
        let end_line = highlight.range.end.line;

        if start_line == end_line {
            commands.spawn((
                DocumentHighlightData {
                    line: start_line,
                    start_character: highlight.range.start.character,
                    end_character: highlight.range.end.character,
                    is_write,
                },
                LspUiElement,
                Name::new("DocumentHighlight"),
            ));
        } else {
            for line in start_line..=end_line {
                let (start_char, end_char) = if line == start_line {
                    (highlight.range.start.character, u32::MAX)
                } else if line == end_line {
                    (0, highlight.range.end.character)
                } else {
                    (0, u32::MAX)
                };

                commands.spawn((
                    DocumentHighlightData {
                        line,
                        start_character: start_char,
                        end_character: end_char,
                        is_write,
                    },
                    LspUiElement,
                    Name::new("DocumentHighlight"),
                ));
            }
        }
    }
}

/// Splice each editor's `InlayHintData` entities into its `LineStyles` so
/// the engine's shape pipeline lays the hints out inline (subsequent source
/// glyphs shift right). Runs after `produce_line_styles` so it sees the
/// freshly-written, inlay-free per-line span vec; rebuilds with virtual
/// spans inserted at the right byte offsets.
///
/// **Idempotent**: drops any pre-existing virtual spans before splicing so
/// re-running on top of its own output produces the same result. This
/// matters because we mutate `LineStyles` in place and Bevy doesn't roll
/// back the previous frame's write.
///
/// Encoding: `InlayHintData.character` is in the LSP server's negotiated
/// position encoding (UTF-16 code units by default). We convert via
/// `bevy_lsp::lsp_position_to_rope_byte` using the editor's
/// `ServerCapabilities::position_encoding()`.
pub fn splice_inlays_into_line_styles(
    mut editors: Query<
        (
            &TextBuffer<RopeBuffer>,
            &mut bevy_instanced_text::LineStyles,
            &bevy_lsp::ServerCapabilities,
            Ref<super::state::LspInlayHints>,
        ),
        With<CodeEditor>,
    >,
    hints: Query<&InlayHintData>,
    theme: Res<crate::lsp_ui_tempera::inline_decorations::InlineDecorationsTheme>,
) {
    for (buffer, mut line_styles, caps, hint_state) in editors.iter_mut() {
        // Only run when something that could affect the splice changed:
        // hint state arrived, or LineStyles got a fresh write whose
        // virtuals we need to re-insert. `line_styles.is_changed()` also
        // returns true on the frame after our own write, but that pass
        // will find no diff vs the cached map and bail without mutating.
        if !hint_state.is_changed() && !line_styles.is_changed() {
            continue;
        }

        // Group inlays by line, keyed by source byte offset in that line
        // (post UTF-16 → byte conversion). Sorted ascending so the
        // splice walk is monotonic.
        let rope = buffer.rope();
        let enc = caps.position_encoding();
        let mut by_line: std::collections::HashMap<u32, Vec<(usize, &InlayHintData)>> =
            std::collections::HashMap::new();
        for hint in hints.iter() {
            if (hint.line as usize) >= rope.len_lines() {
                continue;
            }
            let line_byte_start = rope.line_to_byte(hint.line as usize);
            let pos = lsp_types::Position {
                line: hint.line,
                character: hint.character,
            };
            let rope_byte = bevy_lsp::lsp_position_to_rope_byte(rope, pos, enc);
            let byte_in_line = rope_byte.saturating_sub(line_byte_start);
            by_line
                .entry(hint.line)
                .or_default()
                .push((byte_in_line, hint));
        }
        for v in by_line.values_mut() {
            v.sort_by_key(|(b, _)| *b);
        }

        let mut next = (*line_styles.by_line).clone();
        let mut changed = false;

        for (line, spans) in next.iter_mut() {
            // Drop any virtuals from a prior splice on this line.
            let before_len = spans.len();
            spans.retain(|s| !s.is_virtual);
            if spans.len() != before_len {
                changed = true;
            }

            let Some(inlays) = by_line.get(line) else {
                continue;
            };
            if inlays.is_empty() {
                continue;
            }
            changed = true;
            *spans = splice_inlays_into_spans(spans, inlays, &theme);
        }

        if changed {
            *line_styles = bevy_instanced_text::LineStyles::new(next);
        }
    }
}

/// Splice virtual inlay spans into `source_spans` at the byte offsets in
/// `inlays` (sorted ascending by byte). Returns a new span vec where each
/// inlay sits between source bytes — splitting a source span if the inlay
/// lands mid-span.
fn splice_inlays_into_spans(
    source_spans: &[bevy_instanced_text::FormattedSpan],
    inlays: &[(usize, &InlayHintData)],
    theme: &crate::lsp_ui_tempera::inline_decorations::InlineDecorationsTheme,
) -> Vec<bevy_instanced_text::FormattedSpan> {
    let mut out: Vec<bevy_instanced_text::FormattedSpan> =
        Vec::with_capacity(source_spans.len() + inlays.len());
    let mut inlay_iter = inlays.iter().peekable();
    let mut byte_cursor: usize = 0;

    for span in source_spans {
        let span_start = byte_cursor;
        let span_end = span_start + span.text.len();
        // Cut points for any inlays that fall within (`< span_end`) the
        // current span. An inlay at exactly `span_end` belongs at the
        // start of the *next* span.
        let mut last_cut = 0usize;
        while let Some((byte_in_line, _)) = inlay_iter.peek() {
            if *byte_in_line >= span_end {
                break;
            }
            let (byte_in_line, hint) = inlay_iter.next().unwrap();
            let local = byte_in_line.saturating_sub(span_start);
            if local > last_cut {
                out.push(clone_with_text(span, &span.text[last_cut..local]));
            }
            out.push(inlay_span(hint, theme));
            last_cut = local;
        }
        if last_cut < span.text.len() {
            out.push(clone_with_text(span, &span.text[last_cut..]));
        }
        byte_cursor = span_end;
    }
    // Inlays past the last source byte sit at end-of-line (e.g. trailing
    // type annotations on lines that end before the hint's reported byte).
    for (_, hint) in inlay_iter {
        out.push(inlay_span(hint, theme));
    }
    out
}

fn clone_with_text(
    src: &bevy_instanced_text::FormattedSpan,
    text: &str,
) -> bevy_instanced_text::FormattedSpan {
    bevy_instanced_text::FormattedSpan {
        text: text.to_string(),
        format: src.format.clone(),
        is_virtual: false,
    }
}

/// Build the virtual `FormattedSpan` for a single inlay hint.
fn inlay_span(
    hint: &InlayHintData,
    theme: &crate::lsp_ui_tempera::inline_decorations::InlineDecorationsTheme,
) -> bevy_instanced_text::FormattedSpan {
    let color = match hint.kind {
        InlayHintKind::Type => theme.inlay_type,
        InlayHintKind::Parameter => theme.inlay_parameter,
        InlayHintKind::Other => theme.inlay_other,
    };
    let mut format = bevy_instanced_text::TextFormat::fg(0..0, color);
    if theme.inlay_font_scale != 1.0 {
        format = format.with_scale(theme.inlay_font_scale);
    }
    bevy_instanced_text::FormattedSpan {
        text: hint.label.clone(),
        format,
        is_virtual: true,
    }
}

#[cfg(test)]
mod inlay_splice_tests {
    //! Unit tests for the inlay-hint splice logic.
    //!
    //! `splice_inlays_into_spans` is the meat of the splicer — it takes
    //! source-spans + a per-line list of inlays and returns interleaved
    //! spans where each inlay sits at the right byte offset, splitting
    //! source spans that an inlay falls inside. Tests target this
    //! function directly to keep the harness light; the outer system is
    //! straightforward wiring (group by line, drop prior virtuals, call
    //! this).
    use super::*;
    use crate::lsp_ui_tempera::inline_decorations::InlineDecorationsTheme;
    use bevy_instanced_text::{FormattedSpan, TextFormat};

    fn src(text: &str) -> FormattedSpan {
        FormattedSpan {
            text: text.to_string(),
            format: TextFormat::fg(0..0, bevy::prelude::Color::WHITE),
            is_virtual: false,
        }
    }

    fn hint(line: u32, character: u32, label: &str, kind: InlayHintKind) -> InlayHintData {
        InlayHintData {
            line,
            character,
            label: label.to_string(),
            kind,
        }
    }

    /// An inlay at the boundary between two source spans inserts cleanly
    /// between them without splitting either.
    #[test]
    fn inlay_at_span_boundary_inserts_between() {
        let theme = InlineDecorationsTheme::default();
        let spans = vec![src("let x"), src(" = foo()")];
        // Inlay at byte 5 (start of " = foo()" — right at the boundary).
        let h = hint(0, 5, ": i32", InlayHintKind::Type);
        let inlays = vec![(5usize, &h)];

        let out = splice_inlays_into_spans(&spans, &inlays, &theme);

        assert_eq!(out.len(), 3, "expected source + virtual + source");
        assert_eq!(out[0].text, "let x");
        assert!(!out[0].is_virtual);
        assert_eq!(out[1].text, ": i32");
        assert!(out[1].is_virtual);
        assert_eq!(out[2].text, " = foo()");
        assert!(!out[2].is_virtual);
    }

    /// An inlay landing inside a source span splits the span into a
    /// prefix + the virtual + a suffix, each keyed off the original
    /// source span's style.
    #[test]
    fn inlay_inside_span_splits_it() {
        let theme = InlineDecorationsTheme::default();
        let spans = vec![src("foo(bar)")];
        // Inlay at byte 4 (between `(` and `bar`).
        let h = hint(0, 4, "param: ", InlayHintKind::Parameter);
        let inlays = vec![(4usize, &h)];

        let out = splice_inlays_into_spans(&spans, &inlays, &theme);

        assert_eq!(out.len(), 3);
        assert_eq!(out[0].text, "foo(");
        assert!(!out[0].is_virtual);
        assert_eq!(out[1].text, "param: ");
        assert!(out[1].is_virtual);
        assert_eq!(out[2].text, "bar)");
        assert!(!out[2].is_virtual);
    }

    /// Multiple inlays on one line interleave in sorted byte order.
    #[test]
    fn multiple_inlays_interleave_in_order() {
        let theme = InlineDecorationsTheme::default();
        let spans = vec![src("abcde")];
        let h1 = hint(0, 1, "[1]", InlayHintKind::Type);
        let h2 = hint(0, 3, "[2]", InlayHintKind::Type);
        let inlays = vec![(1usize, &h1), (3usize, &h2)];

        let out = splice_inlays_into_spans(&spans, &inlays, &theme);

        let texts: Vec<&str> = out.iter().map(|s| s.text.as_str()).collect();
        let virtuals: Vec<bool> = out.iter().map(|s| s.is_virtual).collect();
        assert_eq!(texts, vec!["a", "[1]", "bc", "[2]", "de"]);
        assert_eq!(virtuals, vec![false, true, false, true, false]);
    }

    /// An inlay anchored past the line's last source byte appends at the
    /// end (no source tail to split).
    #[test]
    fn inlay_past_end_of_line_appends() {
        let theme = InlineDecorationsTheme::default();
        let spans = vec![src("foo")];
        let h = hint(0, 10, " : trailing", InlayHintKind::Other);
        let inlays = vec![(10usize, &h)];

        let out = splice_inlays_into_spans(&spans, &inlays, &theme);

        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "foo");
        assert!(!out[0].is_virtual);
        assert_eq!(out[1].text, " : trailing");
        assert!(out[1].is_virtual);
    }

    /// No inlays on the line returns the source spans unchanged.
    #[test]
    fn empty_inlays_returns_source_unchanged() {
        let theme = InlineDecorationsTheme::default();
        let spans = vec![src("hello"), src(" world")];
        let inlays: Vec<(usize, &InlayHintData)> = vec![];

        let out = splice_inlays_into_spans(&spans, &inlays, &theme);

        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "hello");
        assert_eq!(out[1].text, " world");
        assert!(out.iter().all(|s| !s.is_virtual));
    }

    /// Two inlays at the same byte offset stack in the order they appear
    /// in the input — e.g. a parameter hint followed by a type hint at
    /// the same source position should appear back-to-back.
    #[test]
    fn coincident_inlays_stack_in_input_order() {
        let theme = InlineDecorationsTheme::default();
        let spans = vec![src("xy")];
        let h1 = hint(0, 1, "[A]", InlayHintKind::Type);
        let h2 = hint(0, 1, "[B]", InlayHintKind::Parameter);
        let inlays = vec![(1usize, &h1), (1usize, &h2)];

        let out = splice_inlays_into_spans(&spans, &inlays, &theme);

        let texts: Vec<&str> = out.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, vec!["x", "[A]", "[B]", "y"]);
    }
}
