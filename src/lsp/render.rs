//! Default render systems for LSP UI components.
//!
//! These systems query marker components (e.g., `CompletionPopupData`) and spawn
//! visual entities as children. Users can disable these systems and provide their
//! own implementations that query the same marker components.
//!
//! # Replacing Default Rendering
//!
//! To use custom rendering:
//!
//! 1. Configure the plugin to disable default UI systems
//! 2. Add your own systems that query the marker components
//! 3. Optionally use `LspUiTheme` for consistent styling
//!
//! ```rust,ignore
//! app.add_plugins(CodeEditorPlugin::new().with_lsp_ui(false));
//!
//! app.add_systems(Update, my_custom_completion_renderer);
//!
//! fn my_custom_completion_renderer(
//!     query: Query<(Entity, &CompletionPopupData), Changed<CompletionPopupData>>,
//!     theme: Res<LspUiTheme>,
//!     mut commands: Commands,
//! ) {
//!     // Your custom rendering
//! }
//! ```

use bevy::prelude::*;
use bevy::sprite::Anchor;

use crate::settings::FontSettings;
use crate::types::ViewportDimensions;

use super::components::*;
use super::theme::LspUiTheme;
use super::ui::{
    CodeActionUI, CompletionUI, HoverUI, InlayHintText, RenameUI, SignatureHelpUI,
    DocumentHighlightMarker,
};

/// Render the completion popup from marker component data
pub fn render_completion_popup(
    mut commands: Commands,
    popup_query: Query<(Entity, &CompletionPopupData), Changed<CompletionPopupData>>,
    visual_query: Query<Entity, With<CompletionUI>>,
    font: Res<FontSettings>,
    viewport: Res<ViewportDimensions>,
    theme: Res<LspUiTheme>,
) {
    for (_popup_entity, popup) in popup_query.iter() {
        // Clear old visuals
        for entity in visual_query.iter() {
            commands.entity(entity).despawn();
        }

        let viewport_width = viewport.width as f32;
        let viewport_height = viewport.height as f32;
        let line_height = font.line_height;

        let pos = Vec3::new(
            -viewport_width / 2.0 + popup.position.x + popup.width / 2.0,
            viewport_height / 2.0 - popup.position.y - popup.height / 2.0,
            theme.completion.z_index,
        );

        commands
            .spawn((
                Sprite {
                    color: theme.completion.background,
                    custom_size: Some(Vec2::new(popup.width, popup.height)),
                    ..default()
                },
                Transform::from_translation(pos),
                CompletionUI,
                LspUiVisual,
                Name::new("CompletionBox"),
            ))
            .with_children(|parent| {
                let visible_items = popup
                    .items
                    .iter()
                    .skip(popup.scroll_offset)
                    .take(popup.max_visible);

                for (i, item) in visible_items.enumerate() {
                    let absolute_index = popup.scroll_offset + i;
                    let is_selected = absolute_index == popup.selected_index;
                    let item_y = (popup.height / 2.0) - (i as f32 * line_height) - (line_height / 2.0) - 5.0;

                    // Selection background
                    if is_selected {
                        parent.spawn((
                            Sprite {
                                color: theme.completion.selected_background,
                                custom_size: Some(Vec2::new(popup.width - 4.0, line_height)),
                                ..default()
                            },
                            Transform::from_translation(Vec3::new(0.0, item_y, 0.1)),
                            LspUiVisual,
                        ));
                    }

                    // Kind icon
                    parent.spawn((
                        Text2d::new(&item.kind_icon),
                        TextFont {
                            font: font.handle.clone().unwrap_or_default(),
                            font_size: font.size * 0.9,
                            ..default()
                        },
                        TextColor(theme.completion.icon_color),
                        Transform::from_translation(Vec3::new(-popup.width / 2.0 + 12.0, item_y, 0.2)),
                        Anchor::CENTER_LEFT,
                        LspUiVisual,
                    ));

                    // Item label
                    let label_color = if item.is_word {
                        theme.completion.word_text_color
                    } else {
                        theme.completion.text_color
                    };

                    parent.spawn((
                        Text2d::new(&item.label),
                        TextFont {
                            font: font.handle.clone().unwrap_or_default(),
                            font_size: font.size,
                            ..default()
                        },
                        TextColor(label_color),
                        Transform::from_translation(Vec3::new(-popup.width / 2.0 + 28.0, item_y, 0.2)),
                        Anchor::CENTER_LEFT,
                        LspUiVisual,
                    ));

                    // Item detail
                    if let Some(detail) = &item.detail {
                        parent.spawn((
                            Text2d::new(detail),
                            TextFont {
                                font: font.handle.clone().unwrap_or_default(),
                                font_size: font.size * 0.8,
                                ..default()
                            },
                            TextColor(theme.completion.detail_color),
                            Transform::from_translation(Vec3::new(popup.width / 2.0 - 10.0, item_y, 0.2)),
                            Anchor::CENTER_RIGHT,
                            LspUiVisual,
                        ));
                    }
                }
            });
    }
}

