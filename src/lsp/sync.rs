//! Systems that sync LSP state resources to marker component entities.
//!
//! These systems create and update entities with `*PopupData` components
//! based on the current state of LSP resources (CompletionState, HoverState, etc.).
//!
//! The render systems then query these marker components to spawn visual elements.

use bevy::prelude::*;

use crate::settings::EditorSettings;
use crate::types::{CodeEditorState, ViewportDimensions};

use super::components::*;
use super::messages::CodeActionOrCommand;
use super::state::*;

/// Sync completion state to marker entity
pub fn sync_completion_popup(
    mut commands: Commands,
    completion_state: Res<CompletionState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    existing: Query<Entity, With<CompletionPopupData>>,
) {
    let filtered_items = completion_state.filtered_items();

    // If not visible or no items, despawn existing
    if !completion_state.visible || filtered_items.is_empty() {
        for entity in existing.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }

    // Calculate position
    let cursor_pos = editor_state.cursor_pos.min(editor_state.rope.len_chars());
    let line_index = editor_state.rope.char_to_line(cursor_pos);
    let line_start = editor_state.rope.line_to_char(line_index);
    let col_index = cursor_pos - line_start;

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;

    let x_offset = settings.ui.layout.code_margin_left + (col_index as f32 * char_width);
    let y_offset = settings.ui.layout.margin_top
        + editor_state.scroll_offset
        + ((line_index + 1) as f32 * line_height);

    // Calculate dynamic width
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

    let calculated_width = (max_char_count as f32 * char_width) + 20.0;
    let box_width = calculated_width.max(200.0).min(600.0);

    let max_visible = settings.completion.max_visible_items;
    let total_items = filtered_items.len();
    let visible_count = total_items.min(max_visible);
    let box_height = (visible_count as f32 * line_height) + 10.0;

    // Convert items to data
    let items: Vec<CompletionItemData> = filtered_items
        .iter()
        .map(CompletionItemData::from)
        .collect();

    let popup_data = CompletionPopupData {
        position: Vec2::new(x_offset + viewport.offset_x, y_offset),
        items,
        selected_index: completion_state.selected_index,
        scroll_offset: completion_state.scroll_offset,
        max_visible,
        width: box_width,
        height: box_height,
    };

    // Update or spawn entity
    if let Some(entity) = existing.iter().next() {
        commands.entity(entity).insert(popup_data);
    } else {
        commands.spawn((popup_data, LspUiElement, Name::new("CompletionPopup")));
    }

    // Despawn extra entities if any
    for entity in existing.iter().skip(1) {
        commands.entity(entity).despawn();
    }
}

/// Sync hover state to marker entity
pub fn sync_hover_popup(
    mut commands: Commands,
    hover_state: Res<HoverState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    existing: Query<Entity, With<HoverPopupData>>,
) {
    if !hover_state.visible || hover_state.content.is_empty() {
        for entity in existing.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }

    let trigger_char_index = hover_state.trigger_char_index.min(editor_state.rope.len_chars());
    let line_index = editor_state.rope.char_to_line(trigger_char_index);
    let line_start = editor_state.rope.line_to_char(line_index);
    let col_index = trigger_char_index - line_start;

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;

    let x_offset = settings.ui.layout.code_margin_left + (col_index as f32 * char_width);
    let y_offset = settings.ui.layout.margin_top
        + editor_state.scroll_offset
        + ((line_index + 1) as f32 * line_height);

    let font_size = settings.font.size * 0.9;
    let padding = 10.0;

    let max_line_chars = hover_state
        .content
        .lines()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0);
    let hover_char_width = settings.font.char_width * 0.9;

    let calculated_width = (max_line_chars as f32 * hover_char_width) + padding * 2.0;
    let box_width = calculated_width.max(100.0).min(600.0);

    let line_count = hover_state.content.lines().count().max(1);
    let box_height = (line_count as f32 * font_size * 1.2) + padding * 2.0;

    let popup_data = HoverPopupData {
        position: Vec2::new(x_offset + viewport.offset_x, y_offset),
        content: hover_state.content.clone(),
        width: box_width,
        height: box_height,
    };

    if let Some(entity) = existing.iter().next() {
        commands.entity(entity).insert(popup_data);
    } else {
        commands.spawn((popup_data, LspUiElement, Name::new("HoverPopup")));
    }

    for entity in existing.iter().skip(1) {
        commands.entity(entity).despawn();
    }
}

