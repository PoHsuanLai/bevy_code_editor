//! LSP UI components (completion popup, hover, signature help, etc.)

use bevy::prelude::*;
use bevy::sprite::Anchor;

use crate::settings::*;
use crate::types::{CodeEditorState, ViewportDimensions};

use super::state::{
    CodeActionState, CompletionState, DocumentHighlightState, HoverState, InlayHintState,
    RenameState, SignatureHelpState,
};

/// Marker for the completion UI root entity
#[derive(Component)]
pub struct CompletionUI;

/// Marker for the hover UI root entity
#[derive(Component)]
pub struct HoverUI;

/// Marker for signature help UI
#[derive(Component)]
pub struct SignatureHelpUI;

/// Marker for code action UI
#[derive(Component)]
pub struct CodeActionUI;

/// Marker for inlay hint text
#[derive(Component)]
pub struct InlayHintText {
    pub line: u32,
    pub character: u32,
}

/// Marker for document highlight
#[derive(Component)]
pub struct DocumentHighlightMarker {
    pub line: u32,
}

/// Marker for rename dialog UI
#[derive(Component)]
pub struct RenameUI;

/// Render the auto-completion UI
pub fn update_completion_ui(
    mut commands: Commands,
    completion_state: Res<CompletionState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    ui_query: Query<Entity, With<CompletionUI>>,
) {
    let filtered_items = completion_state.filtered_items();

    // If not visible or no filtered items, clear and return
    if !completion_state.visible || filtered_items.is_empty() {
        for entity in ui_query.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }

    // Skip update if nothing changed
    if !completion_state.is_changed()
        && !editor_state.is_changed()
        && !viewport.is_changed()
        && !settings.is_changed()
    {
        return;
    }

    // Clear old UI
    for entity in ui_query.iter() {
        commands.entity(entity).despawn();
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

    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    // Calculate dynamic width
    let max_char_count = filtered_items
        .iter()
        .take(10)
        .map(|item| {
            let label_len = item.label().chars().count();
            let detail_len = item.detail().map(|d| d.chars().count()).unwrap_or(0);
            label_len + detail_len + 7 // +7 for icon and spacing
        })
        .max()
        .unwrap_or(20);

    let calculated_width = (max_char_count as f32 * char_width) + 20.0;
    let box_width = calculated_width.max(200.0).min(600.0);

    let max_visible = settings.completion.max_visible_items;
    let total_items = filtered_items.len();
    let visible_count = total_items.min(max_visible);
    let box_height = (visible_count as f32 * line_height) + 10.0;

    let pos = Vec3::new(
        -viewport_width / 2.0 + x_offset + viewport.offset_x + box_width / 2.0,
        viewport_height / 2.0 - y_offset - box_height / 2.0,
        100.0,
    );

    commands
        .spawn((
            Sprite {
                color: Color::srgba(0.15, 0.15, 0.15, 0.95),
                custom_size: Some(Vec2::new(box_width, box_height)),
                ..default()
            },
            Transform::from_translation(pos),
            CompletionUI,
            Name::new("CompletionBox"),
        ))
        .with_children(|parent| {
            let scroll_offset = completion_state.scroll_offset;
            let visible_items = filtered_items.iter().skip(scroll_offset).take(max_visible);

            for (i, item) in visible_items.enumerate() {
                let absolute_index = scroll_offset + i;
                let is_selected = absolute_index == completion_state.selected_index;
                let bg_color = if is_selected {
                    Color::srgba(0.2, 0.4, 0.8, 0.8)
                } else {
                    Color::NONE
                };

                let item_y = (box_height / 2.0) - (i as f32 * line_height) - (line_height / 2.0) - 5.0;

                // Selection background
                if is_selected {
                    parent.spawn((
                        Sprite {
                            color: bg_color,
                            custom_size: Some(Vec2::new(box_width - 4.0, line_height)),
                            ..default()
                        },
                        Transform::from_translation(Vec3::new(0.0, item_y, 0.1)),
                    ));
                }

                // Kind icon
                parent.spawn((
                    Text2d::new(item.kind_icon()),
                    TextFont {
                        font: settings.font.handle.clone().unwrap_or_default(),
                        font_size: settings.font.size * 0.9,
                        ..default()
                    },
                    TextColor(Color::srgba(0.6, 0.6, 0.6, 1.0)),
                    Transform::from_translation(Vec3::new(-box_width / 2.0 + 12.0, item_y, 0.2)),
                    Anchor::CENTER_LEFT,
                ));

                // Item label
                let label_color = if item.is_word() {
                    Color::srgba(0.9, 0.9, 0.8, 1.0)
                } else {
                    Color::WHITE
                };

                parent.spawn((
                    Text2d::new(item.label()),
                    TextFont {
                        font: settings.font.handle.clone().unwrap_or_default(),
                        font_size: settings.font.size,
                        ..default()
                    },
                    TextColor(label_color),
                    Transform::from_translation(Vec3::new(-box_width / 2.0 + 28.0, item_y, 0.2)),
                    Anchor::CENTER_LEFT,
                ));

                // Item detail
                if let Some(detail) = item.detail() {
                    parent.spawn((
                        Text2d::new(detail),
                        TextFont {
                            font: settings.font.handle.clone().unwrap_or_default(),
                            font_size: settings.font.size * 0.8,
                            ..default()
                        },
                        TextColor(Color::srgba(0.7, 0.7, 0.7, 1.0)),
                        Transform::from_translation(Vec3::new(box_width / 2.0 - 10.0, item_y, 0.2)),
                        Anchor::CENTER_RIGHT,
                    ));
                }
            }
        });
}

/// Render the hover UI
pub fn update_hover_ui(
    mut commands: Commands,
    hover_state: Res<HoverState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    ui_query: Query<Entity, With<HoverUI>>,
) {
    if !hover_state.visible || hover_state.content.is_empty() {
        for entity in ui_query.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }

    if !hover_state.is_changed()
        && !editor_state.is_changed()
        && !viewport.is_changed()
        && !settings.is_changed()
    {
        return;
    }

    for entity in ui_query.iter() {
        commands.entity(entity).despawn();
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

    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

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

    let pos = Vec3::new(
        -viewport_width / 2.0 + x_offset + viewport.offset_x + box_width / 2.0,
        viewport_height / 2.0 - y_offset - box_height / 2.0,
        100.0,
    );

    commands
        .spawn((
            Sprite {
                color: Color::srgba(0.1, 0.1, 0.1, 0.95),
                custom_size: Some(Vec2::new(box_width, box_height)),
                ..default()
            },
            Transform::from_translation(pos),
            HoverUI,
            Name::new("HoverBox"),
        ))
        .with_children(|parent| {
            let text_x = -box_width / 2.0 + padding;
            let text_y = box_height / 2.0 - padding;

            parent.spawn((
                Text2d::new(hover_state.content.clone()),
                TextFont {
                    font: settings.font.handle.clone().unwrap_or_default(),
                    font_size,
                    ..default()
                },
                TextColor(Color::WHITE),
                Transform::from_translation(Vec3::new(text_x, text_y, 0.1)),
                Anchor::TOP_LEFT,
            ));
        });
}

/// Render signature help UI
pub fn update_signature_help_ui(
    mut commands: Commands,
    sig_state: Res<SignatureHelpState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    ui_query: Query<Entity, With<SignatureHelpUI>>,
) {
    if !sig_state.visible || sig_state.signatures.is_empty() {
        for entity in ui_query.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }

    if !sig_state.is_changed()
        && !editor_state.is_changed()
        && !viewport.is_changed()
        && !settings.is_changed()
    {
        return;
    }

    for entity in ui_query.iter() {
        commands.entity(entity).despawn();
    }

    let Some(signature) = sig_state.current_signature() else {
        return;
    };

    // Position above cursor
    let cursor_pos = editor_state.cursor_pos.min(editor_state.rope.len_chars());
    let line_index = editor_state.rope.char_to_line(cursor_pos);
    let line_start = editor_state.rope.line_to_char(line_index);
    let col_index = cursor_pos - line_start;

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;

    let x_offset = settings.ui.layout.code_margin_left + (col_index as f32 * char_width);
    // Position ABOVE the current line
    let y_offset = settings.ui.layout.margin_top
        + editor_state.scroll_offset
        + (line_index as f32 * line_height)
        - line_height;

    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    let font_size = settings.font.size * 0.9;
    let padding = 8.0;

    // Build signature text with highlighted parameter
    let sig_label = &signature.label;
    let box_width = (sig_label.chars().count() as f32 * char_width * 0.9 + padding * 2.0)
        .max(100.0)
        .min(600.0);
    let box_height = font_size * 1.4 + padding * 2.0;

    let pos = Vec3::new(
        -viewport_width / 2.0 + x_offset + viewport.offset_x + box_width / 2.0,
        viewport_height / 2.0 - y_offset - box_height / 2.0,
        100.0,
    );

    commands
        .spawn((
            Sprite {
                color: Color::srgba(0.12, 0.12, 0.12, 0.95),
                custom_size: Some(Vec2::new(box_width, box_height)),
                ..default()
            },
            Transform::from_translation(pos),
            SignatureHelpUI,
            Name::new("SignatureHelpBox"),
        ))
        .with_children(|parent| {
            let text_x = -box_width / 2.0 + padding;

            parent.spawn((
                Text2d::new(sig_label.clone()),
                TextFont {
                    font: settings.font.handle.clone().unwrap_or_default(),
                    font_size,
                    ..default()
                },
                TextColor(Color::WHITE),
                Transform::from_translation(Vec3::new(text_x, 0.0, 0.1)),
                Anchor::CENTER_LEFT,
            ));

            // Show active signature indicator if multiple
            if sig_state.signatures.len() > 1 {
                let indicator = format!("{}/{}", sig_state.active_signature + 1, sig_state.signatures.len());
                parent.spawn((
                    Text2d::new(indicator),
                    TextFont {
                        font: settings.font.handle.clone().unwrap_or_default(),
                        font_size: font_size * 0.8,
                        ..default()
                    },
                    TextColor(Color::srgba(0.6, 0.6, 0.6, 1.0)),
                    Transform::from_translation(Vec3::new(box_width / 2.0 - padding, 0.0, 0.1)),
                    Anchor::CENTER_RIGHT,
                ));
            }
        });
}

/// Render code action UI (lightbulb menu)
pub fn update_code_action_ui(
    mut commands: Commands,
    action_state: Res<CodeActionState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    ui_query: Query<Entity, With<CodeActionUI>>,
) {
    if !action_state.visible || action_state.actions.is_empty() {
        for entity in ui_query.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }

    if !action_state.is_changed()
        && !editor_state.is_changed()
        && !viewport.is_changed()
        && !settings.is_changed()
    {
        return;
    }

    for entity in ui_query.iter() {
        commands.entity(entity).despawn();
    }

    let cursor_pos = editor_state.cursor_pos.min(editor_state.rope.len_chars());
    let line_index = editor_state.rope.char_to_line(cursor_pos);

    let line_height = settings.font.line_height;
    let char_width = settings.font.char_width;

    // Position at line start (gutter area)
    let x_offset = settings.ui.layout.code_margin_left - 20.0;
    let y_offset = settings.ui.layout.margin_top
        + editor_state.scroll_offset
        + ((line_index + 1) as f32 * line_height);

    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    // Calculate box dimensions
    let max_label_len = action_state
        .actions
        .iter()
        .map(|a| match a {
            super::messages::CodeActionOrCommand::Action(action) => action.title.chars().count(),
            super::messages::CodeActionOrCommand::Command(cmd) => cmd.title.chars().count(),
        })
        .max()
        .unwrap_or(20);

    let box_width = (max_label_len as f32 * char_width + 20.0).max(200.0).min(400.0);
    let visible_count = action_state.actions.len().min(10);
    let box_height = (visible_count as f32 * line_height) + 10.0;

    let pos = Vec3::new(
        -viewport_width / 2.0 + x_offset + viewport.offset_x + box_width / 2.0,
        viewport_height / 2.0 - y_offset - box_height / 2.0,
        100.0,
    );

    commands
        .spawn((
            Sprite {
                color: Color::srgba(0.15, 0.15, 0.15, 0.95),
                custom_size: Some(Vec2::new(box_width, box_height)),
                ..default()
            },
            Transform::from_translation(pos),
            CodeActionUI,
            Name::new("CodeActionBox"),
        ))
        .with_children(|parent| {
            for (i, action) in action_state.actions.iter().take(10).enumerate() {
                let is_selected = i == action_state.selected_index;
                let item_y = (box_height / 2.0) - (i as f32 * line_height) - (line_height / 2.0) - 5.0;

                if is_selected {
                    parent.spawn((
                        Sprite {
                            color: Color::srgba(0.2, 0.4, 0.8, 0.8),
                            custom_size: Some(Vec2::new(box_width - 4.0, line_height)),
                            ..default()
                        },
                        Transform::from_translation(Vec3::new(0.0, item_y, 0.1)),
                    ));
                }

                let (icon, title) = match action {
                    super::messages::CodeActionOrCommand::Action(a) => {
                        let icon = match &a.kind {
                            Some(kind) if kind.as_str().starts_with("quickfix") => "🔧",
                            Some(kind) if kind.as_str().starts_with("refactor") => "✨",
                            Some(kind) if kind.as_str().starts_with("source") => "📁",
                            _ => "💡",
                        };
                        (icon, a.title.as_str())
                    }
                    super::messages::CodeActionOrCommand::Command(c) => ("⚡", c.title.as_str()),
                };

                parent.spawn((
                    Text2d::new(format!("{} {}", icon, title)),
                    TextFont {
                        font: settings.font.handle.clone().unwrap_or_default(),
                        font_size: settings.font.size,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                    Transform::from_translation(Vec3::new(-box_width / 2.0 + 10.0, item_y, 0.2)),
                    Anchor::CENTER_LEFT,
                ));
            }
        });
}

/// Render inlay hints
pub fn update_inlay_hints_ui(
    mut commands: Commands,
    hint_state: Res<InlayHintState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    hint_query: Query<Entity, With<InlayHintText>>,
) {
    // Clear existing hints if state changed
    if hint_state.is_changed() || editor_state.is_changed() || viewport.is_changed() {
        for entity in hint_query.iter() {
            commands.entity(entity).despawn();
        }
    }

    if hint_state.hints.is_empty() {
        return;
    }

    // Only render hints in visible viewport
    let visible_start_line = (editor_state.scroll_offset / settings.font.line_height) as u32;
    let visible_lines = (viewport.height as f32 / settings.font.line_height) as u32 + 2;
    let visible_end_line = visible_start_line + visible_lines;

    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;
    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;

    for hint in &hint_state.hints {
        let line = hint.position.line;
        let character = hint.position.character;

        // Skip hints outside visible range
        if line < visible_start_line || line > visible_end_line {
            continue;
        }

        // Get hint label text
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

        let pos = Vec3::new(
            -viewport_width / 2.0 + x_offset + viewport.offset_x,
            viewport_height / 2.0 - y_offset,
            50.0, // Below cursor but above text
        );

        // Determine color based on hint kind
        let color = match hint.kind {
            Some(lsp_types::InlayHintKind::TYPE) => Color::srgba(0.5, 0.7, 0.9, 0.7),
            Some(lsp_types::InlayHintKind::PARAMETER) => Color::srgba(0.7, 0.6, 0.9, 0.7),
            _ => Color::srgba(0.6, 0.6, 0.6, 0.7),
        };

        commands.spawn((
            Text2d::new(label_text),
            TextFont {
                font: settings.font.handle.clone().unwrap_or_default(),
                font_size: settings.font.size * 0.85,
                ..default()
            },
            TextColor(color),
            Transform::from_translation(pos),
            Anchor::CENTER_LEFT,
            InlayHintText { line, character },
            Name::new("InlayHint"),
        ));
    }
}

/// Render document highlights (all occurrences of symbol under cursor)
pub fn update_document_highlights_ui(
    mut commands: Commands,
    highlight_state: Res<DocumentHighlightState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    highlight_query: Query<Entity, With<DocumentHighlightMarker>>,
) {
    // Clear existing highlights if state changed
    if highlight_state.is_changed() || editor_state.is_changed() || viewport.is_changed() {
        for entity in highlight_query.iter() {
            commands.entity(entity).despawn();
        }
    }

    if !highlight_state.visible || highlight_state.highlights.is_empty() {
        return;
    }

    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;
    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;

    // Calculate visible line range
    let visible_start_line = (editor_state.scroll_offset / line_height) as u32;
    let visible_lines = (viewport_height / line_height) as u32 + 2;
    let visible_end_line = visible_start_line + visible_lines;

    for highlight in &highlight_state.highlights {
        let start_line = highlight.range.start.line;
        let end_line = highlight.range.end.line;

        // Skip highlights outside visible range
        if end_line < visible_start_line || start_line > visible_end_line {
            continue;
        }

        // Determine highlight color based on kind
        let color = match highlight.kind {
            Some(lsp_types::DocumentHighlightKind::WRITE) => {
                Color::srgba(0.8, 0.5, 0.3, 0.3) // Orange for write
            }
            Some(lsp_types::DocumentHighlightKind::READ) | _ => {
                Color::srgba(0.5, 0.6, 0.8, 0.25) // Blue for read
            }
        };

        // For single-line highlights
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

            let pos = Vec3::new(
                -viewport_width / 2.0 + x_offset + viewport.offset_x + width / 2.0,
                viewport_height / 2.0 - y_offset,
                5.0, // Behind text but visible
            );

            commands.spawn((
                Sprite {
                    color,
                    custom_size: Some(Vec2::new(width, line_height)),
                    ..default()
                },
                Transform::from_translation(pos),
                DocumentHighlightMarker { line },
                Name::new("DocumentHighlight"),
            ));
        } else {
            // Multi-line highlights: render each line separately
            for line in start_line..=end_line {
                if line < visible_start_line || line > visible_end_line {
                    continue;
                }

                let (start_char, end_char) = if line == start_line {
                    (highlight.range.start.character, 1000) // To end of line
                } else if line == end_line {
                    (0, highlight.range.end.character)
                } else {
                    (0, 1000) // Entire line
                };

                let width = (end_char - start_char).min(200) as f32 * char_width;
                let x_offset = settings.ui.layout.code_margin_left + (start_char as f32 * char_width);
                let y_offset = settings.ui.layout.margin_top
                    + editor_state.scroll_offset
                    + (line as f32 * line_height)
                    + (line_height / 2.0);

                let pos = Vec3::new(
                    -viewport_width / 2.0 + x_offset + viewport.offset_x + width / 2.0,
                    viewport_height / 2.0 - y_offset,
                    5.0,
                );

                commands.spawn((
                    Sprite {
                        color,
                        custom_size: Some(Vec2::new(width, line_height)),
                        ..default()
                    },
                    Transform::from_translation(pos),
                    DocumentHighlightMarker { line },
                    Name::new("DocumentHighlight"),
                ));
            }
        }
    }
}

/// Render rename dialog UI (VSCode-style inline input)
pub fn update_rename_ui(
    mut commands: Commands,
    rename_state: Res<RenameState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    ui_query: Query<Entity, With<RenameUI>>,
) {
    if !rename_state.visible {
        for entity in ui_query.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }

    if !rename_state.is_changed()
        && !editor_state.is_changed()
        && !viewport.is_changed()
        && !settings.is_changed()
    {
        return;
    }

    for entity in ui_query.iter() {
        commands.entity(entity).despawn();
    }

    let Some(range) = &rename_state.range else {
        return;
    };

    let line = range.start.line as usize;
    let character = range.start.character as usize;

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;

    // Position the input box directly at the symbol location
    let x_offset = settings.ui.layout.code_margin_left + (character as f32 * char_width);
    let y_offset = settings.ui.layout.margin_top
        + editor_state.scroll_offset
        + (line as f32 * line_height)
        + (line_height / 2.0);

    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    let font_size = settings.font.size;
    let padding_x = 4.0;
    let padding_y = 2.0;

    // Calculate box size based on input text
    let display_text = if rename_state.new_name.is_empty() {
        rename_state.original_text.clone()
    } else {
        rename_state.new_name.clone()
    };

    // Width based on text length, with minimum width
    let text_width = (display_text.chars().count().max(8) as f32 * char_width) + padding_x * 2.0 + 4.0;
    let box_width = text_width.max(100.0).min(300.0);
    let box_height = line_height + padding_y * 2.0;

    let pos = Vec3::new(
        -viewport_width / 2.0 + x_offset + viewport.offset_x,
        viewport_height / 2.0 - y_offset,
        150.0, // Above everything
    );

    commands
        .spawn((
            Sprite {
                color: Color::srgba(0.15, 0.15, 0.18, 1.0), // Dark background
                custom_size: Some(Vec2::new(box_width, box_height)),
                ..default()
            },
            Transform::from_translation(Vec3::new(pos.x + box_width / 2.0, pos.y, pos.z)),
            RenameUI,
            Name::new("RenameInput"),
        ))
        .with_children(|parent| {
            // Blue border (VSCode uses blue for focused inputs)
            parent.spawn((
                Sprite {
                    color: Color::srgba(0.0, 0.48, 0.8, 1.0), // VSCode blue
                    custom_size: Some(Vec2::new(box_width + 2.0, box_height + 2.0)),
                    ..default()
                },
                Transform::from_translation(Vec3::new(0.0, 0.0, -0.1)),
            ));

            // Input text
            parent.spawn((
                Text2d::new(display_text.clone()),
                TextFont {
                    font: settings.font.handle.clone().unwrap_or_default(),
                    font_size,
                    ..default()
                },
                TextColor(Color::WHITE),
                Transform::from_translation(Vec3::new(
                    -box_width / 2.0 + padding_x + 2.0,
                    0.0,
                    0.2,
                )),
                Anchor::CENTER_LEFT,
            ));

            // Text cursor at end of text
            let cursor_x = -box_width / 2.0 + padding_x + 2.0 + (display_text.chars().count() as f32 * char_width);
            parent.spawn((
                Sprite {
                    color: Color::WHITE,
                    custom_size: Some(Vec2::new(1.5, font_size * 0.9)),
                    ..default()
                },
                Transform::from_translation(Vec3::new(cursor_x, 0.0, 0.3)),
            ));
        });
}
