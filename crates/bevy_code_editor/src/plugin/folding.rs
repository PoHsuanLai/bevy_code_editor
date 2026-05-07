//! Code folding
#![allow(dead_code)]

use super::to_bevy_coords_left_aligned;
use crate::settings::{ThemeConfig, UiSettings};
use crate::text_view::{ScrollState, TextBuffer, TextViewViewport};
use crate::types::*;
use bevy::prelude::*;
use bevy_text_engine::FontConfig;

#[cfg(feature = "tree-sitter")]
use bevy::tasks::{block_on, futures_lite, AsyncComputeTaskPool, Task};

/// In-flight fold-detection task. Lives on a child entity so the parent's
/// `Changed<FoldState>` doesn't fire on each task spawn/despawn. Mirrors
/// the `bevy_tree_sitter::ParseTask` pattern.
#[cfg(feature = "tree-sitter")]
#[derive(Component)]
pub(crate) struct FoldDetectTask {
    task: Task<Vec<FoldRegion>>,
    /// `SyntaxTree::tree_version` at kick-off; written into
    /// `FoldState::content_version` on completion to single-flight.
    tree_version: usize,
    /// The editor entity whose `FoldState` this task targets.
    target: Entity,
}

/// Spawn an async fold-detection task whenever an editor's `SyntaxTree`
/// produces a fresher version than the one already reflected in
/// `FoldState`. The walk happens on `AsyncComputeTaskPool`; the apply
/// step ([`apply_fold_detect_tasks`]) writes the result back on the
/// main thread, preserving prior `is_folded` flags.
///
/// Single-flight per editor: while a task is in flight for `entity`,
/// `Changed<SyntaxTree>` cycles on that entity are ignored until the
/// in-flight task lands. The check we then run in `apply` (`tree_version`
/// equality) catches the case where another tree version arrived during
/// the walk and re-spawns naturally on the next tick.
#[cfg(feature = "tree-sitter")]
pub(crate) fn spawn_fold_detect_tasks(
    mut commands: Commands,
    editor_query: Query<
        (Entity, &FoldState, &TextBuffer, &bevy_tree_sitter::SyntaxTree),
        (With<CodeEditor>, Changed<bevy_tree_sitter::SyntaxTree>),
    >,
    in_flight: Query<&FoldDetectTask>,
) {
    let busy: std::collections::HashSet<Entity> =
        in_flight.iter().map(|t| t.target).collect();

    for (entity, fold_state, buffer, syntax_tree) in editor_query.iter() {
        if busy.contains(&entity) {
            continue;
        }
        let Some(tree) = syntax_tree.tree.as_ref() else {
            continue;
        };
        let tree_version = syntax_tree.tree_version as usize;
        if fold_state.content_version == tree_version {
            continue;
        }

        // Cheap clones: `Tree` is ref-counted FFI-side, `Rope` shares
        // chunks via Arc. Both are `Send + 'static`, suitable for the
        // worker.
        let tree_clone = tree.clone();
        let rope_clone = buffer.rope.clone();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let mut regions: Vec<FoldRegion> = Vec::new();
            let root = tree_clone.root_node();
            collect_foldable_regions(&root, &rope_clone, &mut regions, false);
            regions
        });

        commands.spawn((
            FoldDetectTask {
                task,
                tree_version,
                target: entity,
            },
            ChildOf(entity),
        ));
    }
}

/// Poll in-flight `FoldDetectTask`s; merge completed results into the
/// target editor's `FoldState` (preserving prior `is_folded` flags) and
/// despawn the task entity.
#[cfg(feature = "tree-sitter")]
pub(crate) fn apply_fold_detect_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut FoldDetectTask)>,
    mut editors: Query<&mut FoldState, With<CodeEditor>>,
) {
    for (task_entity, mut task) in tasks.iter_mut() {
        let Some(regions) =
            block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };

        if let Ok(mut fold_state) = editors.get_mut(task.target) {
            // If a fresher tree version arrived after we kicked off, our
            // result is stale — skip the write and let the next
            // `spawn_fold_detect_tasks` tick respawn.
            if fold_state.content_version != task.tree_version {
                let old_regions = std::mem::take(&mut fold_state.regions);
                fold_state.regions.reserve(regions.len());
                for mut region in regions {
                    if let Some(old) = old_regions.iter().find(|r| {
                        r.start_line == region.start_line
                            && r.end_line == region.end_line
                    }) {
                        region.is_folded = old.is_folded;
                    }
                    fold_state.regions.push(region);
                }
                fold_state.content_version = task.tree_version;
                fold_state.enabled = true;
            }
        }

        commands.entity(task_entity).despawn();
    }
}

