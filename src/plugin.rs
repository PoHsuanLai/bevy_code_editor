//! Bevy plugin for GPU-accelerated code editor
//!
//! Renders text using Bevy's Text2d with proper alignment and viewport culling

use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::TextSpan;
use crate::settings::EditorSettings;
use crate::types::*;

/// Render mode for the code editor
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderMode {
    /// Standard 2D text rendering (default)
    #[default]
    Render2D,
    /// 3D mesh rendering (use with render3d module systems)
    Render3D,
    /// No rendering - only state and input management
    /// Useful for custom rendering implementations
    None,
}

/// Code editor plugin
pub struct CodeEditorPlugin {
    settings: EditorSettings,
    render_mode: RenderMode,
}

impl Default for CodeEditorPlugin {
    fn default() -> Self {
        Self {
            settings: EditorSettings::default(),
            render_mode: RenderMode::Render2D,
        }
    }
}

impl CodeEditorPlugin {
    /// Create plugin with custom settings
    pub fn with_settings(settings: EditorSettings) -> Self {
        Self { settings, render_mode: RenderMode::Render2D }
    }

    /// Set the render mode for the plugin
    pub fn with_render_mode(mut self, mode: RenderMode) -> Self {
        self.render_mode = mode;
        self
    }

    /// Create a plugin configured for 3D rendering
    /// This sets up state and input handling but not 2D rendering systems
    pub fn for_3d() -> Self {
        Self {
            settings: EditorSettings::default(),
            render_mode: RenderMode::Render3D,
        }
    }

    /// Create a plugin with only state management (no rendering)
    pub fn state_only() -> Self {
        Self {
            settings: EditorSettings::default(),
            render_mode: RenderMode::None,
        }
    }
}

impl Plugin for CodeEditorPlugin {
    fn build(&self, app: &mut App) {
        // Insert core resources (needed for all render modes)
        app.insert_resource(self.settings.clone());
        app.insert_resource(CodeEditorState::default());
        app.insert_resource(crate::input::Keybindings::default());
        app.insert_resource(crate::input::KeyRepeatState::default());
        app.insert_resource(crate::input::MouseDragState::default());

        // Add input handling systems (needed for all render modes)
        app.add_systems(
            Update,
            (
                crate::input::handle_keyboard_input,
                debounce_updates,
            ),
        );

        // Add 2D-specific resources and systems only for 2D mode
        if self.render_mode == RenderMode::Render2D {
            app.insert_resource(ClearColor(self.settings.theme.background));
            app.insert_resource(ViewportDimensions::default());

            app.add_systems(Startup, (init_viewport_from_window, setup).chain());
            app.add_systems(
                Update,
                (
                    crate::input::handle_mouse_input,
                    crate::input::handle_mouse_wheel,
                    auto_scroll_to_cursor,
                    detect_viewport_resize,
                    update_separator_on_resize,
                    update_scroll_only,
                    update_text_display,
                    update_line_numbers,
                    update_selection_highlight,
                    update_cursor,
                    animate_cursor,
                )
                    .chain(),
            );
        }

        // Add LSP systems if feature is enabled
        #[cfg(feature = "lsp")]
        {
            use crate::lsp::*;
            app.insert_resource(LspClient::default());
            app.insert_resource(CompletionState::default());
            app.insert_resource(HoverState::default());
            app.insert_resource(LspSyncState::default());
            // Register LSP events (messages) so external code can listen to them
            app.add_message::<NavigateToFileEvent>();
            app.add_message::<MultipleLocationsEvent>();
            app.add_systems(Update, (
                process_lsp_messages,
                sync_lsp_document,
                update_completion_ui,
                update_hover_ui,
            ));
        }
    }
}

/// Convert top-left coordinates (0,0 = top-left) to Bevy world coordinates (center-origin)
fn to_bevy_coords_dynamic(x: f32, y: f32, viewport_width: f32, viewport_height: f32, offset_x: f32) -> Vec3 {
    Vec3::new(
        x - viewport_width / 2.0 + offset_x,
        viewport_height / 2.0 - y,
        0.0,
    )
}

