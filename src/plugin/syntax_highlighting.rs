//! Syntax highlighting plugin
//!
//! Manages syntax highlighting as a Bevy resource, completely decoupled from editor state.
//! Also provides caching and debouncing for efficient highlighting during scrolling.

use bevy::prelude::*;
use std::collections::VecDeque;
use crate::syntax::{SyntaxProvider, TreeSitterProvider};
use crate::types::{LineSegment, CodeEditorState};

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
            self.provider.as_ref().map(|p| p.is_available()).unwrap_or(false)
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

    /// Clone the parse state for async parsing (returns parser, language, tree, edits, deferred_edits)
    /// Note: Creates new parser/clones tree to avoid blocking main thread access
    #[cfg(feature = "tree-sitter")]
    pub fn clone_parse_state(&mut self) -> (
        Option<tree_sitter::Parser>,
        Option<tree_sitter::Language>,
        Option<tree_sitter::Tree>,
        Vec<tree_sitter::InputEdit>,
        Vec<crate::syntax::tree_sitter::DeferredEdit>,
    ) {
        if let Some(provider) = &mut self.provider {
            // Create a new parser for the async task
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

            // Clone the tree (tree-sitter trees can be cloned for concurrent access)
            let tree = provider.cached_tree.clone();

            // Clone the pending edits
            let edits = provider.pending_edits.clone();

            // Clone the deferred edits (byte positions only - Points calculated in async task)
            let deferred_edits = provider.deferred_edits.clone();

            // Clear pending edits since we're processing them
            provider.pending_edits.clear();
            provider.deferred_edits.clear();

            (
                parser,
                provider.cached_language.clone(),
                tree,
                edits,
                deferred_edits,
            )
        } else {
            (None, None, None, Vec::new(), Vec::new())
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

    /// Record an edit for incremental parsing (deferred - byte positions only)
    /// Points will be calculated lazily during async parse to avoid blocking the main thread
    #[cfg(feature = "tree-sitter")]
    pub fn record_edit_deferred(
        &mut self,
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
    ) {
        if let Some(provider) = &mut self.provider {
            provider.record_edit_deferred(
                start_byte,
                old_end_byte,
                new_end_byte,
            );
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
#[derive(Clone)]
struct CachedRange {
    start_line: usize,
    end_line: usize,
    content_version: u64,
    tree_version: u64,
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
    pub fn get(&mut self, start_line: usize, end_line: usize, content_version: u64, tree_version: u64) -> Option<Vec<Vec<LineSegment>>> {
        // Look for exact match or overlapping range
        let mut found_idx: Option<(usize, usize, usize)> = None;
        for (idx, range) in self.ranges.iter().enumerate() {
            if range.content_version == content_version
                && range.tree_version == tree_version
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
            let result: Vec<Vec<LineSegment>> = self.ranges[idx].lines
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
    pub fn insert(&mut self, start_line: usize, end_line: usize, content_version: u64, tree_version: u64, lines: Vec<Vec<LineSegment>>) {
        // Remove old entries if cache is full
        if self.ranges.len() >= self.max_ranges {
            self.ranges.pop_back();
        }

        // Add to front (most recently used)
        self.ranges.push_front(CachedRange {
            start_line,
            end_line,
            content_version,
            tree_version,
            lines,
        });
    }

    /// Clear cache (call when content changes)
    pub fn clear(&mut self) {
        self.ranges.clear();
    }
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
    mut state: ResMut<CodeEditorState>,
    mut writer: MessageWriter<crate::events::TextEditEvent>,
) {
    if let Some((start_byte, old_end_byte, new_end_byte)) = state.pending_tree_sitter_edit.take() {
        writer.write(crate::events::TextEditEvent::new(
            start_byte,
            old_end_byte,
            new_end_byte,
            state.content_version,
        ));
    }
}

#[cfg(feature = "tree-sitter")]
/// System that listens for TextEditEvent and records edits for incremental parsing
/// This runs after send_text_edit_events to process the sent events
/// OPTIMIZATION: Only store byte positions - Points are calculated lazily during async parse
fn record_edits_for_incremental_parsing(
    mut syntax: ResMut<SyntaxResource>,
    mut events: MessageReader<crate::events::TextEditEvent>,
) {
    for event in events.read() {
        // Record the edit with deferred Point calculation
        // This avoids expensive rope traversals on the main thread
        syntax.record_edit_deferred(
            event.start_byte,
            event.old_end_byte,
            event.new_end_byte,
        );
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
        app.add_message::<crate::events::TextEditEvent>();

        // Add systems for tree-sitter incremental parsing
        #[cfg(feature = "tree-sitter")]
        {
            app.add_systems(Update, (
                // First: send events for pending edits
                send_text_edit_events,
                // Second: record events for incremental parsing
                record_edits_for_incremental_parsing,
            ).chain());
        }
    }
}
