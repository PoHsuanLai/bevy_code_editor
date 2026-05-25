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
        self.clients
            .get(session.0)
            .map_or(false, |c| c.is_ready())
    }

    pub fn service_entity(&self, editor: Entity) -> Option<Entity> {
        self.sessions.get(editor).ok().map(|s| s.0)
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
