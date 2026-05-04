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

use super::layout::build_display_layout;
use crate::plugin::syntax_highlighting::SyntaxResource;
use crate::settings::{FontSettings, PerformanceSettings, SyntaxSettings, ThemeSettings};
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
pub(crate) fn update_display_map_snapshot(
    mut editor_query: Query<
        (
            &TextViewState,
            &TextViewViewport,
            &mut DisplayLayout,
        ),
        With<CodeEditor>,
    >,
    font: Res<FontSettings>,
    theme: Res<ThemeSettings>,
    performance: Res<PerformanceSettings>,
    syntax_settings: Res<SyntaxSettings>,
    #[cfg(feature = "folding")] fold_state: Res<FoldState>,
    mut syntax: ResMut<SyntaxResource>,
) {
    let Ok((tv_state, tv_viewport, mut layout)) = editor_query.single_mut() else {
        return;
    };

    #[cfg(not(feature = "folding"))]
    let fold_state = FoldState::default();

    let new_layout = build_display_layout(
        tv_state,
        tv_viewport,
        &fold_state,
        &font,
        &performance,
        theme.foreground,
        Some(&mut syntax),
        Some(&syntax_settings.theme),
    );

    *layout = new_layout;
}
