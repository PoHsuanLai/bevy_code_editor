//! Component-driven async tree-sitter parsing.
//!
//! Consumers attach three Components to an entity — [`Language`], a
//! [`ParseSource`] impl wrapped in [`ParseSourceComp`], and an empty
//! [`SyntaxTree`] — and [`parse_dirty`] (registered by
//! [`crate::TreeSitterPlugin`]) takes care of the rest:
//!
//! 1. Detects when `ParseSource::content_version()` outruns the version
//!    stored in the entity's [`SyntaxTree`].
//! 2. Spawns a [`ParseTask`] on a CHILD entity (so the parent's
//!    `Changed<SyntaxTree>` doesn't fire spuriously while a parse is
//!    in flight).
//! 3. When the task completes, writes the new tree into the parent's
//!    [`SyntaxTree`] Component (bumping `tree_version`) and despawns the
//!    child task entity.
//!
//! Single-flight per entity: a new parse for an entity won't be started
//! while one is already running. The parse runs on `AsyncComputeTaskPool`
//! and is polled with `futures_lite::future::poll_once`, so the main
//! thread never blocks waiting on it.
//!
//! `byte_to_point` is exposed so consumers can build `tree_sitter::InputEdit`s
//! to feed [`ParseSource::apply_edit`].

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use ropey::Rope;
use std::sync::Arc;

use crate::language::Language;
use crate::tree_sitter::RopeReader;

/// Opaque per-entity parse state — what the system kicks off async, what it
/// hands back when the parse finishes. Stored on a CHILD of the parsed
/// entity so the parent's `Changed<SyntaxTree>` filter only fires on real
/// updates (not on every spawn/despawn of a transient task entity).
#[derive(Component)]
pub struct ParseTask {
    task: Task<Option<tree_sitter::Tree>>,
    /// Content version at the time the parse was kicked off. The parent's
    /// [`SyntaxTree`] is updated with this version when the task completes.
    pub content_version: u64,
    /// The entity owning this parse — i.e. the parent that holds the
    /// [`SyntaxTree`]/`Language`/`ParseSourceComp`.
    pub target: Entity,
}

/// Per-line/buffer source the parser reads from.
///
/// Mirror of `LineFilter` / `LineStyleSource` in `bevy_text_engine`: a
/// trait Component lets the consumer plug their buffer (a `Rope`, a
/// `String` snapshot, whatever) into the parsing pipeline without forcing
/// `bevy_tree_sitter` to know about editor-specific buffer types.
///
/// Implementations are called from the main thread (for `content_version`
/// + `snapshot`) and from worker tasks (the cloned `Rope` they hand back).
/// `apply_edit` is called from the main thread before the next parse
/// kicks off — this is the "tree interpolation" entry point.
pub trait ParseSource: Send + Sync + 'static {
    /// Monotonically-increasing content version. The parser kicks off a new
    /// parse when this differs from the version recorded in the entity's
    /// [`SyntaxTree`].
    fn content_version(&self) -> u64;

    /// Snapshot of the buffer to parse. Cheap clone (`Rope` is Arc-backed).
    /// Called once per parse; the returned rope is moved to the worker.
    fn snapshot(&self) -> Rope;

    /// Apply a tree-sitter edit synchronously to whatever cached tree the
    /// consumer maintains. Optional: implementations that don't keep their
    /// own tree state can leave this as the default no-op.
    ///
    /// Called from the main thread — the consumer's edit-record system
    /// converts buffer edits into `InputEdit`s and forwards them here so
    /// the cached tree's byte offsets stay valid for highlight queries
    /// while the next async re-parse runs.
    fn apply_edit(&self, _edit: tree_sitter::InputEdit) {}
}

/// Component wrapping a [`ParseSource`] trait object.
///
/// Cheap to clone (`Arc` bump). Not Reflect: the inner trait is `dyn`,
/// which isn't reflectable.
#[derive(Component, Clone)]
pub struct ParseSourceComp(pub Arc<dyn ParseSource>);

impl ParseSourceComp {
    /// Convenience wrapper for `Arc::new(value)`.
    pub fn new<T: ParseSource>(value: T) -> Self {
        Self(Arc::new(value))
    }
}

