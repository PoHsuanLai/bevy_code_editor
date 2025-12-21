//! LSP client for communication with language servers
//!
//! Handles process spawning, JSON-RPC protocol, and message routing.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use lsp_types::*;
use serde_json::{json, Value};

use super::capabilities::ServerCapabilitiesCache;
use super::messages::{CodeActionOrCommand, LspMessage, LspResponse, RequestType};

/// Global ID counter for LSP requests
static NEXT_REQUEST_ID: AtomicI64 = AtomicI64::new(1);

/// Default request timeout in seconds
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

/// Pending request info
struct PendingRequest {
    request_type: RequestType,
    sent_at: Instant,
    timeout: Duration,
}

/// LSP client resource
#[derive(Resource)]
pub struct LspClient {
    /// Send messages to language server
    tx: Sender<(LspMessage, Option<(i64, RequestType)>)>,
    /// Receive responses from language server
    rx: Mutex<Receiver<LspResponse>>,
    /// Track pending requests: ID -> (RequestType, sent_at, timeout)
    pending_requests: Arc<Mutex<HashMap<i64, PendingRequest>>>,
    /// Server capabilities cache
    pub capabilities: ServerCapabilitiesCache,
    /// Whether the server is initialized
    pub initialized: bool,
    /// Child process handle (for cleanup)
    child_process: Option<Arc<Mutex<Child>>>,
}

impl Default for LspClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LspClient {
    /// Create a new LSP client (not yet connected)
    pub fn new() -> Self {
        let (tx, _rx_server) = channel();
        let (_tx_client, rx) = channel();

        Self {
            tx,
            rx: Mutex::new(rx),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            capabilities: ServerCapabilitiesCache::new(),
            initialized: false,
            child_process: None,
        }
    }

    /// Start the language server process
    pub fn start(&mut self, command: &str, args: &[&str]) -> std::io::Result<()> {
        #[cfg(debug_assertions)]
        eprintln!("[LSP] Starting server: {} {:?}", command, args);

        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().expect("Failed to open stdin");
        let stdout = child.stdout.take().expect("Failed to open stdout");
        let stderr = child.stderr.take().expect("Failed to open stderr");

        // Store child process handle
        self.child_process = Some(Arc::new(Mutex::new(child)));

        // Channel for Bevy -> Server
        let (tx_to_server, rx_from_bevy) = channel::<(LspMessage, Option<(i64, RequestType)>)>();
        self.tx = tx_to_server;

        // Channel for Server -> Bevy
        let (tx_to_bevy, rx_from_server) = channel::<LspResponse>();
        self.rx = Mutex::new(rx_from_server);

        // Share pending_requests with reader thread
        let pending_requests = self.pending_requests.clone();
        let capabilities = self.capabilities.clone();

        // Writer Thread
        thread::spawn(move || {
            Self::writer_thread(stdin, rx_from_bevy);
        });

        // Reader Thread
        let tx_to_bevy_clone = tx_to_bevy.clone();
        thread::spawn(move || {
            Self::reader_thread(stdout, tx_to_bevy_clone, pending_requests, capabilities);
        });

        // Stderr Logger Thread
        thread::spawn(move || {
            Self::stderr_thread(stderr);
        });

        Ok(())
    }

    /// Writer thread: sends messages to LSP stdin
    fn writer_thread(
        mut stdin: std::process::ChildStdin,
        rx: Receiver<(LspMessage, Option<(i64, RequestType)>)>,
    ) {
        while let Ok((msg, id_info)) = rx.recv() {
            let id = id_info.map(|(id, _)| id);
            let json_str = match msg_to_json(&msg, id) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[LSP] Failed to serialize message: {:?}", e);
                    continue;
                }
            };

