//! Shared helpers used across LSP system features.

use bevy::prelude::*;
use bevy_instanced_text_editor::RopeBuffer;
use lsp_types::*;

use crate::text_view::InstancedText;

/// Apply text edits by emitting `ReplaceRangeRequested` events. The editor's
/// handler routes each through `replace_range`, keeping history, anchors,
/// and `OnEdit` consistent.
pub(super) fn apply_text_edits(
    entity: Entity,
    buffer: &InstancedText<RopeBuffer>,
    edits: Vec<TextEdit>,
    writer: &mut MessageWriter<bevy_instanced_text_editor::ReplaceRangeRequested>,
) {
    let mut edits_sorted = edits;
    edits_sorted.sort_by(|a, b| {
        let a_pos = (a.range.start.line, a.range.start.character);
        let b_pos = (b.range.start.line, b.range.start.character);
        b_pos.cmp(&a_pos)
    });

    for edit in edits_sorted {
        let start_line = edit.range.start.line as usize;
        let end_line = edit.range.end.line as usize;
        let start_char_col = edit.range.start.character as usize;
        let end_char_col = edit.range.end.character as usize;

        if start_line >= buffer.len_lines() {
            continue;
        }
        let start_pos = (buffer.line_to_char(start_line) + start_char_col).min(buffer.len_chars());
        let end_pos = if end_line < buffer.len_lines() {
            (buffer.line_to_char(end_line) + end_char_col).min(buffer.len_chars())
        } else {
            buffer.len_chars()
        };

        writer.write(bevy_instanced_text_editor::ReplaceRangeRequested {
            entity,
            start: start_pos,
            end: end_pos,
            text: edit.new_text,
            kind: bevy_instanced_text_editor::EditKind::Other,
            record_history: true,
        });
    }
}
