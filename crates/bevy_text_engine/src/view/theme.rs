//! Per-entity render colors. Pure rendering — no edit affordances.
//!
//! Background and foreground belong to the rendering substrate: a terminal
//! wants them, a markdown viewer wants them, the editor wants them. Cursor
//! and selection colors live on `bevy_text_editor::EditTheme` (edit-tier);
//! line-numbers / brackets / indent-guides live on the editor crate
//! (editor-tier).

use bevy::prelude::*;

#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct RenderTheme {
    pub background: Color,
    pub foreground: Color,
}

impl Default for RenderTheme {
    fn default() -> Self {
        Self {
            background: Color::srgb(0.117, 0.117, 0.117),
            foreground: Color::srgb(0.827, 0.827, 0.827),
        }
    }
}
