//! "Go to line" dialog state and its input interceptor.

use bevy_instanced_text_editor::RopeBuffer;

use crate::text_view::InstancedText;
use crate::types::{CursorState, SelectionState};
use bevy::prelude::*;

/// Per-editor "go to line" dialog state.
#[derive(Clone, Debug, Default, Component, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct GotoLineState {
    pub active: bool,
    pub input: String,
}

impl GotoLineState {
    /// Returns the parsed line number (1-indexed), or `None` on invalid input.
    pub fn parse_line_number(&self) -> Option<usize> {
        self.input.trim().parse::<usize>().ok()
    }

    pub fn goto(
        &self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        buffer: &InstancedText<RopeBuffer>,
    ) -> bool {
        if let Some(line_num) = self.parse_line_number() {
            let total_lines = buffer.len_lines();
            // 1-indexed input → 0-indexed, clamped
            let target_line = line_num
                .saturating_sub(1)
                .min(total_lines.saturating_sub(1));
            let char_pos = buffer.line_to_char(target_line);
            cursor.cursor_pos = char_pos;
            sel.apply_primary_cursor(cursor);

            return true;
        }
        false
    }

    pub fn clear(&mut self) {
        self.active = false;
        self.input.clear();
    }
}

/// Goto-line dialog interceptor.
///
/// When the dialog is active and the user presses `ClearSelection`
/// (Escape), dismisses the dialog without falling through to the
/// `bevy_instanced_text_editor::ClearSelectionRequested` handler. Returns `true`
/// when the action was consumed.
pub fn goto_line_intercept(
    action: crate::input::keybindings::EditorAction,
    state: &mut GotoLineState,
) -> bool {
    if matches!(
        action,
        crate::input::keybindings::EditorAction::ClearSelection
    ) && state.active
    {
        state.clear();
        return true;
    }
    false
}