/// Sync signature help state to marker entity
pub fn sync_signature_help_popup(
    mut commands: Commands,
    sig_state: Res<SignatureHelpState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    existing: Query<Entity, With<SignatureHelpPopupData>>,
) {
    if !sig_state.visible || sig_state.signatures.is_empty() {
        for entity in existing.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }

    let Some(signature) = sig_state.current_signature() else {
        for entity in existing.iter() {
            commands.entity(entity).despawn();
        }
        return;
    };

    let cursor_pos = editor_state.cursor_pos.min(editor_state.rope.len_chars());
    let line_index = editor_state.rope.char_to_line(cursor_pos);
    let line_start = editor_state.rope.line_to_char(line_index);
    let col_index = cursor_pos - line_start;

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;

    let x_offset = settings.ui.layout.code_margin_left + (col_index as f32 * char_width);
    let y_offset = settings.ui.layout.margin_top
        + editor_state.scroll_offset
        + (line_index as f32 * line_height)
        - line_height;

    let font_size = settings.font.size * 0.9;
    let padding = 8.0;

    let sig_label = &signature.label;
    let box_width = (sig_label.chars().count() as f32 * char_width * 0.9 + padding * 2.0)
        .max(100.0)
        .min(600.0);
    let box_height = font_size * 1.4 + padding * 2.0;

    // Extract parameter ranges if available
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
                    lsp_types::ParameterLabel::Simple(s) => {
                        sig_label.find(s.as_str()).map(|start| (start, start + s.len()))
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let popup_data = SignatureHelpPopupData {
        position: Vec2::new(x_offset + viewport.offset_x, y_offset),
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
        commands.entity(entity).despawn();
    }
}

/// Sync code action state to marker entity
pub fn sync_code_actions_popup(
    mut commands: Commands,
    action_state: Res<CodeActionState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    existing: Query<Entity, With<CodeActionsPopupData>>,
) {
    if !action_state.visible || action_state.actions.is_empty() {
        for entity in existing.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }

    let cursor_pos = editor_state.cursor_pos.min(editor_state.rope.len_chars());
    let line_index = editor_state.rope.char_to_line(cursor_pos);

    let line_height = settings.font.line_height;
    let char_width = settings.font.char_width;

    let x_offset = settings.ui.layout.code_margin_left - 20.0;
    let y_offset = settings.ui.layout.margin_top
        + editor_state.scroll_offset
        + ((line_index + 1) as f32 * line_height);

    let max_label_len = action_state
        .actions
        .iter()
        .map(|a| match a {
            CodeActionOrCommand::Action(action) => action.title.chars().count(),
            CodeActionOrCommand::Command(cmd) => cmd.title.chars().count(),
        })
        .max()
        .unwrap_or(20);

    let box_width = (max_label_len as f32 * char_width + 20.0).max(200.0).min(400.0);
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
                    (icon, action.title.as_str(), action.is_preferred.unwrap_or(false))
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
        position: Vec2::new(x_offset + viewport.offset_x, y_offset),
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
        commands.entity(entity).despawn();
    }
}

/// Sync rename state to marker entity
pub fn sync_rename_input(
    mut commands: Commands,
    rename_state: Res<RenameState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    existing: Query<Entity, With<RenameInputData>>,
) {
    if !rename_state.visible {
        for entity in existing.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }

    let Some(range) = &rename_state.range else {
        for entity in existing.iter() {
            commands.entity(entity).despawn();
        }
        return;
    };

    let line = range.start.line as usize;
    let character = range.start.character as usize;

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;

    let x_offset = settings.ui.layout.code_margin_left + (character as f32 * char_width);
    let y_offset = settings.ui.layout.margin_top
        + editor_state.scroll_offset
        + (line as f32 * line_height)
        + (line_height / 2.0);

    let padding_x = 4.0;
    let padding_y = 2.0;

    let display_text = if rename_state.new_name.is_empty() {
        &rename_state.original_text
    } else {
        &rename_state.new_name
    };

    let text_width = (display_text.chars().count().max(8) as f32 * char_width) + padding_x * 2.0 + 4.0;
    let box_width = text_width.max(100.0).min(300.0);
    let box_height = line_height + padding_y * 2.0;

    let popup_data = RenameInputData {
        position: Vec2::new(x_offset + viewport.offset_x, y_offset),
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
        commands.entity(entity).despawn();
    }
}

