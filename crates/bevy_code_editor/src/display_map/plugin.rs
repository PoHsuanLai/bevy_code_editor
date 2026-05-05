//! `DisplayMapPlugin` — plumbs the editor's fold / syntax / wrap state into
//! the engine's per-frame layout system via trait Components.
//!
//! The engine's `produce_layouts` (in `bevy_text_engine::view::layout_builder`)
//! reads `LineFilter` / `LineStyleSource` / `LayoutWrap` Components off each
//! `TextView` entity and drives layout production itself. This plugin owns:
//!
//! - A startup system that inserts the editor's `FoldFilter` /
//!   `SyntaxStyling` Components on every `CodeEditor` entity.
//! - Three sync systems (`sync_fold_filter`, `sync_syntax_styling`,
//!   `sync_layout_wrap`) that refresh each Component's interior state from
//!   the editor's domain types when those change. They run in
//!   [`LayoutSyncSet`], scheduled `.before(LayoutProduceSet)`.
//!
//! The trait-Component `Arc<dyn _>` can't be downcast back to a concrete
//! type without `Any` — so the startup system also stores parallel
//! `Arc<FoldFilter>` / `Arc<SyntaxStyling>` Components on the editor
//! entity ([`EditorStylingArcs`]). The sync systems write through those.

use bevy::prelude::*;
use bevy_text_engine::{LayoutProduceSet, LayoutWrap, LineFilter, LineStyleSource};
use std::collections::HashSet;
use std::sync::Arc;

use super::styling::{FoldFilter, SyntaxStyling};
use crate::plugin::syntax_highlighting::EditorSyntaxState;
use crate::settings::{IndentationSettings, SyntaxSettings, ThemeSettings, WrappingSettings};
use crate::text_view::TextViewViewport;
use crate::types::{CodeEditor, FoldState};
use bevy_text_engine::FontConfig;

/// System set for sync systems that update trait-Component interior state
/// from editor-domain inputs. Scheduled `.before(LayoutProduceSet)` so the
/// engine's layout system observes this frame's changes.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LayoutSyncSet;

/// Legacy alias kept so existing call sites that referenced
/// `DisplayMapSet` for ordering still compile.
pub type DisplayMapSet = LayoutSyncSet;

/// Concrete `Arc`s for the editor's trait-Component impls. The
/// `LineFilter` / `LineStyleSource` Components hold the same `Arc`s as
/// `dyn` upcasts; this Component preserves the concrete type so the sync
/// systems can call write-through methods (`set_hidden_lines`,
/// `set_theme`) without an `Any` downcast.
#[derive(Component)]
pub(crate) struct EditorStylingArcs {
    pub fold_filter: Arc<FoldFilter>,
    pub syntax_styling: Arc<SyntaxStyling>,
}

pub struct DisplayMapPlugin;

impl Plugin for DisplayMapPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            LayoutSyncSet
                .after(crate::plugin::ApplyStateSet)
                .before(LayoutProduceSet),
        );
        // Engine's `LayoutProduceSet` is scheduled by `TextEnginePlugin`;
        // we configure it to live inside `RenderingSet` so downstream
        // observers (cursor / selection) see the freshly-built layout.
        app.configure_sets(Update, LayoutProduceSet.in_set(crate::plugin::RenderingSet));

        // Must run after `init_editor_syntax` (in `SyntaxPlugin`) so the
        // per-entity `EditorSyntaxState` is queryable when we wire up
        // `LineStyleSource`.
        app.add_systems(
            Startup,
            insert_styling_components
                .after(crate::plugin::syntax_highlighting::init_editor_syntax),
        );
        app.add_systems(
            Update,
            (sync_fold_filter, sync_syntax_styling, sync_layout_wrap)
                .in_set(LayoutSyncSet),
        );
    }
}

