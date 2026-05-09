//! Side-by-side: a `CodeEditor` on the left, a `BevyTerminal` on the
//! right, separated by a thin 1-px divider.
//!
//! Each pane gets its own `Camera2d` with a `viewport` rect so they
//! occupy non-overlapping halves of the window. `RenderLayers` keeps
//! each view's draw calls routed to the correct camera.
//!
//! Run with: `cargo run --example terminal_editor`

use bevscode::prelude::*;
use bevsterm::prelude::*;
use bevy::prelude::*;
use bevy_camera::visibility::RenderLayers;

const DIVIDER_PX: u32 = 1;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "bevsterm — editor + terminal".into(),
                        resolution: [1280u32, 720u32].into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(bevy::asset::AssetPlugin {
                    file_path: "assets".into(),
                    ..default()
                }),
        )
        .insert_resource(ViewportConfig {
            auto_resize_to_window: false,
        })
        .add_plugins(CodeEditorPlugins)
        .add_plugins(BevyTerminalPlugin)
        .add_systems(Startup, layout_panes)
        .run();
}

fn layout_panes(
    asset_server: Res<AssetServer>,
    windows: Query<&Window>,
    mut input_focus: ResMut<bevy::input_focus::InputFocus>,
    mut commands: Commands,
) {
    let Ok(window) = windows.single() else { return };
    // Physical pixels for camera viewport rects.
    let scale = window.scale_factor();
    let phys_w = (window.width() * scale) as u32;
    let phys_h = (window.height() * scale) as u32;
    let phys_divider = (DIVIDER_PX as f32 * scale) as u32;
    let phys_half = (phys_w - phys_divider) / 2;

    // Logical dimensions fed to TextViewViewport (viewport uses logical pixels).
    let log_half = phys_half as f32 / scale;
    let log_h = window.height();

    let bg = ThemeConfig::default().background;
    let editor_layer = RenderLayers::layer(0);
    let terminal_layer = RenderLayers::layer(1);

    let font = FontConfig::from_size(14.0)
        .with_font(asset_server.load("fonts/FiraMono-Regular.ttf"))
        .with_bold_font(asset_server.load("fonts/FiraMono-Medium.ttf"));

    // Left camera → editor.
    commands.spawn((
        Camera2d,
        Camera {
            order: 0,
            clear_color: ClearColorConfig::Custom(bg),
            viewport: Some(bevy::camera::Viewport {
                physical_position: UVec2::new(0, 0),
                physical_size: UVec2::new(phys_half, phys_h),
                ..default()
            }),
            ..default()
        },
        editor_layer.clone(),
        Name::new("EditorCamera"),
    ));

    // Right camera → terminal.
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::Custom(bg),
            viewport: Some(bevy::camera::Viewport {
                physical_position: UVec2::new(phys_half + phys_divider, 0),
                physical_size: UVec2::new(phys_w - phys_half - phys_divider, phys_h),
                ..default()
            }),
            ..default()
        },
        terminal_layer.clone(),
        Name::new("TerminalCamera"),
    ));

    // 1-px divider drawn in NDC by a thin Sprite on its own layer.
    // We place it in the default camera (layer 0) at the window center.
    let divider_layer = RenderLayers::layer(2);
    commands.spawn((
        Camera2d,
        Camera {
            order: 2,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        divider_layer.clone(),
        Name::new("DividerCamera"),
    ));
    let divider_color = Color::srgba(0.3, 0.3, 0.3, 1.0);
    commands.spawn((
        Sprite {
            color: divider_color,
            custom_size: Some(Vec2::new(DIVIDER_PX as f32, window.height())),
            ..default()
        },
        Transform::from_xyz(
            // half_w − window_half puts the divider at the left/right boundary.
            log_half - window.width() / 2.0,
            0.0,
            0.0,
        ),
        divider_layer,
        Name::new("PaneDivider"),
    ));

    // Editor — left pane.
    let editor = commands
        .spawn((
            CodeEditor,
            font.clone(),
            TextViewViewport {
                width: log_half as u32,
                height: log_h as u32,
                hit_test_position: Vec2::new(0.0, 0.0),
                ..default()
            },
            editor_layer,
            Name::new("Editor"),
        ))
        .id();
    input_focus.set(editor);

    // Terminal — right pane.
    commands.spawn((
        BevyTerminal,
        font,
        TextViewViewport {
            width: (window.width() - log_half) as u32,
            height: log_h as u32,
            text_area_left: 12.0,
            text_area_top: 0.0,
            hit_test_position: Vec2::new(log_half + DIVIDER_PX as f32, 0.0),
            ..default()
        },
        terminal_layer,
        Name::new("Terminal"),
    ));
}
