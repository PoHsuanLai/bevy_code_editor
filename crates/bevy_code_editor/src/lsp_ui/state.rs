//! Per-editor LSP UI state Components.
//!
//! These were Resources living in `bevy_lsp::state` until the protocol/UI split;
//! they're popups, debounce timers, and filter state that belong to *the
//! editor's UI layer*, not the LSP protocol. Each is a Component on the editor
//! entity (the same entity that carries [`bevy_lsp::LspClient`] and
//! [`bevy_lsp::LspDocument`]).
//!
//! Consumers query them as `Query<&mut LspCompletionPopup, With<CodeEditor>>`
//! etc. The `#[require]` cascade on [`crate::types::CodeEditor`] inserts them
//! all with `Default`, so a freshly spawned `CodeEditor` is fully usable.

use crate::settings::WordsCompletionMode;
use bevy::prelude::*;
use bevy_text_editor::Anchor;
use lsp_types::*;

/// One tabstop in an active snippet session, anchored so it survives
/// edits that happen while the session is live (e.g. typing into the
/// placeholder selection replaces the placeholder, and subsequent
/// tabstops shift accordingly).
#[derive(Clone, Debug)]
pub struct SessionTabstop {
    pub id: u32,
    pub start: Anchor,
    pub end: Anchor,
}

/// Active snippet tabstop session. Spawned on the editor entity by
/// `listen_apply_completion` when the inserted item carries snippet
/// syntax. Ended (despawned via `Option<&mut>` clear) when the cursor
/// leaves the session, the user presses Esc, or all stops are visited.
#[derive(Component, Default, Debug)]
pub struct TabstopSession {
    /// Stops in walk order: rising `id` values, then `0` (final stop)
    /// last. Empty when no session is active.
    pub stops: Vec<SessionTabstop>,
    /// Index into `stops` of the currently-selected tabstop. The
    /// session ends when this exceeds `stops.len() - 1`.
    pub current: usize,
}

impl TabstopSession {
    pub fn is_active(&self) -> bool {
        !self.stops.is_empty() && self.current < self.stops.len()
    }

    pub fn end(&mut self) {
        self.stops.clear();
        self.current = 0;
    }
}

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
            UnifiedCompletionItem::Lsp(item) => item.insert_text.as_deref().unwrap_or(&item.label),
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

/// Per-editor completion popup state.
///
/// Was `bevy_lsp::CompletionState` (Resource).
#[derive(Component, Default)]
pub struct LspCompletionPopup {
    pub visible: bool,
    pub items: Vec<CompletionItem>,
    pub word_items: Vec<WordCompletionItem>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub start_char_index: usize,
    pub filter: String,
    pub is_incomplete: bool,
    /// Monotonic id of the most recently dispatched LSP completion request.
    /// Bumped on every send; the response handler drops anything older.
    /// Bumped on hide too, so any in-flight request becomes stale.
    pub request_id: u64,
    /// Initial filter at the time the menu was opened. When the user keeps
    /// typing identifier chars (extending this prefix) and the previous
    /// response was complete, we refilter locally instead of re-querying.
    pub initial_query: String,
    /// Mirror of `LspSettings::completion::words_mode`, kept on the
    /// component so `filtered_items` doesn't need access to the resource.
    /// Synced once per frame in `sync_completion_settings`.
    pub words_mode: WordsCompletionMode,
    /// Cache of `completionItem/resolve` results, keyed by the **label**
    /// of the original item. Label-keying survives reordering when the
    /// filter changes; index-keying would not.
    pub resolved: std::collections::HashMap<String, CompletionItem>,
    /// Bumped on each resolve request and on every dismiss / item-list
    /// change so stale resolve responses are dropped before they reach
    /// the popup data.
    pub resolve_request_id: u64,
    /// `(label, request_id)` of the in-flight resolve. None when no
    /// resolve is in flight.
    pub pending_resolve: Option<(String, u64)>,
}

