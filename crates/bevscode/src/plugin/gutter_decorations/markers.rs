//! Glyph-margin SVG icons. Host-facing surface is [`GlyphMarkers`];
//! `sync_gutter_icons` mirrors that Vec into a pool of child `UiSvg`
//! Nodes anchored in the glyph-margin column.
//!
//! Deleted-line indicators come in via `GutterDecorations` instead
//! (matching Monaco — a deleted row has no buffer line of its own
//! to bar). They render here as `diff_removed` icons.

use bevy::prelude::*;
use bevy::text::LineHeight;
use bevy_instanced_text::DisplayLayout;
use bevy_resvg::prelude::*;

use crate::settings::{EditorUi, GutterConfig, Padding};
use crate::types::{CodeEditor, GutterContainer};

use super::bars::{DecorationKind, GutterDecorations};
use super::common::{diff_place, group_pools_by_editor, RowGeometry};
use super::icons::IconAtlas;

/// One marker in the glyph-margin column.
#[derive(Clone, Debug, Reflect)]
#[reflect(Debug)]
pub struct GlyphMarker {
    /// Buffer line (0-indexed).
    pub line: usize,
    pub kind: GlyphKind,
    /// Tint applied to the icon. Severity-bridged markers inherit
    /// `DiagnosticColors`'s palette.
    pub color: Color,
}

/// Visual kind for a [`GlyphMarker`]. Each variant maps to a specific
/// Iconoir SVG: `Breakpoint` → filled circle, `DebugCurrent` →
/// solid play triangle, severities → `xmark-circle` / `warning-triangle`
/// / `info-circle` / `light-bulb`. `Custom` falls back to the
/// breakpoint circle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
#[reflect(Debug, PartialEq, Hash)]
pub enum GlyphKind {
    Breakpoint,
    DebugCurrent,
    DiagnosticError,
    DiagnosticWarning,
    DiagnosticInfo,
    DiagnosticHint,
    Custom,
}

/// Per-editor list of glyph-margin markers. Hosts mutate this
/// directly; `sync_lsp_glyph_markers` also overwrites severity
/// entries each time a fresh diagnostic batch lands.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct GlyphMarkers(pub Vec<GlyphMarker>);

/// Marker for a glyph-margin icon child node.
#[derive(Component, Reflect, Clone, Copy)]
#[reflect(Component)]
pub struct GutterIcon {
    pub editor: Entity,
    pub line: usize,
}

#[allow(clippy::type_complexity)]
pub(crate) fn sync_gutter_icons(
    mut commands: Commands,
    atlas: Option<Res<IconAtlas>>,
    editors: Query<
        (
            Entity,
            &GlyphMarkers,
            &GutterDecorations,
            &GutterConfig,
            &EditorUi,
            &TextFont,
            &LineHeight,
            &Padding,
            &DisplayLayout,
        ),
        With<CodeEditor>,
    >,
    mut existing: Query<(
        Entity,
        &GutterIcon,
        &mut Node,
        &mut SvgColor,
        &mut UiSvg,
        &mut Visibility,
    )>,
    containers: Query<(Entity, &GutterContainer)>,
) {
    let Some(atlas) = atlas else {
        return;
    };

    let mut by_editor = group_pools_by_editor(
        existing.iter().map(|(id, gi, ..)| (id, gi)),
        |gi: &GutterIcon| gi.editor,
    );

    for (editor_entity, markers, decorations, gutter, ui, font, line_height, padding, layout) in
        editors.iter()
    {
        if !ui.glyph_margin || gutter.glyph_margin_width <= 0.0 {
            continue;
        }

        let mut desired: Vec<(usize, Handle<SvgFile>, Color)> = Vec::new();
        for m in &markers.0 {
            desired.push((m.line, atlas.handle_for(m.kind), m.color));
        }
        for d in &decorations.0 {
            if matches!(d.kind, DecorationKind::Deleted) {
                desired.push((d.line, atlas.diff_removed.clone(), d.color));
            }
        }

        let pool = by_editor.entry(editor_entity).or_default();

        // The glyph-margin column is *left* of the digits; anchor icons
        // to the column's right edge with a small overflow so they sit
        // hard against the digits instead of in dead-centre.
        let column_right_x = gutter.glyph_margin_x + gutter.glyph_margin_width;

        // Pool slot N corresponds to `desired[N]` permanently across
        // frames. Hidden lines (collapsed inside a fold) get their slot
        // hidden in place rather than skipped-and-compacted, so the
        // remaining slots retain their original `(line, kind)` and
        // their `UiSvg` handle never has to change. That sidesteps
        // `bevy_resvg`'s "only attach ImageNode on Added" quirk
        // entirely.
        for (idx, (line, handle, color)) in desired.iter().enumerate() {
            let geom = RowGeometry::compute(*line, font, line_height, padding, layout);
            let line = *line;
            let color = *color;
            let handle = handle.clone();

            if let Some(geom) = geom {
                let icon_size = (gutter.glyph_margin_width.min(geom.line_height_px) * 0.6)
                    .round()
                    .max(8.0);
                let nudge_right = (geom.line_height_px * 0.2).round();
                let optical_lift = (geom.line_height_px * 0.05).round();
                let icon_left = (column_right_x - icon_size + nudge_right).round();
                // Bias the icon slightly above geometric centre so it
                // tracks the digits' optical centre (which sits above
                // the row's mid-line because of the descender).
                let icon_top = (geom.top_px + (geom.line_height_px - icon_size) * 0.5
                    - optical_lift)
                    .round();

                if let Some(&entity) = pool.get(idx) {
                    if let Ok((_, _gi, mut node, mut svg_color, _ui_svg, mut vis)) =
                        existing.get_mut(entity)
                    {
                        diff_place(&mut node, icon_left, icon_top, icon_size, icon_size);
                        if svg_color.0 != color {
                            svg_color.0 = color;
                        }
                        if *vis != Visibility::Inherited {
                            *vis = Visibility::Inherited;
                        }
                        commands.entity(entity).insert(GutterIcon {
                            editor: editor_entity,
                            line,
                        });
                    }
                } else {
                    let id = commands
                        .spawn((
                            GutterIcon {
                                editor: editor_entity,
                                line,
                            },
                            UiSvg(handle),
                            SvgColor(color),
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(icon_left),
                                top: Val::Px(icon_top),
                                width: Val::Px(icon_size),
                                height: Val::Px(icon_size),
                                overflow: Overflow::clip(),
                                ..default()
                            },
                            bevy::picking::Pickable::IGNORE,
                            Name::new("GutterIcon"),
                        ))
                        .id();
                    if let Some(parent) = containers
                        .iter()
                        .find_map(|(eid, c)| (c.editor == editor_entity).then_some(eid))
                    {
                        commands.entity(parent).add_child(id);
                    }
                    pool.push(id);
                }
            } else if let Some(&entity) = pool.get(idx) {
                // Buffer line is hidden by a fold (or layout hasn't
                // produced it yet) — keep the entity but hide it in
                // place so its slot identity is preserved.
                if let Ok((_, _, _, _, _, mut vis)) = existing.get_mut(entity) {
                    if *vis != Visibility::Hidden {
                        *vis = Visibility::Hidden;
                    }
                }
            }
        }

        // Any pool entries past `desired.len()` came from a previous
        // frame with more markers; hide them.
        for &entity in pool.iter().skip(desired.len()) {
            if let Ok((_, _, _, _, _, mut vis)) = existing.get_mut(entity) {
                if *vis != Visibility::Hidden {
                    *vis = Visibility::Hidden;
                }
            }
        }
    }
}
