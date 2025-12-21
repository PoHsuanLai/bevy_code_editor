//! LSP-related state resources for Bevy

use bevy::prelude::*;
use lsp_types::*;

/// Default maximum number of visible items in completion popup
pub const COMPLETION_MAX_VISIBLE_DEFAULT: usize = 10;

/// A word completion item (extracted from document)
#[derive(Clone, Debug)]
pub struct WordCompletionItem {
    /// The word text
    pub word: String,
}

/// Unified completion item for display (can be LSP or word-based)
#[derive(Clone, Debug)]
pub enum UnifiedCompletionItem {
    /// LSP completion item
    Lsp(CompletionItem),
    /// Word from document
    Word(WordCompletionItem),
}

impl UnifiedCompletionItem {
    /// Get the display label
    pub fn label(&self) -> &str {
        match self {
            UnifiedCompletionItem::Lsp(item) => &item.label,
            UnifiedCompletionItem::Word(item) => &item.word,
        }
    }

    /// Get the detail text (if any)
    pub fn detail(&self) -> Option<&str> {
        match self {
            UnifiedCompletionItem::Lsp(item) => item.detail.as_deref(),
            UnifiedCompletionItem::Word(_) => Some("word"),
        }
    }

    /// Get the text to insert
    pub fn insert_text(&self) -> &str {
        match self {
            UnifiedCompletionItem::Lsp(item) => {
                item.insert_text.as_deref().unwrap_or(&item.label)
            }
            UnifiedCompletionItem::Word(item) => &item.word,
        }
    }

    /// Check if this is a word completion
    pub fn is_word(&self) -> bool {
        matches!(self, UnifiedCompletionItem::Word(_))
    }

    /// Get the completion kind icon
    pub fn kind_icon(&self) -> &'static str {
        match self {
            UnifiedCompletionItem::Lsp(item) => match item.kind {
                Some(CompletionItemKind::FUNCTION) | Some(CompletionItemKind::METHOD) => "ƒ",
                Some(CompletionItemKind::VARIABLE) => "𝑥",
                Some(CompletionItemKind::CLASS) | Some(CompletionItemKind::STRUCT) => "○",
                Some(CompletionItemKind::INTERFACE) => "◇",
                Some(CompletionItemKind::MODULE) => "□",
                Some(CompletionItemKind::PROPERTY) | Some(CompletionItemKind::FIELD) => "▪",
                Some(CompletionItemKind::CONSTANT) => "𝐶",
                Some(CompletionItemKind::ENUM) => "∈",
                Some(CompletionItemKind::ENUM_MEMBER) => "∋",
                Some(CompletionItemKind::KEYWORD) => "⌘",
                Some(CompletionItemKind::SNIPPET) => "✂",
                Some(CompletionItemKind::TYPE_PARAMETER) => "𝑇",
                _ => "•",
            },
            UnifiedCompletionItem::Word(_) => "𝑤",
        }
    }
}

/// State for the auto-completion UI
#[derive(Resource, Default)]
pub struct CompletionState {
    /// Whether the completion box is currently visible
    pub visible: bool,
    /// Current list of completion items (unfiltered from LSP)
    pub items: Vec<CompletionItem>,
    /// Word completions extracted from the document (fallback when LSP is empty)
    pub word_items: Vec<WordCompletionItem>,
    /// Index of the currently selected item (in filtered list)
    pub selected_index: usize,
    /// Scroll offset (first visible item index)
    pub scroll_offset: usize,
    /// Character index in the document where completion started (trigger position)
    pub start_char_index: usize,
    /// Filter text (what the user has typed since opening completion)
    pub filter: String,
    /// Whether the completion list is incomplete (should re-query on more typing)
    pub is_incomplete: bool,
}

impl CompletionState {
    /// Ensure the selected item is visible by adjusting scroll_offset
    pub fn ensure_selected_visible(&mut self) {
        self.ensure_selected_visible_with_max(COMPLETION_MAX_VISIBLE_DEFAULT);
    }

    /// Ensure the selected item is visible with a specific max visible count
    pub fn ensure_selected_visible_with_max(&mut self, max_visible: usize) {
        let filtered_count = self.filtered_items().len();
        if filtered_count == 0 {
            self.scroll_offset = 0;
            return;
        }

        // Clamp selected_index to valid range
        self.selected_index = self.selected_index.min(filtered_count.saturating_sub(1));

        // If selected is above visible area, scroll up
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        }
        // If selected is below visible area, scroll down
        else if self.selected_index >= self.scroll_offset + max_visible {
            self.scroll_offset = self.selected_index - max_visible + 1;
        }