#[cfg(feature = "tree-sitter")]
pub(crate) fn collect_foldable_regions(
    node: &bevy_tree_sitter::ts::Node,
    rope: &ropey::Rope,
    regions: &mut Vec<FoldRegion>,
    parent_is_foldable_construct: bool,
) {
    let kind = node.kind();

    // Check if this is a function-like or class-like construct that contains a body
    let is_foldable_construct = matches!(
        kind,
        // Function-like constructs
        "function_item" | "function_definition" | "function_declaration" |
        "method_definition" | "method_declaration" | "function_expression" |
        "arrow_function" | "lambda" | "closure_expression" |
        // Class-like constructs
        "class_definition" | "class_declaration" | "struct_item" |
        "enum_item" | "interface_declaration" | "trait_item" | "impl_item"
    );

    // Skip block/body nodes that are direct children of foldable constructs
    // to avoid creating duplicate fold regions at the same line
    let skip_this_node = parent_is_foldable_construct
        && matches!(
            kind,
            "block"
                | "compound_statement"
                | "statement_block"
                | "body"
                | "field_declaration_list"
                | "declaration_list"
                | "enum_variant_list"
        );

    if !skip_this_node {
        // Check if this node is foldable
        if let Some(region) = node_to_fold_region(node, rope) {
            // Only add regions that span multiple lines
            if region.end_line > region.start_line {
                regions.push(region);
            }
        }
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_foldable_regions(&child, rope, regions, is_foldable_construct);
    }
}

#[cfg(feature = "tree-sitter")]
pub(crate) fn node_to_fold_region(
    node: &bevy_tree_sitter::ts::Node,
    rope: &ropey::Rope,
) -> Option<FoldRegion> {
    let kind = node.kind();

    // Map tree-sitter node kinds to FoldKind
    // These mappings work for most languages (Rust, JavaScript, TypeScript, Python, etc.)
    let fold_kind = match kind {
        // Function-like constructs
        "function_item"
        | "function_definition"
        | "function_declaration"
        | "method_definition"
        | "method_declaration"
        | "function_expression"
        | "arrow_function"
        | "lambda"
        | "closure_expression" => Some(FoldKind::Function),

        // Class-like constructs
        "class_definition"
        | "class_declaration"
        | "struct_item"
        | "enum_item"
        | "interface_declaration"
        | "trait_item"
        | "impl_item" => Some(FoldKind::Class),

        // Block constructs
        "block" | "compound_statement" | "statement_block" | "if_expression" | "if_statement"
        | "match_expression" | "switch_statement" | "for_statement" | "for_expression"
        | "while_statement" | "while_expression" | "loop_expression" | "try_statement"
        | "catch_clause" | "finally_clause" => Some(FoldKind::Block),

        // Import/use statements (when grouped)
        "use_declaration" | "import_statement" | "import_declaration" => Some(FoldKind::Imports),

        // Comments
        "comment" | "block_comment" | "line_comment" | "doc_comment" => Some(FoldKind::Comment),

        // String literals (multi-line)
        "string_literal" | "raw_string_literal" | "template_string" => Some(FoldKind::Literal),

        // Region markers (e.g., #region in C#)
        "region" | "preproc_region" => Some(FoldKind::Region),

        // Array/object literals (when multi-line)
        "array" | "array_expression" | "object" | "object_expression" | "struct_expression"
        | "tuple_expression" => Some(FoldKind::Other),

        _ => None,
    };

    fold_kind.and_then(|kind| {
        let start_line = node.start_position().row;
        let end_line = node.end_position().row;

        // Bounds check: tree might have stale line numbers after text deletion
        let line_count = rope.len_lines();
        if start_line >= line_count || end_line >= line_count {
            return None;
        }

        // Calculate indent level from the start of the line
        let _line_start = rope.line_to_char(start_line);
        let line = rope.line(start_line);
        let mut indent_level = 0;
        for c in line.chars() {
            match c {
                ' ' => indent_level += 1,
                '\t' => indent_level += 4,
                _ => break,
            }
        }
        indent_level /= 4; // Convert to indent levels

        Some(FoldRegion {
            start_line,
            end_line,
            is_folded: false,
            kind,
            indent_level,
        })
    })
}