            let content = format!("Content-Length: {}\r\n\r\n{}", json_str.len(), json_str);
            if let Err(e) = stdin.write_all(content.as_bytes()) {
                eprintln!("[LSP] Failed to write to stdin: {:?}", e);
                break;
            }
            let _ = stdin.flush();
        }
    }

    /// Reader thread: receives messages from LSP stdout
    fn reader_thread(
        stdout: std::process::ChildStdout,
        tx: Sender<LspResponse>,
        pending_requests: Arc<Mutex<HashMap<i64, PendingRequest>>>,
        capabilities: ServerCapabilitiesCache,
    ) {
        let mut reader = BufReader::new(stdout);
        let mut buffer = String::new();

        loop {
            buffer.clear();
            if reader.read_line(&mut buffer).unwrap_or(0) == 0 {
                break;
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
                if reader.read_exact(&mut body_buf).is_ok() {
                    if let Ok(body_str) = String::from_utf8(body_buf) {
                        if let Ok(json) = serde_json::from_str::<Value>(&body_str) {
                            // Get request type from pending_requests
                            let response_id = json.get("id").and_then(|v| v.as_i64());
                            let request_type = if let Some(id) = response_id {
                                if let Ok(mut pending) = pending_requests.lock() {
                                    pending.remove(&id).map(|p| p.request_type)
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                            if let Some(response) = parse_lsp_response(&json, request_type, &capabilities) {
                                let _ = tx.send(response);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Stderr logger thread
    fn stderr_thread(stderr: std::process::ChildStderr) {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(line) = line {
                eprintln!("[LSP stderr] {}", line);
            }
        }
    }

    /// Send a message to the language server
    pub fn send(&self, message: LspMessage) {
        // Check capabilities before sending (skip for Initialize and notifications)
        if !self.should_send(&message) {
            #[cfg(debug_assertions)]
            eprintln!("[LSP] Skipping unsupported request: {:?}", std::mem::discriminant(&message));
            return;
        }

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
            LspMessage::SignatureHelp { .. } => {
                let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
                Some((id, RequestType::SignatureHelp))
            }
            LspMessage::CodeAction { .. } => {
                let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
                Some((id, RequestType::CodeAction))
            }
            LspMessage::InlayHint { .. } => {
                let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
                Some((id, RequestType::InlayHint))
            }
            LspMessage::ExecuteCommand { .. } => {
                let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
                // Reuse CodeAction type for execute command responses
                Some((id, RequestType::CodeAction))
            }
            LspMessage::DocumentHighlight { .. } => {
                let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
                Some((id, RequestType::DocumentHighlight))
            }
            LspMessage::PrepareRename { .. } => {
                let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
                Some((id, RequestType::PrepareRename))
            }
            LspMessage::Rename { .. } => {
                let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
                Some((id, RequestType::Rename))
            }
            // Notifications don't have IDs
            LspMessage::Initialized | LspMessage::DidOpen { .. } | LspMessage::DidChange { .. } => None,
        };

        // Track the request
        if let Some((id, request_type)) = id_info {
            if let Ok(mut pending) = self.pending_requests.lock() {
                pending.insert(id, PendingRequest {
                    request_type,
                    sent_at: Instant::now(),
                    timeout: Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS),
                });
            }
            #[cfg(debug_assertions)]
            eprintln!("[LSP] Sending request (id={}): {:?}", id, std::mem::discriminant(&message));
        }

        let _ = self.tx.send((message, id_info));
    }

    /// Check if a message should be sent based on server capabilities
    fn should_send(&self, message: &LspMessage) -> bool {
        match message {
            // Always allow initialize and notifications
            LspMessage::Initialize { .. } => true,
            LspMessage::Initialized => true,
            LspMessage::DidOpen { .. } => true,
            LspMessage::DidChange { .. } => true,
            LspMessage::ExecuteCommand { .. } => true,

            // Check capabilities for requests
            LspMessage::Completion { .. } => self.capabilities.supports_completion(),
            LspMessage::Hover { .. } => self.capabilities.supports_hover(),
            LspMessage::GotoDefinition { .. } => self.capabilities.supports_definition(),
            LspMessage::References { .. } => self.capabilities.supports_references(),
            LspMessage::Format { .. } => self.capabilities.supports_formatting(),
            LspMessage::SignatureHelp { .. } => self.capabilities.supports_signature_help(),
            LspMessage::CodeAction { .. } => self.capabilities.supports_code_actions(),
            LspMessage::InlayHint { .. } => self.capabilities.supports_inlay_hints(),
            LspMessage::DocumentHighlight { .. } => self.capabilities.supports_document_highlight(),
            LspMessage::PrepareRename { .. } => self.capabilities.supports_prepare_rename(),
            LspMessage::Rename { .. } => self.capabilities.supports_rename(),
        }
    }

    /// Try to receive a response from the language server
    pub fn try_recv(&self) -> Option<LspResponse> {
        if let Ok(rx) = self.rx.try_lock() {
            rx.try_recv().ok()
        } else {
            None
        }
    }

    /// Clean up timed out requests
    pub fn cleanup_timeouts(&self) {
        if let Ok(mut pending) = self.pending_requests.lock() {
            let now = Instant::now();
            pending.retain(|id, req| {
                let timed_out = now.duration_since(req.sent_at) > req.timeout;
                if timed_out {
                    #[cfg(debug_assertions)]
                    eprintln!("[LSP] Request {} timed out ({:?})", id, req.request_type);
                }
                !timed_out
            });
        }
    }

    /// Check if the server is ready (initialized with capabilities)
    pub fn is_ready(&self) -> bool {
        self.initialized
    }

    /// Get completion trigger characters
    pub fn completion_triggers(&self) -> Vec<String> {
        self.capabilities.completion_triggers()
    }

    /// Get signature help trigger characters
    pub fn signature_help_triggers(&self) -> Vec<String> {
        self.capabilities.signature_help_triggers()
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // Try to gracefully shut down the child process
        if let Some(child) = &self.child_process {
            if let Ok(mut child) = child.lock() {
                let _ = child.kill();
            }
        }
    }
}

/// Convert LspMessage to JSON-RPC string
fn msg_to_json(msg: &LspMessage, id: Option<i64>) -> serde_json::Result<String> {
    let (method, params, is_notification) = match msg {
        LspMessage::Initialize { root_uri, capabilities } => (
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": root_uri,
                "capabilities": capabilities,
                "clientInfo": {
                    "name": "bevy_code_editor",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
            false,
        ),
        LspMessage::Initialized => ("initialized", json!({}), true),
        LspMessage::DidOpen { uri, language_id, version, text } => (
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": version,
                    "text": text
                }
            }),
            true,
        ),
        LspMessage::DidChange { uri, version, changes } => (
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": changes
            }),
            true,
        ),
        LspMessage::Completion { uri, position } => (
            "textDocument/completion",
            json!({
                "textDocument": { "uri": uri },
                "position": position
            }),
            false,
        ),
        LspMessage::Hover { uri, position } => (
            "textDocument/hover",
            json!({
                "textDocument": { "uri": uri },
                "position": position
            }),
            false,
        ),
        LspMessage::GotoDefinition { uri, position } => (
            "textDocument/definition",
            json!({
                "textDocument": { "uri": uri },
                "position": position
            }),
            false,
        ),
        LspMessage::References { uri, position } => (
            "textDocument/references",
            json!({
                "textDocument": { "uri": uri },
                "position": position,
                "context": { "includeDeclaration": true }
            }),
            false,
        ),
        LspMessage::Format { uri, options } => (
            "textDocument/formatting",
            json!({
                "textDocument": { "uri": uri },
                "options": options
            }),
            false,
        ),
        LspMessage::SignatureHelp { uri, position } => (
            "textDocument/signatureHelp",
            json!({
                "textDocument": { "uri": uri },
                "position": position
            }),
            false,
        ),
        LspMessage::CodeAction { uri, range, diagnostics } => (
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": uri },
                "range": range,
                "context": {
                    "diagnostics": diagnostics
                }
            }),
            false,
        ),
        LspMessage::InlayHint { uri, range } => (
            "textDocument/inlayHint",
            json!({
                "textDocument": { "uri": uri },
                "range": range
            }),
            false,
        ),
        LspMessage::ExecuteCommand { command, arguments } => (
            "workspace/executeCommand",
            json!({
                "command": command,
                "arguments": arguments
            }),
            false,
        ),
        LspMessage::DocumentHighlight { uri, position } => (
            "textDocument/documentHighlight",
            json!({
                "textDocument": { "uri": uri },
                "position": position
            }),
            false,
        ),
        LspMessage::PrepareRename { uri, position } => (
            "textDocument/prepareRename",
            json!({
                "textDocument": { "uri": uri },
                "position": position
            }),
            false,
        ),
        LspMessage::Rename { uri, position, new_name } => (
            "textDocument/rename",
            json!({
                "textDocument": { "uri": uri },
                "position": position,
                "newName": new_name
            }),
            false,
        ),
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
            "id": id.unwrap_or(1),
            "method": method,
            "params": params
        })
    };

    serde_json::to_string(&rpc)
}

