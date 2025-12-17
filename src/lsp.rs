//! LSP (Language Server Protocol) integration
//!
//! This module provides LSP client functionality for advanced code editor features like:
//! - Diagnostics (errors, warnings)
//! - Code completion
//! - Hover information
//! - Go to definition
//! - Find references
//! - Code actions
//! - Formatting

use bevy::prelude::*;
use bevy::sprite::Anchor;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::collections::HashMap;

use lsp_types::*;

use crate::types::{CodeEditorState, ViewportDimensions};
use crate::settings::EditorSettings;

/// Global ID counter for LSP requests
#[cfg(feature = "lsp")]
static NEXT_REQUEST_ID: AtomicI64 = AtomicI64::new(1);

/// Type of LSP request (used to match responses)
#[cfg(feature = "lsp")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestType {
    Initialize,
    Completion,
    Hover,
    GotoDefinition,
    References,
    Format,
}

/// LSP client resource
#[cfg(feature = "lsp")]
#[derive(Resource)]
pub struct LspClient {
    /// Send messages to language server (includes request type for ID tracking)
    tx: Sender<(LspMessage, Option<(i64, RequestType)>)>,
    /// Receive responses from language server (wrapped in Mutex for Sync)
    rx: Mutex<Receiver<LspResponse>>,
    /// Track pending requests: ID -> RequestType
    pending_requests: Arc<Mutex<HashMap<i64, RequestType>>>,
}

/// Default maximum number of visible items in completion popup
/// This is used as a fallback; prefer settings.completion.max_visible_items
#[cfg(feature = "lsp")]
pub const COMPLETION_MAX_VISIBLE_DEFAULT: usize = 10;

/// A word completion item (extracted from document)
#[cfg(feature = "lsp")]
#[derive(Clone, Debug)]
pub struct WordCompletionItem {
    /// The word text
    pub word: String,
}

/// Unified completion item for display (can be LSP or word-based)
#[cfg(feature = "lsp")]
#[derive(Clone, Debug)]
pub enum UnifiedCompletionItem {
    /// LSP completion item
    Lsp(CompletionItem),
    /// Word from document
    Word(WordCompletionItem),
}

#[cfg(feature = "lsp")]
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
}

/// State for the auto-completion UI
#[cfg(feature = "lsp")]
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
}

#[cfg(feature = "lsp")]
impl CompletionState {
    /// Ensure the selected item is visible by adjusting scroll_offset
    /// Uses max_visible_items from settings (or default if not provided)
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
}

#[cfg(feature = "lsp")]
impl CompletionState {
    /// Get filtered items based on current filter text using fuzzy matching
    /// Returns unified items (LSP + word completions) sorted by match score (best matches first)
    /// LSP items are prioritized over word completions
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
                .filter(|item| !lsp_labels.contains(item.word.as_str())) // Avoid duplicates
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
    /// Extracts unique words (identifiers) from the document, excluding the word at cursor
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
                // Filter: at least 2 chars, not the cursor word, not already seen
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
}

/// Get the word at a given character position (for exclusion during word extraction)
#[cfg(feature = "lsp")]
fn get_word_at_position(rope: &ropey::Rope, char_pos: usize) -> Option<String> {
    if char_pos == 0 || char_pos > rope.len_chars() {
        return None;
    }

    let text = rope.to_string();
    let byte_pos = rope.char_to_byte(char_pos.min(rope.len_chars()));

    // Find word boundaries
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
#[cfg(feature = "lsp")]
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

/// State for LSP document synchronization
#[cfg(feature = "lsp")]
#[derive(Resource)]
pub struct LspSyncState {
    /// Whether the document has changed since last sync
    pub dirty: bool,
    /// Timer to debounce sync requests
    pub timer: Timer,
}

#[cfg(feature = "lsp")]
impl Default for LspSyncState {
    fn default() -> Self {
        Self {
            dirty: false,
            // Sync 200ms after last edit
            timer: Timer::from_seconds(0.2, TimerMode::Once),
        }
    }
}

/// Marker for the completion UI root entity
#[cfg(feature = "lsp")]
#[derive(Component)]
pub struct CompletionUI;

/// Marker for the hover UI root entity
#[cfg(feature = "lsp")]
#[derive(Component)]
pub struct HoverUI;

/// Message emitted when navigation to a different file is requested
/// External code should listen to this message to handle cross-file navigation
#[cfg(feature = "lsp")]
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct NavigateToFileEvent {
    /// URI of the file to open
    pub uri: Url,
    /// Line number (0-indexed)
    pub line: usize,
    /// Character position in line (0-indexed)
    pub character: usize,
}

/// Message emitted when there are multiple definition/reference locations
/// External code can display a picker UI for the user to choose
#[cfg(feature = "lsp")]
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct MultipleLocationsEvent {
    /// All available locations
    pub locations: Vec<Location>,
    /// Type of locations (definition, references, etc.)
    pub location_type: LocationType,
}

/// Type of location event
#[cfg(feature = "lsp")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationType {
    Definition,
    References,
}


