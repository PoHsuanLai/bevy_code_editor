//! GPU text rendering

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use crate::settings::*;
use crate::types::*;
use crate::gpu_text::{GlyphAtlas, TextRenderState};
use super::{SyntaxResource, HighlightCache};

/// Marker component for the main GPU text mesh (DEPRECATED - being replaced with per-line meshes)
#[derive(Component)]
pub struct GpuTextMesh;

/// Component for per-line mesh entities
/// Each visible line gets its own mesh entity for incremental updates
#[derive(Component)]
pub struct LineMeshEntity {
    /// Buffer line index this mesh represents
    pub buffer_line: usize,
    /// Display row (for Y positioning, accounting for folding)
    pub display_row: usize,
    /// Content version when this line was last rendered
    pub content_version: u64,
    /// Tree version when this line was last rendered (for syntax highlighting)
    pub tree_version: u64,
}

/// Resource to pool line mesh entities for reuse
#[derive(Resource, Default)]
pub struct LineMeshPool {
    /// Available entities that can be reused
    available: Vec<Entity>,
    /// Currently active line entities (buffer_line -> Entity)
    active: std::collections::HashMap<usize, Entity>,
}

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
    _highlight_cache: ResMut<HighlightCache>,
    mut parse_task_query: Query<(Entity, &mut ParseTask)>,
) {
    // Check if there's a completed parse task
    if let Some((entity, mut parse_task)) = parse_task_query.iter_mut().next() {
        // Poll the task without blocking
        if let Some(tree) = futures_lite::future::block_on(futures_lite::future::poll_once(&mut parse_task.task)) {
            if let Some(tree) = tree {
                // Update the syntax provider with the completed tree and current rope
                // This increments syntax.tree_version, which will trigger a re-render automatically
                syntax.set_parsed_tree(tree, &state.rope);
                state.last_highlighted_version = parse_task.content_version;

                // OPTIMIZATION: Smarter cache invalidation
                // Only clear cache if tree_version changed (parse produced different tree)
                // Cache entries with old tree_version will be naturally invalidated during lookup
                // This preserves cached highlights for unchanged regions
                // highlight_cache.clear(); // Removed - let version mismatch handle invalidation

                // Force a render update to display the new highlights immediately
                state.needs_update = true;
            }
            // Remove the completed task
            commands.entity(entity).despawn();
        }
        // Task still running, don't start a new one
        return;
    }

    // Only start a new parse if content changed and no task is running
    if state.content_version != state.last_highlighted_version && syntax.is_available() {
        info!("Starting tree-sitter parse task (content_version: {}, last_highlighted: {})",
              state.content_version, state.last_highlighted_version);
        // OPTIMIZATION: Don't clear cache here - let it invalidate naturally by version mismatch
        // This allows unchanged lines to remain cached during typing
        // highlight_cache.clear();

        // Clone rope for async task (Rope uses Arc internally so this is cheap)
        let rope = state.rope.clone();
        let content_version = state.content_version;

        // Clone the provider's state for incremental parsing (keeps main state intact)
        let (parser, language, cached_tree, pending_edits, deferred_edits) = syntax.clone_parse_state();

        // Spawn async parse task
        let task_pool = AsyncComputeTaskPool::get();
        let task = task_pool.spawn(async move {
            parse_tree_async(rope, parser, language, cached_tree, pending_edits, deferred_edits)
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
    deferred_edits: Vec<crate::syntax::tree_sitter::DeferredEdit>,
) -> Option<tree_sitter::Tree> {
    // Same parsing logic as update_tree, but runs async
    use crate::syntax::tree_sitter::RopeReader;
    use super::syntax_highlighting::byte_to_point;

    let mut reader = RopeReader::new(&rope);
    let mut callback = |byte_offset: usize, _position: tree_sitter::Point| -> &[u8] {
        reader.read(byte_offset)
    };

    // Try incremental parsing first
    if let Some(ref mut tree) = cached_tree {
        // Apply pending edits (already have full position info)
        for edit in pending_edits {
            tree.edit(&edit);
        }

        // Apply deferred edits (calculate Points now on async thread)
        // OPTIMIZATION: This expensive work happens off the main thread
        for deferred in deferred_edits {
            let start_position = byte_to_point(&rope, deferred.start_byte);
            let old_end_position = byte_to_point(&rope, deferred.old_end_byte);
            let new_end_position = byte_to_point(&rope, deferred.new_end_byte);

            let edit = tree_sitter::InputEdit {
                start_byte: deferred.start_byte,
                old_end_byte: deferred.old_end_byte,
                new_end_byte: deferred.new_end_byte,
                start_position,
                old_end_position,
                new_end_position,
            };
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

/// Convert scroll-only updates to full updates for GPU text
/// GPU text rendering requires rebuilding the mesh on scroll
/// For lazy syntax highlighting, we need to rebuild on every scroll to highlight newly visible lines
pub(crate) fn handle_scroll_for_gpu_text(
    mut state: ResMut<CodeEditorState>,
) {
    if state.needs_scroll_update {
        // Always rebuild on scroll to ensure newly visible lines get highlighted
        // The highlight cache prevents redundant work, so this is efficient
        state.needs_update = true;
        state.needs_scroll_update = false;
    }
}

pub(crate) fn update_gpu_text_display(
    mut commands: Commands,
    mut state: ResMut<CodeEditorState>,
    (font, theme, syntax_settings, performance): (Res<FontSettings>, Res<ThemeSettings>, Res<SyntaxSettings>, Res<PerformanceSettings>),
    viewport: Res<ViewportDimensions>,
    fold_state: Res<FoldState>,
    mut atlas: ResMut<GlyphAtlas>,
    render_state: Res<TextRenderState>,
    mut materials: ResMut<Assets<crate::gpu_text::TextMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mesh_query: Query<(Entity, &bevy::mesh::Mesh2d), With<GpuTextMesh>>,
    mut syntax: ResMut<SyntaxResource>,
    mut highlight_cache: ResMut<HighlightCache>,
    time: Res<Time>,
) {
    use bevy::mesh::{Mesh2d, Indices, PrimitiveTopology};
    use bevy::asset::RenderAssetUsages;
    use crate::gpu_text::{GlyphKey, GlyphRasterizer};

    // Check if we need to update due to tree-sitter parse completion
    #[cfg(feature = "tree-sitter")]
    let tree_updated = state.last_rendered_tree_version != syntax.tree_version;
    #[cfg(not(feature = "tree-sitter"))]
    let tree_updated = false;

    if !state.needs_update && !tree_updated {
        return;
    }

    // NOTE: Tree-sitter update happens in separate async system
    // This allows text to render immediately without waiting for parsing

    let font_size = font.size;
    let line_height = font.line_height;
    let char_width = font.char_width;

    // Calculate visible range
    let buffer = line_height * performance.viewport_buffer_lines as f32;
    let total_buffer_lines = state.line_count();

    let scroll_dist = state.scroll_offset.abs();
    let start_pixels = scroll_dist - viewport.text_area_top - buffer;
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

    // === LAZY HIGHLIGHTING with CACHING ===
    // Inspired by VS Code: Always show highlights when available, parse in background
    // The tree-sitter parsing already happens asynchronously, so we just need to highlight on-demand
    #[cfg(feature = "tree-sitter")]
    let highlighted_lines = if syntax.is_available() && estimated_end_buffer_line > start_buffer_line {
        // Try to get from cache first - ALWAYS use cache if available (no debounce on display)
        if let Some(cached) = highlight_cache.get(start_buffer_line, estimated_end_buffer_line, state.content_version, syntax.tree_version) {
            cached
        } else {
            // Cache miss - need to highlight this range
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
                &syntax_settings.theme,
                theme.foreground,
            );

            // Cache the result for future frames
            highlight_cache.insert(start_buffer_line, estimated_end_buffer_line, state.content_version, syntax.tree_version, lines.clone());
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
        let base_y = viewport.text_area_top + state.scroll_offset + (current_display_row as f32 * line_height) + baseline_offset;

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
        let mut x = viewport.text_area_left - state.horizontal_scroll_offset;

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
            let color_rgba = theme.foreground.to_linear();
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
        for (entity, _) in mesh_query.iter() {
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
    if let Some((entity, _mesh2d)) = mesh_query.iter().next() {
        // Replace the mesh handle to force re-upload
        let new_mesh_handle = meshes.add(mesh);
        commands.entity(entity).insert(Mesh2d(new_mesh_handle));
        commands.entity(entity).insert(Visibility::Visible);
    } else {
        // Create new mesh entity
        let mesh_handle = meshes.add(mesh);
        commands.spawn((
            Mesh2d(mesh_handle),
            crate::gpu_text::MeshMaterial2d(material_handle.clone()),
            Transform::default(),
            GpuTextMesh,  // Marker component to distinguish from minimap mesh
            Name::new("GpuTextMesh"),
            Visibility::Visible,
        ));
    }

    state.needs_update = false;
    // Update render time for debouncing (even though we bypass debounce for text edits)
    state.last_render_time = time.elapsed_secs_f64() * 1000.0;

    // Track that we've rendered with the current syntax tree version
    #[cfg(feature = "tree-sitter")]
    {
        state.last_rendered_tree_version = syntax.tree_version;
    }
}