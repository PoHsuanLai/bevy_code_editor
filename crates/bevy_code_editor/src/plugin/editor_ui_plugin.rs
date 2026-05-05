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
use bevy_text_engine::FontConfig;

use crate::settings::*;
use crate::text_view::{TextViewState, TextViewViewport};
use crate::types::{
    CodeEditor, Separator, ViewportConfig, ViewportDimensions,
};
use bevy_camera::Viewport;

/// Marker component for the editor camera
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct EditorCamera;
use super::{
    to_bevy_coords_left_aligned, update_cursor_line_highlight,
    update_gpu_line_numbers, update_indent_guides,
    update_selection_highlight, EditorSetupSet,
};
use bevy_text_engine::gpu::GlyphAtlas;

use super::{update_bracket_highlight, update_bracket_match};

use super::update_fold_indicators;

use super::scrollbar::update_editor_scrollbar;

/// Resource to store render layer configuration for the editor
// not reflectable: holds `RenderLayers`, which is not derived `Reflect` in
// bevy 0.17. Skipping this resource.
#[derive(Resource, Clone, Default)]
pub struct EditorRenderConfig {
    /// Optional render layer for editor entities.
    /// If Some(layer), all editor UI entities will only render to cameras on that layer.
    /// If None, entities render to all cameras (default behavior).
    pub render_layers: Option<RenderLayers>,
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
/// and query the editor's components and resources directly.
#[derive(Default)]
pub struct EditorUiPlugin {
    /// Optional render layer for editor entities
    pub render_layers: Option<RenderLayers>,
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
                init_viewport_from_window,
                setup_editor_camera,
                compute_viewport_layout,
                setup_editor_ui,
                stamp_editor_render_layers,
            )
                .chain()
                .after(EditorSetupSet),
        );

        // Update viewport when window resizes (if auto_resize_to_window is true)
        // or sync from ViewportDimensions resource (if auto_resize_to_window is false)
        app.add_systems(
            Update,
            (detect_viewport_resize, sync_viewport_from_resource),
        );

        // Update separator position when viewport changes
        app.add_systems(Update, update_separator_on_resize.run_if(viewport_changed));

        // Update layout when UI settings change
        app.add_systems(
            Update,
            compute_viewport_layout.run_if(resource_changed::<UiSettings>),
        );

        // Update font metrics when font loads
        app.add_systems(
            Update,
            update_font_metrics
                .run_if(bevy_text_engine::gpu::atlas_ready)
                .in_set(super::RenderingSet),
        );

        // All UI rendering systems go in RenderingSet
        // GPU Line numbers (run after text display, uses same rendering pipeline)
        app.add_systems(
            Update,
            update_gpu_line_numbers
                .after(bevy_text_engine::TextViewRenderSet)
                .run_if(bevy_text_engine::gpu::atlas_ready)
                .in_set(super::RenderingSet),
        );

        // Fold indicators (feature-gated)
        app.add_systems(
            Update,
            update_fold_indicators
                .after(update_gpu_line_numbers)
                .in_set(super::RenderingSet),
        );

        // Overlay producers (selection, cursor-line) write into
        // `TextViewOverlays`; they must run before the engine's
        // `update_text_views` paint pass reads them.
        app.add_systems(
            Update,
            (update_selection_highlight, update_cursor_line_highlight)
                .in_set(super::RenderingSet),
        );

        // Indent guides still use Sprite entities — they could be migrated to
        // overlay rects, but Sprite entities are fine for the small fixed cost.
        app.add_systems(
            Update,
            update_indent_guides
                .after(update_gpu_line_numbers)
                .in_set(super::RenderingSet),
        );

        // Bracket matching (feature-gated)
        app.add_systems(
            Update,
            (update_bracket_match, update_bracket_highlight)
                .chain()
                .after(update_indent_guides)
                .in_set(super::RenderingSet),
        );

        // Editor scrollbar config update (feature-gated)
        // Run when scroll/content changes, viewport resizes, or scrollbar settings change.
        // Replaces the previous `Changed<CodeEditorState>` filter (now removed —
        // `TextViewState` is the actual mutable scroll/content carrier).
        app.add_systems(
            Update,
            update_editor_scrollbar
                .run_if(
                    (|query: Query<(), (With<CodeEditor>, Changed<TextViewState>)>| {
                        !query.is_empty()
                    })
                    .or(viewport_changed)
                    .or(resource_changed::<ScrollbarSettings>),
                )
                .in_set(super::ApplyStateSet),
        );

        // Note: cursor systems (track_cursor_movement, update_cursor, animate_cursor)
        // are registered by CursorPlugin — not duplicated here.

        // Camera viewport update (for restricted rendering when not full-window)
        // Run on PostStartup to set initial viewport, then on Update when ViewportDimensions changes
        app.add_systems(PostStartup, update_camera_viewport);
        app.add_systems(
            Update,
            update_camera_viewport
                .run_if(|query: Query<(), Changed<TextViewViewport>>| !query.is_empty())
                .in_set(super::ApplyStateSet),
        );
    }
}

