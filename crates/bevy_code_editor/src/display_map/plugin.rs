//! `DisplayMapPlugin` — runs the display-layout build for the editor entity.
//!
//! Sits between input/state mutation and rendering: reads `(TextViewState,
//! ViewportRect, FoldState, syntax)`, writes `DisplayLayout` on the editor.
//! `text_view::update_text_views` then renders that layout.
//!
//! `update_display_map_snapshot` is the live producer for the editor;
//! atlas pre-warming is handled engine-side by
//! `bevy_text_engine::view::plugin::prewarm_atlas_for_layout`.

use bevy::prelude::*;
use bevy_text_engine::FontConfig;

use super::layout::build_display_layout;
use crate::plugin::syntax_highlighting::SyntaxResource;
use crate::settings::{
    IndentationSettings, PerformanceSettings, SyntaxSettings, ThemeSettings, WrappingSettings,
};
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
        app.add_systems(Update, update_display_map_snapshot.in_set(DisplayMapSet));
    }
}

/// Build a fresh `DisplayLayout` for the editor entity each frame.
///
/// Reads from the editor's `TextViewState` + `TextViewViewport` and the
/// global `SyntaxResource`/`FoldState`/settings, writes the resulting layout
/// onto the entity. The renderer (`text_view::update_text_views`) reads it.
///
/// Tracks a fingerprint of the inputs in `Local` state and skips the rebuild
/// when nothing meaningful changed since last frame; `Changed<DisplayLayout>`
/// then stays false and the renderer early-outs without re-uploading the
/// instance buffer.
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_display_map_snapshot(
    mut editor_query: Query<
        (
            &mut TextViewState,
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
    wrapping: Res<WrappingSettings>,
    indentation: Res<IndentationSettings>,
    mut syntax: ResMut<SyntaxResource>,
    mut atlas: ResMut<bevy_text_engine::GlyphAtlas>,
    fonts: Res<Assets<bevy::text::Font>>,
    mut last_fingerprint: Local<Option<LayoutFingerprint>>,
) {
    for (mut tv_state, tv_viewport, mut layout, fold_state, font) in editor_query.iter_mut() {
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
            wrap_enabled: wrapping.enabled,
            wrap_column: wrapping.wrap_column,
        };

        if last_fingerprint.as_ref() == Some(&fingerprint) {
            continue;
        }

        let new_layout = build_display_layout(
            &mut *tv_state,
            tv_viewport,
            fold_state,
            font,
            &performance,
            &wrapping,
            &indentation,
            theme.foreground,
            Some(&mut syntax),
            Some(&syntax_settings.theme),
            Some(&mut atlas),
            Some(&fonts),
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
    wrap_enabled: bool,
    wrap_column: Option<usize>,
}

#[cfg(feature = "tree-sitter")]
fn syntax_version(syntax: &SyntaxResource) -> u64 {
    syntax.tree_version
}

#[cfg(not(feature = "tree-sitter"))]
fn syntax_version(_syntax: &SyntaxResource) -> u64 {
    0
}