/// On startup, attach the editor's trait-Component impls to every
/// `CodeEditor` entity. `LineFilter` / `LineStyleSource` need
/// configuration (the editor's specific impls + the per-entity
/// `EditorSyntaxState` Arc), so they're not part of `CodeEditor`'s
/// `#[require]` cascade.
///
/// Runs after `init_editor_syntax` so the per-entity `EditorSyntaxState`
/// exists; we share its inner `Arc<RwLock<SyntaxInner>>` with the styling
/// component so the engine's `produce_layouts` reads from the same state
/// the parse pipeline writes to.
///
/// `LayoutWrap` IS in the cascade (via `TextView`'s `#[require]`) and
/// starts with the no-wrap default; `sync_layout_wrap` updates it as
/// `WrappingSettings` change.
pub(crate) fn insert_styling_components(
    mut commands: Commands,
    editors: Query<
        (Entity, &EditorSyntaxState),
        (With<CodeEditor>, Without<EditorStylingArcs>),
    >,
) {
    for (entity, syntax_state) in editors.iter() {
        let fold_filter: Arc<FoldFilter> = Arc::new(FoldFilter::new());
        let syntax_styling: Arc<SyntaxStyling> =
            Arc::new(SyntaxStyling::new(syntax_state.share_arc()));

        commands.entity(entity).insert((
            LineFilter(fold_filter.clone()),
            LineStyleSource(syntax_styling.clone()),
            EditorStylingArcs {
                fold_filter,
                syntax_styling,
            },
        ));
    }
}

/// Refresh the editor's `FoldFilter` snapshot when `FoldState` changes.
///
/// Walks `FoldState.regions`, expands each folded region into its hidden
/// line indices, and writes the resulting set into the `FoldFilter`'s
/// interior `RwLock`. The engine's `produce_layouts` then sees the new
/// visibility on its next call.
pub(crate) fn sync_fold_filter(
    editors: Query<(&FoldState, &EditorStylingArcs), (With<CodeEditor>, Changed<FoldState>)>,
) {
    for (fold_state, arcs) in editors.iter() {
        let mut hidden = HashSet::new();
        for region in &fold_state.regions {
            if !region.is_folded {
                continue;
            }
            // The fold hides lines (start_line + 1 ..= end_line) — match
            // `FoldRegion::hides_line`'s semantics.
            for line in (region.start_line + 1)..=region.end_line {
                hidden.insert(line);
            }
        }
        arcs.fold_filter.set_hidden_lines(hidden);
    }
}

/// Refresh the `SyntaxStyling` Component's theme/foreground when
/// `ThemeSettings` or `SyntaxSettings` changes.
///
/// The styling's `version()` is driven by the underlying `tree_version`
/// (parse pipeline). Theme-only changes don't bump `tree_version`, so we
/// also trigger a layout rebuild by touching `TextViewState.content_version`
/// indirectly: actually no — theme tweaks change the `style()` output, so
/// we bump a dedicated `style_epoch` on `SyntaxStyling`. To keep the diff
/// small, we instead rely on Bevy's `Changed<ThemeSettings>` here and let
/// the user expect a one-frame lag (theme changes are user-initiated and
/// rare).
pub(crate) fn sync_syntax_styling(
    editors: Query<&EditorStylingArcs, With<CodeEditor>>,
    theme: Res<ThemeSettings>,
    syntax_settings: Res<SyntaxSettings>,
) {
    if !theme.is_changed() && !syntax_settings.is_changed() {
        return;
    }
    for arcs in editors.iter() {
        arcs.syntax_styling
            .set_theme(syntax_settings.theme.clone(), theme.foreground);
        // Bump the styling's invalidation counter so the engine
        // refingerprints and rebuilds the layout this frame.
        arcs.syntax_styling.bump_style_epoch();
    }
}

/// Refresh `LayoutWrap` from `WrappingSettings` + `IndentationSettings`.
pub(crate) fn sync_layout_wrap(
    mut editors: Query<
        (&TextViewViewport, &FontConfig, &mut LayoutWrap),
        With<CodeEditor>,
    >,
    wrapping: Res<WrappingSettings>,
    indentation: Res<IndentationSettings>,
) {
    for (viewport, font, mut wrap) in editors.iter_mut() {
        let char_width = font.char_width;
        let budget_px: Option<f32> = if wrapping.enabled {
            let viewport_text_w =
                (viewport.width as f32 - viewport.text_area_left).max(char_width);
            let budget = match wrapping.wrap_column {
                Some(col) => (col as f32) * char_width,
                None => viewport_text_w,
            };
            Some(budget.max(char_width))
        } else {
            None
        };
        let indent_px = if wrapping.enabled && wrapping.indent_wrapped_lines {
            indentation.indent_size as f32 * char_width
        } else {
            0.0
        };
        let next = LayoutWrap {
            budget_px,
            indent_px,
        };
        if wrap.budget_px != next.budget_px || wrap.indent_px != next.indent_px {
            *wrap = next;
        }
    }
}
