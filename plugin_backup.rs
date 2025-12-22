//! Bevy plugin for GPU-accelerated code editor
//!
//! Renders text using Bevy's Text2d with proper alignment and viewport culling

use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::TextSpan;
use leafwing_input_manager::prelude::{InputManagerPlugin, InputMap, ActionState};
use crate::input::EditorAction;
use crate::settings::EditorSettings;
use crate::types::*;
use crate::gpu_text::{GpuTextPlugin, GlyphAtlas, TextRenderState};

/// Render mode for the code editor
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderMode {
    /// Standard 2D text rendering using Bevy's Text2d (default)
    #[default]
    Render2D,
    /// GPU-accelerated text rendering using custom glyph atlas and shaders
    /// Much faster for large files as it bypasses Bevy's text layout system
    GpuText,
    /// No rendering - only state and input management
    /// Useful for custom rendering implementations
    None,
}

/// Configuration for LSP UI rendering
#[derive(Clone, Copy, Debug, Default)]
pub struct LspUiConfig {
    /// Whether to enable the default LSP UI rendering systems
    /// Set to false to disable and use your own custom rendering
    pub enable_default_ui: bool,
}

impl LspUiConfig {
    /// Create config with default UI enabled
    pub fn enabled() -> Self {
        Self { enable_default_ui: true }
    }

    /// Create config with default UI disabled
    pub fn disabled() -> Self {
        Self { enable_default_ui: false }
    }
}

/// Code editor plugin
pub struct CodeEditorPlugin {
    settings: EditorSettings,
    render_mode: RenderMode,
    input_map: InputMap<EditorAction>,
    #[cfg(feature = "lsp")]
    lsp_ui_config: LspUiConfig,
}

impl CodeEditorPlugin {
    /// Create a new code editor plugin with the given input map
    ///
    /// # Example
    /// ```ignore
    /// use bevy::prelude::*;
    /// use bevy_code_editor::prelude::*;
    ///
    /// let input_map = InputMap::default()
    ///     .with(EditorAction::Copy, ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::KeyC]))
    ///     .with(EditorAction::Paste, ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::KeyV]));
    ///
    /// App::new()
    ///     .add_plugins(DefaultPlugins)
    ///     .add_plugins(CodeEditorPlugin::new(input_map))
    ///     .run();
    /// ```
    pub fn new(input_map: InputMap<EditorAction>) -> Self {
        Self {
            settings: EditorSettings::default(),
            render_mode: RenderMode::Render2D,
            input_map,
            #[cfg(feature = "lsp")]
            lsp_ui_config: LspUiConfig::enabled(),
        }
    }

    /// Set custom editor settings
    pub fn with_settings(mut self, settings: EditorSettings) -> Self {
        self.settings = settings;
        self
    }

    /// Set the render mode for the plugin
    pub fn with_render_mode(mut self, mode: RenderMode) -> Self {
        self.render_mode = mode;
        self
    }

    /// Configure LSP UI rendering
    ///
    /// Set to `false` to disable default LSP UI systems (completion popup, hover, etc.)
    /// and provide your own custom rendering by querying the marker components.
    ///
    /// # Example
    /// ```ignore
    /// use bevy_code_editor::prelude::*;
    /// use bevy_code_editor::lsp::components::*;
    ///
    /// // Disable default UI
    /// app.add_plugins(CodeEditorPlugin::default().with_lsp_ui(false));
    ///
    /// // Add your custom render system
    /// app.add_systems(Update, my_custom_completion_renderer);
    ///
    /// fn my_custom_completion_renderer(
    ///     query: Query<&CompletionPopupData, Changed<CompletionPopupData>>,
    ///     mut commands: Commands,
    /// ) {
    ///     // Your custom rendering logic
    /// }
    /// ```
    #[cfg(feature = "lsp")]
    pub fn with_lsp_ui(mut self, enable: bool) -> Self {
        self.lsp_ui_config = if enable {
            LspUiConfig::enabled()
        } else {
            LspUiConfig::disabled()
        };
        self
    }

    /// Create a plugin with only state management (no rendering)
    pub fn state_only(input_map: InputMap<EditorAction>) -> Self {
        Self {
            settings: EditorSettings::default(),
            render_mode: RenderMode::None,
            input_map,
            #[cfg(feature = "lsp")]
            lsp_ui_config: LspUiConfig::enabled(),
        }
    }
}

impl Default for CodeEditorPlugin {
    fn default() -> Self {
        Self::new(crate::input::default_input_map())
    }
}

/// Resource to hold the configured input map until it's spawned
#[derive(Resource)]
struct PendingInputMap(InputMap<EditorAction>);

impl Plugin for CodeEditorPlugin {
    fn build(&self, app: &mut App) {
        // Insert core resources (needed for all render modes)
        app.insert_resource(self.settings.clone());
        app.insert_resource(CodeEditorState::default());
        app.insert_resource(crate::input::MouseDragState::default());
        app.insert_resource(KeyRepeatState::default());

        // Store the configured input map for the spawn system
        app.insert_resource(PendingInputMap(self.input_map.clone()));

        // Register leafwing-input-manager plugin for action-based input
        app.add_plugins(InputManagerPlugin::<EditorAction>::default());

        // Spawn the input manager entity with configured keybindings
        // Users can query and modify the InputMap component to customize bindings at runtime
        app.add_systems(Startup, spawn_input_manager);

        // Add input handling systems (needed for all render modes)
        app.add_systems(
            Update,
            (
                crate::input::handle_keyboard_input,
                debounce_updates,
            ),
        );

        // Register editor events for file operations
        // These events are emitted by keybindings and should be handled by the host application
        app.add_message::<SaveRequested>();
        app.add_message::<OpenRequested>();

        // Add 2D-specific resources and systems only for 2D mode
        if self.render_mode == RenderMode::Render2D {
            app.insert_resource(ClearColor(self.settings.theme.background));
            app.insert_resource(ViewportDimensions::default());
            app.insert_resource(BracketMatchState::default());
            app.insert_resource(FindState::default());
            app.insert_resource(GotoLineState::default());
            app.insert_resource(MinimapHoverState::default());
            app.insert_resource(MinimapDragState::default());
            app.insert_resource(FoldState::default());

            app.add_systems(Startup, (init_viewport_from_window, setup).chain());
            // Split systems into two groups to avoid tuple size limits
            app.add_systems(
                Update,
                (
                    crate::input::handle_mouse_input,
                    crate::input::handle_mouse_wheel,
                    animate_smooth_scroll,
                    auto_scroll_to_cursor,
                    detect_viewport_resize,
                    update_separator_on_resize,
                    detect_foldable_regions,
                    update_scroll_only,
                    update_text_display,
                    update_line_numbers,
                    update_fold_indicators,
                )
                    .chain(),
            );
            app.add_systems(
                Update,
                (
                    update_selection_highlight,
                    update_cursor_line_highlight,
                    update_indent_guides,
                    update_bracket_match,
                    update_bracket_highlight,
                    update_find_highlights,
                    update_minimap_hover,
                    handle_minimap_mouse,
                    update_minimap,
                    update_minimap_find_highlights,
                    update_cursor,
                    animate_cursor,
                )
                    .chain()
                    .after(update_fold_indicators),
            );
        }

        // Add GPU text rendering mode
        if self.render_mode == RenderMode::GpuText {
            app.insert_resource(ClearColor(self.settings.theme.background));
            app.insert_resource(ViewportDimensions::default());
            app.insert_resource(BracketMatchState::default());
            app.insert_resource(FindState::default());
            app.insert_resource(GotoLineState::default());
            app.insert_resource(MinimapHoverState::default());
            app.insert_resource(MinimapDragState::default());
            app.insert_resource(FoldState::default());

            // Add the GPU text rendering plugin
            app.add_plugins(GpuTextPlugin);

            app.add_systems(Startup, (init_viewport_from_window, setup).chain());
            // GPU text rendering systems - split into smaller groups to avoid tuple limits
            app.add_systems(
                Update,
                (
                    crate::input::handle_mouse_input,
                    crate::input::handle_mouse_wheel,
                    animate_smooth_scroll,
                    auto_scroll_to_cursor,
                    detect_viewport_resize,
                    update_separator_on_resize,
                )
                    .chain(),
            );
            app.add_systems(
                Update,
                (
                    detect_foldable_regions,
                    update_scroll_only,
                    update_gpu_text_display,
                    update_line_numbers,
                    update_fold_indicators,
                )
                    .chain()
                    .after(update_separator_on_resize),
            );
            app.add_systems(
                Update,
                (
                    update_selection_highlight,
                    update_cursor_line_highlight,
                    update_indent_guides,
                    update_bracket_match,
                    update_bracket_highlight,
                    update_find_highlights,
                )
                    .chain()
                    .after(update_fold_indicators),
            );
            app.add_systems(
                Update,
                (
                    update_minimap_hover,
                    handle_minimap_mouse,
                    update_minimap,
                    update_minimap_find_highlights,
                    update_cursor,
                    animate_cursor,
                )
                    .chain()
                    .after(update_find_highlights),
            );
        }

        // Add LSP systems if feature is enabled
        #[cfg(feature = "lsp")]
        {
            use crate::lsp::prelude::*;
            use crate::lsp::state::{CodeActionState, SignatureHelpState, InlayHintState, DocumentHighlightState, RenameState};
            use crate::lsp::systems::{request_document_highlights, WorkspaceEditEvent};
            use crate::lsp::{LspUiSyncSet, LspUiRenderSet};

            // Core LSP resources
            app.insert_resource(LspClient::default());
            app.insert_resource(CompletionState::default());
            app.insert_resource(HoverState::default());
            app.insert_resource(LspSyncState::default());

            // New feature resources
            app.insert_resource(SignatureHelpState::default());
            app.insert_resource(CodeActionState::default());
            app.insert_resource(InlayHintState::default());
            app.insert_resource(DocumentHighlightState::default());
            app.insert_resource(RenameState::default());

            // Theme resource for UI customization
            app.insert_resource(LspUiTheme::default());

            // Register LSP events (messages) so external code can listen to them
            app.add_message::<NavigateToFileEvent>();
            app.add_message::<MultipleLocationsEvent>();
            app.add_message::<WorkspaceEditEvent>();

            // Configure system set ordering
            app.configure_sets(Update, LspUiSyncSet.before(LspUiRenderSet));

            // Core LSP systems (always enabled)
            app.add_systems(Update, (
                process_lsp_messages,
                sync_lsp_document,
                request_inlay_hints,
                request_document_highlights,
                cleanup_lsp_timeouts,
            ));

            // LSP UI sync systems (state -> marker components)
            // These always run so users can query marker components
            app.add_systems(Update, (
                sync_completion_popup,
                sync_hover_popup,
                sync_signature_help_popup,
                sync_code_actions_popup,
                sync_rename_input,
                sync_inlay_hints,
                sync_document_highlights,
            ).in_set(LspUiSyncSet));

            // LSP UI render systems (marker components -> visuals)
            // Only enabled if default UI is enabled
            if self.lsp_ui_config.enable_default_ui {
                app.add_systems(Update, (
                    render_completion_popup,
                    render_hover_popup,
                    render_signature_help_popup,
                    render_code_actions_popup,
                    render_rename_input,
                    render_inlay_hints,
                    render_document_highlights,
                    cleanup_lsp_ui_visuals,
                ).in_set(LspUiRenderSet));
            }
        }
    }
}

/// Marker component for the editor's input manager entity
#[derive(Component)]
pub struct EditorInputManager;

/// Spawn the input manager entity with configured keybindings
fn spawn_input_manager(mut commands: Commands, pending: Res<PendingInputMap>) {
    commands.spawn((
        EditorInputManager,
        pending.0.clone(),
        ActionState::<EditorAction>::default(),
        Name::new("EditorInputManager"),
    ));
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
    // Only update if separator is enabled and exists
    if !settings.ui.show_separator {
        return;
    }

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

    // Spawn separator line (only if enabled)
    if settings.ui.show_separator {
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
    }

    // Spawn primary cursor (cursor_index = 0)
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
        EditorCursor { cursor_index: 0 },
        Name::new("EditorCursor_0"),
    ));
}