/// Render the hover popup from marker component data
pub fn render_hover_popup(
    mut commands: Commands,
    popup_query: Query<(Entity, &HoverPopupData), Changed<HoverPopupData>>,
    visual_query: Query<Entity, With<HoverUI>>,
    font: Res<FontSettings>,
    viewport: Res<ViewportDimensions>,
    theme: Res<LspUiTheme>,
) {
    for (_popup_entity, popup) in popup_query.iter() {
        for entity in visual_query.iter() {
            commands.entity(entity).despawn();
        }

        let viewport_width = viewport.width as f32;
        let viewport_height = viewport.height as f32;

        let pos = Vec3::new(
            -viewport_width / 2.0 + popup.position.x + popup.width / 2.0,
            viewport_height / 2.0 - popup.position.y - popup.height / 2.0,
            theme.hover.z_index,
        );

        commands
            .spawn((
                Sprite {
                    color: theme.hover.background,
                    custom_size: Some(Vec2::new(popup.width, popup.height)),
                    ..default()
                },
                Transform::from_translation(pos),
                HoverUI,
                LspUiVisual,
                Name::new("HoverBox"),
            ))
            .with_children(|parent| {
                let text_x = -popup.width / 2.0 + theme.hover.padding;
                let text_y = popup.height / 2.0 - theme.hover.padding;

                parent.spawn((
                    Text2d::new(&popup.content),
                    TextFont {
                        font: font.handle.clone().unwrap_or_default(),
                        font_size: font.size * 0.9,
                        ..default()
                    },
                    TextColor(theme.hover.text_color),
                    Transform::from_translation(Vec3::new(text_x, text_y, 0.1)),
                    Anchor::TOP_LEFT,
                    LspUiVisual,
                ));
            });
    }
}

/// Render the signature help popup from marker component data
pub fn render_signature_help_popup(
    mut commands: Commands,
    popup_query: Query<(Entity, &SignatureHelpPopupData), Changed<SignatureHelpPopupData>>,
    visual_query: Query<Entity, With<SignatureHelpUI>>,
    font: Res<FontSettings>,
    viewport: Res<ViewportDimensions>,
    theme: Res<LspUiTheme>,
) {
    for (_popup_entity, popup) in popup_query.iter() {
        for entity in visual_query.iter() {
            commands.entity(entity).despawn();
        }

        let viewport_width = viewport.width as f32;
        let viewport_height = viewport.height as f32;

        let pos = Vec3::new(
            -viewport_width / 2.0 + popup.position.x + popup.width / 2.0,
            viewport_height / 2.0 - popup.position.y - popup.height / 2.0,
            theme.signature_help.z_index,
        );

        commands
            .spawn((
                Sprite {
                    color: theme.signature_help.background,
                    custom_size: Some(Vec2::new(popup.width, popup.height)),
                    ..default()
                },
                Transform::from_translation(pos),
                SignatureHelpUI,
                LspUiVisual,
                Name::new("SignatureHelpBox"),
            ))
            .with_children(|parent| {
                let text_x = -popup.width / 2.0 + theme.signature_help.padding;

                parent.spawn((
                    Text2d::new(&popup.label),
                    TextFont {
                        font: font.handle.clone().unwrap_or_default(),
                        font_size: font.size * 0.9,
                        ..default()
                    },
                    TextColor(theme.signature_help.text_color),
                    Transform::from_translation(Vec3::new(text_x, 0.0, 0.1)),
                    Anchor::CENTER_LEFT,
                    LspUiVisual,
                ));

                // Show active signature indicator if multiple
                if popup.total_signatures > 1 {
                    let indicator = format!("{}/{}", popup.current_index + 1, popup.total_signatures);
                    parent.spawn((
                        Text2d::new(indicator),
                        TextFont {
                            font: font.handle.clone().unwrap_or_default(),
                            font_size: font.size * 0.72,
                            ..default()
                        },
                        TextColor(theme.signature_help.counter_color),
                        Transform::from_translation(Vec3::new(
                            popup.width / 2.0 - theme.signature_help.padding,
                            0.0,
                            0.1,
                        )),
                        Anchor::CENTER_RIGHT,
                        LspUiVisual,
                    ));
                }
            });
    }
}

