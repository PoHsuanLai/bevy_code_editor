//! LSP message types for communication with language servers.
//!
//! Two layers:
//! - [`LspMessage`] / [`LspResponse`] are the protocol DTOs that flow over the
//!   async transport channel inside `LspClient`. Internal to the crate's
//!   transport.
//! - The `Lsp*Response` `Message` types below mirror each [`LspResponse`]
//!   variant onto Bevy's message bus, tagged with the originating
//!   [`Entity`]. `LspPlugin` runs `drain_lsp_responses` each frame to fan the
//!   transport channel out into typed Bevy messages so any host system can
//!   subscribe without owning the [`crate::LspClient`].
//!
//! Coverage goal: every request/notification in the LSP 3.17 spec that has a
//! typed shape in [`lsp_types`] is represented here. Some of them are not
//! consumed by `bevy_code_editor`, but a host that wants to build (say) an
//! outline panel from `documentSymbol` or render call hierarchies just needs
//! to send the request and subscribe to the response message — this crate
//! does not gate features it doesn't itself use.

use bevy_ecs::prelude::*;
use lsp_types::*;
use serde::{Deserialize, Serialize};

/// Type of LSP request, used to match responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestType {
    Initialize,
    Completion,
    CompletionItemResolve,
    Hover,
    GotoDeclaration,
    GotoDefinition,
    GotoTypeDefinition,
    GotoImplementation,
    References,
    Format,
    RangeFormatting,
    OnTypeFormatting,
    SignatureHelp,
    CodeAction,
    CodeActionResolve,
    InlayHint,
    InlayHintResolve,
    DocumentHighlight,
    DocumentSymbol,
    WorkspaceSymbol,
    WorkspaceSymbolResolve,
    FoldingRange,
    SelectionRange,
    DocumentLink,
    DocumentLinkResolve,
    DocumentColor,
    ColorPresentation,
    LinkedEditingRange,
    Moniker,
    PrepareRename,
    Rename,
    PrepareCallHierarchy,
    CallHierarchyIncomingCalls,
    CallHierarchyOutgoingCalls,
    PrepareTypeHierarchy,
    TypeHierarchySupertypes,
    TypeHierarchySubtypes,
    SemanticTokensFull,
    SemanticTokensFullDelta,
    SemanticTokensRange,
    DocumentDiagnostic,
    WorkspaceDiagnostic,
    Shutdown,
}

/// Outgoing message to the language server. Variants are 1:1 with the LSP
/// 3.17 spec; consumers send via [`crate::LspClient::send`] and observe
/// matching [`LspResponse`] variants on the bridge channel.
///
/// Variants carrying an opaque `id: u64` echo it back on their response so
/// consumers can drop stale results when the user moves on. Variants with
/// no `id` are fire-and-forget notifications.
#[derive(Debug, Clone)]
pub enum LspMessage {
    // ─── Lifecycle ────────────────────────────────────────────────────────
    Initialize {
        root_uri: Url,
        capabilities: Box<ClientCapabilities>,
    },

    /// Sent by the transport once `initialize` succeeds. Hosts do not
    /// usually emit this directly.
    Initialized,

    /// Cancel the in-flight request with the given id. `id` matches the id
    /// returned at request submission time on the underlying JSON-RPC
    /// request. async-lsp manages most cancellation via futures, but this
    /// variant exists for completeness.
    CancelRequest {
        id: u64,
    },

    // ─── Document sync ────────────────────────────────────────────────────
    DidOpen {
        uri: Url,
        language_id: String,
        version: i32,
        text: String,
    },

    DidChange {
        uri: Url,
        version: i32,
        changes: Vec<TextDocumentContentChangeEvent>,
    },

    /// `text` is included only when the server's `save.includeText` option
    /// is `true`. Pass `None` otherwise.
    DidSave {
        uri: Url,
        text: Option<String>,
    },

    /// The matching `didOpen` is paired with this notification when a
    /// document tab closes.
    DidClose {
        uri: Url,
    },

