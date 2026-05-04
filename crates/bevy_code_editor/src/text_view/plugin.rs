//! Editor-side interaction plugin for `TextView` entities.
//!
//! Registers mouse-wheel scroll, click + drag selection, and clipboard
//! copy systems for any entity carrying [`bevy_text_engine::TextView`].
//! The rendering plugin and marker component live in the engine crate
//! ([`bevy_text_engine::TextEnginePlugin`] / [`bevy_text_engine::TextEnginePlugins`]).
//!
//! Interaction state (`TextViewDragState`, `TextViewSelectionState`) is
//! attached by [`crate::types::editor::CodeEditor`]'s `#[require]`
//! cascade. Plain `TextView` entities (chat panels, log viewers) don't
//! get those components by default — host code can attach them
//! explicitly when interactivity is desired.

use bevy::prelude::*;

/// Plugin registering mouse + keyboard interaction systems for `TextView`
/// entities. Pair with [`bevy_text_engine::TextEnginePlugins`] which
/// supplies the rendering side.
#[derive(Default)]
pub struct TextInteractionPlugin;

impl Plugin for TextInteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                super::interaction::handle_text_view_scroll,
                super::interaction::handle_text_view_mouse,
                super::interaction::handle_text_view_copy,
            ),
        );
    }
}
