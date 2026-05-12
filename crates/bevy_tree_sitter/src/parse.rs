//! Component-driven async tree-sitter parsing.
//!
//! `parse_dirty` detects when `ParseSource::content_version()` outruns the
//! stored tree version, transitions the entity's `ParseState` to `InFlight`,
//! and writes the result back when the task completes. Single-flight per
//! entity; never blocks the main thread.

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use ropey::Rope;
use std::sync::Arc;

use crate::language::Language;
use crate::tree_sitter::RopeReader;

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

/// Per-entity parsed-tree state. Written by `parse_dirty` on completion;
/// filter `Changed<SyntaxTree>` to react when a new tree lands.
///
/// Not Reflect: `tree_sitter::Tree` owns FFI-side state.
#[derive(Component, Default)]
#[require(ParseState)]
pub struct SyntaxTree {
    pub tree: Option<tree_sitter::Tree>,
    pub content_version: u64,
    /// Bumps on each tree replacement so readers can cache derived data by
    /// tree identity instead of pointer equality.
    pub tree_version: u64,
    /// Buffer-line range dirtied by the edit(s) that produced this tree.
    /// `None` means the full visible window must be rehighlighted (e.g. first
    /// parse, or a huge edit that dropped the cached tree). Set by
    /// `record_edits_for_incremental_parsing` via `bypass_change_detection`
    /// and forwarded through the async task so `produce_line_styles` can do
    /// an incremental rehighlight instead of a full-window rebuild.
    pub dirty_rows: Option<(u32, u32)>,
}

impl SyntaxTree {
    /// Drop the cached tree and reset `content_version` to trigger a fresh parse.
    pub fn clear(&mut self) {
        self.tree = None;
        self.content_version = 0;
        self.tree_version = self.tree_version.wrapping_add(1);
    }
}

/// Per-entity parse pipeline state. Internal to this crate; hosts observe
/// results via `Changed<SyntaxTree>` rather than querying this component.
#[derive(Component, Default)]
pub(crate) enum ParseState {
    #[default]
    Idle,
    InFlight {
        task: Task<Option<tree_sitter::Tree>>,
        content_version: u64,
        dirty_rows: Option<(u32, u32)>,
    },
}

pub(crate) fn parse_dirty(
    mut targets: Query<(
        Entity,
        &Language,
        &ParseSourceComp,
        &mut SyntaxTree,
        &mut ParseState,
    )>,
) {
    for (_, language, source, mut syntax, mut state) in targets.iter_mut() {
        match &mut *state {
            ParseState::Idle => {
                let source_version = source.0.content_version();
                if source_version == syntax.content_version {
                    continue;
                }

                let Some(grammar) = language.tree_sitter.as_ref().map(|c| c.grammar.clone())
                else {
                    continue;
                };

                let rope = source.0.snapshot();
                let cached_tree = syntax.tree.clone();
                let dirty_rows = syntax.bypass_change_detection().dirty_rows;
                let task = AsyncComputeTaskPool::get()
                    .spawn(async move { parse_tree_async(rope, grammar, cached_tree) });

                *state = ParseState::InFlight {
                    task,
                    content_version: source_version,
                    dirty_rows,
                };
            }
            ParseState::InFlight {
                task,
                content_version,
                dirty_rows,
            } => {
                let Some(tree) =
                    futures_lite::future::block_on(futures_lite::future::poll_once(task))
                else {
                    continue;
                };

                let content_version = *content_version;
                let dirty_rows = *dirty_rows;
                *state = ParseState::Idle;

                if let Some(tree) = tree {
                    syntax.tree = Some(tree);
                    syntax.content_version = content_version;
                    syntax.tree_version = syntax.tree_version.wrapping_add(1);
                    syntax.dirty_rows = dirty_rows;
                } else {
                    let s = syntax.bypass_change_detection();
                    s.content_version = content_version;
                }
            }
        }
    }
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
    let line_start_byte = rope.char_to_byte(line_start_char);
    let column_byte = byte_offset - line_start_byte;
    tree_sitter::Point::new(line, column_byte)
}
