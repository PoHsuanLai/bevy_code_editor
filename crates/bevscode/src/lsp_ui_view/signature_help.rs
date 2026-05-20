//! Signature help renderer.
//!
//! Renders the active overload's label with the active-parameter range
//! emphasized, plus a `1/N` pager when multiple overloads exist.

use bevy::prelude::*;

use crate::lsp_ui::components::SignatureHelpPopupData;

use super::anchor::{PopupAnchor, PopupPlacement};
use super::frame::{apply_frame, clear_children};

pub fn update_signature_help_popup(
    mut commands: Commands,
    mut popups: Query<
        (Entity, &SignatureHelpPopupData, &mut Node, Option<&Children>),
        Changed<SignatureHelpPopupData>,
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
            PopupPlacement::PreferAbove,
        );
        clear_children(&mut commands, children);
        if placed.is_none() {
            continue;
        }

        let fg = theme
            .map(|t| t.foreground)
            .unwrap_or(Color::srgb(0.827, 0.827, 0.827));
        let muted = theme
            .map(|t| t.line_numbers)
            .unwrap_or(Color::srgb(0.545, 0.545, 0.545));
        let accent = theme
            .map(|t| t.cursor)
            .unwrap_or(Color::srgb(0.933, 0.933, 0.933));

        let font_size = 13.0;
        let label = &data.label;
        let active_range = data.parameter_ranges.get(data.active_parameter).copied();

        commands.entity(entity).with_children(|p| {
            if data.total_signatures > 1 {
                p.spawn((
                    Text::new(format!(
                        "{}/{}",
                        data.current_index + 1,
                        data.total_signatures
                    )),
                    TextFont {
                        font_size: font_size * 0.85,
                        ..default()
                    },
                    TextColor(muted),
                ));
            }

            // Split the label into [pre][active][post] so the active
            // parameter can render in the accent color. Falls back to a
            // single span when no active range applies.
            p.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(0.0),
                ..default()
            })
            .with_children(|row| match active_range {
                Some((s, e)) if s < e && e <= label.len() => {
                    let pre = &label[..s];
                    let active = &label[s..e];
                    let post = &label[e..];
                    if !pre.is_empty() {
                        row.spawn((
                            Text::new(pre),
                            TextFont {
                                font_size,
                                ..default()
                            },
                            TextColor(fg),
                        ));
                    }
                    row.spawn((
                        Text::new(active),
                        TextFont {
                            font_size,
                            ..default()
                        },
                        TextColor(accent),
                    ));
                    if !post.is_empty() {
                        row.spawn((
                            Text::new(post),
                            TextFont {
                                font_size,
                                ..default()
                            },
                            TextColor(fg),
                        ));
                    }
                }
                _ => {
                    row.spawn((
                        Text::new(label),
                        TextFont {
                            font_size,
                            ..default()
                        },
                        TextColor(fg),
                    ));
                }
            });
        });
    }
}
