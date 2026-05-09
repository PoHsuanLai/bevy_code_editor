//! LSP client transport. async-lsp on a shared tokio runtime, with an mpsc
//! bridge from the async side into ECS via [`LspClient::try_recv`].
//!
//! Coverage scope: every typed request and notification in
//! [`lsp_types::request`] / [`lsp_types::notification`] that the spec
//! defines is wired through here, regardless of whether the in-tree editor
//! actually consumes the response. The crate is the *protocol layer* for
//! Bevy applications; downstream UIs (editor, outline panel, agent
//! tooling) pick which responses they subscribe to.

use std::collections::HashMap;
use std::ops::ControlFlow;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_lsp::concurrency::ConcurrencyLayer;
use async_lsp::panic::CatchUnwindLayer;
use async_lsp::router::Router;
use async_lsp::tracing::TracingLayer;
use async_lsp::{ResponseError, ServerSocket};
use bevy::prelude::*;
use bevy_tokio_tasks::TokioTasksRuntime;
use lsp_types::notification::{
    Cancel as CancelNotif, DidChangeConfiguration, DidChangeTextDocument, DidChangeWatchedFiles,
    DidChangeWorkspaceFolders, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
    Exit as ExitNotif, Initialized as InitializedNotif, LogMessage, LogTrace,
    Notification as LspNotificationTrait, Progress, PublishDiagnostics, ShowMessage,
    TelemetryEvent, WillSaveTextDocument, WorkDoneProgressCancel,
};
use lsp_types::request::{
    ApplyWorkspaceEdit, CallHierarchyIncomingCalls, CallHierarchyOutgoingCalls,
    CallHierarchyPrepare, CodeActionRequest, CodeActionResolveRequest, CodeLensRefresh,
    ColorPresentationRequest, Completion, DocumentColor, DocumentDiagnosticRequest,
    DocumentHighlightRequest, DocumentLinkRequest, DocumentLinkResolve, DocumentSymbolRequest,
    ExecuteCommand, FoldingRangeRequest, Formatting, GotoDeclaration, GotoDefinition,
    GotoImplementation, GotoTypeDefinition, HoverRequest, Initialize as InitializeRequest,
    InlayHintRefreshRequest, InlayHintRequest, InlayHintResolveRequest, LinkedEditingRange,
    MonikerRequest, OnTypeFormatting, PrepareRenameRequest, RangeFormatting, References,
    RegisterCapability, Rename, Request as LspRequestTrait, ResolveCompletionItem,
    SelectionRangeRequest, SemanticTokensFullDeltaRequest, SemanticTokensFullRequest,
    SemanticTokensRangeRequest, SemanticTokensRefresh, ShowDocument, ShowMessageRequest,
    Shutdown as ShutdownRequest, SignatureHelpRequest, TypeHierarchyPrepare, TypeHierarchySubtypes,
    TypeHierarchySupertypes, UnregisterCapability, WillSaveWaitUntil, WorkDoneProgressCreate,
    WorkspaceConfiguration, WorkspaceDiagnosticRefresh, WorkspaceDiagnosticRequest,
    WorkspaceFoldersRequest, WorkspaceSymbolRequest, WorkspaceSymbolResolve,
};
use lsp_types::*;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tower::ServiceBuilder;

use super::messages::{CodeActionOrCommand, LspMessage, LspResponse, WorkspaceSymbolResponseItem};

/// API-parity constant. async-lsp doesn't enforce per-request deadlines;
/// servers handle long-running work via cancel/progress.
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

/// Holds suspended response slots for server-initiated requests. The router
/// stores a oneshot `Sender` keyed by `request_id` when it sees a request
/// from the server; the host's `Respond*` reply pulls the sender out and
/// fulfills it.
type ReplySlots<R> = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<R, ResponseError>>>>>;

#[derive(Default)]
struct InboundReplySlots {
    configuration: ReplySlots<Vec<serde_json::Value>>,
    apply_edit: ReplySlots<ApplyWorkspaceEditResponse>,
    show_message: ReplySlots<Option<MessageActionItem>>,
    show_document: ReplySlots<ShowDocumentResult>,
    work_done_progress_create: ReplySlots<()>,
    register_capability: ReplySlots<()>,
    unregister_capability: ReplySlots<()>,
    workspace_folders: ReplySlots<Option<Vec<WorkspaceFolder>>>,
}

/// Pair with [`crate::LspDocument`] and [`crate::ServerCapabilities`] on the
/// same entity to bind a server connection to a document.
#[derive(Component)]
pub struct LspClient {
    server: Option<ServerSocket>,
    response_tx: UnboundedSender<LspResponse>,
    response_rx: Mutex<UnboundedReceiver<LspResponse>>,
    /// Set by consumers on [`LspResponse::Initialized`].
    pub initialized: bool,
    mainloop_abort: Option<Arc<tokio::task::AbortHandle>>,
    runtime_handle: Option<tokio::runtime::Handle>,
    init_done: Arc<AtomicBool>,
    pre_init_queue: Arc<Mutex<Vec<LspMessage>>>,
    /// Set by `shutdown()` so the mainloop watchdog knows the channel
    /// closing isn't a crash.
    shutting_down: Arc<AtomicBool>,
    /// Monotonic counter for server-initiated request ids relayed onto
    /// the bus. Matches the corresponding `Respond*` outgoing variant.
    next_inbound_request_id: Arc<AtomicU64>,
    inbound_slots: Arc<InboundReplySlots>,
}

