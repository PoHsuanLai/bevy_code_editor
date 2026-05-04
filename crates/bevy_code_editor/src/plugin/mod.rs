use crate::types::*;
use bevy::app::{PluginGroup, PluginGroupBuilder};
use bevy::prelude::*;
use bevy_text_engine::TextEnginePlugins;

pub mod brackets;
pub mod cursor;
pub mod editor_ui_plugin;
pub mod folding;
pub mod gpu_line_numbers;
#[cfg(feature = "lsp")]
pub mod lsp_plugin;
#[cfg(feature = "lsp")]
pub mod lsp_ui_plugin;
pub mod scrollbar;
pub mod syntax_highlighting;
pub mod ui_elements;

#[cfg(feature = "lsp")]
pub use self::lsp_plugin::LspPlugin;
#[cfg(feature = "lsp")]
pub use self::lsp_ui_plugin::LspUiPlugin;

// Re-export plugins publicly
pub use self::brackets::BracketPlugin as BracketPluginType;
pub use self::cursor::CursorPlugin as CursorPluginType;
pub use self::editor_ui_plugin::EditorUiPlugin as EditorUiPluginType;
pub use self::folding::FoldingPlugin as FoldingPluginType;
pub use self::scrollbar::Scrollbar;
pub use self::scrollbar::ScrollbarPlugin as ScrollbarPluginType;
// Fix visibility for lib.rs re-exports
pub use self::brackets::BracketPlugin;
pub use self::cursor::CursorPlugin;
pub use self::editor_ui_plugin::EditorUiPlugin;
pub use self::folding::FoldingPlugin;
pub use self::scrollbar::ScrollbarPlugin;

// Re-export syntax highlighting resources publicly for external use
pub use self::syntax_highlighting::{HighlightCache, SyntaxPlugin, SyntaxResource};

// Re-export helper functions and systems for internal plugin use (crate-visible only)
pub(crate) use self::brackets::{update_bracket_highlight, update_bracket_match};
pub(crate) use self::cursor::update_cursor_line_highlight;
pub(crate) use self::folding::update_fold_indicators;
pub(crate) use self::gpu_line_numbers::update_gpu_line_numbers;
pub(crate) use self::ui_elements::{update_indent_guides, update_selection_highlight};

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

/// Editor systems plugin. Registers editor settings resources, input
/// dispatch, the editor's per-frame update pipeline, and the syntax /
/// folding / cursor / scrollbar / bracket sub-plugins.
///
/// **Dependencies (host responsibility):** this plugin does **not** add the
/// engine GPU pipeline or the `TextView` rendering systems. Hosts must add
/// [`bevy_text_engine::TextEnginePlugins`] and
/// [`crate::text_view::TextInteractionPlugin`] separately, e.g.
///
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use bevy_text_engine::TextEnginePlugins;
/// # use bevy_code_editor::prelude::*;
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins((TextEnginePlugins, TextInteractionPlugin, CodeEditorPlugin))
///     .run();
/// ```
///
/// For a one-line "hello world" with everything pre-wired (engine + interaction
/// + UI + camera), use [`CodeEditorPlugin::standalone`].
#[derive(Default)]
pub struct CodeEditorPlugin;

impl CodeEditorPlugin {
    /// Returns a [`PluginGroup`] that bundles everything for a runnable
    /// editor demo: [`TextEnginePlugins`] (GPU + view systems),
    /// [`crate::text_view::TextInteractionPlugin`] (mouse/keyboard for
    /// text views), [`CodeEditorPlugin`] (editor systems), and
    /// [`EditorUiPlugin`] (line numbers, separator, camera).
    ///
    /// Use this when you just want to drop an editor into an app without
    /// thinking about plugin ordering. For embedded use (one panel inside
    /// a larger UI), prefer adding the constituent plugins yourself.
    pub fn standalone() -> CodeEditorStandalone {
        CodeEditorStandalone
    }
}

/// `PluginGroup` returned by [`CodeEditorPlugin::standalone`].
///
/// Bundles the engine, interaction, editor, and default UI plugins into a
/// single group. Mirror of `bevy::DefaultPlugins`: hosts that want
/// fine-grained control can `.disable::<X>()` individual plugins.
pub struct CodeEditorStandalone;

impl PluginGroup for CodeEditorStandalone {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add_group(TextEnginePlugins)
            .add(crate::text_view::TextInteractionPlugin)
            .add(CodeEditorPlugin)
            .add(EditorUiPlugin::default())
    }
}

