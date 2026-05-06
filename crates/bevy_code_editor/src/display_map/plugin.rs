//! `DisplayMapPlugin` — plumbs the editor's fold / syntax / wrap state into
//! the engine's per-frame layout system via plain-data Components.
//!
//! The engine's `produce_layouts` (in `bevy_text_engine::view::layout_builder`)
//! reads `HiddenLines` / `LineStyles` / `LayoutWrap` Components off each
//! `TextView` entity and drives layout production itself. This plugin owns:
//!
//! - A startup system that inserts default `HiddenLines` / `LineStyles`
//!   Components on every `CodeEditor` entity.
//! - Three producer systems (`produce_hidden_lines`, `produce_line_styles`,
//!   `sync_layout_wrap`) that recompute each Component when the editor's
//!   domain state changes. They run in [`LayoutSyncSet`], scheduled
//!   `.before(LayoutProduceSet)`.

use bevy::prelude::*;
use bevy_text_engine::{
    visible_buffer_range, FontConfig, HiddenLines, LayoutProduceSet, LayoutWrap, LineStyles,
    RunWithText, ScrollState, TextBuffer, TextViewViewport,
};
use std::collections::{HashMap, HashSet};

use super::styling::segs_to_runs;
use crate::plugin::syntax_highlighting::EditorSyntaxState;
use crate::settings::{IndentationSettings, SyntaxTheme, ThemeConfig, WrappingSettings};
use crate::types::{CodeEditor, FoldState};

/// System set for sync systems that update the engine's data Components
/// from editor-domain inputs. Scheduled `.before(LayoutProduceSet)` so the
/// engine's layout system observes this frame's changes.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LayoutSyncSet;

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
        // per-entity `EditorSyntaxState` is queryable when we wire up the
        // styling Components. Runs in both Startup and Update — paired with
        // `init_editor_syntax`'s same dual schedule so editors spawned at
        // runtime get their styling Components attached on the next tick.
        app.add_systems(
            Startup,
            insert_styling_components
                .after(crate::plugin::syntax_highlighting::init_editor_syntax),
        );
        app.add_systems(
            Update,
            insert_styling_components
                .after(crate::plugin::syntax_highlighting::init_editor_syntax)
                .in_set(crate::plugin::ApplyStateSet),
        );
        app.add_systems(
            Update,
            (produce_hidden_lines, produce_line_styles, sync_layout_wrap)
                .in_set(LayoutSyncSet),
        );
    }
}

/// On startup, attach default `HiddenLines` / `LineStyles` Components to
/// every `CodeEditor` entity that doesn't already have them. The producer
/// systems write into these on subsequent ticks.
pub(crate) fn insert_styling_components(
    mut commands: Commands,
    editors: Query<Entity, (With<CodeEditor>, Without<LineStyles>)>,
) {
    for entity in editors.iter() {
        commands
            .entity(entity)
            .insert((HiddenLines::default(), LineStyles::default()));
    }
}

/// Refresh the `HiddenLines` Component when `FoldState` changes.
///
/// Walks `FoldState.regions`, expands each folded region into its hidden
/// line indices, and writes the resulting set into `HiddenLines`. The
/// engine's `produce_layouts` then sees the new visibility on its next
/// call (same frame; `LayoutSyncSet` is `.before(LayoutProduceSet)`).
pub(crate) fn produce_hidden_lines(
    mut editors: Query<
        (&FoldState, &mut HiddenLines),
        (With<CodeEditor>, Changed<FoldState>),
    >,
) {
    for (fold_state, mut hidden) in editors.iter_mut() {
        let mut set = HashSet::new();
        for region in &fold_state.regions {
            if !region.is_folded {
                continue;
            }
            // The fold hides lines (start_line + 1 ..= end_line) — match
            // `FoldRegion::hides_line`'s semantics.
            for line in (region.start_line + 1)..=region.end_line {
                set.insert(line);
            }
        }
        *hidden = HiddenLines::new(set);
    }
}

