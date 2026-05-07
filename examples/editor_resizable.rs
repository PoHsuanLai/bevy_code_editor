//! Resizable code editor example
//!
//! Demonstrates the code editor in a resizable panel that doesn't fill the whole window.
//! You can drag any of the 4 edges or corners to resize the editor.

use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon};
use bevy_code_editor::prelude::*;
use bevy_code_editor::types::ViewportConfig;

/// Which edge/corner is being resized
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum ResizeEdge {
    #[default]
    None,
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl ResizeEdge {
    fn cursor_icon(&self) -> SystemCursorIcon {
        match self {
            ResizeEdge::None => SystemCursorIcon::Default,
            ResizeEdge::Left | ResizeEdge::Right => SystemCursorIcon::EwResize,
            ResizeEdge::Top | ResizeEdge::Bottom => SystemCursorIcon::NsResize,
            ResizeEdge::TopLeft | ResizeEdge::BottomRight => SystemCursorIcon::NwseResize,
            ResizeEdge::TopRight | ResizeEdge::BottomLeft => SystemCursorIcon::NeswResize,
        }
    }
}

/// Tracks the editor panel size and resize state
#[derive(Resource)]
struct EditorPanel {
    /// Left edge position in world coords
    left: f32,
    /// Top edge position in world coords
    top: f32,
    /// Width of the editor panel
    width: f32,
    /// Height of the editor panel
    height: f32,
    /// Which edge is being resized
    resize_edge: ResizeEdge,
    /// Minimum width
    min_width: f32,
    /// Minimum height
    min_height: f32,
}

impl Default for EditorPanel {
    fn default() -> Self {
        Self {
            left: -400.0, // Start 400px left of center
            top: 300.0,   // Start 300px above center
            width: 800.0,
            height: 600.0,
            resize_edge: ResizeEdge::None,
            min_width: 300.0,
            min_height: 200.0,
        }
    }
}

impl EditorPanel {
    fn right(&self) -> f32 {
        self.left + self.width
    }
    fn bottom(&self) -> f32 {
        self.top - self.height
    }
    fn center_x(&self) -> f32 {
        self.left + self.width / 2.0
    }
    fn center_y(&self) -> f32 {
        self.top - self.height / 2.0
    }
}

/// Marker for the panel border
#[derive(Component)]
struct PanelBorder;

const BORDER_WIDTH: f32 = 4.0;
const RESIZE_ZONE: f32 = 12.0; // Wider zone for easier grabbing

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Resizable Code Editor".to_string(),
                resolution: (1400, 900).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(EditorPanel::default())
        .add_plugins(CodeEditorPlugins)
        // Disable auto-resize so we can control viewport manually
        // IMPORTANT: Must be AFTER CodeEditorPlugins to override its default
        .insert_resource(ViewportConfig {
            auto_resize_to_window: false,
        })
        .add_systems(Startup, setup_camera)
        .add_systems(PostStartup, setup)
        .add_systems(
            Update,
            (
                handle_resize_input,
                sync_viewport_to_panel,
                update_panel_visuals,
                update_cursor_icon,
            )
                .chain(),
        )
        .run();
}

fn setup_camera(mut commands: Commands) {
    // `EditorCamera` marker opts into the editor's camera-viewport
    // restriction system (`update_camera_viewport`), which is needed
    // here because we run with `auto_resize_to_window: false`.
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(ThemeConfig::default().background),
            ..default()
        },
        EditorCamera,
    ));
}

