//! Editor UI plugin for rendering editor visual elements

use bevy::prelude::*;
use bevy_instanced_text::{MonoCellWidth, TextOverlays, TextUnderlays};

use crate::settings::*;
use crate::types::{
    BracketMatchRects, CaretRects, CodeEditor, CursorLineRects, IndentGuideRects, SelectionRects,
    Separator,
};

use super::{
    setup_gutter_text_view, sync_gutter_text_view, to_bevy_coords_left_aligned,
    update_cursor_line_highlight, update_indent_guides, update_selection_highlight, EditorSetupSet,
};
use bevy_instanced_text::gpu::GlyphAtlas;

use super::{update_bracket_highlight, update_bracket_match};

/// Editor UI plugin: renders line numbers, separator, cursor, selection.
/// Added automatically by `CodeEditorPlugin`.
#[derive(Default)]
pub struct EditorUiPlugin;

impl Plugin for EditorUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_editor_ui.after(EditorSetupSet));

        // Gutter setup runs every frame because the editor's required
        // components may not all be present during Startup; idempotent
        // via its `existing` guard.
        app.add_systems(Update, setup_gutter_text_view);

        // AutoResizeViewport: keep Node::width/height in Val::Px sync with the window.
        // This runs every frame so window resizes are picked up automatically.
        app.add_systems(Update, sync_node_from_window);

        // Update separator position when ComputedNode changes (driven by Bevy UI layout).
        app.add_systems(Update, update_separator_on_resize.run_if(viewport_changed));

        app.add_systems(
            PostUpdate,
            (
                sync_gutter_width,
                sync_gutter_text_view.after(sync_gutter_width),
            )
                .before(bevy_instanced_text::LayoutProduceSet),
        );

        app.add_systems(
            PostUpdate,
            update_font_metrics
                .run_if(bevy_instanced_text::gpu::atlas_ready)
                .in_set(super::RenderingSet),
        );

        app.add_systems(
            PostUpdate,
            (update_selection_highlight, update_cursor_line_highlight).in_set(super::RenderingSet),
        );

        app.add_systems(
            PostUpdate,
            update_indent_guides.in_set(super::RenderingSet),
        );

        // State update stays in Update; overlay producer reads DisplayLayout so it runs in PostUpdate.
        app.add_systems(
            Update,
            update_bracket_match.in_set(super::ApplyStateSet),
        );
        app.add_systems(
            PostUpdate,
            update_bracket_highlight
                .after(update_indent_guides)
                .in_set(super::RenderingSet),
        );

        app.add_systems(
            PostUpdate,
            merge_overlay_components
                .after(super::RenderingSet)
                .before(bevy_instanced_text::TextViewRenderSet),
        );
    }
}

