//! GPU text rendering

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use crate::settings::EditorSettings;
use crate::types::*;
use crate::gpu_text::{GlyphAtlas, GlyphKey, GlyphRasterizer, TextRenderState};
use super::{SyntaxResource, HighlightCache};
use std::sync::Arc;

/// Component to track async parse tasks
#[cfg(feature = "tree-sitter")]
#[derive(Component)]
pub struct ParseTask {
    task: Task<Option<tree_sitter::Tree>>,
    content_version: u64,
}

/// Update tree-sitter tree asynchronously to avoid blocking frames
#[cfg(feature = "tree-sitter")]
pub(crate) fn update_syntax_tree(
    mut commands: Commands,
    mut state: ResMut<CodeEditorState>,
    mut syntax: ResMut<SyntaxResource>,
    mut highlight_cache: ResMut<HighlightCache>,
    mut parse_task_query: Query<(Entity, &mut ParseTask)>,
) {
    // Check if there's a completed parse task
    if let Some((entity, mut parse_task)) = parse_task_query.iter_mut().next() {
        // Poll the task without blocking
        if let Some(tree) = futures_lite::future::block_on(futures_lite::future::poll_once(&mut parse_task.task)) {
            if let Some(tree) = tree {
                // Update the syntax provider with the completed tree and current rope
                syntax.set_parsed_tree(tree, &state.rope);
                state.last_highlighted_version = parse_task.content_version;
            }
            // Remove the completed task
            commands.entity(entity).despawn();
        }
        // Task still running, don't start a new one
        return;
    }

    // Only start a new parse if content changed and no task is running
    if state.content_version != state.last_highlighted_version && syntax.is_available() {
        // Clear highlight cache when content changes
        highlight_cache.clear();

        // Clone rope for async task (Rope uses Arc internally so this is cheap)
        let rope = state.rope.clone();
        let content_version = state.content_version;

        // Clone the provider's state for incremental parsing (keeps main state intact)
        let (parser, language, cached_tree, pending_edits) = syntax.clone_parse_state();

        // Spawn async parse task
        let task_pool = AsyncComputeTaskPool::get();
        let task = task_pool.spawn(async move {
            parse_tree_async(rope, parser, language, cached_tree, pending_edits)
        });

        // Spawn entity to track the task
        commands.spawn(ParseTask {
            task,
            content_version,
        });
    }
}

#[cfg(feature = "tree-sitter")]
fn parse_tree_async(
    rope: ropey::Rope,
    mut parser: Option<tree_sitter::Parser>,
    language: Option<tree_sitter::Language>,
    mut cached_tree: Option<tree_sitter::Tree>,
    pending_edits: Vec<tree_sitter::InputEdit>,
) -> Option<tree_sitter::Tree> {
    // Same parsing logic as update_tree, but runs async
    use crate::syntax::tree_sitter::RopeReader;

    let mut reader = RopeReader::new(&rope);
    let mut callback = |byte_offset: usize, _position: tree_sitter::Point| -> &[u8] {
        reader.read(byte_offset)
    };

    // Try incremental parsing first
    if let Some(ref mut tree) = cached_tree {
        // Apply pending edits
        for edit in pending_edits {
            tree.edit(&edit);
        }

        // Re-parse incrementally
        if let Some(ref mut parser) = parser {
            if let Some(new_tree) = parser.parse_with(&mut callback, Some(tree)) {
                return Some(new_tree);
            }
        }
    } else if let Some(ref lang) = language {
        // First parse - initialize parser
        if parser.is_none() {
            let mut new_parser = tree_sitter::Parser::new();
            if new_parser.set_language(lang).is_ok() {
                parser = Some(new_parser);
            }
        }

        if let Some(ref mut parser) = parser {
            return parser.parse_with(&mut callback, None);
        }
    }

    None
}

/// Marker component for GPU text mesh entities
#[derive(Component)]
pub struct GpuTextMesh {
    /// The scroll offset when this mesh was built
    pub built_at_scroll: f32,
    pub built_at_horizontal_scroll: f32,
    /// The visible line range when built
    pub first_line: usize,
    pub last_line: usize,
}

