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
// Import RenderLayers from bevy_camera crate directly
use bevy_camera::visibility::RenderLayers;

use crate::settings::*;
use crate::types::{
    CodeEditorState, EditorCursor, LineNumbers, Separator, ViewportConfig, ViewportDimensions,
};
use bevy_camera::Viewport;

/// Marker component for the editor camera
#[derive(Component)]
pub struct EditorCamera;
use super::{
    animate_cursor, handle_minimap_mouse, scrollbar::update_editor_scrollbar,
    to_bevy_coords_dynamic, to_bevy_coords_left_aligned, track_cursor_movement,
    update_bracket_highlight, update_bracket_match, update_cursor, update_cursor_line_highlight,
    update_fold_indicators, update_gpu_text_display, update_indent_guides, update_line_numbers,
    update_minimap, update_minimap_hover, update_selection_highlight, EditorSetupSet,
};

/// Resource to store render layer configuration for the editor
#[derive(Resource, Clone)]
pub struct EditorRenderConfig {
    /// Optional render layer for editor entities.
    /// If Some(layer), all editor UI entities will only render to cameras on that layer.
    /// If None, entities render to all cameras (default behavior).
    pub render_layers: Option<RenderLayers>,
}

impl Default for EditorRenderConfig {
    fn default() -> Self {
        Self {
            render_layers: None,
        }
    }
}

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
/// # Render to Texture Example
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_camera::visibility::RenderLayers;
/// use bevy_code_editor::prelude::*;
///
/// App::new()
///     .add_plugins(CodeEditorPlugin::default())
///     .add_plugins(EditorUiPlugin::with_render_layer(RenderLayers::layer(1)))
///     .run();
/// ```
///
/// # Custom UI
/// If you want to implement your own UI, simply don't add this plugin
/// and query CodeEditorState and other resources directly.
pub struct EditorUiPlugin {
    /// Optional render layer for editor entities
    pub render_layers: Option<RenderLayers>,
}

impl Default for EditorUiPlugin {
    fn default() -> Self {
        Self {
            render_layers: None,
        }
    }
}

impl EditorUiPlugin {
    /// Create a new Editor UI plugin
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an Editor UI plugin that only renders to a specific camera layer
    ///
    /// This is useful for render-to-texture scenarios where you want the editor
    /// to only appear in a specific camera's view.
    pub fn with_render_layer(render_layers: RenderLayers) -> Self {
        Self {
            render_layers: Some(render_layers),
        }
    }
}

impl Plugin for EditorUiPlugin {
    fn build(&self, app: &mut App) {
        // Insert render configuration as a resource
        app.insert_resource(EditorRenderConfig {
            render_layers: self.render_layers.clone(),
        });

        // Startup: setup camera (only if not using render layers), compute layout, and spawn UI entities
        app.add_systems(
            Startup,
            (
                setup_editor_camera,
                compute_viewport_layout,
                setup_editor_ui,
            )
                .chain()
                .after(EditorSetupSet),
        );

        // Update layout when UI settings change
        app.add_systems(
            Update,
            compute_viewport_layout.run_if(resource_changed::<UiSettings>),
        );

        // All UI rendering systems go in RenderingSet
        // Line numbers and fold indicators (run after text display)
        app.add_systems(
            Update,
            (
                update_line_numbers,
                // update_fold_indicators,
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
            )
                .chain()
                .after(update_line_numbers)
                .in_set(super::RenderingSet),
        );

        // Minimap input goes in InputSet
        app.add_systems(
            Update,
            (update_minimap_hover, handle_minimap_mouse)
                .chain()
                .in_set(super::InputSet),
        );

        // Minimap rendering goes in RenderingSet
        app.add_systems(Update, update_minimap.in_set(super::RenderingSet));

        // Editor scrollbar config update goes in ApplyStateSet
        app.add_systems(
            Update,
            update_editor_scrollbar
                .run_if(
                    resource_changed::<CodeEditorState>
                        .or(resource_changed::<ViewportDimensions>)
                        .or(resource_changed::<ScrollbarSettings>),
                )
                .in_set(super::ApplyStateSet),
        );

        // Track cursor movement for blink reset (must run before cursor rendering)
        app.add_systems(Update, track_cursor_movement.in_set(super::ApplyStateSet));

        // Cursor systems in RenderingSet
        app.add_systems(
            Update,
            (update_cursor, animate_cursor)
                .chain()
                .in_set(super::RenderingSet),
        );

        // Camera viewport update (for restricted rendering when not full-window)
        // Run on PostStartup to set initial viewport, then on Update when ViewportDimensions changes
        app.add_systems(PostStartup, update_camera_viewport);
        app.add_systems(
            Update,
            update_camera_viewport
                .run_if(resource_changed::<ViewportDimensions>)
                .in_set(super::ApplyStateSet),
        );
    }
}

