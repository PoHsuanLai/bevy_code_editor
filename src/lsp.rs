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

#[cfg(feature = "lsp")]
use bevy::prelude::*;
use bevy::sprite::Anchor;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

#[cfg(feature = "lsp")]
use lsp_types::*;

#[cfg(feature = "lsp")]
use tower_lsp::jsonrpc::Result as JsonRpcResult;

#[cfg(feature = "lsp")]
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::types::{CodeEditorState, ViewportDimensions};
use crate::settings::EditorSettings;

/// LSP client resource
#[cfg(feature = "lsp")]
#[derive(Resource)]
pub struct LspClient {
    /// Send messages to language server
    tx: Sender<LspMessage>,
    /// Receive responses from language server (wrapped in Mutex for Sync)
    rx: Mutex<Receiver<LspResponse>>,
}

/// State for the auto-completion UI
#[cfg(feature = "lsp")]
#[derive(Resource, Default)]
pub struct CompletionState {
    /// Whether the completion box is currently visible
    pub visible: bool,
    /// Current list of completion items
    pub items: Vec<CompletionItem>,
    /// Index of the currently selected item
    pub selected_index: usize,
    /// Character index in the document where completion started
    pub start_char_index: usize,
    /// Filter text (what the user has typed since opening completion)
    pub filter: String,
}

/// State for hover popups
#[cfg(feature = "lsp")]
#[derive(Resource, Default)]
pub struct HoverState {
    /// Whether the hover box is currently visible
    pub visible: bool,
    /// Content to display in the hover box (markdown)
    pub content: String,
    /// The character index in the document where the hover was triggered
    pub trigger_char_index: usize,
    /// Timer for delaying hover display/hide
    pub timer: Option<Timer>,
    /// The actual LSP range for the hover content (useful for highlighting)
    pub range: Option<Range>,
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

    /// Definition location
    Definition {
        location: Location,
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

        // We don't start the server automatically in new() anymore
        // call start() with the command to run

        Self { 
            tx,
            rx: Mutex::new(rx) 
        }
    }

    /// Start the language server
    pub fn start(&mut self, command: &str, args: &[&str]) -> std::io::Result<()> {
        use std::process::{Command, Stdio};
        use std::io::{BufRead, BufReader, Write, Read};
        use std::thread;
        use serde_json::Value;

        info!("Starting LSP server: {} {:?}", command, args);

        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut stdin = child.stdin.take().expect("Failed to open stdin");
        let stdout = child.stdout.take().expect("Failed to open stdout");
        let stderr = child.stderr.take().expect("Failed to open stderr");

        // Channel for Bevy -> Server
        let (tx_to_server, rx_from_bevy) = channel::<LspMessage>();
        self.tx = tx_to_server;

        // Channel for Server -> Bevy
        let (tx_to_bevy, rx_from_server) = channel::<LspResponse>();
        self.rx = Mutex::new(rx_from_server);

        // Writer Thread (Bevy -> LSP Stdin)
        thread::spawn(move || {
            while let Ok(msg) = rx_from_bevy.recv() {
                // Convert LspMessage to JSON-RPC
                let json_str = match msg_to_json(&msg) {
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
                            debug!("LSP Received raw: {}", body_str);
                            if let Ok(json) = serde_json::from_str::<Value>(&body_str) {
                                // Convert to LspResponse
                                if let Some(response) = parse_lsp_response(&json) {
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
                    info!("LSP stderr: {}", line);
                }
            }
        });

        Ok(())
    }

    /// Send message to language server
    pub fn send(&self, message: LspMessage) {
        debug!("LSP Sending: {:?}", message);
        let _ = self.tx.send(message);
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
fn msg_to_json(msg: &LspMessage) -> serde_json::Result<String> {
    use serde_json::json;
    
    // Simple ID counter could be added if needed, using 1 for now
    let id = 1;

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
        _ => return Ok("{}".to_string()), // TODO: Implement others
    };

    let rpc = if is_notification {
        json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        })
    } else {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        })
    };

    serde_json::to_string(&rpc)
}