/// Render the code actions popup from marker component data
pub fn render_code_actions_popup(
    mut commands: Commands,
    popup_query: Query<(Entity, &CodeActionsPopupData), Changed<CodeActionsPopupData>>,
    visual_query: Query<Entity, With<CodeActionUI>>,
    font: Res<FontSettings>,
    viewport: Res<ViewportDimensions>,
    theme: Res<LspUiTheme>,
) {
    for (_popup_entity, popup) in popup_query.iter() {
        for entity in visual_query.iter() {
            commands.entity(entity).despawn();
        }

        let viewport_width = viewport.width as f32;
        let viewport_height = viewport.height as f32;
        let line_height = font.line_height;

        let pos = Vec3::new(
            -viewport_width / 2.0 + popup.position.x + popup.width / 2.0,
            viewport_height / 2.0 - popup.position.y - popup.height / 2.0,
            theme.code_actions.z_index,
        );

        commands
            .spawn((
                Sprite {
                    color: theme.code_actions.background,
                    custom_size: Some(Vec2::new(popup.width, popup.height)),
                    ..default()
                },
                Transform::from_translation(pos),
                CodeActionUI,
                LspUiVisual,
                Name::new("CodeActionBox"),
            ))
            .with_children(|parent| {
                for (i, action) in popup.actions.iter().enumerate() {
                    let is_selected = i == popup.selected_index;
                    let item_y = (popup.height / 2.0) - (i as f32 * line_height) - (line_height / 2.0) - 5.0;

                    if is_selected {
                        parent.spawn((
                            Sprite {
                                color: theme.code_actions.selected_background,
                                custom_size: Some(Vec2::new(popup.width - 4.0, line_height)),
                                ..default()
                            },
                            Transform::from_translation(Vec3::new(0.0, item_y, 0.1)),
                            LspUiVisual,
                        ));
                    }

                    parent.spawn((
                        Text2d::new(format!("{} {}", action.icon, action.title)),
                        TextFont {
                            font: font.handle.clone().unwrap_or_default(),
                            font_size: font.size,
                            ..default()
                        },
                        TextColor(theme.code_actions.text_color),
                        Transform::from_translation(Vec3::new(-popup.width / 2.0 + 10.0, item_y, 0.2)),
                        Anchor::CENTER_LEFT,
                        LspUiVisual,
                    ));
                }
            });
    }
}

/// Render the rename input from marker component data
pub fn render_rename_input(
    mut commands: Commands,
    popup_query: Query<(Entity, &RenameInputData), Changed<RenameInputData>>,
    visual_query: Query<Entity, With<RenameUI>>,
    font: Res<FontSettings>,
    viewport: Res<ViewportDimensions>,
    theme: Res<LspUiTheme>,
) {
    for (_popup_entity, popup) in popup_query.iter() {
        for entity in visual_query.iter() {
            commands.entity(entity).despawn();
        }

        let viewport_width = viewport.width as f32;
        let viewport_height = viewport.height as f32;
        let char_width = font.char_width;

        let pos = Vec3::new(
            -viewport_width / 2.0 + popup.position.x,
            viewport_height / 2.0 - popup.position.y,
            theme.rename.z_index,
        );

        commands
            .spawn((
                Sprite {
                    color: theme.rename.background,
                    custom_size: Some(Vec2::new(popup.width, popup.height)),
                    ..default()
                },
                Transform::from_translation(Vec3::new(pos.x + popup.width / 2.0, pos.y, pos.z)),
                RenameUI,
                LspUiVisual,
                Name::new("RenameInput"),
            ))
            .with_children(|parent| {
                // Blue border
                parent.spawn((
                    Sprite {
                        color: theme.rename.border,
                        custom_size: Some(Vec2::new(
                            popup.width + theme.rename.border_width * 2.0,
                            popup.height + theme.rename.border_width * 2.0,
                        )),
                        ..default()
                    },
                    Transform::from_translation(Vec3::new(0.0, 0.0, -0.1)),
                    LspUiVisual,
                ));

                // Input text
                parent.spawn((
                    Text2d::new(&popup.text),
                    TextFont {
                        font: font.handle.clone().unwrap_or_default(),
                        font_size: font.size,
                        ..default()
                    },
                    TextColor(theme.rename.text_color),
                    Transform::from_translation(Vec3::new(
                        -popup.width / 2.0 + theme.rename.padding_x + 2.0,
                        0.0,
                        0.2,
                    )),
                    Anchor::CENTER_LEFT,
                    LspUiVisual,
                ));

                // Text cursor at end of text
                let cursor_x = -popup.width / 2.0
                    + theme.rename.padding_x
                    + 2.0
                    + (popup.cursor_position as f32 * char_width);
                parent.spawn((
                    Sprite {
                        color: theme.rename.cursor_color,
                        custom_size: Some(Vec2::new(1.5, font.size * 0.9)),
                        ..default()
                    },
                    Transform::from_translation(Vec3::new(cursor_x, 0.0, 0.3)),
                    LspUiVisual,
                ));
            });
    }
}

