//! Code folding
//!
//! Fold-region detection runs as an async task on `AsyncComputeTaskPool`
//! when the `tree-sitter` feature is enabled. Without tree-sitter, `FoldState`
//! stays on the entity but `regions` remains empty — no folding is detected.
//!
//! No gutter chevron renderer is included. Downstream consumers wanting
//! `▶`/`▼` indicators read `FoldState` + `ScrollState` + `TextViewport`
//! and emit whatever entity they prefer (Sprite, Text2d, or — best —
//! `RectOverlay`s into `TextViewOverlays` so they go through the engine's
//! GPU instanced batch). Click-to-toggle stays wired up via `on_gutter_click`
//! in `input/mouse.rs` regardless of whether anything is rendered there.
#![allow(dead_code)]

use crate::text_view::TextBuffer;
use crate::types::*;
use bevy::prelude::*;

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
type FoldDetectQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static FoldState,
        &'static TextBuffer,
        &'static bevy_tree_sitter::SyntaxTree,
    ),
    (With<CodeEditor>, Changed<bevy_tree_sitter::SyntaxTree>),
>;

#[cfg(feature = "tree-sitter")]
pub(crate) fn spawn_fold_detect_tasks(
    mut commands: Commands,
    editor_query: FoldDetectQuery,
    in_flight: Query<&FoldDetectTask>,
) {
    let busy: std::collections::HashSet<Entity> = in_flight.iter().map(|t| t.target).collect();

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
        let Some(regions) = block_on(futures_lite::future::poll_once(&mut task.task)) else {
            continue;
        };

        if let Ok(mut fold_state) = editors.get_mut(task.target) {
            // If a fresher tree version arrived after we kicked off, our
            // result is stale — skip the write and let the next
            // `spawn_fold_detect_tasks` tick respawn.
            if fold_state.content_version != task.tree_version {
                // Build a HashMap keyed by (start_line, end_line) for O(1)
                // lookup of prior is_folded flags. Without this the merge is
                // O(N²) — sqlite3.c has thousands of regions and the linear
                // find-per-region cost dominates the apply system.
                let prior: std::collections::HashMap<(usize, usize), bool> = fold_state
                    .regions
                    .iter()
                    .map(|r| ((r.start_line, r.end_line), r.is_folded))
                    .collect();

                // Build the new region list (carrying prior is_folded flags)
                // into a temp Vec so we can compare structurally before
                // deciding to write. If nothing changed, skip the write to
                // avoid firing `Changed<FoldState>` — that cascades into
                // `produce_hidden_lines` → `Changed<HiddenLines>` →
                // `produce_line_styles` full window rebuild.
                let mut new_regions: Vec<FoldRegion> = Vec::with_capacity(regions.len());
                for mut region in regions {
                    if let Some(&was_folded) = prior.get(&(region.start_line, region.end_line)) {
                        region.is_folded = was_folded;
                    }
                    new_regions.push(region);
                }

                let unchanged = new_regions.len() == fold_state.regions.len()
                    && new_regions
                        .iter()
                        .zip(fold_state.regions.iter())
                        .all(|(a, b)| a == b);

                if unchanged {
                    // Update content_version without firing Changed<FoldState>.
                    fold_state.bypass_change_detection().content_version = task.tree_version;
                } else {
                    fold_state.regions = new_regions;
                    fold_state.content_version = task.tree_version;
                }
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

pub struct FoldingPlugin;

impl Plugin for FoldingPlugin {
    fn build(&self, _app: &mut App) {
        _app.register_type::<crate::types::fold::GotoLineState>()
            .register_type::<crate::types::fold::FoldState>();
        #[cfg(feature = "tree-sitter")]
        _app.register_type::<crate::types::fold::FoldKind>()
            .register_type::<crate::types::fold::FoldRegion>();

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
    }
}
