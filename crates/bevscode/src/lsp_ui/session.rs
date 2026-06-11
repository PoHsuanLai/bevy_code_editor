//! LSP session wiring — connects a [`CodeEditor`] to a [`LanguageService`]
//! entity.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::types::CodeEditor;

/// Points a [`CodeEditor`] at the entity carrying its
/// [`bevy_lsp::LspClient`] and [`bevy_lsp::ServerCapabilities`].
///
/// Optional — editors without this component have no LSP behavior.
/// Insert it alongside [`bevy_lsp::LspDocument`] and
/// [`bevy_lsp::LspServiceRef`] when wiring an editor to a language
/// server.
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct LspSession(pub Entity);

/// Read-only lookup of [`bevy_lsp::ServerCapabilities`] for an editor.
///
/// Follows [`LspSession`] to the service entity. Returns `None` when
/// the editor has no session or the service entity is missing.
#[derive(SystemParam)]
pub struct LspCaps<'w, 's> {
    sessions: Query<'w, 's, &'static LspSession, With<CodeEditor>>,
    caps: Query<'w, 's, &'static bevy_lsp::ServerCapabilities>,
}

impl LspCaps<'_, '_> {
    pub fn get(&self, editor: Entity) -> Option<&bevy_lsp::ServerCapabilities> {
        let session = self.sessions.get(editor).ok()?;
        self.caps.get(session.0).ok()
    }
}

/// Read-only check of whether the service's [`bevy_lsp::LspClient`] is
/// ready (connected and initialized).
#[derive(SystemParam)]
pub struct LspReady<'w, 's> {
    sessions: Query<'w, 's, &'static LspSession, With<CodeEditor>>,
    clients: Query<'w, 's, &'static bevy_lsp::LspClient>,
}

impl LspReady<'_, '_> {
    pub fn is_ready(&self, editor: Entity) -> bool {
        let Some(session) = self.sessions.get(editor).ok() else {
            return false;
        };
        self.clients.get(session.0).is_ok_and(|c| c.is_ready())
    }

    pub fn service_entity(&self, editor: Entity) -> Option<Entity> {
        self.sessions.get(editor).ok().map(|s| s.0)
    }
}

/// Builds an [`LspRequest`] that routes through [`LspSession`] when
/// present. Call this instead of constructing `LspRequest` directly.
///
/// - If the editor has `LspSession`, targets the service entity and
///   sets `origin` to the editor so responses route back.
/// - If the editor has no session, targets the editor itself (legacy
///   path where `LspClient` lives on the editor).
pub fn lsp_request(
    editor: Entity,
    session: Option<&LspSession>,
    msg: bevy_lsp::LspMessage,
) -> bevy_lsp::LspRequest {
    match session {
        Some(s) => bevy_lsp::LspRequest {
            entity: s.0,
            origin: Some(editor),
            msg,
        },
        None => bevy_lsp::LspRequest {
            entity: editor,
            origin: None,
            msg,
        },
    }
}

/// Spawn a `LanguageService` entity that owns the LSP transport.
///
/// Starts the language server process, spawns an entity with the
/// [`LspClient`](bevy_lsp::LspClient),
/// [`ServerCapabilities`](bevy_lsp::ServerCapabilities), and
/// [`LspRequestOrigins`](bevy_lsp::LspRequestOrigins), then queues
/// `Initialize` and `Initialized` requests. Returns the service
/// entity, or an error if the server process failed to start.
///
/// Wire editors to this service with [`attach_lsp`].
pub fn spawn_language_service(
    commands: &mut Commands,
    lsp_w: &mut MessageWriter<bevy_lsp::LspRequest>,
    command: &str,
    args: &[&str],
    root_uri: lsp_types::Url,
    capabilities: lsp_types::ClientCapabilities,
) -> std::io::Result<Entity> {
    let mut client = bevy_lsp::LspClient::new();
    client.start(command, args)?;

    let service = commands
        .spawn((
            client,
            bevy_lsp::ServerCapabilities::default(),
            bevy_lsp::LspRequestOrigins::default(),
        ))
        .id();

    lsp_w.write(bevy_lsp::LspRequest {
        entity: service,
        origin: None,
        msg: bevy_lsp::LspMessage::Initialize {
            root_uri,
            capabilities: Box::new(capabilities),
        },
    });
    lsp_w.write(bevy_lsp::LspRequest {
        entity: service,
        origin: None,
        msg: bevy_lsp::LspMessage::Initialized,
    });

    Ok(service)
}