/// Update scroll position only without despawning/spawning entities
fn update_scroll_only(
    mut state: ResMut<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    fold_state: Res<FoldState>,
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
    let buffer = line_height * settings.performance.viewport_buffer_lines as f32;
    let visible_top = -buffer;
    let visible_bottom = viewport_height + buffer;

    // Helper to count hidden lines before a given buffer line
    let count_hidden_lines_before = |line: usize| -> usize {
        fold_state.regions.iter()
            .filter(|r| r.is_folded && r.start_line < line)
            .map(|r| r.end_line.saturating_sub(r.start_line))
            .sum()
    };

    // Update text positions and visibility (vertical scroll only)
    // Note: horizontal scrolling requires full update because it changes text content
    for (line_marker, mut transform, mut visibility) in text_query.iter_mut() {
        let buffer_line = line_marker.index;

        // Hide lines that are hidden by folding
        if fold_state.is_line_hidden(buffer_line) {
            *visibility = Visibility::Hidden;
            continue;
        }

        // Calculate display row by subtracting hidden lines above
        let hidden_above = count_hidden_lines_before(buffer_line);
        let display_row = buffer_line.saturating_sub(hidden_above);

        let y = settings.ui.layout.margin_top + state.scroll_offset + (display_row as f32 * line_height);

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
        if let Some(line_num_str) = name.as_str().strip_prefix("LineNumber_buffer_") {
            if let Ok(buffer_line) = line_num_str.parse::<usize>() {
                // Hide lines that are hidden by folding
                let hidden_above = count_hidden_lines_before(buffer_line);
                let display_row = buffer_line.saturating_sub(hidden_above);

                let y = settings.ui.layout.margin_top + state.scroll_offset + (display_row as f32 * line_height);

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
    fold_state: Res<FoldState>,
    line_query: Query<(Entity, &mut Transform, &mut Visibility, &HighlightedTextToken)>,
    children_query: Query<&Children>,
    mut span_query: Query<(&mut TextSpan, &mut TextColor)>,
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
    // Use configurable buffer to prevent blackouts during fast scrolling
    let buffer = line_height * settings.performance.viewport_buffer_lines as f32;
    let visible_top = -buffer;
    let visible_bottom = viewport.height as f32 + buffer;

    // Calculate maximum content width for horizontal scrolling
    let char_width = settings.font.char_width;

    // Check if we're using soft line wrapping
    let use_wrapping = settings.wrapping.enabled && state.display_map.wrap_width > 0;

    // Get total line count
    let total_buffer_lines = state.line_count();

    // Calculate visible display row range
    let scroll_dist = state.scroll_offset.abs();
    let start_pixels = scroll_dist - settings.ui.layout.margin_top - buffer;
    let first_visible_display_row = (start_pixels / line_height).floor().max(0.0) as usize;
    let visible_count = ((viewport.height as f32 + buffer * 2.0) / line_height).ceil() as usize;
    let last_visible_display_row = first_visible_display_row + visible_count;

    // === OPTIMIZATION: Viewport-based max width (like Monaco/VS Code) ===
    // Only scan visible lines for max width - O(visible_lines) not O(all_lines)
    // The scrollbar grows dynamically as user scrolls through the document
    if use_wrapping {
        state.max_content_width = state.display_map.wrap_width as f32 * char_width;
    } else {
        // Update max from currently visible lines
        // Scan visible lines and update cached max
        let visible_start = first_visible_display_row.min(total_buffer_lines);
        let visible_end = (last_visible_display_row + 1).min(total_buffer_lines);
        let mut new_max = state.line_width_tracker.max_width();

        for line_idx in visible_start..visible_end {
            if line_idx < state.rope.len_lines() {
                let line = state.rope.line(line_idx);
                let len = line.len_chars();
                let width = if len > 0 && line.char(len - 1) == '\n' {
                    (len - 1) as u32
                } else {
                    len as u32
                };
                if width > new_max {
                    new_max = width;
                }
            }
        }

        // Update tracker's cached max if we found a longer line
        if new_max > state.line_width_tracker.max_width() {
            state.line_width_tracker.update_line(0, new_max);
        }

        state.max_content_width = new_max as f32 * char_width;
    }

    // === OPTIMIZATION: Entity Pooling ===
    // Gather all existing line entities into a reusable pool
    let mut entity_pool: Vec<(Entity, Transform, Visibility)> = line_query
        .iter()
        .map(|(e, t, v, _)| (e, *t, *v))
        .collect();

    // Track current display row (accounts for folding)
    let mut current_display_row: usize = 0;

    // Process buffer lines, tracking display row position
    for buffer_line in 0..total_buffer_lines {
        // Skip lines that are hidden due to folding
        if fold_state.is_line_hidden(buffer_line) {
            continue;
        }

        // Check if this display row is in visible range
        if current_display_row > last_visible_display_row {
            break; // Past visible area, done
        }

        if current_display_row >= first_visible_display_row {
            // Calculate Y position based on display row
            let y = settings.ui.layout.margin_top + state.scroll_offset + (current_display_row as f32 * line_height);
            let is_visible = y >= visible_top && y <= visible_bottom;

            // Default x offset (no continuation)
            let x_offset = settings.ui.layout.code_margin_left;

            let translation = to_bevy_coords_left_aligned(
                x_offset,
                y,
                viewport.width as f32,
                viewport.height as f32,
                viewport.offset_x,
                0.0,  // No horizontal scroll for wrapped lines
            );

            // === OPTIMIZATION: Get visible segments - either from cache or directly from rope ===
            let visible_segments: Vec<(String, Color)> = if state.has_syntax_highlighting {
                // Syntax highlighting enabled - use cached line segments
                if buffer_line >= state.lines.len() {
                    current_display_row += 1;
                    continue;
                }
                let line_segments = &state.lines[buffer_line];

                if use_wrapping {
                    // Wrapped mode: use pre-computed segments directly
                    line_segments.iter().map(|seg| (seg.text.clone(), seg.color)).collect()
                } else {
                    // Non-wrapped mode: apply horizontal scrolling
                    let chars_to_skip = (state.horizontal_scroll_offset / char_width).floor() as usize;
                    let mut original_char_idx = 0;
                    let mut segments = Vec::new();

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
                            segments.push((segment.text.clone(), segment.color));
                        } else {
                            // Segment is partially visible (scrolled from left)
                            let skip_in_segment = chars_to_skip - original_char_idx;
                            let visible_part: String = segment.text.chars().skip(skip_in_segment).collect();
                            if !visible_part.is_empty() {
                                segments.push((visible_part, segment.color));
                            }
                        }

                        original_char_idx = segment_end;
                    }
                    segments
                }
            } else {
                // === OPTIMIZATION: No syntax highlighting - read directly from rope ===
                // This is O(1) per visible line instead of O(all_lines) upfront
                if buffer_line >= state.rope.len_lines() {
                    current_display_row += 1;
                    continue;
                }

                let rope_line = state.rope.line(buffer_line);
                let line_len = rope_line.len_chars();
                // Remove trailing newline if present
                let text_len = if line_len > 0 && rope_line.char(line_len - 1) == '\n' {
                    line_len - 1
                } else {
                    line_len
                };

                if text_len == 0 {
                    Vec::new()
                } else {
                    // Apply horizontal scrolling
                    let chars_to_skip = (state.horizontal_scroll_offset / char_width).floor() as usize;
                    if chars_to_skip >= text_len {
                        Vec::new()
                    } else {
                        // Get visible portion of the line directly from rope
                        let visible_text: String = rope_line.chars().skip(chars_to_skip).take(text_len - chars_to_skip).collect();
                        vec![(visible_text, settings.theme.foreground)]
                    }
                }
            };

            // Add fold ellipsis indicator if this line is the start of a folded region
            let visible_segments: Vec<(String, Color)> = if fold_state.is_folded_line(buffer_line) {
                let mut segments = visible_segments;
                // Append "..." in a dimmed color to indicate folded content
                let fold_indicator_color = settings.theme.line_numbers.with_alpha(0.8);
                segments.push((" ...".to_string(), fold_indicator_color));
                segments
            } else {
                visible_segments
            };

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
                    HighlightedTextToken { index: buffer_line },
                    Name::new(format!("Row_{}", buffer_line)),
                ));

                // === OPTIMIZATION: Reuse TextSpan children instead of despawn/respawn ===
                let existing_children: Vec<Entity> = children_query
                    .get(entity)
                    .map(|c| c.iter().collect())
                    .unwrap_or_default();

                let needed_spans = visible_segments.len().saturating_sub(1);
                let existing_span_count = existing_children.len();

                // Update parent Text2d
                if !visible_segments.is_empty() {
                    let first_segment = &visible_segments[0];
                    commands.entity(entity).insert((
                        Text2d::new(first_segment.0.clone()),
                        text_font.clone(),
                        TextColor(first_segment.1),
                    ));

                    // Reuse existing TextSpan children where possible
                    for (idx, (text, color)) in visible_segments.iter().skip(1).enumerate() {
                        if idx < existing_span_count {
                            // Reuse existing span
                            let child_entity = existing_children[idx];
                            if let Ok((mut span, mut span_color)) = span_query.get_mut(child_entity) {
                                span.0 = text.clone();
                                span_color.0 = *color;
                            }
                        } else {
                            // Need to spawn a new span
                            commands.entity(entity).with_child((
                                TextSpan::new(text.clone()),
                                text_font.clone(),
                                TextColor(*color),
                            ));
                        }
                    }

                    // Despawn excess children (only if we have more than needed)
                    for child in existing_children.iter().skip(needed_spans) {
                        commands.entity(*child).despawn();
                    }
                } else {
                    // Empty line - despawn all children
                    for child in existing_children.iter() {
                        commands.entity(*child).despawn();
                    }
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
                        HighlightedTextToken { index: buffer_line },
                        Name::new(format!("Row_{}", buffer_line)),
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
                        HighlightedTextToken { index: buffer_line },
                        Name::new(format!("Row_{}", buffer_line)),
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

        // Increment display row for visible lines
        current_display_row += 1;
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
    // === OPTIMIZATION: Skip entirely when no syntax highlighting ===
    // Rendering will read directly from rope for O(visible_lines) instead of O(all_lines)
    if !state.has_syntax_highlighting {
        state.lines.clear();
        state.last_lines_version = state.content_version;
        return;
    }

    // === OPTIMIZATION: Skip rebuilding if content hasn't changed ===
    if state.content_version == state.last_lines_version && !state.lines.is_empty() {
        return;
    }

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
    fold_state: Res<FoldState>,
    mut line_numbers_query: Query<(&mut Text2d, &mut Transform, &mut Visibility, &mut TextColor), With<LineNumbers>>,
) {
    // Hide all line numbers if disabled in settings
    if !settings.ui.show_line_numbers {
        for (_, _, mut visibility, _) in line_numbers_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    if !state.is_changed() && !fold_state.is_changed() {
        return;
    }

    let line_height = settings.font.line_height;
    let font_size = settings.font.size;

    // Collect cursor lines for highlighting active line numbers
    let cursor_lines: std::collections::HashSet<usize> = state
        .cursors
        .iter()
        .map(|c| {
            let pos = c.position.min(state.rope.len_chars());
            state.rope.char_to_line(pos)
        })
        .collect();

    // Check if we're using soft line wrapping
    let use_wrapping = settings.wrapping.enabled && state.display_map.wrap_width > 0;

    // Use configurable buffer for viewport calculations
    let buffer_lines = settings.performance.viewport_buffer_lines as f32;
    let viewport_top = -state.scroll_offset - line_height * buffer_lines;
    let viewport_bottom = viewport_top + viewport.height as f32 + line_height * buffer_lines * 2.0;

    let first_visible_display_row =
        ((viewport_top - settings.ui.layout.margin_top) / line_height).floor().max(0.0) as usize;
    let last_visible_display_row =
        ((viewport_bottom - settings.ui.layout.margin_top) / line_height).ceil() as usize;

    let total_buffer_lines = state.line_count();

    let mut existing_line_numbers: Vec<_> = line_numbers_query.iter_mut().collect();
    let mut entity_index = 0;

    // === OPTIMIZATION: Skip to visible start instead of iterating from 0 ===
    let has_folding = !fold_state.regions.is_empty();

    // Calculate starting buffer line and display row
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

    // Iterate over buffer lines starting from visible area
    for buffer_line in start_buffer_line..total_buffer_lines {
        // Skip lines that are hidden due to folding
        if fold_state.is_line_hidden(buffer_line) {
            continue;
        }

        // For wrapped mode, handle continuation rows
        let is_continuation = if use_wrapping {
            // In wrapped mode, we need to check if this display row is a continuation
            // For now, we'll use the simpler approach without wrapping for folded content
            false
        } else {
            false
        };

        // All lines from start_buffer_line should be in or after visible range
        if current_display_row <= last_visible_display_row {
            // Calculate Y position based on display row (not buffer line)
            let y = settings.ui.layout.margin_top + state.scroll_offset + (current_display_row as f32 * line_height);
            let translation = to_bevy_coords_left_aligned(
                settings.ui.layout.line_number_margin_left,
                y,
                viewport.width as f32,
                viewport.height as f32,
                viewport.offset_x,
                0.0,  // line numbers don't scroll horizontally
            );

            // For continuation rows, show empty or continuation indicator
            let line_number_text = if is_continuation {
                // Show nothing or a continuation indicator for wrapped lines
                String::new()
            } else {
                // Show actual buffer line number (1-indexed)
                (buffer_line + 1).to_string()
            };

            // Use active color for cursor lines
            let line_color = if cursor_lines.contains(&buffer_line) {
                settings.theme.line_numbers_active
            } else {
                settings.theme.line_numbers
            };

            if entity_index < existing_line_numbers.len() {
                let (ref mut text, ref mut transform, ref mut visibility, ref mut text_color) =
                    &mut existing_line_numbers[entity_index];
                text.0 = line_number_text;
                transform.translation = translation;
                text_color.0 = line_color;
                **visibility = Visibility::Visible;
            } else {
                let text_font = TextFont {
                    font: settings.font.handle.clone().unwrap_or_default(),
                    font_size,
                    ..default()
                };

                commands.spawn((
                    Text2d::new(line_number_text),
                    text_font,
                    TextColor(line_color),
                    Transform::from_translation(translation),
                    LineNumbers,
                    Name::new(format!("LineNumber_buffer_{}", buffer_line)),
                    Visibility::Visible,
                ));
            }

            entity_index += 1;
        }

        current_display_row += 1;

        // Early exit if we've passed the visible area
        if current_display_row > last_visible_display_row {
            break;
        }
    }

    // Hide unused line numbers
    for i in entity_index..existing_line_numbers.len() {
        let (_, _, ref mut visibility, _) = &mut existing_line_numbers[i];
        **visibility = Visibility::Hidden;
    }
}

/// Update selection highlight rectangles for all cursors
fn update_selection_highlight(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    fold_state: Res<FoldState>,
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

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;

    // Check if we're using soft line wrapping
    let use_wrapping = settings.wrapping.enabled && state.display_map.wrap_width > 0;

    // Collect all selection ranges from all cursors
    // (cursor_idx, display_row, start_col, end_col, is_continuation)
    let mut selection_rects: Vec<(usize, usize, usize, usize, bool)> = Vec::new();

    for (cursor_idx, cursor) in state.cursors.iter().enumerate() {
        if let Some((start, end)) = cursor.selection_range() {
            if start == end {
                continue;
            }

            let start_line = state.rope.char_to_line(start);
            let end_line = state.rope.char_to_line(end);

            for line_idx in start_line..=end_line {
                // Skip hidden lines
                if fold_state.is_line_hidden(line_idx) {
                    continue;
                }

                let line_start_char = state.rope.line_to_char(line_idx);
                let line = state.rope.line(line_idx);

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

                if sel_start_in_line < sel_end_in_line {
                    if use_wrapping {
                        // For wrapped mode, split selection across display rows
                        for (row_idx, row) in state.display_map.rows.iter().enumerate() {
                            if row.buffer_line != line_idx {
                                continue;
                            }
                            // Calculate overlap between selection and this row
                            let row_sel_start = sel_start_in_line.max(row.start_offset);
                            let row_sel_end = sel_end_in_line.min(row.end_offset);

                            if row_sel_start < row_sel_end {
                                // Convert to display column (relative to row start)
                                let display_start = row_sel_start - row.start_offset;
                                let display_end = row_sel_end - row.start_offset;
                                selection_rects.push((cursor_idx, row_idx, display_start, display_end, row.is_continuation));
                            }
                        }
                    } else {
                        // Convert buffer line to display row
                        let display_row = fold_state.actual_to_display_line(line_idx);
                        selection_rects.push((cursor_idx, display_row, sel_start_in_line, sel_end_in_line, false));
                    }
                }
            }
        }
    }

    // Also handle backward-compatible selection_start/selection_end if cursors is empty/mismatched
    if state.cursors.is_empty() || (state.cursors.len() == 1 && state.selection_start.is_some()) {
        if let (Some(sel_start), Some(sel_end)) = (state.selection_start, state.selection_end) {
            let (start, end) = if sel_start <= sel_end {
                (sel_start, sel_end)
            } else {
                (sel_end, sel_start)
            };

            if start != end && selection_rects.is_empty() {
                let start_line = state.rope.char_to_line(start);
                let end_line = state.rope.char_to_line(end);

                for line_idx in start_line..=end_line {
                    // Skip hidden lines
                    if fold_state.is_line_hidden(line_idx) {
                        continue;
                    }

                    let line_start_char = state.rope.line_to_char(line_idx);
                    let line = state.rope.line(line_idx);

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

                    if sel_start_in_line < sel_end_in_line {
                        if use_wrapping {
                            for (row_idx, row) in state.display_map.rows.iter().enumerate() {
                                if row.buffer_line != line_idx {
                                    continue;
                                }
                                let row_sel_start = sel_start_in_line.max(row.start_offset);
                                let row_sel_end = sel_end_in_line.min(row.end_offset);

                                if row_sel_start < row_sel_end {
                                    let display_start = row_sel_start - row.start_offset;
                                    let display_end = row_sel_end - row.start_offset;
                                    selection_rects.push((0, row_idx, display_start, display_end, row.is_continuation));
                                }
                            }
                        } else {
                            // Convert buffer line to display row
                            let display_row = fold_state.actual_to_display_line(line_idx);
                            selection_rects.push((0, display_row, sel_start_in_line, sel_end_in_line, false));
                        }
                    }
                }
            }
        }
    }

    // Clear all if no selections
    if selection_rects.is_empty() {
        for (_, _, _, mut visibility, _) in selection_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    let mut existing_selections: Vec<_> = selection_query.iter_mut().collect();
    let mut entity_index = 0;

    for (cursor_idx, row_idx, sel_start_col, sel_end_col, is_continuation) in selection_rects {
        let selection_width = (sel_end_col - sel_start_col) as f32 * char_width;

        // Add continuation indent for wrapped lines
        let extra_indent = if use_wrapping && is_continuation && settings.wrapping.indent_wrapped_lines {
            settings.indentation.indent_size as f32 * char_width
        } else {
            0.0
        };

        let x_left_edge = settings.ui.layout.code_margin_left + extra_indent + (sel_start_col as f32 * char_width);
        let y_from_top = settings.ui.layout.margin_top + state.scroll_offset + (row_idx as f32 * line_height);

        let sprite_center_x =
            -(viewport.width as f32) / 2.0 + x_left_edge + selection_width / 2.0;
        let sprite_center_y = (viewport.height as f32) / 2.0 - y_from_top;

        let translation = Vec3::new(sprite_center_x, sprite_center_y, 0.5);

        if entity_index < existing_selections.len() {
            let (_, ref mut transform, ref mut sprite, ref mut visibility, ref mut marker) =
                &mut existing_selections[entity_index];
            sprite.custom_size = Some(Vec2::new(selection_width, line_height));
            transform.translation = translation;
            marker.line_index = row_idx;
            marker.cursor_index = cursor_idx;
            **visibility = Visibility::Visible;
        } else {
            commands.spawn((
                Sprite {
                    color: settings.theme.selection_background,
                    custom_size: Some(Vec2::new(selection_width, line_height)),
                    ..default()
                },
                Transform::from_translation(translation),
                SelectionHighlight { line_index: row_idx, cursor_index: cursor_idx },
                Name::new(format!("Selection_C{}_R{}", cursor_idx, row_idx)),
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

/// Update indent guide rendering
fn update_indent_guides(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    fold_state: Res<FoldState>,
    mut guide_query: Query<(Entity, &mut Transform, &mut Visibility, &mut IndentGuide)>,
) {
    // Hide all guides if disabled
    if !settings.ui.show_indent_guides {
        for (_, _, mut visibility, _) in guide_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    if !state.is_changed() {
        return;
    }

    let line_height = settings.font.line_height;
    let char_width = settings.font.char_width;
    let indent_size = settings.indentation.indent_size;
    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    // Calculate visible display row range
    let visible_start_row = ((-state.scroll_offset) / line_height).floor() as usize;
    let visible_lines = ((viewport_height / line_height).ceil() as usize) + 2;
    let visible_end_row = visible_start_row + visible_lines;

    // Collect guides needed for visible lines
    // Each guide is identified by (display_row, indent_level)
    let mut needed_guides: Vec<(usize, usize)> = Vec::new();

    // === OPTIMIZATION: Start from approximate visible row instead of row 0 ===
    // For files with no folding, we can jump directly to the visible start
    // This changes O(all_lines) to O(visible_lines)
    let total_lines = state.rope.len_lines();
    let has_folding = !fold_state.regions.is_empty();

    // Calculate starting buffer line
    let start_buffer_line = if has_folding {
        // With folding, we need to iterate to find the right buffer line
        // But we can still skip most lines quickly
        let mut display_row = 0;
        let mut buffer_line = 0;
        while buffer_line < total_lines && display_row < visible_start_row {
            if !fold_state.is_line_hidden(buffer_line) {
                display_row += 1;
            }
            buffer_line += 1;
        }
        buffer_line
    } else {
        // No folding: display_row == buffer_line, jump directly
        visible_start_row.min(total_lines)
    };

    // Start display row at visible_start_row (or the actual row if we started earlier)
    let mut current_display_row: usize = if has_folding {
        // With folding, we tracked this while finding start_buffer_line
        let mut display_row = 0;
        for bl in 0..start_buffer_line {
            if !fold_state.is_line_hidden(bl) {
                display_row += 1;
            }
        }
        display_row
    } else {
        start_buffer_line
    };

    // Iterate only through visible buffer lines
    for buffer_line in start_buffer_line..total_lines {
        // Skip hidden lines
        if fold_state.is_line_hidden(buffer_line) {
            continue;
        }

        // Stop if past visible range
        if current_display_row > visible_end_row {
            break;
        }

        let line = state.rope.line(buffer_line);

        // Count leading whitespace to determine indentation
        let mut leading_spaces = 0;
        for c in line.chars() {
            match c {
                ' ' => leading_spaces += 1,
                '\t' => leading_spaces += indent_size,
                _ => break,
            }
        }

        // Calculate number of indent levels
        let indent_levels = leading_spaces / indent_size;

        // Add a guide for each indent level (using display_row for position)
        for level in 0..indent_levels {
            needed_guides.push((current_display_row, level));
        }

        current_display_row += 1;
    }

    // Collect existing guide entities
    let mut existing_guides: Vec<_> = guide_query.iter_mut().collect();
    let mut entity_index = 0;

    for (display_row, level) in needed_guides.iter() {
        let x_offset = settings.ui.layout.code_margin_left + (*level * indent_size) as f32 * char_width;
        let y_offset = settings.ui.layout.margin_top + state.scroll_offset + (*display_row as f32 * line_height);

        // Position the guide line (thin vertical line)
        let sprite_x = -viewport_width / 2.0 + x_offset - state.horizontal_scroll_offset + viewport.offset_x;
        let sprite_y = viewport_height / 2.0 - y_offset;
        let translation = Vec3::new(sprite_x, sprite_y, 0.1); // z=0.1 behind text

        if entity_index < existing_guides.len() {
            // Reuse existing entity
            let (_, ref mut transform, ref mut visibility, ref mut guide) = &mut existing_guides[entity_index];
            transform.translation = translation;
            guide.level = *level;
            guide.line_index = *display_row;
            **visibility = Visibility::Visible;
        } else {
            // Spawn new guide entity
            commands.spawn((
                Sprite {
                    color: settings.theme.indent_guide,
                    custom_size: Some(Vec2::new(1.0, line_height)),
                    ..default()
                },
                Transform::from_translation(translation),
                IndentGuide {
                    level: *level,
                    line_index: *display_row,
                },
                Name::new(format!("IndentGuide_{}_{}", display_row, level)),
                Visibility::Visible,
            ));
        }

        entity_index += 1;
    }

    // Hide unused guide entities
    for i in entity_index..existing_guides.len() {
        let (_, _, ref mut visibility, _) = &mut existing_guides[i];
        **visibility = Visibility::Hidden;
    }
}

/// Animate smooth scrolling by interpolating towards target scroll offset
fn animate_smooth_scroll(
    mut state: ResMut<CodeEditorState>,
    time: Res<Time>,
    settings: Res<EditorSettings>,
) {
    if !settings.scrolling.smooth_scrolling {
        // When smooth scrolling is disabled, sync target with actual
        state.target_scroll_offset = state.scroll_offset;
        state.target_horizontal_scroll_offset = state.horizontal_scroll_offset;
        return;
    }

    // Smooth scrolling interpolation factor (higher = faster)
    // Using exponential decay for natural feel
    let smoothness = 12.0; // Adjust for desired smoothness
    let dt = time.delta_secs();
    let t = 1.0 - (-smoothness * dt).exp();

    // Vertical scroll animation
    let vertical_diff = state.target_scroll_offset - state.scroll_offset;
    if vertical_diff.abs() > 0.1 {
        state.scroll_offset += vertical_diff * t;
        state.needs_scroll_update = true;
    } else if vertical_diff.abs() > 0.0 {
        // Snap to target when close enough
        state.scroll_offset = state.target_scroll_offset;
        state.needs_scroll_update = true;
    }

    // Horizontal scroll animation
    let horizontal_diff = state.target_horizontal_scroll_offset - state.horizontal_scroll_offset;
    if horizontal_diff.abs() > 0.1 {
        state.horizontal_scroll_offset += horizontal_diff * t;
        state.needs_update = true; // Horizontal scroll needs full update
    } else if horizontal_diff.abs() > 0.0 {
        // Snap to target when close enough
        state.horizontal_scroll_offset = state.target_horizontal_scroll_offset;
        state.needs_update = true;
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

/// Update cursor position for all cursors
fn update_cursor(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    fold_state: Res<FoldState>,
    mut cursor_query: Query<(Entity, &EditorCursor, &mut Transform, &mut Visibility)>,
) {
    if !state.is_changed() {
        return;
    }

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;
    let cursor_height = line_height * settings.cursor.height_multiplier;
    let cursor_count = state.cursors.len();

    // Check if we're using soft line wrapping
    let use_wrapping = settings.wrapping.enabled && state.display_map.wrap_width > 0;

    // Collect existing cursor entities by their index
    let mut cursor_entities: std::collections::HashMap<usize, Entity> = std::collections::HashMap::new();
    for (entity, cursor, _, _) in cursor_query.iter() {
        cursor_entities.insert(cursor.cursor_index, entity);
    }

    // Update or create cursor entities for each cursor
    for (idx, cursor) in state.cursors.iter().enumerate() {
        let cursor_pos = cursor.position.min(state.rope.len_chars());
        let line_index = state.rope.char_to_line(cursor_pos);
        let line_start = state.rope.line_to_char(line_index);
        let col_index = cursor_pos - line_start;

        // Calculate display row and column based on wrapping and folding
        let (display_row, display_col) = if use_wrapping {
            state.display_map.buffer_to_display(line_index, col_index)
        } else {
            // Account for folded lines
            let display_row = fold_state.actual_to_display_line(line_index);
            (display_row, col_index)
        };

        // For wrapped continuation rows, add indent offset
        let extra_indent = if use_wrapping && settings.wrapping.indent_wrapped_lines {
            if state.display_map.is_continuation(display_row) {
                settings.indentation.indent_size as f32 * char_width
            } else {
                0.0
            }
        } else {
            0.0
        };

        let x_offset = settings.ui.layout.code_margin_left + extra_indent + (display_col as f32 * char_width);
        let y_offset = settings.ui.layout.margin_top + state.scroll_offset + (display_row as f32 * line_height);

        // No horizontal scroll in wrapped mode
        let h_scroll = if use_wrapping { 0.0 } else { state.horizontal_scroll_offset };

        let translation = to_bevy_coords_left_aligned(
            x_offset,
            y_offset,
            viewport.width as f32,
            viewport.height as f32,
            viewport.offset_x,
            h_scroll,
        );

        if let Some(&entity) = cursor_entities.get(&idx) {
            // Update existing cursor entity
            if let Ok((_, _, mut transform, mut visibility)) = cursor_query.get_mut(entity) {
                transform.translation = Vec3::new(translation.x, translation.y, 1.0);
                *visibility = Visibility::Visible;
            }
            cursor_entities.remove(&idx);
        } else {
            // Spawn new cursor entity
            commands.spawn((
                Sprite {
                    color: settings.theme.cursor,
                    custom_size: Some(Vec2::new(settings.cursor.width, cursor_height)),
                    ..default()
                },
                Transform::from_translation(Vec3::new(translation.x, translation.y, 1.0)),
                Visibility::Visible,
                EditorCursor { cursor_index: idx },
                Name::new(format!("EditorCursor_{}", idx)),
            ));
        }
    }

    // Hide or despawn excess cursor entities
    for (idx, entity) in cursor_entities {
        if idx < cursor_count {
            // This shouldn't happen, but hide just in case
            if let Ok((_, _, _, mut visibility)) = cursor_query.get_mut(entity) {
                *visibility = Visibility::Hidden;
            }
        } else {
            // Despawn cursor entities that are no longer needed
            commands.entity(entity).despawn();
        }
    }
}

/// Animate cursor blinking for all cursors
fn animate_cursor(
    time: Res<Time>,
    settings: Res<EditorSettings>,
    mut cursor_query: Query<&mut Visibility, With<EditorCursor>>,
) {
    if settings.cursor.blink_rate == 0.0 {
        for mut visibility in cursor_query.iter_mut() {
            *visibility = Visibility::Visible;
        }
        return;
    }

    let blink_phase = (time.elapsed_secs() * settings.cursor.blink_rate) % 1.0;
    let new_visibility = if blink_phase < 0.5 {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    for mut visibility in cursor_query.iter_mut() {
        *visibility = new_visibility;
    }
}

/// Update cursor line borders and word highlight (VSCode-style)
/// Shows thin lines above/below current line + highlights the word under cursor
fn update_cursor_line_highlight(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    fold_state: Res<FoldState>,
    mut border_query: Query<(Entity, &CursorLineBorder, &mut Transform, &mut Sprite, &mut Visibility)>,
    mut word_query: Query<(Entity, &CursorWordHighlight, &mut Transform, &mut Sprite, &mut Visibility), Without<CursorLineBorder>>,
) {
    let cursor_line_settings = &settings.cursor_line;

    // Skip if cursor line highlighting is disabled entirely
    if !cursor_line_settings.enabled {
        // Hide all existing borders and word highlights
        for (_, _, _, _, mut visibility) in border_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        for (_, _, _, _, mut visibility) in word_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    // Get base highlight color from theme
    let base_highlight_color = match settings.theme.line_highlight {
        Some(color) => color,
        None => {
            // Hide all existing borders and word highlights
            for (_, _, _, _, mut visibility) in border_query.iter_mut() {
                *visibility = Visibility::Hidden;
            }
            for (_, _, _, _, mut visibility) in word_query.iter_mut() {
                *visibility = Visibility::Hidden;
            }
            return;
        }
    };

    if !state.is_changed() {
        return;
    }

    let line_height = settings.font.line_height;
    let char_width = settings.font.char_width;
    let use_wrapping = settings.wrapping.enabled && state.display_map.wrap_width > 0;

    // Border settings from configuration
    let border_thickness = cursor_line_settings.border_thickness;
    let border_color = cursor_line_settings.border_color.unwrap_or_else(|| {
        // Use base highlight color with configurable alpha multiplier
        Color::srgba(
            base_highlight_color.to_srgba().red,
            base_highlight_color.to_srgba().green,
            base_highlight_color.to_srgba().blue,
            (base_highlight_color.to_srgba().alpha * cursor_line_settings.border_alpha_multiplier).min(1.0),
        )
    });

    // Word highlight color from configuration
    let word_highlight_color = cursor_line_settings.word_highlight_color.unwrap_or(base_highlight_color);

    // Collect existing entities
    let mut border_entities: std::collections::HashMap<(usize, bool), Entity> = std::collections::HashMap::new();
    for (entity, border, _, _, _) in border_query.iter() {
        border_entities.insert((border.cursor_index, border.is_top), entity);
    }

    let mut word_entities: std::collections::HashMap<usize, Entity> = std::collections::HashMap::new();
    for (entity, word_hl, _, _, _) in word_query.iter() {
        word_entities.insert(word_hl.cursor_index, entity);
    }

    // Calculate border width (code area only, not the gutter)
    let code_area_start = settings.ui.layout.code_margin_left;
    let border_width = viewport.width as f32 - code_area_start;
    let border_center_x = -(viewport.width as f32) / 2.0 + code_area_start + border_width / 2.0 + viewport.offset_x;

    // Process each cursor
    for (idx, cursor) in state.cursors.iter().enumerate() {
        let cursor_pos = cursor.position.min(state.rope.len_chars());
        let line_index = state.rope.char_to_line(cursor_pos);

        // Skip if line is hidden due to folding
        if fold_state.is_line_hidden(line_index) {
            continue;
        }

        // Calculate display row
        let display_row = if use_wrapping {
            state.display_map.buffer_to_display(line_index, 0).0
        } else {
            let mut visible_row = line_index;
            for i in 0..line_index {
                if fold_state.is_line_hidden(i) {
                    visible_row = visible_row.saturating_sub(1);
                }
            }
            visible_row
        };

        let y_from_top = settings.ui.layout.margin_top + state.scroll_offset + (display_row as f32 * line_height);

        // === TOP BORDER ===
        if cursor_line_settings.show_border {
            let top_y = (viewport.height as f32) / 2.0 - y_from_top + line_height / 2.0 - border_thickness / 2.0;
            let top_translation = Vec3::new(border_center_x, top_y, -0.4);

            if let Some(&entity) = border_entities.get(&(idx, true)) {
                if let Ok((_, _, mut transform, mut sprite, mut visibility)) = border_query.get_mut(entity) {
                    transform.translation = top_translation;
                    sprite.custom_size = Some(Vec2::new(border_width, border_thickness));
                    sprite.color = border_color;
                    *visibility = Visibility::Visible;
                }
                border_entities.remove(&(idx, true));
            } else {
                commands.spawn((
                    Sprite {
                        color: border_color,
                        custom_size: Some(Vec2::new(border_width, border_thickness)),
                        ..default()
                    },
                    Transform::from_translation(top_translation),
                    Visibility::Visible,
                    CursorLineBorder { cursor_index: idx, is_top: true },
                    Name::new(format!("CursorLineBorder_top_{}", idx)),
                ));
            }

            // === BOTTOM BORDER ===
            let bottom_y = (viewport.height as f32) / 2.0 - y_from_top - line_height / 2.0 + border_thickness / 2.0;
            let bottom_translation = Vec3::new(border_center_x, bottom_y, -0.4);

            if let Some(&entity) = border_entities.get(&(idx, false)) {
                if let Ok((_, _, mut transform, mut sprite, mut visibility)) = border_query.get_mut(entity) {
                    transform.translation = bottom_translation;
                    sprite.custom_size = Some(Vec2::new(border_width, border_thickness));
                    sprite.color = border_color;
                    *visibility = Visibility::Visible;
                }
                border_entities.remove(&(idx, false));
            } else {
                commands.spawn((
                    Sprite {
                        color: border_color,
                        custom_size: Some(Vec2::new(border_width, border_thickness)),
                        ..default()
                    },
                    Transform::from_translation(bottom_translation),
                    Visibility::Visible,
                    CursorLineBorder { cursor_index: idx, is_top: false },
                    Name::new(format!("CursorLineBorder_bottom_{}", idx)),
                ));
            }
        }

        // === WORD HIGHLIGHT ===
        if !cursor_line_settings.highlight_word {
            continue;
        }
        // Find word boundaries at cursor position
        let line_start = state.rope.line_to_char(line_index);
        let col = cursor_pos - line_start;

        // Get the line text
        let line = state.rope.line(line_index);
        let line_chars: Vec<char> = line.chars().collect();

        // Check if cursor is on a word character (also check char before cursor if cursor is at end)
        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';

        let on_word = if col < line_chars.len() && is_word_char(line_chars[col]) {
            true
        } else if col > 0 && col <= line_chars.len() && is_word_char(line_chars[col - 1]) {
            true
        } else {
            false
        };

        // Find word start and end
        let (word_start, word_end) = if on_word {
            // Find a valid starting position
            let start_col = if col < line_chars.len() && is_word_char(line_chars[col]) {
                col
            } else {
                col - 1
            };

            // Scan backwards for word start
            let mut ws = start_col;
            while ws > 0 && is_word_char(line_chars[ws - 1]) {
                ws -= 1;
            }

            // Scan forwards for word end
            let mut we = start_col;
            while we < line_chars.len() && is_word_char(line_chars[we]) {
                we += 1;
            }

            (ws, we)
        } else {
            (col, col)
        };

        // Only show word highlight if we found a word
        if word_end > word_start {
            let word_width = (word_end - word_start) as f32 * char_width;
            let word_x_left = settings.ui.layout.code_margin_left + (word_start as f32 * char_width);

            let word_center_x = -(viewport.width as f32) / 2.0 + word_x_left + word_width / 2.0 + viewport.offset_x - state.horizontal_scroll_offset;
            let word_center_y = (viewport.height as f32) / 2.0 - y_from_top;

            let word_translation = Vec3::new(word_center_x, word_center_y, -0.5);

            if let Some(&entity) = word_entities.get(&idx) {
                if let Ok((_, _, mut transform, mut sprite, mut visibility)) = word_query.get_mut(entity) {
                    transform.translation = word_translation;
                    sprite.custom_size = Some(Vec2::new(word_width, line_height));
                    sprite.color = word_highlight_color;
                    *visibility = Visibility::Visible;
                }
                word_entities.remove(&idx);
            } else {
                commands.spawn((
                    Sprite {
                        color: word_highlight_color,
                        custom_size: Some(Vec2::new(word_width, line_height)),
                        ..default()
                    },
                    Transform::from_translation(word_translation),
                    Visibility::Visible,
                    CursorWordHighlight { cursor_index: idx },
                    Name::new(format!("CursorWordHighlight_{}", idx)),
                ));
            }
        } else {
            // No word under cursor, hide word highlight
            if let Some(&entity) = word_entities.get(&idx) {
                if let Ok((_, _, _, _, mut visibility)) = word_query.get_mut(entity) {
                    *visibility = Visibility::Hidden;
                }
                word_entities.remove(&idx);
            }
        }
    }

    // Despawn excess entities
    for (_, entity) in border_entities {
        commands.entity(entity).despawn();
    }
    for (_, entity) in word_entities {
        commands.entity(entity).despawn();
    }
}

/// Find matching bracket for a given position
fn find_matching_bracket(
    rope: &ropey::Rope,
    pos: usize,
    bracket_pairs: &[(char, char)],
) -> Option<BracketMatch> {
    if pos >= rope.len_chars() {
        return None;
    }

    let char_at_cursor = rope.char(pos);

    // Check if cursor is on a bracket
    // First check opening brackets
    for &(open, close) in bracket_pairs {
        if char_at_cursor == open {
            // Find matching closing bracket
            if let Some(match_pos) = find_closing_bracket(rope, pos, open, close) {
                return Some(BracketMatch {
                    cursor_bracket_pos: pos,
                    matching_bracket_pos: match_pos,
                });
            }
        } else if char_at_cursor == close {
            // Find matching opening bracket
            if let Some(match_pos) = find_opening_bracket(rope, pos, open, close) {
                return Some(BracketMatch {
                    cursor_bracket_pos: pos,
                    matching_bracket_pos: match_pos,
                });
            }
        }
    }

    // Also check character before cursor (common UX pattern)
    if pos > 0 {
        let char_before = rope.char(pos - 1);
        for &(open, close) in bracket_pairs {
            if char_before == open {
                if let Some(match_pos) = find_closing_bracket(rope, pos - 1, open, close) {
                    return Some(BracketMatch {
                        cursor_bracket_pos: pos - 1,
                        matching_bracket_pos: match_pos,
                    });
                }
            } else if char_before == close {
                if let Some(match_pos) = find_opening_bracket(rope, pos - 1, open, close) {
                    return Some(BracketMatch {
                        cursor_bracket_pos: pos - 1,
                        matching_bracket_pos: match_pos,
                    });
                }
            }
        }
    }

    None
}

/// Find matching closing bracket, handling nesting
fn find_closing_bracket(
    rope: &ropey::Rope,
    start_pos: usize,
    open: char,
    close: char,
) -> Option<usize> {
    let mut depth = 1;
    let mut pos = start_pos + 1;
    let len = rope.len_chars();

    while pos < len && depth > 0 {
        let c = rope.char(pos);
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(pos);
            }
        }
        pos += 1;
    }

    None
}

/// Find matching opening bracket, handling nesting
fn find_opening_bracket(
    rope: &ropey::Rope,
    start_pos: usize,
    open: char,
    close: char,
) -> Option<usize> {
    let mut depth = 1;
    let mut pos = start_pos;

    while pos > 0 && depth > 0 {
        pos -= 1;
        let c = rope.char(pos);
        if c == close {
            depth += 1;
        } else if c == open {
            depth -= 1;
            if depth == 0 {
                return Some(pos);
            }
        }
    }

    None
}

/// Update bracket match state based on cursor position
fn update_bracket_match(
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    mut bracket_state: ResMut<BracketMatchState>,
) {
    // Only update when cursor moves or text changes
    if !state.is_changed() {
        return;
    }

    // Check if bracket matching is enabled
    if !settings.brackets.highlight_matching {
        bracket_state.current_match = None;
        return;
    }

    let cursor_pos = state.cursor_pos.min(state.rope.len_chars());
    bracket_state.current_match = find_matching_bracket(
        &state.rope,
        cursor_pos,
        &settings.brackets.pairs,
    );
}

/// Render bracket match highlights
fn update_bracket_highlight(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    bracket_state: Res<BracketMatchState>,
    fold_state: Res<FoldState>,
    mut highlight_query: Query<(Entity, &BracketMatchHighlight, &mut Transform, &mut Sprite, &mut Visibility)>,
) {
    let mut highlights: Vec<_> = highlight_query.iter_mut().collect();

    match &bracket_state.current_match {
        Some(bracket_match) => {
            let char_width = settings.font.char_width;
            let line_height = settings.font.line_height;
            let viewport_width = viewport.width as f32;
            let viewport_height = viewport.height as f32;
            let use_box_style = settings.brackets.use_box_style;
            let border_thickness = settings.brackets.box_border_thickness;

            // Calculate positions for both brackets
            let positions = [
                bracket_match.cursor_bracket_pos,
                bracket_match.matching_bracket_pos,
            ];

            let mut entity_index = 0;

            for (bracket_idx, &bracket_pos) in positions.iter().enumerate() {
                let line_idx = state.rope.char_to_line(bracket_pos);

                // Skip if line is hidden due to folding
                if fold_state.is_line_hidden(line_idx) {
                    continue;
                }

                let line_start = state.rope.line_to_char(line_idx);
                let col_idx = bracket_pos - line_start;

                // Calculate display row accounting for folded lines
                let display_row = fold_state.actual_to_display_line(line_idx);

                let x_offset = settings.ui.layout.code_margin_left + (col_idx as f32 * char_width);
                let y_offset = settings.ui.layout.margin_top + state.scroll_offset + (display_row as f32 * line_height);

                // Calculate base position (center of the bracket character cell)
                let base_x = -viewport_width / 2.0 + x_offset + char_width / 2.0 - state.horizontal_scroll_offset + viewport.offset_x;
                let base_y = viewport_height / 2.0 - y_offset;

                if use_box_style {
                    // Box style: 4 edges per bracket (top, bottom, left, right)
                    let edges = [
                        // (x_offset, y_offset, width, height) relative to base position
                        // Top edge
                        (0.0, line_height / 2.0 - border_thickness / 2.0, char_width, border_thickness),
                        // Bottom edge
                        (0.0, -line_height / 2.0 + border_thickness / 2.0, char_width, border_thickness),
                        // Left edge
                        (-char_width / 2.0 + border_thickness / 2.0, 0.0, border_thickness, line_height),
                        // Right edge
                        (char_width / 2.0 - border_thickness / 2.0, 0.0, border_thickness, line_height),
                    ];

                    for (edge_idx, (dx, dy, w, h)) in edges.iter().enumerate() {
                        let translation = Vec3::new(base_x + dx, base_y + dy, 0.4);
                        let size = Vec2::new(*w, *h);

                        if entity_index < highlights.len() {
                            // Reuse existing entity
                            let (_, _, ref mut transform, ref mut sprite, ref mut visibility) = &mut highlights[entity_index];
                            transform.translation = translation;
                            sprite.custom_size = Some(size);
                            sprite.color = settings.theme.bracket_match;
                            **visibility = Visibility::Visible;
                        } else {
                            // Spawn new edge entity
                            commands.spawn((
                                Sprite {
                                    color: settings.theme.bracket_match,
                                    custom_size: Some(size),
                                    ..default()
                                },
                                Transform::from_translation(translation),
                                BracketMatchHighlight {
                                    bracket_index: bracket_idx,
                                    edge: edge_idx,
                                },
                                Name::new(format!("BracketHighlight_{}_{}", bracket_idx, edge_idx)),
                                Visibility::Visible,
                            ));
                        }
                        entity_index += 1;
                    }
                } else {
                    // Filled style: single sprite per bracket
                    let translation = Vec3::new(base_x, base_y, 0.4);
                    let size = Vec2::new(char_width, line_height);

                    if entity_index < highlights.len() {
                        // Reuse existing entity
                        let (_, _, ref mut transform, ref mut sprite, ref mut visibility) = &mut highlights[entity_index];
                        transform.translation = translation;
                        sprite.custom_size = Some(size);
                        sprite.color = settings.theme.bracket_match;
                        **visibility = Visibility::Visible;
                    } else {
                        // Spawn new highlight entity
                        commands.spawn((
                            Sprite {
                                color: settings.theme.bracket_match,
                                custom_size: Some(size),
                                ..default()
                            },
                            Transform::from_translation(translation),
                            BracketMatchHighlight {
                                bracket_index: bracket_idx,
                                edge: 0,
                            },
                            Name::new(format!("BracketHighlight_{}", bracket_idx)),
                            Visibility::Visible,
                        ));
                    }
                    entity_index += 1;
                }
            }

            // Hide any extra highlight entities
            for i in entity_index..highlights.len() {
                let (_, _, _, _, ref mut visibility) = &mut highlights[i];
                **visibility = Visibility::Hidden;
            }
        }
        None => {
            // Hide all bracket highlights
            for (_, _, _, _, mut visibility) in highlight_query.iter_mut() {
                *visibility = Visibility::Hidden;
            }
        }
    }
}

/// Render find/search match highlights
fn update_find_highlights(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    find_state: Res<FindState>,
    fold_state: Res<FoldState>,
    mut highlight_query: Query<(Entity, &FindHighlight, &mut Transform, &mut Sprite, &mut Visibility)>,
) {
    // If find is not active or no matches, hide all highlights
    if !find_state.active || find_state.matches.is_empty() {
        for (_, _, _, _, mut visibility) in highlight_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;
    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    // Calculate visible line range for culling (in display coordinates)
    let visible_start_row = ((-state.scroll_offset) / line_height).floor() as usize;
    let visible_lines = ((viewport_height / line_height).ceil() as usize) + 2;
    let visible_end_row = visible_start_row + visible_lines;

    // Collect existing highlight entities by match_index
    let mut existing_highlights: std::collections::HashMap<usize, Entity> = std::collections::HashMap::new();
    for (entity, highlight, _, _, _) in highlight_query.iter() {
        existing_highlights.insert(highlight.match_index, entity);
    }

    // Track which highlights we've updated
    let mut used_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Update or create highlights for visible matches
    for (match_idx, find_match) in find_state.matches.iter().enumerate() {
        // Check if this match is visible
        let start_line = state.rope.char_to_line(find_match.start.min(state.rope.len_chars()));

        // Skip if line is hidden due to folding
        if fold_state.is_line_hidden(start_line) {
            continue;
        }

        // Calculate display row accounting for folded lines
        let display_row = fold_state.actual_to_display_line(start_line);

        // Skip if completely outside visible range (in display coordinates)
        if display_row < visible_start_row.saturating_sub(1) || display_row > visible_end_row {
            continue;
        }

        // Determine color based on whether this is the current match
        let is_current = find_state.current_match_index == Some(match_idx);
        let color = if is_current {
            settings.theme.find_match_current
        } else {
            settings.theme.find_match
        };

        // For simplicity, we'll highlight the entire match as a single rectangle on the first line
        // A more complete implementation would handle multi-line matches
        let line_start_char = state.rope.line_to_char(start_line);
        let start_col = find_match.start - line_start_char;
        let match_len = find_match.end - find_match.start;

        let x_offset = settings.ui.layout.code_margin_left + (start_col as f32 * char_width);
        let y_offset = settings.ui.layout.margin_top + state.scroll_offset + (display_row as f32 * line_height);

        // Calculate sprite position and size
        let sprite_width = match_len as f32 * char_width;
        let sprite_x = -viewport_width / 2.0 + x_offset + sprite_width / 2.0 - state.horizontal_scroll_offset + viewport.offset_x;
        let sprite_y = viewport_height / 2.0 - y_offset;
        let translation = Vec3::new(sprite_x, sprite_y, 0.3); // z=0.3 behind bracket highlights

        used_indices.insert(match_idx);

        if let Some(entity) = existing_highlights.get(&match_idx) {
            // Update existing highlight
            if let Ok((_, _, mut transform, mut sprite, mut visibility)) = highlight_query.get_mut(*entity) {
                transform.translation = translation;
                sprite.color = color;
                sprite.custom_size = Some(Vec2::new(sprite_width, line_height));
                *visibility = Visibility::Visible;
            }
        } else {
            // Spawn new highlight entity
            commands.spawn((
                Sprite {
                    color,
                    custom_size: Some(Vec2::new(sprite_width, line_height)),
                    ..default()
                },
                Transform::from_translation(translation),
                FindHighlight { match_index: match_idx },
                Name::new(format!("FindHighlight_{}", match_idx)),
                Visibility::Visible,
            ));
        }
    }

    // Hide unused highlights
    for (entity, highlight, _, _, mut visibility) in highlight_query.iter_mut() {
        if !used_indices.contains(&highlight.match_index) {
            *visibility = Visibility::Hidden;
        }
        // Also clean up highlights that are for indices beyond the current match count
        if highlight.match_index >= find_state.matches.len() {
            commands.entity(entity).despawn();
        }
    }
}

/// Detect mouse hover over the minimap area
fn update_minimap_hover(
    windows: Query<&Window>,
    viewport: Res<ViewportDimensions>,
    settings: Res<EditorSettings>,
    mut hover_state: ResMut<MinimapHoverState>,
) {
    if !settings.minimap.enabled {
        hover_state.is_hovered = false;
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };

    let Some(cursor_pos) = window.cursor_position() else {
        hover_state.is_hovered = false;
        return;
    };

    let viewport_width = viewport.width as f32;
    let minimap_width = settings.minimap.width;

    // Check if cursor is over the minimap area
    let is_over_minimap = if settings.minimap.show_on_right {
        cursor_pos.x >= viewport_width - minimap_width
    } else {
        cursor_pos.x <= minimap_width
    };

    hover_state.is_hovered = is_over_minimap;
}

/// Handle mouse clicks and drags on the minimap for click-to-scroll and drag-to-scroll
fn handle_minimap_mouse(
    windows: Query<&Window>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut state: ResMut<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    hover_state: Res<MinimapHoverState>,
    mut drag_state: ResMut<MinimapDragState>,
) {
    if !settings.minimap.enabled {
        drag_state.is_dragging = false;
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };

    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    let viewport_height = viewport.height as f32;
    let line_count = state.rope.len_lines();
    let line_height = settings.font.line_height;

    // Minimap scaling (same as in update_minimap)
    let minimap_line_height = 4.0;
    let total_minimap_content_height = line_count as f32 * minimap_line_height;
    let scale = if total_minimap_content_height > viewport_height {
        viewport_height / total_minimap_content_height
    } else {
        1.0
    };
    let scaled_line_height = minimap_line_height * scale;
    let effective_minimap_height = (line_count as f32 * scaled_line_height).min(viewport_height);

    // Content Y offset for centering
    let content_y_offset = if settings.minimap.center_when_short && total_minimap_content_height < viewport_height {
        (viewport_height - total_minimap_content_height) / 2.0
    } else {
        0.0
    };

    // Handle mouse button release
    if mouse_button.just_released(MouseButton::Left) {
        drag_state.is_dragging = false;
    }

    // Handle mouse button press on minimap
    if mouse_button.just_pressed(MouseButton::Left) && hover_state.is_hovered {
        drag_state.is_dragging = true;
    }

    // Handle click or drag on minimap
    if (mouse_button.just_pressed(MouseButton::Left) && hover_state.is_hovered) ||
       (drag_state.is_dragging && mouse_button.pressed(MouseButton::Left)) {
        // Convert cursor Y position to a scroll offset
        // cursor_pos.y is from top of window (0 = top)
        let relative_y = cursor_pos.y - content_y_offset;

        // Calculate which "fraction" of the minimap we clicked
        // relative_y / effective_minimap_height gives us the scroll progress (0 = top, 1 = bottom)
        let click_fraction = (relative_y / effective_minimap_height).clamp(0.0, 1.0);

        // Calculate the max scroll offset
        let content_height = line_count as f32 * line_height;
        let max_scroll = -(content_height - viewport_height + settings.ui.layout.margin_top);

        // Calculate new scroll offset
        // We want to center the clicked line in the viewport
        // So we offset by half the viewport height worth of scroll
        let visible_lines = viewport_height / line_height;
        let visible_fraction = (visible_lines / line_count as f32).min(1.0);
        let center_offset_fraction = visible_fraction / 2.0;

        // Adjust click fraction to account for centering
        let adjusted_fraction = (click_fraction - center_offset_fraction).clamp(0.0, 1.0 - visible_fraction);
        let normalized_fraction = if (1.0 - visible_fraction) > 0.0 {
            adjusted_fraction / (1.0 - visible_fraction)
        } else {
            0.0
        };

        let new_scroll = normalized_fraction * max_scroll;

        // Clamp to valid range
        state.scroll_offset = new_scroll.clamp(max_scroll.min(0.0), 0.0);
        state.needs_scroll_update = true;
    }
}

/// Update minimap rendering using tiny text (like VSCode)
/// Renders actual text at a very small font size to create the minimap effect
fn update_minimap(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    hover_state: Res<MinimapHoverState>,
    mut bg_query: Query<(Entity, &mut Transform, &mut Sprite, &mut Visibility), (With<MinimapBackground>, Without<MinimapSlider>, Without<MinimapLine>, Without<MinimapViewportHighlight>)>,
    mut slider_query: Query<(Entity, &mut Transform, &mut Sprite, &mut Visibility), (With<MinimapSlider>, Without<MinimapBackground>, Without<MinimapLine>, Without<MinimapViewportHighlight>)>,
    mut highlight_query: Query<(Entity, &mut Transform, &mut Sprite, &mut Visibility), (With<MinimapViewportHighlight>, Without<MinimapBackground>, Without<MinimapSlider>, Without<MinimapLine>)>,
    mut line_query: Query<(Entity, &mut Text2d, &mut TextColor, &mut Transform, &mut Visibility, &MinimapLine), (Without<MinimapBackground>, Without<MinimapSlider>, Without<MinimapViewportHighlight>)>,
) {
    // Hide everything if minimap is disabled
    if !settings.minimap.enabled {
        for (_, _, _, mut visibility) in bg_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        for (_, _, _, mut visibility) in slider_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        for (_, _, _, mut visibility) in highlight_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        for (_, _, _, _, mut visibility, _) in line_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;
    let minimap_width = settings.minimap.width;
    let line_count = state.rope.len_lines();
    let line_height = settings.font.line_height;

    // Minimap text settings - tiny font like VSCode
    let minimap_font_size = 3.0;  // Very small text
    let minimap_line_height = 4.0;  // Slightly larger than font for readability

    // Calculate total content height in minimap
    let total_minimap_content_height = line_count as f32 * minimap_line_height;

    // Scale to fit if content is taller than viewport
    let scale = if total_minimap_content_height > viewport_height {
        viewport_height / total_minimap_content_height
    } else {
        1.0
    };

    let scaled_line_height = minimap_line_height * scale;
    let effective_minimap_height = (line_count as f32 * scaled_line_height).min(viewport_height);

    // Vertical offset for centering when content is short
    let content_y_offset = if settings.minimap.center_when_short && total_minimap_content_height < viewport_height {
        (viewport_height - total_minimap_content_height) / 2.0
    } else {
        0.0
    };

    // Minimap X position (right or left side)
    let minimap_left_x = if settings.minimap.show_on_right {
        viewport_width / 2.0 - minimap_width + 2.0
    } else {
        -viewport_width / 2.0 + 2.0
    };

    let minimap_center_x = if settings.minimap.show_on_right {
        viewport_width / 2.0 - minimap_width / 2.0
    } else {
        -viewport_width / 2.0 + minimap_width / 2.0
    };

    // === BACKGROUND ===
    if let Ok((_, mut transform, mut sprite, mut visibility)) = bg_query.single_mut() {
        sprite.custom_size = Some(Vec2::new(minimap_width, viewport_height));
        transform.translation = Vec3::new(minimap_center_x, 0.0, 5.0);
        *visibility = Visibility::Visible;
    } else {
        commands.spawn((
            Sprite {
                color: settings.theme.minimap_background,
                custom_size: Some(Vec2::new(minimap_width, viewport_height)),
                ..default()
            },
            Transform::from_translation(Vec3::new(minimap_center_x, 0.0, 5.0)),
            MinimapBackground,
            Name::new("MinimapBackground"),
            Visibility::Visible,
        ));
    }

    // === Calculate viewport position on minimap (shared between slider and highlight) ===
    let content_height = line_count as f32 * line_height;
    let visible_lines = (viewport_height / line_height).ceil();
    let visible_fraction = (visible_lines / line_count as f32).min(1.0);

    // Scroll progress (0 = top, 1 = bottom) - smooth continuous value
    let max_scroll = -(content_height - viewport_height).max(0.0);
    let scroll_progress = if max_scroll < 0.0 {
        (state.scroll_offset / max_scroll).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // Viewport indicator height proportional to visible content
    let indicator_height = (visible_fraction * effective_minimap_height).max(20.0);
    let scrollable_range = effective_minimap_height - indicator_height;
    let indicator_y_offset = scroll_progress * scrollable_range;

    // Position from top, accounting for centering offset
    let indicator_y = viewport_height / 2.0 - indicator_height / 2.0 - indicator_y_offset - content_y_offset;
    let indicator_translation = Vec3::new(minimap_center_x, indicator_y, 5.3);

    // === VIEWPORT HIGHLIGHT (subtle, always visible) ===
    if settings.minimap.show_viewport_highlight {
        if let Ok((_, mut transform, mut sprite, mut visibility)) = highlight_query.single_mut() {
            sprite.custom_size = Some(Vec2::new(minimap_width, indicator_height));
            transform.translation = indicator_translation;
            *visibility = Visibility::Visible;
        } else {
            commands.spawn((
                Sprite {
                    color: settings.theme.minimap_viewport_highlight,
                    custom_size: Some(Vec2::new(minimap_width, indicator_height)),
                    ..default()
                },
                Transform::from_translation(indicator_translation),
                MinimapViewportHighlight,
                Name::new("MinimapViewportHighlight"),
                Visibility::Visible,
            ));
        }
    } else {
        for (_, _, _, mut visibility) in highlight_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
    }

    // === SLIDER (more visible, appears on hover) ===
    let show_slider = settings.minimap.show_slider &&
        (!settings.minimap.slider_on_hover_only || hover_state.is_hovered);

    if show_slider {
        // Slider is at a higher Z than highlight
        let slider_translation = Vec3::new(minimap_center_x, indicator_y, 5.5);

        if let Ok((_, mut transform, mut sprite, mut visibility)) = slider_query.single_mut() {
            sprite.custom_size = Some(Vec2::new(minimap_width, indicator_height));
            transform.translation = slider_translation;
            *visibility = Visibility::Visible;
        } else {
            commands.spawn((
                Sprite {
                    color: settings.theme.minimap_slider,
                    custom_size: Some(Vec2::new(minimap_width, indicator_height)),
                    ..default()
                },
                Transform::from_translation(slider_translation),
                MinimapSlider,
                Name::new("MinimapSlider"),
                Visibility::Visible,
            ));
        }
    } else {
        for (_, _, _, mut visibility) in slider_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
    }

    // === TEXT LINES ===
    // Only update text content when state changes (but positions update every frame above)
    if !state.is_changed() {
        // Still need to update line positions if centering changed
        // Update existing line positions for smooth scrolling
        for (_entity, _, _, mut transform, _, minimap_line) in line_query.iter_mut() {
            let line_y = viewport_height / 2.0 - (minimap_line.line_index as f32 * scaled_line_height) - scaled_line_height / 2.0 - content_y_offset;
            transform.translation.y = line_y;
        }
        return;
    }

    let max_column = settings.minimap.max_column;
    let scaled_font_size = minimap_font_size * scale;

    // Get colors from the cached lines (which have syntax highlighting applied)
    let lines = &state.lines;

    // Collect existing line entities by line_index
    let mut existing_by_index: std::collections::HashMap<usize, Entity> = std::collections::HashMap::new();
    for (entity, _, _, _, _, minimap_line) in line_query.iter() {
        existing_by_index.insert(minimap_line.line_index, entity);
    }

    let mut used_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for line_idx in 0..line_count {
        let line = state.rope.line(line_idx);
        // Get line text, truncate to max_column, and trim trailing newline
        let line_text: String = line.chars()
            .take(max_column)
            .filter(|c| *c != '\n' && *c != '\r')
            .collect();

        // Skip empty lines
        if line_text.trim().is_empty() {
            continue;
        }

        used_indices.insert(line_idx);

        // Y position from top, with centering offset
        let line_y = viewport_height / 2.0 - (line_idx as f32 * scaled_line_height) - scaled_line_height / 2.0 - content_y_offset;

        // X position (left-aligned)
        let line_x = minimap_left_x;

        let translation = Vec3::new(line_x, line_y, 5.2);

        // Get color from syntax highlighting - use first segment's color
        let line_color = if line_idx < lines.len() && !lines[line_idx].is_empty() {
            let segments = &lines[line_idx];
            let first_colored = segments.iter()
                .find(|s| !s.text.trim().is_empty())
                .map(|s| s.color)
                .unwrap_or(settings.theme.foreground);
            first_colored.with_alpha(0.8)
        } else {
            settings.theme.foreground.with_alpha(0.6)
        };

        if let Some(entity) = existing_by_index.get(&line_idx) {
            // Update existing entity
            if let Ok((_, mut text, mut color, mut transform, mut visibility, _)) = line_query.get_mut(*entity) {
                text.0 = line_text;
                *color = TextColor(line_color);
                transform.translation = translation;
                *visibility = Visibility::Visible;
            }
        } else {
            // Spawn new text entity
            let text_font = TextFont {
                font: settings.font.handle.clone().unwrap_or_default(),
                font_size: scaled_font_size.max(2.0),
                ..default()
            };

            commands.spawn((
                Text2d::new(line_text),
                text_font,
                TextColor(line_color),
                Transform::from_translation(translation).with_scale(Vec3::splat(scale.min(1.0))),
                Anchor::CENTER_LEFT,
                MinimapLine { line_index: line_idx },
                Name::new(format!("MinimapText_{}", line_idx)),
                Visibility::Visible,
            ));
        }
    }

    // Hide unused entities
    for (_entity, _, _, _, mut visibility, minimap_line) in line_query.iter_mut() {
        if !used_indices.contains(&minimap_line.line_index) {
            *visibility = Visibility::Hidden;
        }
    }
}

/// Update minimap to show search match highlights
fn update_minimap_find_highlights(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    find_state: Res<FindState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    mut highlight_query: Query<(Entity, &mut Transform, &mut Sprite, &mut Visibility, &MinimapFindHighlight)>,
) {
    // Hide all if minimap disabled or no active search
    if !settings.minimap.enabled || !find_state.active || find_state.matches.is_empty() {
        for (_, _, _, mut visibility, _) in highlight_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    let viewport_height = viewport.height as f32;
    let viewport_width = viewport.width as f32;
    let minimap_width = settings.minimap.width;
    let line_count = state.rope.len_lines();

    // Minimap scaling (same as in update_minimap)
    let minimap_line_height = 4.0;
    let total_minimap_content_height = line_count as f32 * minimap_line_height;
    let scale = if total_minimap_content_height > viewport_height {
        viewport_height / total_minimap_content_height
    } else {
        1.0
    };
    let scaled_line_height = minimap_line_height * scale;

    // Content Y offset for centering
    let content_y_offset = if settings.minimap.center_when_short && total_minimap_content_height < viewport_height {
        (viewport_height - total_minimap_content_height) / 2.0
    } else {
        0.0
    };

    // Minimap X position
    let minimap_center_x = if settings.minimap.show_on_right {
        viewport_width / 2.0 - minimap_width / 2.0
    } else {
        -viewport_width / 2.0 + minimap_width / 2.0
    };

    // Collect lines with matches (deduplicated)
    let mut match_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for m in &find_state.matches {
        let line = state.rope.char_to_line(m.start);
        match_lines.insert(line);
    }

    // Collect existing highlight entities by line index
    let mut existing_by_line: std::collections::HashMap<usize, Entity> = std::collections::HashMap::new();
    for (entity, _, _, _, highlight) in highlight_query.iter() {
        existing_by_line.insert(highlight.line_index, entity);
    }

    let mut used_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Create/update highlight entities for each line with matches
    for line_idx in &match_lines {
        used_lines.insert(*line_idx);

        // Y position from top, with centering offset
        let line_y = viewport_height / 2.0 - (*line_idx as f32 * scaled_line_height) - scaled_line_height / 2.0 - content_y_offset;
        let translation = Vec3::new(minimap_center_x, line_y, 5.1); // Behind text (5.2)

        if let Some(entity) = existing_by_line.get(line_idx) {
            // Update existing
            if let Ok((_, mut transform, mut sprite, mut visibility, _)) = highlight_query.get_mut(*entity) {
                transform.translation = translation;
                sprite.custom_size = Some(Vec2::new(minimap_width, scaled_line_height.max(2.0)));
                sprite.color = settings.theme.find_match.with_alpha(0.5);
                *visibility = Visibility::Visible;
            }
        } else {
            // Spawn new highlight
            commands.spawn((
                Sprite {
                    color: settings.theme.find_match.with_alpha(0.5),
                    custom_size: Some(Vec2::new(minimap_width, scaled_line_height.max(2.0))),
                    ..default()
                },
                Transform::from_translation(translation),
                MinimapFindHighlight { line_index: *line_idx },
                Name::new(format!("MinimapFindHighlight_{}", line_idx)),
                Visibility::Visible,
            ));
        }
    }

    // Hide unused highlight entities
    for (_, _, _, mut visibility, highlight) in highlight_query.iter_mut() {
        if !used_lines.contains(&highlight.line_index) {
            *visibility = Visibility::Hidden;
        }
    }
}

/// Detect foldable regions using tree-sitter
/// This analyzes the syntax tree to find code blocks that can be folded
#[cfg(feature = "tree-sitter")]
fn detect_foldable_regions(
    state: Res<CodeEditorState>,
    mut fold_state: ResMut<FoldState>,
) {
    // Only update when content changes
    if fold_state.content_version == state.content_version as usize {
        return;
    }

    fold_state.content_version = state.content_version as usize;

    // Get the tree-sitter tree from state
    let tree = match &state.cached_tree {
        Some(t) => t,
        None => return,
    };

    let mut regions: Vec<FoldRegion> = Vec::new();
    let root = tree.root_node();
    let text = state.rope.to_string();
    let text_bytes = text.as_bytes();

    // Walk the tree and find foldable nodes
    collect_foldable_regions(&root, text_bytes, &state.rope, &mut regions, false);

    // Preserve fold state for existing regions
    let old_regions = std::mem::take(&mut fold_state.regions);
    for mut region in regions {
        // Check if this region was previously folded
        if let Some(old) = old_regions.iter().find(|r| r.start_line == region.start_line && r.end_line == region.end_line) {
            region.is_folded = old.is_folded;
        }
        fold_state.regions.push(region);
    }

    fold_state.enabled = true;
}

#[cfg(feature = "tree-sitter")]
fn collect_foldable_regions(
    node: &tree_sitter::Node,
    text: &[u8],
    rope: &ropey::Rope,
    regions: &mut Vec<FoldRegion>,
    parent_is_foldable_construct: bool,
) {
    let kind = node.kind();

    // Check if this is a function-like or class-like construct that contains a body
    let is_foldable_construct = matches!(kind,
        // Function-like constructs
        "function_item" | "function_definition" | "function_declaration" |
        "method_definition" | "method_declaration" | "function_expression" |
        "arrow_function" | "lambda" | "closure_expression" |
        // Class-like constructs
        "class_definition" | "class_declaration" | "struct_item" |
        "enum_item" | "interface_declaration" | "trait_item" | "impl_item"
    );

    // Skip block/body nodes that are direct children of foldable constructs
    // to avoid creating duplicate fold regions at the same line
    let skip_this_node = parent_is_foldable_construct && matches!(kind,
        "block" | "compound_statement" | "statement_block" | "body" |
        "field_declaration_list" | "declaration_list" | "enum_variant_list"
    );

    if !skip_this_node {
        // Check if this node is foldable
        if let Some(region) = node_to_fold_region(node, text, rope) {
            // Only add regions that span multiple lines
            if region.end_line > region.start_line {
                regions.push(region);
            }
        }
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_foldable_regions(&child, text, rope, regions, is_foldable_construct);
    }
}

#[cfg(feature = "tree-sitter")]
fn node_to_fold_region(
    node: &tree_sitter::Node,
    _text: &[u8],
    rope: &ropey::Rope,
) -> Option<FoldRegion> {
    let kind = node.kind();

    // Map tree-sitter node kinds to FoldKind
    // These mappings work for most languages (Rust, JavaScript, TypeScript, Python, etc.)
    let fold_kind = match kind {
        // Function-like constructs
        "function_item" | "function_definition" | "function_declaration" |
        "method_definition" | "method_declaration" | "function_expression" |
        "arrow_function" | "lambda" | "closure_expression" => Some(FoldKind::Function),

        // Class-like constructs
        "class_definition" | "class_declaration" | "struct_item" |
        "enum_item" | "interface_declaration" | "trait_item" |
        "impl_item" => Some(FoldKind::Class),

        // Block constructs
        "block" | "compound_statement" | "statement_block" |
        "if_expression" | "if_statement" | "match_expression" |
        "switch_statement" | "for_statement" | "for_expression" |
        "while_statement" | "while_expression" | "loop_expression" |
        "try_statement" | "catch_clause" | "finally_clause" => Some(FoldKind::Block),

        // Import/use statements (when grouped)
        "use_declaration" | "import_statement" | "import_declaration" => Some(FoldKind::Imports),

        // Comments
        "comment" | "block_comment" | "line_comment" | "doc_comment" => Some(FoldKind::Comment),

        // String literals (multi-line)
        "string_literal" | "raw_string_literal" | "template_string" => Some(FoldKind::Literal),

        // Region markers (e.g., #region in C#)
        "region" | "preproc_region" => Some(FoldKind::Region),

        // Array/object literals (when multi-line)
        "array" | "array_expression" | "object" | "object_expression" |
        "struct_expression" | "tuple_expression" => Some(FoldKind::Other),

        _ => None,
    };

    fold_kind.map(|kind| {
        let start_line = node.start_position().row;
        let end_line = node.end_position().row;

        // Calculate indent level from the start of the line
        let _line_start = rope.line_to_char(start_line);
        let line = rope.line(start_line);
        let mut indent_level = 0;
        for c in line.chars() {
            match c {
                ' ' => indent_level += 1,
                '\t' => indent_level += 4,
                _ => break,
            }
        }
        indent_level /= 4; // Convert to indent levels

        FoldRegion {
            start_line,
            end_line,
            is_folded: false,
            kind,
            indent_level,
        }
    })
}

/// Fallback for when tree-sitter is not enabled
#[cfg(not(feature = "tree-sitter"))]
fn detect_foldable_regions(
    state: Res<CodeEditorState>,
    mut fold_state: ResMut<FoldState>,
) {
    // Only update when content changes
    if fold_state.content_version == state.content_version as usize {
        return;
    }

    fold_state.content_version = state.content_version as usize;

    // Simple brace-matching based folding as fallback
    let mut regions: Vec<FoldRegion> = Vec::new();
    let mut brace_stack: Vec<(usize, usize)> = Vec::new(); // (line, indent_level)

    for line_idx in 0..state.rope.len_lines() {
        let line = state.rope.line(line_idx);
        let line_str: String = line.chars().collect();

        // Calculate indent level
        let mut indent_level = 0;
        for c in line_str.chars() {
            match c {
                ' ' => indent_level += 1,
                '\t' => indent_level += 4,
                _ => break,
            }
        }
        indent_level /= 4;

        // Look for opening braces at end of line
        let trimmed = line_str.trim_end();
        if trimmed.ends_with('{') || trimmed.ends_with('[') || trimmed.ends_with('(') {
            brace_stack.push((line_idx, indent_level));
        }

        // Look for closing braces at start of line (after whitespace)
        let trimmed_start = line_str.trim_start();
        if trimmed_start.starts_with('}') || trimmed_start.starts_with(']') || trimmed_start.starts_with(')') {
            if let Some((start_line, start_indent)) = brace_stack.pop() {
                if line_idx > start_line {
                    regions.push(FoldRegion {
                        start_line,
                        end_line: line_idx,
                        is_folded: false,
                        kind: FoldKind::Block,
                        indent_level: start_indent,
                    });
                }
            }
        }
    }

    // Preserve fold state for existing regions
    let old_regions = std::mem::take(&mut fold_state.regions);
    for mut region in regions {
        if let Some(old) = old_regions.iter().find(|r| r.start_line == region.start_line && r.end_line == region.end_line) {
            region.is_folded = old.is_folded;
        }
        fold_state.regions.push(region);
    }

    fold_state.enabled = true;
}

/// Update fold gutter indicators (arrows/chevrons)
fn update_fold_indicators(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    fold_state: Res<FoldState>,
    mut indicator_query: Query<(Entity, &FoldIndicator, &mut Transform, &mut Text2d, &mut Visibility)>,
) {
    // Hide all if folding is disabled
    if !fold_state.enabled || !settings.ui.show_line_numbers {
        for (_, _, _, _, mut visibility) in indicator_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    let line_height = settings.font.line_height;
    let font_size = settings.font.size;
    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    // Calculate visible line range
    let visible_start_line = ((-state.scroll_offset) / line_height).floor() as usize;
    let visible_lines = ((viewport_height / line_height).ceil() as usize) + 2;
    let visible_end_line = (visible_start_line + visible_lines).min(state.rope.len_lines());

    // Collect fold regions that start within visible range
    let visible_regions: Vec<_> = fold_state.regions.iter()
        .filter(|r| r.start_line >= visible_start_line && r.start_line < visible_end_line)
        .collect();

    // Collect existing indicators
    let mut existing_indicators: std::collections::HashMap<usize, Entity> = std::collections::HashMap::new();
    for (entity, indicator, _, _, _) in indicator_query.iter() {
        existing_indicators.insert(indicator.line_index, entity);
    }

    let mut used_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Calculate hidden lines for proper display positioning
    // We need to count how many lines are hidden before each fold region
    let count_hidden_lines_before = |line: usize| -> usize {
        fold_state.regions.iter()
            .filter(|r| r.is_folded && r.start_line < line)
            .map(|r| r.end_line.saturating_sub(r.start_line))
            .sum()
    };

    for region in visible_regions {
        let line_idx = region.start_line;

        // Skip if this region's start line is hidden by another fold
        if fold_state.is_line_hidden(line_idx) {
            continue;
        }

        used_indices.insert(line_idx);

        // Calculate display line by subtracting hidden lines above
        let hidden_above = count_hidden_lines_before(line_idx);
        let display_line = line_idx.saturating_sub(hidden_above);

        // Position in fold gutter (between line numbers and separator)
        // In VSCode style, this is a narrow gutter just before the separator
        let x_offset = settings.ui.layout.separator_x - 12.0; // Just before the separator
        let y_offset = settings.ui.layout.margin_top + state.scroll_offset + (display_line as f32 * line_height);

        let translation = to_bevy_coords_left_aligned(
            x_offset,
            y_offset,
            viewport_width,
            viewport_height,
            viewport.offset_x,
            0.0,
        );

        // Choose indicator character based on fold state
        let indicator_char = if region.is_folded { "▶" } else { "▼" };

        if let Some(entity) = existing_indicators.get(&line_idx) {
            // Update existing indicator
            if let Ok((_, _, mut transform, mut text, mut visibility)) = indicator_query.get_mut(*entity) {
                transform.translation = translation;
                text.0 = indicator_char.to_string();
                *visibility = Visibility::Visible;
            }
        } else {
            // Spawn new indicator
            let text_font = TextFont {
                font: settings.font.handle.clone().unwrap_or_default(),
                font_size: font_size * 0.7,
                ..default()
            };

            commands.spawn((
                Text2d::new(indicator_char.to_string()),
                text_font,
                TextColor(settings.theme.line_numbers.with_alpha(0.8)),
                Transform::from_translation(translation),
                Anchor::CENTER_LEFT,
                FoldIndicator { line_index: line_idx },
                Name::new(format!("FoldIndicator_{}", line_idx)),
                Visibility::Visible,
            ));
        }
    }

    // Hide unused indicators
    for (_entity, indicator, _, _, mut visibility) in indicator_query.iter_mut() {
        if !used_indices.contains(&indicator.line_index) {
            *visibility = Visibility::Hidden;
        }
    }
}

/// Marker component for GPU text mesh entities
#[derive(Component)]
/// Marker component for GPU text mesh entities with scroll tracking
#[derive(Component)]
pub struct GpuTextMesh {
    /// The scroll offset when this mesh was built
    pub built_at_scroll: f32,
    pub built_at_horizontal_scroll: f32,
    /// The visible line range when built
    pub first_line: usize,
    pub last_line: usize,
}

/// GPU-accelerated text display system
/// Uses a glyph atlas and batched mesh rendering instead of Bevy's Text2d
fn update_gpu_text_display(
    mut commands: Commands,
    mut state: ResMut<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    fold_state: Res<FoldState>,
    mut atlas: ResMut<GlyphAtlas>,
    render_state: Res<TextRenderState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mesh_query: Query<(Entity, &GpuTextMesh, &bevy::mesh::Mesh2d)>,
) {
    use bevy::mesh::{Mesh2d, Indices, PrimitiveTopology};
    use bevy::asset::RenderAssetUsages;
    use crate::gpu_text::{GlyphKey, GlyphRasterizer};

    if !state.needs_update {
        return;
    }

    // Update syntax highlighting tokens
    state.update_highlighting();
    update_lines_cache(&mut state, &settings);

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
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut vertex_count: u32 = 0;

    let mut current_display_row: usize = 0;

    for buffer_line in 0..total_buffer_lines {
        if fold_state.is_line_hidden(buffer_line) {
            continue;
        }

        if current_display_row > last_visible_display_row {
            break;
        }

        if current_display_row >= first_visible_display_row {
            // Calculate base Y position
            let base_y = settings.ui.layout.margin_top + state.scroll_offset + (current_display_row as f32 * line_height);

            // Get text segments for this line
            let visible_segments: Vec<(String, Color)> = if state.has_syntax_highlighting && buffer_line < state.lines.len() {
                state.lines[buffer_line].iter().map(|seg| (seg.text.clone(), seg.color)).collect()
            } else if buffer_line < state.rope.len_lines() {
                let rope_line = state.rope.line(buffer_line);
                let line_len = rope_line.len_chars();
                let text_len = if line_len > 0 && rope_line.char(line_len - 1) == '\n' {
                    line_len - 1
                } else {
                    line_len
                };
                if text_len > 0 {
                    let text: String = rope_line.chars().take(text_len).collect();
                    vec![(text, settings.theme.foreground)]
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            // Build glyph quads for this line
            let mut x = settings.ui.layout.code_margin_left - state.horizontal_scroll_offset;

            for (text, color) in &visible_segments {
                let color_rgba = color.to_linear();
                let color_arr = [color_rgba.red, color_rgba.green, color_rgba.blue, color_rgba.alpha];

                for ch in text.chars() {
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
        }

        current_display_row += 1;
    }

    // Create or update the mesh
    let Some(material_handle) = &render_state.material_handle else {
        state.needs_update = false;
        return;
    };

    if positions.is_empty() {
        // No visible text, hide existing mesh
        for (entity, _, _) in mesh_query.iter() {
            commands.entity(entity).insert(Visibility::Hidden);
        }
        state.needs_update = false;
        return;
    }

    // Debug: log vertex count and first position
    if vertex_count > 0 && !positions.is_empty() {
        println!("GPU Text: {} vertices, first pos: {:?}, first color: {:?}",
                 vertex_count, positions[0], colors[0]);
    }

    // Build the mesh
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));

    // Update existing mesh or create new one
    if let Some((entity, _, mesh2d)) = mesh_query.iter().next() {
        // Update existing mesh
        if let Some(existing_mesh) = meshes.get_mut(&mesh2d.0) {
            *existing_mesh = mesh;
        }
        commands.entity(entity).insert(Visibility::Visible);
    } else {
        // Create new mesh entity
        let mesh_handle = meshes.add(mesh);
        commands.spawn((
            Mesh2d(mesh_handle),
            crate::gpu_text::MeshMaterial2d(material_handle.clone()),
            Transform::default(),
            GpuTextMesh,
            Name::new("GpuTextMesh"),
            Visibility::Visible,
        ));
    }

    state.needs_update = false;
}