    /// `reason` is one of the `TextDocumentSaveReason` values
    /// (Manual / AfterDelay / FocusOut).
    WillSave {
        uri: Url,
        reason: TextDocumentSaveReason,
    },

    /// Synchronous variant — the server returns text edits to apply before
    /// the actual save. `id` echoes back on [`LspResponse::WillSaveWaitUntil`].
    WillSaveWaitUntil {
        uri: Url,
        reason: TextDocumentSaveReason,
        id: u64,
    },

    // ─── Workspace sync ───────────────────────────────────────────────────
    /// Settings the server should re-read. `settings` is opaque JSON keyed
    /// however the specific server expects (e.g. rust-analyzer's section
    /// tree, pyright's `python.analysis.*`).
    DidChangeConfiguration {
        settings: serde_json::Value,
    },

    /// Files that changed outside the editor (git pull, rebase, build
    /// outputs). Servers expect this to refresh their watchers.
    DidChangeWatchedFiles {
        changes: Vec<FileEvent>,
    },

    /// Multi-root workspace folder set changed.
    DidChangeWorkspaceFolders {
        event: WorkspaceFoldersChangeEvent,
    },

    // ─── Completion / hover / signature ──────────────────────────────────
    Completion {
        uri: Url,
        position: Position,
        id: u64,
    },

    /// Lazy-load completion details (docs, additional edits). Gated on
    /// `completion_provider.resolve_provider`.
    ResolveCompletionItem {
        item: Box<CompletionItem>,
        id: u64,
    },

    Hover {
        uri: Url,
        position: Position,
        id: u64,
    },

    SignatureHelp {
        uri: Url,
        position: Position,
        id: u64,
    },

    // ─── Navigation ───────────────────────────────────────────────────────
    GotoDeclaration {
        uri: Url,
        position: Position,
        id: u64,
    },

    GotoDefinition {
        uri: Url,
        position: Position,
        id: u64,
    },

    GotoTypeDefinition {
        uri: Url,
        position: Position,
        id: u64,
    },

    GotoImplementation {
        uri: Url,
        position: Position,
        id: u64,
    },

    References {
        uri: Url,
        position: Position,
        id: u64,
    },

    DocumentHighlight {
        uri: Url,
        position: Position,
        id: u64,
    },

    /// File-scoped outline. Servers either return a `SymbolInformation[]`
    /// (flat) or a `DocumentSymbol[]` (hierarchical) — the response carries
    /// both so consumers pick the shape they want.
    DocumentSymbol {
        uri: Url,
        id: u64,
    },

    /// Workspace-wide symbol query (Cmd+T / Ctrl+T). `query` is the
    /// substring filter.
    WorkspaceSymbol {
        query: String,
        id: u64,
    },

    /// Some servers (rust-analyzer) return cheap stubs from
    /// `WorkspaceSymbol` and fill in `Location` lazily here.
    WorkspaceSymbolResolve {
        symbol: WorkspaceSymbol,
        id: u64,
    },

    // ─── Folding / selection ──────────────────────────────────────────────
    FoldingRange {
        uri: Url,
        id: u64,
    },

    /// "Smart expand selection" — server-driven semantic ranges around
    /// each cursor position.
    SelectionRange {
        uri: Url,
        positions: Vec<Position>,
        id: u64,
    },

    // ─── Code actions / formatting ────────────────────────────────────────
    CodeAction {
        uri: Url,
        range: Range,
        diagnostics: Vec<Diagnostic>,
        id: u64,
    },

    /// Lazy-resolve the edits / commands inside a `CodeAction` returned
    /// without an `edit` field.
    CodeActionResolve {
        action: Box<CodeAction>,
        id: u64,
    },

    Format {
        uri: Url,
        options: FormattingOptions,
        id: u64,
    },

    RangeFormatting {
        uri: Url,
        range: Range,
        options: FormattingOptions,
        id: u64,
    },

    OnTypeFormatting {
        uri: Url,
        position: Position,
        ch: String,
        options: FormattingOptions,
        id: u64,
    },