        // Clamp scroll_offset to valid range
        let max_scroll = filtered_count.saturating_sub(max_visible);
        self.scroll_offset = self.scroll_offset.min(max_scroll);
    }

    /// Get filtered items based on current filter text using fuzzy matching
    pub fn filtered_items(&self) -> Vec<UnifiedCompletionItem> {
        use fuzzy_matcher::FuzzyMatcher;
        use fuzzy_matcher::skim::SkimMatcherV2;
        use std::collections::HashSet;

        let matcher = SkimMatcherV2::default();

        // First, filter and score LSP items
        let mut lsp_scored: Vec<(UnifiedCompletionItem, i64)> = if self.filter.is_empty() {
            self.items.iter()
                .map(|item| (UnifiedCompletionItem::Lsp(item.clone()), 0))
                .collect()
        } else {
            self.items
                .iter()
                .filter_map(|item| {
                    let score = matcher.fuzzy_match(&item.label, &self.filter)
                        .or_else(|| {
                            item.filter_text.as_ref()
                                .and_then(|f| matcher.fuzzy_match(f, &self.filter))
                        });
                    score.map(|s| (UnifiedCompletionItem::Lsp(item.clone()), s))
                })
                .collect()
        };

        // Sort LSP items by score (higher is better)
        lsp_scored.sort_by(|a, b| b.1.cmp(&a.1));

        // Collect LSP labels to avoid duplicates with word completions
        let lsp_labels: HashSet<&str> = self.items.iter().map(|i| i.label.as_str()).collect();

        // Filter and score word completions (only if filter is not empty)
        let mut word_scored: Vec<(UnifiedCompletionItem, i64)> = if self.filter.is_empty() {
            Vec::new()
        } else {
            self.word_items
                .iter()
                .filter(|item| !lsp_labels.contains(item.word.as_str()))
                .filter_map(|item| {
                    matcher.fuzzy_match(&item.word, &self.filter)
                        .map(|s| (UnifiedCompletionItem::Word(item.clone()), s))
                })
                .collect()
        };

        // Sort word items by score
        word_scored.sort_by(|a, b| b.1.cmp(&a.1));

        // Combine: LSP items first, then word completions
        let mut result: Vec<UnifiedCompletionItem> = lsp_scored.into_iter().map(|(item, _)| item).collect();
        result.extend(word_scored.into_iter().map(|(item, _)| item));

        result
    }

    /// Update word completions from the rope
    pub fn update_word_completions(&mut self, rope: &ropey::Rope, cursor_pos: usize) {
        use std::collections::HashSet;

        let mut seen: HashSet<String> = HashSet::new();
        let mut words: Vec<WordCompletionItem> = Vec::new();

        // Get the word at cursor position (to exclude it)
        let cursor_word = get_word_at_position(rope, cursor_pos);

        // Iterate through the entire document and extract words
        let text = rope.to_string();
        let mut word_start: Option<usize> = None;

        for (i, c) in text.char_indices() {
            let is_word_char = c.is_alphanumeric() || c == '_';

            if is_word_char {
                if word_start.is_none() {
                    word_start = Some(i);
                }
            } else if let Some(start) = word_start {
                let word = &text[start..i];
                if word.len() >= 2
                    && cursor_word.as_ref().map_or(true, |cw| cw != word)
                    && !seen.contains(word)
                {
                    seen.insert(word.to_string());
                    words.push(WordCompletionItem { word: word.to_string() });
                }
                word_start = None;
            }
        }

        // Handle word at end of text
        if let Some(start) = word_start {
            let word = &text[start..];
            if word.len() >= 2
                && cursor_word.as_ref().map_or(true, |cw| cw != word)
                && !seen.contains(word)
            {
                words.push(WordCompletionItem { word: word.to_string() });
            }
        }

        self.word_items = words;
    }

    /// Reset completion state
    pub fn reset(&mut self) {
        self.visible = false;
        self.items.clear();
        self.word_items.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.filter.clear();
        self.is_incomplete = false;
    }
}

/// Get the word at a given character position
fn get_word_at_position(rope: &ropey::Rope, char_pos: usize) -> Option<String> {
    if char_pos == 0 || char_pos > rope.len_chars() {
        return None;
    }

    let text = rope.to_string();
    let byte_pos = rope.char_to_byte(char_pos.min(rope.len_chars()));

    let start = text[..byte_pos]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);

    let end = text[byte_pos..]
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| byte_pos + i)
        .unwrap_or(text.len());

    if start < end {
        Some(text[start..end].to_string())
    } else {
        None
    }
}

/// State for hover popups
#[derive(Resource, Default)]
pub struct HoverState {
    /// Whether the hover box is currently visible
    pub visible: bool,
    /// Content to display in the hover box (markdown)
    pub content: String,
    /// The character index in the document where the mouse currently is
    pub trigger_char_index: usize,
    /// The character index for which we sent the hover request (to match response)
    pub pending_char_index: Option<usize>,
    /// Timer for delaying hover display/hide
    pub timer: Option<Timer>,
    /// The actual LSP range for the hover content (useful for highlighting)
    pub range: Option<Range>,
    /// Whether we've already sent a hover request for this position
    pub request_sent: bool,
}

