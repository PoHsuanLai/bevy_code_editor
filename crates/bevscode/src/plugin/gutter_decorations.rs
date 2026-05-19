//! Glyph-margin icons + line-decoration bars for the gutter.
//!
//! Two host-facing Components, each on the editor entity:
//! - [`GlyphMarkers`]: SVG icons that sit in the glyph-margin column
//!   (`GutterConfig::glyph_margin_x`). Used for LSP diagnostic severity
//!   icons, breakpoints, debug arrows.
//! - [`GutterDecorations`]: thin colored bars in the line-decoration
//!   strip (`GutterConfig::line_decorations_x`). Used for VCS
//!   added / modified indicators and per-line severity bars. Deleted
//!   rows render as a small triangle icon in the glyph margin instead
//!   of a bar (matching Monaco's "deleted lines have no row").
//!
//! Icons ship bundled as Octicons SVGs (MIT-licensed) — they are
//! rasterised once at plugin startup via `bevy_resvg` into
//! [`SvgFile`] assets and then referenced by child `UiSvg` entities.
//! Hosts do not need to copy any asset files; the icons are embedded
//! in the crate via `include_bytes!`.

use bevy::asset::RenderAssetUsages;
use bevy::picking::events::{Pointer, Press};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::text::LineHeight;
use bevy::ui::{ComputedNode, ScrollPosition};
use bevy_instanced_text::{MonoCellWidth, RectOverlay, RowVertical};
use bevy_resvg::prelude::*;
use bevy_resvg::resvg::{
    self,
    tiny_skia::Pixmap,
    usvg::{self, Transform, Tree},
};

use crate::settings::{EditorTheme, EditorUi, GutterConfig};
use crate::types::{CodeEditor, FoldState};

const ICON_RASTER_PX: u32 = 48;

/// Embedded Octicons SVG sources (16-px viewBox, MIT-licensed).
const SVG_DOT_FILL: &[u8] = include_bytes!("../../assets/icons/dot-fill.svg");
const SVG_TRIANGLE_RIGHT: &[u8] = include_bytes!("../../assets/icons/triangle-right.svg");
const SVG_X_CIRCLE_FILL: &[u8] = include_bytes!("../../assets/icons/x-circle-fill.svg");
const SVG_ALERT_FILL: &[u8] = include_bytes!("../../assets/icons/alert-fill.svg");
const SVG_INFO: &[u8] = include_bytes!("../../assets/icons/info.svg");
const SVG_LIGHT_BULB: &[u8] = include_bytes!("../../assets/icons/light-bulb.svg");
const SVG_DIFF_REMOVED: &[u8] = include_bytes!("../../assets/icons/diff-removed.svg");

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
/// Octicons SVG: `Breakpoint` → `dot-fill`, `DebugCurrent` →
/// `triangle-right`, severities → `x-circle-fill` / `alert-fill` /
/// `info` / `light-bulb`. `Custom` falls back to `dot-fill`.
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

/// Per-editor list of glyph-margin markers. Hosts mutate this directly;
/// `sync_lsp_glyph_markers` also overwrites severity entries each time
/// a fresh diagnostic batch lands.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct GlyphMarkers(pub Vec<GlyphMarker>);

/// One bar in the line-decoration strip.
#[derive(Clone, Debug, Reflect)]
#[reflect(Debug)]
pub struct LineDecoration {
    pub line: usize,
    pub kind: DecorationKind,
    pub color: Color,
}

/// Categorisation for a [`LineDecoration`]. Bars render in the
/// line-decoration strip for [`Added`](DecorationKind::Added) /
/// [`Modified`](DecorationKind::Modified) /
/// [`DiagnosticBar`](DecorationKind::DiagnosticBar) /
/// [`Custom`](DecorationKind::Custom). [`Deleted`](DecorationKind::Deleted)
/// instead places a small triangle icon in the glyph margin at the
/// deletion row, matching Monaco — a deleted line has no row of its
/// own to bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum DecorationKind {
    Added,
    Modified,
    Deleted,
    DiagnosticBar,
    Custom,
}