/// Update camera viewport to restrict rendering to the editor panel bounds
/// This is essential when auto_resize_to_window is false (e.g., resizable panel mode)
fn update_camera_viewport(
    config: Res<ViewportConfig>,
    viewport_query: Query<&TextViewViewport, With<CodeEditor>>,
    windows: Query<&Window>,
    mut camera_query: Query<(&mut Camera, &mut Transform), With<EditorCamera>>,
) {
    // Only set camera viewport when NOT auto-resizing (manual viewport control)
    if config.auto_resize_to_window {
        // Clear any existing viewport restriction and reset camera position
        for (mut camera, mut transform) in camera_query.iter_mut() {
            if camera.viewport.is_some() {
                camera.viewport = None;
            }
            transform.translation = Vec3::ZERO;
        }
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };

    for viewport in viewport_query.iter() {
        // Convert from center-origin viewport coordinates to top-left origin window coordinates.
        // - ViewportOrigin::ScreenAbsolute(p) → p is the panel's LEFT/TOP world-space edges.
        // - ViewportOrigin::CenteredOrtho → panel is centered at world (0,0).
        // viewport.width/height are the panel dimensions.
        let window_width = window.width();
        let window_height = window.height();
        let scale_factor = window.scale_factor();

        // Calculate top-left corner of panel in window coordinates
        // When offset is (0,0), panel is centered in window (auto-resize mode)
        // When offset is non-zero, it represents the panel's left/top edges (resizable mode)
        let panel_left_world = viewport.world_left();
        let panel_top_world = viewport.world_top();

        // Convert world coords to window coords (window: 0,0 = top-left, Y down)
        // Then scale to physical pixels (Viewport uses physical coordinates)
        // Window X = world X + window_width/2
        // Window Y = window_height/2 - world Y
        let window_x = ((panel_left_world + window_width / 2.0).max(0.0) * scale_factor) as u32;
        let window_y = (((window_height / 2.0 - panel_top_world).max(0.0)) * scale_factor) as u32;
        let physical_width = (viewport.width as f32 * scale_factor) as u32;
        let physical_height = (viewport.height as f32 * scale_factor) as u32;

        // Calculate camera position (center of panel)
        let camera_x = panel_left_world + viewport.width as f32 / 2.0;
        let camera_y = panel_top_world - viewport.height as f32 / 2.0;

        for (mut camera, mut transform) in camera_query.iter_mut() {
            // Set the camera viewport to restrict which window pixels to render to
            camera.viewport = Some(Viewport {
                physical_position: UVec2::new(window_x, window_y),
                physical_size: UVec2::new(physical_width, physical_height),
                ..default()
            });

            // Move the camera to the panel center
            // Content is positioned relative to camera at (0,0)
            transform.translation = Vec3::new(camera_x, camera_y, transform.translation.z);
        }
    }
}