/// Marker component for per-line GPU text mesh
#[derive(Component)]
pub struct GpuLineMesh {
    /// The buffer line number this mesh represents
    pub line_number: usize,
    /// Content version when this line was last built
    pub content_version: u64,
}

/// Marker component for GPU minimap mesh entity
#[derive(Component)]
pub struct GpuMinimapMesh {
    /// The content version when this mesh was built
    pub built_at_version: u64,
}

/// Convert scroll-only updates to full updates for GPU text
/// GPU text rendering requires rebuilding the mesh on scroll
/// OPTIMIZATION: Only rebuild mesh when we've scrolled significantly (> 5 lines)
pub(crate) fn handle_scroll_for_gpu_text(
    mut state: ResMut<CodeEditorState>,
    settings: Res<EditorSettings>,
    mesh_query: Query<&GpuTextMesh>,
) {
    if state.needs_scroll_update {
        // Check if we've scrolled significantly from the last built position
        let line_height = settings.font.line_height;
        let scroll_dist = state.scroll_offset.abs();
        let current_line = (scroll_dist / line_height).floor() as usize;

        // Only rebuild if we've scrolled more than 5 lines from last build
        let should_rebuild = if let Some(mesh_info) = mesh_query.iter().next() {
            let last_built_line = (mesh_info.built_at_scroll.abs() / line_height).floor() as usize;
            current_line.abs_diff(last_built_line) > 5
        } else {
            true // No mesh yet, need to build
        };

        if should_rebuild {
            state.needs_update = true;
        }
        state.needs_scroll_update = false;
    }
}