/// Fallback for when tree-sitter is not enabled
#[cfg(not(feature = "tree-sitter"))]
pub(crate) fn detect_foldable_regions(
    mut editor_query: Query<(&TextBuffer, &mut FoldState), With<CodeEditor>>,
) {
    for (buffer, mut fold_state) in editor_query.iter_mut() {
        // Only update when content changes
        if fold_state.content_version == buffer.content_version as usize {
            continue;
        }

        fold_state.content_version = buffer.content_version as usize;

        // Simple brace-matching based folding as fallback
        let mut regions: Vec<FoldRegion> = Vec::new();
        let mut brace_stack: Vec<(usize, usize)> = Vec::new(); // (line, indent_level)

        for line_idx in 0..buffer.rope.len_lines() {
            let line = buffer.rope.line(line_idx);
            let line_str: String = line.chars().collect();

            // Calculate indent level
            let mut indent_level = 0;
            for c in line_str.chars() {
                match c {
                    ' ' => indent_level += 1,
                    '\t' => indent_level += 4,
                    _ => break,
                }
            }
            indent_level /= 4;

            // Look for opening braces at end of line
            let trimmed = line_str.trim_end();
            if trimmed.ends_with('{') || trimmed.ends_with('[') || trimmed.ends_with('(') {
                brace_stack.push((line_idx, indent_level));
            }

            // Look for closing braces at start of line (after whitespace)
            let trimmed_start = line_str.trim_start();
            if trimmed_start.starts_with('}')
                || trimmed_start.starts_with(']')
                || trimmed_start.starts_with(')')
            {
                if let Some((start_line, start_indent)) = brace_stack.pop() {
                    if line_idx > start_line {
                        regions.push(FoldRegion {
                            start_line,
                            end_line: line_idx,
                            is_folded: false,
                            kind: FoldKind::Block,
                            indent_level: start_indent,
                        });
                    }
                }
            }
        }

        // Preserve fold state for existing regions
        let old_regions = std::mem::take(&mut fold_state.regions);
        for mut region in regions {
            if let Some(old) = old_regions
                .iter()
                .find(|r| r.start_line == region.start_line && r.end_line == region.end_line)
            {
                region.is_folded = old.is_folded;
            }
            fold_state.regions.push(region);
        }

        fold_state.enabled = true;
    }
}

// Duplicate imports removed - already imported at top of file

pub struct FoldingPlugin;

impl Plugin for FoldingPlugin {
    fn build(&self, _app: &mut App) {
        _app.register_type::<crate::types::fold::FoldIndicator>()
            .register_type::<crate::types::fold::FoldKind>()
            .register_type::<crate::types::fold::FoldRegion>()
            .register_type::<crate::types::fold::FoldState>()
            .register_type::<crate::types::fold::GotoLineState>();

        // With tree-sitter: walk the parse tree off the main thread and
        // apply the result on completion. The walk is genuinely O(tree
        // nodes) — for sqlite3.c (~7 MB) it's ~90 ms, which would otherwise
        // hitch the frame after every parse-completion (e.g. delete-all).
        //
        // Without tree-sitter: brace-matching fallback is fast enough to
        // run synchronously each tick.
        #[cfg(feature = "tree-sitter")]
        _app.add_systems(
            Update,
            (
                spawn_fold_detect_tasks.in_set(super::ApplyStateSet),
                apply_fold_detect_tasks
                    .after(spawn_fold_detect_tasks)
                    .in_set(super::ApplyStateSet),
            ),
        );
        #[cfg(not(feature = "tree-sitter"))]
        _app.add_systems(
            Update,
            detect_foldable_regions.in_set(super::ApplyStateSet),
        );

        _app.add_systems(
            Update,
            update_fold_indicators
                .after(super::gpu_line_numbers::update_gpu_line_numbers)
                .in_set(super::RenderingSet),
        );
    }
}