/// Per-editor list of line-decoration bars.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct GutterDecorations(pub Vec<LineDecoration>);

/// Line-decoration strip rects produced by
/// [`update_line_decoration_overlays`]. Merged into `TextUnderlays`.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct LineDecorationRects(pub Vec<RectOverlay>);

/// Kept for back-compat with the merge pipeline. No longer populated —
/// glyph markers now render through child `UiSvg` entities. Eventually
/// removable once all merge consumers stop reading it.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct GlyphMarginRects(pub Vec<RectOverlay>);

/// Emitted by [`on_glyph_margin_press`] when the user clicks in the
/// glyph-margin column. Hosts subscribe to wire breakpoints / step
/// markers / etc.
#[derive(Message, Clone, Debug, Reflect)]
#[reflect(Clone, Debug)]
pub struct GlyphMarginClicked {
    pub editor: Entity,
    pub line: usize,
    pub button: PointerButton,
}

/// Rasterised icon handles, populated once at plugin Startup.
#[derive(Resource, Default, Clone)]
pub struct IconAtlas {
    pub breakpoint: Handle<SvgFile>,
    pub debug_current: Handle<SvgFile>,
    pub diag_error: Handle<SvgFile>,
    pub diag_warning: Handle<SvgFile>,
    pub diag_info: Handle<SvgFile>,
    pub diag_hint: Handle<SvgFile>,
    pub diff_removed: Handle<SvgFile>,
}

impl IconAtlas {
    pub fn handle_for(&self, kind: GlyphKind) -> Handle<SvgFile> {
        match kind {
            GlyphKind::Breakpoint | GlyphKind::Custom => self.breakpoint.clone(),
            GlyphKind::DebugCurrent => self.debug_current.clone(),
            GlyphKind::DiagnosticError => self.diag_error.clone(),
            GlyphKind::DiagnosticWarning => self.diag_warning.clone(),
            GlyphKind::DiagnosticInfo => self.diag_info.clone(),
            GlyphKind::DiagnosticHint => self.diag_hint.clone(),
        }
    }
}

/// Marker for a glyph-margin icon child node.
#[derive(Component, Reflect, Clone, Copy)]
#[reflect(Component)]
pub struct GutterIcon {
    /// Editor this icon belongs to.
    pub editor: Entity,
    /// Buffer line for hit-testing / re-anchoring across edits.
    pub line: usize,
}