pub(crate) fn update_gpu_text_display(
    mut commands: Commands,
    mut state: ResMut<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    fold_state: Res<FoldState>,
    mut atlas: ResMut<GlyphAtlas>,
    render_state: Res<TextRenderState>,
    mut materials: ResMut<Assets<crate::gpu_text::TextMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mesh_query: Query<(Entity, &GpuTextMesh, &bevy::mesh::Mesh2d)>,
    mut syntax: ResMut<SyntaxResource>,
    mut highlight_cache: ResMut<HighlightCache>,
    time: Res<Time>,
) {
    use bevy::mesh::{Mesh2d, Indices, PrimitiveTopology};
    use bevy::asset::RenderAssetUsages;
    use crate::gpu_text::{GlyphKey, GlyphRasterizer};

    if !state.needs_update {
        return;
    }

    // NOTE: Tree-sitter update happens in separate async system
    // This allows text to render immediately without waiting for parsing

    let font_size = settings.font.size;
    let line_height = settings.font.line_height;
    let char_width = settings.font.char_width;

    // Calculate visible range
    let buffer = line_height * settings.performance.viewport_buffer_lines as f32;
    let total_buffer_lines = state.line_count();

    let scroll_dist = state.scroll_offset.abs();
    let start_pixels = scroll_dist - settings.ui.layout.margin_top - buffer;
    let first_visible_display_row = (start_pixels / line_height).floor().max(0.0) as usize;
    let visible_count = ((viewport.height as f32 + buffer * 2.0) / line_height).ceil() as usize;
    let last_visible_display_row = first_visible_display_row + visible_count;

    // Collect all visible glyph quads
    // Pre-allocate with estimated capacity to avoid reallocations
    let estimated_chars_per_line = 80;
    let estimated_capacity = visible_count * estimated_chars_per_line;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(estimated_capacity * 4);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(estimated_capacity * 4);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(estimated_capacity * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(estimated_capacity * 6);
    let mut vertex_count: u32 = 0;

    // === OPTIMIZATION: Skip directly to visible range instead of iterating from 0 ===
    let has_folding = !fold_state.regions.is_empty();

    let (start_buffer_line, mut current_display_row) = if has_folding {
        // With folding, we need to iterate to find the right buffer line
        let mut display_row = 0;
        let mut buffer_line = 0;
        while buffer_line < total_buffer_lines && display_row < first_visible_display_row {
            if !fold_state.is_line_hidden(buffer_line) {
                display_row += 1;
            }
            buffer_line += 1;
        }
        (buffer_line, display_row)
    } else {
        // No folding: display_row == buffer_line, jump directly
        let start = first_visible_display_row.min(total_buffer_lines);
        (start, start)
    };

    // Estimate end buffer line for lazy highlighting
    let estimated_end_buffer_line = (start_buffer_line + visible_count + 10).min(total_buffer_lines);

    // === LAZY HIGHLIGHTING with CACHING and DEBOUNCING ===
    // Like Zed: Only highlight visible range, cache results, debounce during fast scrolling
    #[cfg(feature = "tree-sitter")]
    let highlighted_lines = if syntax.is_available() && estimated_end_buffer_line > start_buffer_line {
        let current_time = time.elapsed_secs_f64() * 1000.0; // Convert to ms

        // Try to get from cache first
        if let Some(cached) = highlight_cache.get(start_buffer_line, estimated_end_buffer_line, state.content_version) {
            cached
        } else if highlight_cache.should_debounce(current_time) {
            // During fast scrolling, show plain text instead of re-highlighting every frame
            Vec::new()
        } else {
            // Extract ONLY the visible text range (not the entire file!)
            let start_char = state.rope.line_to_char(start_buffer_line);
            let end_char = state.rope.line_to_char(estimated_end_buffer_line.min(state.rope.len_lines()));
            // OPTIMIZATION: Use chunks instead of to_string() to avoid allocation
            let visible_text: String = state.rope.slice(start_char..end_char).chunks().collect();
            let start_byte = state.rope.char_to_byte(start_char);

            let lines = syntax.highlight_range(
                &visible_text,
                0, // Start from 0 since we're passing a slice
                estimated_end_buffer_line - start_buffer_line,
                start_byte, // Byte offset in the full document
                &settings.theme.syntax,
                settings.theme.foreground,
            );

            // Cache the result
            highlight_cache.insert(start_buffer_line, estimated_end_buffer_line, state.content_version, lines.clone());
            highlight_cache.mark_highlighted(current_time);
            lines
        }
    } else {
        Vec::new()
    };

    #[cfg(not(feature = "tree-sitter"))]
    let highlighted_lines: Vec<Vec<LineSegment>> = Vec::new();

    for buffer_line in start_buffer_line..total_buffer_lines {
        if fold_state.is_line_hidden(buffer_line) {
            continue;
        }

        if current_display_row > last_visible_display_row {
            break;
        }

        // Calculate base Y position
        // Add baseline offset to align GPU text with Text2d line numbers
        let baseline_offset = font_size * 0.32;
        let base_y = settings.ui.layout.margin_top + state.scroll_offset + (current_display_row as f32 * line_height) + baseline_offset;

        // Get text segments for this line
        // Use lazy-highlighted lines if available
        let relative_line = buffer_line.saturating_sub(start_buffer_line);

        // OPTIMIZATION: Avoid String clones - borrow from highlighted_lines or rope directly
        let segments_ref = if !highlighted_lines.is_empty() && relative_line < highlighted_lines.len() {
            Some(&highlighted_lines[relative_line])
        } else {
            None
        };

        // Build glyph quads for this line
        let mut x = settings.ui.layout.code_margin_left - state.horizontal_scroll_offset;

        // Process highlighted segments if available
        if let Some(segments) = segments_ref {
            for seg in segments {
                let color_rgba = seg.color.to_linear();
                let color_arr = [color_rgba.red, color_rgba.green, color_rgba.blue, color_rgba.alpha];

                for ch in seg.text.chars() {
                    if ch == '\n' || ch == '\r' {
                        continue;
                    }

                    if ch == '\t' {
                        x += char_width * 4.0;
                        continue;
                    }

                    let key = GlyphKey::new(ch, font_size);
                    if let Some(info) = atlas.get_or_insert(key, || {
                        GlyphRasterizer::rasterize(ch, font_size)
                    }) {
                        // Convert to Bevy coordinates (center origin, Y up)
                        let screen_x = x + info.offset.x;
                        let screen_y = base_y - info.offset.y;

                        // Convert screen coords to Bevy world coords
                        let world_x = screen_x - viewport.width as f32 / 2.0 + viewport.offset_x;
                        let world_y = viewport.height as f32 / 2.0 - screen_y;

                        // Create quad vertices (bottom-left origin)
                        let w = info.size.x;
                        let h = info.size.y;

                        // Four corners of the glyph quad
                        positions.push([world_x, world_y - h, 0.0]);       // bottom-left
                        positions.push([world_x + w, world_y - h, 0.0]);   // bottom-right
                        positions.push([world_x + w, world_y, 0.0]);       // top-right
                        positions.push([world_x, world_y, 0.0]);           // top-left

                        // UV coordinates from atlas
                        uvs.push([info.uv_min.x, info.uv_max.y]); // bottom-left (flipped Y)
                        uvs.push([info.uv_max.x, info.uv_max.y]); // bottom-right
                        uvs.push([info.uv_max.x, info.uv_min.y]); // top-right
                        uvs.push([info.uv_min.x, info.uv_min.y]); // top-left

                        // Colors for all 4 vertices
                        colors.push(color_arr);
                        colors.push(color_arr);
                        colors.push(color_arr);
                        colors.push(color_arr);

                        // Indices for two triangles
                        indices.push(vertex_count);
                        indices.push(vertex_count + 1);
                        indices.push(vertex_count + 2);
                        indices.push(vertex_count);
                        indices.push(vertex_count + 2);
                        indices.push(vertex_count + 3);

                        vertex_count += 4;
                        x += info.advance;
                    } else {
                        x += char_width;
                    }
                }
            }
        } else if buffer_line < state.rope.len_lines() {
            // Fallback: render directly from rope without highlighting
            let rope_line = state.rope.line(buffer_line);
            let color_rgba = settings.theme.foreground.to_linear();
            let color_arr = [color_rgba.red, color_rgba.green, color_rgba.blue, color_rgba.alpha];

            for ch in rope_line.chars() {
                if ch == '\n' || ch == '\r' {
                    continue;
                }

                if ch == '\t' {
                    x += char_width * 4.0;
                    continue;
                }

                let key = GlyphKey::new(ch, font_size);
                if let Some(info) = atlas.get_or_insert(key, || {
                    GlyphRasterizer::rasterize(ch, font_size)
                }) {
                    // Convert to Bevy coordinates (center origin, Y up)
                    let screen_x = x + info.offset.x;
                    let screen_y = base_y - info.offset.y;

                    // Convert screen coords to Bevy world coords
                    let world_x = screen_x - viewport.width as f32 / 2.0 + viewport.offset_x;
                    let world_y = viewport.height as f32 / 2.0 - screen_y;

                    // Create quad vertices (bottom-left origin)
                    let w = info.size.x;
                    let h = info.size.y;

                    // Four corners of the glyph quad
                    positions.push([world_x, world_y - h, 0.0]);       // bottom-left
                    positions.push([world_x + w, world_y - h, 0.0]);   // bottom-right
                    positions.push([world_x + w, world_y, 0.0]);       // top-right
                    positions.push([world_x, world_y, 0.0]);           // top-left

                    // UV coordinates from atlas
                    uvs.push([info.uv_min.x, info.uv_max.y]); // bottom-left (flipped Y)
                    uvs.push([info.uv_max.x, info.uv_max.y]); // bottom-right
                    uvs.push([info.uv_max.x, info.uv_min.y]); // top-right
                    uvs.push([info.uv_min.x, info.uv_min.y]); // top-left

                    // Colors for all 4 vertices
                    colors.push(color_arr);
                    colors.push(color_arr);
                    colors.push(color_arr);
                    colors.push(color_arr);

                    // Indices for two triangles
                    indices.push(vertex_count);
                    indices.push(vertex_count + 1);
                    indices.push(vertex_count + 2);
                    indices.push(vertex_count);
                    indices.push(vertex_count + 2);
                    indices.push(vertex_count + 3);

                    vertex_count += 4;
                    x += info.advance;
                } else {
                    x += char_width;
                }
            }
        }

        current_display_row += 1;
    }

    // Create or update the mesh
    let Some(material_handle) = &render_state.material_handle else {
        state.needs_update = false;
        return;
    };

    // Update the material's atlas texture to match the current atlas
    if let Some(material) = materials.get_mut(material_handle) {
        material.atlas_texture = atlas.texture.clone();
    }

    // Upload atlas changes to GPU
    atlas.update_texture(&mut images);

    if positions.is_empty() {
        // No visible text, hide existing mesh
        for (entity, _, _) in mesh_query.iter() {
            commands.entity(entity).insert(Visibility::Hidden);
        }
        state.needs_update = false;
        return;
    }

    // Build the mesh
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));

    // Update existing mesh or create new one
    if let Some((entity, _, mesh2d)) = mesh_query.iter().next() {
        // Replace the mesh handle to force re-upload
        let new_mesh_handle = meshes.add(mesh);
        commands.entity(entity).insert(Mesh2d(new_mesh_handle));
        commands.entity(entity).insert(Visibility::Visible);
        // Update scroll position marker
        commands.entity(entity).insert(GpuTextMesh {
            built_at_scroll: state.scroll_offset,
            built_at_horizontal_scroll: state.horizontal_scroll_offset,
            first_line: first_visible_display_row,
            last_line: last_visible_display_row,
        });
        // Remove the old mesh (it will be cleaned up automatically)
    } else {
        // Create new mesh entity
        let mesh_handle = meshes.add(mesh);
        commands.spawn((
            Mesh2d(mesh_handle),
            crate::gpu_text::MeshMaterial2d(material_handle.clone()),
            Transform::default(),
            GpuTextMesh {
                built_at_scroll: state.scroll_offset,
                built_at_horizontal_scroll: state.horizontal_scroll_offset,
                first_line: first_visible_display_row,
                last_line: last_visible_display_row,
            },
            Name::new("GpuTextMesh"),
            Visibility::Visible,
        ));
    }

    state.needs_update = false;
    // Update render time for debouncing (even though we bypass debounce for text edits)
    state.last_render_time = time.elapsed_secs_f64() * 1000.0;
}

