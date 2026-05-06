use crate::types::*;
use bevy::app::{PluginGroup, PluginGroupBuilder};
use bevy::prelude::*;

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

pub use self::brackets::BracketPlugin;
pub use self::cursor::CursorPlugin;
pub use self::editor_ui_plugin::{EditorCamera, EditorUiPlugin};
pub use self::folding::FoldingPlugin;
pub use self::scrollbar::Scrollbar;
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

/// Editor systems plugin — just the editor's own resources, system sets,
/// observers, and IDE-specific event/handler wiring. Does **not** add the
/// engine GPU pipeline, the editable-text core, the input manager, or the
/// editor sub-plugins (cursor / syntax / folding / brackets / scrollbar / UI).
///
/// Most hosts should use [`CodeEditorPlugins`] instead — the full bundle.
/// `CodeEditorPlugin` is for hosts that compose their own version of those
/// dependencies (e.g. a render-to-texture wrapper that owns
/// [`bevy_text_engine::TextEnginePlugins`] and [`EditorUiPlugin`] directly)
/// and need to avoid double-adds.
///
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use bevy_code_editor::prelude::*;
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(CodeEditorPlugins)
///     .run();
/// ```
#[derive(Default)]
pub struct CodeEditorPlugin;

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
        // every edit op. The editor crate observes those triggers to drive
        // incremental tree-sitter reparse and LSP `did_change` via the
        // `TextEditEvent` bus.
        app.add_observer(crate::input::on_edit_invalidate_caches);

        // Auto-scroll-to-cursor sets target_scroll_offset; the actual
        // animation toward target lives in `bevy_text_engine` (smooth) and
        // `bevy_text_editor::apply_instant_scroll` (instant when
        // `ScrollConfig.smooth = false`). Both already run for any TextView,
        // so the editor only needs the cursor-target setter.
        app.add_systems(
            Update,
            ui_elements::auto_scroll_to_cursor
                .run_if(ui_elements::should_auto_scroll)
                .in_set(ApplyStateSet),
        );

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

/// Full editor bundle. Adds the engine GPU pipeline, the editable-text
/// core, the input manager, [`CodeEditorPlugin`], and every editor sub-
/// plugin (cursor / syntax / folding / brackets / scrollbar / UI / display
/// map, plus LSP under the `lsp` feature).
///
/// Disable individual plugins with `.build().disable::<EditorUiPlugin>()`
/// when composing with hosts that own one of the dependencies directly.
pub struct CodeEditorPlugins;

impl PluginGroup for CodeEditorPlugins {
    fn build(self) -> PluginGroupBuilder {
        let group = PluginGroupBuilder::start::<Self>()
            .add(bevy_text_engine::gpu::GlyphAtlasPlugin)
            .add(bevy_text_engine::gpu::InstancedTextRenderPlugin)
            .add(bevy_text_engine::view::plugin::TextEnginePlugin)
            .add(bevy_text_engine::ui::ScrollbarPlugin)
            .add(bevy::input_focus::InputDispatchPlugin)
            .add(bevy_text_editor::TextEditorPlugin::without_typing_observer())
            .add(leafwing_input_manager::plugin::InputManagerPlugin::<
                crate::input::EditorAction,
            >::default())
            .add(CodeEditorPlugin)
            .add(CursorPlugin)
            .add(syntax_highlighting::SyntaxPlugin)
            .add(FoldingPlugin)
            .add(BracketPlugin)
            .add(ScrollbarPlugin)
            .add(EditorUiPlugin)
            .add(crate::display_map::DisplayMapPlugin);
        #[cfg(feature = "lsp")]
        let group = group.add(LspPlugin);
        group
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
        // Navigation
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
