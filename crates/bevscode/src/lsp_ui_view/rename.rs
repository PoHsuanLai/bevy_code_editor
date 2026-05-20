//! Inline rename input renderer.
//!
//! Renders the current rename text plus a thin caret bar at
//! `cursor_position`. Actual character input is funneled through
//! [`crate::lsp_ui::interceptors`], which updates [`LspRenamePopup`]
//! and the sync layer re-emits a fresh [`RenameInputData`].
//!
//! Embedding a real [`bevy_instanced_text_editor::TextEditor`] is a
//! follow-up — the existing input interception path already gives us
//! correct editing semantics on the rename buffer.
//!
//! [`LspRenamePopup`]: crate::lsp_ui::state::LspRenamePopup

use bevy::prelude::*;

use crate::lsp_ui::components::RenameInputData;

use super::anchor::{PopupAnchor, PopupPlacement};
use super::frame::{apply_frame, clear_children};

pub fn update_rename_input(
    mut commands: Commands,
    mut popups: Query<
        (Entity, &RenameInputData, &mut Node, Option<&Children>),
        Changed<RenameInputData>,
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
        let caret = theme
            .map(|t| t.cursor)
            .unwrap_or(Color::srgb(0.933, 0.933, 0.933));

        commands.entity(entity).with_children(|p| {
            p.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(1.0),
                ..default()
            })
            .with_children(|row| {
                let (pre, post) = split_at_char(&data.text, data.cursor_position);
                if !pre.is_empty() {
                    row.spawn((
                        Text::new(pre),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(fg),
                    ));
                }
                row.spawn((
                    Node {
                        width: Val::Px(1.0),
                        height: Val::Px(16.0),
                        ..default()
                    },
                    BackgroundColor(caret),
                ));
                if !post.is_empty() {
                    row.spawn((
                        Text::new(post),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(fg),
                    ));
                }
            });
        });
    }
}

fn split_at_char(s: &str, char_index: usize) -> (&str, &str) {
    let split_byte = s
        .char_indices()
        .nth(char_index)
        .map(|(b, _)| b)
        .unwrap_or(s.len());
    s.split_at(split_byte)
}
