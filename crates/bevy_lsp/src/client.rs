//! LSP client transport. async-lsp over Bevy's smol-based AsyncComputeTaskPool,
//! with an async-channel bridge from the async side into ECS via
//! [`LspClient::try_recv`].

use std::collections::HashMap;
use std::future::Future;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use async_lsp::concurrency::ConcurrencyLayer;
use async_lsp::panic::CatchUnwindLayer;
use async_lsp::router::Router;
use async_lsp::tracing::TracingLayer;
use async_lsp::{ResponseError, ServerSocket};
use bevy_ecs::prelude::*;
use bevy_log::{debug, info, warn};
use bevy_tasks::{AsyncComputeTaskPool, Task};
use futures::channel::oneshot;

#[cfg(not(target_arch = "wasm32"))]
use crate::transport::StdioTransport;
use crate::transport::{LspTransport, MaybeSend};

/// Spawn an async task on Bevy's compute pool. Uses `spawn_local` on wasm32
/// (single-threaded, where many in-browser handles such as `WebSocket` are
/// `!Send`) and the thread-pooled `spawn` everywhere else.
fn spawn_task<Fut>(future: Fut) -> Task<()>
where
    Fut: Future<Output = ()> + MaybeSend + 'static,
{
    let pool = AsyncComputeTaskPool::get();
    #[cfg(not(target_arch = "wasm32"))]
    {
        pool.spawn(future)
    }
    #[cfg(target_arch = "wasm32")]
    {
        pool.spawn_local(future)
    }
}
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
use tower::ServiceBuilder;

use super::messages::{CodeActionOrCommand, LspMessage, LspResponse, WorkspaceSymbolResponseItem};

pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

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
    response_tx: async_channel::Sender<LspResponse>,
    response_rx: async_channel::Receiver<LspResponse>,
    pub(crate) initialized: bool, // set by plugin.rs on LspServerInitialized
    // Holds the background tasks alive. Dropped on shutdown/drop.
    _tasks: Vec<Task<()>>,
    // Messages queued before initialize completes. Drained by plugin.rs on
    // the frame it processes LspServerInitialized — no Arc<Mutex> needed.
    pub(crate) pre_init_queue: Vec<LspMessage>,
    init_done: Arc<AtomicBool>,
    shutting_down: Arc<AtomicBool>,
    next_inbound_request_id: Arc<AtomicU64>,
    inbound_slots: Arc<InboundReplySlots>,
    /// Populated by the connect task once the transport reports a PID. `None`
    /// when the transport has no concept of a local PID (WebSocket, in-browser
    /// worker). Surfaced in [`InitializeParams::process_id`].
    client_process_id: Arc<OnceLock<Option<u32>>>,
}

impl Default for LspClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LspClient {
    pub fn new() -> Self {
        let (response_tx, response_rx) = async_channel::unbounded();
        Self {
            server: None,
            response_tx,
            response_rx,
            initialized: false,
            _tasks: Vec::new(),
            pre_init_queue: Vec::new(),
            init_done: Arc::new(AtomicBool::new(false)),
            shutting_down: Arc::new(AtomicBool::new(false)),
            next_inbound_request_id: Arc::new(AtomicU64::new(1)),
            inbound_slots: Arc::new(InboundReplySlots::default()),
            client_process_id: Arc::new(OnceLock::new()),
        }
    }

    /// Spawn the language server over a stdio subprocess. Convenience for the
    /// native default; equivalent to [`Self::start_with`] with a
    /// [`StdioTransport`]. `Err` only on synchronous setup failure (e.g.
    /// caller asked for an empty command). Connect / async errors surface as
    /// [`LspResponse::Crashed`].
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start(&mut self, command: &str, args: &[&str]) -> std::io::Result<()> {
        self.start_with(StdioTransport::new(command, args.iter().copied()));
        Ok(())
    }

