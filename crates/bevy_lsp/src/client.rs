//! LSP client transport over `async-lsp` driven by a shared tokio runtime.
//!
//! [`LspClient`] is a per-entity Component owning a handle to the running
//! [`async_lsp::ServerSocket`] (the request-sending side of the language-server
//! peer) plus an mpsc bridge that drains server-pushed notifications and
//! awaited request results into Bevy ECS via [`LspClient::try_recv`].
//!
//! ## Threading model
//!
//! The async-lsp `MainLoop` runs as a tokio task on the shared
//! [`bevy_tokio_tasks::TokioTasksRuntime`] resource. Outgoing requests issued
//! via [`LspClient::send`] are also spawned on the same runtime; the resulting
//! `LspResponse` (typed result for a request, or notification-payload variant
//! pushed by the Router) is forwarded into the bridge `mpsc::UnboundedReceiver`
//! that [`LspClient::try_recv`] drains.
//!
//! ## Pair with
//! - [`crate::LspDocument`] — per-document URI/version.
//! - [`crate::ServerCapabilities`] — populated from `Initialize` response.

use std::ops::ControlFlow;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use async_lsp::concurrency::ConcurrencyLayer;
use async_lsp::panic::CatchUnwindLayer;
use async_lsp::router::Router;
use async_lsp::tracing::TracingLayer;
use async_lsp::{LanguageServer, ServerSocket};
use bevy::prelude::*;
use bevy_tokio_tasks::TokioTasksRuntime;
use lsp_types::notification::PublishDiagnostics;
use lsp_types::*;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tower::ServiceBuilder;

use super::capabilities::ServerCapabilities;
use super::messages::{CodeActionOrCommand, LspMessage, LspResponse};

/// Default request timeout in seconds.
///
/// Kept for API parity. async-lsp drives the request future on the runtime
/// without a per-request deadline; servers handle long-running work via
/// cancel/progress reporting. Public so existing callers can still reference
/// the constant.
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

/// LSP client Component.
///
/// Pair with [`crate::LspDocument`] and [`crate::ServerCapabilities`] on the
/// same entity to model a single editor-document-server triple.
#[derive(Component)]
pub struct LspClient {
    /// Send handle for the running language-server peer. `None` until
    /// [`LspClient::start`] succeeds.
    server: Option<ServerSocket>,
    /// Sender given to the async-lsp Router so notification handlers can push
    /// `LspResponse` variants into the bridge. Cloned for each spawned request
    /// future to push the typed response in.
    response_tx: UnboundedSender<LspResponse>,
    /// Bridge receiver drained by [`LspClient::try_recv`].
    response_rx: Mutex<UnboundedReceiver<LspResponse>>,
    /// Whether the server is initialized.
    ///
    /// Flipped to `true` by consumers when they observe the
    /// [`LspResponse::Initialized`] response. Capability-aware send gating is
    /// the caller's job — see [`LspClient::send_if_supported`].
    pub initialized: bool,
    /// Abort handle for the spawned MainLoop task; on drop the language server
    /// process exits via stdio close (and `kill_on_drop`).
    mainloop_abort: Option<Arc<tokio::task::AbortHandle>>,
}

impl Default for LspClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LspClient {
    /// Construct a not-yet-started client.
    ///
    /// Call [`LspClient::start`] with a [`TokioTasksRuntime`] to spawn the
    /// language server process and wire up the async-lsp main loop.
    pub fn new() -> Self {
        let (response_tx, response_rx) = unbounded_channel();
        Self {
            server: None,
            response_tx,
            response_rx: Mutex::new(response_rx),
            initialized: false,
            mainloop_abort: None,
        }
    }

