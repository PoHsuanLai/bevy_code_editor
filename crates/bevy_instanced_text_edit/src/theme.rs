//! Per-entity edit-affordance colors: the caret and the selection band.
//!
//! Lives here (not in `bevy_instanced_text`) because cursors and selections
//! are editing concepts. Pure rendering colors (background, foreground)
//! live on `bevy_instanced_text::RenderTheme`; line numbers, brackets,
//! indent guides, etc. live on the editor crate.

use bevy::prelude::*;

#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct EditTheme {
    pub cursor: Color,
    pub selection_background: Color,
}

impl Default for EditTheme {
    fn default() -> Self {
        Self {
            cursor: Color::srgb(0.933, 0.933, 0.933),
            selection_background: Color::srgba(0.231, 0.373, 0.604, 0.4),
        }
    }
}
