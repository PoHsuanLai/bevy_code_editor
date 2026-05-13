//! [`InstancedTextInteractionPlugin`] wires picking + observers for scroll, drag-select,
//! and copy on any `TextView`. [`InstancedTextEditPlugin`] adds the editable-text core
//! (typed-char, edit history, undo/redo, clipboard). Both are idempotent re:
//! `DefaultPickingPlugins` and `InputDispatchPlugin`.

use bevy::input_focus::InputDispatchPlugin;
use bevy::picking::DefaultPickingPlugins;
use bevy::prelude::*;

use crate::components::{ScrollConfig, TextViewDragState};
use crate::editing_events::*;
use crate::handlers;
use crate::interaction::{
    on_focused_keyboard, on_pointer_drag, on_pointer_press, on_pointer_release, on_pointer_scroll,
};
use crate::state::{
    CursorState, EditHistoryState, IndentConfig, OnEdit, SelectionState, SnapshotPreEdit,
    TextEditor,
};

type ChangedCursorQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static CursorState), (With<TextEditor>, Changed<CursorState>)>;
type ChangedSelectionQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static SelectionState), (With<TextEditor>, Changed<SelectionState>)>;
use crate::typing::on_focused_keyboard_typing;

/// Contains `emit_edit_triggers`. Schedule downstream systems `.after(EditEmitSet)`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct EditEmitSet;

/// Pointer + keyboard interaction for `TextView` entities. Pair with
/// [`bevy_instanced_text::InstancedTextPlugins`] for the rendering side.
#[derive(Default)]
pub struct InstancedTextInteractionPlugin;

impl Plugin for InstancedTextInteractionPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<ScrollConfig>()
            .register_type::<TextViewDragState>()
            .register_type::<crate::interaction::InteractionSettings>();

        // Default clipboard backend; embedders override by inserting
        // their own `ClipboardResource` before plugin setup.
        app.init_resource::<crate::clipboard::ClipboardResource>();

        if !app.is_plugin_added::<bevy::picking::PickingPlugin>() {
            app.add_plugins(DefaultPickingPlugins);
        }
        if !app.is_plugin_added::<InputDispatchPlugin>() {
            app.add_plugins(InputDispatchPlugin);
        }

        app.add_observer(on_pointer_press);
        app.add_observer(on_pointer_drag);
        app.add_observer(on_pointer_release);
        app.add_observer(on_pointer_scroll);
        app.add_observer(on_focused_keyboard);

        // Instant snap for entities with `ScrollConfig.smooth = false`. Runs
        // before the engine's smooth-scroll animator, so the animator sees
        // target == offset and is a no-op for those entities.
        app.add_systems(
            Update,
            apply_instant_scroll.before(bevy_instanced_text::view::plugin::TextViewRenderSet),
        );
    }
}

fn apply_instant_scroll(mut q: Query<(&mut bevy_instanced_text::ScrollState, &ScrollConfig)>) {
    for (mut scroll, cfg) in q.iter_mut() {
        if (scroll.smooth_scroll_duration - cfg.smooth_scroll_duration).abs() > f32::EPSILON {
            scroll.smooth_scroll_duration = cfg.smooth_scroll_duration;
        }
        if cfg.smooth {
            continue;
        }
        if (scroll.target_scroll_offset - scroll.scroll_offset).abs() > 0.001 {
            scroll.scroll_offset = scroll.target_scroll_offset;
        }
        if (scroll.target_horizontal_scroll_offset - scroll.horizontal_scroll_offset).abs() > 0.001
        {
            scroll.horizontal_scroll_offset = scroll.target_horizontal_scroll_offset;
        }
    }
}

/// Editable-text core: typed-char insertion, edit history, undo/redo, clipboard.
/// Adds [`InstancedTextInteractionPlugin`] idempotently. Use
/// [`Self::without_typing_observer()`] when the host handles typing itself
/// (bracket auto-close, IME, LSP completion triggers).
#[derive(Clone, Copy, Debug)]
pub struct InstancedTextEditPlugin {
    /// When `true`, registers a `FocusedInput<KeyboardInput>` observer that
    /// inserts printable chars. Set `false` when the host inserts them itself.
    pub typing_observer: bool,
}

impl Default for InstancedTextEditPlugin {
    fn default() -> Self {
        Self {
            typing_observer: true,
        }
    }
}

impl InstancedTextEditPlugin {
    pub const fn without_typing_observer() -> Self {
        Self {
            typing_observer: false,
        }
    }
}