impl LspCompletionPopup {
    /// Hide the popup and bump `request_id` so any in-flight LSP response
    /// for this menu is dropped instead of resurrecting it.
    pub fn dismiss(&mut self) {
        self.visible = false;
        self.filter.clear();
        self.initial_query.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.is_incomplete = false;
        self.request_id = self.request_id.wrapping_add(1);
        self.resolve_request_id = self.resolve_request_id.wrapping_add(1);
        self.pending_resolve = None;
        self.resolved.clear();
    }

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
        use fuzzy_matcher::skim::SkimMatcherV2;
        use fuzzy_matcher::FuzzyMatcher;
        use std::collections::HashSet;

        let matcher = SkimMatcherV2::default();

        // First, filter and score LSP items
        let mut lsp_scored: Vec<(UnifiedCompletionItem, i64)> = if self.filter.is_empty() {
            self.items
                .iter()
                .map(|item| (UnifiedCompletionItem::Lsp(item.clone()), 0))
                .collect()
        } else {
            self.items
                .iter()
                .filter_map(|item| {
                    let score = matcher.fuzzy_match(&item.label, &self.filter).or_else(|| {
                        item.filter_text
                            .as_ref()
                            .and_then(|f| matcher.fuzzy_match(f, &self.filter))
                    });
                    score.map(|s| (UnifiedCompletionItem::Lsp(item.clone()), s))
                })
                .collect()
        };

        // Sort LSP items by score (higher is better)
        lsp_scored.sort_by(|a, b| b.1.cmp(&a.1));

        // Decide whether to merge buffer-word completions based on the mode.
        let include_words = match self.words_mode {
            WordsCompletionMode::Disabled => false,
            WordsCompletionMode::Always => true,
            WordsCompletionMode::Fallback => self.items.is_empty() || self.is_incomplete,
        };

        // Collect LSP labels to avoid duplicates with word completions
        let lsp_labels: HashSet<&str> = self.items.iter().map(|i| i.label.as_str()).collect();

        // Filter and score word completions (only if filter is not empty)
        let mut word_scored: Vec<(UnifiedCompletionItem, i64)> =
            if !include_words || self.filter.is_empty() {
                Vec::new()
            } else {
                self.word_items
                    .iter()
                    .filter(|item| !lsp_labels.contains(item.word.as_str()))
                    .filter_map(|item| {
                        matcher
                            .fuzzy_match(&item.word, &self.filter)
                            .map(|s| (UnifiedCompletionItem::Word(item.clone()), s))
                    })
                    .collect()
            };

        // Sort word items by score
        word_scored.sort_by(|a, b| b.1.cmp(&a.1));

        // Combine: LSP items first, then word completions
        let mut result: Vec<UnifiedCompletionItem> =
            lsp_scored.into_iter().map(|(item, _)| item).collect();
        result.extend(word_scored.into_iter().map(|(item, _)| item));