/// Compute ViewportDimensions layout fields based on UI settings
fn compute_viewport_layout(
    mut viewport_query: Query<(&mut TextViewViewport, &FontConfig), With<CodeEditor>>,
    ui: Res<UiSettings>,
) {
    for (mut viewport, font) in viewport_query.iter_mut() {
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
}

/// Helper function to apply render layers to an entity if configured
fn apply_render_layers(entity: &mut EntityCommands, config: &EditorRenderConfig) {
    if let Some(ref layers) = config.render_layers {
        entity.insert(layers.clone());
    }
}

/// Stamp the configured `RenderLayers` onto the `CodeEditor` entity at startup.
///
/// `text_view::update_text_views` reads `RenderLayers` from the source entity
/// to propagate them to the spawned batch entity, which is how multi-viewport
/// camera filtering works. Without this, the editor's text would render to all
/// cameras (including the host app's main camera) when `EditorRenderConfig`
/// configures a dedicated layer.
fn stamp_editor_render_layers(
    mut commands: Commands,
    editor_query: Query<Entity, With<CodeEditor>>,
    config: Res<EditorRenderConfig>,
) {
    let Some(layers) = config.render_layers.as_ref() else {
        return;
    };
    for entity in &editor_query {
        commands.entity(entity).insert(layers.clone());
    }
}

/// Setup camera for standalone editor mode (only if not using render layers)
/// Initialize viewport dimensions from the actual window size
fn init_viewport_from_window(
    mut viewport_query: Query<&mut TextViewViewport, With<CodeEditor>>,
    config: Res<ViewportConfig>,
    windows: Query<&Window>,
) {
    // Only auto-initialize if auto_resize_to_window is true
    if !config.auto_resize_to_window {
        return;
    }

    if let Ok(window) = windows.single() {
        for mut viewport in viewport_query.iter_mut() {
            viewport.width = window.resolution.width() as u32;
            viewport.height = window.resolution.height() as u32;
        }
    }
}

/// Detect viewport resize and update dimensions
fn detect_viewport_resize(
    config: Res<ViewportConfig>,
    mut viewport_query: Query<&mut TextViewViewport, With<CodeEditor>>,
    windows: Query<&Window>,
) {
    // Only auto-resize when enabled
    if !config.auto_resize_to_window {
        return;
    }

    if let Ok(window) = windows.single() {
        let new_width = window.resolution.width() as u32;
        let new_height = window.resolution.height() as u32;

        for mut viewport in viewport_query.iter_mut() {
            // Only update if changed to avoid unnecessary change detection
            if viewport.width != new_width || viewport.height != new_height {
                viewport.width = new_width;
                viewport.height = new_height;
            }
        }
    }
}

/// Sync ViewportDimensions resource → TextViewViewport component.
/// When auto_resize_to_window is false, the host app sets ViewportDimensions
/// and this system propagates size to the per-entity TextViewViewport.
/// `origin` is NOT propagated — in render-to-texture mode the camera is
/// centered at origin, so TextViewViewport keeps `ViewportOrigin::CenteredOrtho`.
fn sync_viewport_from_resource(
    config: Res<ViewportConfig>,
    dims: Res<ViewportDimensions>,
    mut viewport_query: Query<&mut TextViewViewport, With<CodeEditor>>,
) {
    if config.auto_resize_to_window {
        return;
    }
    for mut viewport in viewport_query.iter_mut() {
        if viewport.width != dims.width || viewport.height != dims.height {
            viewport.width = dims.width;
            viewport.height = dims.height;
        }
    }
}

fn setup_editor_camera(mut commands: Commands, render_config: Res<EditorRenderConfig>) {
    // Only spawn camera if NOT using render layers (standalone mode)
    // When using render layers, the host application manages cameras
    // and is responsible for the clear color too.
    if render_config.render_layers.is_none() {
        // Match the per-entity ThemeConfig::default() background; hosts that
        // theme their editors differently are expected to manage their own
        // camera and clear color.
        let bg = ThemeConfig::default().background;
        commands.spawn((
            Camera2d,
            Projection::Orthographic(OrthographicProjection {
                scale: 1.0, // 1:1 world units to pixels
                ..OrthographicProjection::default_2d()
            }),
            Camera {
                clear_color: ClearColorConfig::Custom(bg),
                ..default()
            },
            EditorCamera,
            Name::new("EditorCamera"),
        ));
    }
}

/// Setup UI entities (line numbers, cursor, separator) for each `CodeEditor`.
fn setup_editor_ui(
    mut commands: Commands,
    _cursor_settings: Res<CursorSettings>,
    ui: Res<UiSettings>,
    editor_query: Query<(&TextViewViewport, &ThemeConfig), With<CodeEditor>>,
    render_config: Res<EditorRenderConfig>,
) {
    for (viewport, theme) in editor_query.iter() {
        let viewport_width = viewport.width as f32;
        let viewport_height = viewport.height as f32;

        // GPU line numbers are created dynamically by update_gpu_line_numbers system
        // No need to spawn Text2d entities here

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
                    0.0,
                )),
                Separator,
                Name::new("Separator"),
            ));
            apply_render_layers(&mut separator, &render_config);
        }

        // Cursor carets are pushed into TextViewOverlays each frame by
        // push_cursor_overlays — no Sprite entity to spawn here.
    }
}

/// Run condition: returns true when the TextViewViewport component has changed
fn viewport_changed(query: Query<(), Changed<TextViewViewport>>) -> bool {
    !query.is_empty()
}

/// Update separator SIZE and POSITION when viewport changes
fn update_separator_on_resize(
    viewport_query: Query<&TextViewViewport, With<CodeEditor>>,
    mut separator_query: Query<(&mut Sprite, &mut Transform), With<Separator>>,
) {
    // Use the first viewport found; with multiple editors a follow-up will key
    // separator entities to specific editors.
    let Some(viewport) = viewport_query.iter().next() else {
        return;
    };

    for (mut sprite, mut transform) in separator_query.iter_mut() {
        let viewport_height = viewport.height as f32;
        let viewport_width = viewport.width as f32;

        // Only update separator height - position stays fixed relative to camera
        sprite.custom_size = Some(Vec2::new(1.0, viewport_height));

        // Update position (critical when viewport width changes or offset changes)
        transform.translation = to_bevy_coords_left_aligned(
            viewport.separator_x,
            viewport_height / 2.0,
            viewport_width,
            viewport_height,
            0.0,
        );
    }
}

/// Update each editor's `FontConfig.char_width` to match the actual rasterized
/// glyph advance for its `size`. Per-entity so multiple editors with different
/// font sizes each get accurate metrics.
fn update_font_metrics(
    mut editors: Query<&mut FontConfig, With<CodeEditor>>,
    mut atlas: ResMut<GlyphAtlas>,
    fonts: Res<Assets<bevy::text::Font>>,
) {
    for mut font in editors.iter_mut() {
        // Measure '0' for monospace width. Shape via cosmic-text — shape.width
        // is the exact advance. Resolves the entity's Handle<Font> if set.
        let font_id = font.font.as_ref().and_then(|h| atlas.ensure_font(h, &fonts));
        let width = atlas.shape_line("0", font.size, font_id).width;
        if width > 0.0 && (font.char_width - width).abs() > 0.01 {
            info!(
                "Updating font char_width from {:.3} to {:.3} (measured)",
                font.char_width, width
            );
            font.char_width = width;
        }
    }
}
