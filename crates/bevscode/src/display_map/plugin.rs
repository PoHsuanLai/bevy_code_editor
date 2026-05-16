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
use bevy_instanced_text_editor::RopeBuffer;
use bevy::prelude::*;
use bevy::ui::{ComputedNode, ScrollPosition};
use bevy_instanced_text::{
    visible_buffer_range, HiddenLines, LayoutProduceSet, LineStyles, MonoCellWidth, FormattedSpan,
    TextBounds, TextBuffer,
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
    // Per-entity pending edit: (dirty_range, line_shift, shift_pivot).
    // `None` dirty_range = full rebuild; line_shift != 0 means keys in by_line
    // at or after shift_pivot must be relocated before re-highlighting.
    mut dirty_lines: Local<HashMap<Entity, (Option<(u32, u32)>, i32, u32)>>,
) {
    let _span = bevy::prelude::info_span!("produce_line_styles").entered();
    for event in edit_events.read() {
        let start_row = event.delta.start_position.row;
        let old_end_row = event.delta.old_end_position.row;
        let new_end_row = event.delta.new_end_position.row;
        let line_delta = new_end_row as i32 - old_end_row as i32;
        // Dirty range covers the lines that changed content. For line-count
        // edits (Enter / backspace-over-newline) that's start_row..=new_end_row.
        let dirty_range = Some((start_row, new_end_row));
        let incoming = (dirty_range, line_delta, start_row);
        for entity in content_changed.iter() {
            match dirty_lines.entry(entity) {
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert(incoming);
                }
                std::collections::hash_map::Entry::Occupied(mut o) => {
                    let entry = o.get_mut();
                    match (entry.0.as_mut(), dirty_range) {
                        (Some((lo, hi)), Some((new_lo, new_hi))) => {
                            *lo = (*lo).min(new_lo);
                            *hi = (*hi).max(new_hi);
                            entry.1 += line_delta;
                            entry.2 = entry.2.min(start_row);
                        }
                        _ => *entry = (None, 0, 0),
                    }
                }
            }
        }
    }

    // When an async parse completes, union SyntaxTree::dirty_rows into our
    // dirty_lines map. `None` dirty_rows = full rebuild (first parse / huge edit).
    // Syntax completions don't shift line indices, so line_delta stays 0.
    #[cfg(feature = "tree-sitter")]
    for (entity, syntax_tree) in syntax_tree_changed.iter() {
        let incoming = (syntax_tree.dirty_rows, 0i32, 0u32);
        let entry = dirty_lines.entry(entity).or_insert(incoming);
        match (entry.0.as_mut(), syntax_tree.dirty_rows) {
            (Some((lo, hi)), Some((new_lo, new_hi))) => {
                *lo = (*lo).min(new_lo);
                *hi = (*hi).max(new_hi);
            }
            // Either side is None → full rebuild needed.
            _ => entry.0 = None,
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
        scroll,
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
        let visible = visible_buffer_range(&**buffer, scroll.y, viewport_height, text_area_top, line_height, mono.px, wrap, hidden);
        if visible.start >= visible.end {
            *line_styles = LineStyles::new(HashMap::new());
            syntax.covered = 0..0;
            dirty_lines.remove(&entity);
            continue;
        }

        let total_lines = buffer.len_lines();

        // Extend the highlight window past the engine's render window so
        // `by_line` stays warm across small scroll deltas — same idea as
        // Zed's syntax-cache margin: render tight, cache wide.
        const HIGHLIGHT_LOOKAHEAD_LINES: usize = 64;
        let range = visible.start.saturating_sub(HIGHLIGHT_LOOKAHEAD_LINES)
            ..visible.end.saturating_add(HIGHLIGHT_LOOKAHEAD_LINES).min(total_lines);

        // Determine which lines to (re)highlight this frame.
        // `None` = full rebuild. Content edits without a matching edit event
        // (e.g. set_text) also get a full rebuild.
        let (dirty_range, line_shift, shift_pivot) = if needs_full {
            (None, 0i32, 0u32)
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
        let mut by_line: HashMap<u32, Vec<FormattedSpan>> = if !is_incremental {
            HashMap::new()
        } else {
            // Clone the existing Arc'd map so we can patch it, then apply any
            // line-index shift caused by insertions/deletions of newlines.
            let mut map = (*line_styles.by_line).clone();
            if line_shift != 0 {
                // Collect keys that need to move, in the right order to avoid
                // clobbering: shift down (negative delta) → process ascending,
                // shift up (positive delta) → process descending.
                let mut to_shift: Vec<u32> = map
                    .keys()
                    .copied()
                    .filter(|&k| k >= shift_pivot)
                    .collect();
                if line_shift < 0 {
                    to_shift.sort_unstable();
                } else {
                    to_shift.sort_unstable_by(|a, b| b.cmp(a));
                }
                for old_key in to_shift {
                    if let Some(val) = map.remove(&old_key) {
                        let new_key = (old_key as i32 + line_shift) as u32;
                        map.insert(new_key, val);
                    }
                }
            }
            map
        };

        // On a full rebuild the covered range expands to the full window.
        // On incremental the covered range doesn't shrink (scroll handles that).
        let new_covered = if !is_incremental {
            range.start as u32..range.end as u32
        } else {
            let old = &syntax.covered;
            old.start.min(range.start as u32)..old.end.max(range.end as u32)
        };

        // Batch all dirty lines into one highlight_range call — one tree-sitter
        // query instead of N. Collect (line_index, line_text) for visible,
        // non-hidden lines; build a single contiguous text block; call once;
        // distribute results back into by_line.
        let mut batch: Vec<(usize, String)> = Vec::new();
        for buffer_line in highlight_lines {
            if buffer_line >= total_lines {
                break;
            }
            if let Some(h) = hidden {
                if !h.is_visible(buffer_line) {
                    if by_line.remove(&(buffer_line as u32)).is_some() {
                        // map_changed handled below
                    }
                    continue;
                }
            }
            let line_text: String = buffer.line(buffer_line).to_string();
            batch.push((buffer_line, line_text));
        }

        let mut map_changed = false;

        // Remove hidden lines that were in the dirty range.
        if let Some(h) = hidden {
            for &(li, _) in &batch {
                if !h.is_visible(li) {
                    if by_line.remove(&(li as u32)).is_some() {
                        map_changed = true;
                    }
                }
            }
        }

        if !batch.is_empty() {
            // Build a single text block: lines joined with \n, no trailing \n.
            // Record each line's start byte in the block for splitting results.
            let batch_start_byte = buffer.line_to_byte(batch[0].0);
            let mut block = String::new();
            let mut line_offsets: Vec<usize> = Vec::with_capacity(batch.len());
            for (_, line_text) in &batch {
                line_offsets.push(block.len());
                let no_nl = line_text.strip_suffix('\n').unwrap_or(line_text);
                block.push_str(no_nl);
                block.push('\n');
            }
            // Strip the trailing \n added above.
            block.pop();

            let _hl_span = bevy::prelude::info_span!("highlight_line").entered();
            #[cfg(feature = "tree-sitter")]
            let per_line_segs = if let Some(st) = syntax_tree {
                syntax.highlight_range(
                    &block,
                    batch_start_byte,
                    st,
                    buffer.rope(),
                    syntax_theme,
                    theme.foreground,
                )
            } else {
                vec![vec![]; batch.len()]
            };
            #[cfg(not(feature = "tree-sitter"))]
            let per_line_segs =
                syntax.highlight_range(&block, batch_start_byte, syntax_theme, theme.foreground);

            for (i, (buffer_line, _)) in batch.iter().enumerate() {
                let segs = per_line_segs.get(i).cloned().unwrap_or_default();
                if segs.iter().all(|s| s.text.trim().is_empty()) {
                    if by_line.remove(&(*buffer_line as u32)).is_some() {
                        map_changed = true;
                    }
                } else {
                    by_line.insert(*buffer_line as u32, segs_to_runs(&segs));
                    map_changed = true;
                }
            }
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
            indentation.tab_width as f32 * char_width
        } else {
            0.0
        };
        let next = TextBounds { width, indent_px };
        if wrap.width != next.width || wrap.indent_px != next.indent_px {
            *wrap = next;
        }
    }
}