/// Update the lines cache using syntax highlighting (used by minimap)
/// Note: The main editor uses lazy highlighting and doesn't need this
pub(crate) fn update_lines_cache(
    state: &mut CodeEditorState,
    settings: &EditorSettings,
    syntax: &mut SyntaxResource,
) {
    // === OPTIMIZATION: Skip entirely when no syntax highlighting ===
    // Rendering will read directly from rope for O(visible_lines) instead of O(all_lines)
    #[cfg(feature = "tree-sitter")]
    let has_syntax = syntax.is_available();

    #[cfg(not(feature = "tree-sitter"))]
    let has_syntax = false;

    if !has_syntax {
        state.lines.clear();
        state.last_lines_version = state.content_version;
        return;
    }

    // === OPTIMIZATION: Skip rebuilding if content hasn't changed ===
    if state.content_version == state.last_lines_version && !state.lines.is_empty() {
        return;
    }

    // Use syntax highlighting to get all lines
    // OPTIMIZATION: Use rope chunks instead of full to_string() conversion
    #[cfg(feature = "tree-sitter")]
    let lines = {
        let line_count = state.line_count();
        // Collect chunks efficiently
        let chunk_text: String = state.rope.chunks().collect();
        syntax.highlight_range(
            &chunk_text,
            0,
            line_count,
            0, // Start from byte 0 (full document)
            &settings.theme.syntax,
            settings.theme.foreground,
        )
    };

    #[cfg(not(feature = "tree-sitter"))]
    let lines: Vec<Vec<LineSegment>> = vec![Vec::new()];

    state.lines = lines.clone();

    // Build display map for soft line wrapping
    let wrap_width = if settings.wrapping.enabled {
        match settings.wrapping.mode {
            crate::settings::WrapMode::None => 0,
            crate::settings::WrapMode::Column => settings.wrapping.column,
            crate::settings::WrapMode::Viewport => {
                // Calculate wrap width from viewport
                // This is approximate - we'd need viewport info for exact width
                // For now, use column setting as fallback
                settings.wrapping.column
            }
        }
    } else {
        0
    };

    state.display_map.rebuild(&lines, wrap_width, settings.font.char_width);
    state.last_lines_version = state.content_version;
}

/// Map highlight type to color based on theme
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