impl Default for LspClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LspClient {
    /// Construct a not-yet-started client; call [`LspClient::start`] to spawn
    /// the language server.
    pub fn new() -> Self {
        let (response_tx, response_rx) = unbounded_channel();
        Self {
            server: None,
            response_tx,
            response_rx: Mutex::new(response_rx),
            initialized: false,
            mainloop_abort: None,
            runtime_handle: None,
            init_done: Arc::new(AtomicBool::new(false)),
            pre_init_queue: Arc::new(Mutex::new(Vec::new())),
            shutting_down: Arc::new(AtomicBool::new(false)),
            next_inbound_request_id: Arc::new(AtomicU64::new(1)),
            inbound_slots: Arc::new(InboundReplySlots::default()),
        }
    }

    /// Spawn the language server and run its main loop on `runtime`. `Err` only
    /// on synchronous spawn failure (binary missing, permissions); async errors
    /// log via `warn!` and surface as the bridge channel going quiet.
    pub fn start(
        &mut self,
        runtime: &TokioTasksRuntime,
        command: &str,
        args: &[&str],
    ) -> std::io::Result<()> {
        #[cfg(debug_assertions)]
        debug!("[LSP] Starting server: {} {:?}", command, args);

        // tokio::process::Command::spawn needs an active reactor on the
        // current thread, but Bevy systems run outside any. Enter the runtime
        // for the spawn, then drop the guard.
        let handle = runtime.runtime().handle().clone();
        let mut child = {
            let _guard = handle.enter();
            tokio::process::Command::new(command)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()?
        };
        self.runtime_handle = Some(handle);

        let stdin = child.stdin.take().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "child stdin missing")
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "child stdout missing")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "child stderr missing")
        })?;

        let bridge_tx = self.response_tx.clone();
        let next_id = self.next_inbound_request_id.clone();
        let slots = self.inbound_slots.clone();
        let (mainloop, server) = async_lsp::MainLoop::new_client(move |_server| {
            let router = build_router(bridge_tx.clone(), next_id.clone(), slots.clone());
            ServiceBuilder::new()
                .layer(TracingLayer::default())
                .layer(CatchUnwindLayer::default())
                .layer(ConcurrencyLayer::default())
                .service(router)
        });

        self.server = Some(server);

        runtime.spawn_background_task(move |_ctx| async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                debug!("[LSP stderr] {}", line);
            }
        });

        // run_buffered wants futures::AsyncRead/Write; tokio's ChildStd* only
        // implement the tokio variants. Compat shim bridges them.
        let watchdog_tx = self.response_tx.clone();
        let watchdog_flag = self.shutting_down.clone();
        let join = runtime.spawn_background_task(move |_ctx| async move {
            let stdout = stdout.compat();
            let stdin = stdin.compat_write();
            let outcome = mainloop.run_buffered(stdout, stdin).await;
            let _ = child.wait().await;
            // Only treat mainloop exit as a crash if we weren't shutting down.
            if !watchdog_flag.load(Ordering::Acquire) {
                if let Err(err) = outcome {
                    warn!("[LSP] main loop exited unexpectedly: {err}");
                } else {
                    warn!("[LSP] main loop exited unexpectedly");
                }
                let _ = watchdog_tx.send(LspResponse::Crashed);
            }
        });

        self.mainloop_abort = Some(Arc::new(join.abort_handle()));

        Ok(())
    }

    pub fn started(&self) -> bool {
        self.server.is_some()
    }

    /// Responses arrive asynchronously via [`Self::try_recv`].
    pub fn send(&self, message: LspMessage) {
        let Some(server) = self.server.as_ref() else {
            #[cfg(debug_assertions)]
            debug!("[LSP] send() called before start(); dropping message");
            return;
        };
        let Some(handle) = self.runtime_handle.as_ref() else {
            #[cfg(debug_assertions)]
            debug!("[LSP] send() called before start(); no runtime handle");
            return;
        };

        match message {
            LspMessage::Initialize {
                root_uri,
                capabilities,
            } => {
                self.start_initialize(server.clone(), handle.clone(), root_uri, capabilities);
            }
            LspMessage::Initialized => {}
            // Shutdown / Exit must always go through, even before init is
            // done — the host may be exiting on a half-initialized server.
            other @ (LspMessage::Shutdown { .. } | LspMessage::Exit) => {
                dispatch(
                    server,
                    &self.response_tx,
                    handle,
                    &self.inbound_slots,
                    other,
                );
            }
            other if !self.init_done.load(Ordering::Acquire) => {
                self.pre_init_queue.lock().unwrap().push(other);
            }
            other => dispatch(
                server,
                &self.response_tx,
                handle,
                &self.inbound_slots,
                other,
            ),
        }
    }

    fn start_initialize(
        &self,
        server: ServerSocket,
        handle: tokio::runtime::Handle,
        root_uri: Url,
        capabilities: Box<ClientCapabilities>,
    ) {
        let tx = self.response_tx.clone();
        let init_done = self.init_done.clone();
        let queue = self.pre_init_queue.clone();
        let slots = self.inbound_slots.clone();
        handle.spawn(async move {
            #[allow(deprecated)]
            let params = InitializeParams {
                process_id: Some(std::process::id()),
                root_uri: Some(root_uri),
                capabilities: *capabilities,
                client_info: Some(ClientInfo {
                    name: "bevy_lsp".into(),
                    version: Some(env!("CARGO_PKG_VERSION").into()),
                }),
                ..InitializeParams::default()
            };
            match server.request::<InitializeRequest>(params).await {
                Ok(result) => {
                    if let Err(err) = server.notify::<InitializedNotif>(InitializedParams {}) {
                        warn!("[LSP] initialized notify failed: {err}");
                    }
                    init_done.store(true, Ordering::Release);
                    let drained: Vec<LspMessage> = std::mem::take(&mut *queue.lock().unwrap());
                    let h = tokio::runtime::Handle::current();
                    for msg in drained {
                        dispatch(&server, &tx, &h, &slots, msg);
                    }
                    emit(
                        &tx,
                        LspResponse::Initialized {
                            capabilities: Box::new(result.capabilities),
                        },
                    );
                }
                Err(err) => warn!("[LSP] {} failed: {err}", InitializeRequest::METHOD),
            }
        });
    }

    /// Drain one response from the bridge if available.
    pub fn try_recv(&self) -> Option<LspResponse> {
        if let Ok(mut rx) = self.response_rx.try_lock() {
            rx.try_recv().ok()
        } else {
            None
        }
    }

    /// No-op; API parity. async-lsp manages request lifetime.
    pub fn cleanup_timeouts(&self) {}

    pub fn is_ready(&self) -> bool {
        self.initialized
    }

    /// Send `Shutdown` then `Exit`; setting `shutting_down` first prevents the
    /// watchdog from reporting the channel close as `Crashed`. Usually wired
    /// from a `bevy::app::AppExit` observer.
    pub fn shutdown(&self) {
        if self.server.is_none() {
            return;
        }
        self.shutting_down.store(true, Ordering::Release);
        // id=0 because ShutdownAck handling is informational; async-lsp
        // delivers the response whenever it arrives.
        self.send(LspMessage::Shutdown { id: 0 });
        self.send(LspMessage::Exit);
    }

    /// `true` after the watchdog reports the mainloop closing without an
    /// explicit shutdown — host should drop and re-spawn the client.
    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::Acquire)
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // abort() + kill_on_drop(true) terminates the server process.
        if let Some(abort) = self.mainloop_abort.take() {
            abort.abort();
        }
    }
}