impl Plugin for CodeEditorPlugin {
    fn build(&self, app: &mut App) {
        use crate::settings::*;

        // Initialize each editor-side settings resource with its Default.
        // Per-entity values (font size, scroll behaviour) are already
        // FontConfig / ScrollConfig components on the editor entity; what
        // remains here is genuinely-global config like theme, UI toggles,
        // indentation rules, etc.
        app.init_resource::<FontSettings>();
        app.init_resource::<ThemeSettings>();
        app.init_resource::<UiSettings>();
        app.init_resource::<IndentationSettings>();
        app.init_resource::<BracketSettings>();
        app.init_resource::<CursorSettings>();
        app.init_resource::<CursorLineSettings>();
        app.init_resource::<ScrollingSettings>();
        app.init_resource::<SearchSettings>();
        app.init_resource::<SyntaxSettings>();
        app.init_resource::<PerformanceSettings>();
        app.init_resource::<WrappingSettings>();
        app.init_resource::<ScrollbarSettings>();
        #[cfg(feature = "lsp")]
        app.init_resource::<LspSettings>();

        // Initialize core resources
        app.init_resource::<ViewportConfig>();
        app.init_resource::<ViewportDimensions>();
        app.insert_resource(crate::input::MouseDragState::default());
        app.insert_resource(KeyRepeatState::default());

        // BracketMatchState, GotoLineState, and FoldState are now per-editor
        // components (cascaded via #[require] on CodeEditor); no global resource init.

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

        // GPU text rendering plugins live in `bevy_text_engine::TextEnginePlugins`
        // and are the host's responsibility to add — see this plugin's
        // top-level doc-comment. We don't auto-add them: doing so would make
        // the engine plugins effectively part of CodeEditorPlugin's surface,
        // and hosts couldn't disable / replace them via standard PluginGroup
        // tools.

        // Per-entity keyboard focus, idempotent if the host already added it.
        if !app.is_plugin_added::<bevy::input_focus::InputDispatchPlugin>() {
            app.add_plugins(bevy::input_focus::InputDispatchPlugin);
        }

        // Add input manager plugin for action-based input
        app.add_plugins(leafwing_input_manager::plugin::InputManagerPlugin::<
            crate::input::EditorAction,
        >::default());

        // Spawn the editor entity, plus a default EditorInputManager with the
        // standard keybindings. Hosts that want to override the keymap can spawn
        // their own EditorInputManager entity *before* PostStartup; the default
        // spawn is gated on no existing one being present.
        app.add_systems(Startup, spawn_editor_entity);
        app.add_systems(PostStartup, spawn_default_input_manager);

        // Register editor events (for file operations)
        // These events are emitted by keybindings and should be handled by the host application
        app.add_message::<SaveRequested>();
        app.add_message::<OpenRequested>();

        // Per-event keyboard input goes through a FocusedInput observer;
        // action polling stays a system so leafwing's ActionState (which is
        // itself polled, not event-streamed) still drives shortcuts.
        app.add_observer(crate::input::on_focused_keyboard);
        app.add_systems(
            Update,
            (
                crate::input::process_editor_actions,
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
                ui_elements::animate_smooth_scroll,
                ui_elements::auto_scroll_to_cursor.run_if(ui_elements::should_auto_scroll),
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

        // Display-map snapshot — runs between input/state and rendering.
        app.add_plugins(crate::display_map::DisplayMapPlugin);

        // The renderer (`update_text_views`) is registered by `TextEnginePlugin`
        // — see `bevy_text_engine::view::plugin`. It already runs in
        // `TextViewRenderSet` with `.run_if(atlas_ready)`. We just configure
        // the editor-side ordering: rendering must observe this frame's
        // cursor / selection overlays.
        app.configure_sets(
            Update,
            bevy_text_engine::TextViewRenderSet
                .in_set(RenderingSet)
                .after(crate::plugin::cursor::push_cursor_overlays)
                .after(crate::plugin::cursor::update_cursor_line_highlight)
                .after(crate::plugin::ui_elements::update_selection_highlight),
        );
    }
}


/// Spawn the default editor entity. `CodeEditor`'s `#[require]` cascade
/// pulls in every supporting component.
fn spawn_editor_entity(mut commands: Commands) {
    commands.spawn((CodeEditor, Name::new("CodeEditor")));
}

/// Spawn a default `EditorInputManager` with `default_input_map()` if the host
/// app didn't spawn one before `PostStartup`. Hosts that want a custom keymap
/// can spawn their own at `Startup` and this becomes a no-op.
fn spawn_default_input_manager(
    mut commands: Commands,
    existing: Query<(), With<EditorInputManager>>,
) {
    if !existing.is_empty() {
        return;
    }
    commands.spawn((
        EditorInputManager,
        crate::input::default_input_map(),
        Name::new("EditorInputManager"),
    ));
}