    /// Execute a server command, usually one returned by a code action.
    ExecuteCommand {
        command: String,
        arguments: Option<Vec<serde_json::Value>>,
    },

    // ─── Inlay hints / decorative ─────────────────────────────────────────
    InlayHint {
        uri: Url,
        range: Range,
        id: u64,
    },

    /// Lazy-resolve inlay hint details (tooltips, command bindings).
    InlayHintResolve {
        hint: InlayHint,
        id: u64,
    },

    /// Clickable spans in the document (URL → goto). Most servers
    /// implement this for comments containing URIs.
    DocumentLink {
        uri: Url,
        id: u64,
    },

    DocumentLinkResolve {
        link: DocumentLink,
        id: u64,
    },

    /// Color literals (`#fff`, `rgb(...)`) the server can recognize.
    DocumentColor {
        uri: Url,
        id: u64,
    },

    /// Alternate textual presentations for a picked color.
    ColorPresentation {
        uri: Url,
        color: lsp_types::Color,
        range: Range,
        id: u64,
    },

    /// Synchronized rename: typing in one part of a tag pair updates the
    /// other (HTML/JSX-style).
    LinkedEditingRange {
        uri: Url,
        position: Position,
        id: u64,
    },

    /// Symbol-graph identity for code-intel pipelines.
    Moniker {
        uri: Url,
        position: Position,
        id: u64,
    },

    // ─── Rename ───────────────────────────────────────────────────────────
    PrepareRename {
        uri: Url,
        position: Position,
        id: u64,
    },

    Rename {
        uri: Url,
        position: Position,
        new_name: String,
        id: u64,
    },

    // ─── Call hierarchy ───────────────────────────────────────────────────
    PrepareCallHierarchy {
        uri: Url,
        position: Position,
        id: u64,
    },

    CallHierarchyIncomingCalls {
        item: CallHierarchyItem,
        id: u64,
    },

    CallHierarchyOutgoingCalls {
        item: CallHierarchyItem,
        id: u64,
    },

    // ─── Type hierarchy ──────────────────────────────────────────────────
    PrepareTypeHierarchy {
        uri: Url,
        position: Position,
        id: u64,
    },

    TypeHierarchySupertypes {
        item: TypeHierarchyItem,
        id: u64,
    },

    TypeHierarchySubtypes {
        item: TypeHierarchyItem,
        id: u64,
    },

    // ─── Semantic tokens ──────────────────────────────────────────────────
    SemanticTokensFull {
        uri: Url,
        id: u64,
    },

    /// Delta encoding from a previous response's `result_id`.
    SemanticTokensFullDelta {
        uri: Url,
        previous_result_id: String,
        id: u64,
    },

    SemanticTokensRange {
        uri: Url,
        range: Range,
        id: u64,
    },

    // ─── Pull diagnostics (LSP 3.17) ──────────────────────────────────────
    DocumentDiagnostic {
        uri: Url,
        identifier: Option<String>,
        previous_result_id: Option<String>,
        id: u64,
    },

    WorkspaceDiagnostic {
        identifier: Option<String>,
        previous_result_ids: Vec<PreviousResultId>,
        id: u64,
    },

    // ─── Server-pull responses (host responds to server-initiated requests)
    //
    // When the server sends `workspace/configuration`, `workspace/applyEdit`,
    // `window/showMessageRequest`, `window/showDocument`,
    // `window/workDoneProgress/create`, `client/registerCapability`, or
    // `client/unregisterCapability`, the transport surfaces it as a
    // matching `LspResponse::*Requested` variant carrying a `request_id`.
    // The host computes its answer and sends one of these `Respond*`
    // variants back through the same client; the transport pairs `id`
    // with the suspended JSON-RPC response slot.
    /// Reply to a `workspace/configuration` request from the server. The
    /// `items` Vec must match the order of the requested items.
    RespondConfiguration {
        id: u64,
        items: Vec<serde_json::Value>,
    },