/// Messages sent to language server
#[cfg(feature = "lsp")]
#[derive(Debug, Clone)]
pub enum LspMessage {
    /// Initialize the language server
    Initialize {
        root_uri: Url,
        capabilities: ClientCapabilities,
    },

    /// Initialized notification
    Initialized,

    /// Text document opened
    DidOpen {
        uri: Url,
        language_id: String,
        version: i32,
        text: String,
    },

    /// Text document changed
    DidChange {
        uri: Url,
        version: i32,
        changes: Vec<TextDocumentContentChangeEvent>,
    },

    /// Request completion at position
    Completion {
        uri: Url,
        position: Position,
    },

    /// Request hover information
    Hover {
        uri: Url,
        position: Position,
    },

    /// Go to definition
    GotoDefinition {
        uri: Url,
        position: Position,
    },

    /// Find references
    References {
        uri: Url,
        position: Position,
    },

    /// Format document
    Format {
        uri: Url,
    },
}

/// Responses from language server
#[cfg(feature = "lsp")]
#[derive(Debug, Clone)]
pub enum LspResponse {
    /// Diagnostics published
    Diagnostics {
        uri: Url,
        diagnostics: Vec<Diagnostic>,
    },

    /// Completion response
    Completion {
        items: Vec<CompletionItem>,
    },

    /// Hover response
    Hover {
        content: String,
        range: Option<Range>,
    },

    /// Definition location(s) - may have multiple definitions
    Definition {
        locations: Vec<Location>,
    },

    /// Reference locations
    References {
        locations: Vec<Location>,
    },

    /// Format edits
    Format {
        edits: Vec<TextEdit>,
    },
}

/// Diagnostic marker for rendering in editor
#[cfg(feature = "lsp")]
#[derive(Component, Clone, Debug)]
pub struct DiagnosticMarker {
    /// Line number (0-indexed)
    pub line: usize,
    /// Diagnostic severity
    pub severity: DiagnosticSeverity,
    /// Diagnostic message
    pub message: String,
    /// Text range
    pub range: Range,
}