/// Render inlay hints from marker component data
pub fn render_inlay_hints(
    mut commands: Commands,
    hint_query: Query<(Entity, &InlayHintData), Added<InlayHintData>>,
    font: Res<FontSettings>,
    viewport: Res<ViewportDimensions>,
    theme: Res<LspUiTheme>,
) {
    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    for (entity, hint) in hint_query.iter() {
        let color = match hint.kind {
            InlayHintKind::Type => theme.inlay_hints.type_color,
            InlayHintKind::Parameter => theme.inlay_hints.parameter_color,
            InlayHintKind::Other => theme.inlay_hints.default_color,
        };

        let pos = Vec3::new(
            -viewport_width / 2.0 + hint.position.x,
            viewport_height / 2.0 - hint.position.y,
            theme.inlay_hints.z_index,
        );

        commands.entity(entity).insert((
            Text2d::new(&hint.label),
            TextFont {
                font: font.handle.clone().unwrap_or_default(),
                font_size: font.size * theme.inlay_hints.font_size_multiplier,
                ..default()
            },
            TextColor(color),
            Transform::from_translation(pos),
            Anchor::CENTER_LEFT,
            InlayHintText {
                line: hint.line,
                character: hint.character,
            },
            LspUiVisual,
        ));
    }
}

/// Render document highlights from marker component data
pub fn render_document_highlights(
    mut commands: Commands,
    highlight_query: Query<(Entity, &DocumentHighlightData), Added<DocumentHighlightData>>,
    viewport: Res<ViewportDimensions>,
    theme: Res<LspUiTheme>,
) {
    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    for (entity, highlight) in highlight_query.iter() {
        let color = if highlight.is_write {
            theme.document_highlights.write_color
        } else {
            theme.document_highlights.read_color
        };

        let pos = Vec3::new(
            -viewport_width / 2.0 + highlight.position.x,
            viewport_height / 2.0 - highlight.position.y,
            5.0, // Behind text
        );

        commands.entity(entity).insert((
            Sprite {
                color,
                custom_size: Some(Vec2::new(highlight.width, highlight.height)),
                ..default()
            },
            Transform::from_translation(pos),
            DocumentHighlightMarker { line: highlight.line },
            LspUiVisual,
        ));
    }
}

/// Clean up visual entities when marker entities are removed
pub fn cleanup_lsp_ui_visuals(
    mut commands: Commands,
    removed_completion: RemovedComponents<CompletionPopupData>,
    removed_hover: RemovedComponents<HoverPopupData>,
    removed_signature: RemovedComponents<SignatureHelpPopupData>,
    removed_code_actions: RemovedComponents<CodeActionsPopupData>,
    removed_rename: RemovedComponents<RenameInputData>,
    completion_visuals: Query<Entity, With<CompletionUI>>,
    hover_visuals: Query<Entity, With<HoverUI>>,
    signature_visuals: Query<Entity, With<SignatureHelpUI>>,
    code_action_visuals: Query<Entity, With<CodeActionUI>>,
    rename_visuals: Query<Entity, With<RenameUI>>,
) {
    if !removed_completion.is_empty() {
        for entity in completion_visuals.iter() {
            commands.entity(entity).despawn();
        }
    }
    if !removed_hover.is_empty() {
        for entity in hover_visuals.iter() {
            commands.entity(entity).despawn();
        }
    }
    if !removed_signature.is_empty() {
        for entity in signature_visuals.iter() {
            commands.entity(entity).despawn();
        }
    }
    if !removed_code_actions.is_empty() {
        for entity in code_action_visuals.iter() {
            commands.entity(entity).despawn();
        }
    }
    if !removed_rename.is_empty() {
        for entity in rename_visuals.iter() {
            commands.entity(entity).despawn();
        }
    }
}
