//! Resizable code editor example
//!
//! Demonstrates the code editor in a resizable panel that doesn't fill the whole window.
//! You can drag any of the 4 edges or corners to resize the editor.

use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon};
use bevy_camera::visibility::RenderLayers;
use bevy_camera::ClearColorConfig;
use bevy_code_editor::prelude::*;

/// Render layer for borders (layer 1) - only seen by BorderCamera
const BORDER_LAYER: RenderLayers = RenderLayers::layer(1);

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
    /// Center X position of the panel
    center_x: f32,
    /// Center Y position of the panel
    center_y: f32,
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
            center_x: 0.0,
            center_y: 0.0,
            width: 800.0,
            height: 600.0,
            resize_edge: ResizeEdge::None,
            min_width: 300.0,
            min_height: 200.0,
        }
    }
}

impl EditorPanel {
    fn left(&self) -> f32 {
        self.center_x - self.width / 2.0
    }
    fn right(&self) -> f32 {
        self.center_x + self.width / 2.0
    }
    fn top(&self) -> f32 {
        self.center_y + self.height / 2.0
    }
    fn bottom(&self) -> f32 {
        self.center_y - self.height / 2.0
    }
}

/// Marker for the panel border
#[derive(Component)]
struct PanelBorder;

const BORDER_WIDTH: f32 = 4.0;
const RESIZE_ZONE: f32 = 8.0;

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
        // Disable auto-resize so we can control viewport manually
        .insert_resource(ViewportConfig {
            auto_resize_to_window: false,
        })
        .add_plugins(CodeEditorPlugin::default())
        .add_plugins(EditorUiPlugin::default())
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                handle_resize_input,
                update_panel_visuals,
                sync_viewport_to_panel,
                update_cursor_icon,
            )
                .chain(),
        )
        .run();
}

/// Marker for the border camera (renders borders outside editor viewport)
#[derive(Component)]
struct BorderCamera;

fn setup(mut commands: Commands, mut state: ResMut<CodeEditorState>, panel: Res<EditorPanel>) {
    // Spawn a separate camera for the borders (renders to full window, order -1 = before editor)
    // This camera ONLY sees layer 1 (borders), so it won't render editor content
    commands.spawn((
        Camera2d,
        Camera {
            order: -1, // Render before editor camera
            clear_color: ClearColorConfig::Custom(Color::srgb(0.1, 0.1, 0.12)),
            ..default()
        },
        BORDER_LAYER, // Only see border layer
        BorderCamera,
        Name::new("BorderCamera"),
    ));

    // Spawn 4 border sprites (top, bottom, left, right)
    // These are on BORDER_LAYER so only BorderCamera sees them
    // Top border
    commands.spawn((
        Sprite {
            color: Color::srgb(0.3, 0.3, 0.35),
            custom_size: Some(Vec2::new(panel.width + BORDER_WIDTH * 2.0, BORDER_WIDTH)),
            ..default()
        },
        Transform::from_translation(Vec3::new(
            panel.center_x,
            panel.top() + BORDER_WIDTH / 2.0,
            10.0,
        )),
        BORDER_LAYER,
        PanelBorder,
        Name::new("BorderTop"),
    ));
    // Bottom border
    commands.spawn((
        Sprite {
            color: Color::srgb(0.3, 0.3, 0.35),
            custom_size: Some(Vec2::new(panel.width + BORDER_WIDTH * 2.0, BORDER_WIDTH)),
            ..default()
        },
        Transform::from_translation(Vec3::new(
            panel.center_x,
            panel.bottom() - BORDER_WIDTH / 2.0,
            10.0,
        )),
        BORDER_LAYER,
        PanelBorder,
        Name::new("BorderBottom"),
    ));
    // Left border (extends to meet top/bottom borders)
    commands.spawn((
        Sprite {
            color: Color::srgb(0.3, 0.3, 0.35),
            custom_size: Some(Vec2::new(BORDER_WIDTH, panel.height + BORDER_WIDTH * 2.0)),
            ..default()
        },
        Transform::from_translation(Vec3::new(
            panel.left() - BORDER_WIDTH / 2.0,
            panel.center_y,
            10.0,
        )),
        BORDER_LAYER,
        PanelBorder,
        Name::new("BorderLeft"),
    ));
    // Right border (extends to meet top/bottom borders)
    commands.spawn((
        Sprite {
            color: Color::srgb(0.3, 0.3, 0.35),
            custom_size: Some(Vec2::new(BORDER_WIDTH, panel.height + BORDER_WIDTH * 2.0)),
            ..default()
        },
        Transform::from_translation(Vec3::new(
            panel.right() + BORDER_WIDTH / 2.0,
            panel.center_y,
            10.0,
        )),
        BORDER_LAYER,
        PanelBorder,
        Name::new("BorderRight"),
    ));

    // Set initial content
    state.is_focused = true;
    state.set_text(
        r#"// Resizable Editor Demo
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
"#,
    );
}