#[cfg(feature = "lsp")]
impl LspClient {
    /// Create a new LSP client
    pub fn new() -> Self {
        let (tx, _rx_server) = channel();
        let (_tx_client, rx) = channel();

        Self {
            tx,
            rx: Mutex::new(rx),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start the language server
    pub fn start(&mut self, command: &str, args: &[&str]) -> std::io::Result<()> {
        use std::process::{Command, Stdio};
        use std::io::{BufRead, BufReader, Write, Read};
        use std::thread;
        use serde_json::Value;

        #[cfg(debug_assertions)]
        eprintln!("[LSP] Starting server: {} {:?}", command, args);

        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut stdin = child.stdin.take().expect("Failed to open stdin");
        let stdout = child.stdout.take().expect("Failed to open stdout");
        let stderr = child.stderr.take().expect("Failed to open stderr");

        // Channel for Bevy -> Server (now includes optional ID and request type)
        let (tx_to_server, rx_from_bevy) = channel::<(LspMessage, Option<(i64, RequestType)>)>();
        self.tx = tx_to_server;

        // Channel for Server -> Bevy
        let (tx_to_bevy, rx_from_server) = channel::<LspResponse>();
        self.rx = Mutex::new(rx_from_server);

        // Share pending_requests with the reader thread
        let pending_requests = self.pending_requests.clone();

        // Writer Thread (Bevy -> LSP Stdin)
        thread::spawn(move || {
            while let Ok((msg, id_info)) = rx_from_bevy.recv() {
                // Convert LspMessage to JSON-RPC with the provided ID
                let id = id_info.map(|(id, _)| id);
                let json_str = match msg_to_json(&msg, id) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Failed to serialize message: {:?}", e);
                        continue;
                    }
                };

                let content = format!("Content-Length: {}\r\n\r\n{}", json_str.len(), json_str);
                if let Err(e) = stdin.write_all(content.as_bytes()) {
                    eprintln!("Failed to write to LSP stdin: {:?}", e);
                    break;
                }
                let _ = stdin.flush();
            }
        });

        // Reader Thread (LSP Stdout -> Bevy)
        let tx_to_bevy_clone = tx_to_bevy.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut buffer = String::new();

            loop {
                buffer.clear();
                // Read Content-Length header
                if reader.read_line(&mut buffer).unwrap_or(0) == 0 {
                    break; // EOF
                }

                let mut content_len = 0;
                if buffer.starts_with("Content-Length: ") {
                    if let Ok(len) = buffer.trim_start_matches("Content-Length: ").trim().parse::<usize>() {
                        content_len = len;
                    }
                }

                // Read empty line
                buffer.clear();
                if reader.read_line(&mut buffer).unwrap_or(0) == 0 {
                    break;
                }

                // Read body
                if content_len > 0 {
                    let mut body_buf = vec![0u8; content_len];
                    if let Ok(()) = reader.read_exact(&mut body_buf) {
                        if let Ok(body_str) = String::from_utf8(body_buf) {
                            // Parse JSON
                            if let Ok(json) = serde_json::from_str::<Value>(&body_str) {
                                // Get the request type from pending_requests using the response ID
                                let response_id = json.get("id").and_then(|v| v.as_i64());
                                let request_type = if let Some(id) = response_id {
                                    if let Ok(mut pending) = pending_requests.lock() {
                                        let req_type = pending.remove(&id);
                                        #[cfg(debug_assertions)]
                                        eprintln!("[LSP] Response id={} matched to {:?}", id, req_type);
                                        req_type
                                    } else {
                                        None
                                    }
                                } else {
                                    // Check if it's a notification (has method but no id)
                                    #[cfg(debug_assertions)]
                                    if let Some(method) = json.get("method").and_then(|m| m.as_str()) {
                                        eprintln!("[LSP] Notification: {}", method);
                                    }
                                    None
                                };

                                // Convert to LspResponse using the request type
                                if let Some(response) = parse_lsp_response(&json, request_type) {
                                    #[cfg(debug_assertions)]
                                    eprintln!("[LSP] Parsed response: {:?}", std::mem::discriminant(&response));
                                    let _ = tx_to_bevy_clone.send(response);
                                }
                            }
                        }
                    }
                }
            }
        });

        // Stderr Logger Thread
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(line) = line {
                    eprintln!("[LSP stderr] {}", line);
                }
            }
        });

        Ok(())
    }

    /// Send message to language server
    pub fn send(&self, message: LspMessage) {
        // Determine if this is a request (needs ID) or notification (no ID)
        let id_info = match &message {
            LspMessage::Initialize { .. } => {
                let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
                Some((id, RequestType::Initialize))
            }
            LspMessage::Completion { .. } => {
                let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
                Some((id, RequestType::Completion))
            }
            LspMessage::Hover { .. } => {
                let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
                Some((id, RequestType::Hover))
            }
            LspMessage::GotoDefinition { .. } => {
                let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
                Some((id, RequestType::GotoDefinition))
            }
            LspMessage::References { .. } => {
                let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
                Some((id, RequestType::References))
            }
            LspMessage::Format { .. } => {
                let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
                Some((id, RequestType::Format))
            }
            // Notifications don't have IDs
            LspMessage::Initialized | LspMessage::DidOpen { .. } | LspMessage::DidChange { .. } => None,
        };

        // Track the request if it has an ID
        if let Some((id, request_type)) = id_info {
            if let Ok(mut pending) = self.pending_requests.lock() {
                pending.insert(id, request_type);
            }
            #[cfg(debug_assertions)]
            eprintln!("[LSP] Sending (id={}): {:?}", id, message);
        } else {
            #[cfg(debug_assertions)]
            eprintln!("[LSP] Sending (notification): {:?}", message);
        }

        let _ = self.tx.send((message, id_info));
    }

    /// Try to receive response from language server
    pub fn try_recv(&self) -> Option<LspResponse> {
        if let Ok(rx) = self.rx.try_lock() {
            rx.try_recv().ok()
        } else {
            None
        }
    }
}

