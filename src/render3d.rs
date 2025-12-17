//! 3D rendering for the code editor using bevy_fontmesh
//!
//! This module provides 3D text mesh rendering as an alternative to the default 2D rendering.
//! Each line of code is rendered as a separate 3D text mesh, and syntax highlighting is
//! achieved through per-character materials.

use bevy::prelude::*;
use bevy_fontmesh::prelude::*;

use crate::settings::EditorSettings;
use crate::types::CodeEditorState;

/// Marker component for the 3D code editor root entity
#[derive(Component)]
pub struct CodeEditor3D;

/// Marker component for a 3D line of code
#[derive(Component)]
pub struct CodeLine3D {
    /// Line index in the document (0-indexed)
    pub line_index: usize,
}

/// Marker component for the 3D cursor
#[derive(Component)]
pub struct Cursor3D;

/// Marker component for 3D line numbers
#[derive(Component)]
pub struct LineNumber3D {
    pub line_index: usize,
}

/// Resource for 3D editor configuration
#[derive(Resource)]
pub struct Editor3DConfig {
    /// Font handle for 3D text
    pub font: Handle<FontMesh>,
    /// Extrusion depth for text meshes
    pub depth: f32,
    /// Curve subdivision quality (5-30)
    pub subdivision: u8,
    /// Scale factor for the 3D text
    pub scale: f32,
    /// Material for regular text
    pub default_material: Handle<StandardMaterial>,
    /// Material for keywords
    pub keyword_material: Handle<StandardMaterial>,
    /// Material for strings
    pub string_material: Handle<StandardMaterial>,
    /// Material for comments
    pub comment_material: Handle<StandardMaterial>,
    /// Material for numbers
    pub number_material: Handle<StandardMaterial>,
    /// Material for the cursor
    pub cursor_material: Handle<StandardMaterial>,
    /// Material for line numbers
    pub line_number_material: Handle<StandardMaterial>,
}

impl Default for Editor3DConfig {
    fn default() -> Self {
        Self {
            font: Handle::default(),
            depth: 0.1,
            subdivision: 15,
            scale: 1.0,
            default_material: Handle::default(),
            keyword_material: Handle::default(),
            string_material: Handle::default(),
            comment_material: Handle::default(),
            number_material: Handle::default(),
            cursor_material: Handle::default(),
            line_number_material: Handle::default(),
        }
    }
}

/// State tracking for 3D rendering updates
#[derive(Resource, Default)]
pub struct Editor3DState {
    /// Last known line count (to detect changes)
    pub last_line_count: usize,
    /// Last known cursor position
    pub last_cursor_pos: usize,
    /// Whether a full rebuild is needed
    pub needs_rebuild: bool,
}

/// Initialize 3D editor materials
pub fn setup_3d_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    config: Option<Res<Editor3DConfig>>,
) {
    // Skip if already configured
    if config.is_some() {
        return;
    }

    // Set dark background
    commands.insert_resource(ClearColor(Color::srgb(0.1, 0.1, 0.12)));

    let config = Editor3DConfig {
        font: asset_server.load("fonts/FiraMono-Medium.ttf"),
        depth: 0.08,
        subdivision: 12,
        scale: 0.5,
        default_material: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            metallic: 0.1,
            perceptual_roughness: 0.8,
            ..default()
        }),
        keyword_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.7, 0.5, 0.9), // Purple
            metallic: 0.3,
            perceptual_roughness: 0.5,
            ..default()
        }),
        string_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.6, 0.9, 0.5), // Green
            metallic: 0.2,
            perceptual_roughness: 0.6,
            ..default()
        }),
        comment_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.5, 0.5, 0.55), // Gray
            metallic: 0.1,
            perceptual_roughness: 0.8,
            ..default()
        }),
        number_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.7, 0.5), // Orange
            metallic: 0.2,
            perceptual_roughness: 0.6,
            ..default()
        }),
        cursor_material: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            emissive: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
            metallic: 0.5,
            perceptual_roughness: 0.3,
            ..default()
        }),
        line_number_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.5, 0.5, 0.6),
            metallic: 0.1,
            perceptual_roughness: 0.9,
            ..default()
        }),
    };

    commands.insert_resource(config);
    commands.insert_resource(Editor3DState::default());
}

