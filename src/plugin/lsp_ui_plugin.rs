//! LSP UI plugin for rendering LSP features
//!
//! This plugin provides default UI rendering for LSP features like
//! completion popups, hover tooltips, signature help, etc.
//!
//! This plugin is optional - users can implement their own UI by
//! querying the marker components created by LspPlugin.

use bevy::prelude::*;

use crate::lsp::prelude::*;
use crate::lsp::{LspUiRenderSet, LspUiSyncSet};
use crate::lsp::theme::LspUiTheme;
use crate::lsp::render::{
    cleanup_lsp_ui_visuals, render_code_actions_popup, render_completion_popup,
    render_document_highlights, render_hover_popup, render_inlay_hints, render_rename_input,
    render_signature_help_popup,
};

/// LSP UI plugin providing default rendering for LSP features
///
/// This plugin must be added AFTER LspPlugin.
/// It renders the UI for completion, hover, diagnostics, etc.
///
/// # Example
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_code_editor::prelude::*;
///
/// App::new()
///     .add_plugins(CodeEditorPlugin::default())
///     .add_plugins(LspPlugin::default())
///     .add_plugins(LspUiPlugin::default())
///     .run();
/// ```
///
/// # Custom UI
/// If you want to implement your own UI, simply don't add this plugin
/// and query the marker components created by LspPlugin instead.
pub struct LspUiPlugin;

impl Default for LspUiPlugin {
    fn default() -> Self {
        Self
    }
}

impl LspUiPlugin {
    /// Create a new LSP UI plugin
    pub fn new() -> Self {
        Self::default()
    }
}

impl Plugin for LspUiPlugin {
    fn build(&self, app: &mut App) {
        // Insert UI theme resource
        app.insert_resource(LspUiTheme::default());

        // LSP UI render systems (marker components -> visuals)
        app.add_systems(
            Update,
            (
                render_completion_popup,
                render_hover_popup,
                render_signature_help_popup,
                render_code_actions_popup,
                render_rename_input,
                render_inlay_hints,
                render_document_highlights,
                cleanup_lsp_ui_visuals,
            )
                .in_set(LspUiRenderSet),
        );
    }
}