    /// Spawn the language server and start the async-lsp main loop on the
    /// shared tokio runtime.
    ///
    /// Returns `Err` only on synchronous spawn failure (binary not on PATH,
    /// permissions, ...). Async errors (transport closed, server exited
    /// mid-session) are logged via `warn!` and surface as the bridge channel
    /// stops yielding.
    pub fn start(
        &mut self,
        runtime: &TokioTasksRuntime,
        command: &str,
        args: &[&str],
    ) -> std::io::Result<()> {
        #[cfg(debug_assertions)]
        debug!("[LSP] Starting server: {} {:?}", command, args);

        // Synchronously spawn the child so the caller learns of immediate
        // failures. The async main loop is launched afterwards and given the
        // live child.
        let mut child = tokio::process::Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdin = child.stdin.take().expect("Failed to open stdin");
        let stdout = child.stdout.take().expect("Failed to open stdout");
        let stderr = child.stderr.take().expect("Failed to open stderr");

        // Build the async-lsp router with notification handlers that funnel
        // server-pushed events into the ECS-side bridge. Router state is `()`
        // because notification handlers move their captures by-value.
        let bridge_tx = self.response_tx.clone();
        let (mainloop, server) = async_lsp::MainLoop::new_client(move |_server| {
            let mut router: Router<()> = Router::new(());
            let diag_tx = bridge_tx.clone();
            router
                .notification::<PublishDiagnostics>(move |_, params| {
                    let _ = diag_tx.send(LspResponse::Diagnostics {
                        uri: params.uri,
                        diagnostics: params.diagnostics,
                    });
                    ControlFlow::Continue(())
                })
                // Swallow other notifications (window/showMessage, $/progress)
                // without breaking the main loop.
                .unhandled_notification(|_, _| ControlFlow::Continue(()));

            ServiceBuilder::new()
                .layer(TracingLayer::default())
                .layer(CatchUnwindLayer::default())
                .layer(ConcurrencyLayer::default())
                .service(router)
        });

        self.server = Some(server);

        // Stderr logger.
        runtime.spawn_background_task(move |_ctx| async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                warn!("[LSP stderr] {}", line);
            }
        });

        // Main loop. tokio↔futures I/O bridge: async-lsp's `run_buffered`
        // accepts `futures::AsyncRead/Write`, but `tokio::process::ChildStd*`
        // implement only the tokio variants. `tokio_util::compat` adapts.
        let join = runtime.spawn_background_task(move |_ctx| async move {
            let stdout = stdout.compat();
            let stdin = stdin.compat_write();
            if let Err(err) = mainloop.run_buffered(stdout, stdin).await {
                warn!("[LSP] main loop exited with error: {err}");
            }
            // Reap the child to avoid zombies.
            let _ = child.wait().await;
        });

        self.mainloop_abort = Some(Arc::new(join.abort_handle()));

        Ok(())
    }

    /// Has the server process been started?
    pub fn started(&self) -> bool {
        self.server.is_some()
    }

    /// Send a message to the language server unconditionally.
    ///
    /// Capability-aware gating is the caller's responsibility — see
    /// [`Self::send_if_supported`] for a check-then-send convenience.
    /// For requests, the response is delivered asynchronously via the bridge
    /// channel and surfaces from [`Self::try_recv`].
    pub fn send(&self, message: LspMessage) {
        let Some(server) = self.server.as_ref() else {
            #[cfg(debug_assertions)]
            debug!("[LSP] send() called before start(); dropping message");
            return;
        };
        dispatch(server, &self.response_tx, message);
    }

    /// Send a message only if the server's capabilities permit it.
    ///
    /// Initialization handshake messages (`Initialize`, `Initialized`) and
    /// notifications (`DidOpen`, `DidChange`, `ExecuteCommand`) are always
    /// permitted; everything else is gated on the matching `supports_*`
    /// predicate from `caps`. Returns `true` if the message was queued.
    pub fn send_if_supported(&self, message: LspMessage, caps: &ServerCapabilities) -> bool {
        let allowed = match &message {
            LspMessage::Initialize { .. }
            | LspMessage::Initialized
            | LspMessage::DidOpen { .. }
            | LspMessage::DidChange { .. }
            | LspMessage::ExecuteCommand { .. } => true,

            LspMessage::Completion { .. } => caps.supports_completion(),
            LspMessage::Hover { .. } => caps.supports_hover(),
            LspMessage::GotoDefinition { .. } => caps.supports_definition(),
            LspMessage::References { .. } => caps.supports_references(),
            LspMessage::Format { .. } => caps.supports_formatting(),
            LspMessage::SignatureHelp { .. } => caps.supports_signature_help(),
            LspMessage::CodeAction { .. } => caps.supports_code_actions(),
            LspMessage::InlayHint { .. } => caps.supports_inlay_hints(),
            LspMessage::DocumentHighlight { .. } => caps.supports_document_highlight(),
            LspMessage::PrepareRename { .. } => caps.supports_prepare_rename(),
            LspMessage::Rename { .. } => caps.supports_rename(),
        };

        if allowed {
            self.send(message);
            true
        } else {
            trace!(
                "[LSP] Skipping unsupported request: {:?}",
                std::mem::discriminant(&message)
            );
            false
        }
    }

    /// Try to receive a response from the language server.
    pub fn try_recv(&self) -> Option<LspResponse> {
        if let Ok(mut rx) = self.response_rx.try_lock() {
            rx.try_recv().ok()
        } else {
            None
        }
    }

    /// No-op kept for API parity. async-lsp manages outgoing-request lifetime
    /// internally; orphaned futures are reclaimed when the bridge channel is
    /// dropped.
    pub fn cleanup_timeouts(&self) {}

    /// Check if the server is ready (initialized).
    pub fn is_ready(&self) -> bool {
        self.initialized
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // Aborting the JoinHandle drops the main loop's `tokio::process::Child`
        // wrapper. Combined with `kill_on_drop(true)` set during spawn, this
        // ensures the language-server process exits when the entity is
        // despawned.
        if let Some(abort) = self.mainloop_abort.take() {
            abort.abort();
        }
    }
}