// Helper to serialize LspMessage to JSON-RPC string
#[cfg(feature = "lsp")]
fn msg_to_json(msg: &LspMessage, id: Option<i64>) -> serde_json::Result<String> {
    use serde_json::json;

    let (method, params, is_notification) = match msg {
        LspMessage::Initialize { root_uri, capabilities } => (
            "initialize", 
            json!({ "processId": null, "rootUri": root_uri, "capabilities": capabilities }),
            false
        ),
        LspMessage::Initialized => (
            "initialized",
            json!({}),
            true
        ),
        LspMessage::DidOpen { uri, language_id, version, text } => (
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": language_id, "version": version, "text": text } }),
            true
        ),
        LspMessage::DidChange { uri, version, changes } => (
            "textDocument/didChange",
            json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": changes }),
            true
        ),
        LspMessage::Completion { uri, position } => (
            "textDocument/completion",
            json!({ "textDocument": { "uri": uri }, "position": position }),
            false
        ),
        LspMessage::Hover { uri, position } => (
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": position }),
            false
        ),
        LspMessage::GotoDefinition { uri, position } => (
            "textDocument/definition",
            json!({ "textDocument": { "uri": uri }, "position": position }),
            false
        ),
        LspMessage::References { uri, position } => (
            "textDocument/references",
            json!({
                "textDocument": { "uri": uri },
                "position": position,
                "context": { "includeDeclaration": true }
            }),
            false
        ),
        LspMessage::Format { uri } => (
            "textDocument/formatting",
            json!({
                "textDocument": { "uri": uri },
                "options": {
                    "tabSize": 4,
                    "insertSpaces": true
                }
            }),
            false
        ),
    };

    let rpc = if is_notification {
        json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        })
    } else {
        // Use provided ID for requests, fallback to 1 if not provided
        let request_id = id.unwrap_or(1);
        json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params
        })
    };

    serde_json::to_string(&rpc)
}

// Helper to parse JSON-RPC response to LspResponse
#[cfg(feature = "lsp")]
fn parse_lsp_response(json: &serde_json::Value, request_type: Option<RequestType>) -> Option<LspResponse> {
    // Check for diagnostics notification (server-initiated, no ID)
    if let Some(method) = json.get("method").and_then(|m| m.as_str()) {
        if method == "textDocument/publishDiagnostics" {
            if let Some(params) = json.get("params") {
                if let Ok(diagnostics) = serde_json::from_value::<PublishDiagnosticsParams>(params.clone()) {
                    return Some(LspResponse::Diagnostics {
                        uri: diagnostics.uri,
                        diagnostics: diagnostics.diagnostics,
                    });
                }
            }
        }
        // Other notifications can be added here
        return None;
    }

    // For responses, use the request_type to determine how to parse
    let result = json.get("result")?;

    // Handle null results (e.g., no hover info available)
    if result.is_null() {
        return None;
    }

    match request_type {
        Some(RequestType::Completion) => {
            // Result can be CompletionList or Vec<CompletionItem>
            if let Ok(items) = serde_json::from_value::<Vec<CompletionItem>>(result.clone()) {
                return Some(LspResponse::Completion { items });
            }
            if let Ok(list) = serde_json::from_value::<CompletionList>(result.clone()) {
                return Some(LspResponse::Completion { items: list.items });
            }
            #[cfg(debug_assertions)]
            eprintln!("[LSP] Failed to parse completion response");
            None
        }
        Some(RequestType::Hover) => {
            if let Ok(hover) = serde_json::from_value::<Hover>(result.clone()) {
                let content_string = match hover.contents {
                    lsp_types::HoverContents::Markup(markup) => markup.value,
                    lsp_types::HoverContents::Scalar(marked_string) => {
                        match marked_string {
                            lsp_types::MarkedString::String(s) => s,
                            lsp_types::MarkedString::LanguageString(lang_string_struct) => lang_string_struct.value,
                        }
                    },
                    lsp_types::HoverContents::Array(arr) => arr.into_iter().map(|marked_string| {
                        match marked_string {
                            lsp_types::MarkedString::String(s) => s,
                            lsp_types::MarkedString::LanguageString(lang_string_struct) => lang_string_struct.value,
                        }
                    }).collect::<Vec<_>>().join("\n"),
                };

                return Some(LspResponse::Hover {
                    content: content_string,
                    range: hover.range,
                });
            }
            #[cfg(debug_assertions)]
            eprintln!("[LSP] Failed to parse hover response");
            None
        }
        Some(RequestType::GotoDefinition) => {
            // Can be Location, Vec<Location>, or Vec<LocationLink>
            // Single location
            if let Ok(location) = serde_json::from_value::<Location>(result.clone()) {
                return Some(LspResponse::Definition { locations: vec![location] });
            }
            // Multiple locations
            if let Ok(locations) = serde_json::from_value::<Vec<Location>>(result.clone()) {
                if !locations.is_empty() {
                    return Some(LspResponse::Definition { locations });
                }
            }
            // LocationLink format (convert to Location)
            if let Ok(links) = serde_json::from_value::<Vec<lsp_types::LocationLink>>(result.clone()) {
                let locations: Vec<Location> = links.into_iter().map(|link| Location {
                    uri: link.target_uri,
                    range: link.target_selection_range,
                }).collect();
                if !locations.is_empty() {
                    return Some(LspResponse::Definition { locations });
                }
            }
            #[cfg(debug_assertions)]
            eprintln!("[LSP] Failed to parse definition response");
            None
        }
        Some(RequestType::References) => {
            if let Ok(locations) = serde_json::from_value::<Vec<Location>>(result.clone()) {
                return Some(LspResponse::References { locations });
            }
            #[cfg(debug_assertions)]
            eprintln!("[LSP] Failed to parse references response");
            None
        }
        Some(RequestType::Format) => {
            if let Ok(edits) = serde_json::from_value::<Vec<TextEdit>>(result.clone()) {
                return Some(LspResponse::Format { edits });
            }
            #[cfg(debug_assertions)]
            eprintln!("[LSP] Failed to parse format response");
            None
        }
        Some(RequestType::Initialize) => {
            // Initialize response is typically just acknowledged, we don't need to return anything special
            #[cfg(debug_assertions)]
            eprintln!("[LSP] Initialize response received");
            None
        }
        None => {
            // No request type - this shouldn't happen for responses with IDs
            // Fall back to guessing (legacy behavior)
            #[cfg(debug_assertions)]
            eprintln!("[LSP] Warning: Response without known request type, cannot parse");
            None
        }
    }
}

