//! Code-actions popup renderer.
//!
//! Mirrors the completion popup structure: a vertical list with one row
//! per action, the selected row highlighted. Icons come from the
//! sync layer's pre-resolved Unicode glyphs for now; switching to
//! [`IconAtlas`](crate::plugin::gutter_decorations) is a follow-up.

use bevy::prelude::*;

use crate::lsp_ui::components::{CodeActionItemData, CodeActionsPopupData};

use super::anchor::{PopupAnchor, PopupPlacement};
use super::frame::{apply_frame, clear_children};

pub fn update_code_actions_popup(
    mut commands: Commands,
    mut popups: Query<
        (Entity, &CodeActionsPopupData, &mut Node, Option<&Children>),
        Changed<CodeActionsPopupData>,
    >,
    anchor: PopupAnchor,
) {
    for (entity, data, mut node, children) in popups.iter_mut() {
        let theme = anchor.theme(data.editor);
        let placed = apply_frame(
            &mut commands,
            entity,
            &mut node,
            &anchor,
            theme,
            data.editor,
            data.line,
            data.character,
            Vec2::new(data.width, data.height),
            PopupPlacement::PreferBelow,
        );
        clear_children(&mut commands, children);
        if placed.is_none() {
            continue;
        }

        let fg = theme
            .map(|t| t.foreground)
            .unwrap_or(Color::srgb(0.827, 0.827, 0.827));
        let selected_bg = theme
            .map(|t| t.selection_background)
            .unwrap_or(Color::srgba(0.231, 0.373, 0.604, 0.4));

        let row_count = data.actions.len().clamp(1, 10);
        let row_height = data.height / row_count as f32;

        commands.entity(entity).with_children(|p| {
            for (i, action) in data.actions.iter().enumerate() {
                let selected = i == data.selected_index;
                spawn_action_row(p, action, selected, row_height, fg, selected_bg);
            }
        });
    }
}

fn spawn_action_row(
    parent: &mut ChildSpawnerCommands,
    action: &CodeActionItemData,
    selected: bool,
    height: f32,
    fg: Color,
    selected_bg: Color,
) {
    parent
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(height),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                column_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(if selected { selected_bg } else { Color::NONE }),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(&action.icon),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(fg),
            ));
            row.spawn((
                Text::new(&action.title),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(fg),
            ));
        });
}