    RespondApplyEdit {
        id: u64,
        response: ApplyWorkspaceEditResponse,
    },

    RespondShowMessageRequest {
        id: u64,
        action: Option<MessageActionItem>,
    },

    RespondShowDocument {
        id: u64,
        success: bool,
    },

    RespondWorkDoneProgressCreate {
        id: u64,
    },

    RespondRegisterCapability {
        id: u64,
    },
    RespondUnregisterCapability {
        id: u64,
    },

    RespondWorkspaceFolders {
        id: u64,
        folders: Option<Vec<WorkspaceFolder>>,
    },

    /// Cancel an active progress operation by its token.
    WorkDoneProgressCancel {
        token: ProgressToken,
    },

    // ─── Termination ──────────────────────────────────────────────────────
    /// Send before [`LspMessage::Exit`] for graceful termination. `id` is
    /// opaque; the response arrives as [`LspResponse::ShutdownAck`].
    Shutdown {
        id: u64,
    },

    /// Tell the server to exit. Notification — no response.
    Exit,
}

/// Incoming message from the language server. Variants split into:
/// - **Responses to client-initiated requests** — carry `id` echoed from the
///   originating [`LspMessage`].
/// - **Server-initiated notifications** — diagnostics, log messages,
///   progress updates. Fire-and-forget on the host side.
/// - **Server-initiated requests** — variants ending in `Requested` carry
///   `request_id`; the host must respond with the matching
///   `LspMessage::Respond*` variant or the server may stall.
#[derive(Debug, Clone)]
pub enum LspResponse {
    Initialized {
        capabilities: Box<ServerCapabilities>,
    },

    // ─── Notifications from the server ────────────────────────────────────
    Diagnostics {
        uri: Url,
        /// Server-supplied document version this batch was computed against.
        /// `None` means the server didn't supply one (older spec). Consumers
        /// should discard batches whose `version` is older than the client's
        /// current [`crate::LspDocument::version`].
        version: Option<i32>,
        diagnostics: Vec<Diagnostic>,
    },

    LogMessage {
        typ: MessageType,
        message: String,
    },

    ShowMessage {
        typ: MessageType,
        message: String,
    },

    /// `$/progress` payload. Hosts that don't render progress can ignore.
    Progress {
        token: ProgressToken,
        value: ProgressParamsValue,
    },

    /// `telemetry/event` — opaque JSON payload for analytics.
    Telemetry {
        data: serde_json::Value,
    },

    LogTrace {
        message: String,
        verbose: Option<String>,
    },

    // ─── Server requests requiring a host reply ──────────────────────────
    /// Server asks for configuration sections — host inspects `items` and
    /// replies with [`LspMessage::RespondConfiguration`].
    ConfigurationRequested {
        request_id: u64,
        items: Vec<ConfigurationItem>,
    },

    ApplyEditRequested {
        request_id: u64,
        label: Option<String>,
        edit: WorkspaceEdit,
    },

    ShowMessageRequestRequested {
        request_id: u64,
        typ: MessageType,
        message: String,
        actions: Option<Vec<MessageActionItem>>,
    },

    ShowDocumentRequested {
        request_id: u64,
        uri: Url,
        external: Option<bool>,
        take_focus: Option<bool>,
        selection: Option<Range>,
    },

    WorkDoneProgressCreateRequested {
        request_id: u64,
        token: ProgressToken,
    },

    RegisterCapabilityRequested {
        request_id: u64,
        registrations: Vec<Registration>,
    },

    UnregisterCapabilityRequested {
        request_id: u64,
        unregistrations: Vec<Unregistration>,
    },

    WorkspaceFoldersRequested {
        request_id: u64,
    },

    // ─── Refresh hints (server asks the client to invalidate caches) ─────
    SemanticTokensRefreshRequested,
    InlayHintRefreshRequested,
    CodeLensRefreshRequested,
    DiagnosticsRefreshRequested,