#[cfg(feature = "lsp")]
impl Default for LspClient {
    fn default() -> Self {
        Self::new()
    }
}

/// System to process LSP messages and update Bevy resources
#[cfg(feature = "lsp")]
pub fn process_lsp_messages(
    lsp_client: Res<LspClient>,
    mut commands: Commands,
    diagnostics_query: Query<Entity, With<DiagnosticMarker>>,
    mut completion_state: ResMut<CompletionState>,
    mut hover_state: ResMut<HoverState>,
    mut editor_state: ResMut<CodeEditorState>,
    mut navigate_events: MessageWriter<NavigateToFileEvent>,
    mut multi_location_events: MessageWriter<MultipleLocationsEvent>,
) {
    // Process new messages from LSP
    while let Some(response) = lsp_client.try_recv() {
        match response {
            LspResponse::Diagnostics { uri: _, diagnostics } => {
                // Clear old diagnostics
                // In a real app, we should only clear diagnostics for the specific URI
                for entity in diagnostics_query.iter() {
                    commands.entity(entity).despawn();
                }

                for diagnostic in diagnostics {
                    // Spawn diagnostic marker entity
                    commands.spawn(DiagnosticMarker {
                        line: diagnostic.range.start.line as usize,
                        severity: diagnostic.severity.unwrap_or(DiagnosticSeverity::HINT),
                        message: diagnostic.message.clone(),
                        range: diagnostic.range,
                    });
                }
            },
            LspResponse::Completion { items } => {
                #[cfg(debug_assertions)]
                eprintln!("[LSP] Completion response: {} items", items.len());
                completion_state.items = items;
                completion_state.visible = !completion_state.items.is_empty();
                completion_state.selected_index = 0;
            },
            LspResponse::Hover { content, range } => {
                #[cfg(debug_assertions)]
                eprintln!("[LSP] Hover response: {} chars, pending={:?}, trigger={}",
                    content.len(), hover_state.pending_char_index, hover_state.trigger_char_index);

                // Only show hover if we have content AND mouse is still at the position we requested
                if !content.is_empty() {
                    if let Some(pending_pos) = hover_state.pending_char_index {
                        if pending_pos == hover_state.trigger_char_index {
                            #[cfg(debug_assertions)]
                            eprintln!("[LSP] Hover showing: first 100 chars = {:?}", &content[..content.len().min(100)]);
                            hover_state.content = content;
                            hover_state.range = range;
                            hover_state.visible = true;
                        } else {
                            #[cfg(debug_assertions)]
                            eprintln!("[LSP] Hover DISCARDED: position mismatch (pending={} != trigger={})",
                                pending_pos, hover_state.trigger_char_index);
                        }
                    } else {
                        #[cfg(debug_assertions)]
                        eprintln!("[LSP] Hover DISCARDED: no pending request");
                    }
                }
                hover_state.pending_char_index = None; // Clear pending regardless
            }
            LspResponse::Definition { locations } => {
                if locations.is_empty() {
                    #[cfg(debug_assertions)]
                    eprintln!("[LSP] Definition response: no locations");
                    continue;
                }

                #[cfg(debug_assertions)]
                eprintln!("[LSP] Definition response: {} location(s)", locations.len());

                // Check if multiple locations - emit event for picker UI
                if locations.len() > 1 {
                    multi_location_events.write(MultipleLocationsEvent {
                        locations: locations.clone(),
                        location_type: LocationType::Definition,
                    });
                    // For now, also navigate to first location
                }

                // Navigate to first location
                let location = &locations[0];
                let current_uri = editor_state.document_uri.as_ref();
                let is_same_file = current_uri.is_some_and(|uri| uri == &location.uri);

                if is_same_file {
                    // Same file - just move cursor
                    let line_num = location.range.start.line as usize;
                    let char_in_line = location.range.start.character as usize;

                    if line_num < editor_state.rope.len_lines() {
                        let line_start_char = editor_state.rope.line_to_char(line_num);
                        let target_char_pos = line_start_char + char_in_line;

                        editor_state.cursor_pos = target_char_pos.min(editor_state.rope.len_chars());
                        editor_state.needs_update = true;
                    }
                } else {
                    // Different file - emit navigation event
                    navigate_events.write(NavigateToFileEvent {
                        uri: location.uri.clone(),
                        line: location.range.start.line as usize,
                        character: location.range.start.character as usize,
                    });
                }
            }
            LspResponse::References { locations } => {
                #[cfg(debug_assertions)]
                eprintln!("[LSP] References response: {} location(s)", locations.len());

                if !locations.is_empty() {
                    // Emit event with all reference locations
                    multi_location_events.write(MultipleLocationsEvent {
                        locations,
                        location_type: LocationType::References,
                    });
                }
            }
            LspResponse::Format { edits } => {
                #[cfg(debug_assertions)]
                eprintln!("[LSP] Format response: {} edit(s)", edits.len());

                // Apply formatting edits in reverse order (to preserve positions)
                let mut edits_sorted = edits;
                edits_sorted.sort_by(|a, b| {
                    // Sort by start position, descending (apply from end to start)
                    let a_pos = (a.range.start.line, a.range.start.character);
                    let b_pos = (b.range.start.line, b.range.start.character);
                    b_pos.cmp(&a_pos)
                });

                for edit in edits_sorted {
                    let start_line = edit.range.start.line as usize;
                    let end_line = edit.range.end.line as usize;
                    let start_char = edit.range.start.character as usize;
                    let end_char = edit.range.end.character as usize;

                    // Convert LSP positions to rope char indices
                    if start_line < editor_state.rope.len_lines() {
                        let start_line_char = editor_state.rope.line_to_char(start_line);
                        let start_pos = start_line_char + start_char;

                        let end_pos = if end_line < editor_state.rope.len_lines() {
                            let end_line_char = editor_state.rope.line_to_char(end_line);
                            (end_line_char + end_char).min(editor_state.rope.len_chars())
                        } else {
                            editor_state.rope.len_chars()
                        };

                        let start_pos = start_pos.min(editor_state.rope.len_chars());
                        let end_pos = end_pos.min(editor_state.rope.len_chars());

                        if start_pos <= end_pos {
                            // Remove old text
                            let start_byte = editor_state.rope.char_to_byte(start_pos);
                            let end_byte = editor_state.rope.char_to_byte(end_pos);
                            editor_state.rope.remove(start_byte..end_byte);
                            // Insert new text
                            editor_state.rope.insert(start_pos, &edit.new_text);
                        }
                    }
                }

                editor_state.pending_update = true;
                editor_state.dirty_lines = None; // Full re-highlight needed
                editor_state.previous_line_count = editor_state.rope.len_lines();
            }
        }
    }
}