/// Update camera viewport to restrict rendering to the editor panel bounds
/// This is essential when auto_resize_to_window is false (e.g., resizable panel mode)
fn update_camera_viewport(
    config: Res<ViewportConfig>,
    viewport: Res<ViewportDimensions>,
    windows: Query<&Window>,
    mut camera_query: Query<&mut Camera, With<EditorCamera>>,
) {
    // Only set camera viewport when NOT auto-resizing (manual viewport control)
    if config.auto_resize_to_window {
        // Clear any existing viewport restriction
        for mut camera in camera_query.iter_mut() {
            if camera.viewport.is_some() {
                camera.viewport = None;
            }
        }
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };

    // Convert from center-origin viewport coordinates to top-left origin window coordinates
    // viewport.offset_x/offset_y are the center of the panel in world coords (center-origin)
    // viewport.width/height are the panel dimensions
    let window_width = window.width();
    let window_height = window.height();
    let scale_factor = window.scale_factor() as f32;

    // Calculate top-left corner of panel in window coordinates
    // World coords: center_x = offset_x, center_y = offset_y
    // Panel extends from (offset_x - width/2) to (offset_x + width/2) in X
    // and from (offset_y - height/2) to (offset_y + height/2) in Y
    let panel_left_world = viewport.offset_x - viewport.width as f32 / 2.0;
    let panel_top_world = viewport.offset_y + viewport.height as f32 / 2.0; // Top of panel in world Y

    // Convert world coords to window coords (window: 0,0 = top-left, Y down)
    // Then scale to physical pixels (Viewport uses physical coordinates)
    // Window X = world X + window_width/2
    // Window Y = window_height/2 - world Y
    let window_x = ((panel_left_world + window_width / 2.0).max(0.0) * scale_factor) as u32;
    let window_y = (((window_height / 2.0 - panel_top_world).max(0.0)) * scale_factor) as u32;
    let physical_width = (viewport.width as f32 * scale_factor) as u32;
    let physical_height = (viewport.height as f32 * scale_factor) as u32;

    for mut camera in camera_query.iter_mut() {
        camera.viewport = Some(Viewport {
            physical_position: UVec2::new(window_x, window_y),
            physical_size: UVec2::new(physical_width, physical_height),
            ..default()
        });
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

/// Helper function to apply render layers to an entity if configured
fn apply_render_layers(entity: &mut EntityCommands, config: &EditorRenderConfig) {
    if let Some(ref layers) = config.render_layers {
        entity.insert(layers.clone());
    }
}

/// Setup camera for standalone editor mode (only if not using render layers)
fn setup_editor_camera(
    mut commands: Commands,
    theme: Res<ThemeSettings>,
    render_config: Res<EditorRenderConfig>,
) {
    // Only spawn camera if NOT using render layers (standalone mode)
    // When using render layers, the host application manages cameras
    if render_config.render_layers.is_none() {
        commands.spawn((
            Camera2d,
            Projection::Orthographic(OrthographicProjection {
                scale: 1.0, // 1:1 world units to pixels
                ..OrthographicProjection::default_2d()
            }),
            Camera {
                clear_color: ClearColorConfig::Custom(theme.background),
                ..default()
            },
            EditorCamera,
            Name::new("EditorCamera"),
        ));
    }
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
    render_config: Res<EditorRenderConfig>,
) {
    // Load font
    let font_handle: Handle<Font> = asset_server.load(&font.family);
    font.handle = Some(font_handle.clone());

    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    // Spawn line numbers
    let mut line_numbers = commands.spawn((
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
            0.0, // Camera viewport handles panel positioning
        )),
        LineNumbers,
        Name::new("LineNumbers"),
    ));
    apply_render_layers(&mut line_numbers, &render_config);

    // Spawn separator line (only if enabled)
    if ui.show_separator {
        let mut separator = commands.spawn((
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
                0.0, // Camera viewport handles panel positioning
                0.0, // separator doesn't scroll horizontally
            )),
            Separator,
            Name::new("Separator"),
        ));
        apply_render_layers(&mut separator, &render_config);
    }

    // Spawn primary cursor (cursor_index = 0)
    let cursor_height = font.line_height * cursor_settings.height_multiplier;
    let mut cursor = commands.spawn((
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
            0.0, // Camera viewport handles panel positioning
        )),
        Visibility::Hidden,
        EditorCursor { cursor_index: 0 },
        Name::new("EditorCursor_0"),
    ));
    apply_render_layers(&mut cursor, &render_config);
}
