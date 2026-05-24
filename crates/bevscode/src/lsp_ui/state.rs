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
use bevy_instanced_text_editor::Anchor;
use lsp_types::*;
use std::collections::{HashMap, VecDeque};

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
    Lsp(Box<CompletionItem>),
    /// Word from document
    Word(WordCompletionItem),
}

impl UnifiedCompletionItem {
    pub fn label(&self) -> &str {
        match self {
            UnifiedCompletionItem::Lsp(item) => &item.label,
            UnifiedCompletionItem::Word(item) => &item.word,
        }
    }

    pub fn detail(&self) -> Option<&str> {
        match self {
            UnifiedCompletionItem::Lsp(item) => item.detail.as_deref(),
            UnifiedCompletionItem::Word(_) => Some("word"),
        }
    }

    pub fn insert_text(&self) -> &str {
        match self {
            UnifiedCompletionItem::Lsp(item) => item.insert_text.as_deref().unwrap_or(&item.label),
            UnifiedCompletionItem::Word(item) => &item.word,
        }
    }

    pub fn is_word(&self) -> bool {
        matches!(self, UnifiedCompletionItem::Word(_))
    }

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
    /// Initial filter at the time the menu was opened. When the user keeps
    /// typing identifier chars (extending this prefix) and the previous
    /// response was complete, we refilter locally instead of re-querying.
    pub initial_query: String,
    /// Mirror of `LspConfig::completion::words_mode`, kept on the
    /// component so `filtered_items` doesn't need access to the resource.
    /// Synced once per frame in `sync_completion_settings`.
    pub words_mode: WordsCompletionMode,
    /// Cache of `completionItem/resolve` results, keyed by the **label**
    /// of the original item. Label-keying survives reordering when the
    /// filter changes; index-keying would not.
    pub resolved: HashMap<String, CompletionItem>,
    /// Bumped on each resolve request and on every dismiss / item-list
    /// change so stale resolve responses are dropped before they reach
    /// the popup data.
    pub resolve_request_id: u64,
    /// `(label, request_id)` of the in-flight resolve. None when no
    /// resolve is in flight.
    pub pending_resolve: Option<(String, u64)>,
    /// Most-recently-accepted completion labels, newest at the front,
    /// capped at [`RECENT_LABELS_CAP`]. Drives
    /// [`crate::settings::SuggestSelection::RecentlyUsed`] preselection.
    pub recent_labels: VecDeque<String>,
    /// Last-accepted label per `RECENT_PREFIX_LEN`-char prefix of the
    /// typed word at acceptance time. Drives
    /// [`crate::settings::SuggestSelection::RecentlyUsedByPrefix`].
    pub recent_by_prefix: HashMap<String, String>,
}

/// Maximum number of remembered recently-accepted labels.
pub const RECENT_LABELS_CAP: usize = 32;

/// Prefix length used to key `LspCompletionPopup::recent_by_prefix`.
pub const RECENT_PREFIX_LEN: usize = 3;