        result
    }

    /// Update word completions from the rope. Skips the scan entirely when
    /// `words_mode == Disabled` so we don't pay the per-keystroke cost.
    pub fn update_word_completions(&mut self, rope: &ropey::Rope, cursor_pos: usize) {
        use std::collections::HashSet;

        if self.words_mode == WordsCompletionMode::Disabled {
            self.word_items.clear();
            return;
        }

        let mut seen: HashSet<String> = HashSet::new();
        let mut words: Vec<WordCompletionItem> = Vec::new();

        // Get the word at cursor position (to exclude it)
        let cursor_word = get_word_at_position(rope, cursor_pos);

        // Iterate through the entire document and extract words
        // OPTIMIZATION: Use rope chunks instead of full to_string() conversion
        let chunk_text: String = rope.chunks().collect();
        let mut word_start: Option<usize> = None;

        for (i, c) in chunk_text.char_indices() {
            let is_word_char = c.is_alphanumeric() || c == '_';

            if is_word_char {
                if word_start.is_none() {
                    word_start = Some(i);
                }
            } else if let Some(start) = word_start {
                let word = &chunk_text[start..i];
                if word.len() >= 2
                    && cursor_word.as_ref().is_none_or(|cw| cw != word)
                    && !seen.contains(word)
                {
                    seen.insert(word.to_string());
                    words.push(WordCompletionItem {
                        word: word.to_string(),
                    });
                }
                word_start = None;
            }
        }

        // Handle word at end of text
        if let Some(start) = word_start {
            let word = &chunk_text[start..];
            if word.len() >= 2
                && cursor_word.as_ref().is_none_or(|cw| cw != word)
                && !seen.contains(word)
            {
                words.push(WordCompletionItem {
                    word: word.to_string(),
                });
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
        self.resolved.clear();
        self.pending_resolve = None;
    }
}

/// Get the word at a given character position
fn get_word_at_position(rope: &ropey::Rope, char_pos: usize) -> Option<String> {
    if char_pos == 0 || char_pos > rope.len_chars() {
        return None;
    }

    // OPTIMIZATION: Work with rope directly instead of converting to string
    let line_idx = rope.char_to_line(char_pos);
    let line = rope.line(line_idx);
    let line_start_char = rope.line_to_char(line_idx);
    let pos_in_line = char_pos - line_start_char;

    let line_text: String = line.chars().collect();

    // Find word boundaries within the line
    let byte_pos_in_line = line_text
        .char_indices()
        .nth(pos_in_line)
        .map(|(i, _)| i)
        .unwrap_or(line_text.len());

    let start = line_text[..byte_pos_in_line]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);

    let end = line_text[byte_pos_in_line..]
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| byte_pos_in_line + i)
        .unwrap_or(line_text.len());

    if start < end {
        Some(line_text[start..end].to_string())
    } else {
        None
    }
}

/// Per-editor hover popup state.
///
/// Was `bevy_lsp::HoverState` (Resource).
#[derive(Component, Default)]
pub struct LspHoverPopup {
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

impl LspHoverPopup {
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

/// Per-editor signature help popup state.
///
/// Was `bevy_lsp::SignatureHelpState` (Resource).
#[derive(Component, Default)]
pub struct LspSignatureHelpPopup {
    pub visible: bool,
    pub signatures: Vec<SignatureInformation>,
    pub active_signature: usize,
    pub active_parameter: usize,
    pub trigger_position: usize,
    /// Bumped on every request and on dismiss; response handler drops
    /// anything older than the current value.
    pub request_id: u64,
}

impl LspSignatureHelpPopup {
    pub fn current_signature(&self) -> Option<&SignatureInformation> {
        self.signatures.get(self.active_signature)
    }

    pub fn dismiss(&mut self) {
        self.visible = false;
        self.signatures.clear();
        self.active_signature = 0;
        self.active_parameter = 0;
        self.request_id = self.request_id.wrapping_add(1);
    }

