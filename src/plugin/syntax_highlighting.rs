//! Syntax highlighting plugin
//!
//! Manages syntax highlighting as a Bevy resource, completely decoupled from editor state.
//! Also provides caching and debouncing for efficient highlighting during scrolling.

use crate::syntax::{SyntaxProvider, TreeSitterProvider};
use crate::text_view::TextViewState;
use crate::types::{CodeEditor, CodeEditorState, LineSegment, SyntaxCacheState};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use std::collections::VecDeque;

/// Resource that holds the syntax highlighting provider
#[derive(Resource)]
pub struct SyntaxResource {
    #[cfg(feature = "tree-sitter")]
    provider: Option<TreeSitterProvider>,

    /// Version counter incremented each time the syntax tree is updated
    /// Used to detect when highlighting needs to be refreshed
    #[cfg(feature = "tree-sitter")]
    pub tree_version: u64,
}

impl SyntaxResource {
    /// Create a new syntax resource (no provider initially)
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "tree-sitter")]
            provider: None,
            #[cfg(feature = "tree-sitter")]
            tree_version: 0,
        }
    }

    /// Set the tree-sitter provider
    #[cfg(feature = "tree-sitter")]
    pub fn set_provider(&mut self, provider: TreeSitterProvider) {
        self.provider = Some(provider);
    }

    /// Get mutable reference to the provider
    #[cfg(feature = "tree-sitter")]
    pub fn provider_mut(&mut self) -> Option<&mut TreeSitterProvider> {
        self.provider.as_mut()
    }

    /// Get readonly access to the tree-sitter tree (for folding, etc.)
    #[cfg(feature = "tree-sitter")]
    pub fn tree(&self) -> Option<&tree_sitter::Tree> {
        self.provider.as_ref()?.tree()
    }

    /// Check if syntax highlighting is available
    pub fn is_available(&self) -> bool {
        #[cfg(feature = "tree-sitter")]
        {
            self.provider
                .as_ref()
                .map(|p| p.is_available())
                .unwrap_or(false)
        }

        #[cfg(not(feature = "tree-sitter"))]
        {
            false
        }
    }

    /// Highlight a range of lines (lazy highlighting)
    #[cfg(feature = "tree-sitter")]
    pub fn highlight_range(
        &mut self,
        text: &str,
        start_line: usize,
        end_line: usize,
        start_byte: usize,
        theme: &crate::settings::SyntaxTheme,
        default_color: Color,
    ) -> Vec<Vec<crate::types::LineSegment>> {
        if let Some(provider) = &mut self.provider {
            provider.highlight_range(text, start_line, end_line, start_byte, theme, default_color)
        } else {
            // Return plain text
            text.lines()
                .map(|line| {
                    if line.trim().is_empty() {
                        vec![]
                    } else {
                        vec![crate::types::LineSegment {
                            text: line.to_string(),
                            color: default_color,
                            background: None,
                            corner_radius: 0.0,
                            font_scale: 0.0,
                            skew: 0.0,
                        }]
                    }
                })
                .collect()
        }
    }

    /// Invalidate the tree-sitter tree (like Zed does when content changes)
    #[cfg(feature = "tree-sitter")]
    pub fn invalidate_tree(&mut self) {
        if let Some(provider) = &mut self.provider {
            provider.invalidate_tree();
        }
    }

    /// Update the parse tree with new rope
    #[cfg(feature = "tree-sitter")]
    pub fn update_tree(&mut self, rope: &ropey::Rope) {
        if let Some(provider) = &mut self.provider {
            provider.update_tree(rope);
        }
    }

    /// Clone the parse state for async parsing.
    ///
    /// Edits have already been applied to the tree via `apply_sync_edit()`, so
    /// the cloned tree is ready for incremental re-parse without further edits.
    #[cfg(feature = "tree-sitter")]
    pub fn clone_parse_state(
        &mut self,
    ) -> (
        Option<tree_sitter::Parser>,
        Option<tree_sitter::Language>,
        Option<tree_sitter::Tree>,
    ) {
        if let Some(provider) = &mut self.provider {
            let parser = if let Some(ref language) = provider.cached_language {
                let mut new_parser = tree_sitter::Parser::new();
                if new_parser.set_language(language).is_ok() {
                    Some(new_parser)
                } else {
                    None
                }
            } else {
                None
            };

            // Clone the tree — edits already applied synchronously
            let tree = provider.cached_tree.clone();

            (parser, provider.cached_language.clone(), tree)
        } else {
            (None, None, None)
        }
    }

    /// Set the parsed tree from async task (also restores parser and rope)
    #[cfg(feature = "tree-sitter")]
    pub fn set_parsed_tree(&mut self, tree: tree_sitter::Tree, rope: &ropey::Rope) {
        if let Some(provider) = &mut self.provider {
            provider.cached_tree = Some(tree);
            // Cache the rope for highlighting (clone is cheap - Rope uses Arc internally)
            provider.cached_rope = Some(rope.clone());
            // Recreate parser if needed
            if provider.cached_parser.is_none() {
                if let Some(ref language) = provider.cached_language {
                    let mut parser = tree_sitter::Parser::new();
                    if parser.set_language(language).is_ok() {
                        provider.cached_parser = Some(parser);
                    }
                }
            }

            // Increment tree version to signal that highlighting should be refreshed
            self.tree_version += 1;
        }
    }

    /// Apply an edit synchronously to the cached tree (tree interpolation).
    ///
    /// This updates the tree's byte offsets without re-parsing, keeping it valid
    /// for highlighting queries while the async re-parse runs in the background.
    #[cfg(feature = "tree-sitter")]
    pub fn apply_sync_edit(&mut self, edit: tree_sitter::InputEdit, rope: &ropey::Rope) {
        if let Some(provider) = &mut self.provider {
            provider.apply_sync_edit(edit, rope);
        }
    }
}