/// Parse JSON-RPC response to LspResponse
fn parse_lsp_response(
    json: &Value,
    request_type: Option<RequestType>,
    capabilities: &ServerCapabilitiesCache,
) -> Option<LspResponse> {
    // Check for notifications (no id, has method)
    if let Some(method) = json.get("method").and_then(|m| m.as_str()) {
        return parse_notification(json, method);
    }

    // For responses, use the request_type to determine how to parse
    let result = json.get("result")?;

    // Handle null results
    if result.is_null() {
        // Log null results for debugging
        #[cfg(debug_assertions)]
        if let Some(ref rt) = request_type {
            eprintln!("[LSP] Received null result for request type: {:?}", rt);
        }
        return None;
    }

    match request_type {
        Some(RequestType::Initialize) => {
            if let Ok(init_result) = serde_json::from_value::<InitializeResult>(result.clone()) {
                capabilities.set(init_result.capabilities.clone());
                return Some(LspResponse::Initialized {
                    capabilities: init_result.capabilities,
                });
            }
            None
        }
        Some(RequestType::Completion) => {
            // Result can be CompletionList or Vec<CompletionItem>
            if let Ok(items) = serde_json::from_value::<Vec<CompletionItem>>(result.clone()) {
                return Some(LspResponse::Completion { items, is_incomplete: false });
            }
            if let Ok(list) = serde_json::from_value::<CompletionList>(result.clone()) {
                return Some(LspResponse::Completion {
                    items: list.items,
                    is_incomplete: list.is_incomplete,
                });
            }
            None
        }
        Some(RequestType::Hover) => {
            if let Ok(hover) = serde_json::from_value::<Hover>(result.clone()) {
                let content = extract_hover_content(&hover.contents);
                return Some(LspResponse::Hover {
                    content,
                    range: hover.range,
                });
            }
            None
        }
        Some(RequestType::GotoDefinition) => {
            // Can be Location, Vec<Location>, or Vec<LocationLink>
            if let Ok(location) = serde_json::from_value::<Location>(result.clone()) {
                return Some(LspResponse::Definition { locations: vec![location] });
            }
            if let Ok(locations) = serde_json::from_value::<Vec<Location>>(result.clone()) {
                if !locations.is_empty() {
                    return Some(LspResponse::Definition { locations });
                }
            }
            if let Ok(links) = serde_json::from_value::<Vec<LocationLink>>(result.clone()) {
                let locations: Vec<Location> = links
                    .into_iter()
                    .map(|link| Location {
                        uri: link.target_uri,
                        range: link.target_selection_range,
                    })
                    .collect();
                if !locations.is_empty() {
                    return Some(LspResponse::Definition { locations });
                }
            }
            None
        }
        Some(RequestType::References) => {
            if let Ok(locations) = serde_json::from_value::<Vec<Location>>(result.clone()) {
                return Some(LspResponse::References { locations });
            }
            None
        }
        Some(RequestType::Format) => {
            if let Ok(edits) = serde_json::from_value::<Vec<TextEdit>>(result.clone()) {
                return Some(LspResponse::Format { edits });
            }
            None
        }
        Some(RequestType::SignatureHelp) => {
            if let Ok(sig_help) = serde_json::from_value::<SignatureHelp>(result.clone()) {
                return Some(LspResponse::SignatureHelp {
                    signatures: sig_help.signatures,
                    active_signature: sig_help.active_signature,
                    active_parameter: sig_help.active_parameter,
                });
            }
            None
        }
        Some(RequestType::CodeAction) => {
            if let Ok(actions) = serde_json::from_value::<Vec<CodeActionOrCommand>>(result.clone()) {
                return Some(LspResponse::CodeActions { actions });
            }
            // Try parsing as lsp_types::CodeActionOrCommand
            if let Ok(lsp_actions) = serde_json::from_value::<Vec<lsp_types::CodeActionOrCommand>>(result.clone()) {
                let actions: Vec<CodeActionOrCommand> = lsp_actions
                    .into_iter()
                    .map(|a| match a {
                        lsp_types::CodeActionOrCommand::CodeAction(action) => {
                            CodeActionOrCommand::Action(action)
                        }
                        lsp_types::CodeActionOrCommand::Command(cmd) => {
                            CodeActionOrCommand::Command(cmd)
                        }
                    })
                    .collect();
                return Some(LspResponse::CodeActions { actions });
            }
            None
        }
        Some(RequestType::InlayHint) => {
            if let Ok(hints) = serde_json::from_value::<Vec<InlayHint>>(result.clone()) {
                return Some(LspResponse::InlayHints { hints });
            }
            None
        }
        Some(RequestType::DocumentHighlight) => {
            if let Ok(highlights) = serde_json::from_value::<Vec<DocumentHighlight>>(result.clone()) {
                return Some(LspResponse::DocumentHighlights { highlights });
            }
            None
        }
        Some(RequestType::PrepareRename) => {
            #[cfg(debug_assertions)]
            eprintln!("[LSP] PrepareRename result: {}", result);

            // Can be Range or { range, placeholder }
            if let Ok(range) = serde_json::from_value::<Range>(result.clone()) {
                #[cfg(debug_assertions)]
                eprintln!("[LSP] Parsed PrepareRename as Range: {:?}", range);
                return Some(LspResponse::PrepareRename { range, placeholder: None });
            }
            if let Ok(prepare) = serde_json::from_value::<PrepareRenameResponse>(result.clone()) {
                #[cfg(debug_assertions)]
                eprintln!("[LSP] Parsed PrepareRename as PrepareRenameResponse");
                match prepare {
                    PrepareRenameResponse::Range(range) => {
                        return Some(LspResponse::PrepareRename { range, placeholder: None });
                    }
                    PrepareRenameResponse::RangeWithPlaceholder { range, placeholder } => {
                        return Some(LspResponse::PrepareRename { range, placeholder: Some(placeholder) });
                    }
                    PrepareRenameResponse::DefaultBehavior { .. } => {
                        // Server uses default behavior, we need to extract word at position
                        #[cfg(debug_assertions)]
                        eprintln!("[LSP] PrepareRename DefaultBehavior - not supported yet");
                        return None;
                    }
                }
            }
            #[cfg(debug_assertions)]
            eprintln!("[LSP] Failed to parse PrepareRename result");
            None
        }
        Some(RequestType::Rename) => {
            if let Ok(edit) = serde_json::from_value::<WorkspaceEdit>(result.clone()) {
                return Some(LspResponse::Rename { edit });
            }
            None
        }
        None => None,
    }
}

/// Parse LSP notification
fn parse_notification(json: &Value, method: &str) -> Option<LspResponse> {
    match method {
        "textDocument/publishDiagnostics" => {
            if let Some(params) = json.get("params") {
                if let Ok(diag_params) = serde_json::from_value::<PublishDiagnosticsParams>(params.clone()) {
                    return Some(LspResponse::Diagnostics {
                        uri: diag_params.uri,
                        diagnostics: diag_params.diagnostics,
                    });
                }
            }
            None
        }
        _ => {
            #[cfg(debug_assertions)]
            eprintln!("[LSP] Unhandled notification: {}", method);
            None
        }
    }
}

/// Extract text content from HoverContents
fn extract_hover_content(contents: &HoverContents) -> String {
    match contents {
        HoverContents::Markup(markup) => markup.value.clone(),
        HoverContents::Scalar(marked_string) => match marked_string {
            MarkedString::String(s) => s.clone(),
            MarkedString::LanguageString(ls) => ls.value.clone(),
        },
        HoverContents::Array(arr) => arr
            .iter()
            .map(|ms| match ms {
                MarkedString::String(s) => s.clone(),
                MarkedString::LanguageString(ls) => ls.value.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}
