//! Editor-core plugin: registers the rope-backed editing systems for any
//! [`TextEditor`] entity. Pair with [`bevy_instanced_text::InstancedTextPlugins`]
//! and [`bevy_instanced_text_interaction::InstancedTextInteractionPlugin`] (the latter
//! is added transitively).

use bevy::prelude::*;
use bevy_instanced_text_interaction::{CursorState, InstancedTextInteractionPlugin, SelectionState};

use crate::text_edit::{self, CursorMoved as EditorCursorMoved, SelectionChanged, *};
use crate::widget;
use crate::text::RopeBuffer;
use crate::text_state::{
    EditHistoryState, IndentConfig, OnEdit, SnapshotPreEdit, TextEditor,
};
use crate::typing::on_focused_keyboard_typing;

type ChangedCursorQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static CursorState), (With<TextEditor>, Changed<CursorState>)>;
type ChangedSelectionQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static SelectionState), (With<TextEditor>, Changed<SelectionState>)>;

/// Contains `emit_edit_triggers`. Schedule downstream systems `.after(EditEmitSet)`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct EditEmitSet;

/// Editable-text core: typed-char insertion, edit history, undo/redo, clipboard.
/// Adds [`InstancedTextInteractionPlugin::<RopeBuffer>`] and the renderer
/// content-type plugin idempotently. Use
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
        // Pull in the shared interaction layer (clipboard, pointer observers,
        // caret/selection components) for the RopeBuffer content type.
        app.add_plugins(InstancedTextInteractionPlugin::<RopeBuffer>::default());

        // Register the RopeBuffer content type so TextBuffer<RopeBuffer>
        // entities participate in layout.
        app.add_plugins(bevy_instanced_text::TextContentPlugin::<RopeBuffer>::default());

        app.register_type::<TextEditor>();
        app.register_type::<IndentConfig>();
        app.register_type::<OnEdit>();
        app.register_type::<SnapshotPreEdit>();
        app.register_type::<EditorCursorMoved>();
        app.register_type::<SelectionChanged>();
        app.add_message::<OnEdit>();
        app.add_message::<EditorCursorMoved>();
        app.add_message::<SelectionChanged>();

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
    mut writer: MessageWriter<EditorCursorMoved>,
    q: ChangedCursorQuery,
    all: Query<Entity, With<TextEditor>>,
    mut last: Local<std::collections::HashMap<Entity, usize>>,
) {
    for (entity, cursor) in q.iter() {
        let prev = last.insert(entity, cursor.cursor_pos);
        if prev != Some(cursor.cursor_pos) {
            writer.write(EditorCursorMoved {
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
    mut writer: MessageWriter<SelectionChanged>,
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
        writer.write(SelectionChanged {
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


    app.add_systems(
        Update,
        (
            widget::cursor_move::handle_move_cursor_left,
            widget::cursor_move::handle_move_cursor_right,
            widget::cursor_move::handle_move_cursor_up,
            widget::cursor_move::handle_move_cursor_down,
            widget::cursor_move::handle_move_cursor_word_left,
            widget::cursor_move::handle_move_cursor_word_right,
            widget::cursor_move::handle_move_cursor_line_start,
            widget::cursor_move::handle_move_cursor_line_end,
            widget::cursor_move::handle_move_cursor_document_start,
            widget::cursor_move::handle_move_cursor_document_end,
            widget::cursor_move::handle_move_cursor_page_up,
            widget::cursor_move::handle_move_cursor_page_down,
        ),
    );

    app.add_systems(
        Update,
        (
            widget::selection::handle_select_left,
            widget::selection::handle_select_right,
            widget::selection::handle_select_up,
            widget::selection::handle_select_down,
            widget::selection::handle_select_word_left,
            widget::selection::handle_select_word_right,
            widget::selection::handle_select_line_start,
            widget::selection::handle_select_line_end,
            widget::selection::handle_select_all,
            widget::selection::handle_clear_selection,
        ),
    );

    app.add_systems(
        Update,
        (
            widget::text_input::handle_insert_newline,
            widget::text_input::handle_insert_tab,
            widget::text_input::handle_delete_backward,
            widget::text_input::handle_delete_forward,
            widget::text_input::handle_delete_word_backward,
            widget::text_input::handle_delete_word_forward,
            widget::text_input::handle_delete_line,
            widget::text_input::handle_undo,
            widget::text_input::handle_redo,
            widget::text_input::handle_replace_range,
            widget::text_input::handle_set_text,
            widget::clipboard::handle_copy,
            widget::clipboard::handle_cut,
            widget::clipboard::handle_paste,
        ),
    );
}