fn setup(
    mut commands: Commands,
    mut editor_query: Query<(Entity, &mut TextViewViewport), With<CodeEditor>>,
    panel: Res<EditorPanel>,
    mut input_focus: ResMut<bevy::input_focus::InputFocus>,
    mut set_text_writer: MessageWriter<bevy_text_editor::SetTextRequested>,
) {
    let Ok((entity, mut viewport)) = editor_query.single_mut() else {
        return;
    };

    // Set initial viewport position (left/top of panel in window coords).
    // The TextView entity's `Transform` is now the layout origin — the
    // engine no longer tracks viewport.origin. Hit-test position still
    // lives on the viewport.
    let panel_pos = bevy::math::Vec2::new(panel.left, panel.top);
    commands
        .entity(entity)
        .insert(Transform::from_translation(panel_pos.extend(0.0)));
    viewport.hit_test_position = panel_pos;

    // Spawn 4 border sprites (top, bottom, left, right)
    // Camera is positioned at panel center, so borders are relative to camera center (0, 0)
    // IMPORTANT: Borders must be INSIDE the viewport bounds, as camera viewport clips everything outside
    // Top border (inside the top edge)
    commands.spawn((
        Sprite {
            color: Color::srgb(0.3, 0.3, 0.35),
            custom_size: Some(Vec2::new(panel.width, BORDER_WIDTH)),
            ..default()
        },
        Transform::from_translation(Vec3::new(
            0.0,
            panel.height / 2.0 - BORDER_WIDTH / 2.0, // Inside the panel
            10.0,                                    // Positive Z to render in front
        )),
        PanelBorder,
        Name::new("BorderTop"),
    ));
    // Bottom border (inside the bottom edge)
    commands.spawn((
        Sprite {
            color: Color::srgb(0.3, 0.3, 0.35),
            custom_size: Some(Vec2::new(panel.width, BORDER_WIDTH)),
            ..default()
        },
        Transform::from_translation(Vec3::new(
            0.0,
            -panel.height / 2.0 + BORDER_WIDTH / 2.0, // Inside the panel
            10.0,                                     // Positive Z to render in front
        )),
        PanelBorder,
        Name::new("BorderBottom"),
    ));
    // Left border (inside the left edge)
    commands.spawn((
        Sprite {
            color: Color::srgb(0.3, 0.3, 0.35),
            custom_size: Some(Vec2::new(BORDER_WIDTH, panel.height)),
            ..default()
        },
        Transform::from_translation(Vec3::new(
            -panel.width / 2.0 + BORDER_WIDTH / 2.0, // Inside the panel
            0.0,
            10.0, // Positive Z to render in front
        )),
        PanelBorder,
        Name::new("BorderLeft"),
    ));
    // Right border (inside the right edge)
    commands.spawn((
        Sprite {
            color: Color::srgb(0.3, 0.3, 0.35),
            custom_size: Some(Vec2::new(BORDER_WIDTH, panel.height)),
            ..default()
        },
        Transform::from_translation(Vec3::new(
            panel.width / 2.0 - BORDER_WIDTH / 2.0, // Inside the panel
            0.0,
            10.0, // Positive Z to render in front
        )),
        PanelBorder,
        Name::new("BorderRight"),
    ));

    // Set initial content
    input_focus.set(entity);
    set_text_writer.write(bevy_text_editor::SetTextRequested {
        entity,
        text: r#"// Resizable Editor Demo
//
// Drag any edge or corner to resize the editor!
// - Edges: resize in one direction
// - Corners: resize in both directions
//
// The minimap and all UI elements will adapt.

fn main() {
    println!("Hello from the resizable editor!");

    // Some sample code to show the editor working
    let numbers = vec![1, 2, 3, 4, 5];
    let sum: i32 = numbers.iter().sum();
    println!("Sum: {}", sum);

    // More lines to test scrolling
    for i in 0..50 {
        println!("Line {}: The quick brown fox jumps over the lazy dog", i);
    }
}

struct Example {
    name: String,
    value: i32,
}

impl Example {
    fn new(name: &str, value: i32) -> Self {
        Self {
            name: name.to_string(),
            value,
        }
    }

    fn display(&self) {
        println!("{}: {}", self.name, self.value);
    }
}

// More code to make scrolling more interesting
mod utils {
    pub fn calculate_fibonacci(n: u64) -> u64 {
        match n {
            0 => 0,
            1 => 1,
            _ => calculate_fibonacci(n - 1) + calculate_fibonacci(n - 2),
        }
    }

    pub fn is_prime(n: u64) -> bool {
        if n < 2 {
            return false;
        }
        for i in 2..=(n as f64).sqrt() as u64 {
            if n % i == 0 {
                return false;
            }
        }
        true
    }
}
"#
        .to_string(),
    });
}