/// System to spawn/update 3D code lines
pub fn update_3d_code_lines(
    mut commands: Commands,
    editor_state: Res<CodeEditorState>,
    config: Option<Res<Editor3DConfig>>,
    state: Option<ResMut<Editor3DState>>,
    font_assets: Res<Assets<FontMesh>>,
    line_query: Query<(Entity, &CodeLine3D)>,
    editor_root: Query<Entity, With<CodeEditor3D>>,
) {
    // Wait for config to be initialized
    let Some(config) = config else { return };
    let Some(mut state) = state else { return };
    // Wait for font to load
    if font_assets.get(&config.font).is_none() {
        return;
    }

    let current_line_count = editor_state.rope.len_lines();

    // Check if we need to rebuild
    let needs_update = editor_state.pending_update
        || editor_state.needs_update
        || state.needs_rebuild
        || state.last_line_count != current_line_count;

    if !needs_update {
        return;
    }

    // Get or create root entity
    let root_entity = if let Ok(entity) = editor_root.single() {
        entity
    } else {
        commands
            .spawn((
                CodeEditor3D,
                Transform::default(),
                GlobalTransform::default(),
                Visibility::default(),
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ))
            .id()
    };

    // Despawn old lines
    for (entity, _) in line_query.iter() {
        commands.entity(entity).despawn();
    }

    // Get font metrics for positioning
    let font = font_assets.get(&config.font).unwrap();
    let metrics = font.font_metrics().unwrap_or(FontMetrics {
        ascender: 1.0,
        descender: -0.3,
        line_gap: 0.1,
        line_height: 1.4,
    });

    let line_height = metrics.line_height * config.scale;

    // Spawn new lines
    commands.entity(root_entity).with_children(|parent| {
        for line_idx in 0..current_line_count {
            let line = editor_state.rope.line(line_idx);
            let line_text: String = line.chars().collect();

            // Skip empty lines (but still reserve space)
            let trimmed = line_text.trim_end_matches('\n');
            if trimmed.is_empty() {
                continue;
            }

            let y_pos = -(line_idx as f32) * line_height;

            // Spawn line as TextMeshGlyphs for per-character control
            parent.spawn((
                CodeLine3D { line_index: line_idx },
                TextMeshGlyphs {
                    text: trimmed.to_string(),
                    font: config.font.clone(),
                    style: TextMeshStyle {
                        depth: config.depth,
                        subdivision: config.subdivision,
                        anchor: TextAnchor::TopLeft,
                        justify: JustifyText::Left,
                    },
                },
                MeshMaterial3d(config.default_material.clone()),
                Transform::from_xyz(0.0, y_pos, 0.0).with_scale(Vec3::splat(config.scale)),
                Visibility::default(),
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ));
        }
    });

    state.last_line_count = current_line_count;
    state.needs_rebuild = false;
}

/// System to update the 3D cursor position
pub fn update_3d_cursor(
    mut commands: Commands,
    editor_state: Res<CodeEditorState>,
    config: Option<Res<Editor3DConfig>>,
    state: Option<ResMut<Editor3DState>>,
    font_assets: Res<Assets<FontMesh>>,
    cursor_query: Query<Entity, With<Cursor3D>>,
    editor_root: Query<Entity, With<CodeEditor3D>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    // Wait for config to be initialized
    let Some(config) = config else { return };
    let Some(mut state) = state else { return };
    // Wait for font to load
    let font = match font_assets.get(&config.font) {
        Some(f) => f,
        None => return,
    };

    // Only update if cursor moved
    if state.last_cursor_pos == editor_state.cursor_pos && !state.needs_rebuild {
        return;
    }

    // Get root entity
    let root_entity = match editor_root.single() {
        Ok(e) => e,
        Err(_) => return,
    };

    // Despawn old cursor
    for entity in cursor_query.iter() {
        commands.entity(entity).despawn();
    }

    // Calculate cursor position
    let cursor_pos = editor_state.cursor_pos.min(editor_state.rope.len_chars());
    let line_idx = editor_state.rope.char_to_line(cursor_pos);
    let line_start = editor_state.rope.line_to_char(line_idx);
    let col_idx = cursor_pos - line_start;

    // Get line text up to cursor
    let line = editor_state.rope.line(line_idx);
    let line_text: String = line.chars().take(col_idx).collect();

    // Calculate x position using font metrics
    let x_pos = font.text_width(&line_text) * config.scale;

    let metrics = font.font_metrics().unwrap_or(FontMetrics {
        ascender: 1.0,
        descender: -0.3,
        line_gap: 0.1,
        line_height: 1.4,
    });

    let line_height = metrics.line_height * config.scale;
    let y_pos = -(line_idx as f32) * line_height;

    // Create cursor mesh (thin box)
    let cursor_width = 0.02 * config.scale;
    let cursor_height = metrics.line_height * config.scale * 0.9;
    let cursor_depth = config.depth * config.scale * 2.0;

    let cursor_mesh = meshes.add(Cuboid::new(cursor_width, cursor_height, cursor_depth));

    // Spawn cursor
    commands.entity(root_entity).with_children(|parent| {
        parent.spawn((
            Cursor3D,
            Mesh3d(cursor_mesh),
            MeshMaterial3d(config.cursor_material.clone()),
            Transform::from_xyz(
                x_pos + cursor_width / 2.0,
                y_pos - cursor_height / 2.0,
                cursor_depth / 2.0,
            ),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
        ));
    });

    state.last_cursor_pos = editor_state.cursor_pos;
}

