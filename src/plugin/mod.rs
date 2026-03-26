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
pub(crate) use self::cursor::{
    animate_cursor, track_cursor_movement, update_cursor,
    update_cursor_line_highlight,
};
pub(crate) use self::ui_elements::{
    update_indent_guides, update_selection_highlight,
};
#[cfg(feature = "brackets")]
pub(crate) use self::brackets::{update_bracket_highlight, update_bracket_match};
#[cfg(feature = "folding")]
pub(crate) use self::folding::update_fold_indicators;
pub(crate) use self::gpu_line_numbers::update_gpu_line_numbers;
pub(crate) use self::gpu_text_instanced::update_gpu_text_instanced;
pub use self::gpu_text_instanced::LineGlyphCache;

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

        // Initialize core editor state resources
        app.insert_resource(CodeEditorState::default());
        app.insert_resource(ViewportDimensions::default());
        app.insert_resource(ViewportConfig::default());
        app.insert_resource(crate::input::MouseDragState::default());
        app.insert_resource(KeyRepeatState::default());

        // Initialize feature-specific resources
        app.insert_resource(BracketMatchState::default());
        app.insert_resource(GotoLineState::default());
        app.insert_resource(LineGlyphCache::default());

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

        // Add sync systems to keep Resource and entity Component in agreement during migration
        app.add_systems(
            First,
            sync_resource_to_entity,
        );
        app.add_systems(
            Last,
            sync_entity_to_resource,
        );

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
fn force_initial_render(mut state: ResMut<CodeEditorState>) {
    if state.pending_update {
        state.needs_update = true;
        state.pending_update = false;
    }
}

/// Debouncing system: Only promote pending_update to needs_update if enough time has passed
/// This prevents excessive re-renders during rapid typing
fn debounce_updates(mut state: ResMut<CodeEditorState>, time: Res<Time>) {
    if !state.pending_update {
        return;
    }

    let current_time = time.elapsed_secs_f64() * 1000.0;
    let elapsed = current_time - state.last_render_time;

    if elapsed >= DEBOUNCE_INTERVAL_MS {
        state.needs_update = true;
        state.pending_update = false;
        state.last_render_time = current_time;
    }
}

/// Spawn the editor entity with TextViewState + TextViewViewport components
fn spawn_editor_entity(mut commands: Commands) {
    commands.spawn((
        CodeEditor,
        crate::text_view::TextViewState::default(),
        crate::text_view::TextViewViewport::default(),
        Name::new("CodeEditor"),
    ));
}

/// Sync Resource → Entity: copy TextViewState-overlapping fields from the Resource to the entity component.
/// Runs at the start of each frame so the entity reflects any host-app changes to the Resource.
fn sync_resource_to_entity(
    state: Res<CodeEditorState>,
    viewport_res: Res<ViewportDimensions>,
    mut query: Query<
        (&mut crate::text_view::TextViewState, &mut crate::text_view::TextViewViewport),
        With<CodeEditor>,
    >,
) {
    let Ok((mut tv, mut vp)) = query.single_mut() else {
        return;
    };

    // Sync text buffer & scroll
    if state.is_changed() {
        tv.rope = state.rope.clone();
        tv.scroll_offset = state.scroll_offset;
        tv.target_scroll_offset = state.target_scroll_offset;
        tv.horizontal_scroll_offset = state.horizontal_scroll_offset;
        tv.target_horizontal_scroll_offset = state.target_horizontal_scroll_offset;
        tv.needs_update = state.needs_update;
        tv.needs_scroll_update = state.needs_scroll_update;
        tv.pending_update = state.pending_update;
        tv.last_render_time = state.last_render_time;
        tv.content_version = state.content_version;
        tv.dirty_lines = state.dirty_lines.clone();
        tv.previous_line_count = state.previous_line_count;
        tv.max_content_width = state.max_content_width;
        tv.max_content_width_version = state.max_content_width_version;
        tv.max_width_line = state.max_width_line;
        tv.line_width_tracker = state.line_width_tracker.clone();
    }

    // Sync viewport
    if viewport_res.is_changed() {
        vp.width = viewport_res.width;
        vp.height = viewport_res.height;
        vp.offset_x = viewport_res.offset_x;
        vp.offset_y = viewport_res.offset_y;
        vp.text_area_left = viewport_res.text_area_left;
        vp.text_area_top = viewport_res.text_area_top;
        vp.gutter_width = viewport_res.gutter_width;
        vp.separator_x = viewport_res.separator_x;
    }
}

/// Sync Entity → Resource: copy back any changes made by entity-based systems to the Resource.
/// Runs at the end of each frame so Resource-based systems see the latest state.
fn sync_entity_to_resource(
    mut state: ResMut<CodeEditorState>,
    mut viewport_res: ResMut<ViewportDimensions>,
    query: Query<
        (&crate::text_view::TextViewState, &crate::text_view::TextViewViewport),
        With<CodeEditor>,
    >,
) {
    let Ok((tv, vp)) = query.single() else {
        return;
    };

    // Sync back text buffer & scroll
    state.rope = tv.rope.clone();
    state.scroll_offset = tv.scroll_offset;
    state.target_scroll_offset = tv.target_scroll_offset;
    state.horizontal_scroll_offset = tv.horizontal_scroll_offset;
    state.target_horizontal_scroll_offset = tv.target_horizontal_scroll_offset;
    state.needs_update = tv.needs_update;
    state.needs_scroll_update = tv.needs_scroll_update;
    state.pending_update = tv.pending_update;
    state.last_render_time = tv.last_render_time;
    state.content_version = tv.content_version;
    state.dirty_lines = tv.dirty_lines.clone();
    state.previous_line_count = tv.previous_line_count;
    state.max_content_width = tv.max_content_width;
    state.max_content_width_version = tv.max_content_width_version;
    state.max_width_line = tv.max_width_line;
    state.line_width_tracker = tv.line_width_tracker.clone();

    // Sync back viewport
    viewport_res.width = vp.width;
    viewport_res.height = vp.height;
    viewport_res.offset_x = vp.offset_x;
    viewport_res.offset_y = vp.offset_y;
    viewport_res.text_area_left = vp.text_area_left;
    viewport_res.text_area_top = vp.text_area_top;
    viewport_res.gutter_width = vp.gutter_width;
    viewport_res.separator_x = vp.separator_x;
}