use bevy::prelude::*;
use crate::types::*;

pub mod brackets;
pub mod cursor;
pub mod editor_ui_plugin;
pub mod folding;
pub mod gpu_line_numbers;
pub mod gpu_text_instanced;
#[cfg(feature = "lsp")]
pub mod lsp_plugin;
#[cfg(feature = "lsp")]
pub mod lsp_ui_plugin;
#[cfg(feature = "egui-overlays")]
pub mod lsp_egui_ui_plugin;
pub mod scrollbar;
pub mod syntax_highlighting;
pub mod ui_elements;

#[cfg(feature = "lsp")]
pub use self::lsp_plugin::LspPlugin;
#[cfg(feature = "lsp")]
pub use self::lsp_ui_plugin::LspUiPlugin;

// Re-export plugins publicly
pub use self::editor_ui_plugin::EditorUiPlugin as EditorUiPluginType;
pub use self::brackets::BracketPlugin as BracketPluginType;
pub use self::cursor::CursorPlugin as CursorPluginType;
pub use self::folding::FoldingPlugin as FoldingPluginType;
pub use self::scrollbar::ScrollbarPlugin as ScrollbarPluginType;
pub use self::scrollbar::Scrollbar;
// Fix visibility for lib.rs re-exports
pub use self::brackets::BracketPlugin;
pub use self::cursor::CursorPlugin;
pub use self::folding::FoldingPlugin;
pub use self::editor_ui_plugin::EditorUiPlugin;
pub use self::scrollbar::ScrollbarPlugin;

// Re-export syntax highlighting resources publicly for external use
pub use self::syntax_highlighting::{HighlightCache, SyntaxResource, SyntaxPlugin};

// Re-export helper functions and systems for internal plugin use (crate-visible only)
pub(crate) use self::cursor::update_cursor_line_highlight;
pub(crate) use self::ui_elements::{
    update_indent_guides, update_selection_highlight,
};
#[cfg(feature = "brackets")]
pub(crate) use self::brackets::{update_bracket_highlight, update_bracket_match};
#[cfg(feature = "folding")]
pub(crate) use self::folding::update_fold_indicators;
pub(crate) use self::gpu_line_numbers::update_gpu_line_numbers;
pub(crate) use self::gpu_text_instanced::update_gpu_text_instanced;

/// Marker component for the entity that handles editor input (InputManager)
#[derive(Component)]
pub struct EditorInputManager;

// Helper to convert dynamic coordinate based on scroll alignment
// Returns camera-relative coordinates (entities positioned at camera origin)
// The camera moves to follow the viewport, so entities stay fixed in world space
pub fn to_bevy_coords_dynamic(x: f32, y: f32, viewport_w: f32, viewport_h: f32) -> Vec3 {
    let world_x = -viewport_w / 2.0 + x;
    let world_y = viewport_h / 2.0 - y;
    Vec3::new(world_x, world_y, 0.0)
}

pub fn to_bevy_coords_left_aligned(
    x: f32,
    y: f32,
    viewport_w: f32,
    viewport_h: f32,
    scroll_x: f32,
) -> Vec3 {
    let world_x = -viewport_w / 2.0 + x - scroll_x;
    let world_y = viewport_h / 2.0 - y;
    Vec3::new(world_x, world_y, 0.0)
}

// System sets for ordering
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct InputSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApplyStateSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderingSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct EditorSetupSet;

/// Debouncing interval: Only promote pending_update to needs_update if enough time has passed
/// OPTIMIZATION: Reduced to minimize input lag (~60fps)
const DEBOUNCE_INTERVAL_MS: f64 = 16.0;

#[derive(Default)]
pub struct CodeEditorPlugin;