/// Render the auto-completion UI
#[cfg(feature = "lsp")]
pub fn update_completion_ui(
    mut commands: Commands,
    completion_state: Res<CompletionState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    ui_query: Query<Entity, With<CompletionUI>>,
) {
    // Get filtered items
    let filtered_items = completion_state.filtered_items();

    // If not visible or no filtered items, ensure cleared and return
    if !completion_state.visible || filtered_items.is_empty() {
        for entity in ui_query.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }

    // If visible but nothing relevant changed, skip update (keep existing UI)
    if !completion_state.is_changed() && !editor_state.is_changed() && !viewport.is_changed() && !settings.is_changed() {
        return;
    }

    // Clear old (rebuilding)
    for entity in ui_query.iter() {
        commands.entity(entity).despawn();
    }

    // Calculate position relative to cursor
    let cursor_pos = editor_state.cursor_pos.min(editor_state.rope.len_chars());
    let line_index = editor_state.rope.char_to_line(cursor_pos);
    let line_start = editor_state.rope.line_to_char(line_index);
    let col_index = cursor_pos - line_start;

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;

    // We want the box to appear BELOW the current line.
    let x_offset = settings.ui.layout.code_margin_left + (col_index as f32 * char_width);
    let y_offset = settings.ui.layout.margin_top + editor_state.scroll_offset + ((line_index + 1) as f32 * line_height);

    // Convert to Bevy world coordinates
    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    // Calculate dynamic width based on longest filtered item
    let max_char_count = filtered_items.iter()
        .take(10) // Only consider visible items for width
        .map(|item| {
            let label_len = item.label().chars().count();
            let detail_len = item.detail().map(|d| d.chars().count()).unwrap_or(0);
            label_len + detail_len + 5 // +5 for spacing
        })
        .max()
        .unwrap_or(20);

    let calculated_width = (max_char_count as f32 * char_width) + 20.0; // Padding
    let box_width = calculated_width.max(200.0).min(600.0); // Clamp width

    // Calculate visible item count (use settings)
    let max_visible = settings.completion.max_visible_items;
    let total_items = filtered_items.len();
    let visible_count = total_items.min(max_visible);
    let box_height = (visible_count as f32 * line_height) + 10.0;

    let pos = Vec3::new(
        -viewport_width / 2.0 + x_offset + viewport.offset_x + box_width / 2.0,
        viewport_height / 2.0 - y_offset - box_height / 2.0,
        100.0 // High Z-index to float on top
    );

    // Spawn container
    commands.spawn((
        Sprite {
            color: bevy::prelude::Color::srgba(0.15, 0.15, 0.15, 0.95), // Dark background
            custom_size: Some(Vec2::new(box_width, box_height)),
            ..default()
        },
        Transform::from_translation(pos),
        CompletionUI,
        Name::new("CompletionBox"),
    )).with_children(|parent| {
        // Render visible items (with scroll offset)
        let scroll_offset = completion_state.scroll_offset;
        let visible_items = filtered_items.iter()
            .skip(scroll_offset)
            .take(max_visible);

        for (i, item) in visible_items.enumerate() {
            let absolute_index = scroll_offset + i;
            let is_selected = absolute_index == completion_state.selected_index;
            let bg_color = if is_selected {
                bevy::prelude::Color::srgba(0.2, 0.4, 0.8, 0.8) // Highlight selection
            } else {
                bevy::prelude::Color::NONE
            };

            let item_y = (box_height / 2.0) - (i as f32 * line_height) - (line_height / 2.0) - 5.0;

            // Item background (if selected)
            if is_selected {
                parent.spawn((
                    Sprite {
                        color: bg_color,
                        custom_size: Some(Vec2::new(box_width - 4.0, line_height)),
                        ..default()
                    },
                    Transform::from_translation(Vec3::new(0.0, item_y, 0.1)),
                ));
            }

            // Item Label (word completions in slightly different color)
            let label_color = if item.is_word() {
                bevy::prelude::Color::srgba(0.9, 0.9, 0.8, 1.0) // Slightly yellow-ish for word completions
            } else {
                bevy::prelude::Color::WHITE
            };

            parent.spawn((
                Text2d::new(item.label()),
                TextFont {
                    font: settings.font.handle.clone().unwrap_or_default(),
                    font_size: settings.font.size,
                    ..default()
                },
                TextColor(label_color),
                Transform::from_translation(Vec3::new(-box_width / 2.0 + 10.0, item_y, 0.2)),
                Anchor::CENTER_LEFT,
            ));

            // Item Detail (Right aligned, if exists)
            if let Some(detail) = item.detail() {
                 parent.spawn((
                    Text2d::new(detail),
                    TextFont {
                        font: settings.font.handle.clone().unwrap_or_default(),
                        font_size: settings.font.size * 0.8, // Slightly smaller
                        ..default()
                    },
                    TextColor(bevy::prelude::Color::srgba(0.7, 0.7, 0.7, 1.0)), // Grey
                    Transform::from_translation(Vec3::new(box_width / 2.0 - 10.0, item_y, 0.2)),
                    Anchor::CENTER_RIGHT,
                ));
            }
        }
    });
}