type Tx = UnboundedSender<LspResponse>;

/// Spawn a typed request and feed its result into `map` on success.
fn spawn<R>(
    server: &ServerSocket,
    tx: &Tx,
    params: R::Params,
    map: impl FnOnce(R::Result, &Tx) + Send + 'static,
) where
    R: LspRequestTrait + 'static,
    R::Params: Send + 'static,
    R::Result: Send + 'static,
{
    let server = server.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        match server.request::<R>(params).await {
            Ok(result) => map(result, &tx),
            Err(err) => log_request_error::<R>(err),
        }
    });
}

fn log_request_error<R: LspRequestTrait>(err: async_lsp::Error) {
    use async_lsp::{Error, ErrorCode};
    if let Error::Response(ref resp) = err {
        if resp.code == ErrorCode::CONTENT_MODIFIED || resp.code == ErrorCode::REQUEST_CANCELLED {
            debug!("[LSP] {} cancelled by server: {err}", R::METHOD);
            return;
        }
    }
    warn!("[LSP] {} failed: {err}", R::METHOD);
}

fn fire<N>(server: &ServerSocket, params: N::Params)
where
    N: LspNotificationTrait + 'static,
    N::Params: Send + 'static,
{
    if let Err(err) = server.notify::<N>(params) {
        warn!("[LSP] {} failed: {err}", N::METHOD);
    }
}

#[inline]
fn emit(tx: &Tx, r: LspResponse) {
    let _ = tx.send(r);
}

fn text_pos(uri: Url, position: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri },
        position,
    }
}