/// Convert coordinates for left-aligned elements
fn to_bevy_coords_left_aligned(
    margin_from_left: f32,
    y: f32,
    viewport_width: f32,
    viewport_height: f32,
    offset_x: f32,
    _horizontal_scroll: f32,  // Unused: horizontal scrolling is handled by character culling
) -> Vec3 {
    // Text always starts at the code margin position
    // Horizontal scrolling is handled by substring culling in the rendering code
    let x = -viewport_width / 2.0 + margin_from_left + offset_x;

    Vec3::new(
        x,
        viewport_height / 2.0 - y,
        0.0,
    )
}

/// Debouncing system: Only promote pending_update to needs_update if enough time has passed
const DEBOUNCE_INTERVAL_MS: f64 = 16.0; // ~60fps

fn debounce_updates(mut state: ResMut<CodeEditorState>, time: Res<Time>) {
    if !state.pending_update {
        return;
    }

    let current_time = time.elapsed_secs_f64() * 1000.0;
    let elapsed = current_time - state.last_render_time;

    if elapsed >= DEBOUNCE_INTERVAL_MS {
        // Update lines cache before marking as ready for update
        // We need settings here, but debounce_updates only has access to state and time
        // We'll mark needs_update=true, and the first thing update_text_display does is update highlighting/lines
        state.needs_update = true;
        state.pending_update = false;
        state.last_render_time = current_time;
    }
}

/// Initialize viewport dimensions from the actual window size
fn init_viewport_from_window(
    mut viewport: ResMut<ViewportDimensions>,
    windows: Query<&Window>,
) {
    if let Some(window) = windows.iter().next() {
        viewport.width = window.resolution.width() as u32;
        viewport.height = window.resolution.height() as u32;
    }
}

/// Detect viewport resize and trigger position update
fn detect_viewport_resize(
    mut viewport: ResMut<ViewportDimensions>,
    windows: Query<&Window>,
    mut state: ResMut<CodeEditorState>,
) {
    if let Some(window) = windows.iter().next() {
        let new_width = window.resolution.width() as u32;
        let new_height = window.resolution.height() as u32;

        if viewport.width != new_width || viewport.height != new_height {
            viewport.width = new_width;
            viewport.height = new_height;
            state.needs_scroll_update = true;
        }
    }
}

/// Update separator height and position when viewport changes
fn update_separator_on_resize(
    viewport: Res<ViewportDimensions>,
    settings: Res<EditorSettings>,
    mut separator_query: Query<(&mut Sprite, &mut Transform), With<Separator>>,
) {
    if viewport.is_changed() {
        if let Ok((mut sprite, mut transform)) = separator_query.single_mut() {
            let viewport_width = viewport.width as f32;
            let viewport_height = viewport.height as f32;
            sprite.custom_size = Some(Vec2::new(1.0, viewport_height));
            transform.translation = to_bevy_coords_left_aligned(
                settings.ui.layout.separator_x,
                viewport_height / 2.0,
                viewport_width,
                viewport_height,
                viewport.offset_x,
                0.0,  // separator doesn't scroll horizontally
            );
        }
    }
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut settings: ResMut<EditorSettings>,
    viewport: Res<ViewportDimensions>,
) {
    // Spawn 2D camera for the editor with 1:1 pixel mapping
    commands.spawn((
        Camera2d::default(),
        Projection::Orthographic(OrthographicProjection {
            scale: 1.0,  // 1:1 world units to pixels
            ..OrthographicProjection::default_2d()
        }),
        Camera {
            clear_color: ClearColorConfig::Custom(settings.theme.background),
            ..default()
        },
        Name::new("EditorCamera"),
    ));

    // Load font
    let font_handle: Handle<Font> = asset_server.load(&settings.font.family);
    settings.font.handle = Some(font_handle.clone());

    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    // Spawn line numbers
    commands.spawn((
        Text2d::new("1"),
        TextFont {
            font: font_handle.clone(),
            font_size: settings.font.size,
            ..default()
        },
        TextColor(settings.theme.line_numbers),
        Transform::from_translation(to_bevy_coords_dynamic(
            settings.ui.layout.line_number_margin_left,
            settings.ui.layout.margin_top,
            viewport_width,
            viewport_height,
            viewport.offset_x,
        )),
        LineNumbers,
        Name::new("LineNumbers"),
    ));

    // Spawn separator line
    commands.spawn((
        Sprite {
            color: settings.theme.separator,
            custom_size: Some(Vec2::new(1.0, viewport_height)),
            ..default()
        },
        Transform::from_translation(to_bevy_coords_left_aligned(
            settings.ui.layout.separator_x,
            viewport_height / 2.0,
            viewport_width,
            viewport_height,
            viewport.offset_x,
            0.0,  // separator doesn't scroll horizontally
        )),
        Separator,
        Name::new("Separator"),
    ));

    // Spawn cursor
    let cursor_height = settings.font.line_height * settings.cursor.height_multiplier;
    commands.spawn((
        Sprite {
            color: settings.theme.cursor,
            custom_size: Some(Vec2::new(settings.cursor.width, cursor_height)),
            ..default()
        },
        Transform::from_translation(to_bevy_coords_dynamic(
            settings.ui.layout.code_margin_left,
            settings.ui.layout.margin_top,
            viewport_width,
            viewport_height,
            viewport.offset_x,
        )),
        Visibility::Hidden,
        EditorCursor,
        Name::new("EditorCursor"),
    ));
}