/// Render the hover UI
#[cfg(feature = "lsp")]
pub fn update_hover_ui(
    mut commands: Commands,
    hover_state: Res<HoverState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    ui_query: Query<Entity, With<HoverUI>>,
) {
    // If not visible or empty, ensure cleared and return
    if !hover_state.visible || hover_state.content.is_empty() {
        for entity in ui_query.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }

    // If visible/ready but nothing relevant changed, skip update (keep existing UI)
    if !hover_state.is_changed() && !editor_state.is_changed() && !viewport.is_changed() && !settings.is_changed() {
        return;
    }

    // Despawn existing UI (rebuilding)
    for entity in ui_query.iter() {
        commands.entity(entity).despawn();
    }

    // Calculate position relative to trigger char index
    let trigger_char_index = hover_state.trigger_char_index.min(editor_state.rope.len_chars());
    let line_index = editor_state.rope.char_to_line(trigger_char_index);
    let line_start = editor_state.rope.line_to_char(line_index);
    let col_index = trigger_char_index - line_start;

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;

    // Position: X = code_margin + col * char_width
    // Position: Y = margin_top + scroll + (line + 1) * line_height (1 line below trigger)
    let x_offset = settings.ui.layout.code_margin_left + (col_index as f32 * char_width);
    let y_offset = settings.ui.layout.margin_top + editor_state.scroll_offset + ((line_index + 1) as f32 * line_height);

    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;
    
    let font_size = settings.font.size * 0.9; // Slightly smaller font for hover
    let padding = 10.0;
    
    // Better width estimation
    let max_line_chars = hover_state.content.lines().map(|l| l.chars().count()).max().unwrap_or(0);
    // Use 0.6 * font_size as generic char width estimate if settings.char_width is for main font
    // Actually settings.font.char_width is accurate for the main font. 
    // Since we use 0.9 scale, we scale width too.
    let hover_char_width = settings.font.char_width * 0.9;
    
    let calculated_width = (max_line_chars as f32 * hover_char_width) + padding * 2.0;
    let box_width = calculated_width.max(100.0).min(600.0);
    
    let line_count = hover_state.content.lines().count().max(1);
    let box_height = (line_count as f32 * font_size * 1.2) + padding * 2.0; 
    
    let pos = Vec3::new(
        -viewport_width / 2.0 + x_offset + viewport.offset_x + box_width / 2.0,
        viewport_height / 2.0 - y_offset - box_height / 2.0, 
        100.0 // High Z-index to float on top
    );

    // Spawn container
    commands.spawn((
        Sprite {
            color: bevy::prelude::Color::srgba(0.1, 0.1, 0.1, 0.95), // Darker background
            custom_size: Some(Vec2::new(box_width, box_height)),
            ..default()
        },
        Transform::from_translation(pos),
        HoverUI,
        Name::new("HoverBox"),
    )).with_children(|parent| {
        // Position text at left edge of box with padding
        // Parent center is (0,0), so left edge is at -box_width/2
        // We add padding to get the text start position
        let text_x = -box_width / 2.0 + padding;
        // For vertical: top edge is at box_height/2, we want text to start from top with padding
        let text_y = box_height / 2.0 - padding;

        parent.spawn((
            Text2d::new(hover_state.content.clone()),
            TextFont {
                font: settings.font.handle.clone().unwrap_or_default(),
                font_size,
                ..default()
            },
            TextColor(bevy::prelude::Color::WHITE),
            Transform::from_translation(Vec3::new(text_x, text_y, 0.1)),
            Anchor::TOP_LEFT,
        ));
    });
}

