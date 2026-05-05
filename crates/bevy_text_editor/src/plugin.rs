//! Plugins for `TextView` interaction and editable text editing.
//!
//! [`TextInteractionPlugin`] wires the picking backend and observer-based
//! handlers that turn pointer + focused-keyboard events into scroll,
//! drag-selection, and clipboard copy on any entity carrying
//! [`bevy_text_engine::TextView`]. The rendering side lives in the engine
//! crate ([`bevy_text_engine::TextEnginePlugin`] /
//! [`bevy_text_engine::TextEnginePlugins`]).
//!
//! [`TextEditorPlugin`] adds the editable-text core on top: typed-character
//! insertion, edit history, undo / redo, the typed editing-event registry,
//! and the per-action handler systems. Pulls in `TextInteractionPlugin`.
//!
//! Both plugins idempotently add [`bevy::picking::DefaultPickingPlugins`]
//! and [`bevy::input_focus::InputDispatchPlugin`] if the host hasn't
//! already.

use bevy::input_focus::InputDispatchPlugin;
use bevy::picking::{DefaultPickingPlugins, PickingSystems};
use bevy::prelude::*;

use crate::components::{ScrollConfig, TextViewDragState};
use crate::editing_events::*;
use crate::handlers;
use crate::interaction::{
    on_focused_keyboard, on_pointer_drag, on_pointer_press, on_pointer_release, on_pointer_scroll,
};
use crate::picking::text_view_picking_backend;
use crate::state::{EditHistoryState, IndentConfig, OnEdit, SnapshotPreEdit, TextEditor};
use crate::typing::on_focused_keyboard_typing;

/// Public ordering hook: `emit_edit_triggers` runs in this set. Consumers
/// reading state populated by `OnEdit` observers (e.g. an editor's
/// per-frame edit-event drain) should schedule themselves `.after(EditEmitSet)`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct EditEmitSet;

/// Plugin registering pointer + keyboard interaction for `TextView`
/// entities. Pair with [`bevy_text_engine::TextEnginePlugins`] which
/// supplies the rendering side.
#[derive(Default)]
pub struct TextInteractionPlugin;

impl Plugin for TextInteractionPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<ScrollConfig>()
            .register_type::<TextViewDragState>();

        if !app.is_plugin_added::<bevy::picking::PickingPlugin>() {
            app.add_plugins(DefaultPickingPlugins);
        }
        if !app.is_plugin_added::<InputDispatchPlugin>() {
            app.add_plugins(InputDispatchPlugin);
        }

        app.add_systems(
            PreUpdate,
            text_view_picking_backend.in_set(PickingSystems::Backend),
        );

        app.add_observer(on_pointer_press);
        app.add_observer(on_pointer_drag);
        app.add_observer(on_pointer_release);
        app.add_observer(on_pointer_scroll);
        app.add_observer(on_focused_keyboard);
    }
}

/// Plugin registering the editable-text core: typed-character insertion,
/// edit history, undo / redo, clipboard, and the editing-event handlers.
///
/// Adds [`TextInteractionPlugin`] (idempotent) so spawning a [`TextEditor`]
/// gives you a fully working editable text view: click to focus, type to
/// edit, drag to select, scroll to scroll, Cmd/Ctrl+C/X/V/Z/Y do the
/// expected things.
///
/// Hosts that want a leafwing keymap layer (Ctrl+Right for word-jump, etc.)
/// add their own dispatcher system that emits the appropriate `*Requested`
/// events; the code editor crate (`bevy_code_editor`) does this. Hosts
/// without leafwing can compose simpler input bindings via observers.
///
/// Construct with [`Self::default()`] for the everything-on configuration.
/// Use [`Self::without_typing_observer()`] when the host has its own
/// typed-character handler (e.g. the code editor's bracket / LSP-aware
/// observer); in that case the host inserts characters itself.
#[derive(Clone, Copy, Debug)]
pub struct TextEditorPlugin {
    /// When `true`, the plugin registers a `FocusedInput<KeyboardInput>`
    /// observer that inserts printable characters into the focused
    /// `TextEditor`. Hosts that want bracket auto-close, IME, or LSP
    /// completion triggers handle typing themselves and pass `false`.
    pub typing_observer: bool,
}