#[allow(clippy::type_complexity)]
/// Assemble per-producer typed components into the two engine overlay components.
/// Only runs for editors where at least one source component changed.
#[allow(clippy::type_complexity)]
fn merge_overlay_components(
    mut query: Query<
        (
            &SelectionRects,
            &IndentGuideRects,
            &CursorLineRects,
            &CaretRects,
            &BracketMatchRects,
            &mut TextUnderlays,
            &mut TextOverlays,
        ),
        (
            With<CodeEditor>,
            Or<(
                Changed<SelectionRects>,
                Changed<IndentGuideRects>,
                Changed<CursorLineRects>,
                Changed<CaretRects>,
                Changed<BracketMatchRects>,
            )>,
        ),
    >,
) {
    for (sel, guides, cursor_line, carets, brackets, mut underlays, mut overlays) in &mut query {
        underlays.0.clear();
        underlays.0.extend_from_slice(&guides.0);
        underlays.0.extend_from_slice(&sel.0);

        overlays.0.clear();
        overlays.0.extend_from_slice(&cursor_line.0);
        overlays.0.extend_from_slice(&carets.0);
        overlays.0.extend_from_slice(&brackets.0);
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

/// Sync gutter geometry into `Node::padding` and `GutterConfig.gutter_width`
/// from `EditorUi` + `TextFont`.
///
/// `padding.left`  = gutter_width + code_margin_left  (→ ComputedNode::content_inset)
/// `padding.top`   = margin_top                        (→ ComputedNode::content_inset)
/// `gutter_width` on `GutterConfig` is kept for gpu_line_numbers positioning,
/// which needs the gutter sub-region width separately from total padding.
///
/// Runs every frame (not change-filtered) so async `char_width` updates
/// from `update_font_metrics` are picked up immediately.
fn sync_gutter_width(
    mut editors: Query<(&mut Node, &mut GutterConfig, &MonoCellWidth, &EditorUi), With<CodeEditor>>,
) {
    for (mut node, mut gutter_config, mono, ui) in editors.iter_mut() {
        let gutter_width = if ui.show_line_numbers {
            ui.gutter_padding_left + ui.gutter_padding_right + (mono.px * 4.0)
        } else {
            0.0
        };
        let padding_left = Val::Px(gutter_width + ui.code_margin_left);
        let padding_top = Val::Px(ui.margin_top);
        if node.padding.left != padding_left || node.padding.top != padding_top {
            node.padding.left = padding_left;
            node.padding.top = padding_top;
        }
        if (gutter_config.gutter_width - gutter_width).abs() > 0.01 {
            gutter_config.gutter_width = gutter_width;
        }
    }
}

/// Setup UI entities (separator) for each `CodeEditor`.
fn setup_editor_ui(
    mut commands: Commands,
    editor_query: Query<
        (
            &ComputedNode,
            &GutterConfig,
            &EditorTheme,
            &EditorUi,
            Option<&bevy_camera::visibility::RenderLayers>,
        ),
        With<CodeEditor>,
    >,
) {
    for (computed, gutter, theme, ui, render_layers) in editor_query.iter() {
        let inv = computed.inverse_scale_factor();
        let logical = computed.size() * inv;
        let viewport_width = logical.x;
        let viewport_height = logical.y;

        if ui.show_separator {
            let mut cmds = commands.spawn((
                Sprite {
                    color: theme.separator,
                    custom_size: Some(Vec2::new(1.0, viewport_height)),
                    ..default()
                },
                Transform::from_translation(to_bevy_coords_left_aligned(
                    gutter.gutter_width,
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

fn viewport_changed(query: Query<(), (With<CodeEditor>, Changed<ComputedNode>)>) -> bool {
    !query.is_empty()
}

fn update_separator_on_resize(
    viewport_query: Query<(&ComputedNode, &GutterConfig), With<CodeEditor>>,
    mut separator_query: Query<(&mut Sprite, &mut Transform), With<Separator>>,
) {
    let Some((computed, gutter)) = viewport_query.iter().next() else {
        return;
    };

    let inv = computed.inverse_scale_factor();
    let logical = computed.size() * inv;
    let viewport_width = logical.x;
    let viewport_height = logical.y;

    for (mut sprite, mut transform) in separator_query.iter_mut() {
        sprite.custom_size = Some(Vec2::new(1.0, viewport_height));
        transform.translation = to_bevy_coords_left_aligned(
            gutter.gutter_width,
            viewport_height / 2.0,
            viewport_width,
            viewport_height,
            0.0,
        );
    }
}

fn update_font_metrics(
    mut editors: Query<(&TextFont, &mut MonoCellWidth), With<CodeEditor>>,
    mut atlas: ResMut<GlyphAtlas>,
    fonts: Res<Assets<bevy::text::Font>>,
) {
    for (font, mut mono) in editors.iter_mut() {
        let font_id = atlas.ensure_font(&font.font, &fonts);
        let width = atlas.shape_line("0", font.font_size, font_id).width;
        if width > 0.0 && (mono.px - width).abs() > 0.01 {
            info!(
                "Updating char_width from {:.3} to {:.3} (measured)",
                mono.px, width
            );
            mono.px = width;
        }
    }
}
