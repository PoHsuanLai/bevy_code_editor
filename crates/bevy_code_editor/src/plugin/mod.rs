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
pub mod scrollbar;
pub mod syntax_highlighting;
pub mod ui_elements;

#[cfg(feature = "lsp")]
pub use self::lsp_plugin::LspPlugin;

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
pub use self::syntax_highlighting::{EditorSyntaxState, SyntaxPlugin};

// Re-export helper functions and systems for internal plugin use (crate-visible only)
pub(crate) use self::brackets::{update_bracket_highlight, update_bracket_match};
pub(crate) use self::cursor::update_cursor_line_highlight;
pub(crate) use self::folding::update_fold_indicators;
pub(crate) use self::gpu_line_numbers::update_gpu_line_numbers;
pub(crate) use self::ui_elements::{update_indent_guides, update_selection_highlight};

/// Marker component for the entity that handles editor input (InputManager).
///
/// The `#[require]` cascade attaches `KeyRepeatState` so the dispatcher
/// can track held actions per input source — multiple input managers
/// (different keymap presets, replay, scripted input) get independent
/// repeat state without a global Resource.
#[derive(Component)]
#[require(KeyRepeatState)]
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

/// Marker set for `dispatch_action_events`. Per-action handler systems
/// declare `.after(ActionDispatchSet)` so the fan-out of typed events
/// happens before any handler reads them.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActionDispatchSet;

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
/// Internally adds `bevy_text_editor::TextEditorPlugin` (with the typed-
/// character observer disabled — the editor has its own bracket-aware
/// observer) so the editable-text core (cursor / selection / edit history /
/// undo / clipboard handlers) is wired up automatically.
///
/// **Dependencies (host responsibility):** this plugin does **not** add the
/// engine GPU pipeline or the `TextView` rendering systems. Hosts must add
/// [`bevy_text_engine::TextEnginePlugins`] separately, e.g.
///
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use bevy_text_engine::TextEnginePlugins;
/// # use bevy_code_editor::prelude::*;
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins((TextEnginePlugins, CodeEditorPlugin))
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
    /// [`CodeEditorPlugin`] (editor systems — internally adds the
    /// `bevy_text_editor` plugin which brings pointer interaction +
    /// editable-text core), and [`EditorUiPlugin`] (line numbers,
    /// separator, camera).
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
/// Bundles the engine, editor (which transitively brings the
/// `bevy_text_editor` interaction + editable-text plugins), and default UI
/// plugins into a single group. Mirror of `bevy::DefaultPlugins`: hosts
/// that want fine-grained control can `.disable::<X>()` individual plugins.
pub struct CodeEditorStandalone;

impl PluginGroup for CodeEditorStandalone {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add_group(TextEnginePlugins)
            .add(CodeEditorPlugin)
            .add(EditorUiPlugin::default())
    }
}