/// Rasterise the embedded Octicons SVGs into [`SvgFile`] assets and
/// stash their handles in the [`IconAtlas`] resource. Runs once at
/// PreStartup so glyph markers spawned in Startup find handles.
pub(crate) fn setup_icon_atlas(mut commands: Commands, mut svgs: ResMut<Assets<SvgFile>>) {
    let opt = usvg::Options::default();
    let mut bake = |bytes: &[u8]| -> Handle<SvgFile> {
        let tree = Tree::from_data(bytes, &opt).expect("embedded Octicons SVG should parse");
        let original_size = tree.size();
        let scale_x = ICON_RASTER_PX as f32 / original_size.width();
        let scale_y = ICON_RASTER_PX as f32 / original_size.height();
        let mut pixmap =
            Pixmap::new(ICON_RASTER_PX, ICON_RASTER_PX).expect("Pixmap allocation for icon atlas");
        resvg::render(
            &tree,
            Transform::from_scale(scale_x, scale_y),
            &mut pixmap.as_mut(),
        );
        let image = Image::new(
            Extent3d {
                width: ICON_RASTER_PX,
                height: ICON_RASTER_PX,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixmap.take(),
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::default(),
        );
        svgs.add(SvgFile(image))
    };
    let atlas = IconAtlas {
        breakpoint: bake(SVG_DOT_FILL),
        debug_current: bake(SVG_TRIANGLE_RIGHT),
        diag_error: bake(SVG_X_CIRCLE_FILL),
        diag_warning: bake(SVG_ALERT_FILL),
        diag_info: bake(SVG_INFO),
        diag_hint: bake(SVG_LIGHT_BULB),
        diff_removed: bake(SVG_DIFF_REMOVED),
    };
    commands.insert_resource(atlas);
}

/// Spawn / despawn / re-position child `UiSvg` icon entities so that
/// they mirror the editor's [`GlyphMarkers`] (plus any
/// `DecorationKind::Deleted` rows). Position tracks the editor's
/// current scroll & fold state. Diffing avoids per-frame churn — only
/// the icons whose tint / position changed get updated.
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
            &FoldState,
            &TextFont,
            &LineHeight,
            &ScrollPosition,
            &crate::settings::Padding,
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
) {
    let Some(atlas) = atlas else {
        return;
    };

    use std::collections::HashMap;
    let mut by_editor: HashMap<Entity, Vec<Entity>> = HashMap::new();
    for (id, gi, ..) in existing.iter() {
        by_editor.entry(gi.editor).or_default().push(id);
    }

    for (
        editor_entity,
        markers,
        decorations,
        gutter,
        ui,
        fold,
        font,
        line_height,
        scroll,
        padding,
    ) in editors.iter()
    {
        let glyph_visible = ui.glyph_margin && gutter.glyph_margin_width > 0.0;
        let line_height_px = bevy_instanced_text::resolve_line_height(*line_height, font.font_size);
        if line_height_px <= 0.0 {
            continue;
        }

        struct Desired {
            line: usize,
            handle: Handle<SvgFile>,
            color: Color,
        }
        let mut desired: Vec<Desired> = Vec::with_capacity(markers.0.len() + decorations.0.len());
        if glyph_visible {
            for m in &markers.0 {
                desired.push(Desired {
                    line: m.line,
                    handle: atlas.handle_for(m.kind),
                    color: m.color,
                });
            }
            for d in &decorations.0 {
                if matches!(d.kind, DecorationKind::Deleted) {
                    desired.push(Desired {
                        line: d.line,
                        handle: atlas.diff_removed.clone(),
                        color: d.color,
                    });
                }
            }
        }

        let pool = by_editor.entry(editor_entity).or_default();

        let icon_size = (gutter.glyph_margin_width)
            .min(line_height_px)
            .round()
            .max(8.0);
        let column_center_x = gutter.glyph_margin_x + gutter.glyph_margin_width * 0.5;
        let icon_left = (column_center_x - icon_size * 0.5).round();
        let text_top = padding.top;

        for (idx, want) in desired.iter().enumerate() {
            let display_row = fold.actual_to_display_line(want.line) as f32;
            let top_px = (text_top + display_row * line_height_px - scroll.y).round();
            let target_handle = want.handle.clone();
            let line = want.line;
            let color = want.color;

            if let Some(&entity) = pool.get(idx) {
                if let Ok((_, _gi, mut node, mut svg_color, mut ui_svg, mut vis)) =
                    existing.get_mut(entity)
                {
                    let target_top = Val::Px(top_px);
                    let target_left = Val::Px(icon_left);
                    let target_w = Val::Px(icon_size);
                    let target_h = Val::Px(icon_size);
                    if node.top != target_top {
                        node.top = target_top;
                    }
                    if node.left != target_left {
                        node.left = target_left;
                    }
                    if node.width != target_w {
                        node.width = target_w;
                    }
                    if node.height != target_h {
                        node.height = target_h;
                    }
                    if svg_color.0 != color {
                        svg_color.0 = color;
                    }
                    if ui_svg.0 != target_handle {
                        ui_svg.0 = target_handle;
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
                        UiSvg(target_handle),
                        SvgColor(color),
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(icon_left),
                            top: Val::Px(top_px),
                            width: Val::Px(icon_size),
                            height: Val::Px(icon_size),
                            overflow: Overflow::clip(),
                            ..default()
                        },
                        bevy::picking::Pickable::IGNORE,
                        Name::new("GutterIcon"),
                    ))
                    .id();
                commands.entity(editor_entity).add_child(id);
                pool.push(id);
            }
        }

        for &entity in pool.iter().skip(desired.len()) {
            if let Ok((_, _, _, _, _, mut vis)) = existing.get_mut(entity) {
                if *vis != Visibility::Hidden {
                    *vis = Visibility::Hidden;
                }
            }
        }
    }
}