/// Build a [`TextDocumentPositionParams`] from a URI + position.
fn text_pos(uri: Url, position: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri },
        position,
    }
}

/// Spawn an async-lsp request future on the current tokio runtime, mapping
/// `Ok(value)` → bridge-channel send via `map`, `Err` → `warn!`.
///
/// The macro keeps each `LspMessage` arm in [`dispatch`] to its essence:
/// the parameter struct construction and the response decoding. It's a
/// macro rather than a generic function because the per-method
/// `LanguageServer::xxx(&mut self, ...)` call cannot be uniformly named via
/// trait bounds (`R::Result` differs per method).
macro_rules! spawn_request {
    ($server:expr, $tx:expr, $name:literal, $call:expr, |$ok:ident| $map:expr) => {{
        let mut server = $server.clone();
        let tx = $tx.clone();
        tokio::spawn(async move {
            match $call(&mut server).await {
                Ok($ok) => $map(&tx),
                Err(err) => warn!("[LSP] {} failed: {err}", $name),
            }
        });
    }};
}

/// Translate an `LspMessage` into the corresponding `async-lsp` request /
/// notification submission. For requests, spawn the result future on the
/// runtime and forward the typed response into the bridge.
///
/// `LanguageServer` trait methods (`did_open`, `hover`, ...) take `&mut self`,
/// so each arm rebinds a clone of the [`ServerSocket`] as `mut`. The clone is
/// cheap — it's an mpsc::UnboundedSender to the main-loop event queue.
#[allow(clippy::too_many_lines)]
fn dispatch(server: &ServerSocket, bridge_tx: &UnboundedSender<LspResponse>, message: LspMessage) {
    match message {
        LspMessage::Initialize {
            root_uri,
            capabilities,
        } => {
            #[allow(deprecated)]
            let params = InitializeParams {
                process_id: Some(std::process::id()),
                root_uri: Some(root_uri),
                capabilities,
                client_info: Some(ClientInfo {
                    name: "bevy_code_editor".into(),
                    version: Some(env!("CARGO_PKG_VERSION").into()),
                }),
                ..InitializeParams::default()
            };
            spawn_request!(
                server,
                bridge_tx,
                "initialize",
                move |s: &mut ServerSocket| s.initialize(params),
                |result| |tx: &UnboundedSender<_>| {
                    let _ = tx.send(LspResponse::Initialized {
                        capabilities: result.capabilities,
                    });
                }
            );
        }
        LspMessage::Initialized => {
            let mut server = server.clone();
            if let Err(err) = server.initialized(InitializedParams {}) {
                warn!("[LSP] initialized notification failed: {err}");
            }
        }
        LspMessage::DidOpen {
            uri,
            language_id,
            version,
            text,
        } => {
            let mut server = server.clone();
            let params = DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri,
                    language_id,
                    version,
                    text,
                },
            };
            if let Err(err) = server.did_open(params) {
                warn!("[LSP] did_open failed: {err}");
            }
        }
        LspMessage::DidChange {
            uri,
            version,
            changes,
        } => {
            let mut server = server.clone();
            let params = DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier { uri, version },
                content_changes: changes,
            };
            if let Err(err) = server.did_change(params) {
                warn!("[LSP] did_change failed: {err}");
            }
        }
        LspMessage::Completion { uri, position } => {
            let params = CompletionParams {
                text_document_position: text_pos(uri, position),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: None,
            };
            spawn_request!(
                server,
                bridge_tx,
                "completion",
                move |s: &mut ServerSocket| s.completion(params),
                |result| |tx: &UnboundedSender<_>| match result {
                    Some(CompletionResponse::Array(items)) => {
                        let _ = tx.send(LspResponse::Completion {
                            items,
                            is_incomplete: false,
                        });
                    }
                    Some(CompletionResponse::List(list)) => {
                        let _ = tx.send(LspResponse::Completion {
                            items: list.items,
                            is_incomplete: list.is_incomplete,
                        });
                    }
                    None => {}
                }
            );
        }
        LspMessage::Hover { uri, position } => {
            let params = HoverParams {
                text_document_position_params: text_pos(uri, position),
                work_done_progress_params: Default::default(),
            };
            spawn_request!(
                server,
                bridge_tx,
                "hover",
                move |s: &mut ServerSocket| s.hover(params),
                |result| |tx: &UnboundedSender<_>| {
                    if let Some(hover) = result {
                        let _ = tx.send(LspResponse::Hover {
                            content: extract_hover_content(&hover.contents),
                            range: hover.range,
                        });
                    }
                }
            );
        }
        LspMessage::GotoDefinition { uri, position } => {
            let params = GotoDefinitionParams {
                text_document_position_params: text_pos(uri, position),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn_request!(
                server,
                bridge_tx,
                "definition",
                move |s: &mut ServerSocket| s.definition(params),
                |result| |tx: &UnboundedSender<_>| {
                    let locations = match result {
                        Some(GotoDefinitionResponse::Scalar(loc)) => vec![loc],
                        Some(GotoDefinitionResponse::Array(locs)) => locs,
                        Some(GotoDefinitionResponse::Link(links)) => links
                            .into_iter()
                            .map(|link| Location {
                                uri: link.target_uri,
                                range: link.target_selection_range,
                            })
                            .collect(),
                        None => return,
                    };
                    if !locations.is_empty() {
                        let _ = tx.send(LspResponse::Definition { locations });
                    }
                }
            );
        }
        LspMessage::References { uri, position } => {
            let params = ReferenceParams {
                text_document_position: text_pos(uri, position),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: ReferenceContext {
                    include_declaration: true,
                },
            };
            spawn_request!(
                server,
                bridge_tx,
                "references",
                move |s: &mut ServerSocket| s.references(params),
                |result| |tx: &UnboundedSender<_>| {
                    if let Some(locations) = result {
                        let _ = tx.send(LspResponse::References { locations });
                    }
                }
            );
        }
        LspMessage::Format { uri, options } => {
            let params = DocumentFormattingParams {
                text_document: TextDocumentIdentifier { uri },
                options,
                work_done_progress_params: Default::default(),
            };
            spawn_request!(
                server,
                bridge_tx,
                "formatting",
                move |s: &mut ServerSocket| s.formatting(params),
                |result| |tx: &UnboundedSender<_>| {
                    if let Some(edits) = result {
                        let _ = tx.send(LspResponse::Format { edits });
                    }
                }
            );
        }
        LspMessage::SignatureHelp { uri, position } => {
            let params = SignatureHelpParams {
                text_document_position_params: text_pos(uri, position),
                work_done_progress_params: Default::default(),
                context: None,
            };
            spawn_request!(
                server,
                bridge_tx,
                "signature_help",
                move |s: &mut ServerSocket| s.signature_help(params),
                |result| |tx: &UnboundedSender<_>| {
                    if let Some(sig) = result {
                        let _ = tx.send(LspResponse::SignatureHelp {
                            signatures: sig.signatures,
                            active_signature: sig.active_signature,
                            active_parameter: sig.active_parameter,
                        });
                    }
                }
            );
        }
        LspMessage::CodeAction {
            uri,
            range,
            diagnostics,
        } => {
            let params = CodeActionParams {
                text_document: TextDocumentIdentifier { uri },
                range,
                context: CodeActionContext {
                    diagnostics,
                    only: None,
                    trigger_kind: None,
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn_request!(
                server,
                bridge_tx,
                "code_action",
                move |s: &mut ServerSocket| s.code_action(params),
                |result| |tx: &UnboundedSender<_>| {
                    if let Some(actions) = result {
                        let actions = actions
                            .into_iter()
                            .map(|a| match a {
                                lsp_types::CodeActionOrCommand::CodeAction(a) => {
                                    CodeActionOrCommand::Action(a)
                                }
                                lsp_types::CodeActionOrCommand::Command(c) => {
                                    CodeActionOrCommand::Command(c)
                                }
                            })
                            .collect();
                        let _ = tx.send(LspResponse::CodeActions { actions });
                    }
                }
            );
        }
        LspMessage::InlayHint { uri, range } => {
            let params = InlayHintParams {
                text_document: TextDocumentIdentifier { uri },
                range,
                work_done_progress_params: Default::default(),
            };
            spawn_request!(
                server,
                bridge_tx,
                "inlay_hint",
                move |s: &mut ServerSocket| s.inlay_hint(params),
                |result| |tx: &UnboundedSender<_>| {
                    if let Some(hints) = result {
                        let _ = tx.send(LspResponse::InlayHints { hints });
                    }
                }
            );
        }
        LspMessage::ExecuteCommand { command, arguments } => {
            let params = ExecuteCommandParams {
                command,
                arguments: arguments.unwrap_or_default(),
                work_done_progress_params: Default::default(),
            };
            // Reuse `LspResponse::CodeActions { actions: vec![] }` as the
            // round-trip ack; the editor adapter doesn't read the payload
            // for executeCommand, it only listens for the response.
            spawn_request!(
                server,
                bridge_tx,
                "execute_command",
                move |s: &mut ServerSocket| s.execute_command(params),
                |_result| |tx: &UnboundedSender<_>| {
                    let _ = tx.send(LspResponse::CodeActions { actions: vec![] });
                }
            );
        }
        LspMessage::DocumentHighlight { uri, position } => {
            let params = DocumentHighlightParams {
                text_document_position_params: text_pos(uri, position),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn_request!(
                server,
                bridge_tx,
                "document_highlight",
                move |s: &mut ServerSocket| s.document_highlight(params),
                |result| |tx: &UnboundedSender<_>| {
                    if let Some(highlights) = result {
                        let _ = tx.send(LspResponse::DocumentHighlights { highlights });
                    }
                }
            );
        }
        LspMessage::PrepareRename { uri, position } => {
            let params = text_pos(uri, position);
            spawn_request!(
                server,
                bridge_tx,
                "prepare_rename",
                move |s: &mut ServerSocket| s.prepare_rename(params),
                |result| |tx: &UnboundedSender<_>| match result {
                    Some(PrepareRenameResponse::Range(range)) => {
                        let _ = tx.send(LspResponse::PrepareRename {
                            range,
                            placeholder: None,
                        });
                    }
                    Some(PrepareRenameResponse::RangeWithPlaceholder { range, placeholder }) => {
                        let _ = tx.send(LspResponse::PrepareRename {
                            range,
                            placeholder: Some(placeholder),
                        });
                    }
                    // DefaultBehavior asks us to fall back to identifier-at-cursor,
                    // which the protocol layer doesn't have visibility into.
                    Some(PrepareRenameResponse::DefaultBehavior { .. }) | None => {}
                }
            );
        }
        LspMessage::Rename {
            uri,
            position,
            new_name,
        } => {
            let params = RenameParams {
                text_document_position: text_pos(uri, position),
                new_name,
                work_done_progress_params: Default::default(),
            };
            spawn_request!(
                server,
                bridge_tx,
                "rename",
                move |s: &mut ServerSocket| s.rename(params),
                |result| |tx: &UnboundedSender<_>| {
                    if let Some(edit) = result {
                        let _ = tx.send(LspResponse::Rename { edit });
                    }
                }
            );
        }
    }
}

/// Extract text content from `HoverContents`.
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