impl Default for SyntaxResource {
    fn default() -> Self {
        Self::new()
    }
}

// ========== Highlight Cache ==========

/// A cached range of highlighted lines
/// NOTE: We only cache by content_version, not tree_version.
/// When tree updates, stale entity detection handles re-highlighting.
#[derive(Clone)]
struct CachedRange {
    start_line: usize,
    end_line: usize,
    content_version: u64,
    lines: Vec<Vec<LineSegment>>,
}

/// Cache of highlighted line ranges using a sliding window
/// Keeps the most recently highlighted ranges in memory
#[derive(Resource)]
pub struct HighlightCache {
    /// LRU cache of highlighted ranges
    ranges: VecDeque<CachedRange>,
    /// Maximum number of ranges to keep
    max_ranges: usize,
    /// Debounce timer for highlighting
    pub last_highlight_time: f64,
    /// Minimum time between highlights (ms)
    pub debounce_ms: f64,
}

impl Default for HighlightCache {
    fn default() -> Self {
        Self {
            ranges: VecDeque::new(),
            max_ranges: 20, // Keep last 20 ranges (covers scrolling up/down)
            last_highlight_time: 0.0,
            debounce_ms: 50.0, // Only highlight every 50ms (20fps) - more aggressive than VS Code's 200ms
        }
    }
}

impl HighlightCache {
    /// Check if we should debounce (skip) highlighting right now
    pub fn should_debounce(&self, current_time: f64) -> bool {
        (current_time - self.last_highlight_time) < self.debounce_ms
    }

    /// Update the last highlight time
    pub fn mark_highlighted(&mut self, current_time: f64) {
        self.last_highlight_time = current_time;
    }

    /// Get cached highlights if available
    /// Returns Some if the requested range is fully covered by cache
    /// NOTE: Only checks content_version, not tree_version. Tree version changes are handled
    /// by the stale entity detection system which marks entities for rebuild when tree updates.
    pub fn get(
        &mut self,
        start_line: usize,
        end_line: usize,
        content_version: u64,
        _tree_version: u64,
    ) -> Option<Vec<Vec<LineSegment>>> {
        // Look for exact match or overlapping range
        let mut found_idx: Option<(usize, usize, usize)> = None;
        for (idx, range) in self.ranges.iter().enumerate() {
            if range.content_version == content_version
                && range.start_line <= start_line
                && range.end_line >= end_line
            {
                let offset = start_line - range.start_line;
                let count = end_line - start_line;
                found_idx = Some((idx, offset, count));
                break;
            }
        }

        if let Some((idx, offset, count)) = found_idx {
            // Extract the subset we need first
            let result: Vec<Vec<LineSegment>> = self.ranges[idx]
                .lines
                .iter()
                .skip(offset)
                .take(count)
                .cloned()
                .collect();

            // Move to front (LRU) - now we can mutate
            if idx > 0 {
                let range = self.ranges.remove(idx).unwrap();
                self.ranges.push_front(range);
            }

            Some(result)
        } else {
            None
        }
    }

