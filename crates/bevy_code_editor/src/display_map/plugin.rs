//! `DisplayMapPlugin` — runs the display-layout build for the editor entity.
//!
//! Sits between input/state mutation and rendering: reads `(TextViewState,
//! ViewportRect, FoldState, syntax)`, writes `DisplayLayout` on the editor.
//! `text_view::update_text_views` then renders that layout.
//!
//! As of step 9 this replaces `update_gpu_text_instanced`. The transform stack
//! (`FoldMap`/`WrapMap`/`TabMap`) isn't yet wired here — the bridge in
//! `display_map::layout::build_display_layout` still does inline fold-skipping
//! and per-line syntax highlighting. A future refactor moves the transform
//! stack in and emits incremental dirty patches.

use bevy::prelude::*;
use bevy_text_engine::FontConfig;

use super::layout::build_display_layout;
use crate::plugin::syntax_highlighting::SyntaxResource;
use crate::settings::{PerformanceSettings, SyntaxSettings, ThemeSettings};
use crate::text_view::{DisplayLayout, TextViewState, TextViewViewport};
use crate::types::{CodeEditor, FoldState};

/// System set for display-map snapshot work — runs after edits are applied
/// (`ApplyStateSet`) and before the renderer (`RenderingSet`).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DisplayMapSet;

pub struct DisplayMapPlugin;

impl Plugin for DisplayMapPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            DisplayMapSet
                .after(crate::plugin::ApplyStateSet)
                .before(crate::plugin::RenderingSet),
        );
        app.add_systems(
            Update,
            (update_display_map_snapshot, prewarm_atlas_for_layout)
                .chain()
                .in_set(DisplayMapSet),
        );
    }
}

/// Build a fresh `DisplayLayout` for the editor entity each frame.
///
/// Reads from the editor's `TextViewState` + `TextViewViewport` and the
/// global `SyntaxResource`/`FoldState`/settings, writes the resulting layout
/// onto the entity. The renderer (`text_view::update_text_views`) reads it.
///
/// W5 (skip-on-unchanged): tracks a fingerprint of the inputs in `Local` state.
/// If nothing meaningful changed since last frame, skip rebuilding the layout;
/// `Changed<DisplayLayout>` then stays false and the renderer can early-out
/// without re-uploading the instance buffer.
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_display_map_snapshot(
    mut editor_query: Query<
        (
            &TextViewState,
            &TextViewViewport,
            &mut DisplayLayout,
            &FoldState,
            &FontConfig,
        ),
        With<CodeEditor>,
    >,
    theme: Res<ThemeSettings>,
    performance: Res<PerformanceSettings>,
    syntax_settings: Res<SyntaxSettings>,
    mut syntax: ResMut<SyntaxResource>,
    mut last_fingerprint: Local<Option<LayoutFingerprint>>,
) {
    for (tv_state, tv_viewport, mut layout, fold_state, font) in editor_query.iter_mut() {
        let fingerprint = LayoutFingerprint {
            content_version: tv_state.content_version,
            scroll_bits: tv_state.scroll_offset.to_bits(),
            h_scroll_bits: tv_state.horizontal_scroll_offset.to_bits(),
            viewport_w: tv_viewport.width,
            viewport_h: tv_viewport.height,
            viewport_top_bits: tv_viewport.text_area_top.to_bits(),
            font_size_tenths: (font.size * 10.0) as u32,
            line_height_tenths: (font.line_height * 10.0) as u32,
            syntax_version: syntax_version(&syntax),
            fold_count: fold_state.regions.len(),
        };

        if last_fingerprint.as_ref() == Some(&fingerprint) {
            continue;
        }

        let new_layout = build_display_layout(
            tv_state,
            tv_viewport,
            fold_state,
            font,
            &performance,
            theme.foreground,
            Some(&mut syntax),
            Some(&syntax_settings.theme),
        );

        *layout = new_layout;
        *last_fingerprint = Some(fingerprint);
    }
}

/// Cheap, hashable summary of every input that affects the layout.
/// Equality means "no rebuild needed"; inequality forces a rebuild.
/// Floats are compared by bit pattern (NaN comparisons aren't a real concern
/// here — scroll/viewport never produce NaN under normal use).
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct LayoutFingerprint {
    content_version: u64,
    scroll_bits: u32,
    h_scroll_bits: u32,
    viewport_w: u32,
    viewport_h: u32,
    viewport_top_bits: u32,
    font_size_tenths: u32,
    line_height_tenths: u32,
    syntax_version: u64,
    fold_count: usize,
}

#[cfg(feature = "tree-sitter")]
fn syntax_version(syntax: &SyntaxResource) -> u64 {
    syntax.tree_version
}

#[cfg(not(feature = "tree-sitter"))]
fn syntax_version(_syntax: &SyntaxResource) -> u64 {
    0
}

/// Pre-rasterize every glyph the freshly-built `DisplayLayout` will need.
///
/// Runs in `DisplayMapSet` after `update_display_map_snapshot`, so by the time
/// `text_view::update_text_views` reads the layout the atlas is fully
/// populated. The renderer never triggers atlas mutation during the paint
/// pass — eliminates first-encounter scroll stutter (W6).
pub(crate) fn prewarm_atlas_for_layout(
    layouts: Query<(Ref<DisplayLayout>, &FontConfig)>,
    mut atlas: ResMut<bevy_text_engine::gpu::GlyphAtlas>,
) {
    for (layout, font) in &layouts {
        if !layout.is_changed() {
            continue;
        }
        // Per-line font size pulled from the entity's `FontConfig`. Per-run
        // font_scale is rare enough that mid-paint rasterization is OK.
        atlas.ensure_glyphs(
            layout
                .lines
                .iter()
                .flat_map(|l| l.text.chars().map(|c| (c, font.size))),
        );
    }
}