impl HoverState {
    /// Reset hover state
    pub fn reset(&mut self) {
        self.visible = false;
        self.content.clear();
        self.timer = None;
        self.range = None;
        self.request_sent = false;
        self.pending_char_index = None;
    }
}

/// State for signature help
#[derive(Resource, Default)]
pub struct SignatureHelpState {
    /// Whether the signature help is currently visible
    pub visible: bool,
    /// Available signatures
    pub signatures: Vec<SignatureInformation>,
    /// Currently active signature index
    pub active_signature: usize,
    /// Currently active parameter index
    pub active_parameter: usize,
    /// Character position where signature help was triggered
    pub trigger_position: usize,
}

impl SignatureHelpState {
    /// Get the currently active signature
    pub fn current_signature(&self) -> Option<&SignatureInformation> {
        self.signatures.get(self.active_signature)
    }

    /// Reset state
    pub fn reset(&mut self) {
        self.visible = false;
        self.signatures.clear();
        self.active_signature = 0;
        self.active_parameter = 0;
    }
}

/// State for code actions
#[derive(Resource, Default)]
pub struct CodeActionState {
    /// Whether the code action menu is visible
    pub visible: bool,
    /// Available code actions
    pub actions: Vec<super::messages::CodeActionOrCommand>,
    /// Selected action index
    pub selected_index: usize,
    /// The range for which actions were requested
    pub range: Option<Range>,
}

impl CodeActionState {
    /// Reset state
    pub fn reset(&mut self) {
        self.visible = false;
        self.actions.clear();
        self.selected_index = 0;
        self.range = None;
    }
}

/// State for inlay hints
#[derive(Resource, Default)]
pub struct InlayHintState {
    /// Cached inlay hints for current view
    pub hints: Vec<InlayHint>,
    /// The range for which hints are cached
    pub cached_range: Option<Range>,
    /// Whether hints need to be refreshed
    pub needs_refresh: bool,
}

impl InlayHintState {
    /// Check if a range is covered by the cache
    pub fn is_range_cached(&self, range: &Range) -> bool {
        if let Some(cached) = &self.cached_range {
            cached.start.line <= range.start.line && cached.end.line >= range.end.line
        } else {
            false
        }
    }

    /// Invalidate the cache
    pub fn invalidate(&mut self) {
        self.hints.clear();
        self.cached_range = None;
        self.needs_refresh = true;
    }
}

/// State for LSP document synchronization
#[derive(Resource)]
pub struct LspSyncState {
    /// Whether the document has changed since last sync
    pub dirty: bool,
    /// Timer to debounce sync requests
    pub timer: Timer,
}

impl Default for LspSyncState {
    fn default() -> Self {
        Self {
            dirty: false,
            timer: Timer::from_seconds(0.2, TimerMode::Once),
        }
    }
}

/// State for document highlights (all occurrences of symbol under cursor)
#[derive(Resource, Default)]
pub struct DocumentHighlightState {
    /// Current highlights
    pub highlights: Vec<DocumentHighlight>,
    /// The cursor position for which highlights were requested
    pub cursor_position: usize,
    /// Whether highlights are currently visible
    pub visible: bool,
    /// Timer for debouncing highlight requests
    pub debounce_timer: Option<Timer>,
}

impl DocumentHighlightState {
    /// Reset state
    pub fn reset(&mut self) {
        self.highlights.clear();
        self.visible = false;
        self.debounce_timer = None;
    }

    /// Clear highlights without resetting timer
    pub fn clear_highlights(&mut self) {
        self.highlights.clear();
        self.visible = false;
    }
}

/// State for rename operation
#[derive(Resource, Default)]
pub struct RenameState {
    /// Whether rename dialog is visible
    pub visible: bool,
    /// The range being renamed
    pub range: Option<Range>,
    /// The original text being renamed
    pub original_text: String,
    /// The new name being typed
    pub new_name: String,
    /// Position where rename was initiated
    pub position: Option<Position>,
    /// Whether we're waiting for prepare rename response
    pub preparing: bool,
    /// Error message if rename failed
    pub error: Option<String>,
}

impl RenameState {
    /// Reset state
    pub fn reset(&mut self) {
        self.visible = false;
        self.range = None;
        self.original_text.clear();
        self.new_name.clear();
        self.position = None;
        self.preparing = false;
        self.error = None;
    }

    /// Start preparing rename at position
    pub fn start_prepare(&mut self, position: Position) {
        self.reset();
        self.position = Some(position);
        self.preparing = true;
    }

    /// Handle prepare rename response
    pub fn on_prepare_response(&mut self, range: Range, placeholder: Option<String>) {
        self.preparing = false;
        self.range = Some(range);
        self.original_text = placeholder.clone().unwrap_or_default();
        self.new_name = placeholder.unwrap_or_default();
        self.visible = true;
    }

    /// Check if rename is ready to submit
    pub fn can_submit(&self) -> bool {
        self.visible && !self.new_name.is_empty() && self.new_name != self.original_text
    }
}
