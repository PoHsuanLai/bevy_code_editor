//! Core types for the code editor

use bevy::prelude::*;
use ropey::Rope;
use std::ops::Range;

#[cfg(feature = "tree-sitter")]
use tree_sitter_highlight::HighlightConfiguration;

#[cfg(feature = "lsp")]
use lsp_types::Url;

/// A segment of text with a specific color on a specific line
#[derive(Clone, Debug)]
pub struct LineSegment {
    pub text: String,
    pub color: Color,
}

/// Token with its highlight type and text content
#[derive(Clone, Debug)]
pub struct HighlightedToken {
    pub text: String,
    pub highlight_type: Option<String>,
}

/// Viewport dimensions resource (replaces dioxus-bevy's ViewportDimensions)
#[derive(Resource, Clone, Copy, Debug)]
pub struct ViewportDimensions {
    pub width: u32,
    pub height: u32,
    /// Horizontal offset for the editor content (useful for sidebars)
    pub offset_x: f32,
}

impl Default for ViewportDimensions {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            offset_x: 0.0,
        }
    }
}

/// Main editor state resource
#[derive(Resource)]
pub struct CodeEditorState {
    /// Text buffer (efficient rope data structure)
    pub rope: Rope,

    /// Cursor position (char index)
    pub cursor_pos: usize,

    /// Last cursor position (for detecting cursor movement)
    pub last_cursor_pos: usize,

    /// Selection start (None = no selection)
    pub selection_start: Option<usize>,

    /// Selection end
    pub selection_end: Option<usize>,

    /// Is editor focused
    pub is_focused: bool,

    /// Needs full re-render
    pub needs_update: bool,

    /// Only scroll changed, don't rebuild entities
    pub needs_scroll_update: bool,

    /// Cached highlighted tokens
    pub tokens: Vec<HighlightedToken>,

    /// Cached processed lines for rendering (optimization)
    pub lines: Vec<Vec<LineSegment>>,

    /// Cached highlight configuration (optional, for syntax highlighting)
    #[cfg(feature = "tree-sitter")]
    pub highlight_config: Option<HighlightConfiguration>,

    /// The URI of the document being edited (for LSP)
    #[cfg(feature = "lsp")]
    pub document_uri: Option<Url>,

    /// Document version for LSP synchronization (incremented on each change)
    #[cfg(feature = "lsp")]
    pub document_version: i32,

    /// Vertical scroll offset in pixels
    pub scroll_offset: f32,

    /// Horizontal scroll offset in pixels
    pub horizontal_scroll_offset: f32,

    /// Maximum content width (longest line in pixels)
    pub max_content_width: f32,

    /// Pool of reusable text entities (PERFORMANCE)
    pub entity_pool: Vec<Entity>,

    /// Pool of reusable line number entities (PERFORMANCE)
    pub line_number_pool: Vec<Entity>,

    /// Track which lines changed for incremental highlighting (PERFORMANCE)
    pub dirty_lines: Option<Range<usize>>,

    /// Track line count for detecting changes
    pub previous_line_count: usize,

    /// Debouncing: true if update is pending but not yet applied (PERFORMANCE)
    pub pending_update: bool,

    /// Last time we rendered (in seconds) for debouncing (PERFORMANCE)
    pub last_render_time: f64,
}

impl Default for CodeEditorState {
    fn default() -> Self {
        let initial_text = "";
        let rope = Rope::from_str(initial_text);
        let line_count = rope.len_lines();

        Self {
            rope,
            cursor_pos: 0,
            last_cursor_pos: 0,
            selection_start: None,
            selection_end: None,
            is_focused: false,
            needs_update: true,
            needs_scroll_update: false,
            tokens: Vec::new(),
            lines: Vec::new(),
            #[cfg(feature = "tree-sitter")]
            highlight_config: None,
            #[cfg(feature = "lsp")]
            document_uri: None,
            #[cfg(feature = "lsp")]
            document_version: 1,
            scroll_offset: 0.0,
            horizontal_scroll_offset: 0.0,
            max_content_width: 0.0,
            entity_pool: Vec::new(),
            line_number_pool: Vec::new(),
            dirty_lines: None,
            previous_line_count: line_count,
            pending_update: false,
            last_render_time: 0.0,
        }
    }
}

impl CodeEditorState {
    /// Create new editor state with initial text
    pub fn new(text: &str) -> Self {
        let rope = Rope::from_str(text);
        let line_count = rope.len_lines();

        Self {
            rope,
            cursor_pos: 0,
            last_cursor_pos: 0,
            selection_start: None,
            selection_end: None,
            is_focused: false,
            needs_update: true,
            needs_scroll_update: false,
            tokens: Vec::new(),
            lines: Vec::new(),
            #[cfg(feature = "tree-sitter")]
            highlight_config: None,
            #[cfg(feature = "lsp")]
            document_uri: None,
            #[cfg(feature = "lsp")]
            document_version: 1,
            scroll_offset: 0.0,
            horizontal_scroll_offset: 0.0,
            max_content_width: 0.0,
            entity_pool: Vec::new(),
            line_number_pool: Vec::new(),
            dirty_lines: None,
            previous_line_count: line_count,
            pending_update: false,
            last_render_time: 0.0,
        }
    }

