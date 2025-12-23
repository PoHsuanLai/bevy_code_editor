//! Editor UI plugin for rendering editor visual elements
//!
//! This plugin provides default UI rendering for the code editor including:
//! - Line numbers
//! - Selection highlights
//! - Cursor rendering and animation
//! - Bracket matching highlights
//! - Find/replace highlights
//! - Indent guides
//! - Fold indicators
//! - Minimap
//!
//! This plugin is optional - users can implement their own UI by
//! querying the editor state directly.

use bevy::prelude::*;

use crate::types::{LineNumbers, EditorCursor, Separator, ViewportDimensions};
use crate::settings::EditorSettings;
use super::{
    update_line_numbers, update_fold_indicators,
    update_selection_highlight, update_cursor_line_highlight,
    update_indent_guides, update_bracket_match, update_bracket_highlight,
    update_find_highlights, update_minimap_hover, handle_minimap_mouse,
    update_minimap, update_minimap_find_highlights,
    update_cursor, animate_cursor,
    to_bevy_coords_dynamic, to_bevy_coords_left_aligned,
    EditorSetupSet,
    update_gpu_text_display,
};

/// Editor UI plugin providing default rendering for editor visual elements
///
/// This plugin must be added AFTER CodeEditorPlugin.
/// It renders line numbers, selection, cursor, etc.
///
/// # Example
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_code_editor::prelude::*;
///
/// App::new()
///     .add_plugins(CodeEditorPlugin::default())
///     .add_plugins(EditorUiPlugin::default())
///     .run();
/// ```
///
/// # Custom UI
/// If you want to implement your own UI, simply don't add this plugin
/// and query CodeEditorState and other resources directly.
pub struct EditorUiPlugin;

impl Default for EditorUiPlugin {
    fn default() -> Self {
        Self
    }
}

impl EditorUiPlugin {
    /// Create a new Editor UI plugin
    pub fn new() -> Self {
        Self::default()
    }
}

impl Plugin for EditorUiPlugin {
    fn build(&self, app: &mut App) {
        // Startup: spawn UI entities (must run after CodeEditorPlugin's setup for viewport)
        app.add_systems(Startup, setup_editor_ui.after(EditorSetupSet));

        // Line numbers and fold indicators (run after text display)
        app.add_systems(
            Update,
            (
                update_line_numbers,
                update_fold_indicators,
            )
                .chain()
                .after(update_gpu_text_display),
        );

        // Selection and highlighting systems
        app.add_systems(
            Update,
            (
                update_selection_highlight,
                update_cursor_line_highlight,
                update_indent_guides,
                update_bracket_match,
                update_bracket_highlight,
                update_find_highlights,
            )
                .chain()
                .after(update_line_numbers),
        );

        // Minimap systems
        app.add_systems(
            Update,
            update_minimap_hover.after(update_find_highlights),
        );
        app.add_systems(
            Update,
            handle_minimap_mouse.after(update_minimap_hover),
        );
        app.add_systems(
            Update,
            update_minimap.after(handle_minimap_mouse),
        );
        app.add_systems(
            Update,
            update_minimap_find_highlights.after(update_minimap),
        );

        // Cursor systems
        app.add_systems(
            Update,
            update_cursor.after(update_minimap_find_highlights),
        );
        app.add_systems(
            Update,
            animate_cursor.after(update_cursor),
        );
    }
}

/// Setup UI entities (line numbers, cursor, separator)
fn setup_editor_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut settings: ResMut<EditorSettings>,
    viewport: Res<ViewportDimensions>,
) {
    // Load font
    let font_handle: Handle<Font> = asset_server.load(&settings.font.family);
    settings.font.handle = Some(font_handle.clone());

    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    // Spawn line numbers
    commands.spawn((
        Text2d::new("1"),
        TextFont {
            font: font_handle.clone(),
            font_size: settings.font.size,
            ..default()
        },
        TextColor(settings.theme.line_numbers),
        Transform::from_translation(to_bevy_coords_dynamic(
            settings.ui.layout.line_number_margin_left,
            settings.ui.layout.margin_top,
            viewport_width,
            viewport_height,
            viewport.offset_x,
        )),
        LineNumbers,
        Name::new("LineNumbers"),
    ));

    // Spawn separator line (only if enabled)
    if settings.ui.show_separator {
        commands.spawn((
            Sprite {
                color: settings.theme.separator,
                custom_size: Some(Vec2::new(1.0, viewport_height)),
                ..default()
            },
            Transform::from_translation(to_bevy_coords_left_aligned(
                settings.ui.layout.separator_x,
                viewport_height / 2.0,
                viewport_width,
                viewport_height,
                viewport.offset_x,
                0.0,  // separator doesn't scroll horizontally
            )),
            Separator,
            Name::new("Separator"),
        ));
    }

    // Spawn primary cursor (cursor_index = 0)
    let cursor_height = settings.font.line_height * settings.cursor.height_multiplier;
    commands.spawn((
        Sprite {
            color: settings.theme.cursor,
            custom_size: Some(Vec2::new(settings.cursor.width, cursor_height)),
            ..default()
        },
        Transform::from_translation(to_bevy_coords_dynamic(
            settings.ui.layout.code_margin_left,
            settings.ui.layout.margin_top,
            viewport_width,
            viewport_height,
            viewport.offset_x,
        )),
        Visibility::Hidden,
        EditorCursor { cursor_index: 0 },
        Name::new("EditorCursor_0"),
    ));
}
