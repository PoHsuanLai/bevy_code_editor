//! Async tree-sitter parse machinery.
//!
//! Building blocks for off-main-thread parsing — consumers (like an editor)
//! kick off a parse with [`spawn_parse`] when their content version moves
//! ahead of the last parsed version, then poll completed [`ParseTask`]
//! components via [`poll_parse_tasks`] to receive `ParseCompleted` events.
//!
//! The parse runs on `AsyncComputeTaskPool`. Polling uses
//! `futures_lite::future::poll_once` so the system never blocks the main
//! thread — load-bearing for editor responsiveness on large files.

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use ropey::Rope;

use crate::tree_sitter::RopeReader;

/// A pending parse task, attached as a component to a transient entity.
#[derive(Component)]
pub struct ParseTask {
    task: Task<Option<tree_sitter::Tree>>,
    /// Content version at the time the parse was kicked off. Consumers use
    /// this to detect "the parse I see has the same content as my buffer."
    pub content_version: u64,
    /// Editor / view entity this parse belongs to. Echoed back on the
    /// completion event so multi-view consumers can route results.
    pub target: Entity,
}

/// Emitted by [`poll_parse_tasks`] when a `ParseTask` finishes.
///
/// `tree` is `Some` when the parse produced a tree, `None` if it bailed
/// (no parser, no language, or parser returned `None`). Routing the result
/// back into a `TreeSitterProvider` is the consumer's job.
#[derive(Message, Clone)]
pub struct ParseCompleted {
    pub target: Entity,
    pub content_version: u64,
    pub tree: Option<tree_sitter::Tree>,
}

/// Spawn a `ParseTask` entity that re-parses `rope` against the given
/// parser/language/cached_tree state.
///
/// `cached_tree` should already have any pending edits applied via
/// `tree.edit()` (see `TreeSitterProvider::apply_sync_edit`); this is the
/// "tree interpolation" step that lets the synchronous tree stay valid for
/// query work while we re-parse off the main thread.
pub fn spawn_parse(
    commands: &mut Commands,
    target: Entity,
    content_version: u64,
    rope: Rope,
    parser: Option<tree_sitter::Parser>,
    language: Option<tree_sitter::Language>,
    cached_tree: Option<tree_sitter::Tree>,
) {
    let task_pool = AsyncComputeTaskPool::get();
    let task =
        task_pool.spawn(async move { parse_tree_async(rope, parser, language, cached_tree) });
    commands.spawn(ParseTask {
        task,
        content_version,
        target,
    });
}

/// System that polls every in-flight `ParseTask`. For each task that's
/// finished, writes a `ParseCompleted` event and despawns the task entity.
///
/// Runs in `Update` by default (registered by [`crate::TreeSitterPlugin`]).
pub fn poll_parse_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut ParseTask)>,
    mut completed: MessageWriter<ParseCompleted>,
) {
    for (entity, mut parse_task) in tasks.iter_mut() {
        let Some(tree) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut parse_task.task))
        else {
            continue;
        };

        completed.write(ParseCompleted {
            target: parse_task.target,
            content_version: parse_task.content_version,
            tree,
        });
        commands.entity(entity).despawn();
    }
}

/// Pure parse worker — runs on the async pool, no Bevy types beyond the
/// inputs. Tries incremental parse against `cached_tree` first, falls back
/// to a fresh parse from `language` if there's no cached tree.
fn parse_tree_async(
    rope: Rope,
    mut parser: Option<tree_sitter::Parser>,
    language: Option<tree_sitter::Language>,
    cached_tree: Option<tree_sitter::Tree>,
) -> Option<tree_sitter::Tree> {
    let mut reader = RopeReader::new(&rope);
    let mut callback =
        |byte_offset: usize, _position: tree_sitter::Point| -> &[u8] { reader.read(byte_offset) };

    if let Some(ref tree) = cached_tree {
        if let Some(ref mut parser) = parser {
            if let Some(new_tree) = parser.parse_with(&mut callback, Some(tree)) {
                return Some(new_tree);
            }
        }
    } else if let Some(ref lang) = language {
        if parser.is_none() {
            let mut new_parser = tree_sitter::Parser::new();
            if new_parser.set_language(lang).is_ok() {
                parser = Some(new_parser);
            }
        }

        if let Some(ref mut parser) = parser {
            return parser.parse_with(&mut callback, None);
        }
    }

    None
}

/// Helper: convert a byte offset to a tree-sitter `Point` (row, column-in-bytes).
///
/// Cheap O(log n) rope lookup — fine to call on the main thread for every
/// edit before kicking off the async parse.
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