fn detect_resize_edge(panel: &EditorPanel, cursor_x: f32, cursor_y: f32) -> ResizeEdge {
    // Borders are drawn INSIDE the panel edges by BORDER_WIDTH/2
    // So the visual border center is at panel.edge - BORDER_WIDTH/2
    // But for better UX, we detect resize at the actual panel edges with wider zone
    let near_left = (cursor_x - panel.left).abs() < RESIZE_ZONE;
    let near_right = (cursor_x - panel.right()).abs() < RESIZE_ZONE;
    let near_top = (cursor_y - panel.top).abs() < RESIZE_ZONE;
    let near_bottom = (cursor_y - panel.bottom()).abs() < RESIZE_ZONE;

    // Must be within extended bounds (panel + resize zone on outside, panel edge on inside)
    let in_x_range =
        cursor_x >= panel.left - RESIZE_ZONE && cursor_x <= panel.right() + RESIZE_ZONE;
    let in_y_range =
        cursor_y >= panel.bottom() - RESIZE_ZONE && cursor_y <= panel.top + RESIZE_ZONE;

    if !in_x_range || !in_y_range {
        return ResizeEdge::None;
    }

    // Check corners first (they take priority)
    match (near_left, near_right, near_top, near_bottom) {
        (true, _, true, _) => ResizeEdge::TopLeft,
        (_, true, true, _) => ResizeEdge::TopRight,
        (true, _, _, true) => ResizeEdge::BottomLeft,
        (_, true, _, true) => ResizeEdge::BottomRight,
        (true, _, _, _) => ResizeEdge::Left,
        (_, true, _, _) => ResizeEdge::Right,
        (_, _, true, _) => ResizeEdge::Top,
        (_, _, _, true) => ResizeEdge::Bottom,
        _ => ResizeEdge::None,
    }
}

fn handle_resize_input(
    mut panel: ResMut<EditorPanel>,
    windows: Query<&Window>,
    mouse_button: Res<ButtonInput<MouseButton>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    // Convert to world coordinates (center origin)
    let cursor_x = cursor_pos.x - window.width() / 2.0;
    let cursor_y = window.height() / 2.0 - cursor_pos.y;

    // Detect which edge we're near
    let hover_edge = detect_resize_edge(&panel, cursor_x, cursor_y);

    // Start resizing on mouse press
    if mouse_button.just_pressed(MouseButton::Left) && hover_edge != ResizeEdge::None {
        panel.resize_edge = hover_edge;
    }

    // Stop resizing on mouse release
    if mouse_button.just_released(MouseButton::Left) {
        panel.resize_edge = ResizeEdge::None;
    }

    // Update dimensions while resizing
    // Directly modify the edge positions - opposite edges stay fixed
    if panel.resize_edge != ResizeEdge::None {
        let edge = panel.resize_edge;

        match edge {
            ResizeEdge::Left | ResizeEdge::TopLeft | ResizeEdge::BottomLeft => {
                // Move left edge, right edge stays fixed
                let new_left = cursor_x.min(panel.right() - panel.min_width);
                panel.width = panel.right() - new_left;
                panel.left = new_left;
            }
            ResizeEdge::Right | ResizeEdge::TopRight | ResizeEdge::BottomRight => {
                // Move right edge, left edge stays fixed
                let new_right = cursor_x.max(panel.left + panel.min_width);
                panel.width = new_right - panel.left;
            }
            _ => {}
        }

        match edge {
            ResizeEdge::Top | ResizeEdge::TopLeft | ResizeEdge::TopRight => {
                // Move top edge, bottom stays fixed
                let new_top = cursor_y.max(panel.bottom() + panel.min_height);
                panel.height = new_top - panel.bottom();
                panel.top = new_top;
            }
            ResizeEdge::Bottom | ResizeEdge::BottomLeft | ResizeEdge::BottomRight => {
                // Move bottom edge, top stays fixed
                let new_bottom = cursor_y.min(panel.top - panel.min_height);
                panel.height = panel.top - new_bottom;
            }
            _ => {}
        }
    }
}