    // ─── Responses keyed by request id ────────────────────────────────────
    Completion {
        id: u64,
        items: Vec<CompletionItem>,
        is_incomplete: bool,
    },

    ResolvedCompletionItem {
        id: u64,
        item: Box<CompletionItem>,
    },

    Hover {
        id: u64,
        content: String,
        kind: MarkupKind,
        range: Option<Range>,
    },

    SignatureHelp {
        id: u64,
        signatures: Vec<SignatureInformation>,
        active_signature: Option<u32>,
        active_parameter: Option<u32>,
    },

    Declaration {
        id: u64,
        locations: Vec<Location>,
    },

    Definition {
        id: u64,
        locations: Vec<Location>,
    },

    TypeDefinition {
        id: u64,
        locations: Vec<Location>,
    },

    Implementation {
        id: u64,
        locations: Vec<Location>,
    },

    References {
        id: u64,
        locations: Vec<Location>,
    },

    DocumentHighlights {
        id: u64,
        highlights: Vec<DocumentHighlight>,
    },

    /// Either `SymbolInformation[]` (flat, deprecated but widespread) or
    /// `DocumentSymbol[]` (hierarchical). Hosts pick whichever shape they
    /// render; servers only return one.
    DocumentSymbols {
        id: u64,
        flat: Vec<SymbolInformation>,
        nested: Vec<DocumentSymbol>,
    },

    WorkspaceSymbols {
        id: u64,
        symbols: Vec<WorkspaceSymbolResponseItem>,
    },

    ResolvedWorkspaceSymbol {
        id: u64,
        symbol: WorkspaceSymbol,
    },

    FoldingRanges {
        id: u64,
        ranges: Vec<FoldingRange>,
    },

    SelectionRanges {
        id: u64,
        ranges: Vec<SelectionRange>,
    },

    CodeActions {
        id: u64,
        actions: Vec<CodeActionOrCommand>,
    },

    ResolvedCodeAction {
        id: u64,
        action: Box<CodeAction>,
    },

    Format {
        id: u64,
        edits: Vec<TextEdit>,
    },

    RangeFormatting {
        id: u64,
        edits: Vec<TextEdit>,
    },

    OnTypeFormatting {
        id: u64,
        edits: Vec<TextEdit>,
    },

    WillSaveWaitUntil {
        id: u64,
        edits: Vec<TextEdit>,
    },

    InlayHints {
        id: u64,
        hints: Vec<InlayHint>,
    },

    ResolvedInlayHint {
        id: u64,
        hint: InlayHint,
    },

    DocumentLinks {
        id: u64,
        links: Vec<DocumentLink>,
    },

    ResolvedDocumentLink {
        id: u64,
        link: DocumentLink,
    },

    DocumentColors {
        id: u64,
        colors: Vec<ColorInformation>,
    },

    ColorPresentations {
        id: u64,
        presentations: Vec<ColorPresentation>,
    },

    LinkedEditingRanges {
        id: u64,
        ranges: Option<LinkedEditingRanges>,
    },

    Monikers {
        id: u64,
        monikers: Vec<Moniker>,
    },

    PrepareRename {
        id: u64,
        range: Range,
        placeholder: Option<String>,
    },

    Rename {
        id: u64,
        edit: WorkspaceEdit,
    },

    PrepareCallHierarchy {
        id: u64,
        items: Vec<CallHierarchyItem>,
    },

    CallHierarchyIncomingCalls {
        id: u64,
        calls: Vec<CallHierarchyIncomingCall>,
    },

    CallHierarchyOutgoingCalls {
        id: u64,
        calls: Vec<CallHierarchyOutgoingCall>,
    },

    PrepareTypeHierarchy {
        id: u64,
        items: Vec<TypeHierarchyItem>,
    },

    TypeHierarchySupertypes {
        id: u64,
        items: Vec<TypeHierarchyItem>,
    },

    TypeHierarchySubtypes {
        id: u64,
        items: Vec<TypeHierarchyItem>,
    },

    SemanticTokens {
        id: u64,
        result: SemanticTokensResult,
    },