/// Sync inlay hints to marker entities
pub fn sync_inlay_hints(
    mut commands: Commands,
    hint_state: Res<InlayHintState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    existing: Query<Entity, With<InlayHintData>>,
) {
    // Only update if something changed
    if !hint_state.is_changed() && !editor_state.is_changed() && !viewport.is_changed() {
        return;
    }

    // Clear existing hints
    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }

    if hint_state.hints.is_empty() {
        return;
    }

    let visible_start_line = (editor_state.scroll_offset / settings.font.line_height) as u32;
    let visible_lines = (viewport.height as f32 / settings.font.line_height) as u32 + 2;
    let visible_end_line = visible_start_line + visible_lines;

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;

    for hint in &hint_state.hints {
        let line = hint.position.line;
        let character = hint.position.character;

        if line < visible_start_line || line > visible_end_line {
            continue;
        }

        let label_text = match &hint.label {
            lsp_types::InlayHintLabel::String(s) => s.clone(),
            lsp_types::InlayHintLabel::LabelParts(parts) => {
                parts.iter().map(|p| p.value.as_str()).collect::<Vec<_>>().join("")
            }
        };

        let x_offset = settings.ui.layout.code_margin_left + (character as f32 * char_width);
        let y_offset = settings.ui.layout.margin_top
            + editor_state.scroll_offset
            + (line as f32 * line_height)
            + (line_height / 2.0);

        let kind = match hint.kind {
            Some(lsp_types::InlayHintKind::TYPE) => InlayHintKind::Type,
            Some(lsp_types::InlayHintKind::PARAMETER) => InlayHintKind::Parameter,
            _ => InlayHintKind::Other,
        };

        commands.spawn((
            InlayHintData {
                position: Vec2::new(x_offset + viewport.offset_x, y_offset),
                label: label_text,
                kind,
                line,
                character,
            },
            LspUiElement,
            Name::new("InlayHint"),
        ));
    }
}

/// Sync document highlights to marker entities
pub fn sync_document_highlights(
    mut commands: Commands,
    highlight_state: Res<DocumentHighlightState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    existing: Query<Entity, With<DocumentHighlightData>>,
) {
    if !highlight_state.is_changed() && !editor_state.is_changed() && !viewport.is_changed() {
        return;
    }

    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }

    if !highlight_state.visible || highlight_state.highlights.is_empty() {
        return;
    }

    let viewport_height = viewport.height as f32;
    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;

    let visible_start_line = (editor_state.scroll_offset / line_height) as u32;
    let visible_lines = (viewport_height / line_height) as u32 + 2;
    let visible_end_line = visible_start_line + visible_lines;

    for highlight in &highlight_state.highlights {
        let start_line = highlight.range.start.line;
        let end_line = highlight.range.end.line;

        if end_line < visible_start_line || start_line > visible_end_line {
            continue;
        }

        let is_write = matches!(highlight.kind, Some(lsp_types::DocumentHighlightKind::WRITE));

        // Single-line highlight
        if start_line == end_line {
            let line = start_line;
            let start_char = highlight.range.start.character;
            let end_char = highlight.range.end.character;
            let width = (end_char - start_char) as f32 * char_width;

            let x_offset = settings.ui.layout.code_margin_left + (start_char as f32 * char_width);
            let y_offset = settings.ui.layout.margin_top
                + editor_state.scroll_offset
                + (line as f32 * line_height)
                + (line_height / 2.0);

            commands.spawn((
                DocumentHighlightData {
                    position: Vec2::new(x_offset + viewport.offset_x + width / 2.0, y_offset),
                    width,
                    height: line_height,
                    is_write,
                    line,
                },
                LspUiElement,
                Name::new("DocumentHighlight"),
            ));
        } else {
            // Multi-line highlights
            for line in start_line..=end_line {
                if line < visible_start_line || line > visible_end_line {
                    continue;
                }

                let (start_char, end_char) = if line == start_line {
                    (highlight.range.start.character, 1000)
                } else if line == end_line {
                    (0, highlight.range.end.character)
                } else {
                    (0, 1000)
                };

                let width = (end_char - start_char).min(200) as f32 * char_width;
                let x_offset = settings.ui.layout.code_margin_left + (start_char as f32 * char_width);
                let y_offset = settings.ui.layout.margin_top
                    + editor_state.scroll_offset
                    + (line as f32 * line_height)
                    + (line_height / 2.0);

                commands.spawn((
                    DocumentHighlightData {
                        position: Vec2::new(x_offset + viewport.offset_x + width / 2.0, y_offset),
                        width,
                        height: line_height,
                        is_write,
                        line,
                    },
                    LspUiElement,
                    Name::new("DocumentHighlight"),
                ));
            }
        }
    }
}