    /// Backward-compat alias.
    pub fn reset(&mut self) {
        self.dismiss();
    }
}

/// Per-editor code actions popup state.
///
/// Was `bevy_lsp::CodeActionState` (Resource).
#[derive(Component, Default)]
pub struct LspCodeActionsPopup {
    pub visible: bool,
    pub actions: Vec<bevy_lsp::CodeActionOrCommand>,
    pub selected_index: usize,
    pub range: Option<Range>,
    /// Bumped on every request and on dismiss; response handler drops
    /// anything older than the current value.
    pub request_id: u64,
}

impl LspCodeActionsPopup {
    pub fn dismiss(&mut self) {
        self.visible = false;
        self.actions.clear();
        self.selected_index = 0;
        self.request_id = self.request_id.wrapping_add(1);
    }
}

impl LspCodeActionsPopup {
    /// Reset state
    pub fn reset(&mut self) {
        self.visible = false;
        self.actions.clear();
        self.selected_index = 0;
        self.range = None;
    }
}

/// Per-editor inlay hints state.
///
/// Was `bevy_lsp::InlayHintState` (Resource).
#[derive(Component, Default)]
pub struct LspInlayHints {
    /// Cached inlay hints for current view
    pub hints: Vec<InlayHint>,
    /// The range for which hints are cached
    pub cached_range: Option<Range>,
    /// Whether hints need to be refreshed
    pub needs_refresh: bool,
}

impl LspInlayHints {
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

/// A pending LSP request (position-based). `position` is already in the
/// negotiated wire encoding — convert with [`bevy_lsp::rope_char_to_lsp_position`]
/// at enqueue time.
#[derive(Clone, Debug)]
pub struct PendingLspRequest {
    pub uri: Url,
    pub position: Position,
}

/// A pending code action request (range-based)
#[derive(Clone, Debug)]
pub struct PendingCodeActionRequest {
    pub uri: Url,
    pub range: Range,
}

/// Per-feature LSP debounce timers (Zed-style tiered debouncing).
///
/// Was `bevy_lsp::LspDebounceTimers` (Resource). Per-editor Component.
#[derive(Component)]
pub struct LspDebounceTimers {
    /// Completion: 50ms after last keystroke
    pub completion_timer: Timer,
    pub pending_completion: Option<PendingLspRequest>,

    /// Hover: 150ms after cursor stops
    pub hover_timer: Timer,
    pub pending_hover: Option<PendingLspRequest>,

    /// Code actions: 250ms after cursor stops
    pub code_action_timer: Timer,
    pub pending_code_action: Option<PendingCodeActionRequest>,

    /// Document highlights: 100ms after cursor stops
    pub highlight_timer: Timer,
    pub pending_highlight: Option<PendingLspRequest>,
}

impl Default for LspDebounceTimers {
    fn default() -> Self {
        Self {
            completion_timer: Timer::from_seconds(0.05, TimerMode::Once),
            pending_completion: None,
            hover_timer: Timer::from_seconds(0.15, TimerMode::Once),
            pending_hover: None,
            code_action_timer: Timer::from_seconds(0.25, TimerMode::Once),
            pending_code_action: None,
            highlight_timer: Timer::from_seconds(0.1, TimerMode::Once),
            pending_highlight: None,
        }
    }
}

/// Per-editor "extra" LSP sync state — the bits of `LspSyncState` that didn't
/// move into [`bevy_lsp::LspDocument`].
///
/// `LspDocument` already owns `uri` and `version`; this Component owns the
/// debounced did_change driver state (dirty flag + timer).
#[derive(Component)]
pub struct LspSyncStateExtra {
    /// Whether the document has changed since last sync
    pub dirty: bool,
    /// Timer to debounce sync requests
    pub timer: Timer,
}

impl Default for LspSyncStateExtra {
    fn default() -> Self {
        Self {
            dirty: false,
            timer: Timer::from_seconds(0.2, TimerMode::Once),
        }
    }
}

/// Per-editor document highlight state (all occurrences of symbol under cursor).
///
/// Was `bevy_lsp::DocumentHighlightState` (Resource).
#[derive(Component, Default)]
pub struct LspDocumentHighlights {
    /// Current highlights
    pub highlights: Vec<DocumentHighlight>,
    /// The cursor position for which highlights were requested
    pub cursor_position: usize,
    /// Whether highlights are currently visible
    pub visible: bool,
    /// Timer for debouncing highlight requests
    pub debounce_timer: Option<Timer>,
    pub in_flight_position: Option<usize>,
}

impl LspDocumentHighlights {
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

/// Per-editor rename popup state.
///
/// Was `bevy_lsp::RenameState` (Resource).
#[derive(Component, Default)]
pub struct LspRenamePopup {
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

impl LspRenamePopup {
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