    /// Store highlighted lines in cache
    pub fn insert(
        &mut self,
        start_line: usize,
        end_line: usize,
        content_version: u64,
        _tree_version: u64,
        lines: Vec<Vec<LineSegment>>,
    ) {
        // Remove old entries if cache is full
        if self.ranges.len() >= self.max_ranges {
            self.ranges.pop_back();
        }

        // Add to front (most recently used)
        self.ranges.push_front(CachedRange {
            start_line,
            end_line,
            content_version,
            lines,
        });
    }

    /// Clear cache (call when content changes)
    pub fn clear(&mut self) {
        self.ranges.clear();
    }
}

// ========== Async Parsing Logic ==========

/// Component to track async parse tasks
#[cfg(feature = "tree-sitter")]
#[derive(Component)]
pub struct ParseTask {
    task: Task<Option<tree_sitter::Tree>>,
    content_version: u64,
}

/// Update tree-sitter tree asynchronously to avoid blocking frames
#[cfg(feature = "tree-sitter")]
pub(crate) fn update_syntax_tree(
    mut commands: Commands,
    mut editor_query: Query<
        (
            &mut CodeEditorState,
            &mut SyntaxCacheState,
            &mut TextViewState,
        ),
        With<CodeEditor>,
    >,
    mut syntax: ResMut<SyntaxResource>,
    mut highlight_cache: ResMut<HighlightCache>,
    mut parse_task_query: Query<(Entity, &mut ParseTask)>,
) {
    let Ok((_state, mut syntax_cache, mut tv)) = editor_query.single_mut() else {
        return;
    };

    // Check if there's a completed parse task
    if let Some((entity, mut parse_task)) = parse_task_query.iter_mut().next() {
        // Poll the task without blocking
        if let Some(tree) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut parse_task.task))
        {
            if let Some(tree) = tree {
                // Update the syntax provider with the completed tree and current rope
                // This increments syntax.tree_version, which will trigger a re-render automatically
                syntax.set_parsed_tree(tree, &tv.rope);
                syntax_cache.last_highlighted_version = parse_task.content_version;

                // Clear the highlight cache when tree-sitter finishes
                highlight_cache.clear();

                // Bump content_version so the line glyph cache is fully invalidated
                // (otherwise cached uncolored glyphs from the pre-parse render persist).
                // Set last_highlighted_version to match so we don't re-trigger a parse.
                tv.content_version += 1;
                syntax_cache.last_highlighted_version = tv.content_version;
                tv.dirty_lines = None;
                tv.needs_update = true;
            }
            // Remove the completed task
            commands.entity(entity).despawn();
        }
        // Task still running, don't start a new one
        return;
    }

    // Only start a new parse if content changed and no task is running
    if tv.content_version != syntax_cache.last_highlighted_version && syntax.is_available() {
        let rope = tv.rope.clone();
        let content_version = tv.content_version;

        // Clone the provider's state — edits already applied via apply_sync_edit()
        let (parser, language, cached_tree) = syntax.clone_parse_state();

        // Spawn async parse task
        let task_pool = AsyncComputeTaskPool::get();
        let task =
            task_pool.spawn(async move { parse_tree_async(rope, parser, language, cached_tree) });

        commands.spawn(ParseTask {
            task,
            content_version,
        });
    }
}