    /// Update syntax highlighting tokens
    #[cfg(feature = "tree-sitter")]
    pub fn update_highlighting(&mut self) {
        use tree_sitter_highlight::{Highlighter as TSHighlighter, HighlightEvent};

        // These are the actual capture names from tree-sitter grammars (e.g., tree-sitter-rust)
        // They use dotted notation like "comment.documentation", "function.method", etc.
        const HIGHLIGHT_NAMES: &[&str] = &[
            "attribute",
            "comment",
            "comment.documentation",
            "constant",
            "constant.builtin",
            "constructor",
            "escape",
            "function",
            "function.macro",
            "function.method",
            "keyword",
            "label",
            "number",
            "operator",
            "property",
            "punctuation.bracket",
            "punctuation.delimiter",
            "string",
            "type",
            "type.builtin",
            "variable",
            "variable.builtin",
            "variable.parameter",
        ];

        let text = self.rope.to_string();

        if let Some(config) = &self.highlight_config {
            let mut tokens = Vec::with_capacity(text.len() / 10);
            let mut ts_highlighter = TSHighlighter::new();

            let highlights = match ts_highlighter.highlight(config, text.as_bytes(), None, |_| None) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("Tree-sitter highlight error: {:?}", e);
                    self.tokens = vec![HighlightedToken {
                        text: text.clone(),
                        highlight_type: None,
                    }];
                    self.dirty_lines = None;
                    return;
                }
            };

            let mut current_highlight: Option<String> = None;

            for event in highlights {
                match event {
                    Ok(HighlightEvent::Source { start, end }) => {
                        let text_fragment = &text[start..end];
                        tokens.push(HighlightedToken {
                            text: text_fragment.to_string(),
                            highlight_type: current_highlight.clone(),
                        });
                    }
                    Ok(HighlightEvent::HighlightStart(highlight)) => {
                        if highlight.0 < HIGHLIGHT_NAMES.len() {
                            let hl_name = HIGHLIGHT_NAMES[highlight.0].to_string();
                            current_highlight = Some(hl_name);
                        } else {
                            eprintln!("WARNING: Highlight index {} out of range (max {})", highlight.0, HIGHLIGHT_NAMES.len());
                        }
                    }
                    Ok(HighlightEvent::HighlightEnd) => {
                        current_highlight = None;
                    }
                    Err(e) => {
                        eprintln!("Highlight event error: {:?}", e);
                    }
                }
            }

            if tokens.is_empty() {
                tokens.push(HighlightedToken {
                    text: text.clone(),
                    highlight_type: None,
                });
            }

            self.tokens = tokens;
        } else {
            // Fallback: no highlighting
            self.tokens = vec![HighlightedToken {
                text,
                highlight_type: None,
            }];
        }

        self.dirty_lines = None;
    }

    /// Update highlighting without syntax highlighting feature
    #[cfg(not(feature = "tree-sitter"))]
    pub fn update_highlighting(&mut self) {
        let text = self.rope.to_string();
        self.tokens = vec![HighlightedToken {
            text,
            highlight_type: None,
        }];
        self.dirty_lines = None;
    }

    /// Get text content as string
    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// Get line count
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// Insert character at cursor position
    pub fn insert_char(&mut self, c: char) {
        let cursor_pos = self.cursor_pos.min(self.rope.len_chars());
        let line_idx = self.rope.char_to_line(cursor_pos);

        self.rope.insert_char(cursor_pos, c);
        self.cursor_pos += 1;
        self.pending_update = true;

        let new_line_count = self.rope.len_lines();
        if c == '\n' {
            self.dirty_lines = Some(line_idx..new_line_count);
        } else {
            self.dirty_lines = Some(line_idx..(line_idx + 1).min(new_line_count));
        }
        self.previous_line_count = new_line_count;
    }

    /// Delete character before cursor
    pub fn delete_backward(&mut self) {
        if self.cursor_pos > 0 && self.cursor_pos <= self.rope.len_chars() {
            let line_idx = self.rope.char_to_line(self.cursor_pos - 1);
            let char_idx = self.rope.char_to_byte(self.cursor_pos - 1);
            let byte_idx_end = self.rope.char_to_byte(self.cursor_pos);
            self.rope.remove(char_idx..byte_idx_end);
            self.cursor_pos -= 1;
            self.pending_update = true;

            let new_line_count = self.rope.len_lines();
            self.dirty_lines = Some(line_idx..new_line_count);
            self.previous_line_count = new_line_count;
        }
    }

    /// Delete character after cursor
    pub fn delete_forward(&mut self) {
        if self.cursor_pos < self.rope.len_chars() {
            let line_idx = self.rope.char_to_line(self.cursor_pos);
            let char_idx = self.rope.char_to_byte(self.cursor_pos);
            let byte_idx_end = self.rope.char_to_byte(self.cursor_pos + 1);
            self.rope.remove(char_idx..byte_idx_end);
            self.pending_update = true;

            let new_line_count = self.rope.len_lines();
            self.dirty_lines = Some(line_idx..new_line_count);
            self.previous_line_count = new_line_count;
        }
    }

    /// Move cursor by delta
    pub fn move_cursor(&mut self, delta: isize) {
        if delta < 0 {
            let amount = (-delta) as usize;
            self.cursor_pos = self.cursor_pos.saturating_sub(amount);
        } else {
            let amount = delta as usize;
            self.cursor_pos = (self.cursor_pos + amount).min(self.rope.len_chars());
        }
    }

    /// Set text content
    pub fn set_text(&mut self, text: &str) {
        self.rope = Rope::from_str(text);
        self.cursor_pos = self.cursor_pos.min(self.rope.len_chars());
        self.pending_update = true;
        self.dirty_lines = None;
        self.previous_line_count = self.rope.len_lines();
    }
}

/// Component markers for editor entities

#[derive(Component)]
pub struct EditorText;

#[derive(Component)]
pub struct HighlightedTextToken {
    pub index: usize,
}

#[derive(Component)]
pub struct EditorCursor;

#[derive(Component)]
pub struct LineNumbers;

#[derive(Component)]
pub struct Separator;

#[derive(Component)]
pub struct SelectionHighlight {
    pub line_index: usize,
}
