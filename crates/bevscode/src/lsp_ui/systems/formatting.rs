//! Formatting: apply formatting responses to the buffer.

use bevy::prelude::*;
use bevy_instanced_text_editor::RopeBuffer;

use crate::text_view::InstancedText;
use crate::types::CodeEditor;

use super::shared::apply_text_edits;
use bevy_lsp::LspFormatResponse;

pub fn on_lsp_format(
    mut events: MessageReader<LspFormatResponse>,
    q: Query<&InstancedText<RopeBuffer>, With<CodeEditor>>,
    mut replace_writer: MessageWriter<bevy_instanced_text_editor::ReplaceRangeRequested>,
) {
    for ev in events.read() {
        let Ok(buffer) = q.get(ev.entity) else {
            continue;
        };
        trace!("[LSP] Format: {} edit(s)", ev.edits.len());
        apply_text_edits(ev.entity, buffer, ev.edits.clone(), &mut replace_writer);
    }
}
