//! LSP client transport. async-lsp on a shared tokio runtime, with an mpsc
//! bridge from the async side into ECS via [`LspClient::try_recv`].

use std::ops::ControlFlow;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_lsp::concurrency::ConcurrencyLayer;
use async_lsp::panic::CatchUnwindLayer;
use async_lsp::router::Router;
use async_lsp::tracing::TracingLayer;
use async_lsp::ServerSocket;
use bevy::prelude::*;
use bevy_tokio_tasks::TokioTasksRuntime;
use lsp_types::notification::{
    DidChangeTextDocument, DidOpenTextDocument, Initialized as InitializedNotif,
    Notification as LspNotificationTrait, PublishDiagnostics,
};
use lsp_types::notification::Exit as ExitNotif;
use lsp_types::request::{
    CodeActionRequest, Completion, DocumentHighlightRequest, ExecuteCommand, Formatting,
    GotoDefinition, HoverRequest, InlayHintRequest, Initialize as InitializeRequest,
    PrepareRenameRequest, References, Rename, Request as LspRequestTrait, ResolveCompletionItem,
    Shutdown as ShutdownRequest, SignatureHelpRequest,
};
use lsp_types::*;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tower::ServiceBuilder;

use super::capabilities::ServerCapabilities;
use super::messages::{CodeActionOrCommand, LspMessage, LspResponse};

/// API-parity constant. async-lsp doesn't enforce per-request deadlines;
/// servers handle long-running work via cancel/progress.
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

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

        let stdin = child.stdin.take().expect("Failed to open stdin");
        let stdout = child.stdout.take().expect("Failed to open stdout");
        let stderr = child.stderr.take().expect("Failed to open stderr");

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
                .unhandled_notification(|_, _| ControlFlow::Continue(()));

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
            LspMessage::Initialize { root_uri, capabilities } => {
                self.start_initialize(server.clone(), handle.clone(), root_uri, capabilities);
            }
            LspMessage::Initialized => {}
            // Shutdown / Exit must always go through, even before init is
            // done — the host may be exiting on a half-initialized server.
            other @ (LspMessage::Shutdown { .. } | LspMessage::Exit) => {
                dispatch(server, &self.response_tx, handle, other);
            }
            other if !self.init_done.load(Ordering::Acquire) => {
                self.pre_init_queue.lock().unwrap().push(other);
            }
            other => dispatch(server, &self.response_tx, handle, other),
        }
    }

    fn start_initialize(
        &self,
        server: ServerSocket,
        handle: tokio::runtime::Handle,
        root_uri: Url,
        capabilities: ClientCapabilities,
    ) {
        let tx = self.response_tx.clone();
        let init_done = self.init_done.clone();
        let queue = self.pre_init_queue.clone();
        handle.spawn(async move {
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
            match server.request::<InitializeRequest>(params).await {
                Ok(result) => {
                    if let Err(err) =
                        server.notify::<InitializedNotif>(InitializedParams {})
                    {
                        warn!("[LSP] initialized notify failed: {err}");
                    }
                    init_done.store(true, Ordering::Release);
                    let drained: Vec<LspMessage> = std::mem::take(&mut *queue.lock().unwrap());
                    let h = tokio::runtime::Handle::current();
                    for msg in drained {
                        dispatch(&server, &tx, &h, msg);
                    }
                    emit(&tx, LspResponse::Initialized { capabilities: result.capabilities });
                }
                Err(err) => warn!("[LSP] {} failed: {err}", InitializeRequest::METHOD),
            }
        });
    }

    /// Like [`Self::send`] but skips messages `caps` doesn't advertise.
    /// Init handshake + notifications are always permitted.
    pub fn send_if_supported(&self, message: LspMessage, caps: &ServerCapabilities) -> bool {
        let allowed = match &message {
            LspMessage::Initialize { .. }
            | LspMessage::Initialized
            | LspMessage::DidOpen { .. }
            | LspMessage::DidChange { .. }
            | LspMessage::ExecuteCommand { .. }
            | LspMessage::Shutdown { .. }
            | LspMessage::Exit => true,

            LspMessage::Completion { .. } => caps.supports_completion(),
            LspMessage::ResolveCompletionItem { .. } => caps.supports_completion_resolve(),
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
fn spawn<R>(server: &ServerSocket, tx: &Tx, params: R::Params, map: impl FnOnce(R::Result, &Tx) + Send + 'static)
where
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

fn dispatch(
    server: &ServerSocket,
    tx: &Tx,
    handle: &tokio::runtime::Handle,
    message: LspMessage,
) {
    // Bare tokio::spawn calls inside the match need an active reactor.
    let _guard = handle.enter();
    match message {
        // Initialize / Initialized are handled by `LspClient::send` directly.
        LspMessage::Initialize { .. } | LspMessage::Initialized => {}
        LspMessage::DidOpen { uri, language_id, version, text } => did_open(server, uri, language_id, version, text),
        LspMessage::DidChange { uri, version, changes } => did_change(server, uri, version, changes),
        LspMessage::Completion { uri, position, id } => completion(server, tx, uri, position, id),
        LspMessage::ResolveCompletionItem { item, id } => resolve_completion(server, tx, item, id),
        LspMessage::Hover { uri, position } => hover(server, tx, uri, position),
        LspMessage::GotoDefinition { uri, position } => goto_definition(server, tx, uri, position),
        LspMessage::References { uri, position } => references(server, tx, uri, position),
        LspMessage::Format { uri, options } => format(server, tx, uri, options),
        LspMessage::SignatureHelp { uri, position, id } => signature_help(server, tx, uri, position, id),
        LspMessage::CodeAction { uri, range, diagnostics, id } => code_action(server, tx, uri, range, diagnostics, id),
        LspMessage::InlayHint { uri, range } => inlay_hint(server, tx, uri, range),
        LspMessage::ExecuteCommand { command, arguments } => execute_command(server, tx, command, arguments),
        LspMessage::DocumentHighlight { uri, position } => document_highlight(server, tx, uri, position),
        LspMessage::PrepareRename { uri, position } => prepare_rename(server, tx, uri, position),
        LspMessage::Rename { uri, position, new_name } => rename(server, tx, uri, position, new_name),
        LspMessage::Shutdown { id } => shutdown(server, tx, id),
        LspMessage::Exit => fire::<ExitNotif>(server, ()),
    }
}

fn did_open(server: &ServerSocket, uri: Url, language_id: String, version: i32, text: String) {
    fire::<DidOpenTextDocument>(
        server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem { uri, language_id, version, text },
        },
    );
}

fn did_change(
    server: &ServerSocket,
    uri: Url,
    version: i32,
    changes: Vec<TextDocumentContentChangeEvent>,
) {
    fire::<DidChangeTextDocument>(
        server,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier { uri, version },
            content_changes: changes,
        },
    );
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
            emit(tx, LspResponse::Completion { id, items, is_incomplete: false });
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
            emit(tx, LspResponse::Completion { id, items: Vec::new(), is_incomplete: false });
        }
    });
}

