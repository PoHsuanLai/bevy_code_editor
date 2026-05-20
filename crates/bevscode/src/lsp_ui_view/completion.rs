//! Completion popup renderer.
//!
//! Reads [`CompletionPopupData`] and (re)builds a vertical list of
//! `Node` items under the popup entity. Position is resolved every
//! frame via [`PopupAnchor`] so the popup tracks the cursor row as the
//! user types or scrolls.

use bevy::prelude::*;

use crate::lsp_ui::components::{CompletionItemData, CompletionPopupData};

use super::anchor::{PopupAnchor, PopupPlacement};
use super::frame::{apply_frame, clear_children};

/// Update the popup `Node`'s position + children on every change to
/// [`CompletionPopupData`]. Despawn is handled upstream by
/// [`crate::lsp_ui::sync::sync_completion_popup`], which despawns the
/// entire popup entity when the completion popup hides.
pub fn update_completion_popup(
    mut commands: Commands,
    mut popups: Query<
        (Entity, &CompletionPopupData, &mut Node, Option<&Children>),
        Changed<CompletionPopupData>,
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

        let item_height = data.height / data.max_visible.max(1) as f32;
        let fg = theme
            .map(|t| t.foreground)
            .unwrap_or(Color::srgb(0.827, 0.827, 0.827));
        let muted = theme
            .map(|t| t.line_numbers)
            .unwrap_or(Color::srgb(0.545, 0.545, 0.545));
        let selected_bg = theme
            .map(|t| t.selection_background)
            .unwrap_or(Color::srgba(0.231, 0.373, 0.604, 0.4));

        commands.entity(entity).with_children(|p| {
            let start = data.scroll_offset;
            let end = (start + data.max_visible).min(data.items.len());
            for (i, item) in data.items[start..end].iter().enumerate() {
                let absolute = start + i;
                let selected = absolute == data.selected_index;
                spawn_item(p, item, selected, item_height, fg, muted, selected_bg);
            }
        });
    }
}

fn spawn_item(
    parent: &mut ChildSpawnerCommands,
    item: &CompletionItemData,
    selected: bool,
    height: f32,
    fg: Color,
    muted: Color,
    selected_bg: Color,
) {
    let font_size = 14.0;
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
                Text::new(&item.label),
                TextFont {
                    font_size,
                    ..default()
                },
                TextColor(fg),
            ));
            if let Some(detail) = &item.detail {
                row.spawn((
                    Text::new(detail),
                    TextFont {
                        font_size,
                        ..default()
                    },
                    TextColor(muted),
                ));
            }
        });
}
