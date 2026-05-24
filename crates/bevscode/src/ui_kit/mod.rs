//! Tempera glue layer.
//!
//! Bevscode's chrome — every LSP popup, the gutter, the editor background —
//! reads its colors and metrics from tempera's [`ColorPalette`], [`Spacing`],
//! [`Typography`], [`FontHandle`], and [`MenuTokens`] resources. That is
//! the same surface tempera uses to style buttons, dropdowns, dialogs in
//! the user's other apps, so swapping the palette here flips the editor in
//! lockstep with the rest of the UI.
//!
//! Pieces:
//!
//! - [`BevscodePalettePlugin`] installs tempera's [`ThemePlugin`] (idempotent)
//!   and runs [`sync_palette_into_editor_theme`] every time the palette
//!   changes, mapping the shadcn-aligned tokens onto each `EditorTheme`.
//! - [`palette_to_editor_theme`] is the single mapping function. Editor-
//!   specific fields (selection background, line numbers, bracket pair
//!   palette, fold marker, …) have no shadcn equivalent and are left alone.
//! - [`PopupChrome`] is the `SystemParam` bundle popup renderers read so
//!   they pull one parameter instead of five.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use tempera::theme::{ColorPalette, FontHandle, MenuTokens, Spacing, ThemePlugin, Typography};

use crate::settings::EditorTheme;
use crate::types::CodeEditor;

pub mod markdown_theme;
pub use markdown_theme::markdown_theme_from_chrome;

/// Installs tempera's theme resources and keeps every `EditorTheme` in sync
/// with the active [`ColorPalette`].
pub struct BevscodePalettePlugin;

impl Plugin for BevscodePalettePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ThemePlugin>() {
            app.add_plugins(ThemePlugin);
        }
        app.add_systems(Update, sync_palette_into_editor_theme);
    }
}

/// Gated adapter for [`tempera::TemperaPlugin`].
///
/// `TemperaPlugin::build` unconditionally calls `add_plugins` on every
/// tempera widget plugin, so re-registering it panics when a downstream
/// app has already added tempera itself. `PluginGroupBuilder::add` can't
/// be made conditional from inside a `PluginGroup`, so [`CodeEditorPlugins`]
/// adds this thin wrapper instead — it forwards to `TemperaPlugin` only
/// when tempera hasn't already been installed.
///
/// [`CodeEditorPlugins`]: crate::plugin::CodeEditorPlugins
pub struct EditorTemperaPlugin;

impl Plugin for EditorTemperaPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<tempera::TemperaPlugin>() {
            app.add_plugins(tempera::TemperaPlugin);
        }
    }
}

/// Map the shadcn-aligned slots of `palette` onto the slots of `theme`
/// that have a direct equivalent. Editor-specific colors (selection,
/// gutter, bracket pairs, fold, whitespace, link) keep their existing
/// values — they're tuned per-editor and don't belong to the popover/
/// chrome vocabulary.
pub fn palette_to_editor_theme(palette: &ColorPalette, theme: &mut EditorTheme) {
    theme.background = palette.background;
    theme.foreground = palette.foreground;
    theme.separator = palette.border;
    theme.placeholder_color = palette.muted_foreground;
}

fn sync_palette_into_editor_theme(
    palette: Res<ColorPalette>,
    mut themes: ParamSet<(
        Query<&mut EditorTheme, With<CodeEditor>>,
        Query<&mut EditorTheme, (With<CodeEditor>, Added<EditorTheme>)>,
    )>,
) {
    if palette.is_changed() {
        for mut theme in themes.p0().iter_mut() {
            palette_to_editor_theme(&palette, &mut theme);
        }
    } else {
        for mut theme in themes.p1().iter_mut() {
            palette_to_editor_theme(&palette, &mut theme);
        }
    }
}

/// Read-only slice of tempera tokens consumed by popup renderers. Pulls
/// the five resources every chrome path touches in one `SystemParam`.
#[derive(SystemParam)]
pub struct PopupChrome<'w> {
    pub palette: Res<'w, ColorPalette>,
    pub spacing: Res<'w, Spacing>,
    pub typography: Res<'w, Typography>,
    pub font: Res<'w, FontHandle>,
    pub menu: Res<'w, MenuTokens>,
}

impl PopupChrome<'_> {
    /// Body-row font (typography.sm). Matches tempera's command palette
    /// and dropdown row sizes.
    #[must_use]
    pub fn body_font(&self) -> TextFont {
        self.font.text_font(self.typography.sm)
    }

    /// Bold variant of the body font — used for active signature
    /// parameters.
    #[must_use]
    pub fn body_font_bold(&self) -> TextFont {
        self.font.text_font_bold(self.typography.sm)
    }

    /// Smaller secondary font (typography.xs) — pagers, detail spans.
    #[must_use]
    pub fn small_font(&self) -> TextFont {
        self.font.text_font(self.typography.xs)
    }
}