fn hover(server: &ServerSocket, tx: &Tx, uri: Url, position: Position) {
    let params = HoverParams {
        text_document_position_params: text_pos(uri, position),
        work_done_progress_params: Default::default(),
    };
    spawn::<HoverRequest>(server, tx, params, |result, tx| {
        if let Some(h) = result {
            let (content, kind) = extract_hover_content(&h.contents);
            emit(tx, LspResponse::Hover {
                content,
                kind,
                range: h.range,
            });
        }
    });
}

fn goto_definition(server: &ServerSocket, tx: &Tx, uri: Url, position: Position) {
    let params = GotoDefinitionParams {
        text_document_position_params: text_pos(uri, position),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    spawn::<GotoDefinition>(server, tx, params, |result, tx| {
        let locations = match result {
            Some(GotoDefinitionResponse::Scalar(loc)) => vec![loc],
            Some(GotoDefinitionResponse::Array(locs)) => locs,
            Some(GotoDefinitionResponse::Link(links)) => links
                .into_iter()
                .map(|link| Location { uri: link.target_uri, range: link.target_selection_range })
                .collect(),
            None => return,
        };
        if !locations.is_empty() {
            emit(tx, LspResponse::Definition { locations });
        }
    });
}

fn references(server: &ServerSocket, tx: &Tx, uri: Url, position: Position) {
    let params = ReferenceParams {
        text_document_position: text_pos(uri, position),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: ReferenceContext { include_declaration: true },
    };
    spawn::<References>(server, tx, params, |result, tx| {
        if let Some(locations) = result {
            emit(tx, LspResponse::References { locations });
        }
    });
}

fn format(server: &ServerSocket, tx: &Tx, uri: Url, options: FormattingOptions) {
    let params = DocumentFormattingParams {
        text_document: TextDocumentIdentifier { uri },
        options,
        work_done_progress_params: Default::default(),
    };
    spawn::<Formatting>(server, tx, params, |result, tx| {
        if let Some(edits) = result {
            emit(tx, LspResponse::Format { edits });
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
            LspResponse::SignatureHelp { id, signatures, active_signature, active_parameter },
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
        context: CodeActionContext { diagnostics, only: None, trigger_kind: None },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    spawn::<CodeActionRequest>(server, tx, params, move |result, tx| {
        let actions = result
            .unwrap_or_default()
            .into_iter()
            .map(|a| match a {
                lsp_types::CodeActionOrCommand::CodeAction(a) => CodeActionOrCommand::Action(a),
                lsp_types::CodeActionOrCommand::Command(c) => CodeActionOrCommand::Command(c),
            })
            .collect();
        emit(tx, LspResponse::CodeActions { id, actions });
    });
}

fn inlay_hint(server: &ServerSocket, tx: &Tx, uri: Url, range: Range) {
    let params = InlayHintParams {
        text_document: TextDocumentIdentifier { uri },
        range,
        work_done_progress_params: Default::default(),
    };
    spawn::<InlayHintRequest>(server, tx, params, |result, tx| {
        if let Some(hints) = result {
            emit(tx, LspResponse::InlayHints { hints });
        }
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
    // Fire-and-forget: no ack expected. If the command produces edits,
    // the server emits them via workspace/applyEdit (not handled here).
    // Fire-and-forget. If the command produces edits, the server emits them
    // via workspace/applyEdit, which is not handled here.
    spawn::<ExecuteCommand>(server, tx, params, |_result, _tx| {});
}

fn document_highlight(server: &ServerSocket, tx: &Tx, uri: Url, position: Position) {
    let params = DocumentHighlightParams {
        text_document_position_params: text_pos(uri, position),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    spawn::<DocumentHighlightRequest>(server, tx, params, |result, tx| {
        emit(
            tx,
            LspResponse::DocumentHighlights {
                highlights: result.unwrap_or_default(),
            },
        );
    });
}

fn prepare_rename(server: &ServerSocket, tx: &Tx, uri: Url, position: Position) {
    spawn::<PrepareRenameRequest>(server, tx, text_pos(uri, position), |result, tx| match result {
        Some(PrepareRenameResponse::Range(range)) => {
            emit(tx, LspResponse::PrepareRename { range, placeholder: None });
        }
        Some(PrepareRenameResponse::RangeWithPlaceholder { range, placeholder }) => {
            emit(tx, LspResponse::PrepareRename { range, placeholder: Some(placeholder) });
        }
        // DefaultBehavior wants an identifier-at-cursor fallback, which the
        // protocol layer can't compute.
        Some(PrepareRenameResponse::DefaultBehavior { .. }) | None => {}
    });
}

fn resolve_completion(server: &ServerSocket, tx: &Tx, item: CompletionItem, id: u64) {
    spawn::<ResolveCompletionItem>(server, tx, item, move |result, tx| {
        emit(tx, LspResponse::ResolvedCompletionItem { id, item: result });
    });
}

fn shutdown(server: &ServerSocket, tx: &Tx, id: u64) {
    spawn::<ShutdownRequest>(server, tx, (), move |_result, tx| {
        emit(tx, LspResponse::ShutdownAck { id });
    });
}

fn rename(server: &ServerSocket, tx: &Tx, uri: Url, position: Position, new_name: String) {
    let params = RenameParams {
        text_document_position: text_pos(uri, position),
        new_name,
        work_done_progress_params: Default::default(),
    };
    spawn::<Rename>(server, tx, params, |result, tx| {
        if let Some(edit) = result {
            emit(tx, LspResponse::Rename { edit });
        }
    });
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

/// `LanguageString` becomes a fenced markdown code block so the renderer can
/// treat it uniformly.
fn marked_string_with_kind(ms: &MarkedString) -> (String, MarkupKind) {
    match ms {
        MarkedString::String(s) => (s.clone(), MarkupKind::PlainText),
        MarkedString::LanguageString(ls) => (
            format!("```{}\n{}\n```", ls.language, ls.value),
            MarkupKind::Markdown,
        ),
    }
}

/// When the array is heterogeneous (language-strings + plain text) we render
/// the whole thing as markdown; when all elements are plain text we concatenate.
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
