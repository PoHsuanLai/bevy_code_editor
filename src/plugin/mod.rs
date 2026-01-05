//! Bevy plugin for GPU-accelerated code editor
//!
//! Renders text using custom GPU-accelerated glyph atlas and shaders

mod cursor;
mod editor_ui_plugin;
mod gpu_line_numbers;
mod gpu_text_render;

#[cfg(feature = "instanced-rendering")]
pub(crate) mod gpu_text_instanced;

mod syntax_highlighting;
mod ui_elements;

#[cfg(feature = "brackets")]
mod brackets;

#[cfg(feature = "folding")]
mod folding;

#[cfg(feature = "minimap")]
mod minimap;

#[cfg(feature = "scrollbar")]
mod scrollbar;

#[cfg(feature = "lsp")]
mod lsp_plugin;

#[cfg(feature = "lsp")]
mod lsp_ui_plugin;

pub(crate) use cursor::*;
pub(crate) use gpu_line_numbers::*;
pub(crate) use gpu_text_render::*;
pub(crate) use ui_elements::*;

#[cfg(feature = "brackets")]
pub(crate) use brackets::*;

#[cfg(feature = "folding")]
pub(crate) use folding::*;

#[cfg(feature = "minimap")]
pub(crate) use minimap::*;

// Re-export scrollbar plugin publicly
#[cfg(feature = "scrollbar")]
pub use scrollbar::{mouse_not_over_scrollbar, Scrollbar, ScrollbarPlugin};

// Re-export syntax plugin publicly
pub use syntax_highlighting::{HighlightCache, SyntaxPlugin, SyntaxResource};

// Re-export editor UI plugin and render config publicly
pub use editor_ui_plugin::{EditorCamera, EditorRenderConfig, EditorUiPlugin};

// Re-export LSP plugins publicly (feature-gated)
#[cfg(feature = "lsp")]
pub use lsp_plugin::LspPlugin;

#[cfg(feature = "lsp")]
pub use lsp_ui_plugin::LspUiPlugin;

use crate::gpu_text::GpuTextPlugin;
use crate::input::EditorAction;
use crate::settings::*;
use crate::types::*;
use bevy::prelude::*;
use leafwing_input_manager::prelude::{ActionState, InputManagerPlugin, InputMap};

/// System set for core editor setup (runs in Startup schedule)
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct EditorSetupSet;

/// System set for input handling (mouse, keyboard, scrollbar drag)
/// These systems write to target_scroll_offset and other input state
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct InputSet;

/// System set for applying state changes (smooth scroll, auto-scroll to cursor)
/// These systems read targets and write to actual state (scroll_offset)
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApplyStateSet;

/// System set for rendering/visual updates (text, UI, scrollbar visuals)
/// These systems read state and update visual entities
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderingSet;

