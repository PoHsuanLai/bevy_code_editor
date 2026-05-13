//! `DisplayMapPlugin` — plumbs the editor's fold / syntax / wrap state into
//! the engine's per-frame layout system via plain-data Components.
//!
//! The engine's `produce_layouts` (in `bevy_instanced_text::view::layout_builder`)
//! reads `HiddenLines` / `LineStyles` / `TextBounds` Components off each
//! `TextView` entity and drives layout production itself. This plugin owns:
//!
//! - A startup system that inserts default `HiddenLines` / `LineStyles`
//!   Components on every `CodeEditor` entity.
//! - Three producer systems (`produce_hidden_lines`, `produce_line_styles`,
//!   `sync_layout_wrap`) that recompute each Component when the editor's
//!   domain state changes. They run in [`LayoutSyncSet`], scheduled
//!   `.before(LayoutProduceSet)`.

use crate::types::events::TextEdited;
use bevy_text_editor::RopeBuffer;
use bevy::prelude::*;
use bevy::ui::ComputedNode;
use bevy::ui::ScrollPosition;
use bevy_instanced_text::{
    visible_buffer_range, HiddenLines, LayoutProduceSet, TextBounds, LineStyles, MonoCellWidth,
    RunWithText, SmoothScroll, TextBuffer,
};
use std::collections::{HashMap, HashSet};