    SemanticTokensDelta {
        id: u64,
        result: SemanticTokensFullDeltaResult,
    },

    SemanticTokensRange {
        id: u64,
        result: SemanticTokensRangeResult,
    },

    DocumentDiagnostic {
        id: u64,
        report: DocumentDiagnosticReportResult,
    },

    WorkspaceDiagnostic {
        id: u64,
        report: WorkspaceDiagnosticReportResult,
    },

    ShutdownAck {
        id: u64,
    },

    /// Server process exited unexpectedly or its read channel closed
    /// without an explicit shutdown. Hosts can drop or restart the client.
    Crashed,
}

/// Code action or command returned by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CodeActionOrCommand {
    Action(Box<CodeAction>),
    Command(lsp_types::Command),
}

/// `workspace/symbol` returns either the legacy `SymbolInformation` shape
/// or the newer `WorkspaceSymbol` (which carries an opaque `data` payload
/// for `workspaceSymbol/resolve`). Both are surfaced; hosts pick the shape
/// they render.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WorkspaceSymbolResponseItem {
    Symbol(WorkspaceSymbol),
    Information(SymbolInformation),
}

// ─── Outbound request message ──────────────────────────────────────────
//
// Hosts write `MessageWriter<LspRequest>` to send any LSP request or
// notification without importing `LspClient` directly. `LspPlugin` wires
// one observer that routes each message to the right client entity.

/// Wraps an [`LspMessage`] with the target [`LspClient`] entity.
/// Write with `MessageWriter<LspRequest>`; respond by reading the
/// matching `Lsp*Response` message.
#[derive(Message, EntityEvent, Clone, Debug)]
pub struct LspRequest {
    pub entity: Entity,
    pub msg: LspMessage,
}

// ─── Bevy messages ─────────────────────────────────────────────────────
//
// One per LspResponse variant. Hosts subscribe to whichever they care about.
// None of these are `Reflect` — the lsp_types payloads aren't.

macro_rules! lsp_msg {
    ($($name:ident { $($field:ident : $ty:ty),* $(,)? }),* $(,)?) => {
        $(
            #[derive(Message, Clone, Debug)]
            pub struct $name {
                pub entity: Entity,
                $(pub $field: $ty,)*
            }
        )*
    };
}