/// Build the async-lsp router that handles server→client traffic.
///
/// Notifications are forwarded as `LspResponse` notification variants.
/// Requests get a fresh inbound `request_id`, the `oneshot::Sender` is
/// stashed in `slots`, and the response variant goes onto the bridge — the
/// host then sends a matching `LspMessage::Respond*` which fulfills the
/// suspended sender.
fn build_router(tx: Tx, next_id: Arc<AtomicU64>, slots: Arc<InboundReplySlots>) -> Router<()> {
    let mut router: Router<()> = Router::new(());

    // ─── Notifications ─────────────────────────────────────────────────────

    let t = tx.clone();
    router.notification::<PublishDiagnostics>(move |_, params| {
        let _ = t.send(LspResponse::Diagnostics {
            uri: params.uri,
            diagnostics: params.diagnostics,
        });
        ControlFlow::Continue(())
    });

    let t = tx.clone();
    router.notification::<LogMessage>(move |_, params| {
        let _ = t.send(LspResponse::LogMessage {
            typ: params.typ,
            message: params.message,
        });
        ControlFlow::Continue(())
    });

    let t = tx.clone();
    router.notification::<ShowMessage>(move |_, params| {
        let _ = t.send(LspResponse::ShowMessage {
            typ: params.typ,
            message: params.message,
        });
        ControlFlow::Continue(())
    });

    let t = tx.clone();
    router.notification::<Progress>(move |_, params| {
        let _ = t.send(LspResponse::Progress {
            token: params.token,
            value: params.value,
        });
        ControlFlow::Continue(())
    });

    let t = tx.clone();
    router.notification::<TelemetryEvent>(move |_, value| {
        // Server sends either an object or an array — serialize back to
        // a generic `serde_json::Value` so consumers don't have to know
        // about lsp_types' OneOf alias.
        let data = match value {
            OneOf::Left(map) => serde_json::Value::Object(map),
            OneOf::Right(arr) => serde_json::Value::Array(arr),
        };
        let _ = t.send(LspResponse::Telemetry { data });
        ControlFlow::Continue(())
    });

    let t = tx.clone();
    router.notification::<LogTrace>(move |_, params| {
        let _ = t.send(LspResponse::LogTrace {
            message: params.message,
            verbose: params.verbose,
        });
        ControlFlow::Continue(())
    });

    // ─── Server requests requiring host reply ─────────────────────────────

    inbound_request::<WorkspaceConfiguration, _>(
        &mut router,
        next_id.clone(),
        slots.configuration.clone(),
        tx.clone(),
        |request_id, params| LspResponse::ConfigurationRequested {
            request_id,
            items: params.items,
        },
    );

    inbound_request::<ApplyWorkspaceEdit, _>(
        &mut router,
        next_id.clone(),
        slots.apply_edit.clone(),
        tx.clone(),
        |request_id, params| LspResponse::ApplyEditRequested {
            request_id,
            label: params.label,
            edit: params.edit,
        },
    );

    inbound_request::<ShowMessageRequest, _>(
        &mut router,
        next_id.clone(),
        slots.show_message.clone(),
        tx.clone(),
        |request_id, params| LspResponse::ShowMessageRequestRequested {
            request_id,
            typ: params.typ,
            message: params.message,
            actions: params.actions,
        },
    );

    inbound_request::<ShowDocument, _>(
        &mut router,
        next_id.clone(),
        slots.show_document.clone(),
        tx.clone(),
        |request_id, params| LspResponse::ShowDocumentRequested {
            request_id,
            uri: params.uri,
            external: params.external,
            take_focus: params.take_focus,
            selection: params.selection,
        },
    );

    inbound_request::<WorkDoneProgressCreate, _>(
        &mut router,
        next_id.clone(),
        slots.work_done_progress_create.clone(),
        tx.clone(),
        |request_id, params| LspResponse::WorkDoneProgressCreateRequested {
            request_id,
            token: params.token,
        },
    );

    inbound_request::<RegisterCapability, _>(
        &mut router,
        next_id.clone(),
        slots.register_capability.clone(),
        tx.clone(),
        |request_id, params| LspResponse::RegisterCapabilityRequested {
            request_id,
            registrations: params.registrations,
        },
    );

    inbound_request::<UnregisterCapability, _>(
        &mut router,
        next_id.clone(),
        slots.unregister_capability.clone(),
        tx.clone(),
        |request_id, params| LspResponse::UnregisterCapabilityRequested {
            request_id,
            unregistrations: params.unregisterations,
        },
    );

    inbound_request::<WorkspaceFoldersRequest, _>(
        &mut router,
        next_id,
        slots.workspace_folders.clone(),
        tx.clone(),
        |request_id, _params| LspResponse::WorkspaceFoldersRequested { request_id },
    );

    // ─── Refresh requests (no payload, () return) ────────────────────────

    let t = tx.clone();
    router.request::<SemanticTokensRefresh, _>(move |_, _params| {
        let _ = t.send(LspResponse::SemanticTokensRefreshRequested);
        async move { Ok(()) }
    });

    let t = tx.clone();
    router.request::<InlayHintRefreshRequest, _>(move |_, _params| {
        let _ = t.send(LspResponse::InlayHintRefreshRequested);
        async move { Ok(()) }
    });

    let t = tx.clone();
    router.request::<CodeLensRefresh, _>(move |_, _params| {
        let _ = t.send(LspResponse::CodeLensRefreshRequested);
        async move { Ok(()) }
    });

    let t = tx;
    router.request::<WorkspaceDiagnosticRefresh, _>(move |_, _params| {
        let _ = t.send(LspResponse::DiagnosticsRefreshRequested);
        async move { Ok(()) }
    });

    router
        .unhandled_notification(|_, _| ControlFlow::Continue(()))
        .unhandled_request(|_, _| async move {
            Err(ResponseError::new(
                async_lsp::ErrorCode::METHOD_NOT_FOUND,
                "request not handled by bevy_lsp",
            ))
        });

    router
}

/// Wire one server→client request type into the router. The handler:
/// 1. Mints a fresh `request_id`.
/// 2. Stashes a `oneshot::Sender` keyed by `request_id` in `slots`.
/// 3. Emits a `Requested` variant on the bridge for the host to see.
/// 4. Returns the future that awaits the sender.
fn inbound_request<R, F>(
    router: &mut Router<()>,
    next_id: Arc<AtomicU64>,
    slots: ReplySlots<R::Result>,
    tx: Tx,
    surface: F,
) where
    R: LspRequestTrait + 'static,
    R::Params: Send + 'static,
    R::Result: Send + 'static,
    F: Fn(u64, R::Params) -> LspResponse + Send + Sync + 'static,
{
    router.request::<R, _>(move |_, params| {
        let request_id = next_id.fetch_add(1, Ordering::Relaxed);
        let (resp_tx, resp_rx) = oneshot::channel();
        slots.lock().unwrap().insert(request_id, resp_tx);
        let _ = tx.send(surface(request_id, params));
        async move {
            match resp_rx.await {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(err)) => Err(err),
                Err(_) => Err(ResponseError::new(
                    async_lsp::ErrorCode::INTERNAL_ERROR,
                    "host dropped reply channel",
                )),
            }
        }
    });
}

fn fulfill_slot<T>(
    slots: &Mutex<HashMap<u64, oneshot::Sender<Result<T, ResponseError>>>>,
    id: u64,
    value: T,
) where
    T: 'static,
{
    if let Some(slot) = slots.lock().unwrap().remove(&id) {
        let _ = slot.send(Ok(value));
    } else {
        debug!("[LSP] respond for unknown inbound id {id}");
    }
}