use super::styling::segs_to_runs;
use crate::plugin::syntax_highlighting::EditorSyntaxState;
use crate::settings::{EditorTheme, Indentation, SyntaxColors, Wrapping};
use crate::types::CodeEditor;
#[cfg(feature = "tree-sitter")]
use crate::types::FoldState;

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
        // Engine's `LayoutProduceSet` is scheduled by `InstancedTextPlugin`;
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
            insert_styling_components.after(crate::plugin::syntax_highlighting::init_editor_syntax),
        );
        app.add_systems(
            Update,
            insert_styling_components
                .after(crate::plugin::syntax_highlighting::init_editor_syntax)
                .in_set(crate::plugin::ApplyStateSet),
        );
        app.add_systems(
            Update,
            (
                #[cfg(feature = "tree-sitter")]
                produce_hidden_lines,
                produce_line_styles,
                sync_layout_wrap,
            )
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
/// Only writes when the hidden-line set actually differs from the current one.
/// `FoldState` change-detection fires on every async fold-detection completion
/// (which preserves `is_folded` flags across reparses), so without this check
/// every reparse would invalidate `HiddenLines` and cascade into a full
/// `produce_line_styles` rebuild via `Changed<HiddenLines>`.
#[cfg(feature = "tree-sitter")]
type ProduceHiddenLinesQuery<'w, 's> = Query<
    'w,
    's,
    (&'static FoldState, &'static mut HiddenLines),
    (With<CodeEditor>, Changed<FoldState>),
>;

#[cfg(feature = "tree-sitter")]
pub(crate) fn produce_hidden_lines(mut editors: ProduceHiddenLinesQuery) {
    for (fold_state, mut hidden) in editors.iter_mut() {
        let mut set = HashSet::new();
        for region in &fold_state.regions {
            if !region.is_folded {
                continue;
            }
            for line in (region.start_line + 1)..=region.end_line {
                set.insert(line);
            }
        }
        if *hidden.0 != set {
            *hidden = HiddenLines::new(set);
        }
    }
}

/// Recompute styled runs for each editor's visible buffer-line window and
/// write them into the entity's `LineStyles` Component.
///
/// On a pure content edit (only `TextBuffer<RopeBuffer>` changed), only the lines
/// touched by the edit are re-highlighted and merged into the existing map —
/// unchanged lines keep their cached runs. On any other change (scroll,
/// viewport resize, theme swap, new parse tree, hidden-lines update) the
/// full visible window is rebuilt from scratch.
#[allow(clippy::type_complexity)]
pub(crate) fn produce_line_styles(
    mut editors: Query<
        (
            Entity,
            &TextBuffer<RopeBuffer>,
            &ScrollPosition,
            &SmoothScroll,
            &ComputedNode,
            &TextFont,
            &bevy::text::LineHeight,
            &MonoCellWidth,
            Option<&TextBounds>,
            Option<&HiddenLines>,
            &mut EditorSyntaxState,
            Option<&bevy_tree_sitter::SyntaxTree>,
            &mut LineStyles,
            &EditorTheme,
            &SyntaxColors,
        ),
        With<CodeEditor>,
    >,
    #[cfg(feature = "tree-sitter")] content_changed: Query<
        Entity,
        (With<CodeEditor>, Changed<TextBuffer<RopeBuffer>>),
    >,
    // Full viewport rebuild: layout/theme/viewport changes that invalidate
    // the entire visible window. Does NOT include Changed<SyntaxTree> —
    // that's handled incrementally via SyntaxTree::dirty_rows below.
    #[cfg(feature = "tree-sitter")] full_rebuild_changed: Query<
        Entity,
        (
            With<CodeEditor>,
            Or<(
                Changed<ScrollPosition>,
                Changed<ComputedNode>,
                Changed<HiddenLines>,
                Changed<EditorTheme>,
                Changed<SyntaxColors>,
            )>,
        ),
    >,
    // Async parse completions: SyntaxTree changed, but only the rows touched
    // by the original edit need rehighlighting. dirty_rows carries that range.
    #[cfg(feature = "tree-sitter")] syntax_tree_changed: Query<
        (Entity, &bevy_tree_sitter::SyntaxTree),
        (With<CodeEditor>, Changed<bevy_tree_sitter::SyntaxTree>),
    >,
    #[cfg(not(feature = "tree-sitter"))] content_changed: Query<
        Entity,
        (With<CodeEditor>, Changed<TextBuffer<RopeBuffer>>),
    >,
    #[cfg(not(feature = "tree-sitter"))] full_rebuild_changed: Query<
        Entity,
        (
            With<CodeEditor>,
            Or<(
                Changed<ScrollPosition>,
                Changed<ComputedNode>,
                Changed<HiddenLines>,
                Changed<EditorTheme>,
                Changed<SyntaxColors>,
            )>,
        ),
    >,
    mut edit_events: MessageReader<TextEdited>,
    // Per-entity dirty line range from the last edit. None = full rebuild needed.
    mut dirty_lines: Local<HashMap<Entity, Option<(u32, u32)>>>,
) {
    let _span = bevy::prelude::info_span!("produce_line_styles").entered();
    // Collect edit events: record the changed line range per entity.
    // Multiple edits in one frame are unioned. `None` means full rebuild.
    for event in edit_events.read() {
        // Line-count edits shift every buffer-line index after the edit point.
        // LineStyles.by_line is keyed by index so incremental rebuild leaves
        // stale entries under wrong keys. Force a full rebuild (None).
        let line_count_changed =
            event.delta.old_end_position.row != event.delta.new_end_position.row;
        let dirty = if line_count_changed {
            None
        } else {
            let start_row = event.delta.start_position.row;
            let end_row = event.delta.new_end_position.row;
            Some((start_row, end_row))
        };
        for entity in content_changed.iter() {
            let entry = dirty_lines.entry(entity).or_insert(dirty);
            match (entry.as_mut(), dirty) {
                (Some((lo, hi)), Some((new_lo, new_hi))) => {
                    *lo = (*lo).min(new_lo);
                    *hi = (*hi).max(new_hi);
                }
                // Either side is None → full rebuild.
                _ => *entry = None,
            }
        }
    }

    // When an async parse completes, union SyntaxTree::dirty_rows into our
    // dirty_lines map. `None` dirty_rows = full rebuild (first parse / huge edit).
    #[cfg(feature = "tree-sitter")]
    for (entity, syntax_tree) in syntax_tree_changed.iter() {
        let entry = dirty_lines.entry(entity).or_insert(syntax_tree.dirty_rows);
        match (entry.as_mut(), syntax_tree.dirty_rows) {
            (Some((lo, hi)), Some((new_lo, new_hi))) => {
                *lo = (*lo).min(new_lo);
                *hi = (*hi).max(new_hi);
            }
            // Either side is None → full rebuild needed.
            _ => *entry = None,
        }
    }

    let full_rebuild: HashSet<Entity> = full_rebuild_changed.iter().collect();
    let content_only: HashSet<Entity> = content_changed.iter().collect();
    #[cfg(feature = "tree-sitter")]
    let syntax_changed: HashSet<Entity> = syntax_tree_changed.iter().map(|(e, _)| e).collect();
    #[cfg(not(feature = "tree-sitter"))]
    let syntax_changed: HashSet<Entity> = HashSet::new();

    let any_dirty =
        !full_rebuild.is_empty() || !content_only.is_empty() || !syntax_changed.is_empty();
    if !any_dirty {
        dirty_lines.retain(|e, _| {
            content_only.contains(e) || full_rebuild.contains(e) || syntax_changed.contains(e)
        });
        return;
    }

    for (
        entity,
        buffer,
        scroll_pos,
        _smooth,
        computed,
        font,
        lh,
        mono,
        wrap,
        hidden,
        mut syntax,
        syntax_tree,
        mut line_styles,
        theme,
        syntax_theme,
    ) in editors.iter_mut()
    {
        let needs_full = full_rebuild.contains(&entity);
        let needs_content = content_only.contains(&entity);
        let needs_syntax = syntax_changed.contains(&entity);
        if !needs_full && !needs_content && !needs_syntax {
            continue;
        }

        let inv = computed.inverse_scale_factor();
        let viewport_height = computed.size().y * inv;
        let text_area_top = computed.content_inset().min_inset.y * inv;
        let wrap = wrap.copied().unwrap_or_default();
        let line_height = bevy_instanced_text::resolve_line_height(*lh, font.font_size);
        let range = visible_buffer_range(&**buffer, scroll_pos.y, viewport_height, text_area_top, line_height, mono.px, wrap, hidden);
        if range.start >= range.end {
            *line_styles = LineStyles::new(HashMap::new());
            syntax.covered = 0..0;
            dirty_lines.remove(&entity);
            continue;
        }

        let total_lines = buffer.len_lines();

        // Determine which lines to (re)highlight this frame.
        // `None` = full rebuild. Content edits without a matching edit event
        // (e.g. set_text) also get a full rebuild.
        let dirty_range: Option<(u32, u32)> = if needs_full {
            None
        } else {
            dirty_lines.get(&entity).copied().unwrap_or_default()
        };

        let highlight_lines: Box<dyn Iterator<Item = usize>> = match dirty_range {
            // Incremental: only highlight the lines the edit touched, clamped
            // to the visible window. Unchanged lines keep their cached runs.
            Some((dirty_start, dirty_end)) => {
                let lo = (dirty_start as usize).max(range.start).min(range.end);
                let hi = (dirty_end as usize + 1).min(range.end);
                Box::new(lo..hi)
            }
            // Full rebuild: highlight the entire visible window.
            None => Box::new(range.start..range.end),
        };

        // On a full rebuild start fresh; on incremental reuse the existing map.
        let is_incremental = dirty_range.is_some();
        let mut by_line: HashMap<u32, Vec<RunWithText>> = if !is_incremental {
            HashMap::new()
        } else {
            // Clone the existing Arc'd map so we can patch it.
            (*line_styles.by_line).clone()
        };

        // On a full rebuild the covered range expands to the full window.
        // On incremental the covered range doesn't shrink (scroll handles that).
        let new_covered = if !is_incremental {
            range.start as u32..range.end as u32
        } else {
            let old = &syntax.covered;
            old.start.min(range.start as u32)..old.end.max(range.end as u32)
        };

        let mut map_changed = false;
        for buffer_line in highlight_lines {
            if buffer_line >= total_lines {
                break;
            }
            if let Some(h) = hidden {
                if !h.is_visible(buffer_line) {
                    if by_line.remove(&(buffer_line as u32)).is_some() {
                        map_changed = true;
                    }
                    continue;
                }
            }
            let line_text: String = buffer.line(buffer_line).to_string();
            let line_no_nl = line_text.strip_suffix('\n').unwrap_or(&line_text);
            let start_byte = buffer.line_to_byte(buffer_line);

            let _hl_span = bevy::prelude::info_span!("highlight_line").entered();
            let mut per_line = {
                #[cfg(feature = "tree-sitter")]
                {
                    if let Some(st) = syntax_tree {
                        syntax.highlight_range(
                            line_no_nl,
                            start_byte,
                            st,
                            buffer.rope(),
                            syntax_theme,
                            theme.foreground,
                        )
                    } else {
                        vec![vec![]]
                    }
                }
                #[cfg(not(feature = "tree-sitter"))]
                syntax.highlight_range(line_no_nl, start_byte, syntax_theme, theme.foreground)
            };
            let segs = per_line.pop().unwrap_or_default();
            if segs.iter().all(|s| s.text.trim().is_empty()) {
                if by_line.remove(&(buffer_line as u32)).is_some() {
                    map_changed = true;
                }
                continue;
            }
            by_line.insert(buffer_line as u32, segs_to_runs(&segs));
            map_changed = true;
        }

        // Only write LineStyles when content actually changed. An unconditional
        // write creates a fresh Arc every frame, changing the Arc address and
        // triggering layout_miss_styles on every idle frame.
        let covered_changed = syntax.covered != new_covered;
        if map_changed || covered_changed || !is_incremental {
            *line_styles = LineStyles::new(by_line);
            syntax.covered = new_covered;
        }
        dirty_lines.remove(&entity);
    }
}

/// Refresh `TextBounds` from `Wrapping` + `Indentation`.
pub(crate) fn sync_layout_wrap(
    mut editors: Query<
        (
            &ComputedNode,
            &MonoCellWidth,
            &mut TextBounds,
            &Wrapping,
            &Indentation,
        ),
        With<CodeEditor>,
    >,
) {
    for (computed, mono, mut wrap, wrapping, indentation) in editors.iter_mut() {
        let char_width = mono.px;
        let width: Option<f32> = if wrapping.enabled {
            let inv = computed.inverse_scale_factor();
            let viewport_text_w =
                (computed.size().x * inv - computed.content_inset().min_inset.x * inv)
                    .max(char_width);
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
        let next = TextBounds { width, indent_px };
        if wrap.width != next.width || wrap.indent_px != next.indent_px {
            *wrap = next;
        }
    }
}
