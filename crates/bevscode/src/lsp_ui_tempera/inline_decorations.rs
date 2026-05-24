//! Inline LSP decorations: inlay hints (UI text) and document
//! highlights (engine overlay rects).
//!
//! Document highlights go through the engine's [`RectOverlay`] path so
//! they share the editor's draw call and stay pixel-aligned with the
//! glyph grid. Inlay hint labels are spawned as `bevy_ui` [`Text`] nodes
//! parented under the editor entity, positioned absolutely from
//! [`RowMetrics`] — same pattern the LSP popups use, and the only path
//! that respects the editor's screen position + clipping. Using `Text2d`
//! here would render the labels in world space (independent of the
//! editor's UI rect) and they would drift, scroll opposite to the
//! editor, and escape clipping.

use bevy::prelude::*;
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

/// Marker on the spawned UI node for each inlay hint. `sync_inlay_hints`
/// drains and re-spawns these every time `LspInlayHints` changes.
#[derive(Component)]
pub struct InlayHintGlyph;

/// Render every [`InlayHintData`] as a `bevy_ui` text node parented under
/// the editor entity. Re-runs each frame because scroll / resize must
/// reposition the node even when the hint data itself is unchanged.
///
/// Hints whose row is outside the editor's vertical viewport get
/// `Display::None` instead of being clamped — without this they would
/// pile up at the editor's top edge (`bevy_ui` clamps negative `top`
/// to 0).
pub fn render_inlay_hints(
    mut commands: Commands,
    hints: Query<(Entity, &InlayHintData)>,
    editors: Query<(Entity, &TextFont, &ComputedNode), With<CodeEditor>>,
    metrics: RowMetricsParam,
    theme: Res<InlineDecorationsTheme>,
) {
    let Ok((editor_entity, font, computed)) = editors.single() else {
        return;
    };
    let m = metrics.get_or_panic(editor_entity);
    let inv = computed.inverse_scale_factor();
    let logical_h = computed.size().y * inv;
    let line_height = m.row_height();

    for (entity, hint) in hints.iter() {
        let color = match hint.kind {
            InlayHintKind::Type => theme.inlay_type,
            InlayHintKind::Parameter => theme.inlay_parameter,
            InlayHintKind::Other => theme.inlay_other,
        };

        let row_top = m
            .cell_top_left_at_x(hint.line, hint.character as f32 * m.cell_width())
            .y;
        let row_bot = row_top + line_height;
        let off_screen = row_bot <= 0.0 || row_top >= logical_h;

        let cell_left = m
            .cell_top_left_at_x(hint.line, hint.character as f32 * m.cell_width())
            .x;

        let Ok(mut cmd) = commands.get_entity(entity) else {
            continue;
        };
        cmd.queue_silenced(bevy::ecs::system::entity_command::insert(
            (
                Text::new(hint.label.clone()),
                TextFont {
                    font: font.font.clone(),
                    font_size: font.font_size * theme.inlay_font_scale,
                    ..default()
                },
                TextColor(color),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(cell_left),
                    top: Val::Px(row_top),
                    display: if off_screen { Display::None } else { Display::Flex },
                    ..default()
                },
                ChildOf(editor_entity),
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