fn update_panel_visuals(
    panel: Res<EditorPanel>,
    mut border_query: Query<(&mut Sprite, &mut Transform, &Name), With<PanelBorder>>,
) {
    // Update borders when panel changes
    if !panel.is_changed() {
        return;
    }

    // Position borders in ABSOLUTE WORLD COORDINATES
    // This way they stay fixed when only one edge moves
    // Borders are INSIDE the panel edges
    for (mut sprite, mut transform, name) in border_query.iter_mut() {
        match name.as_str() {
            "BorderTop" => {
                sprite.custom_size = Some(Vec2::new(panel.width, BORDER_WIDTH));
                // Top border in world coords: center X at panel.center_x(), Y at panel.top - BORDER_WIDTH/2
                transform.translation =
                    Vec3::new(panel.center_x(), panel.top - BORDER_WIDTH / 2.0, 10.0);
            }
            "BorderBottom" => {
                sprite.custom_size = Some(Vec2::new(panel.width, BORDER_WIDTH));
                // Bottom border in world coords
                transform.translation =
                    Vec3::new(panel.center_x(), panel.bottom() + BORDER_WIDTH / 2.0, 10.0);
            }
            "BorderLeft" => {
                sprite.custom_size = Some(Vec2::new(BORDER_WIDTH, panel.height));
                // Left border in world coords
                transform.translation =
                    Vec3::new(panel.left + BORDER_WIDTH / 2.0, panel.center_y(), 10.0);
            }
            "BorderRight" => {
                sprite.custom_size = Some(Vec2::new(BORDER_WIDTH, panel.height));
                // Right border in world coords
                transform.translation =
                    Vec3::new(panel.right() - BORDER_WIDTH / 2.0, panel.center_y(), 10.0);
            }
            _ => {}
        }
    }
}

fn sync_viewport_to_panel(
    panel: Res<EditorPanel>,
    mut viewport_query: Query<&mut TextViewViewport, With<CodeEditor>>,
) {
    if !panel.is_changed() {
        return;
    }

    let Ok(mut viewport) = viewport_query.single_mut() else {
        return;
    };

    let new_width = panel.width as u32;
    let new_height = panel.height as u32;

    // Check if size changed
    let size_changed = viewport.width != new_width || viewport.height != new_height;

    if size_changed {
        // Only update viewport SIZE, never position.
        // origin and hit_test_position are set once at startup and never change —
        // this prevents UI elements from repositioning when the panel resizes.
        viewport.width = new_width;
        viewport.height = new_height;
    }
}

fn update_cursor_icon(
    panel: Res<EditorPanel>,
    mut commands: Commands,
    windows: Query<(Entity, &Window)>,
) {
    let Ok((window_entity, window)) = windows.single() else {
        return;
    };

    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    // Convert to world coordinates
    let cursor_x = cursor_pos.x - window.width() / 2.0;
    let cursor_y = window.height() / 2.0 - cursor_pos.y;

    // If actively resizing, keep that cursor
    let icon = if panel.resize_edge != ResizeEdge::None {
        CursorIcon::System(panel.resize_edge.cursor_icon())
    } else {
        // Check if hovering over an edge
        let hover_edge = detect_resize_edge(&panel, cursor_x, cursor_y);
        if hover_edge != ResizeEdge::None {
            CursorIcon::System(hover_edge.cursor_icon())
        } else {
            // Check if over editor area (inside panel)
            let over_editor = cursor_x > panel.left
                && cursor_x < panel.right()
                && cursor_y > panel.bottom()
                && cursor_y < panel.top;
            if over_editor {
                CursorIcon::System(SystemCursorIcon::Text)
            } else {
                CursorIcon::System(SystemCursorIcon::Default)
            }
        }
    };

    commands.entity(window_entity).insert(icon);
}