    /// Spawn the language server using `transport`. The async [`LspTransport::connect`]
    /// call runs on Bevy's task pool; any transport-side failure (failed spawn,
    /// failed WebSocket handshake, premature exit) is reported by emitting
    /// [`LspResponse::Crashed`] on this client's response channel.
    pub fn start_with<T: LspTransport>(&mut self, transport: T) {
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

        let watchdog_tx = self.response_tx.clone();
        let watchdog_flag = self.shutting_down.clone();
        let pid_slot = self.client_process_id.clone();

        info!("[LSP] start_with: scheduling driver task on AsyncComputeTaskPool");
        let driver = spawn_task(async move {
            info!("[LSP] driver task started; calling transport.connect()");
            let (reader, writer, handle) = match transport.connect().await {
                Ok(t) => {
                    info!("[LSP] transport.connect() succeeded");
                    t
                }
                Err(err) => {
                    warn!("[LSP] transport connect failed: {err}");
                    let _ = watchdog_tx.try_send(LspResponse::Crashed);
                    return;
                }
            };
            let _ = pid_slot.set(handle.client_process_id);
            info!(
                "[LSP] transport pid={:?}; entering MainLoop::run_buffered",
                handle.client_process_id,
            );
            let _aux = handle.auxiliary_tasks;

            let outcome = mainloop.run_buffered(reader, writer).await;
            info!("[LSP] MainLoop::run_buffered returned outcome={:?}", outcome.as_ref().map(|_| ()).map_err(|e| e.to_string()));
            handle.exited.await;
            info!("[LSP] transport handle exited");

            if !watchdog_flag.load(Ordering::Acquire) {
                if let Err(err) = outcome {
                    warn!("[LSP] main loop exited unexpectedly: {err}");
                } else {
                    warn!("[LSP] main loop exited unexpectedly");
                }
                let _ = watchdog_tx.try_send(LspResponse::Crashed);
            }
        });

        self._tasks.push(driver);
    }

    pub fn started(&self) -> bool {
        self.server.is_some()
    }

    /// Queue or dispatch a message. Responses arrive via [`Self::try_recv`].
    pub fn send(&mut self, message: LspMessage) {
        let Some(server) = self.server.as_ref() else {
            #[cfg(debug_assertions)]
            debug!("[LSP] send() called before start(); dropping message");
            return;
        };

        match message {
            LspMessage::Initialize {
                root_uri,
                capabilities,
            } => {
                self.start_initialize(server.clone(), root_uri, capabilities);
            }
            LspMessage::Initialized => {}
            other @ (LspMessage::Shutdown { .. } | LspMessage::Exit) => {
                dispatch(server, &self.response_tx, &self.inbound_slots, other);
            }
            other if !self.init_done.load(Ordering::Acquire) => {
                self.pre_init_queue.push(other);
            }
            other => dispatch(server, &self.response_tx, &self.inbound_slots, other),
        }
    }

    fn start_initialize(
        &self,
        server: ServerSocket,
        root_uri: Url,
        capabilities: Box<ClientCapabilities>,
    ) {
        let tx = self.response_tx.clone();
        let init_done = self.init_done.clone();
        let pid = self.client_process_id.get().and_then(|p| *p);
        spawn_task(async move {
            #[allow(deprecated)]
            let params = InitializeParams {
                process_id: pid,
                root_uri: Some(root_uri),
                capabilities: *capabilities,
                client_info: Some(ClientInfo {
                    name: "bevy_lsp".into(),
                    version: Some(env!("CARGO_PKG_VERSION").into()),
                }),
                ..InitializeParams::default()
            };
            info!("[LSP] sending initialize request");
            match server.request::<InitializeRequest>(params).await {
                Ok(result) => {
                    info!("[LSP] initialize response received");
                    if let Err(err) = server.notify::<InitializedNotif>(InitializedParams {}) {
                        warn!("[LSP] initialized notify failed: {err}");
                    }
                    init_done.store(true, Ordering::Release);
                    emit(
                        &tx,
                        LspResponse::Initialized {
                            capabilities: Box::new(result.capabilities),
                        },
                    );
                }
                Err(err) => warn!("[LSP] {} failed: {err}", InitializeRequest::METHOD),
            }
        })
        .detach();
    }

    pub fn try_recv(&self) -> Option<LspResponse> {
        self.response_rx.try_recv().ok()
    }

    pub fn cleanup_timeouts(&self) {}

    pub fn is_ready(&self) -> bool {
        self.initialized
    }

