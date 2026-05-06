//! Component-driven async tree-sitter parsing.
//!
//! [`parse_dirty`] detects when `ParseSource::content_version()` outruns the
//! stored tree version, spawns a [`ParseTask`] on a child entity (so
//! `Changed<SyntaxTree>` doesn't fire while in-flight), and writes the result
//! back when complete. Single-flight per entity; never blocks the main thread.

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use ropey::Rope;
use std::sync::Arc;

use crate::language::Language;
use crate::tree_sitter::RopeReader;

/// Stored on a CHILD entity so the parent's `Changed<SyntaxTree>` doesn't fire
/// on every spawn/despawn of this transient task entity.
#[derive(Component)]
pub struct ParseTask {
    task: Task<Option<tree_sitter::Tree>>,
    /// Version at kick-off time; written into [`SyntaxTree`] on completion.
    pub content_version: u64,
    /// The parent entity that holds `SyntaxTree`/`Language`/`ParseSourceComp`.
    pub target: Entity,
}

/// Buffer interface for the parse pipeline. `content_version` and `snapshot`
/// are called on the main thread; the cloned `Rope` is moved to the worker.
/// `apply_edit` is called on the main thread before each parse to keep the
/// cached tree's byte offsets valid for highlight queries.
pub trait ParseSource: Send + Sync + 'static {
    /// Monotonically increasing; a new parse kicks off when this differs from
    /// the version stored in [`SyntaxTree`].
    fn content_version(&self) -> u64;

    /// Cheap clone (`Rope` is Arc-backed); moved to the async worker.
    fn snapshot(&self) -> Rope;

    /// Optional: implementations without their own cached tree can leave this as
    /// the default no-op.
    fn apply_edit(&self, _edit: tree_sitter::InputEdit) {}
}

/// Wraps a [`ParseSource`] trait object. Cheap to clone (`Arc` bump).
/// Not Reflect: `dyn` isn't reflectable.
#[derive(Component, Clone)]
pub struct ParseSourceComp(pub Arc<dyn ParseSource>);

impl ParseSourceComp {
    pub fn new<T: ParseSource>(value: T) -> Self {
        Self(Arc::new(value))
    }
}

/// Per-entity parsed-tree state. Written by [`parse_dirty`] on completion;
/// filter `Changed<SyntaxTree>` to react when a new tree lands.
///
/// Not Reflect: `tree_sitter::Tree` owns FFI-side state.
#[derive(Component, Default)]
pub struct SyntaxTree {
    pub tree: Option<tree_sitter::Tree>,
    pub content_version: u64,
    /// Bumps on each tree replacement so readers can cache derived data by
    /// tree identity instead of pointer equality.
    pub tree_version: u64,
}

impl SyntaxTree {
    /// Drop the cached tree and reset `content_version` to trigger a fresh parse.
    pub fn clear(&mut self) {
        self.tree = None;
        self.content_version = 0;
        self.tree_version = self.tree_version.wrapping_add(1);
    }
}

#[allow(clippy::type_complexity)]
pub(crate) fn parse_dirty(
    mut commands: Commands,
    mut targets: Query<(Entity, &Language, &ParseSourceComp, &mut SyntaxTree)>,
    mut tasks: Query<(Entity, &mut ParseTask)>,
) {
    let mut in_flight: std::collections::HashSet<Entity> =
        std::collections::HashSet::new();
    for (_, task) in tasks.iter() {
        in_flight.insert(task.target);
    }

    for (entity, language, source, syntax) in targets.iter_mut() {
        let source_version = source.0.content_version();
        if source_version == syntax.content_version {
            continue;
        }
        if in_flight.contains(&entity) {
            continue;
        }

        let Some(grammar) = language.tree_sitter.as_ref().map(|c| c.grammar.clone()) else {
            continue;
        };

        let rope = source.0.snapshot();
        let cached_tree = syntax.tree.clone();
        let task_pool = AsyncComputeTaskPool::get();
        let task = task_pool
            .spawn(async move { parse_tree_async(rope, grammar, cached_tree) });

        commands.spawn((
            ParseTask {
                task,
                content_version: source_version,
                target: entity,
            },
            ChildOf(entity),
        ));
        in_flight.insert(entity);
    }

    let mut completed: Vec<(Entity, ParseCompletion)> = Vec::new();
    for (task_entity, mut parse_task) in tasks.iter_mut() {
        let Some(tree) = futures_lite::future::block_on(
            futures_lite::future::poll_once(&mut parse_task.task),
        ) else {
            continue;
        };
        completed.push((
            task_entity,
            ParseCompletion {
                target: parse_task.target,
                content_version: parse_task.content_version,
                tree,
            },
        ));
    }

    for (task_entity, completion) in completed {
        if let Ok((_, _, _, mut syntax)) = targets.get_mut(completion.target) {
            if let Some(tree) = completion.tree {
                syntax.tree = Some(tree);
                syntax.content_version = completion.content_version;
                syntax.tree_version = syntax.tree_version.wrapping_add(1);
            } else {
                // Record the attempted version to avoid infinite-loop retries.
                // bypass_change_detection keeps Changed<SyntaxTree> silent on
                // failed parses — downstream readers don't care.
                let s = syntax.bypass_change_detection();
                s.content_version = completion.content_version;
            }
        }
        commands.entity(task_entity).despawn();
    }
}

struct ParseCompletion {
    target: Entity,
    content_version: u64,
    tree: Option<tree_sitter::Tree>,
}

/// Async worker: incremental parse against `cached_tree` if available,
/// otherwise full parse from `grammar`.
fn parse_tree_async(
    rope: Rope,
    grammar: tree_sitter::Language,
    cached_tree: Option<tree_sitter::Tree>,
) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&grammar).is_err() {
        return None;
    }

    let mut reader = RopeReader::new(&rope);
    let mut callback =
        |byte_offset: usize, _position: tree_sitter::Point| -> &[u8] { reader.read(byte_offset) };

    parser.parse_with(&mut callback, cached_tree.as_ref())
}

/// O(log n) rope lookup — safe to call on the main thread per edit.
pub fn byte_to_point(rope: &Rope, byte_offset: usize) -> tree_sitter::Point {
    let byte_offset = byte_offset.min(rope.len_bytes());
    let char_offset = rope.byte_to_char(byte_offset);
    let line = rope.char_to_line(char_offset);
    let line_start_char = rope.line_to_char(line);
    let column_char = char_offset - line_start_char;

    let line_slice = rope.line(line);
    let mut column_byte = 0;
    for (i, _) in line_slice.chars().enumerate() {
        if i >= column_char {
            break;
        }
        column_byte += line_slice.char(i).len_utf8();
    }

    tree_sitter::Point::new(line, column_byte)
}