impl Default for TextEditorPlugin {
    fn default() -> Self {
        Self {
            typing_observer: true,
        }
    }
}

impl TextEditorPlugin {
    /// Plugin variant that omits the typed-character observer. Pair with a
    /// host-side keyboard handler that calls
    /// [`crate::handlers::edit::insert_char`] (or equivalent) for each
    /// printable char.
    pub const fn without_typing_observer() -> Self {
        Self {
            typing_observer: false,
        }
    }
}

impl Plugin for TextEditorPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TextInteractionPlugin>() {
            app.add_plugins(TextInteractionPlugin);
        }

        app.register_type::<TextEditor>();
        app.register_type::<IndentConfig>();
        app.register_type::<OnEdit>();
        app.register_type::<SnapshotPreEdit>();
        app.add_message::<OnEdit>();

        register_editing_events(app);

        if self.typing_observer {
            app.add_observer(on_focused_keyboard_typing);
        }

        register_handler_systems(app);

        // Drains EditHistoryState's pending fields into per-entity
        // `OnEdit` triggers. Runs in EditEmitSet so consumers can order
        // their downstream systems after it.
        app.configure_sets(Update, EditEmitSet);
        app.add_systems(
            Update,
            (mirror_snapshot_marker, emit_edit_triggers)
                .chain()
                .in_set(EditEmitSet),
        );
    }
}

/// Mirror the [`SnapshotPreEdit`] marker into the `snapshot_pre_edits`
/// flag on each entity's `EditHistoryState` so the pure `replace_range`
/// primitive can decide whether to clone the rope without taking a Bevy
/// query.
fn mirror_snapshot_marker(
    mut q: Query<(&mut EditHistoryState, Has<SnapshotPreEdit>), With<TextEditor>>,
) {
    for (mut hist, has_marker) in q.iter_mut() {
        if hist.snapshot_pre_edits != has_marker {
            hist.snapshot_pre_edits = has_marker;
        }
    }
}

/// Drain `EditHistoryState`'s pending-edit fields into per-entity [`OnEdit`]
/// triggers. Idempotent — once a frame, taking the field clears it.
pub fn emit_edit_triggers(
    mut commands: Commands,
    mut q: Query<(Entity, &mut EditHistoryState), With<TextEditor>>,
) {
    for (entity, mut hist) in q.iter_mut() {
        if hist.pending_byte_edit.is_none()
            && hist.invalidate_lines_from.is_none()
            && hist.pre_edit_rope.is_none()
        {
            continue;
        }
        let byte_edit = hist.pending_byte_edit.take();
        let invalidate_lines_from = hist.invalidate_lines_from.take();
        let pre_edit_rope = hist.pre_edit_rope.take();
        commands.trigger(OnEdit {
            entity,
            byte_edit,
            invalidate_lines_from,
            pre_edit_rope,
        });
    }
}

fn register_editing_events(app: &mut App) {
    macro_rules! register {
        ($($ty:ty),* $(,)?) => {
            $( app.add_message::<$ty>(); )*
        };
    }

    register!(
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
        // Editing
        DeleteBackwardRequested,
        DeleteForwardRequested,
        DeleteWordBackwardRequested,
        DeleteWordForwardRequested,
        DeleteLineRequested,
        InsertNewlineRequested,
        InsertTabRequested,
        // Clipboard
        CopyRequested,
        CutRequested,
        PasteRequested,
        // Undo / redo
        UndoRequested,
        RedoRequested,
        // Programmatic edits (LSP, completion, formatting, refactoring, host setup)
        ReplaceRangeRequested,
        SetTextRequested,
    );
}

fn register_handler_systems(app: &mut App) {
    use handlers::*;

    app.add_systems(
        Update,
        (
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
        ),
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
        ),
    );

    app.add_systems(
        Update,
        (
            edit::handle_insert_newline,
            edit::handle_insert_tab,
            edit::handle_delete_backward,
            edit::handle_delete_forward,
            edit::handle_delete_word_backward,
            edit::handle_delete_word_forward,
            edit::handle_delete_line,
            edit::handle_undo,
            edit::handle_redo,
            edit::handle_replace_range,
            edit::handle_set_text,
            clipboard::handle_copy,
            clipboard::handle_cut,
            clipboard::handle_paste,
        ),
    );
}