/// Emit a thin vertical bar per [`LineDecoration`] (excluding
/// `Deleted` — that variant is rendered as a triangle icon by
/// `sync_gutter_icons`). The bar uses `RowVertical::Full` so the
/// cap-to-descender height leaves a 1-px gap to the neighbouring
/// row, matching Monaco's "discrete bars" look. X coordinates are
/// snapped to integers to avoid sub-pixel smearing on fractional
/// scroll offsets.
#[allow(clippy::type_complexity)]
pub(crate) fn update_line_decoration_overlays(
    mut editor_query: Query<
        (
            &GutterConfig,
            &EditorUi,
            &FoldState,
            &GutterDecorations,
            &mut LineDecorationRects,
        ),
        With<CodeEditor>,
    >,
) {
    for (gutter, ui, fold, decorations, mut rects) in editor_query.iter_mut() {
        let mut new_rects: Vec<RectOverlay> = Vec::with_capacity(decorations.0.len());
        if gutter.line_decorations_width > 0.0 {
            let bar_width = (gutter.line_decorations_width * 0.5)
                .clamp(2.0, 4.0)
                .round();
            let center_node_x = gutter.line_decorations_x + gutter.line_decorations_width * 0.5;
            let center = (center_node_x - (gutter.gutter_width + ui.code_margin_left)).round();
            for decoration in &decorations.0 {
                if matches!(decoration.kind, DecorationKind::Deleted) {
                    continue;
                }
                let display_row = fold.actual_to_display_line(decoration.line) as u32;
                let half = (bar_width * 0.5).round();
                new_rects.push(RectOverlay {
                    display_row,
                    x_range: (center - half)..(center + half),
                    vertical: RowVertical::Full,
                    color: decoration.color,
                    z: 0,
                    corners: bevy_instanced_text::CornerRadii::ZERO,
                });
            }
        }
        if rects.0 != new_rects {
            rects.0 = new_rects;
        }
    }
}

/// Glyph-margin overlay is no longer produced (icons render as child
/// `UiSvg` entities). Keep the system as a no-op to leave the merge
/// pipeline + Component cascade untouched; downstream removal is a
/// follow-up cleanup.
pub(crate) fn update_glyph_margin_overlays(mut q: Query<&mut GlyphMarginRects, With<CodeEditor>>) {
    for mut rects in q.iter_mut() {
        if !rects.0.is_empty() {
            rects.0.clear();
        }
    }
}