/// Per-entity parsed-tree state.
///
/// Written by [`parse_dirty`] when an async parse completes; read by
/// consumers (highlight queries, fold detection, code outline) via a
/// regular `Query<&SyntaxTree>`. `Changed<SyntaxTree>` is the invalidation
/// hook — filter on it to react when a new tree lands.
///
/// Not Reflect: `tree_sitter::Tree` doesn't impl `Reflect` (it owns
/// FFI-side state via the C API), so this Component opts out. Consumers
/// inspecting via reflection will see the Component name but not its
/// contents — same trade-off as `LineStyleSource`.
#[derive(Component, Default)]
pub struct SyntaxTree {
    /// The parsed tree, if any. `None` until the first parse completes (or
    /// after [`Self::clear`]).
    pub tree: Option<tree_sitter::Tree>,
    /// `ParseSource::content_version()` at the time this tree was parsed.
    /// The pipeline kicks off a new parse whenever the source's version
    /// outruns this.
    pub content_version: u64,
    /// Bumps every time `tree` is replaced. Lets readers cache derived
    /// data keyed by tree identity instead of pointer equality.
    pub tree_version: u64,
}

impl SyntaxTree {
    /// Drop the cached tree. Resets `content_version` to 0 so the next
    /// `parse_dirty` tick will kick off a fresh parse.
    pub fn clear(&mut self) {
        self.tree = None;
        self.content_version = 0;
        self.tree_version = self.tree_version.wrapping_add(1);
    }
}

/// System that drives the per-entity parse pipeline.
///
/// Runs in `Update` (registered by [`crate::TreeSitterPlugin`]). For every
/// entity with `(Language, ParseSourceComp, &mut SyntaxTree)`:
///
/// - If a child `ParseTask` already exists for this entity, skip
///   (single-flight).
/// - Otherwise, if `ParseSource::content_version()` differs from
///   `SyntaxTree::content_version`, kick off an `AsyncComputeTaskPool`
///   parse on a child entity.
///
/// Then polls every existing `ParseTask`. For each one that's finished,
/// writes the parsed tree onto the parent's `SyntaxTree` (bumping
/// `tree_version`) and despawns the task entity.
#[allow(clippy::type_complexity)]
pub(crate) fn parse_dirty(
    mut commands: Commands,
    mut targets: Query<(Entity, &Language, &ParseSourceComp, &mut SyntaxTree)>,
    mut tasks: Query<(Entity, &mut ParseTask)>,
) {
    // Track which targets already have an in-flight parse so we don't kick
    // off a second one. Same target may have multiple completed tasks in
    // theory (race-free: completion + new spawn happen in the same system
    // call), but in practice we drain one per frame anyway.
    let mut in_flight: std::collections::HashSet<Entity> =
        std::collections::HashSet::new();
    for (_, task) in tasks.iter() {
        in_flight.insert(task.target);
    }

    // Kick off new parses for entities whose source version outruns the
    // tree's version, and that don't already have one in flight.
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

    // Drain completed tasks. We poll every task this frame; for finished
    // ones we write the result onto the parent and despawn the task entity.
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
                // Successful parse: write the new tree + bump versions.
                // This is the path that fires `Changed<SyntaxTree>` for
                // downstream readers.
                syntax.tree = Some(tree);
                syntax.content_version = completion.content_version;
                syntax.tree_version = syntax.tree_version.wrapping_add(1);
            } else {
                // Parse failed (no language, parser couldn't run). Record
                // the version we tried so we don't infinite-loop. Use
                // `bypass_change_detection` to keep `Changed<SyntaxTree>`
                // from firing on a no-op update — readers don't care
                // about failed parses.
                let s = syntax.bypass_change_detection();
                s.content_version = completion.content_version;
            }
        }
        commands.entity(task_entity).despawn();
    }
}

/// Internal carrier for a finished parse. Not exposed; the system writes
/// directly into the target's `SyntaxTree`.
struct ParseCompletion {
    target: Entity,
    content_version: u64,
    tree: Option<tree_sitter::Tree>,
}

/// Pure parse worker — runs on the async pool, no Bevy types beyond the
/// inputs. Tries incremental parse against `cached_tree` first, falls back
/// to a fresh parse from `grammar` if there's no cached tree.
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

/// Helper: convert a byte offset to a tree-sitter `Point` (row, column-in-bytes).
///
/// Cheap O(log n) rope lookup — fine to call on the main thread for every
/// edit before forwarding to [`ParseSource::apply_edit`].
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