impl Plugin for CodeEditorPlugin {
    fn build(&self, app: &mut App) {
        use crate::settings::*;

        // Initialize each editor-side settings resource with its Default.
        // Per-entity values (font, theme, syntax theme, scroll behaviour)
        // live as `FontConfig` / `ThemeConfig` / `SyntaxTheme` /
        // `ScrollConfig` components on the editor entity (cascaded via
        // `#[require]`); what remains here is genuinely-global config like
        // UI toggles, indentation rules, etc.
        app.init_resource::<UiSettings>();
        app.init_resource::<IndentationSettings>();
        app.init_resource::<BracketSettings>();
        app.init_resource::<CursorSettings>();
        app.init_resource::<CursorLineSettings>();
        app.init_resource::<PerformanceSettings>();
        app.init_resource::<WrappingSettings>();
        app.init_resource::<ScrollbarSettings>();
        #[cfg(feature = "lsp")]
        app.init_resource::<LspSettings>();

        // Initialize core resources
        app.init_resource::<ViewportConfig>();
        app.init_resource::<ViewportDimensions>();

        // BracketMatchState, GotoLineState, FoldState, and KeyRepeatState are
        // per-editor / per-input-manager components (cascaded via #[require]);
        // no global resource init. Mouse drag tracking lives on
        // `bevy_text_editor::TextViewDragState`, written by the picking-driven
        // press/drag observers in `bevy_text_editor::interaction`.

        // Configure system set ordering
        app.configure_sets(
            Update,
            (
                InputSet,
                bevy_text_editor::EditEmitSet.after(InputSet),
                ApplyStateSet.after(bevy_text_editor::EditEmitSet),
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

        // Editable-text core: registers the 33 editing events, basic editing
        // handlers (cursor / selection / delete / clipboard / undo), and the
        // pointer-interaction plugin. We pass `without_typing_observer()`
        // because the editor has its own bracket / LSP-aware typed-character
        // observer (`crate::input::on_focused_keyboard`).
        app.add_plugins(bevy_text_editor::TextEditorPlugin::without_typing_observer());

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

        // Register editor-side events. The 33 editing events are registered
        // by `TextEditorPlugin` already; the IDE-only events (replace,
        // goto-line, multi-cursor, folding, LSP request, save / open) are
        // registered here.
        app.add_message::<SaveRequested>();
        app.add_message::<OpenRequested>();
        register_ide_action_events(app);

        #[cfg(feature = "lsp")]
        app.init_resource::<crate::input::handlers::lsp_followup::PendingActionFollowup>();

        // Per-event keyboard input goes through a FocusedInput observer
        // (bracket auto-close + LSP completion triggers). Shortcut input
        // (ActionState) is driven by `dispatch_action_events` which fans out
        // into typed events; per-action handler systems consume those events
        // and apply edits.
        app.add_observer(crate::input::on_focused_keyboard);
        app.add_systems(
            Update,
            crate::input::dispatch_action_events
                .in_set(InputSet)
                .in_set(ActionDispatchSet),
        );
        // Editor-specific mouse interactions are observer-driven via
        // `bevy_picking`. Plain-click cursor placement, drag-extend
        // selection, and scroll wheel are handled by
        // `bevy_text_editor::interaction`'s observers (registered by
        // `TextInteractionPlugin`); the editor adds modifier-click
        // behaviors and the LSP hover trigger on top.
        app.add_observer(crate::input::on_fold_gutter_press);
        app.add_observer(crate::input::on_alt_click);
        #[cfg(feature = "lsp")]
        {
            app.add_observer(crate::input::on_ctrl_click_goto_definition);
            app.add_observer(crate::input::on_pointer_move_for_hover);
            app.add_observer(crate::input::on_pointer_out_for_hover);
            app.add_systems(
                Update,
                crate::input::tick_lsp_hover_timer
                    .in_set(InputSet)
                    .after(ActionDispatchSet),
            );
        }

        // Per-action IDE handlers — read each `*Requested` event and apply.
        // All handlers run in `InputSet` after the dispatcher.
        register_handler_systems(app);

        // `bevy_text_editor` fires `OnEdit` triggers per editor entity after
        // every edit op. The editor crate observes those triggers to keep
        // its per-entity caches (`SyntaxCacheState` for incremental tree-
        // sitter reparse, `EditorDisplayState` for line-entity invalidation)
        // in sync. No system polls the source state; cross-crate
        // propagation flows through the event bus.
        app.add_observer(crate::input::on_edit_invalidate_caches);

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


/// Register the IDE-only `*Requested` events. The 33 editing events are
/// registered by `bevy_text_editor::TextEditorPlugin`.
fn register_ide_action_events(app: &mut App) {
    use crate::input::action_events::*;

    macro_rules! register {
        ($($ty:ty),* $(,)?) => {
            $( app.add_message::<$ty>(); )*
        };
    }

    register!(
        // Search / navigation
        ReplaceRequested,
        GotoLineRequested,
        // LSP
        RequestCompletionRequested,
        GotoDefinitionRequested,
        RenameSymbolRequested,
        // Multi-cursor
        AddCursorAtNextOccurrenceRequested,
        AddCursorAboveRequested,
        AddCursorBelowRequested,
        ClearSecondaryCursorsRequested,
        // Folding
        ToggleFoldRequested,
        FoldRequested,
        UnfoldRequested,
        FoldAllRequested,
        UnfoldAllRequested,
    );
}

/// Register IDE-only per-action handler systems. The basic editing handlers
/// (cursor / selection / delete / clipboard / undo) are registered by
/// `bevy_text_editor::TextEditorPlugin`. Handlers here cover multi-cursor,
/// folding, goto-line, LSP requests, and the LSP follow-up.
fn register_handler_systems(app: &mut App) {
    use crate::input::handlers::*;

    app.add_systems(
        Update,
        (
            // Multi-cursor (4)
            multi_cursor::handle_add_cursor_at_next_occurrence,
            multi_cursor::handle_add_cursor_above,
            multi_cursor::handle_add_cursor_below,
            multi_cursor::handle_clear_secondary_cursors,
            // Folding (5)
            folding::handle_toggle_fold,
            folding::handle_fold,
            folding::handle_unfold,
            folding::handle_fold_all,
            folding::handle_unfold_all,
            // File / dialog (1)
            file::handle_goto_line,
        )
            .in_set(InputSet)
            .after(ActionDispatchSet),
    );

    #[cfg(feature = "lsp")]
    app.add_systems(
        Update,
        (
            lsp::handle_request_completion,
            lsp::handle_goto_definition,
            lsp::handle_rename_symbol,
        )
            .in_set(InputSet)
            .after(ActionDispatchSet),
    );

    // LSP follow-up runs after all handlers — emits did_change and updates
    // the completion popup based on the snapshot taken before dispatch.
    #[cfg(feature = "lsp")]
    app.add_systems(
        Update,
        crate::input::handlers::lsp_followup::lsp_followup
            .in_set(InputSet)
            .after(lsp::handle_request_completion),
    );
}

/// Spawn the default editor entity. `CodeEditor`'s `#[require]` cascade
/// pulls in every supporting component.
pub(crate) fn spawn_editor_entity(mut commands: Commands) {
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