/// Reset hover state and hide UI
#[cfg(feature = "lsp")]
pub fn reset_hover_state(hover_state: &mut HoverState) {
    hover_state.visible = false;
    hover_state.content = String::new();
    hover_state.timer = None;
    hover_state.range = None;
    hover_state.request_sent = false;
    hover_state.pending_char_index = None;
}

/// System to sync document with LSP (debounced)
/// Note: This is an alternative sync mechanism. The primary sync happens
/// in input.rs via send_did_change() which properly increments document_version.
/// This debounced sync is useful for external text modifications.
#[cfg(feature = "lsp")]
pub fn sync_lsp_document(
    time: Res<Time>,
    mut sync_state: ResMut<LspSyncState>,
    editor_state: Res<CodeEditorState>,
    lsp_client: Res<LspClient>,
) {
    if !sync_state.dirty {
        return;
    }

    sync_state.timer.tick(time.delta());

    if sync_state.timer.is_finished() {
        if let Some(uri) = &editor_state.document_uri {
            // Use the document_version from editor state (already incremented by input handling)
            let version = editor_state.document_version;

            let change = lsp_types::TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: editor_state.rope.to_string(),
            };

            lsp_client.send(LspMessage::DidChange {
                uri: uri.clone(),
                version,
                changes: vec![change],
            });

            #[cfg(debug_assertions)]
            eprintln!("[LSP] Debounced sync sent, version={}", version);
        }

        sync_state.dirty = false;
        sync_state.timer.reset();
    }
}