/// Bridge `DiagnosticMarker` entities (spawned by the LSP layer) into
/// per-editor [`GlyphMarkers`] + [`GutterDecorations`]. Severity → icon
/// + color via `DiagnosticColors`. Runs only when the `lsp` feature is
/// enabled.
#[cfg(feature = "lsp")]
#[allow(clippy::type_complexity)]
pub(crate) fn sync_lsp_glyph_markers(
    diagnostics: Query<&crate::lsp_ui::systems::DiagnosticMarker>,
    mut editors: Query<
        (
            &crate::settings::DiagnosticColors,
            &mut GlyphMarkers,
            &mut GutterDecorations,
        ),
        With<CodeEditor>,
    >,
) {
    use lsp_types::DiagnosticSeverity;
    let mut per_line: std::collections::HashMap<usize, DiagnosticSeverity> = Default::default();
    for diag in diagnostics.iter() {
        let entry = per_line.entry(diag.line).or_insert(diag.severity);
        if severity_rank(diag.severity) > severity_rank(*entry) {
            *entry = diag.severity;
        }
    }
    for (colors, mut markers, mut decorations) in editors.iter_mut() {
        let mut new_markers: Vec<GlyphMarker> = Vec::with_capacity(per_line.len());
        let mut new_bars: Vec<LineDecoration> = Vec::with_capacity(per_line.len());
        for (&line, &severity) in &per_line {
            let (kind, color) = match severity {
                DiagnosticSeverity::ERROR => (GlyphKind::DiagnosticError, colors.error),
                DiagnosticSeverity::WARNING => (GlyphKind::DiagnosticWarning, colors.warning),
                DiagnosticSeverity::INFORMATION => (GlyphKind::DiagnosticInfo, colors.info),
                _ => (GlyphKind::DiagnosticHint, colors.hint),
            };
            new_markers.push(GlyphMarker { line, kind, color });
            new_bars.push(LineDecoration {
                line,
                kind: DecorationKind::DiagnosticBar,
                color,
            });
        }
        markers.0.retain(|m| {
            !matches!(
                m.kind,
                GlyphKind::DiagnosticError
                    | GlyphKind::DiagnosticWarning
                    | GlyphKind::DiagnosticInfo
                    | GlyphKind::DiagnosticHint
            )
        });
        decorations
            .0
            .retain(|d| !matches!(d.kind, DecorationKind::DiagnosticBar));
        markers.0.extend(new_markers);
        decorations.0.extend(new_bars);
    }
}

#[cfg(feature = "lsp")]
fn severity_rank(severity: lsp_types::DiagnosticSeverity) -> u8 {
    use lsp_types::DiagnosticSeverity;
    match severity {
        DiagnosticSeverity::ERROR => 4,
        DiagnosticSeverity::WARNING => 3,
        DiagnosticSeverity::INFORMATION => 2,
        _ => 1,
    }
}

/// Click observer: emit [`GlyphMarginClicked`] when the user clicks
/// inside the glyph-margin column. The line is derived from the click's
/// y position in the same way as the fold-gutter strip.
#[allow(clippy::too_many_arguments)]
pub fn on_glyph_margin_press(
    trigger: On<Pointer<Press>>,
    editors: Query<
        (
            &ScrollPosition,
            &ComputedNode,
            &GutterConfig,
            &FoldState,
            &TextFont,
            &bevy::text::LineHeight,
            &MonoCellWidth,
        ),
        With<CodeEditor>,
    >,
    mut writer: MessageWriter<GlyphMarginClicked>,
) {
    let entity = trigger.event().entity;
    let Ok((scroll, computed, gutter, fold_state, font, lh, _mono)) = editors.get(entity) else {
        return;
    };
    if gutter.glyph_margin_width <= 0.0 {
        return;
    }
    let Some(local_pos) = crate::input::mouse::hit_to_local_px(&trigger.event().hit, computed)
    else {
        return;
    };
    if local_pos.x < gutter.glyph_margin_x
        || local_pos.x >= gutter.glyph_margin_x + gutter.glyph_margin_width
    {
        return;
    }
    let inv = computed.inverse_scale_factor();
    let line_height = bevy_instanced_text::resolve_line_height(*lh, font.font_size);
    let text_area_top = computed.content_inset().min_inset.y * inv;
    let relative_y = local_pos.y - text_area_top + scroll.y;
    if relative_y < 0.0 || line_height <= 0.0 {
        return;
    }
    let display_row = (relative_y / line_height) as usize;
    let buffer_line = fold_state.display_to_actual_line(display_row);
    writer.write(GlyphMarginClicked {
        editor: entity,
        line: buffer_line,
        button: trigger.event().button,
    });
    let _ = EditorTheme::default;
}
