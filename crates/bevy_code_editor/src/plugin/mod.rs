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
pub use self::syntax_highlighting::{EditorSyntaxState, HighlightCache, SyntaxPlugin};

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

        // Register editor events (for file operations + every per-action event).
        // SaveRequested / OpenRequested are public host-facing events; the
        // remaining `*Requested` events are an internal fan-out layer that
        // plugins can hook to intercept user actions.
        app.add_message::<SaveRequested>();
        app.add_message::<OpenRequested>();
        register_action_events(app);

        #[cfg(feature = "lsp")]
        app.init_resource::<crate::input::handlers::lsp_followup::PendingActionFollowup>();

        // Per-event keyboard input goes through a FocusedInput observer.
        // Shortcut input (ActionState) is driven by `dispatch_action_events`
        // which fans out into typed events; per-action handler systems
        // consume those events and apply edits.
        app.add_observer(crate::input::on_focused_keyboard);
        app.add_systems(
            Update,
            crate::input::dispatch_action_events
                .in_set(InputSet)
                .in_set(ActionDispatchSet),
        );
        app.add_systems(
            Update,
            (
                crate::input::handle_mouse_input,
                crate::input::handle_mouse_wheel,
            )
                .chain()
                .in_set(InputSet)
                .after(ActionDispatchSet),
        );

        // Per-action handlers — read each `*Requested` event and apply.
        // All handlers run in `InputSet` after the dispatcher.
        register_handler_systems(app);

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


/// Register every `*Requested` event (one per `EditorAction` variant minus
/// Save/Open which are registered explicitly above as host-facing events).
/// Pulled out of `build` to keep that function focused on the editor's
/// system-set wiring instead of a wall of `add_message` calls.
fn register_action_events(app: &mut App) {
    use crate::input::action_events::*;

    macro_rules! register {
        ($($ty:ty),* $(,)?) => {
            $( app.add_message::<$ty>(); )*
        };
    }

    register!(
        // Deletion
        DeleteBackwardRequested,
        DeleteForwardRequested,
        DeleteWordBackwardRequested,
        DeleteWordForwardRequested,
        DeleteLineRequested,
        // Insertion
        InsertNewlineRequested,
        InsertTabRequested,
        // Cursor movement
        MoveCursorLeftRequested,
        MoveCursorRightRequested,
        MoveCursorUpRequested,
        MoveCursorDownRequested,
        MoveCursorWordLeftRequested,
        MoveCursorWordRightRequested,
        MoveCursorLineStartRequested,
        MoveCursorLineEndRequested,
        MoveCursorDocumentStartRequested,
        MoveCursorDocumentEndRequested,
        MoveCursorPageUpRequested,
        MoveCursorPageDownRequested,
        // Selection
        SelectLeftRequested,
        SelectRightRequested,
        SelectUpRequested,
        SelectDownRequested,
        SelectWordLeftRequested,
        SelectWordRightRequested,
        SelectLineStartRequested,
        SelectLineEndRequested,
        SelectAllRequested,
        ClearSelectionRequested,
        // Clipboard
        CopyRequested,
        CutRequested,
        PasteRequested,
        // Undo / redo
        UndoRequested,
        RedoRequested,
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

/// Register every per-action handler system. All run in `InputSet` after
/// `dispatch_action_events`. Split into two `add_systems` calls because
/// Bevy's tuple system-set has a max arity below the total number of
/// handlers (~46).
fn register_handler_systems(app: &mut App) {
    use crate::input::handlers::*;

    app.add_systems(
        Update,
        (
            // Cursor movement (12)
            cursor_move::handle_move_cursor_left,
            cursor_move::handle_move_cursor_right,
            cursor_move::handle_move_cursor_up,
            cursor_move::handle_move_cursor_down,
            cursor_move::handle_move_cursor_word_left,
            cursor_move::handle_move_cursor_word_right,
            cursor_move::handle_move_cursor_line_start,
            cursor_move::handle_move_cursor_line_end,
            cursor_move::handle_move_cursor_document_start,
            cursor_move::handle_move_cursor_document_end,
            cursor_move::handle_move_cursor_page_up,
            cursor_move::handle_move_cursor_page_down,
            // Selection (10) — Bevy's `(...).chain()` arity caps tuples at
            // 16, so this group fits with cursor movement above kept
            // separate.
        )
            .in_set(InputSet)
            .after(ActionDispatchSet),
    );

    app.add_systems(
        Update,
        (
            selection::handle_select_left,
            selection::handle_select_right,
            selection::handle_select_up,
            selection::handle_select_down,
            selection::handle_select_word_left,
            selection::handle_select_word_right,
            selection::handle_select_line_start,
            selection::handle_select_line_end,
            selection::handle_select_all,
            selection::handle_clear_selection,
        )
            .in_set(InputSet)
            .after(ActionDispatchSet),
    );

    app.add_systems(
        Update,
        (
            // Edit (9)
            edit::handle_insert_newline,
            edit::handle_insert_tab,
            edit::handle_delete_backward,
            edit::handle_delete_forward,
            edit::handle_delete_word_backward,
            edit::handle_delete_word_forward,
            edit::handle_delete_line,
            edit::handle_replace,
            edit::handle_undo,
            edit::handle_redo,
            // Clipboard (3)
            clipboard::handle_copy,
            clipboard::handle_cut,
            clipboard::handle_paste,
        )
            .in_set(InputSet)
            .after(ActionDispatchSet),
    );

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
            .after(lsp::handle_request_completion)
            .after(edit::handle_delete_backward)
            .after(edit::handle_delete_forward)
            .after(edit::handle_undo)
            .after(edit::handle_redo)
            .after(clipboard::handle_cut)
            .after(clipboard::handle_paste),
    );
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