/// Code editor plugin with GPU-accelerated text rendering
pub struct CodeEditorPlugin {
    settings: SettingsBundle,
    input_map: InputMap<EditorAction>,
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
            settings: EditorSettingsBuilder::default().build(),
            input_map,
        }
    }

    /// Set custom editor settings using builder
    pub fn with_settings(mut self, settings: SettingsBundle) -> Self {
        self.settings = settings;
        self
    }

    /// Set custom editor settings using builder function
    pub fn with_settings_builder(mut self, builder: EditorSettingsBuilder) -> Self {
        self.settings = builder.build();
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
        // Insert all settings resources
        self.settings.clone().insert_into(app);

        // Insert core resources (needed for all render modes)
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

        // Configure SystemSet ordering: Input → ApplyState → Rendering
        app.configure_sets(Update, (InputSet, ApplyStateSet, RenderingSet).chain());

        // Add input handling systems (needed for all render modes)
        app.add_systems(Update, crate::input::handle_keyboard_input);
        app.add_systems(Update, debounce_updates);

        // Register editor events for file operations
        // These events are emitted by keybindings and should be handled by the host application
        app.add_message::<SaveRequested>();
        app.add_message::<OpenRequested>();

        // Add rendering resources
        app.insert_resource(ClearColor(self.settings.theme.background));
        app.init_resource::<ViewportConfig>();
        app.insert_resource(ViewportDimensions::default());
        app.insert_resource(GotoLineState::default());
        app.insert_resource(FoldState::default());
        app.insert_resource(gpu_text_render::LineMeshPool::default());

        // Feature-gated resources
        #[cfg(feature = "brackets")]
        app.insert_resource(BracketMatchState::default());

        #[cfg(feature = "minimap")]
        {
            app.insert_resource(MinimapHoverState::default());
            app.insert_resource(MinimapDragState::default());
        }

        #[cfg(feature = "folding")]
        app.insert_resource(FoldState::default());

        // Add the GPU text rendering plugins
        app.add_plugins(GpuTextPlugin);

        #[cfg(feature = "instanced-rendering")]
        app.add_plugins(crate::gpu_text::InstancedTextRenderPlugin);

        // Add the scrollbar plugin (feature-gated)
        #[cfg(feature = "scrollbar")]
        app.add_plugins(scrollbar::ScrollbarPlugin);

        // Add the syntax highlighting plugin
        app.add_plugins(SyntaxPlugin);

        app.add_systems(Startup, init_viewport_from_window.in_set(EditorSetupSet));

        // GPU text rendering systems - split into smaller groups to avoid tuple limits
        // Input systems - handle user input and write to target state
        #[cfg(feature = "scrollbar")]
        app.add_systems(
            Update,
            (
                crate::input::handle_mouse_input.run_if(mouse_not_over_scrollbar),
                crate::input::handle_mouse_wheel,
            )
                .chain()
                .in_set(InputSet),
        );

        #[cfg(not(feature = "scrollbar"))]
        app.add_systems(
            Update,
            (
                crate::input::handle_mouse_input,
                crate::input::handle_mouse_wheel,
            )
                .chain()
                .in_set(InputSet),
        );

        // Apply state systems - read targets and apply to actual state
        app.add_systems(
            Update,
            (
                animate_smooth_scroll,
                auto_scroll_to_cursor.run_if(crate::plugin::ui_elements::should_auto_scroll),
                detect_viewport_resize,
                update_separator_on_resize,
            )
                .chain()
                .in_set(ApplyStateSet),
        );
        // Rendering systems - update visuals based on state

        // INSTANCED RENDERING (experimental - single draw call for all glyphs)
        #[cfg(all(feature = "instanced-rendering", feature = "folding"))]
        app.add_systems(
            Update,
            (
                detect_foldable_regions,
                gpu_text_instanced::update_gpu_text_instanced,
            )
                .chain()
                .run_if(crate::gpu_text::atlas_ready)
                .in_set(RenderingSet),
        );

        #[cfg(all(feature = "instanced-rendering", not(feature = "folding")))]
        app.add_systems(
            Update,
            gpu_text_instanced::update_gpu_text_instanced
                .run_if(crate::gpu_text::atlas_ready)
                .in_set(RenderingSet),
        );

        // PER-LINE MESH RENDERING (default - stable approach)
        #[cfg(all(not(feature = "instanced-rendering"), feature = "folding"))]
        app.add_systems(
            Update,
            (
                detect_foldable_regions,
                update_gpu_text_per_line,
            )
                .chain()
                .run_if(crate::gpu_text::atlas_ready)
                .in_set(RenderingSet),
        );

        #[cfg(all(not(feature = "instanced-rendering"), not(feature = "folding")))]
        app.add_systems(
            Update,
            update_gpu_text_per_line
                .run_if(crate::gpu_text::atlas_ready)
                .in_set(RenderingSet),
        );

        // Update syntax tree AFTER rendering (async) to avoid blocking display
        #[cfg(all(feature = "tree-sitter", feature = "instanced-rendering"))]
        app.add_systems(Update, update_syntax_tree.after(gpu_text_instanced::update_gpu_text_instanced));

        #[cfg(all(feature = "tree-sitter", not(feature = "instanced-rendering")))]
        app.add_systems(Update, update_syntax_tree.after(update_gpu_text_per_line));
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
fn to_bevy_coords_dynamic(
    x: f32,
    y: f32,
    viewport_width: f32,
    viewport_height: f32,
    offset_x: f32,
) -> Vec3 {
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
    _horizontal_scroll: f32, // Unused: horizontal scrolling is handled by character culling
) -> Vec3 {
    // Text always starts at the code margin position
    // Horizontal scrolling is handled by substring culling in the rendering code
    let x = -viewport_width / 2.0 + margin_from_left + offset_x;

    Vec3::new(x, viewport_height / 2.0 - y, 0.0)
}

/// Debouncing system: Only promote pending_update to needs_update if enough time has passed
/// OPTIMIZATION: Reduced to minimize input lag, but mesh rebuilds still occur
/// For large files, the bottleneck is GPU mesh rebuild, not tree-sitter parsing
const DEBOUNCE_INTERVAL_MS: f64 = 16.0;

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
fn init_viewport_from_window(mut viewport: ResMut<ViewportDimensions>, windows: Query<&Window>) {
    if let Some(window) = windows.iter().next() {
        viewport.width = window.resolution.width() as u32;
        viewport.height = window.resolution.height() as u32;
    }
}

/// Detect viewport resize and trigger position update
/// Only runs when ViewportConfig::auto_resize_to_window is true
fn detect_viewport_resize(
    config: Res<ViewportConfig>,
    mut viewport: ResMut<ViewportDimensions>,
    windows: Query<&Window>,
    mut state: ResMut<CodeEditorState>,
) {
    // Skip auto-resize if disabled - user controls viewport manually
    if !config.auto_resize_to_window {
        return;
    }

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
    ui: Res<UiSettings>,
    mut separator_query: Query<(&mut Sprite, &mut Transform), With<Separator>>,
) {
    // Only update if separator is enabled and exists
    if !ui.show_separator {
        return;
    }

    if viewport.is_changed() {
        if let Ok((mut sprite, mut transform)) = separator_query.single_mut() {
            let viewport_width = viewport.width as f32;
            let viewport_height = viewport.height as f32;
            sprite.custom_size = Some(Vec2::new(1.0, viewport_height));
            transform.translation = to_bevy_coords_left_aligned(
                viewport.separator_x,
                viewport_height / 2.0,
                viewport_width,
                viewport_height,
                0.0, // Camera viewport handles panel positioning
                0.0, // separator doesn't scroll horizontally
            );
        }
    }
}