/// Update scroll position only without despawning/spawning entities
fn update_scroll_only(
    mut state: ResMut<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    mut text_query: Query<(&HighlightedTextToken, &mut Transform, &mut Visibility), Without<LineNumbers>>,
    mut line_numbers_query: Query<
        (&LineNumbers, &mut Transform, &Name),
        Without<HighlightedTextToken>,
    >,
) {
    if !state.needs_scroll_update {
        return;
    }

    let line_height = settings.font.line_height;
    let viewport_height = viewport.height as f32;
    let viewport_width = viewport.width as f32;

    // Calculate visible Y range in the same coordinate space as y
    // y = margin_top + scroll_offset + (line_num * line_height)
    // Line is visible when: 0 <= y <= viewport_height
    // With buffer: -buffer <= y <= viewport_height + buffer
    let buffer = line_height * 3.0;
    let visible_top = -buffer;
    let visible_bottom = viewport_height + buffer;

    // Update text positions and visibility (vertical scroll only)
    // Note: horizontal scrolling requires full update because it changes text content
    for (line_marker, mut transform, mut visibility) in text_query.iter_mut() {
        let line_num = line_marker.index;
        let y = settings.ui.layout.margin_top + state.scroll_offset + (line_num as f32 * line_height);

        // No horizontal scroll parameter - text position is fixed, content is culled instead
        let new_translation =
            to_bevy_coords_left_aligned(settings.ui.layout.code_margin_left, y, viewport_width, viewport_height, viewport.offset_x, 0.0);
        transform.translation = new_translation;

        // Update visibility based on whether line is in viewport
        let is_visible = y >= visible_top && y <= visible_bottom;
        *visibility = if is_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    // Update line numbers
    for (_line_marker, mut transform, name) in line_numbers_query.iter_mut() {
        if let Some(line_num_str) = name.as_str().strip_prefix("LineNumber_") {
            if let Ok(line_num) = line_num_str.parse::<usize>() {
                let y = settings.ui.layout.margin_top + state.scroll_offset + ((line_num - 1) as f32 * line_height);

                let new_translation = to_bevy_coords_left_aligned(
                    settings.ui.layout.line_number_margin_left,
                    y,
                    viewport_width,
                    viewport_height,
                    viewport.offset_x,
                    0.0,  // line numbers don't scroll horizontally
                );
                transform.translation = new_translation;
            }
        }
    }

    state.needs_scroll_update = false;
}

/// Update text display when state changes - Using Text2d with TextSpan children
fn update_text_display(
    mut commands: Commands,
    mut state: ResMut<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    line_query: Query<(Entity, &mut Transform, &mut Visibility, &HighlightedTextToken)>,
    children_query: Query<&Children>,
) {
    if !state.needs_update {
        return;
    }

    // Update syntax highlighting tokens
    state.update_highlighting();
    update_lines_cache(&mut state, &settings);

    let font_size = settings.font.size;
    let line_height = settings.font.line_height;

    // === OPTIMIZATION: Viewport Culling ===
    // Calculate visible Y range in the same coordinate space as y
    // buffer: increased to 10 lines to prevent blackouts during fast scrolling
    let buffer = line_height * 10.0;
    let visible_top = -buffer;
    let visible_bottom = viewport.height as f32 + buffer;

    // Use pre-processed lines from state
    // We can't borrow &state.lines yet because we need to mutate state.max_content_width
    
    // Calculate maximum content width for horizontal scrolling
    let char_width = settings.font.char_width;
    let mut max_line_width = 0.0f32;
    // Note: iterating all lines for max width is still O(N) but much faster than full render
    // We could optimize this by caching max_width in state too
    for line_segments in &state.lines {
        let line_text: String = line_segments.iter().map(|seg| seg.text.as_str()).collect();
        let line_width = line_text.chars().count() as f32 * char_width;
        max_line_width = max_line_width.max(line_width);
    }
    state.max_content_width = max_line_width;

    let lines = &state.lines;

    // Correctly calculate start line accounting for margin_top and scroll_offset
    // y = margin_top + scroll + line*h
    // We want line where y >= -buffer
    // line*h >= -buffer - scroll - margin_top
    // line >= (-scroll - margin_top - buffer) / h
    // Note: scroll_offset is negative, so -scroll is positive distance
    let scroll_dist = state.scroll_offset.abs();
    let start_pixels = scroll_dist - settings.ui.layout.margin_top - buffer;
    let start_line = (start_pixels / line_height).floor().max(0.0) as usize;
    
    let visible_count = ((viewport.height as f32 + buffer * 2.0) / line_height).ceil() as usize;
    let end_line = (start_line + visible_count).min(lines.len());

    // === OPTIMIZATION: Entity Pooling ===
    // Gather all existing line entities into a reusable pool
    let mut entity_pool: Vec<(Entity, Transform, Visibility)> = line_query
        .iter()
        .map(|(e, t, v, _)| (e, *t, *v))
        .collect();
    
    // Sort by entity index or reuse arbitrarily? Stack is fine.
    // We'll pop from the pool.
    
    // Process only visible lines
    for line_num in start_line..end_line {
        let line_segments = &lines[line_num];
        
        let y = settings.ui.layout.margin_top + state.scroll_offset + (line_num as f32 * line_height);
        let is_visible = y >= visible_top && y <= visible_bottom;

        let translation = to_bevy_coords_left_aligned(
            settings.ui.layout.code_margin_left,
            y,
            viewport.width as f32,
            viewport.height as f32,
            viewport.offset_x,
            0.0,
        );

        // Apply horizontal scrolling: calculate which segments are visible
        let chars_to_skip = (state.horizontal_scroll_offset / char_width).floor() as usize;

        // Re-map colors to visible text after horizontal scrolling
        let mut original_char_idx = 0;
        let mut visible_segments: Vec<(String, Color)> = Vec::new();

        for segment in line_segments.iter() {
            let segment_char_count = segment.text.chars().count();
            let segment_end = original_char_idx + segment_char_count;

            if segment_end <= chars_to_skip {
                // Segment is completely before visible area
                original_char_idx = segment_end;
                continue;
            }

            if original_char_idx >= chars_to_skip {
                // Segment is entirely visible
                visible_segments.push((segment.text.clone(), segment.color));
            } else {
                // Segment is partially visible (scrolled from left)
                let skip_in_segment = chars_to_skip - original_char_idx;
                let visible_part: String = segment.text.chars().skip(skip_in_segment).collect();
                if !visible_part.is_empty() {
                    visible_segments.push((visible_part, segment.color));
                }
            }

            original_char_idx = segment_end;
        }

        // Reuse an entity from the pool, or spawn a new one
        if let Some((entity, _, _)) = entity_pool.pop() {
            let text_font = TextFont {
                font: settings.font.handle.clone().unwrap_or_default(),
                font_size,
                ..default()
            };

            // Update transform and visibility
            commands.entity(entity).insert((
                Transform::from_translation(translation),
                if is_visible {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                },
                HighlightedTextToken { index: line_num }, // Update index to current line
                Name::new(format!("Line_{}", line_num)),
            ));

            // Despawn old children
            if let Ok(children) = children_query.get(entity) {
                for child in children.iter() {
                    commands.entity(child).despawn();
                }
            }

            // Update Text2d parent and spawn TextSpan children
            if !visible_segments.is_empty() {
                let first_segment = &visible_segments[0];
                commands.entity(entity).insert((
                    Text2d::new(first_segment.0.clone()),
                    text_font.clone(),
                    TextColor(first_segment.1),
                ));

                if visible_segments.len() > 1 {
                    commands.entity(entity).with_children(|parent| {
                        for (text, color) in visible_segments.iter().skip(1) {
                            parent.spawn((
                                TextSpan::new(text.clone()),
                                text_font.clone(),
                                TextColor(*color),
                            ));
                        }
                    });
                }
            } else {
                // Empty line
                commands.entity(entity).insert((
                     Text2d::new(""),
                     text_font,
                     TextColor::default(),
                ));
            }
        } else {
            // Pool empty, spawn new entity
            let text_font = TextFont {
                font: settings.font.handle.clone().unwrap_or_default(),
                font_size,
                ..default()
            };

            if visible_segments.is_empty() {
                // Empty line
                commands.spawn((
                    Text2d::new(""),
                    text_font,
                    Transform::from_translation(translation),
                    Anchor::CENTER_LEFT,
                    HighlightedTextToken { index: line_num },
                    Name::new(format!("Line_{}", line_num)),
                    if is_visible {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    },
                ));
            } else {
                // First segment goes in parent
                let first_segment = &visible_segments[0];
                let mut entity_commands = commands.spawn((
                    Text2d::new(first_segment.0.clone()),
                    text_font.clone(),
                    TextColor(first_segment.1),
                    Transform::from_translation(translation),
                    Anchor::CENTER_LEFT,
                    HighlightedTextToken { index: line_num },
                    Name::new(format!("Line_{}", line_num)),
                    if is_visible {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    },
                ));

                // Rest go in TextSpan children
                if visible_segments.len() > 1 {
                    entity_commands.with_children(|parent| {
                        for (text, color) in visible_segments.iter().skip(1) {
                            parent.spawn((
                                TextSpan::new(text.clone()),
                                text_font.clone(),
                                TextColor(*color),
                            ));
                        }
                    });
                }
            }
        }
    }

    // Hide remaining unused entities in the pool
    for (entity, _, _) in entity_pool {
        commands.entity(entity).insert(Visibility::Hidden);
    }

    state.needs_update = false;
}

// Helper to update highlighting AND rebuild cached lines
fn update_lines_cache(
    state: &mut CodeEditorState,
    settings: &EditorSettings,
) {
    let mut lines: Vec<Vec<LineSegment>> = Vec::new();
    let mut current_line_segments: Vec<LineSegment> = Vec::new();

    for token in state.tokens.iter() {
        let token_lines: Vec<&str> = token.text.split('\n').collect();
        let color = map_highlight_color(
            token.highlight_type.as_deref(),
            &settings.theme.syntax,
            settings.theme.foreground,
        );

        for (line_idx, line_text) in token_lines.iter().enumerate() {
            if line_idx > 0 {
                lines.push(current_line_segments.clone());
                current_line_segments.clear();
            }

            if !line_text.is_empty() {
                current_line_segments.push(LineSegment {
                    text: line_text.to_string(),
                    color,
                });
            }
        }
    }

    if !current_line_segments.is_empty() {
        lines.push(current_line_segments);
    }
    
    // Ensure we have at least one empty line if file is empty
    if lines.is_empty() {
        lines.push(Vec::new());
    }

    state.lines = lines;
}

/// Map highlight type to color based on theme
///
/// This uses a generic approach that works with any tree-sitter grammar by matching
/// against common semantic categories. Tree-sitter grammars use dotted notation
/// (e.g., "function.method", "comment.documentation", "string.special").
///
/// We extract the base category (first part before the dot) and map to theme colors.
/// This allows the highlighter to work with any language's tree-sitter grammar without
/// hardcoding language-specific keywords.
fn map_highlight_color(
    highlight_type: Option<&str>,
    syntax_theme: &crate::settings::SyntaxTheme,
    default_color: Color,
) -> Color {
    let hl_type = match highlight_type {
        Some(t) => t,
        None => return default_color,
    };

    // Extract the base category (first part before dot, or the whole string)
    let base_category = hl_type.split('.').next().unwrap_or(hl_type);

    // Map semantic categories to theme colors
    // This works across all tree-sitter grammars because they follow standard naming conventions
    match base_category {
        // Keywords and control flow
        "keyword" | "conditional" | "repeat" | "exception" => syntax_theme.keyword,

        // Functions and methods
        "function" | "method" => syntax_theme.function,

        // Types and classes
        "type" | "class" | "interface" | "struct" | "enum" => syntax_theme.type_name,

        // Variables and parameters
        "variable" | "parameter" | "field" => syntax_theme.variable,

        // Constants and literals
        "constant" | "boolean" | "number" | "float" => syntax_theme.constant,

        // Strings and characters
        "string" | "character" => syntax_theme.string,

        // Comments and documentation
        "comment" | "note" | "warning" | "danger" => syntax_theme.comment,

        // Operators and punctuation
        "operator" => syntax_theme.operator,
        "punctuation" | "delimiter" | "bracket" | "special" => syntax_theme.punctuation,

        // Properties and attributes
        "property" | "attribute" | "tag" | "decorator" => syntax_theme.property,

        // Constructors
        "constructor" => syntax_theme.constructor,

        // Labels and other
        "label" => syntax_theme.label,
        "escape" => syntax_theme.escape,
        "embedded" | "include" | "preproc" => syntax_theme.embedded,

        // Namespaces and modules (use type color)
        "namespace" | "module" => syntax_theme.type_name,

        // Default for unknown categories
        _ => default_color,
    }
}

/// Update line numbers display
fn update_line_numbers(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    mut line_numbers_query: Query<(&mut Text2d, &mut Transform, &mut Visibility), With<LineNumbers>>,
) {
    // Hide all line numbers if disabled in settings
    if !settings.ui.show_line_numbers {
        for (_, _, mut visibility) in line_numbers_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    if !state.is_changed() {
        return;
    }

    let line_count = state.line_count();
    let line_height = settings.font.line_height;
    let font_size = settings.font.size;

    let viewport_top = -state.scroll_offset - line_height * 3.0;
    let viewport_bottom = viewport_top + viewport.height as f32 + line_height * 6.0;

    let first_visible_line =
        ((viewport_top - settings.ui.layout.margin_top) / line_height).floor().max(0.0) as usize;
    let last_visible_line =
        ((viewport_bottom - settings.ui.layout.margin_top) / line_height).ceil().min(line_count as f32) as usize;

    let mut existing_line_numbers: Vec<_> = line_numbers_query.iter_mut().collect();
    let mut entity_index = 0;

    for line_num in (first_visible_line + 1)..=(last_visible_line + 1).min(line_count) {
        let y = settings.ui.layout.margin_top + state.scroll_offset + ((line_num - 1) as f32 * line_height);
        let translation = to_bevy_coords_left_aligned(
            settings.ui.layout.line_number_margin_left,
            y,
            viewport.width as f32,
            viewport.height as f32,
            viewport.offset_x,
            0.0,  // line numbers don't scroll horizontally
        );

        if entity_index < existing_line_numbers.len() {
            let (ref mut text, ref mut transform, ref mut visibility) =
                &mut existing_line_numbers[entity_index];
            text.0 = line_num.to_string();
            transform.translation = translation;
            **visibility = Visibility::Visible;
        } else {
            let text_font = TextFont { font_size, ..default() };

            commands.spawn((
                Text2d::new(line_num.to_string()),
                text_font,
                TextColor(settings.theme.line_numbers),
                Transform::from_translation(translation),
                LineNumbers,
                Name::new(format!("LineNumber_{}", line_num)),
                Visibility::Visible,
            ));
        }

        entity_index += 1;
    }

    // Hide unused line numbers
    for i in entity_index..existing_line_numbers.len() {
        let (_, _, ref mut visibility) = &mut existing_line_numbers[i];
        **visibility = Visibility::Hidden;
    }
}

/// Update selection highlight rectangles
fn update_selection_highlight(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    mut selection_query: Query<(
        Entity,
        &mut Transform,
        &mut Sprite,
        &mut Visibility,
        &mut SelectionHighlight,
    )>,
) {
    if !state.is_changed() {
        return;
    }

    // Clear selection if none
    if state.selection_start.is_none() || state.selection_end.is_none() {
        for (_, _, _, mut visibility, _) in selection_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    let selection_start = state.selection_start.unwrap();
    let selection_end = state.selection_end.unwrap();

    let (start, end) = if selection_start <= selection_end {
        (selection_start, selection_end)
    } else {
        (selection_end, selection_start)
    };

    if start == end {
        for (_, _, _, mut visibility, _) in selection_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    let start_line = state.rope.char_to_line(start);
    let end_line = state.rope.char_to_line(end);

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;

    let mut existing_selections: Vec<_> = selection_query.iter_mut().collect();
    let mut entity_index = 0;

    for line_idx in start_line..=end_line {
        let line_start_char = state.rope.line_to_char(line_idx);
        let line = state.rope.line(line_idx);
        let _line_end_char = line_start_char + line.len_chars();

        let sel_start_in_line = if line_idx == start_line {
            start - line_start_char
        } else {
            0
        };

        let sel_end_in_line = if line_idx == end_line {
            end - line_start_char
        } else {
            line.len_chars()
        };

        if sel_start_in_line >= sel_end_in_line {
            continue;
        }

        let selection_width = (sel_end_in_line - sel_start_in_line) as f32 * char_width;
        let x_left_edge = settings.ui.layout.code_margin_left + (sel_start_in_line as f32 * char_width);
        let y_from_top = settings.ui.layout.margin_top + state.scroll_offset + (line_idx as f32 * line_height);

        let sprite_center_x =
            -(viewport.width as f32) / 2.0 + x_left_edge + selection_width / 2.0;
        let sprite_center_y = (viewport.height as f32) / 2.0 - y_from_top;

        let translation = Vec3::new(sprite_center_x, sprite_center_y, 0.5);

        if entity_index < existing_selections.len() {
            let (_, ref mut transform, ref mut sprite, ref mut visibility, ref mut marker) =
                &mut existing_selections[entity_index];
            sprite.custom_size = Some(Vec2::new(selection_width, line_height));
            transform.translation = translation;
            marker.line_index = line_idx;
            **visibility = Visibility::Visible;
        } else {
            commands.spawn((
                Sprite {
                    color: settings.theme.selection_background,
                    custom_size: Some(Vec2::new(selection_width, line_height)),
                    ..default()
                },
                Transform::from_translation(translation),
                SelectionHighlight { line_index: line_idx },
                Name::new(format!("Selection_Line_{}", line_idx)),
                Visibility::Visible,
            ));
        }

        entity_index += 1;
    }

    // Hide unused selections
    for i in entity_index..existing_selections.len() {
        let (_, _, _, ref mut visibility, _) = &mut existing_selections[i];
        **visibility = Visibility::Hidden;
    }
}

/// Auto-scroll viewport to keep cursor visible
fn auto_scroll_to_cursor(
    mut state: ResMut<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
) {
    // Only auto-scroll when cursor actually moves (not when scroll changes)
    let cursor_pos = state.cursor_pos.min(state.rope.len_chars());
    if cursor_pos == state.last_cursor_pos {
        return;
    }

    // Update last cursor position
    state.last_cursor_pos = cursor_pos;
    let line_index = state.rope.char_to_line(cursor_pos);
    let line_height = settings.font.line_height;
    let viewport_height = viewport.height as f32;
    let viewport_width = viewport.width as f32;

    // === VERTICAL AUTO-SCROLL ===

    // Calculate cursor's Y position
    let cursor_y = settings.ui.layout.margin_top + state.scroll_offset + (line_index as f32 * line_height);

    // Define visible range (with some margin)
    let margin_vertical = line_height * 2.0;
    let visible_top = margin_vertical;
    let visible_bottom = viewport_height - margin_vertical;

    // Adjust scroll if cursor is outside visible range
    if cursor_y < visible_top {
        // Cursor is above visible area - scroll up
        state.scroll_offset += visible_top - cursor_y;
        state.needs_scroll_update = true;
    } else if cursor_y > visible_bottom {
        // Cursor is below visible area - scroll down
        state.scroll_offset -= cursor_y - visible_bottom;
        state.needs_scroll_update = true;
    }

    // Clamp scroll_offset to valid range
    state.scroll_offset = state.scroll_offset.min(0.0);
    let line_count = state.rope.len_lines();
    let content_height = line_count as f32 * line_height;
    let max_scroll = -(content_height - viewport_height + settings.ui.layout.margin_top);
    state.scroll_offset = state.scroll_offset.max(max_scroll.min(0.0));

    // === HORIZONTAL AUTO-SCROLL ===

    // Calculate cursor's X position (column within line)
    let line_start = state.rope.line_to_char(line_index);
    let col_index = cursor_pos - line_start;
    let char_width = settings.font.char_width;

    // Cursor X position relative to code area (before scrolling)
    let cursor_x = col_index as f32 * char_width;

    // Define horizontal visible range (with some margin)
    let margin_horizontal = char_width * 5.0; // 5 characters of margin
    let visible_left = state.horizontal_scroll_offset;
    let visible_right = state.horizontal_scroll_offset + viewport_width - settings.ui.layout.code_margin_left - margin_horizontal;

    // Adjust horizontal scroll if cursor is outside visible range
    if cursor_x < visible_left {
        // Cursor is left of visible area - scroll left
        state.horizontal_scroll_offset = cursor_x.max(0.0);
        state.needs_scroll_update = true;
    } else if cursor_x > visible_right {
        // Cursor is right of visible area - scroll right
        state.horizontal_scroll_offset = cursor_x - (viewport_width - settings.ui.layout.code_margin_left - margin_horizontal);
        state.needs_scroll_update = true;
    }

    // Clamp horizontal_scroll_offset to valid range
    // Minimum is 0.0 (don't scroll past the left edge)
    state.horizontal_scroll_offset = state.horizontal_scroll_offset.max(0.0);

    // Maximum is when rightmost content reaches viewport edge
    let max_horizontal_scroll = (state.max_content_width - viewport_width).max(0.0);
    state.horizontal_scroll_offset = state.horizontal_scroll_offset.min(max_horizontal_scroll);
}

/// Update cursor position
fn update_cursor(
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    mut cursor_query: Query<(&mut Transform, &mut Visibility), With<EditorCursor>>,
) {
    if !state.is_changed() {
        return;
    }

    let Ok((mut cursor_transform, mut cursor_visibility)) = cursor_query.single_mut() else {
        return;
    };

    *cursor_visibility = Visibility::Visible;

    let cursor_pos = state.cursor_pos.min(state.rope.len_chars());
    let line_index = state.rope.char_to_line(cursor_pos);
    let line_start = state.rope.line_to_char(line_index);
    let col_index = cursor_pos - line_start;

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;

    let x_offset = settings.ui.layout.code_margin_left + (col_index as f32 * char_width);
    let y_offset = settings.ui.layout.margin_top + state.scroll_offset + (line_index as f32 * line_height);

    let translation = to_bevy_coords_left_aligned(
        x_offset,
        y_offset,
        viewport.width as f32,
        viewport.height as f32,
        viewport.offset_x,
        state.horizontal_scroll_offset,
    );
    cursor_transform.translation = Vec3::new(translation.x, translation.y, 1.0);
}

/// Animate cursor blinking
fn animate_cursor(
    time: Res<Time>,
    settings: Res<EditorSettings>,
    mut cursor_query: Query<&mut Visibility, With<EditorCursor>>,
) {
    let Ok(mut visibility) = cursor_query.single_mut() else {
        return;
    };

    if settings.cursor.blink_rate == 0.0 {
        *visibility = Visibility::Visible;
        return;
    }

    let blink_phase = (time.elapsed_secs() * settings.cursor.blink_rate) % 1.0;
    *visibility = if blink_phase < 0.5 {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
}