/// Recompute styled runs for each editor's visible buffer-line window and
/// write them into the entity's `LineStyles` Component.
///
/// Runs when any input that affects styling output changes (per-entity
/// `Changed<>` filter): `TextBuffer`, `ScrollState`, `TextViewViewport`,
/// `HiddenLines`, `ThemeConfig`, `SyntaxTheme`, and (with `tree-sitter`)
/// `SyntaxTree`.
///
/// Uses [`visible_buffer_range`] to scope work to lines about to render —
/// the engine's layout system uses the same helper, so producer and
/// consumer agree by construction.
#[allow(clippy::type_complexity)]
pub(crate) fn produce_line_styles(
    mut editors: Query<
        (
            Entity,
            &TextBuffer,
            &ScrollState,
            &TextViewViewport,
            &FontConfig,
            Option<&LayoutWrap>,
            Option<&HiddenLines>,
            &mut EditorSyntaxState,
            &mut LineStyles,
            &ThemeConfig,
            &SyntaxTheme,
        ),
        With<CodeEditor>,
    >,
    #[cfg(feature = "tree-sitter")] state_changed: Query<
        Entity,
        (
            With<CodeEditor>,
            Or<(
                Changed<TextBuffer>,
                Changed<ScrollState>,
                Changed<TextViewViewport>,
                Changed<HiddenLines>,
                Changed<ThemeConfig>,
                Changed<SyntaxTheme>,
                Changed<bevy_tree_sitter::SyntaxTree>,
            )>,
        ),
    >,
    #[cfg(not(feature = "tree-sitter"))] state_changed: Query<
        Entity,
        (
            With<CodeEditor>,
            Or<(
                Changed<TextBuffer>,
                Changed<ScrollState>,
                Changed<TextViewViewport>,
                Changed<HiddenLines>,
                Changed<ThemeConfig>,
                Changed<SyntaxTheme>,
            )>,
        ),
    >,
) {
    let dirty: HashSet<Entity> = state_changed.iter().collect();
    if dirty.is_empty() {
        return;
    }

    for (
        entity,
        buffer,
        scroll,
        viewport,
        font,
        wrap,
        hidden,
        mut syntax,
        mut line_styles,
        theme,
        syntax_theme,
    ) in editors.iter_mut()
    {
        if !dirty.contains(&entity) {
            continue;
        }

        let wrap = wrap.copied().unwrap_or_default();
        let range = visible_buffer_range(buffer, scroll, viewport, font, wrap, hidden);
        if range.start >= range.end {
            *line_styles = LineStyles::new(HashMap::new(), 0..0);
            continue;
        }

        let mut by_line: HashMap<u32, Vec<RunWithText>> = HashMap::new();
        let total_lines = buffer.line_count();
        for buffer_line in range.start..range.end {
            if buffer_line >= total_lines {
                break;
            }
            // Skip hidden lines — styling them wastes work since the engine
            // won't render them anyway.
            if let Some(h) = hidden {
                if !h.is_visible(buffer_line) {
                    continue;
                }
            }
            let line_text: String = buffer.rope.line(buffer_line).to_string();
            let line_no_nl = line_text.strip_suffix('\n').unwrap_or(&line_text);
            let start_byte = buffer.rope.line_to_byte(buffer_line);

            let mut per_line = syntax.highlight_range(
                line_no_nl,
                buffer_line,
                buffer_line + 1,
                start_byte,
                syntax_theme,
                theme.foreground,
            );
            let segs = per_line.pop().unwrap_or_default();
            // Whitespace-only lines: the engine renders the rope text in
            // `default_fg`. Emitting an empty `Vec` matches that contract.
            if segs.iter().all(|s| s.text.trim().is_empty()) {
                continue;
            }
            by_line.insert(buffer_line as u32, segs_to_runs(&segs));
        }

        *line_styles = LineStyles::new(by_line, range.start as u32..range.end as u32);
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