impl LspCompletionPopup {
    /// Clear popup-local state (filter, selection, scroll, resolved
    /// cache). The lifecycle id-bump that invalidates in-flight LSP
    /// responses lives on [`CompletionLifecycle`] — call its
    /// `.dismiss()` at the same site.
    pub fn dismiss(&mut self) {
        self.visible = false;
        self.filter.clear();
        self.initial_query.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.is_incomplete = false;
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
                .map(|item| (UnifiedCompletionItem::Lsp(Box::new(item.clone())), 0))
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
                    score.map(|s| (UnifiedCompletionItem::Lsp(Box::new(item.clone())), s))
                })
                .collect()
        };

        // Sort LSP items by score (higher is better)
        lsp_scored.sort_by_key(|b| std::cmp::Reverse(b.1));

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
        word_scored.sort_by_key(|b| std::cmp::Reverse(b.1));

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

    /// Record the acceptance of `label` against the typed prefix `query`
    /// so [`crate::settings::SuggestSelection::RecentlyUsed`] and
    /// [`crate::settings::SuggestSelection::RecentlyUsedByPrefix`] can
    /// preselect it next time.
    pub fn remember_acceptance(&mut self, label: &str, query: &str) {
        self.recent_labels.retain(|l| l != label);
        self.recent_labels.push_front(label.to_string());
        while self.recent_labels.len() > RECENT_LABELS_CAP {
            self.recent_labels.pop_back();
        }
        let prefix_key: String = query.chars().take(RECENT_PREFIX_LEN).collect();
        if !prefix_key.is_empty() {
            self.recent_by_prefix.insert(prefix_key, label.to_string());
        }
    }

    /// Return the index in `filtered` of the item to preselect under
    /// `mode`. `None` ⇒ caller should keep `0` (best fuzzy match).
    pub fn preselect_index(
        &self,
        filtered: &[UnifiedCompletionItem],
        mode: crate::settings::SuggestSelection,
    ) -> Option<usize> {
        use crate::settings::SuggestSelection;
        match mode {
            SuggestSelection::First => None,
            SuggestSelection::RecentlyUsed => self.recent_match(filtered),
            SuggestSelection::RecentlyUsedByPrefix => {
                let prefix_key: String = self.filter.chars().take(RECENT_PREFIX_LEN).collect();
                if !prefix_key.is_empty() {
                    if let Some(label) = self.recent_by_prefix.get(&prefix_key) {
                        if let Some(idx) = filtered.iter().position(|it| it.label() == label) {
                            return Some(idx);
                        }
                    }
                }
                self.recent_match(filtered)
            }
        }
    }

    /// Newest-first scan of `recent_labels` for the first item that appears
    /// in `filtered`.
    fn recent_match(&self, filtered: &[UnifiedCompletionItem]) -> Option<usize> {
        for label in &self.recent_labels {
            if let Some(idx) = filtered.iter().position(|it| it.label() == label) {
                return Some(idx);
            }
        }
        None
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
#[derive(Component)]
pub struct LspHoverPopup {
    /// Whether the hover box is currently visible
    pub visible: bool,
    /// Content to display in the hover box. Format is described by `kind`.
    pub content: String,
    /// Source format of `content`. UI consumers route the markdown path
    /// through a markdown renderer when `kind == Markdown`; otherwise
    /// they fall back to plain-text rendering.
    pub kind: MarkupKind,
    /// The character index in the document where the mouse currently is
    pub trigger_char_index: usize,
    /// Debounce timer for the *trigger* path (pointer-stopped-on-cell).
    /// Distinct from the dismiss-grace timer on [`HoverLifecycle`].
    pub timer: Option<Timer>,
    /// The actual LSP range for the hover content (useful for highlighting).
    /// The same range is mirrored to [`HoverLifecycle::hot_zone`] so the
    /// pointer-move observer can suppress re-arm inside it.
    pub range: Option<Range>,
    /// Last viewport-local pointer position seen by the hover-move observer.
    /// Used to skip the rope/layout hit-test when the pointer has barely
    /// moved since the previous event (a per-pixel `Pointer<Move>` would
    /// otherwise re-run `screen_to_char_pos` on every sub-pixel jitter).
    pub last_pointer_pos: Option<bevy::math::Vec2>,
}

impl Default for LspHoverPopup {
    fn default() -> Self {
        Self {
            visible: false,
            content: String::new(),
            // Default to plain text — servers that send markdown override
            // this on the response. Picking `PlainText` as the default
            // keeps cold-state rendering deterministic regardless of
            // whether the markdown feature is on.
            kind: MarkupKind::PlainText,
            trigger_char_index: 0,
            timer: None,
            range: None,
            last_pointer_pos: None,
        }
    }
}

impl LspHoverPopup {
    /// Clear popup-local state. The id-bump that invalidates in-flight
    /// LSP responses + clears the hot zone lives on [`HoverLifecycle`]
    /// — call its `.dismiss()` at the same site.
    pub fn reset(&mut self) {
        self.visible = false;
        self.content.clear();
        self.kind = MarkupKind::PlainText;
        self.timer = None;
        self.range = None;
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
}

impl LspSignatureHelpPopup {
    pub fn current_signature(&self) -> Option<&SignatureInformation> {
        self.signatures.get(self.active_signature)
    }

    /// Clear popup-local state. The id-bump that invalidates in-flight
    /// LSP responses lives on [`SignatureLifecycle`] — call its
    /// `.dismiss()` at the same site.
    pub fn dismiss(&mut self) {
        self.visible = false;
        self.signatures.clear();
        self.active_signature = 0;
        self.active_parameter = 0;
    }

    /// Backward-compat alias.
    pub fn reset(&mut self) {
        self.dismiss();
    }
}

/// Per-editor code actions popup state. Holds the response from
/// `textDocument/codeAction` (quick-fix / refactor menu).
///
/// **Producer not yet wired.** The transport
/// ([`super::super::systems::request_code_actions`] helper) and the
/// response handler are in place; a system that watches for diagnostics
/// under the cursor (or an explicit user trigger like `Ctrl+.`) and
/// calls the helper still needs to be added before the lightbulb UI is
/// fed.
#[derive(Component, Default)]
pub struct LspCodeActionsPopup {
    pub visible: bool,
    pub actions: Vec<bevy_lsp::CodeActionOrCommand>,
    pub selected_index: usize,
    pub range: Option<Range>,
}

impl LspCodeActionsPopup {
    /// Clear popup-local state. The id-bump that invalidates in-flight
    /// LSP responses lives on [`CodeActionsLifecycle`] — call its
    /// `.dismiss()` at the same site.
    pub fn dismiss(&mut self) {
        self.visible = false;
        self.actions.clear();
        self.selected_index = 0;
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

/// Per-feature LSP debounce timers for popup-driving requests
/// (cursor-stops-then-fire pattern). Completion + hover use this
/// shared component; document highlights and code actions live on
/// their own popup state Components ([`LspDocumentHighlights`],
/// [`LspCodeActionsPopup`]) and run their own timers there.
#[derive(Component)]
pub struct LspDebounceTimers {
    /// Completion: armed by `listen_completion_requests` from
    /// `LspConfig::completion::delay_ms`.
    pub completion_timer: Timer,
    pub pending_completion: Option<PendingLspRequest>,

    /// Hover: armed by `listen_hover_requests` from
    /// `LspConfig::hover::delay_ms`.
    pub hover_timer: Timer,
    pub pending_hover: Option<PendingLspRequest>,
}

impl Default for LspDebounceTimers {
    fn default() -> Self {
        // Durations are placeholders — the request-arming code re-sets each
        // timer's duration from `LspConfig` before resetting it, so what
        // matters here is that the timer starts in a `finished()` state.
        let t = |secs: f32| {
            let mut timer = Timer::from_seconds(secs, TimerMode::Once);
            timer.tick(std::time::Duration::from_secs_f32(secs));
            timer
        };
        Self {
            completion_timer: t(0.1),
            pending_completion: None,
            hover_timer: t(0.3),
            pending_hover: None,
        }
    }
}

/// Per-editor `textDocument/didChange` batcher.
///
/// `listen_text_edit_events` pushes one [`TextDocumentContentChangeEvent`]
/// per editor edit and resets `timer`; `sync_lsp_document` flushes the
/// accumulated batch in a single notification when the timer expires.
/// Mirrors the debounced approach used by Zed / VS Code / Helix: typing
/// bursts collapse into one server reparse, but trailing edits still
/// reach the server within `LspConfig::did_change_delay_ms`.
///
/// `LspDocument` owns `uri` and `version`; this Component owns the
/// pending payload + cadence.
#[derive(Component)]
pub struct LspDidChangeBatcher {
    /// Pending incremental change events, in edit order.
    pub pending: Vec<TextDocumentContentChangeEvent>,
    /// When set, the next flush sends a full-document sync instead of
    /// the accumulated incremental batch. Set on first edit without a
    /// pre-edit rope snapshot, or when `LspConfig::full_document_sync`
    /// is on. Cleared after every flush.
    pub force_full_doc: bool,
    /// Debounce timer; reset on every queued edit, fires when typing pauses.
    pub timer: Timer,
}

impl Default for LspDidChangeBatcher {
    fn default() -> Self {
        // Duration overwritten on first edit from `LspConfig::did_change_delay_ms`.
        Self {
            pending: Vec::new(),
            force_full_doc: false,
            timer: Timer::from_seconds(0.15, TimerMode::Once),
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
        self.error = None;
    }

    /// Start preparing rename at position. The "preparing" flag now
    /// lives implicitly on [`RenameLifecycle`] — non-zero `request_id`
    /// with `popup_entity == None` means a prepareRename is in flight.
    pub fn start_prepare(&mut self, position: Position) {
        self.reset();
        self.position = Some(position);
    }

    /// Handle prepare rename response
    pub fn on_prepare_response(&mut self, range: Range, placeholder: Option<String>) {
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

/// Dismiss-side lifecycle state shared by all five LSP popups.
///
/// Not a `Component` itself — wrapped by the five typed lifecycle
/// Components ([`HoverLifecycle`] et al.) so each popup gets its own
/// instance on the editor entity and a `Query<&mut HoverLifecycle>`
/// doesn't conflict with `Query<&mut CompletionLifecycle>`.
///
/// The shared concerns live here so we only encode the dismiss state
/// machine once:
///
/// - `request_id` deduplicates LSP responses — bumped on every send
///   AND on every dismiss, so any in-flight response for a popup that
///   has since closed (or been retriggered at a different position)
///   is dropped by the response handler's id check.
/// - `popup_entity` is the chrome `Entity` produced by the renderer.
///   `None` while the popup is hidden; the renderer writes it when it
///   spawns chrome and clears it on dismiss.
/// - `pointer_in_popup` is flipped by `Pointer<Over>`/`Pointer<Out>`
///   observers attached to the popup chrome, so the editor's hover
///   tracker can tell "pointer left the editor, but landed on my own
///   popup" from "pointer actually left everything".
/// - `dismiss_after` is the *grace* timer — `Some` means "schedule
///   dismiss" and the generic tick system fires the feature's
///   `dismiss()` only after it elapses with `pointer_in_popup ==
///   false`. Moving the cursor onto the popup before it fires cancels
///   it.
/// - `hot_zone` is the LSP range the popup is "about" (set from the
///   response). Pointer moves *inside* the range don't re-arm the
///   debounce timer or cancel the visible popup — fixes the "wobble
///   within an identifier dismisses my hover" symptom.
#[derive(Default, Debug)]
pub struct PopupLifecycleData {
    pub request_id: u64,
    /// Highest request id whose response has been accepted into the
    /// popup state. Newer requests are still in flight; older
    /// responses are dropped as stale. This lets us accept whichever
    /// response arrives latest in wall-clock order — important when
    /// the LSP server takes longer to respond than the user takes to
    /// re-arm a new request at a nearby position (rust-analyzer cold
    /// hovers are ~3s; a user moving the mouse may have already armed
    /// requests 2-3 ahead).
    pub last_accepted_id: u64,
    pub popup_entity: Option<Entity>,
    pub pointer_in_popup: bool,
    pub dismiss_after: Option<Timer>,
    pub hot_zone: Option<Range>,
}

impl PopupLifecycleData {
    /// Bump and return the next request id. The send-site stamps it
    /// onto the wire message; the response handler matches.
    pub fn new_request(&mut self) -> u64 {
        self.request_id = self.request_id.wrapping_add(1);
        self.request_id
    }

    /// Bump the id (invalidating any in-flight response), clear the
    /// popup-entity ref and the hot zone, and cancel any pending
    /// dismiss timer. Callers also reset their feature-specific state
    /// (selected_index, content, etc.).
    pub fn dismiss(&mut self) {
        self.request_id = self.request_id.wrapping_add(1);
        self.last_accepted_id = self.request_id;
        self.popup_entity = None;
        self.hot_zone = None;
        self.dismiss_after = None;
        self.pointer_in_popup = false;
    }

    /// Returns `true` when a response with `id` should supersede
    /// whatever is currently in the popup. Used by response handlers
    /// in place of a strict `id == request_id` check, which loses
    /// responses whenever the user has re-armed before the previous
    /// reply arrives. Accepts responses for *any* request the editor
    /// has actually sent (`id <= request_id`) and that is newer than
    /// what the popup is currently showing (`id > last_accepted_id`).
    pub fn accept_response(&mut self, id: u64) -> bool {
        if id == 0 || id > self.request_id || id <= self.last_accepted_id {
            return false;
        }
        self.last_accepted_id = id;
        true
    }

    /// Schedule dismissal after `ms` milliseconds. Cancelled by setting
    /// `dismiss_after = None` (e.g. when the pointer re-enters the
    /// popup or the hot zone).
    pub fn arm_dismiss(&mut self, ms: u32) {
        self.dismiss_after = Some(Timer::new(
            std::time::Duration::from_millis(ms as u64),
            TimerMode::Once,
        ));
    }

    /// True when the LSP `hot_zone` covers `position`. False when no
    /// hot zone has been published yet (e.g. between debounce-fire and
    /// response).
    pub fn hot_zone_contains(&self, position: Position) -> bool {
        let Some(range) = self.hot_zone else {
            return false;
        };
        let after_start = (position.line, position.character)
            >= (range.start.line, range.start.character);
        let before_end =
            (position.line, position.character) <= (range.end.line, range.end.character);
        after_start && before_end
    }
}

/// Generate one typed lifecycle Component + its `PopupBackref` peer.
/// Each Component is a newtype around [`PopupLifecycleData`] that
/// `Deref`s through, so call sites look like
/// `lc.new_request()` / `lc.popup_entity = …` rather than
/// `lc.0.new_request()`.
macro_rules! lifecycle_component {
    ($lifecycle:ident, $backref:ident) => {
        #[derive(Component, Default, Debug)]
        pub struct $lifecycle(pub PopupLifecycleData);

        impl std::ops::Deref for $lifecycle {
            type Target = PopupLifecycleData;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl std::ops::DerefMut for $lifecycle {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }

        /// Reverse handle inserted on the popup chrome entity so the
        /// chrome's `Pointer<Over>` / `Pointer<Out>` observers can
        /// locate the editor whose lifecycle to mutate.
        #[derive(Component, Debug)]
        pub struct $backref {
            pub editor: Entity,
        }

        impl $backref {
            pub fn from_editor(editor: Entity) -> Self {
                Self { editor }
            }
        }
    };
}

lifecycle_component!(HoverLifecycle, HoverPopupBackref);
lifecycle_component!(CompletionLifecycle, CompletionPopupBackref);
lifecycle_component!(SignatureLifecycle, SignaturePopupBackref);
lifecycle_component!(CodeActionsLifecycle, CodeActionsPopupBackref);
lifecycle_component!(RenameLifecycle, RenamePopupBackref);

/// Marker inserted on a popup chrome entity once its `Pointer<Over>` /
/// `Pointer<Out>` observers have been attached, so renderers can
/// re-spawn the chrome data Component every frame without re-attaching
/// observers (which would multiply per-frame and never deregister).
#[derive(Component, Debug)]
pub struct PopupObserversAttached;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_request_returns_strictly_increasing_ids() {
        let mut lc = PopupLifecycleData::default();
        let a = lc.new_request();
        let b = lc.new_request();
        let c = lc.new_request();
        assert!(a < b && b < c);
        assert_eq!(lc.request_id, c);
    }

    #[test]
    fn dismiss_bumps_id_and_clears_state() {
        let mut lc = PopupLifecycleData::default();
        let id_before = lc.new_request();
        lc.popup_entity = Some(Entity::from_raw_u32(42).unwrap());
        lc.hot_zone = Some(Range::new(
            Position::new(0, 0),
            Position::new(0, 5),
        ));
        lc.arm_dismiss(200);
        lc.pointer_in_popup = true;

        lc.dismiss();

        assert_eq!(lc.request_id, id_before.wrapping_add(1));
        assert!(lc.popup_entity.is_none());
        assert!(lc.hot_zone.is_none());
        assert!(lc.dismiss_after.is_none());
        assert!(!lc.pointer_in_popup);
    }

    #[test]
    fn arm_dismiss_sets_timer_of_requested_duration() {
        let mut lc = PopupLifecycleData::default();
        lc.arm_dismiss(250);
        let t = lc.dismiss_after.expect("timer armed");
        assert_eq!(t.duration(), std::time::Duration::from_millis(250));
    }

    #[test]
    fn hot_zone_contains_inclusive_at_boundaries() {
        let mut lc = PopupLifecycleData::default();
        lc.hot_zone = Some(Range::new(
            Position::new(2, 4),
            Position::new(2, 10),
        ));
        assert!(lc.hot_zone_contains(Position::new(2, 4)));
        assert!(lc.hot_zone_contains(Position::new(2, 7)));
        assert!(lc.hot_zone_contains(Position::new(2, 10)));
        assert!(!lc.hot_zone_contains(Position::new(2, 3)));
        assert!(!lc.hot_zone_contains(Position::new(2, 11)));
        assert!(!lc.hot_zone_contains(Position::new(1, 4)));
    }

    #[test]
    fn hot_zone_contains_false_when_unset() {
        let lc = PopupLifecycleData::default();
        assert!(!lc.hot_zone_contains(Position::new(0, 0)));
    }

    #[test]
    fn accept_response_drops_id_zero() {
        let mut lc = PopupLifecycleData::default();
        lc.new_request();
        assert!(!lc.accept_response(0));
    }

    #[test]
    fn accept_response_drops_unseen_future_ids() {
        let mut lc = PopupLifecycleData::default();
        lc.new_request(); // request_id = 1
        assert!(!lc.accept_response(2));
        assert_eq!(lc.last_accepted_id, 0);
    }

    #[test]
    fn accept_response_drops_already_superseded() {
        let mut lc = PopupLifecycleData::default();
        lc.new_request();
        lc.new_request(); // request_id = 2
        assert!(lc.accept_response(2));
        assert_eq!(lc.last_accepted_id, 2);
        // A late-arriving response for the older request is now stale.
        assert!(!lc.accept_response(1));
    }

    #[test]
    fn accept_response_accepts_older_inflight_when_no_newer_seen() {
        // The case that motivated the helper: user re-arms requests
        // 1, 2, 3 before any response arrives. Response for 2 lands
        // first; it should display. Response for 1 then arrives — it's
        // stale because 2 has already been shown. Response for 3
        // arrives — it should supersede 2.
        let mut lc = PopupLifecycleData::default();
        lc.new_request();
        lc.new_request();
        lc.new_request(); // request_id = 3
        assert!(lc.accept_response(2));
        assert!(!lc.accept_response(1));
        assert!(lc.accept_response(3));
    }
}
