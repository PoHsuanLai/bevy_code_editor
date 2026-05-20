//! Hover tooltip renderer.

use bevy::prelude::*;

use crate::lsp_ui::components::HoverPopupData;

use super::anchor::{PopupAnchor, PopupPlacement};
use super::frame::{apply_frame, clear_children};

pub fn update_hover_popup(
    mut commands: Commands,
    mut popups: Query<
        (Entity, &HoverPopupData, &mut Node, Option<&Children>),
        Changed<HoverPopupData>,
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

        commands.entity(entity).with_children(|p| {
            p.spawn((
                Text::new(&data.content),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(fg),
                Node {
                    max_width: Val::Px(data.width - 8.0),
                    ..default()
                },
            ));
        });
    }
}