// Helper to parse JSON-RPC response to LspResponse
#[cfg(feature = "lsp")]
fn parse_lsp_response(json: &serde_json::Value) -> Option<LspResponse> {
    // Basic parsing logic - needs refinement for production
    
    // Check for diagnostics notification
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
    }

    // Check for completion response (by ID matching, ideally)
    if let Some(result) = json.get("result") {
        // Try parsing as completion items
        // Result can be CompletionList or Vec<CompletionItem>
        if let Ok(items) = serde_json::from_value::<Vec<CompletionItem>>(result.clone()) {
            return Some(LspResponse::Completion { items });
        }
        if let Ok(list) = serde_json::from_value::<CompletionList>(result.clone()) {
            return Some(LspResponse::Completion { items: list.items });
        }
    }

    // Check for hover response
    if let Some(result) = json.get("result") {
        if let Ok(hover) = serde_json::from_value::<Hover>(result.clone()) {
            // hover.contents is HoverContents, not Option<HoverContents>
            let contents_enum = hover.contents; 
            let content_string = match contents_enum {
                lsp_types::HoverContents::Markup(markup) => markup.value,
                lsp_types::HoverContents::Scalar(marked_string) => {
                    // MarkedString is an enum itself, we need to match its variants
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
    }

    None
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
                debug!("Received {} completion items", items.len());
                completion_state.items = items;
                completion_state.visible = !completion_state.items.is_empty();
                completion_state.selected_index = 0;
                if completion_state.visible {
                    info!("Completion UI set to visible");
                }
                // TODO: Set start_char_index based on cursor position or trigger
            },
            LspResponse::Hover { content, range } => {
                info!("Received hover content: {}", content);
                hover_state.content = content;
                hover_state.range = range;
                hover_state.visible = true;
                hover_state.timer = Some(Timer::new(std::time::Duration::from_millis(300), TimerMode::Once)); // Small delay for display
                info!("Hover UI set to visible (timer started)");
            }
            LspResponse::Definition { location } => {
                // For simplicity, just move cursor to definition start
                // TODO: Handle multiple locations, different files, etc.
                let line_num = location.range.start.line as usize;
                let char_in_line = location.range.start.character as usize;
                
                // Convert line/char to rope char index
                if line_num < editor_state.rope.len_lines() {
                    let line_start_char = editor_state.rope.line_to_char(line_num);
                    let target_char_pos = line_start_char + char_in_line;
                    
                    editor_state.cursor_pos = target_char_pos.min(editor_state.rope.len_chars());
                    editor_state.needs_update = true;
                    info!("Moved cursor to definition at line {}, char {}", line_num, char_in_line);
                }
            }
            _ => {} // TODO: Handle other LSP responses
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
    // If not visible, ensure cleared and return
    if !completion_state.visible || completion_state.items.is_empty() {
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

    info!("Spawning Completion UI with {} items", completion_state.items.len());

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
    
    // Calculate dynamic width based on longest item
    let max_char_count = completion_state.items.iter()
        .take(10) // Only consider visible items for width
        .map(|item| {
            let label_len = item.label.chars().count();
            let detail_len = item.detail.as_deref().map(|d| d.chars().count()).unwrap_or(0);
            label_len + detail_len + 5 // +5 for spacing
        })
        .max()
        .unwrap_or(20);

    let calculated_width = (max_char_count as f32 * char_width) + 20.0; // Padding
    let box_width = calculated_width.max(200.0).min(600.0); // Clamp width

    let box_height = (completion_state.items.len().min(10) as f32 * line_height) + 10.0; // Max 10 items
    
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
        // Render items
        let visible_items = completion_state.items.iter().take(10); // Show max 10
        
        for (i, item) in visible_items.enumerate() {
            let is_selected = i == completion_state.selected_index;
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
            
            // Item Label
            parent.spawn((
                Text2d::new(&item.label),
                TextFont {
                    font: settings.font.handle.clone().unwrap_or_default(),
                    font_size: settings.font.size,
                    ..default()
                },
                TextColor(bevy::prelude::Color::WHITE),
                Transform::from_translation(Vec3::new(-box_width / 2.0 + 10.0, item_y, 0.2)),
                Anchor::CENTER_LEFT,
            ));
            
            // Item Detail (Right aligned, if exists)
            if let Some(detail) = &item.detail {
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
    mut hover_state: ResMut<HoverState>,
    editor_state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    time: Res<Time>,
    ui_query: Query<Entity, With<HoverUI>>,
) {
    // If not visible or empty, ensure cleared and return
    if !hover_state.visible || hover_state.content.is_empty() {
        for entity in ui_query.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }
    
    // Timer for delayed display
    if let Some(timer) = &mut hover_state.timer {
        timer.tick(time.delta());
        if !timer.is_finished() {
            return;
        }
    }

    // If visible/ready but nothing relevant changed, skip update (keep existing UI)
    // Note: We check hover_state.is_changed() because timer ticking might not mark it changed if accessed via &mut but not modified? 
    // Actually timer tick modifies it. So it will be changed.
    // But we should check if content/pos changed.
    // If timer is just ticking, we don't need to rebuild UI every frame once visible.
    // But timer is part of state.
    // Ideally we separate "content changed" from "timer ticked".
    // For now, let's just rebuild if state changed.
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
        parent.spawn(( 
            Text2d::new(hover_state.content.clone()),
            TextFont {
                font: settings.font.handle.clone().unwrap_or_default(),
                font_size,
                ..default()
            },
            TextColor(bevy::prelude::Color::WHITE),
            Transform::from_translation(Vec3::new(0.0, 0.0, 0.1)),
            Anchor::CENTER_LEFT,
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
}

/// System to sync document with LSP (debounced)
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

    if sync_state.timer.finished() {
        if let Some(uri) = &editor_state.document_uri {
            let version = 1; // TODO: Increment version properly
            
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
            
            //debug!("LSP Sync: Sent DidChange");
        }
        
        sync_state.dirty = false;
        sync_state.timer.reset();
    }
}