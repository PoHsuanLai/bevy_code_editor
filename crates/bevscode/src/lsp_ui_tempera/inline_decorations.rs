//! Inline LSP decorations: inlay hints (sprite text) and document
//! highlights (engine overlay rects).
//!
//! These don't go through `bevy_ui`. Routing them through the engine's
//! `Text2d` / `RectOverlay` paths means they share the same draw call
//! and instance buffer as the editor's glyphs, so they stay pixel-
//! aligned under any clip-projection convention. Sprites parented under
//! the editor's UI `Node` would land on slightly different pixels and
//! drift by ~half a row, which is why the engine exposes the overlay
//! path for selection / cursor decorations in the first place.

use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy_instanced_text::{
    CornerRadii, DisplayLayout, MonoCellWidth, RectOverlay, RowMetricsParam, RowVertical,
    TextOverlays,
};

use crate::lsp_ui::components::{
    DocumentHighlightData, InlayHintData, InlayHintKind, LspUiVisual,
};
use crate::types::CodeEditor;

/// Per-editor styling for inline LSP decorations. Hosts override by
/// `app.insert_resource(InlineDecorationsTheme { .. })`.
#[derive(Resource, Clone, Debug)]
pub struct InlineDecorationsTheme {
    pub inlay_type: Color,
    pub inlay_parameter: Color,
    pub inlay_other: Color,
    /// Multiplier applied to the editor's font size for inlay hint
    /// glyphs.
    pub inlay_font_scale: f32,
    pub inlay_z: f32,
    pub highlight_read: Color,
    pub highlight_write: Color,
}

impl Default for InlineDecorationsTheme {
    fn default() -> Self {
        Self {
            inlay_type: Color::srgba(0.5, 0.7, 0.9, 0.7),
            inlay_parameter: Color::srgba(0.7, 0.6, 0.9, 0.7),
            inlay_other: Color::srgba(0.6, 0.6, 0.6, 0.7),
            inlay_font_scale: 0.85,
            inlay_z: 50.0,
            highlight_read: Color::srgba(0.5, 0.6, 0.8, 0.25),
            highlight_write: Color::srgba(0.8, 0.5, 0.3, 0.3),
        }
    }
}

/// Marker on the spawned `Text2d` for each inlay hint. Despawn is
/// handled by `sync_inlay_hints` (it drains and re-spawns the marker
/// entities each frame data changes).
#[derive(Component)]
pub struct InlayHintGlyph;

/// Spawn / reposition a `Text2d` for each [`InlayHintData`]. Re-runs
/// every frame because scroll / resize must move the glyph even when
/// the hint data itself is unchanged (no `Added` filter).
pub fn render_inlay_hints(
    mut commands: Commands,
    hints: Query<(Entity, &InlayHintData)>,
    editors: Query<(Entity, &TextFont), With<CodeEditor>>,
    metrics: RowMetricsParam,
    theme: Res<InlineDecorationsTheme>,
) {
    let Ok((editor_entity, font)) = editors.single() else {
        return;
    };
    let m = metrics.get_or_panic(editor_entity);

    for (entity, hint) in hints.iter() {
        let color = match hint.kind {
            InlayHintKind::Type => theme.inlay_type,
            InlayHintKind::Parameter => theme.inlay_parameter,
            InlayHintKind::Other => theme.inlay_other,
        };

        let band = m.row_glyph_band(hint.line);
        let cell_left = m
            .cell_top_left_at_x(hint.line, hint.character as f32 * m.cell_width())
            .x;
        let pos = Vec3::new(
            cell_left,
            (band.min.y + band.max.y) * 0.5,
            theme.inlay_z,
        );

        let Ok(mut cmd) = commands.get_entity(entity) else {
            continue;
        };
        cmd.queue_silenced(bevy::ecs::system::entity_command::insert(
            (
                Text2d::new(&hint.label),
                TextFont {
                    font: font.font.clone(),
                    font_size: font.font_size * theme.inlay_font_scale,
                    ..default()
                },
                TextColor(color),
                Transform::from_translation(pos),
                Anchor::CENTER_LEFT,
                InlayHintGlyph,
                LspUiVisual,
            ),
            bevy::ecs::bundle::InsertMode::Replace,
        ));
    }
}

/// Push a [`RectOverlay`] for every [`DocumentHighlightData`] into the
/// editor's [`TextOverlays`] (engine overlay slot `z = -2`, between
/// selections at `-1` and the line background at `0`).
pub fn render_document_highlights(
    highlights: Query<&DocumentHighlightData>,
    mut editors: Query<(&MonoCellWidth, &DisplayLayout, &mut TextOverlays), With<CodeEditor>>,
    theme: Res<InlineDecorationsTheme>,
) {
    let Ok((mono, layout, mut overlays)) = editors.single_mut() else {
        return;
    };

    overlays.0.retain(|r| r.z != -2);

    for highlight in highlights.iter() {
        let color = if highlight.is_write {
            theme.highlight_write
        } else {
            theme.highlight_read
        };

        let buffer_row = highlight.line;
        let start_byte = highlight.start_character as usize;
        let Some((display_row, start_byte_in_row)) =
            layout.buffer_to_display(buffer_row, start_byte)
        else {
            continue;
        };

        let start_x = layout
            .x_at_byte(display_row, start_byte_in_row)
            .unwrap_or(highlight.start_character as f32 * mono.px);

        let end_x = if highlight.end_character == u32::MAX {
            layout
                .lines
                .iter()
                .find(|l| l.display_row == display_row)
                .and_then(|l| layout.x_at_byte(display_row, l.text.len()))
                .unwrap_or_else(|| {
                    start_x
                        + highlight
                            .end_character
                            .saturating_sub(highlight.start_character)
                            .min(200) as f32
                            * mono.px
                })
        } else {
            let end_byte = highlight.end_character as usize;
            layout
                .x_at_byte(
                    display_row,
                    end_byte.saturating_sub(start_byte) + start_byte_in_row,
                )
                .unwrap_or(
                    start_x
                        + (highlight.end_character - highlight.start_character) as f32 * mono.px,
                )
        };

        if end_x <= start_x {
            continue;
        }

        overlays.0.push(RectOverlay {
            display_row,
            x_range: start_x..end_x,
            vertical: RowVertical::Full,
            color,
            z: -2,
            corners: CornerRadii::ZERO,
        });
    }
}
