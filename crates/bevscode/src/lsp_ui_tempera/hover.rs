//! Hover tooltip renderer.
//!
//! LSP hover responses are CommonMark per the spec, so the popup body
//! is a `bevy_markdown::Markdown` child entity. Plain-text-only servers
//! still render correctly: prose without markdown syntax becomes plain
//! paragraphs through the same parser. Fenced code blocks get
//! tree-sitter syntax highlighting via the `MarkdownHighlighter`
//! resource installed by [`super::LspUiTemperaPlugin`].

use bevy::prelude::*;
use bevy_markdown::Markdown;

use crate::lsp_ui::components::HoverPopupData;
use crate::ui_kit::{markdown_theme_from_chrome, PopupChrome};

use super::anchor::{PopupAnchor, PopupPlacement};
use super::chrome::apply_chrome;

pub fn update_hover_popup(
    mut commands: Commands,
    mut popups: Query<
        (Entity, &HoverPopupData, &mut Node, Option<&Children>),
        Changed<HoverPopupData>,
    >,
    mut markdown_children: Query<&mut Markdown>,
    anchor: PopupAnchor,
    chrome: PopupChrome,
) {
    for (entity, data, mut node, children) in popups.iter_mut() {
        let placed = apply_chrome(
            &mut commands,
            entity,
            &mut node,
            &anchor,
            &chrome,
            data.editor,
            data.line,
            data.character,
            Vec2::new(data.width, data.height),
            PopupPlacement::PreferBelow,
        );
        if placed.is_none() {
            continue;
        }

        // The popup data Component is overwritten every frame the hover
        // is visible (see `sync::sync_hover_popup`), so always despawning
        // and respawning the Markdown child would churn its grandchildren
        // every frame and leave the popup visually empty. Reuse the
        // existing child and only mutate its `source` when the content
        // actually changed.
        let existing_md = children
            .into_iter()
            .flat_map(|c| c.iter())
            .find(|c| markdown_children.get(*c).is_ok());

        if let Some(child) = existing_md {
            if let Ok(mut md) = markdown_children.get_mut(child) {
                if md.source != data.content {
                    md.source = data.content.clone();
                }
            }
        } else {
            let (fonts, colors, spacing, scales) = markdown_theme_from_chrome(&chrome);
            commands.entity(entity).with_children(|p| {
                p.spawn((
                    Markdown { source: data.content.clone() },
                    fonts,
                    colors,
                    spacing,
                    scales,
                    Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        max_width: Val::Px(data.width - chrome.spacing.sm),
                        ..default()
                    },
                ));
            });
        }
    }
}