impl Plugin for InstancedTextEditPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<InstancedTextInteractionPlugin>() {
            app.add_plugins(InstancedTextInteractionPlugin);
        }

        // Register the RopeBuffer content type so TextBuffer<RopeBuffer> entities get layouts.
        app.add_plugins(bevy_instanced_text::TextContentPlugin::<crate::rope_content::RopeBuffer>::default());

        app.register_type::<TextEditor>();
        app.register_type::<IndentConfig>();
        app.register_type::<OnEdit>();
        app.register_type::<SnapshotPreEdit>();
        app.register_type::<crate::theme::TextCursorColor>();
        app.register_type::<crate::theme::TextSelectionColor>();
        app.register_type::<crate::cursor_settings::CursorSettings>();
        app.register_type::<crate::cursor_settings::CursorStyle>();
        app.register_type::<crate::editing_events::CursorMoved>();
        app.register_type::<crate::editing_events::SelectionChanged>();
        app.add_message::<OnEdit>();
        app.add_message::<crate::editing_events::CursorMoved>();
        app.add_message::<crate::editing_events::SelectionChanged>();

        register_editing_events(app);

        if self.typing_observer {
            app.add_observer(on_focused_keyboard_typing);
        }

        register_handler_systems(app);

        app.configure_sets(Update, EditEmitSet);
        app.add_systems(
            Update,
            (
                mirror_snapshot_marker,
                emit_edit_triggers,
                emit_cursor_moved,
                emit_selection_changed,
            )
                .chain()
                .in_set(EditEmitSet),
        );
    }
}

/// Mirrors [`SnapshotPreEdit`] into `EditHistoryState.snapshot_pre_edits` so
/// `replace_range` can decide to clone without an extra Bevy query.
fn mirror_snapshot_marker(
    mut q: Query<(&mut EditHistoryState, Has<SnapshotPreEdit>), With<TextEditor>>,
) {
    for (mut hist, has_marker) in q.iter_mut() {
        if hist.snapshot_pre_edits != has_marker {
            hist.snapshot_pre_edits = has_marker;
        }
    }
}

/// Drain pending edit fields into per-entity [`OnEdit`] triggers. Runs once per frame.
pub fn emit_edit_triggers(
    mut commands: Commands,
    mut q: Query<(Entity, &mut EditHistoryState), With<TextEditor>>,
) {
    for (entity, mut hist) in q.iter_mut() {
        if hist.pending_byte_edit.is_none() && hist.pre_edit_rope.is_none() {
            continue;
        }
        let byte_edit = hist.pending_byte_edit.take();
        let pre_edit_rope = hist.pre_edit_rope.take();
        commands.trigger(OnEdit {
            entity,
            byte_edit,
            pre_edit_rope,
        });
    }
}

/// Compare each editor's `cursor_pos` to the previous frame's, emit one
/// `CursorMoved` per actual movement. The state's `last_cursor_pos` is the
/// auto-scroll detector's field; we keep our own tracker so we don't race
/// with that system's reads.
pub fn emit_cursor_moved(
    mut writer: MessageWriter<crate::editing_events::CursorMoved>,
    q: ChangedCursorQuery,
    all: Query<Entity, With<TextEditor>>,
    mut last: Local<std::collections::HashMap<Entity, usize>>,
) {
    for (entity, cursor) in q.iter() {
        let prev = last.insert(entity, cursor.cursor_pos);
        if prev != Some(cursor.cursor_pos) {
            writer.write(crate::editing_events::CursorMoved {
                entity,
                from: prev.unwrap_or(cursor.cursor_pos),
                to: cursor.cursor_pos,
            });
        }
    }
    last.retain(|e, _| all.get(*e).is_ok());
}

/// Watch the selection collection for any change (anchor/head/mode/count)
/// and emit `SelectionChanged`. Hashes the collection's anchored points so
/// the dedupe survives a re-construction that produces the same selection.
pub fn emit_selection_changed(
    mut writer: MessageWriter<crate::editing_events::SelectionChanged>,
    q: ChangedSelectionQuery,
    mut last: Local<std::collections::HashMap<Entity, u64>>,
    all: Query<Entity, With<TextEditor>>,
) {
    use std::hash::{Hash, Hasher};
    for (entity, sel) in q.iter() {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for s in sel.selections.iter() {
            s.head_offset().hash(&mut hasher);
            s.anchor_offset().hash(&mut hasher);
        }
        sel.selections.iter().count().hash(&mut hasher);
        let fingerprint = hasher.finish();

        if last.get(&entity) == Some(&fingerprint) {
            continue;
        }
        last.insert(entity, fingerprint);

        let total: usize = sel.selections.iter().map(|s| s.end() - s.start()).sum();
        writer.write(crate::editing_events::SelectionChanged {
            entity,
            selection_count: sel.selections.iter().count(),
            total_chars_selected: total,
        });
    }
    last.retain(|e, _| all.get(*e).is_ok());
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