impl Plugin for CodeEditorPlugin {
    fn build(&self, app: &mut App) {
        // Initialize all settings with defaults
        crate::settings::EditorSettingsBuilder::default()
            .build()
            .insert_into(app);

        // Initialize core resources
        app.insert_resource(ViewportConfig::default());
        app.insert_resource(crate::input::MouseDragState::default());
        app.insert_resource(KeyRepeatState::default());

        // Initialize feature-specific resources
        app.insert_resource(BracketMatchState::default());
        app.insert_resource(GotoLineState::default());

        #[cfg(feature = "folding")]
        app.insert_resource(FoldState::default());

        // Configure system set ordering
        app.configure_sets(
            Update,
            (
                InputSet,
                ApplyStateSet.after(InputSet),
                RenderingSet.after(ApplyStateSet),
            )
                .chain(),
        );

        // Add GPU text rendering plugins (must be added before other plugins that depend on it)
        app.add_plugins(crate::gpu_text::GpuTextPlugin);
        app.add_plugins(crate::gpu_text::InstancedTextRenderPlugin);

        // Add input manager plugin for action-based input
        app.add_plugins(leafwing_input_manager::plugin::InputManagerPlugin::<
            crate::input::EditorAction,
        >::default());

        // Spawn the input manager entity and editor entity with default keybindings
        app.add_systems(Startup, (spawn_input_manager, spawn_editor_entity));

        // Force initial render after setup
        app.add_systems(PostStartup, force_initial_render);

        // Register editor events (for file operations)
        // These events are emitted by keybindings and should be handled by the host application
        app.add_message::<SaveRequested>();
        app.add_message::<OpenRequested>();

        // Add input handling systems in InputSet
        app.add_systems(
            Update,
            (
                crate::input::handle_keyboard_input,
                crate::input::handle_mouse_input,
                crate::input::handle_mouse_wheel,
            )
                .chain()
                .in_set(InputSet),
        );

        // Add state update systems in ApplyStateSet (convert targets to actual state)
        app.add_systems(
            Update,
            (
                debounce_updates,
                ui_elements::animate_smooth_scroll,
                ui_elements::auto_scroll_to_cursor
                    .run_if(ui_elements::should_auto_scroll),
            )
                .chain()
                .in_set(ApplyStateSet),
        );

        // Add sub-plugins
        app.add_plugins((
            CursorPlugin,
            syntax_highlighting::SyntaxPlugin,
            FoldingPlugin,
            BracketPlugin,
            ScrollbarPlugin,
        ));

        #[cfg(feature = "lsp")]
        app.add_plugins(LspPlugin);

        // Add main rendering system
        app.add_systems(
            Update,
            update_gpu_text_instanced
                .run_if(crate::gpu_text::atlas_ready)
                .in_set(RenderingSet),
        );
    }
}

/// Spawn the input manager entity with configured keybindings
fn spawn_input_manager(mut commands: Commands) {
    commands.spawn((
        EditorInputManager,
        crate::input::default_input_map(),
        leafwing_input_manager::action_state::ActionState::<crate::input::EditorAction>::default(),
        Name::new("EditorInputManager"),
    ));
}

/// Force initial render by promoting pending_update to needs_update
fn force_initial_render(
    mut editor_query: Query<(&mut CodeEditorState, &mut crate::text_view::TextViewState), With<CodeEditor>>,
) {
    let Ok((mut _state, mut tv)) = editor_query.single_mut() else {
        return;
    };
    if tv.pending_update {
        tv.needs_update = true;
        tv.pending_update = false;
    }
}

/// Debouncing system: Only promote pending_update to needs_update if enough time has passed
/// This prevents excessive re-renders during rapid typing
fn debounce_updates(
    mut editor_query: Query<(&mut CodeEditorState, &mut crate::text_view::TextViewState), With<CodeEditor>>,
    time: Res<Time>,
) {
    let Ok((mut _state, mut tv)) = editor_query.single_mut() else {
        return;
    };
    if !tv.pending_update {
        return;
    }

    let current_time = time.elapsed_secs_f64() * 1000.0;
    let elapsed = current_time - tv.last_render_time;

    if elapsed >= DEBOUNCE_INTERVAL_MS {
        tv.needs_update = true;
        tv.pending_update = false;
        tv.last_render_time = current_time;
    }
}

/// Spawn the editor entity with TextViewState + TextViewViewport components
fn spawn_editor_entity(mut commands: Commands) {
    commands.spawn((
        CodeEditor,
        CodeEditorState::default(),
        SelectionState::default(),
        EditHistoryState::default(),
        SyntaxCacheState::default(),
        EditorDisplayState::default(),
        CursorState::default(),
        crate::text_view::TextViewState::default(),
        crate::text_view::TextViewViewport::default(),
        Name::new("CodeEditor"),
    ));
}