/// System to update 3D line numbers
pub fn update_3d_line_numbers(
    mut commands: Commands,
    editor_state: Res<CodeEditorState>,
    config: Option<Res<Editor3DConfig>>,
    font_assets: Res<Assets<FontMesh>>,
    line_number_query: Query<Entity, With<LineNumber3D>>,
    editor_root: Query<Entity, With<CodeEditor3D>>,
) {
    // Wait for config to be initialized
    let Some(config) = config else { return };
    // Wait for font to load
    let font = match font_assets.get(&config.font) {
        Some(f) => f,
        None => return,
    };

    // Only update when lines change
    let current_line_count = editor_state.rope.len_lines();
    if line_number_query.iter().count() == current_line_count {
        return;
    }

    // Get root entity
    let root_entity = match editor_root.single() {
        Ok(e) => e,
        Err(_) => return,
    };

    // Despawn old line numbers
    for entity in line_number_query.iter() {
        commands.entity(entity).despawn();
    }

    let metrics = font.font_metrics().unwrap_or(FontMetrics {
        ascender: 1.0,
        descender: -0.3,
        line_gap: 0.1,
        line_height: 1.4,
    });

    let line_height = metrics.line_height * config.scale;
    let _gutter_width = font.text_width("9999") * config.scale + 0.2;

    // Spawn line numbers
    commands.entity(root_entity).with_children(|parent| {
        for line_idx in 0..current_line_count {
            let y_pos = -(line_idx as f32) * line_height;
            let line_num_text = format!("{:>4}", line_idx + 1);

            parent.spawn((
                LineNumber3D { line_index: line_idx },
                TextMesh {
                    text: line_num_text,
                    font: config.font.clone(),
                    style: TextMeshStyle {
                        depth: config.depth * 0.5,
                        subdivision: config.subdivision,
                        anchor: TextAnchor::TopRight,
                        justify: JustifyText::Right,
                    },
                },
                Mesh3d::default(),
                MeshMaterial3d(config.line_number_material.clone()),
                Transform::from_xyz(-0.3, y_pos, 0.0).with_scale(Vec3::splat(config.scale * 0.8)),
                Visibility::default(),
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ));
        }
    });
}

/// System to animate cursor blink in 3D
pub fn animate_3d_cursor(
    time: Res<Time>,
    settings: Res<EditorSettings>,
    mut cursor_query: Query<&mut Visibility, With<Cursor3D>>,
) {
    for mut visibility in cursor_query.iter_mut() {
        let blink_phase = (time.elapsed_secs() * settings.cursor.blink_rate) % 1.0;
        *visibility = if blink_phase < 0.5 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

/// Setup basic 3D scene (camera, lights)
pub fn setup_3d_scene(mut commands: Commands) {
    // Camera looking at the code
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(5.0, -3.0, 8.0).looking_at(Vec3::new(5.0, -3.0, 0.0), Vec3::Y),
    ));

    // Key light
    commands.spawn((
        PointLight {
            intensity: 100000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(10.0, 5.0, 10.0),
    ));

    // Fill light
    commands.spawn((
        PointLight {
            intensity: 50000.0,
            color: Color::srgb(0.5, 0.5, 1.0),
            ..default()
        },
        Transform::from_xyz(-5.0, 0.0, 8.0),
    ));

    // Ambient light
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 200.0,
        ..default()
    });
}