lsp_msg! {
    LspServerInitialized { capabilities: ServerCapabilities },
    LspDiagnosticsUpdated { uri: Url, version: Option<i32>, diagnostics: Vec<Diagnostic> },
    LspLogMessage { typ: MessageType, message: String },
    LspShowMessage { typ: MessageType, message: String },
    LspProgress { token: ProgressToken, value: ProgressParamsValue },
    LspTelemetry { data: serde_json::Value },
    LspLogTrace { message: String, verbose: Option<String> },

    LspConfigurationRequested { request_id: u64, items: Vec<ConfigurationItem> },
    LspApplyEditRequested { request_id: u64, label: Option<String>, edit: WorkspaceEdit },
    LspShowMessageRequestRequested {
        request_id: u64,
        typ: MessageType,
        message: String,
        actions: Option<Vec<MessageActionItem>>,
    },
    LspShowDocumentRequested {
        request_id: u64,
        uri: Url,
        external: Option<bool>,
        take_focus: Option<bool>,
        selection: Option<Range>,
    },
    LspWorkDoneProgressCreateRequested { request_id: u64, token: ProgressToken },
    LspRegisterCapabilityRequested { request_id: u64, registrations: Vec<Registration> },
    LspUnregisterCapabilityRequested { request_id: u64, unregistrations: Vec<Unregistration> },
    LspWorkspaceFoldersRequested { request_id: u64 },

    LspSemanticTokensRefreshRequested {},
    LspInlayHintRefreshRequested {},
    LspCodeLensRefreshRequested {},
    LspDiagnosticsRefreshRequested {},

    LspCompletionResponse { id: u64, items: Vec<CompletionItem>, is_incomplete: bool },
    LspResolvedCompletionItem { id: u64, item: CompletionItem },
    LspHoverResponse { id: u64, content: String, kind: MarkupKind, range: Option<Range> },
    LspSignatureHelpResponse {
        id: u64,
        signatures: Vec<SignatureInformation>,
        active_signature: Option<u32>,
        active_parameter: Option<u32>,
    },
    LspDeclarationResponse { id: u64, locations: Vec<Location> },
    LspDefinitionResponse { id: u64, locations: Vec<Location> },
    LspTypeDefinitionResponse { id: u64, locations: Vec<Location> },
    LspImplementationResponse { id: u64, locations: Vec<Location> },
    LspReferencesResponse { id: u64, locations: Vec<Location> },
    LspDocumentHighlightsResponse { id: u64, highlights: Vec<DocumentHighlight> },
    LspDocumentSymbolsResponse {
        id: u64,
        flat: Vec<SymbolInformation>,
        nested: Vec<DocumentSymbol>,
    },
    LspWorkspaceSymbolsResponse { id: u64, symbols: Vec<WorkspaceSymbolResponseItem> },
    LspResolvedWorkspaceSymbol { id: u64, symbol: WorkspaceSymbol },
    LspFoldingRangesResponse { id: u64, ranges: Vec<FoldingRange> },
    LspSelectionRangesResponse { id: u64, ranges: Vec<SelectionRange> },
    LspCodeActionsResponse { id: u64, actions: Vec<CodeActionOrCommand> },
    LspResolvedCodeAction { id: u64, action: CodeAction },
    LspFormatResponse { id: u64, edits: Vec<TextEdit> },
    LspRangeFormattingResponse { id: u64, edits: Vec<TextEdit> },
    LspOnTypeFormattingResponse { id: u64, edits: Vec<TextEdit> },
    LspWillSaveWaitUntilResponse { id: u64, edits: Vec<TextEdit> },
    LspInlayHintsResponse { id: u64, hints: Vec<InlayHint> },
    LspResolvedInlayHint { id: u64, hint: InlayHint },
    LspDocumentLinksResponse { id: u64, links: Vec<DocumentLink> },
    LspResolvedDocumentLink { id: u64, link: DocumentLink },
    LspDocumentColorsResponse { id: u64, colors: Vec<ColorInformation> },
    LspColorPresentationsResponse { id: u64, presentations: Vec<ColorPresentation> },
    LspLinkedEditingRangesResponse { id: u64, ranges: Option<LinkedEditingRanges> },
    LspMonikersResponse { id: u64, monikers: Vec<Moniker> },
    LspPrepareRenameResponse { id: u64, range: Range, placeholder: Option<String> },
    LspRenameResponse { id: u64, edit: WorkspaceEdit },
    LspPrepareCallHierarchyResponse { id: u64, items: Vec<CallHierarchyItem> },
    LspCallHierarchyIncomingCallsResponse { id: u64, calls: Vec<CallHierarchyIncomingCall> },
    LspCallHierarchyOutgoingCallsResponse { id: u64, calls: Vec<CallHierarchyOutgoingCall> },
    LspPrepareTypeHierarchyResponse { id: u64, items: Vec<TypeHierarchyItem> },
    LspTypeHierarchySupertypesResponse { id: u64, items: Vec<TypeHierarchyItem> },
    LspTypeHierarchySubtypesResponse { id: u64, items: Vec<TypeHierarchyItem> },
    LspSemanticTokensResponse { id: u64, result: SemanticTokensResult },
    LspSemanticTokensDeltaResponse { id: u64, result: SemanticTokensFullDeltaResult },
    LspSemanticTokensRangeResponse { id: u64, result: SemanticTokensRangeResult },
    LspDocumentDiagnosticResponse { id: u64, report: DocumentDiagnosticReportResult },
    LspWorkspaceDiagnosticResponse { id: u64, report: WorkspaceDiagnosticReportResult },
    LspShutdownAck { id: u64 },
    LspServerCrashed {},
}