/// Sync per-editor fold-gutter indicator entities (▶/▼ glyphs) with the
/// current `FoldState` and viewport.
///
/// Runs only when something an indicator depends on actually changed. Idle
/// frames (typing-paused, no scroll) are a complete no-op. Each indicator
/// write is also gated on a real value change so we don't trip Bevy's
/// `Changed<>` markers — those would cascade into `Text2d` layout, sprite
/// extraction, and a render-graph re-walk every frame.
#[allow(clippy::type_complexity)]
pub(crate) fn update_fold_indicators(
    mut commands: Commands,
    editor_query: Query<
        (
            Entity,
            &TextBuffer,
            &ScrollState,
            &TextViewViewport,
            &FoldState,
            &FontConfig,
            &ThemeConfig,
        ),
        With<CodeEditor>,
    >,
    dirty_editors: Query<
        Entity,
        (
            With<CodeEditor>,
            Or<(
                Changed<FoldState>,
                Changed<ScrollState>,
                Changed<TextViewViewport>,
                Changed<FontConfig>,
            )>,
        ),
    >,
    ui: Res<UiSettings>,
    mut indicator_query: Query<(
        Entity,
        &FoldIndicator,
        &mut Transform,
        &mut Text2d,
        &mut Visibility,
    )>,
) {
    let dirty: std::collections::HashSet<Entity> = dirty_editors.iter().collect();
    if dirty.is_empty() && !ui.is_changed() {
        return;
    }

    let mut existing_indicators: std::collections::HashMap<usize, Entity> =
        std::collections::HashMap::with_capacity(indicator_query.iter().len());
    for (entity, indicator, _, _, _) in indicator_query.iter() {
        existing_indicators.insert(indicator.line_index, entity);
    }
    let mut used_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut any_enabled = false;

    for (editor_entity, buffer, scroll, viewport, fold_state, font, theme) in editor_query.iter() {
        if !fold_state.enabled || !ui.show_line_numbers {
            continue;
        }
        // If nothing on this editor changed and UiSettings didn't flip,
        // its indicators are already in the right state — skip.
        if !ui.is_changed() && !dirty.contains(&editor_entity) {
            // Still mark its visible folds as "used" so we don't hide them
            // when running for a *different* editor that did change.
            let line_height = font.line_height;
            let visible_start_line = ((-scroll.scroll_offset) / line_height).floor() as usize;
            let visible_lines = ((viewport.height as f32 / line_height).ceil() as usize) + 2;
            let visible_end_line =
                (visible_start_line + visible_lines).min(buffer.rope.len_lines());
            for region in &fold_state.regions {
                if region.start_line >= visible_start_line
                    && region.start_line < visible_end_line
                {
                    used_indices.insert(region.start_line);
                }
            }
            any_enabled = true;
            continue;
        }
        any_enabled = true;

        let line_height = font.line_height;
        let font_size = font.font_size;
        let viewport_width = viewport.width as f32;
        let viewport_height = viewport.height as f32;

        let visible_start_line = ((-scroll.scroll_offset) / line_height).floor() as usize;
        let visible_lines = ((viewport_height / line_height).ceil() as usize) + 2;
        let visible_end_line = (visible_start_line + visible_lines).min(buffer.rope.len_lines());

        for region in &fold_state.regions {
            if region.start_line < visible_start_line || region.start_line >= visible_end_line {
                continue;
            }
            let line_idx = region.start_line;

            // O(n_folded_regions) — was O(n_regions) probe per indicator.
            let display_line = fold_state.actual_to_display_line(line_idx);
            // Skip if folded under an enclosing region (display row collapsed
            // into the parent's placeholder).
            if display_line < line_idx
                && fold_state
                    .regions
                    .iter()
                    .any(|r| r.is_folded && r.start_line < line_idx && r.end_line >= line_idx)
            {
                continue;
            }

            used_indices.insert(line_idx);

            let x_offset = viewport.gutter_width - 12.0;
            let y_offset = viewport.text_area_top
                + scroll.scroll_offset
                + (display_line as f32 * line_height);
            let translation = to_bevy_coords_left_aligned(
                x_offset,
                y_offset,
                viewport_width,
                viewport_height,
                0.0,
            );
            let indicator_char = if region.is_folded { "▶" } else { "▼" };

            if let Some(entity) = existing_indicators.get(&line_idx) {
                if let Ok((_, _, mut transform, mut text, mut visibility)) =
                    indicator_query.get_mut(*entity)
                {
                    if transform.translation != translation {
                        transform.translation = translation;
                    }
                    if text.0 != indicator_char {
                        text.0 = indicator_char.to_string();
                    }
                    if *visibility != Visibility::Visible {
                        *visibility = Visibility::Visible;
                    }
                }
            } else {
                let text_font = TextFont {
                    font: font.font.clone(),
                    font_size: font_size * 0.7,
                    ..default()
                };
                commands.spawn((
                    Text2d::new(indicator_char.to_string()),
                    text_font,
                    TextColor(theme.line_numbers.with_alpha(0.8)),
                    Transform::from_translation(translation),
                    FoldIndicator { line_index: line_idx },
                    Name::new(format!("FoldIndicator_{}", line_idx)),
                    Visibility::Visible,
                ));
            }
        }
    }

    if !any_enabled {
        // No editor has folding on; hide everything (only if not already hidden).
        for (_, _, _, _, mut visibility) in indicator_query.iter_mut() {
            if *visibility != Visibility::Hidden {
                *visibility = Visibility::Hidden;
            }
        }
    } else {
        for (_entity, indicator, _, _, mut visibility) in indicator_query.iter_mut() {
            if !used_indices.contains(&indicator.line_index)
                && *visibility != Visibility::Hidden
            {
                *visibility = Visibility::Hidden;
            }
        }
    }
}
