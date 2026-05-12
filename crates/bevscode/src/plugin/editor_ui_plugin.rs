//! Editor UI plugin for rendering editor visual elements

use bevy::prelude::*;
use bevy_instanced_text::TextFont;

use crate::settings::*;
use crate::text_view::TextViewport;
use crate::types::{CodeEditor, Separator};

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
            setup_editor_ui.after(EditorSetupSet),
        );

        // AutoResizeViewport: keep Node::width/height in Val::Px sync with the window.
        // This runs every frame so window resizes are picked up automatically.
        app.add_systems(Update, sync_node_from_window);

        // Update separator position when viewport changes (now driven by sync_viewport_from_node)
        app.add_systems(Update, update_separator_on_resize.run_if(viewport_changed));

        // Update gutter_width on the TextViewport when UI settings change.
        // sync_viewport_from_node owns width/height/margins; bevscode owns gutter_width.
        app.add_systems(Update, sync_gutter_width);

        // Update font metrics when font loads
        app.add_systems(
            Update,
            update_font_metrics
                .run_if(bevy_instanced_text::gpu::atlas_ready)
                .in_set(super::RenderingSet),
        );

        app.add_systems(
            Update,
            update_gpu_line_numbers
                .after(bevy_instanced_text::TextViewRenderSet)
                .run_if(bevy_instanced_text::gpu::atlas_ready)
                .in_set(super::RenderingSet),
        );

        app.add_systems(
            Update,
            (update_selection_highlight, update_cursor_line_highlight).in_set(super::RenderingSet),
        );

        app.add_systems(
            Update,
            update_indent_guides
                .after(update_gpu_line_numbers)
                .in_set(super::RenderingSet),
        );

        app.add_systems(
            Update,
            (update_bracket_match, update_bracket_highlight)
                .chain()
                .after(update_indent_guides)
                .in_set(super::RenderingSet),
        );
    }
}

/// Opt-in marker: editors with this component have their `Node` automatically
/// sized to the primary window via a full-screen `Val::Percent(100.0)` node.
/// Hosts that manage layout themselves (multi-pane, render-to-texture) omit this.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct AutoResizeViewport;

/// Keep `Node` pixel size in sync with the primary window for `AutoResizeViewport` editors.
/// Val::Px is used (not Val::Percent) so Bevy UI layout can resolve the size without
/// needing a UI camera to compute percentages against.
fn sync_node_from_window(
    mut editors: Query<&mut Node, (With<CodeEditor>, With<AutoResizeViewport>)>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let Ok(window) = windows.single() else { return };
    let w = window.width();
    let h = window.height();
    for mut node in editors.iter_mut() {
        let target_w = Val::Px(w);
        let target_h = Val::Px(h);
        if node.width != target_w || node.height != target_h {
            node.width = target_w;
            node.height = target_h;
        }
    }
}

/// Sync `TextViewport.gutter_width` from `EditorUi` + `TextFont`.
/// The engine's `sync_viewport_from_node` owns width/height/margins from `Node`;
/// this system owns the gutter_width field which has no `Node` equivalent.
fn sync_gutter_width(
    mut editors: Query<
        (&mut TextViewport, &TextFont, &EditorUi),
        (With<CodeEditor>, Changed<EditorUi>),
    >,
) {
    for (mut viewport, font, ui) in editors.iter_mut() {
        let gutter_width = if ui.show_line_numbers {
            ui.gutter_padding_left + ui.gutter_padding_right + (font.char_width * 4.0)
        } else {
            0.0
        };
        if (viewport.gutter_width - gutter_width).abs() > 0.01 {
            viewport.gutter_width = gutter_width;
        }
    }
}

/// Setup UI entities (separator) for each `CodeEditor`.
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
    }
}

fn viewport_changed(query: Query<(), Changed<TextViewport>>) -> bool {
    !query.is_empty()
}

fn update_separator_on_resize(
    viewport_query: Query<&TextViewport, With<CodeEditor>>,
    mut separator_query: Query<(&mut Sprite, &mut Transform), With<Separator>>,
) {
    let Some(viewport) = viewport_query.iter().next() else {
        return;
    };

    for (mut sprite, mut transform) in separator_query.iter_mut() {
        let viewport_height = viewport.height as f32;
        let viewport_width = viewport.width as f32;

        sprite.custom_size = Some(Vec2::new(1.0, viewport_height));
        transform.translation = to_bevy_coords_left_aligned(
            viewport.gutter_width,
            viewport_height / 2.0,
            viewport_width,
            viewport_height,
            0.0,
        );
    }
}

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
