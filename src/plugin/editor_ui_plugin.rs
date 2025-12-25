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

use crate::types::{LineNumbers, EditorCursor, Separator, ViewportDimensions, CodeEditorState};
use crate::settings::*;
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
    scrollbar::update_editor_scrollbar,
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
        Self
    }
}

impl Plugin for EditorUiPlugin {
    fn build(&self, app: &mut App) {
        // Startup: compute layout and spawn UI entities
        app.add_systems(Startup, (
            compute_viewport_layout,
            setup_editor_ui,
        ).chain().after(EditorSetupSet));

        // Update layout when UI settings change
        app.add_systems(Update, compute_viewport_layout.run_if(resource_changed::<UiSettings>));

        // All UI rendering systems go in RenderingSet
        // Line numbers and fold indicators (run after text display)
        app.add_systems(
            Update,
            (
                update_line_numbers,
                update_fold_indicators,
            )
                .chain()
                .after(update_gpu_text_display)
                .in_set(super::RenderingSet),
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
                .after(update_line_numbers)
                .in_set(super::RenderingSet),
        );

        // Minimap input goes in InputSet
        app.add_systems(
            Update,
            (
                update_minimap_hover,
                handle_minimap_mouse,
            )
                .chain()
                .in_set(super::InputSet),
        );

        // Minimap rendering goes in RenderingSet
        app.add_systems(
            Update,
            (
                update_minimap,
                update_minimap_find_highlights,
            )
                .chain()
                .after(update_find_highlights)
                .in_set(super::RenderingSet),
        );

        // Editor scrollbar config update goes in ApplyStateSet
        app.add_systems(
            Update,
            update_editor_scrollbar
                .run_if(resource_changed::<CodeEditorState>
                    .or(resource_changed::<ViewportDimensions>)
                    .or(resource_changed::<ScrollbarSettings>))
                .in_set(super::ApplyStateSet),
        );

        // Cursor systems in RenderingSet
        app.add_systems(
            Update,
            (
                update_cursor,
                animate_cursor,
            )
                .chain()
                .after(update_minimap_find_highlights)
                .in_set(super::RenderingSet),
        );
    }
}

/// Compute ViewportDimensions layout fields based on UI settings
fn compute_viewport_layout(
    mut viewport: ResMut<ViewportDimensions>,
    ui: Res<UiSettings>,
    font: Res<FontSettings>,
) {
    // Compute gutter width based on line number display
    viewport.gutter_width = if ui.show_line_numbers {
        ui.gutter_padding_left + ui.gutter_padding_right
            // Reserve space for at least 4 digits (9999 lines)
            + (font.char_width * 4.0)
    } else {
        0.0
    };

    // Compute separator position (right edge of gutter)
    viewport.separator_x = viewport.gutter_width;

    // Compute text area left position (gutter + code margin)
    viewport.text_area_left = viewport.gutter_width + ui.code_margin_left;

    // Top margin for text area
    viewport.text_area_top = ui.margin_top;
}

/// Setup UI entities (line numbers, cursor, separator)
fn setup_editor_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut font: ResMut<FontSettings>,
    theme: Res<ThemeSettings>,
    cursor_settings: Res<CursorSettings>,
    ui: Res<UiSettings>,
    viewport: Res<ViewportDimensions>,
) {
    // Load font
    let font_handle: Handle<Font> = asset_server.load(&font.family);
    font.handle = Some(font_handle.clone());

    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    // Spawn line numbers
    commands.spawn((
        Text2d::new("1"),
        TextFont {
            font: font_handle.clone(),
            font_size: font.size,
            ..default()
        },
        TextColor(theme.line_numbers),
        Transform::from_translation(to_bevy_coords_dynamic(
            viewport.gutter_width / 2.0,
            viewport.text_area_top,
            viewport_width,
            viewport_height,
            viewport.offset_x,
        )),
        LineNumbers,
        Name::new("LineNumbers"),
    ));

    // Spawn separator line (only if enabled)
    if ui.show_separator {
        commands.spawn((
            Sprite {
                color: theme.separator,
                custom_size: Some(Vec2::new(1.0, viewport_height)),
                ..default()
            },
            Transform::from_translation(to_bevy_coords_left_aligned(
                viewport.separator_x,
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
    let cursor_height = font.line_height * cursor_settings.height_multiplier;
    commands.spawn((
        Sprite {
            color: theme.cursor,
            custom_size: Some(Vec2::new(cursor_settings.width, cursor_height)),
            ..default()
        },
        Transform::from_translation(to_bevy_coords_dynamic(
            viewport.text_area_left,
            viewport.text_area_top,
            viewport_width,
            viewport_height,
            viewport.offset_x,
        )),
        Visibility::Hidden,
        EditorCursor { cursor_index: 0 },
        Name::new("EditorCursor_0"),
    ));
}