#[cfg(feature = "tree-sitter")]
fn parse_tree_async(
    rope: ropey::Rope,
    mut parser: Option<tree_sitter::Parser>,
    language: Option<tree_sitter::Language>,
    cached_tree: Option<tree_sitter::Tree>,
) -> Option<tree_sitter::Tree> {
    use crate::syntax::tree_sitter::RopeReader;

    let mut reader = RopeReader::new(&rope);
    let mut callback =
        |byte_offset: usize, _position: tree_sitter::Point| -> &[u8] { reader.read(byte_offset) };

    // Incremental parse — edits already applied to tree via apply_sync_edit()
    if let Some(ref tree) = cached_tree {
        if let Some(ref mut parser) = parser {
            if let Some(new_tree) = parser.parse_with(&mut callback, Some(tree)) {
                return Some(new_tree);
            }
        }
    } else if let Some(ref lang) = language {
        // First parse - initialize parser
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

// ========== Edit Recording for Incremental Parsing ==========

#[cfg(feature = "tree-sitter")]
/// Helper function to convert a byte offset to a tree-sitter Point (row, column)
/// Used during async parsing - not called on main thread anymore for edit recording
pub(crate) fn byte_to_point(rope: &ropey::Rope, byte_offset: usize) -> tree_sitter::Point {
    // Clamp byte offset to valid range
    let byte_offset = byte_offset.min(rope.len_bytes());

    // Convert byte offset to char offset
    let char_offset = rope.byte_to_char(byte_offset);

    // Get line and column from char offset
    let line = rope.char_to_line(char_offset);
    let line_start_char = rope.line_to_char(line);
    let column_char = char_offset - line_start_char;

    // Convert column from char offset to byte offset within the line
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

#[cfg(feature = "tree-sitter")]
/// System that sends TextEditEvent when pending_tree_sitter_edit is set
/// This runs before record_edits_for_incremental_parsing to ensure edits are recorded
fn send_text_edit_events(
    mut editor_query: Query<
        (&mut CodeEditorState, &mut SyntaxCacheState, &TextViewState),
        With<CodeEditor>,
    >,
    mut writer: MessageWriter<crate::types::events::TextEditEvent>,
) {
    let Ok((_state, mut syntax_cache, tv)) = editor_query.single_mut() else {
        return;
    };
    if let Some((start_byte, old_end_byte, new_end_byte)) =
        syntax_cache.pending_tree_sitter_edit.take()
    {
        writer.write(crate::types::events::TextEditEvent::new(
            start_byte,
            old_end_byte,
            new_end_byte,
            tv.content_version,
        ));
    }
}

#[cfg(feature = "tree-sitter")]
/// System that applies edits synchronously to the cached tree (tree interpolation).
///
/// This runs after send_text_edit_events. By applying `tree.edit()` on the main
/// thread, the tree stays valid for highlighting queries while the async re-parse
/// runs in the background — eliminating the color flash on keystroke.
fn record_edits_for_incremental_parsing(
    editor_query: Query<(&CodeEditorState, &SyntaxCacheState, &TextViewState), With<CodeEditor>>,
    mut syntax: ResMut<SyntaxResource>,
    mut events: MessageReader<crate::types::events::TextEditEvent>,
) {
    let Ok((_state, _syntax_cache, tv)) = editor_query.single() else {
        return;
    };
    for event in events.read() {
        // Compute Points on main thread — these are sub-μs O(log n) rope lookups
        let start_position = byte_to_point(&tv.rope, event.start_byte);
        let old_end_position = byte_to_point(&tv.rope, event.old_end_byte);
        let new_end_position = byte_to_point(&tv.rope, event.new_end_byte);

        let edit = tree_sitter::InputEdit {
            start_byte: event.start_byte,
            old_end_byte: event.old_end_byte,
            new_end_byte: event.new_end_byte,
            start_position,
            old_end_position,
            new_end_position,
        };

        // Apply edit to the cached tree immediately (tree interpolation)
        syntax.apply_sync_edit(edit, &tv.rope);
    }
}

// ========== Plugin ==========

/// Syntax highlighting plugin
pub struct SyntaxPlugin;

impl Plugin for SyntaxPlugin {
    fn build(&self, app: &mut App) {
        // Insert the syntax resource
        app.insert_resource(SyntaxResource::new());

        // Insert the highlight cache
        app.insert_resource(HighlightCache::default());

        // Register the TextEditEvent for cross-plugin communication
        // This allows LSP and other plugins to listen for text changes
        app.add_message::<crate::types::events::TextEditEvent>();

        // Add systems for tree-sitter incremental parsing
        #[cfg(feature = "tree-sitter")]
        {
            // These must run in ApplyStateSet so the tree is interpolated
            // BEFORE RenderingSet calls highlight_range()
            app.add_systems(
                Update,
                (
                    // First: send events for pending edits
                    send_text_edit_events,
                    // Second: apply edits to tree synchronously (tree interpolation)
                    record_edits_for_incremental_parsing,
                )
                    .chain()
                    .in_set(crate::plugin::ApplyStateSet),
            );

            // Async parse task polling — also in ApplyStateSet
            app.add_systems(
                Update,
                update_syntax_tree.in_set(crate::plugin::ApplyStateSet),
            );
        }
    }
}
