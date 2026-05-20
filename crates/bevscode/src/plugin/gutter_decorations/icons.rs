//! Icon atlas: rasterise embedded Octicons SVGs into `SvgFile` assets
//! and stash their handles in [`IconAtlas`]. Hosts don't deal with
//! asset files — icons are baked once at PreStartup and consumed by
//! the per-kind sync systems via the atlas resource.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_resvg::prelude::*;
use bevy_resvg::resvg::{
    self,
    tiny_skia::Pixmap,
    usvg::{self, Transform, Tree},
};

use super::markers::GlyphKind;

const ICON_RASTER_PX: u32 = 48;

const SVG_DOT_FILL: &[u8] = include_bytes!("../../../assets/icons/dot-fill.svg");
const SVG_TRIANGLE_RIGHT: &[u8] = include_bytes!("../../../assets/icons/triangle-right.svg");
const SVG_X_CIRCLE_FILL: &[u8] = include_bytes!("../../../assets/icons/x-circle-fill.svg");
const SVG_ALERT_FILL: &[u8] = include_bytes!("../../../assets/icons/alert-fill.svg");
const SVG_INFO: &[u8] = include_bytes!("../../../assets/icons/info.svg");
const SVG_LIGHT_BULB: &[u8] = include_bytes!("../../../assets/icons/light-bulb.svg");
const SVG_DIFF_REMOVED: &[u8] = include_bytes!("../../../assets/icons/diff-removed.svg");
const SVG_CHEVRON_DOWN: &[u8] = include_bytes!("../../../assets/icons/chevron-down.svg");
const SVG_CHEVRON_RIGHT: &[u8] = include_bytes!("../../../assets/icons/chevron-right.svg");

/// Rasterised icon handles, populated once at plugin PreStartup.
#[derive(Resource, Default, Clone)]
pub struct IconAtlas {
    pub breakpoint: Handle<SvgFile>,
    pub debug_current: Handle<SvgFile>,
    pub diag_error: Handle<SvgFile>,
    pub diag_warning: Handle<SvgFile>,
    pub diag_info: Handle<SvgFile>,
    pub diag_hint: Handle<SvgFile>,
    pub diff_removed: Handle<SvgFile>,
    pub chevron_down: Handle<SvgFile>,
    pub chevron_right: Handle<SvgFile>,
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

/// Bake one SVG to a tintable `SvgFile`. `optical_scale` is applied
/// around the pixmap centre on top of the base "fit into 48×48" scale
/// — values >1 enlarge the visible glyph (e.g. dot-fill needs 1.6 to
/// match neighbouring icons), <1 pull edge-bleeding glyphs inward.
fn bake_icon(svgs: &mut Assets<SvgFile>, bytes: &[u8], optical_scale: f32) -> Handle<SvgFile> {
    // Octicons SVGs ship without an explicit `fill`, so resvg falls
    // back to the SVG spec default (black) at rasterise time. A
    // black sprite multiplied by `Sprite.color` stays black, so the
    // host's `GlyphMarker.color` tint never lands. Inject a CSS
    // stylesheet that forces white pixels, which then tint correctly
    // through the sprite multiplier.
    let opt = usvg::Options {
        style_sheet: Some("* { fill: #fff !important; }".to_string()),
        ..usvg::Options::default()
    };
    let tree = Tree::from_data(bytes, &opt).expect("embedded Octicons SVG should parse");
    let original_size = tree.size();
    let base = ICON_RASTER_PX as f32 / original_size.width().max(original_size.height());
    let s = base * optical_scale;
    let offset = (ICON_RASTER_PX as f32 - original_size.width() * s) * 0.5;
    let transform = Transform::from_scale(s, s).post_translate(offset, offset);
    let mut pixmap =
        Pixmap::new(ICON_RASTER_PX, ICON_RASTER_PX).expect("Pixmap allocation for icon atlas");
    resvg::render(&tree, transform, &mut pixmap.as_mut());
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
}

/// PreStartup: build the [`IconAtlas`]. Per-kind optical scales are
/// tuned to match Monaco's visual hierarchy:
/// severity icons ≥ debug arrow > breakpoint dot.
pub(crate) fn setup_icon_atlas(mut commands: Commands, mut svgs: ResMut<Assets<SvgFile>>) {
    let atlas = IconAtlas {
        // Tuned to match Monaco's denser visual weight: the breakpoint
        // dot lands ~10 visible px (was 12), chevrons + debug arrow
        // pull in so they don't fill the full 16-px column.
        breakpoint: bake_icon(&mut svgs, SVG_DOT_FILL, 1.2),
        debug_current: bake_icon(&mut svgs, SVG_TRIANGLE_RIGHT, 1.1),
        diag_error: bake_icon(&mut svgs, SVG_X_CIRCLE_FILL, 0.75),
        diag_warning: bake_icon(&mut svgs, SVG_ALERT_FILL, 0.75),
        diag_info: bake_icon(&mut svgs, SVG_INFO, 0.75),
        diag_hint: bake_icon(&mut svgs, SVG_LIGHT_BULB, 0.75),
        diff_removed: bake_icon(&mut svgs, SVG_DIFF_REMOVED, 0.75),
        chevron_down: bake_icon(&mut svgs, SVG_CHEVRON_DOWN, 0.7),
        chevron_right: bake_icon(&mut svgs, SVG_CHEVRON_RIGHT, 0.7),
    };
    commands.insert_resource(atlas);
}