/// Wire a [`CodeEditor`] entity to a `LanguageService`. Inserts
/// [`LspSession`], [`LspDocument`](bevy_lsp::LspDocument), and
/// [`LspServiceRef`](bevy_lsp::LspServiceRef), then sends a
/// `textDocument/didOpen` notification.
///
/// The `init_lsp_components_on_session` system will insert all LSP UI
/// state components on the next frame.
pub fn attach_lsp(
    commands: &mut Commands,
    lsp_w: &mut MessageWriter<bevy_lsp::LspRequest>,
    editor: Entity,
    service: Entity,
    uri: lsp_types::Url,
    language_id: &str,
    text: String,
) {
    commands.entity(editor).insert((
        LspSession(service),
        bevy_lsp::LspDocument::new(uri.clone(), language_id),
        bevy_lsp::LspServiceRef(service),
    ));

    lsp_w.write(bevy_lsp::LspRequest {
        entity: service,
        origin: None,
        msg: bevy_lsp::LspMessage::DidOpen {
            uri,
            language_id: language_id.to_string(),
            version: 1,
            text,
        },
    });
}

/// Syncs [`bevy_lsp::ServerCapabilities`] from the service entity to
/// editors that have an [`LspSession`]. This lets existing systems
/// that query `&ServerCapabilities` on the editor continue to work
/// without modification.
pub(crate) fn sync_capabilities_from_service(
    mut commands: Commands,
    editors: Query<(Entity, &LspSession), With<CodeEditor>>,
    services: Query<&bevy_lsp::ServerCapabilities>,
) {
    for (editor, session) in &editors {
        if let Ok(caps) = services.get(session.0) {
            commands.entity(editor).insert(caps.clone());
        }
    }
}

/// Inserts all LSP UI state components when [`LspSession`] is added to
/// an editor. This replaces the old `#[require]` cascade for LSP
/// components.
pub(crate) fn init_lsp_components_on_session(
    mut commands: Commands,
    query: Query<Entity, (With<CodeEditor>, Added<LspSession>)>,
) {
    for entity in &query {
        commands.entity(entity).insert((
            crate::settings::DiagnosticColors::default(),
            crate::settings::Suggest::default(),
            crate::settings::LspConfig::default(),
            crate::lsp_ui::completion::LspCompletionPopup::default(),
            crate::lsp_ui::state::LspHoverPopup::default(),
            crate::lsp_ui::state::LspSignatureHelpPopup::default(),
            crate::lsp_ui::state::LspCodeActionsPopup::default(),
            crate::lsp_ui::state::LspInlayHints::default(),
            crate::lsp_ui::state::LspDocumentHighlights::default(),
            crate::lsp_ui::state::LspRenamePopup::default(),
            crate::lsp_ui::state::LspDebounceTimers::default(),
            crate::lsp_ui::state::LspDidChangeBatcher::default(),
        ));
        commands.entity(entity).insert((
            crate::lsp_ui::state::TabstopSession::default(),
            crate::lsp_ui::lifecycle::HoverLifecycle::default(),
            crate::lsp_ui::lifecycle::CompletionLifecycle::default(),
            crate::lsp_ui::lifecycle::SignatureLifecycle::default(),
            crate::lsp_ui::lifecycle::CodeActionsLifecycle::default(),
            crate::lsp_ui::lifecycle::RenameLifecycle::default(),
            crate::plugin::DiagnosticUnderlineRects::default(),
        ));
    }
}