fn detect_resize_edge(panel: &EditorPanel, cursor_x: f32, cursor_y: f32) -> ResizeEdge {
    let near_left = (cursor_x - panel.left()).abs() < RESIZE_ZONE;
    let near_right = (cursor_x - panel.right()).abs() < RESIZE_ZONE;
    let near_top = (cursor_y - panel.top()).abs() < RESIZE_ZONE;
    let near_bottom = (cursor_y - panel.bottom()).abs() < RESIZE_ZONE;

    // Must be within extended bounds (panel + resize zone)
    let in_x_range =
        cursor_x >= panel.left() - RESIZE_ZONE && cursor_x <= panel.right() + RESIZE_ZONE;
    let in_y_range =
        cursor_y >= panel.bottom() - RESIZE_ZONE && cursor_y <= panel.top() + RESIZE_ZONE;

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
    if panel.resize_edge != ResizeEdge::None {
        let edge = panel.resize_edge;

        // Calculate new bounds based on which edge is being dragged
        let mut new_left = panel.left();
        let mut new_right = panel.right();
        let mut new_top = panel.top();
        let mut new_bottom = panel.bottom();

        match edge {
            ResizeEdge::Left | ResizeEdge::TopLeft | ResizeEdge::BottomLeft => {
                new_left = cursor_x.min(new_right - panel.min_width);
            }
            ResizeEdge::Right | ResizeEdge::TopRight | ResizeEdge::BottomRight => {
                new_right = cursor_x.max(new_left + panel.min_width);
            }
            _ => {}
        }

        match edge {
            ResizeEdge::Top | ResizeEdge::TopLeft | ResizeEdge::TopRight => {
                new_top = cursor_y.max(new_bottom + panel.min_height);
            }
            ResizeEdge::Bottom | ResizeEdge::BottomLeft | ResizeEdge::BottomRight => {
                new_bottom = cursor_y.min(new_top - panel.min_height);
            }
            _ => {}
        }

        // Update panel from new bounds
        panel.width = new_right - new_left;
        panel.height = new_top - new_bottom;
        panel.center_x = (new_left + new_right) / 2.0;
        panel.center_y = (new_top + new_bottom) / 2.0;
    }
}

fn update_panel_visuals(
    panel: Res<EditorPanel>,
    mut border_query: Query<(&mut Sprite, &mut Transform, &Name), With<PanelBorder>>,
) {
    if !panel.is_changed() {
        return;
    }

    for (mut sprite, mut transform, name) in border_query.iter_mut() {
        match name.as_str() {
            "BorderTop" => {
                sprite.custom_size =
                    Some(Vec2::new(panel.width + BORDER_WIDTH * 2.0, BORDER_WIDTH));
                transform.translation =
                    Vec3::new(panel.center_x, panel.top() + BORDER_WIDTH / 2.0, 10.0);
            }
            "BorderBottom" => {
                sprite.custom_size =
                    Some(Vec2::new(panel.width + BORDER_WIDTH * 2.0, BORDER_WIDTH));
                transform.translation =
                    Vec3::new(panel.center_x, panel.bottom() - BORDER_WIDTH / 2.0, 10.0);
            }
            "BorderLeft" => {
                // Extend to meet top/bottom borders
                sprite.custom_size =
                    Some(Vec2::new(BORDER_WIDTH, panel.height + BORDER_WIDTH * 2.0));
                transform.translation =
                    Vec3::new(panel.left() - BORDER_WIDTH / 2.0, panel.center_y, 10.0);
            }
            "BorderRight" => {
                // Extend to meet top/bottom borders
                sprite.custom_size =
                    Some(Vec2::new(BORDER_WIDTH, panel.height + BORDER_WIDTH * 2.0));
                transform.translation =
                    Vec3::new(panel.right() + BORDER_WIDTH / 2.0, panel.center_y, 10.0);
            }
            _ => {}
        }
    }
}

fn sync_viewport_to_panel(
    panel: Res<EditorPanel>,
    mut viewport: ResMut<ViewportDimensions>,
    mut state: ResMut<CodeEditorState>,
) {
    if !panel.is_changed() {
        return;
    }

    let new_width = panel.width as u32;
    let new_height = panel.height as u32;
    let new_offset_x = panel.center_x;
    let new_offset_y = panel.center_y;

    // Check if anything changed (size OR position)
    let size_changed = viewport.width != new_width || viewport.height != new_height;
    let position_changed = (viewport.offset_x - new_offset_x).abs() > 0.1
        || (viewport.offset_y - new_offset_y).abs() > 0.1;

    if size_changed || position_changed {
        viewport.width = new_width;
        viewport.height = new_height;
        viewport.offset_x = new_offset_x;
        viewport.offset_y = new_offset_y;

        // Trigger full update for minimap and all UI elements
        state.needs_scroll_update = true;
        state.pending_update = true;
        state.needs_update = true;
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
            let over_editor = cursor_x > panel.left()
                && cursor_x < panel.right()
                && cursor_y > panel.bottom()
                && cursor_y < panel.top();
            if over_editor {
                CursorIcon::System(SystemCursorIcon::Text)
            } else {
                CursorIcon::System(SystemCursorIcon::Default)
            }
        }
    };

    commands.entity(window_entity).insert(icon);
}