fn dispatch(
    server: &ServerSocket,
    tx: &Tx,
    handle: &tokio::runtime::Handle,
    slots: &Arc<InboundReplySlots>,
    message: LspMessage,
) {
    let _guard = handle.enter();
    use LspMessage as M;
    match message {
        M::Initialize { .. } | M::Initialized => {}

        // ─── Cancellation ────────────────────────────────────────────────
        M::CancelRequest { id } => fire::<CancelNotif>(
            server,
            CancelParams {
                id: NumberOrString::Number(id as i32),
            },
        ),
        M::WorkDoneProgressCancel { token } => {
            fire::<WorkDoneProgressCancel>(server, WorkDoneProgressCancelParams { token })
        }

        // ─── Document sync ────────────────────────────────────────────────
        M::DidOpen {
            uri,
            language_id,
            version,
            text,
        } => {
            fire::<DidOpenTextDocument>(
                server,
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri,
                        language_id,
                        version,
                        text,
                    },
                },
            );
        }
        M::DidChange {
            uri,
            version,
            changes,
        } => fire::<DidChangeTextDocument>(
            server,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier { uri, version },
                content_changes: changes,
            },
        ),
        M::DidSave { uri, text } => fire::<DidSaveTextDocument>(
            server,
            DidSaveTextDocumentParams {
                text_document: TextDocumentIdentifier { uri },
                text,
            },
        ),
        M::DidClose { uri } => fire::<DidCloseTextDocument>(
            server,
            DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier { uri },
            },
        ),
        M::WillSave { uri, reason } => fire::<WillSaveTextDocument>(
            server,
            WillSaveTextDocumentParams {
                text_document: TextDocumentIdentifier { uri },
                reason,
            },
        ),
        M::WillSaveWaitUntil { uri, reason, id } => {
            let params = WillSaveTextDocumentParams {
                text_document: TextDocumentIdentifier { uri },
                reason,
            };
            spawn::<WillSaveWaitUntil>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::WillSaveWaitUntil {
                        id,
                        edits: result.unwrap_or_default(),
                    },
                );
            });
        }

        // ─── Workspace sync ───────────────────────────────────────────────
        M::DidChangeConfiguration { settings } => {
            fire::<DidChangeConfiguration>(server, DidChangeConfigurationParams { settings })
        }
        M::DidChangeWatchedFiles { changes } => {
            fire::<DidChangeWatchedFiles>(server, DidChangeWatchedFilesParams { changes })
        }
        M::DidChangeWorkspaceFolders { event } => {
            fire::<DidChangeWorkspaceFolders>(server, DidChangeWorkspaceFoldersParams { event })
        }

        // ─── Completion / hover / signature ──────────────────────────────
        M::Completion { uri, position, id } => completion(server, tx, uri, position, id),
        M::ResolveCompletionItem { item, id } => {
            spawn::<ResolveCompletionItem>(server, tx, *item, move |result, tx| {
                emit(
                    tx,
                    LspResponse::ResolvedCompletionItem {
                        id,
                        item: Box::new(result),
                    },
                )
            })
        }
        M::Hover { uri, position, id } => hover(server, tx, uri, position, id),
        M::SignatureHelp { uri, position, id } => signature_help(server, tx, uri, position, id),

        // ─── Navigation ───────────────────────────────────────────────────
        M::GotoDeclaration { uri, position, id } => {
            let params = GotoDefinitionParams {
                text_document_position_params: text_pos(uri, position),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<GotoDeclaration>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::Declaration {
                        id,
                        locations: flatten_decl(result),
                    },
                );
            });
        }
        M::GotoDefinition { uri, position, id } => {
            let params = GotoDefinitionParams {
                text_document_position_params: text_pos(uri, position),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<GotoDefinition>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::Definition {
                        id,
                        locations: flatten_def(result),
                    },
                );
            });
        }
        M::GotoTypeDefinition { uri, position, id } => {
            let params = GotoDefinitionParams {
                text_document_position_params: text_pos(uri, position),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<GotoTypeDefinition>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::TypeDefinition {
                        id,
                        locations: flatten_type_def(result),
                    },
                );
            });
        }
        M::GotoImplementation { uri, position, id } => {
            let params = GotoDefinitionParams {
                text_document_position_params: text_pos(uri, position),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<GotoImplementation>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::Implementation {
                        id,
                        locations: flatten_impl(result),
                    },
                );
            });
        }
        M::References { uri, position, id } => {
            let params = ReferenceParams {
                text_document_position: text_pos(uri, position),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: ReferenceContext {
                    include_declaration: true,
                },
            };
            spawn::<References>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::References {
                        id,
                        locations: result.unwrap_or_default(),
                    },
                );
            });
        }
        M::DocumentHighlight { uri, position, id } => {
            let params = DocumentHighlightParams {
                text_document_position_params: text_pos(uri, position),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<DocumentHighlightRequest>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::DocumentHighlights {
                        id,
                        highlights: result.unwrap_or_default(),
                    },
                );
            });
        }
        M::DocumentSymbol { uri, id } => {
            let params = DocumentSymbolParams {
                text_document: TextDocumentIdentifier { uri },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<DocumentSymbolRequest>(server, tx, params, move |result, tx| {
                let (flat, nested) = match result {
                    Some(DocumentSymbolResponse::Flat(items)) => (items, Vec::new()),
                    Some(DocumentSymbolResponse::Nested(items)) => (Vec::new(), items),
                    None => (Vec::new(), Vec::new()),
                };
                emit(tx, LspResponse::DocumentSymbols { id, flat, nested });
            });
        }
        M::WorkspaceSymbol { query, id } => {
            let params = WorkspaceSymbolParams {
                query,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<WorkspaceSymbolRequest>(server, tx, params, move |result, tx| {
                let symbols = match result {
                    Some(WorkspaceSymbolResponse::Flat(items)) => items
                        .into_iter()
                        .map(WorkspaceSymbolResponseItem::Information)
                        .collect(),
                    Some(WorkspaceSymbolResponse::Nested(items)) => items
                        .into_iter()
                        .map(WorkspaceSymbolResponseItem::Symbol)
                        .collect(),
                    None => Vec::new(),
                };
                emit(tx, LspResponse::WorkspaceSymbols { id, symbols });
            });
        }
        M::WorkspaceSymbolResolve { symbol, id } => {
            spawn::<WorkspaceSymbolResolve>(server, tx, symbol, move |result, tx| {
                emit(
                    tx,
                    LspResponse::ResolvedWorkspaceSymbol { id, symbol: result },
                )
            })
        }

        // ─── Folding / selection ──────────────────────────────────────────
        M::FoldingRange { uri, id } => {
            let params = FoldingRangeParams {
                text_document: TextDocumentIdentifier { uri },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<FoldingRangeRequest>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::FoldingRanges {
                        id,
                        ranges: result.unwrap_or_default(),
                    },
                );
            });
        }
        M::SelectionRange { uri, positions, id } => {
            let params = SelectionRangeParams {
                text_document: TextDocumentIdentifier { uri },
                positions,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<SelectionRangeRequest>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::SelectionRanges {
                        id,
                        ranges: result.unwrap_or_default(),
                    },
                );
            });
        }

        // ─── Code actions / formatting ────────────────────────────────────
        M::CodeAction {
            uri,
            range,
            diagnostics,
            id,
        } => code_action(server, tx, uri, range, diagnostics, id),
        M::CodeActionResolve { action, id } => {
            spawn::<CodeActionResolveRequest>(server, tx, *action, move |result, tx| {
                emit(
                    tx,
                    LspResponse::ResolvedCodeAction {
                        id,
                        action: Box::new(result),
                    },
                )
            })
        }
        M::Format { uri, options, id } => {
            let params = DocumentFormattingParams {
                text_document: TextDocumentIdentifier { uri },
                options,
                work_done_progress_params: Default::default(),
            };
            spawn::<Formatting>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::Format {
                        id,
                        edits: result.unwrap_or_default(),
                    },
                );
            });
        }
        M::RangeFormatting {
            uri,
            range,
            options,
            id,
        } => {
            let params = DocumentRangeFormattingParams {
                text_document: TextDocumentIdentifier { uri },
                range,
                options,
                work_done_progress_params: Default::default(),
            };
            spawn::<RangeFormatting>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::RangeFormatting {
                        id,
                        edits: result.unwrap_or_default(),
                    },
                );
            });
        }
        M::OnTypeFormatting {
            uri,
            position,
            ch,
            options,
            id,
        } => {
            let params = DocumentOnTypeFormattingParams {
                text_document_position: text_pos(uri, position),
                ch,
                options,
            };
            spawn::<OnTypeFormatting>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::OnTypeFormatting {
                        id,
                        edits: result.unwrap_or_default(),
                    },
                );
            });
        }
        M::ExecuteCommand { command, arguments } => execute_command(server, tx, command, arguments),

        // ─── Inlay hints / decorative ─────────────────────────────────────
        M::InlayHint { uri, range, id } => {
            let params = InlayHintParams {
                text_document: TextDocumentIdentifier { uri },
                range,
                work_done_progress_params: Default::default(),
            };
            spawn::<InlayHintRequest>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::InlayHints {
                        id,
                        hints: result.unwrap_or_default(),
                    },
                );
            });
        }
        M::InlayHintResolve { hint, id } => {
            spawn::<InlayHintResolveRequest>(server, tx, hint, move |result, tx| {
                emit(tx, LspResponse::ResolvedInlayHint { id, hint: result })
            })
        }
        M::DocumentLink { uri, id } => {
            let params = DocumentLinkParams {
                text_document: TextDocumentIdentifier { uri },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<DocumentLinkRequest>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::DocumentLinks {
                        id,
                        links: result.unwrap_or_default(),
                    },
                );
            });
        }
        M::DocumentLinkResolve { link, id } => {
            spawn::<DocumentLinkResolve>(server, tx, link, move |result, tx| {
                emit(tx, LspResponse::ResolvedDocumentLink { id, link: result })
            })
        }
        M::DocumentColor { uri, id } => {
            let params = DocumentColorParams {
                text_document: TextDocumentIdentifier { uri },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<DocumentColor>(server, tx, params, move |result, tx| {
                emit(tx, LspResponse::DocumentColors { id, colors: result });
            });
        }
        M::ColorPresentation {
            uri,
            color,
            range,
            id,
        } => {
            let params = ColorPresentationParams {
                text_document: TextDocumentIdentifier { uri },
                color,
                range,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<ColorPresentationRequest>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::ColorPresentations {
                        id,
                        presentations: result,
                    },
                );
            });
        }
        M::LinkedEditingRange { uri, position, id } => {
            let params = LinkedEditingRangeParams {
                text_document_position_params: text_pos(uri, position),
                work_done_progress_params: Default::default(),
            };
            spawn::<LinkedEditingRange>(server, tx, params, move |result, tx| {
                emit(tx, LspResponse::LinkedEditingRanges { id, ranges: result });
            });
        }
        M::Moniker { uri, position, id } => {
            let params = MonikerParams {
                text_document_position_params: text_pos(uri, position),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<MonikerRequest>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::Monikers {
                        id,
                        monikers: result.unwrap_or_default(),
                    },
                );
            });
        }

        // ─── Rename ───────────────────────────────────────────────────────
        M::PrepareRename { uri, position, id } => prepare_rename(server, tx, uri, position, id),
        M::Rename {
            uri,
            position,
            new_name,
            id,
        } => {
            let params = RenameParams {
                text_document_position: text_pos(uri, position),
                new_name,
                work_done_progress_params: Default::default(),
            };
            spawn::<Rename>(server, tx, params, move |result, tx| {
                if let Some(edit) = result {
                    emit(tx, LspResponse::Rename { id, edit });
                }
            });
        }

        // ─── Call hierarchy ───────────────────────────────────────────────
        M::PrepareCallHierarchy { uri, position, id } => {
            let params = CallHierarchyPrepareParams {
                text_document_position_params: text_pos(uri, position),
                work_done_progress_params: Default::default(),
            };
            spawn::<CallHierarchyPrepare>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::PrepareCallHierarchy {
                        id,
                        items: result.unwrap_or_default(),
                    },
                );
            });
        }
        M::CallHierarchyIncomingCalls { item, id } => {
            let params = CallHierarchyIncomingCallsParams {
                item,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<CallHierarchyIncomingCalls>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::CallHierarchyIncomingCalls {
                        id,
                        calls: result.unwrap_or_default(),
                    },
                );
            });
        }
        M::CallHierarchyOutgoingCalls { item, id } => {
            let params = CallHierarchyOutgoingCallsParams {
                item,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<CallHierarchyOutgoingCalls>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::CallHierarchyOutgoingCalls {
                        id,
                        calls: result.unwrap_or_default(),
                    },
                );
            });
        }

        // ─── Type hierarchy ──────────────────────────────────────────────
        M::PrepareTypeHierarchy { uri, position, id } => {
            let params = TypeHierarchyPrepareParams {
                text_document_position_params: text_pos(uri, position),
                work_done_progress_params: Default::default(),
            };
            spawn::<TypeHierarchyPrepare>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::PrepareTypeHierarchy {
                        id,
                        items: result.unwrap_or_default(),
                    },
                );
            });
        }
        M::TypeHierarchySupertypes { item, id } => {
            let params = TypeHierarchySupertypesParams {
                item,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<TypeHierarchySupertypes>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::TypeHierarchySupertypes {
                        id,
                        items: result.unwrap_or_default(),
                    },
                );
            });
        }
        M::TypeHierarchySubtypes { item, id } => {
            let params = TypeHierarchySubtypesParams {
                item,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<TypeHierarchySubtypes>(server, tx, params, move |result, tx| {
                emit(
                    tx,
                    LspResponse::TypeHierarchySubtypes {
                        id,
                        items: result.unwrap_or_default(),
                    },
                );
            });
        }

        // ─── Semantic tokens ──────────────────────────────────────────────
        M::SemanticTokensFull { uri, id } => {
            let params = SemanticTokensParams {
                text_document: TextDocumentIdentifier { uri },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<SemanticTokensFullRequest>(server, tx, params, move |result, tx| {
                if let Some(result) = result {
                    emit(tx, LspResponse::SemanticTokens { id, result });
                }
            });
        }
        M::SemanticTokensFullDelta {
            uri,
            previous_result_id,
            id,
        } => {
            let params = SemanticTokensDeltaParams {
                text_document: TextDocumentIdentifier { uri },
                previous_result_id,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<SemanticTokensFullDeltaRequest>(server, tx, params, move |result, tx| {
                if let Some(result) = result {
                    emit(tx, LspResponse::SemanticTokensDelta { id, result });
                }
            });
        }
        M::SemanticTokensRange { uri, range, id } => {
            let params = SemanticTokensRangeParams {
                text_document: TextDocumentIdentifier { uri },
                range,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<SemanticTokensRangeRequest>(server, tx, params, move |result, tx| {
                if let Some(result) = result {
                    emit(tx, LspResponse::SemanticTokensRange { id, result });
                }
            });
        }

        // ─── Pull diagnostics ────────────────────────────────────────────
        M::DocumentDiagnostic {
            uri,
            identifier,
            previous_result_id,
            id,
        } => {
            let params = DocumentDiagnosticParams {
                text_document: TextDocumentIdentifier { uri },
                identifier,
                previous_result_id,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<DocumentDiagnosticRequest>(server, tx, params, move |result, tx| {
                emit(tx, LspResponse::DocumentDiagnostic { id, report: result });
            });
        }
        M::WorkspaceDiagnostic {
            identifier,
            previous_result_ids,
            id,
        } => {
            let params = WorkspaceDiagnosticParams {
                identifier,
                previous_result_ids,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            spawn::<WorkspaceDiagnosticRequest>(server, tx, params, move |result, tx| {
                emit(tx, LspResponse::WorkspaceDiagnostic { id, report: result });
            });
        }

        // ─── Server-pull responses ───────────────────────────────────────
        M::RespondConfiguration { id, items } => fulfill_slot(&slots.configuration, id, items),
        M::RespondApplyEdit { id, response } => fulfill_slot(&slots.apply_edit, id, response),
        M::RespondShowMessageRequest { id, action } => {
            fulfill_slot(&slots.show_message, id, action)
        }
        M::RespondShowDocument { id, success } => {
            fulfill_slot(&slots.show_document, id, ShowDocumentResult { success })
        }
        M::RespondWorkDoneProgressCreate { id } => {
            fulfill_slot(&slots.work_done_progress_create, id, ())
        }
        M::RespondRegisterCapability { id } => fulfill_slot(&slots.register_capability, id, ()),
        M::RespondUnregisterCapability { id } => fulfill_slot(&slots.unregister_capability, id, ()),
        M::RespondWorkspaceFolders { id, folders } => {
            fulfill_slot(&slots.workspace_folders, id, folders)
        }

        // ─── Termination ──────────────────────────────────────────────────
        M::Shutdown { id } => spawn::<ShutdownRequest>(server, tx, (), move |_result, tx| {
            emit(tx, LspResponse::ShutdownAck { id });
        }),
        M::Exit => fire::<ExitNotif>(server, ()),
    }
}

fn completion(server: &ServerSocket, tx: &Tx, uri: Url, position: Position, id: u64) {
    let params = CompletionParams {
        text_document_position: text_pos(uri, position),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    };
    spawn::<Completion>(server, tx, params, move |result, tx| match result {
        Some(CompletionResponse::Array(items)) => {
            emit(
                tx,
                LspResponse::Completion {
                    id,
                    items,
                    is_incomplete: false,
                },
            );
        }
        Some(CompletionResponse::List(list)) => {
            emit(
                tx,
                LspResponse::Completion {
                    id,
                    items: list.items,
                    is_incomplete: list.is_incomplete,
                },
            );
        }
        None => {
            emit(
                tx,
                LspResponse::Completion {
                    id,
                    items: Vec::new(),
                    is_incomplete: false,
                },
            );
        }
    });
}

fn hover(server: &ServerSocket, tx: &Tx, uri: Url, position: Position, id: u64) {
    let params = HoverParams {
        text_document_position_params: text_pos(uri, position),
        work_done_progress_params: Default::default(),
    };
    spawn::<HoverRequest>(server, tx, params, move |result, tx| {
        if let Some(h) = result {
            let (content, kind) = extract_hover_content(&h.contents);
            emit(
                tx,
                LspResponse::Hover {
                    id,
                    content,
                    kind,
                    range: h.range,
                },
            );
        }
    });
}

fn signature_help(server: &ServerSocket, tx: &Tx, uri: Url, position: Position, id: u64) {
    let params = SignatureHelpParams {
        text_document_position_params: text_pos(uri, position),
        work_done_progress_params: Default::default(),
        context: None,
    };
    spawn::<SignatureHelpRequest>(server, tx, params, move |result, tx| {
        let (signatures, active_signature, active_parameter) = match result {
            Some(sig) => (sig.signatures, sig.active_signature, sig.active_parameter),
            None => (Vec::new(), None, None),
        };
        emit(
            tx,
            LspResponse::SignatureHelp {
                id,
                signatures,
                active_signature,
                active_parameter,
            },
        );
    });
}

fn code_action(
    server: &ServerSocket,
    tx: &Tx,
    uri: Url,
    range: Range,
    diagnostics: Vec<Diagnostic>,
    id: u64,
) {
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
    spawn::<CodeActionRequest>(server, tx, params, move |result, tx| {
        let actions = result
            .unwrap_or_default()
            .into_iter()
            .map(|a| match a {
                lsp_types::CodeActionOrCommand::CodeAction(a) => {
                    CodeActionOrCommand::Action(Box::new(a))
                }
                lsp_types::CodeActionOrCommand::Command(c) => CodeActionOrCommand::Command(c),
            })
            .collect();
        emit(tx, LspResponse::CodeActions { id, actions });
    });
}

fn execute_command(
    server: &ServerSocket,
    tx: &Tx,
    command: String,
    arguments: Option<Vec<serde_json::Value>>,
) {
    let params = ExecuteCommandParams {
        command,
        arguments: arguments.unwrap_or_default(),
        work_done_progress_params: Default::default(),
    };
    // Fire-and-forget. If the command produces edits, the server emits them
    // via workspace/applyEdit, which the inbound router relays.
    spawn::<ExecuteCommand>(server, tx, params, |_result, _tx| {});
}

fn prepare_rename(server: &ServerSocket, tx: &Tx, uri: Url, position: Position, id: u64) {
    spawn::<PrepareRenameRequest>(server, tx, text_pos(uri, position), move |result, tx| {
        match result {
            Some(PrepareRenameResponse::Range(range)) => {
                emit(
                    tx,
                    LspResponse::PrepareRename {
                        id,
                        range,
                        placeholder: None,
                    },
                );
            }
            Some(PrepareRenameResponse::RangeWithPlaceholder { range, placeholder }) => {
                emit(
                    tx,
                    LspResponse::PrepareRename {
                        id,
                        range,
                        placeholder: Some(placeholder),
                    },
                );
            }
            // DefaultBehavior wants an identifier-at-cursor fallback, which the
            // protocol layer can't compute.
            Some(PrepareRenameResponse::DefaultBehavior { .. }) | None => {}
        }
    });
}

fn flatten_def(r: Option<GotoDefinitionResponse>) -> Vec<Location> {
    match r {
        Some(GotoDefinitionResponse::Scalar(loc)) => vec![loc],
        Some(GotoDefinitionResponse::Array(locs)) => locs,
        Some(GotoDefinitionResponse::Link(links)) => links
            .into_iter()
            .map(|link| Location {
                uri: link.target_uri,
                range: link.target_selection_range,
            })
            .collect(),
        None => Vec::new(),
    }
}

// Declaration / type-definition / implementation all alias to
// `GotoDefinitionResponse` in lsp_types — single helper handles all four.
fn flatten_decl(r: Option<GotoDefinitionResponse>) -> Vec<Location> {
    flatten_def(r)
}
fn flatten_type_def(r: Option<GotoDefinitionResponse>) -> Vec<Location> {
    flatten_def(r)
}
fn flatten_impl(r: Option<GotoDefinitionResponse>) -> Vec<Location> {
    flatten_def(r)
}

/// Flatten LSP `HoverContents` into a `(text, kind)` pair. An `Array` mixing
/// `LanguageString` with plain text renders as Markdown so the renderer can
/// treat fenced blocks uniformly.
fn extract_hover_content(contents: &HoverContents) -> (String, MarkupKind) {
    match contents {
        HoverContents::Markup(markup) => (markup.value.clone(), markup.kind.clone()),
        HoverContents::Scalar(s) => marked_string_with_kind(s),
        HoverContents::Array(arr) => {
            let any_lang = arr
                .iter()
                .any(|ms| matches!(ms, MarkedString::LanguageString(_)));
            let mut out = String::new();
            for (i, ms) in arr.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                }
                out.push_str(&render_marked_string(ms, any_lang));
            }
            let kind = if any_lang {
                MarkupKind::Markdown
            } else {
                MarkupKind::PlainText
            };
            (out, kind)
        }
    }
}

fn marked_string_with_kind(ms: &MarkedString) -> (String, MarkupKind) {
    match ms {
        MarkedString::String(s) => (s.clone(), MarkupKind::PlainText),
        MarkedString::LanguageString(ls) => (
            format!("```{}\n{}\n```", ls.language, ls.value),
            MarkupKind::Markdown,
        ),
    }
}

fn render_marked_string(ms: &MarkedString, force_markdown: bool) -> String {
    match ms {
        MarkedString::String(s) => s.clone(),
        MarkedString::LanguageString(ls) => {
            if force_markdown {
                format!("```{}\n{}\n```", ls.language, ls.value)
            } else {
                ls.value.clone()
            }
        }
    }
}
