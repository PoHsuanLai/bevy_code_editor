//! Bevy plugin for GPU-accelerated code editor
//!
//! Renders text using custom GPU-accelerated glyph atlas and shaders

mod ui_elements;
mod cursor;
mod brackets;
mod minimap;
mod folding;
mod gpu_text_render;
mod scrollbar;
mod syntax_highlighting;

pub(crate) use ui_elements::*;
pub(crate) use cursor::*;
pub(crate) use brackets::*;
pub(crate) use minimap::*;
pub(crate) use folding::*;
pub(crate) use gpu_text_render::*;

// Re-export scrollbar plugin publicly
pub use scrollbar::{ScrollbarPlugin, Scrollbar};

// Re-export syntax plugin publicly
pub use syntax_highlighting::{SyntaxPlugin, SyntaxResource, HighlightCache};

use bevy::prelude::*;
use leafwing_input_manager::prelude::{InputManagerPlugin, InputMap, ActionState};
use crate::input::EditorAction;
use crate::settings::EditorSettings;
use crate::types::*;
use crate::gpu_text::GpuTextPlugin;

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

/// Code editor plugin with GPU-accelerated text rendering
pub struct CodeEditorPlugin {
    settings: EditorSettings,
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

        // Add rendering resources
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

        // Add the scrollbar plugin
        app.add_plugins(scrollbar::ScrollbarPlugin);

        // Add the syntax highlighting plugin
        app.add_plugins(SyntaxPlugin);

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
                handle_scroll_for_gpu_text,
                update_gpu_text_display,
                update_line_numbers,
                update_fold_indicators,
            )
                .chain()
                .after(update_separator_on_resize),
        );

        // Update syntax tree AFTER rendering (async) to avoid blocking display
        #[cfg(feature = "tree-sitter")]
        app.add_systems(
            Update,
            update_syntax_tree.after(update_gpu_text_display),
        );
        // Enable selection/highlighting systems
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
                .after(update_line_numbers),
        );
        // Enable minimap and cursor systems (split to avoid tuple limit)
        app.add_systems(
            Update,
            update_minimap_hover.after(update_find_highlights),
        );
        app.add_systems(
            Update,
            handle_minimap_mouse.after(update_minimap_hover),
        );
        app.add_systems(
            Update,
            update_minimap.after(handle_minimap_mouse),  // GPU minimap rendering
        );
        app.add_systems(
            Update,
            update_minimap_find_highlights.after(update_minimap),
        );
        app.add_systems(
            Update,
            update_cursor.after(update_minimap_find_highlights),
        );
        app.add_systems(
            Update,
            animate_cursor.after(update_cursor),
        );

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
