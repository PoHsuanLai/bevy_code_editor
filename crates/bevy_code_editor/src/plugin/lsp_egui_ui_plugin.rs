//! LSP egui/armas UI plugin
//!
//! Replaces the Bevy Sprite-based LSP UI with egui overlays styled by armas.
//! Use this instead of `LspUiPlugin` when your app uses bevy_egui.
//!
//! Inlay hints and document highlights remain as Bevy sprites since they are
//! inline decorations, not interactive popups.

use bevy::prelude::*;

use crate::lsp::egui_render::*;
use crate::lsp::render::{render_document_highlights, render_inlay_hints};
use crate::lsp::theme::LspUiTheme;
use crate::lsp::LspUiRenderSet;

/// LSP UI plugin using egui/armas overlays for popup rendering.
///
/// This plugin replaces `LspUiPlugin`. Do not add both.
///
/// # Example
/// ```rust,ignore
/// use bevy::prelude::*;
/// use bevy_code_editor::prelude::*;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(bevy_egui::EguiPlugin::default())
///     .add_plugins(CodeEditorPlugin::default())
///     .add_plugins(LspPlugin::default())
///     .add_plugins(LspEguiUiPlugin::default())
///     .run();
/// ```
pub struct LspEguiUiPlugin;

impl Default for LspEguiUiPlugin {
    fn default() -> Self {
        Self
    }
}

impl Plugin for LspEguiUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LspEguiViewportOffset>();

        // Insert LspUiTheme for the sprite-based systems (inlay hints, doc highlights)
        app.insert_resource(LspUiTheme::default());

        app.add_systems(
            Update,
            (
                // egui-based popup rendering
                render_completion_egui,
                render_hover_egui,
                render_signature_help_egui,
                render_code_actions_egui,
                render_rename_egui,
                // Sprite-based inline decorations (no egui equivalent needed)
                render_inlay_hints,
                render_document_highlights,
            )
                .in_set(LspUiRenderSet),
        );
    }
}
