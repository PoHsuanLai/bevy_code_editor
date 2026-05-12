//! Editor UI plugin for rendering editor visual elements
//!
//! This plugin provides default UI rendering for the code editor including:
//! - Line numbers
//! - Selection highlights
//! - Cursor rendering and animation
//! - Bracket matching highlights
//! - Indent guides
//! - Fold indicators
//!
//! This plugin is optional - users can implement their own UI by
//! querying the editor state directly.

use bevy::prelude::*;
use bevy_instanced_text::TextFont;

use crate::settings::*;
use crate::text_view::TextViewport;
use crate::types::{CodeEditor, Separator};

/// Marker component for the editor camera
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct EditorCamera;
use super::{
    to_bevy_coords_left_aligned, update_cursor_line_highlight, update_gpu_line_numbers,
    update_indent_guides, update_selection_highlight, EditorSetupSet,
};
use bevy_instanced_text::gpu::GlyphAtlas;

use super::{update_bracket_highlight, update_bracket_match};

/// Editor UI plugin: renders line numbers, separator, cursor, selection.
/// Added automatically by `CodeEditorPlugin`.
#[derive(Default)]
pub struct EditorUiPlugin;

impl Plugin for EditorUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                init_viewport_from_window,
                compute_viewport_layout,
                setup_editor_ui,
            )
                .chain()
                .after(EditorSetupSet),
        );

        app.add_systems(Update, detect_viewport_resize);

        // Update separator position when viewport changes
        app.add_systems(Update, update_separator_on_resize.run_if(viewport_changed));

        // Update layout when UI settings change
        app.add_systems(Update, compute_viewport_layout);

        // Update font metrics when font loads
        app.add_systems(
            Update,
            update_font_metrics
                .run_if(bevy_instanced_text::gpu::atlas_ready)
                .in_set(super::RenderingSet),
        );

        // All UI rendering systems go in RenderingSet
        // GPU Line numbers (run after text display, uses same rendering pipeline)
        app.add_systems(
            Update,
            update_gpu_line_numbers
                .after(bevy_instanced_text::TextViewRenderSet)
                .run_if(bevy_instanced_text::gpu::atlas_ready)
                .in_set(super::RenderingSet),
        );

        // Overlay producers (selection, cursor-line) write into
        // `TextViewOverlays`; they must run before the engine's
        // `update_text_views` paint pass reads them.
        app.add_systems(
            Update,
            (update_selection_highlight, update_cursor_line_highlight).in_set(super::RenderingSet),
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

        // Note: cursor systems (track_cursor_movement, update_cursor, animate_cursor)
        // are registered by CursorPlugin — not duplicated here.

    }
}

type ViewportLayoutQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut TextViewport,
        &'static TextFont,
        &'static EditorUi,
    ),
    (With<CodeEditor>, Changed<EditorUi>),
>;

/// Compute ViewportDimensions layout fields based on UI settings
fn compute_viewport_layout(mut viewport_query: ViewportLayoutQuery) {
    for (mut viewport, font, ui) in viewport_query.iter_mut() {
        // Compute gutter width based on line number display
        viewport.gutter_width = if ui.show_line_numbers {
            ui.gutter_padding_left + ui.gutter_padding_right
                // Reserve space for at least 4 digits (9999 lines)
                + (font.char_width * 4.0)
        } else {
            0.0
        };

        // Text area starts past the gutter + a code margin.
        viewport.text_area_left = viewport.gutter_width + ui.code_margin_left;

        // Top margin for text area
        viewport.text_area_top = ui.margin_top;
    }
}

/// Opt-in marker: editors with this component have their `TextViewport`
/// automatically sized to the primary window. Hosts that manage viewport size
/// themselves (multi-pane, render-to-texture) simply omit this component.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct AutoResizeViewport;

/// Initialize viewport dimensions from the actual window size
fn init_viewport_from_window(
    mut viewport_query: Query<&mut TextViewport, (With<CodeEditor>, With<AutoResizeViewport>)>,
    windows: Query<&Window>,
) {
    if let Ok(window) = windows.single() {
        for mut viewport in viewport_query.iter_mut() {
            viewport.width = window.resolution.width() as u32;
            viewport.height = window.resolution.height() as u32;
        }
    }
}

/// Detect viewport resize and update dimensions
fn detect_viewport_resize(
    mut viewport_query: Query<&mut TextViewport, (With<CodeEditor>, With<AutoResizeViewport>)>,
    windows: Query<&Window>,
) {
    if let Ok(window) = windows.single() {
        let new_width = window.resolution.width() as u32;
        let new_height = window.resolution.height() as u32;

        for mut viewport in viewport_query.iter_mut() {
            if viewport.width != new_width || viewport.height != new_height {
                viewport.width = new_width;
                viewport.height = new_height;
            }
        }
    }
}

/// Setup UI entities (line numbers, cursor, separator) for each `CodeEditor`.
fn setup_editor_ui(
    mut commands: Commands,
    editor_query: Query<
        (
            &TextViewport,
            &EditorTheme,
            &EditorUi,
            Option<&bevy_camera::visibility::RenderLayers>,
        ),
        With<CodeEditor>,
    >,
) {
    for (viewport, theme, ui, render_layers) in editor_query.iter() {
        let viewport_width = viewport.width as f32;
        let viewport_height = viewport.height as f32;

        // GPU line numbers are created dynamically by update_gpu_line_numbers system
        // No need to spawn Text2d entities here

        // Spawn separator line (only if enabled)
        if ui.show_separator {
            let mut cmds = commands.spawn((
                Sprite {
                    color: theme.separator,
                    custom_size: Some(Vec2::new(1.0, viewport_height)),
                    ..default()
                },
                Transform::from_translation(to_bevy_coords_left_aligned(
                    viewport.gutter_width,
                    viewport_height / 2.0,
                    viewport_width,
                    viewport_height,
                    0.0,
                )),
                Separator,
                Name::new("Separator"),
            ));
            if let Some(layers) = render_layers {
                cmds.insert(layers.clone());
            }
        }

        // Cursor carets are pushed into TextViewOverlays each frame by
        // push_cursor_overlays — no Sprite entity to spawn here.
    }
}

/// Run condition: returns true when the TextViewport component has changed
fn viewport_changed(query: Query<(), Changed<TextViewport>>) -> bool {
    !query.is_empty()
}

/// Update separator SIZE and POSITION when viewport changes
fn update_separator_on_resize(
    viewport_query: Query<&TextViewport, With<CodeEditor>>,
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
            viewport.gutter_width,
            viewport_height / 2.0,
            viewport_width,
            viewport_height,
            0.0,
        );
    }
}

/// Update each editor's `TextFont.char_width` to match the actual rasterized
/// glyph advance for its `size`. Per-entity so multiple editors with different
/// font sizes each get accurate metrics.
fn update_font_metrics(
    mut editors: Query<&mut TextFont, With<CodeEditor>>,
    mut atlas: ResMut<GlyphAtlas>,
    fonts: Res<Assets<bevy::text::Font>>,
) {
    for mut font in editors.iter_mut() {
        let font_id = atlas.ensure_font(&font.font, &fonts);
        let width = atlas.shape_line("0", font.font_size, font_id).width;
        if width > 0.0 && (font.char_width - width).abs() > 0.01 {
            info!(
                "Updating font char_width from {:.3} to {:.3} (measured)",
                font.char_width, width
            );
            font.char_width = width;
        }
    }
}