    pub fn shutdown(&mut self) {
        if self.server.is_none() {
            return;
        }
        self.shutting_down.store(true, Ordering::Release);
        self.send(LspMessage::Shutdown { id: 0 });
        self.send(LspMessage::Exit);
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::Acquire)
    }
}

type Tx = async_channel::Sender<LspResponse>;

fn spawn<R>(
    server: &ServerSocket,
    tx: &Tx,
    params: R::Params,
    map: impl FnOnce(R::Result, &Tx) + MaybeSend + 'static,
) where
    R: LspRequestTrait + 'static,
    R::Params: MaybeSend + 'static,
    R::Result: MaybeSend + 'static,
{
    let server = server.clone();
    let tx = tx.clone();
    spawn_task(async move {
        match server.request::<R>(params).await {
            Ok(result) => map(result, &tx),
            Err(err) => log_request_error::<R>(err),
        }
    })
    .detach();
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
    let _ = tx.try_send(r);
}

fn text_pos(uri: Url, position: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri },
        position,
    }
}

fn build_router(tx: Tx, next_id: Arc<AtomicU64>, slots: Arc<InboundReplySlots>) -> Router<()> {
    let mut router: Router<()> = Router::new(());

    // ─── Notifications ──────────────────────────────────────────────────────

    let t = tx.clone();
    router.notification::<PublishDiagnostics>(move |_, params| {
        info!(
            "[LSP] publishDiagnostics uri={} version={:?} count={}",
            params.uri,
            params.version,
            params.diagnostics.len(),
        );
        for (i, d) in params.diagnostics.iter().enumerate() {
            info!(
                "[LSP]   raw diag[{}] line={} col={}..{} severity={:?} src={:?} code={:?} msg={:?}",
                i,
                d.range.start.line,
                d.range.start.character,
                d.range.end.character,
                d.severity,
                d.source,
                d.code,
                d.message,
            );
        }
        let _ = t.try_send(LspResponse::Diagnostics {
            uri: params.uri,
            version: params.version,
            diagnostics: params.diagnostics,
        });
        ControlFlow::Continue(())
    });

    let t = tx.clone();
    router.notification::<LogMessage>(move |_, params| {
        let _ = t.try_send(LspResponse::LogMessage {
            typ: params.typ,
            message: params.message,
        });
        ControlFlow::Continue(())
    });

    let t = tx.clone();
    router.notification::<ShowMessage>(move |_, params| {
        let _ = t.try_send(LspResponse::ShowMessage {
            typ: params.typ,
            message: params.message,
        });
        ControlFlow::Continue(())
    });

    let t = tx.clone();
    router.notification::<Progress>(move |_, params| {
        let _ = t.try_send(LspResponse::Progress {
            token: params.token,
            value: params.value,
        });
        ControlFlow::Continue(())
    });

    let t = tx.clone();
    router.notification::<TelemetryEvent>(move |_, value| {
        let data = match value {
            OneOf::Left(map) => serde_json::Value::Object(map),
            OneOf::Right(arr) => serde_json::Value::Array(arr),
        };
        let _ = t.try_send(LspResponse::Telemetry { data });
        ControlFlow::Continue(())
    });

    let t = tx.clone();
    router.notification::<LogTrace>(move |_, params| {
        let _ = t.try_send(LspResponse::LogTrace {
            message: params.message,
            verbose: params.verbose,
        });
        ControlFlow::Continue(())
    });

    // ─── Server requests requiring host reply ────────────────────────────────

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

    // ─── Refresh requests (no payload, () return) ────────────────────────────

    let t = tx.clone();
    router.request::<SemanticTokensRefresh, _>(move |_, _params| {
        let _ = t.try_send(LspResponse::SemanticTokensRefreshRequested);
        async move { Ok(()) }
    });

    let t = tx.clone();
    router.request::<InlayHintRefreshRequest, _>(move |_, _params| {
        let _ = t.try_send(LspResponse::InlayHintRefreshRequested);
        async move { Ok(()) }
    });

    let t = tx.clone();
    router.request::<CodeLensRefresh, _>(move |_, _params| {
        let _ = t.try_send(LspResponse::CodeLensRefreshRequested);
        async move { Ok(()) }
    });

    let t = tx;
    router.request::<WorkspaceDiagnosticRefresh, _>(move |_, _params| {
        let _ = t.try_send(LspResponse::DiagnosticsRefreshRequested);
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
        let _ = tx.try_send(surface(request_id, params));
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

fn dispatch(server: &ServerSocket, tx: &Tx, slots: &Arc<InboundReplySlots>, message: LspMessage) {
    use LspMessage as M;
    match message {
        M::Initialize { .. } | M::Initialized => {}

        // ─── Cancellation ──────────────────────────────────────────────────
        M::CancelRequest { id } => fire::<CancelNotif>(
            server,
            CancelParams {
                id: NumberOrString::Number(id as i32),
            },
        ),
        M::WorkDoneProgressCancel { token } => {
            fire::<WorkDoneProgressCancel>(server, WorkDoneProgressCancelParams { token })
        }

        // ─── Document sync ─────────────────────────────────────────────────
        M::DidOpen {
            uri,
            language_id,
            version,
            text,
        } => fire::<DidOpenTextDocument>(
            server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri,
                    language_id,
                    version,
                    text,
                },
            },
        ),
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

        // ─── Workspace sync ────────────────────────────────────────────────
        M::DidChangeConfiguration { settings } => {
            fire::<DidChangeConfiguration>(server, DidChangeConfigurationParams { settings })
        }
        M::DidChangeWatchedFiles { changes } => {
            fire::<DidChangeWatchedFiles>(server, DidChangeWatchedFilesParams { changes })
        }
        M::DidChangeWorkspaceFolders { event } => {
            fire::<DidChangeWorkspaceFolders>(server, DidChangeWorkspaceFoldersParams { event })
        }

        // ─── Completion / hover / signature ───────────────────────────────
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

        // ─── Navigation ────────────────────────────────────────────────────
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
                        locations: flatten_goto(result),
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
                        locations: flatten_goto(result),
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
                        locations: flatten_goto(result),
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
                        locations: flatten_goto(result),
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

        // ─── Folding / selection ───────────────────────────────────────────
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

        // ─── Code actions / formatting ─────────────────────────────────────
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

        // ─── Inlay hints / decorative ──────────────────────────────────────
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

        // ─── Rename ────────────────────────────────────────────────────────
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

        // ─── Call hierarchy ────────────────────────────────────────────────
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

        // ─── Type hierarchy ───────────────────────────────────────────────
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

        // ─── Pull diagnostics ─────────────────────────────────────────────
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

        // ─── Server-pull responses ─────────────────────────────────────────
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
        Some(CompletionResponse::Array(items)) => emit(
            tx,
            LspResponse::Completion {
                id,
                items,
                is_incomplete: false,
            },
        ),
        Some(CompletionResponse::List(list)) => emit(
            tx,
            LspResponse::Completion {
                id,
                items: list.items,
                is_incomplete: list.is_incomplete,
            },
        ),
        None => emit(
            tx,
            LspResponse::Completion {
                id,
                items: Vec::new(),
                is_incomplete: false,
            },
        ),
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
    spawn::<ExecuteCommand>(server, tx, params, |_result, _tx| {});
}

fn prepare_rename(server: &ServerSocket, tx: &Tx, uri: Url, position: Position, id: u64) {
    spawn::<PrepareRenameRequest>(server, tx, text_pos(uri, position), move |result, tx| {
        match result {
            Some(PrepareRenameResponse::Range(range)) => emit(
                tx,
                LspResponse::PrepareRename {
                    id,
                    range,
                    placeholder: None,
                },
            ),
            Some(PrepareRenameResponse::RangeWithPlaceholder { range, placeholder }) => emit(
                tx,
                LspResponse::PrepareRename {
                    id,
                    range,
                    placeholder: Some(placeholder),
                },
            ),
            // DefaultBehavior needs identifier-at-cursor, which the protocol layer can't compute.
            Some(PrepareRenameResponse::DefaultBehavior { .. }) | None => {}
        }
    });
}

fn flatten_goto(r: Option<GotoDefinitionResponse>) -> Vec<Location> {
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
