//! Hover: drain hover responses into popup state.

use bevy::prelude::*;

use crate::types::CodeEditor;

use super::super::state::{HoverLifecycle, LspHoverPopup};
use bevy_lsp::LspHoverResponse;

pub fn on_lsp_hover(
    mut events: MessageReader<LspHoverResponse>,
    mut q: Query<(&mut LspHoverPopup, &mut HoverLifecycle), With<CodeEditor>>,
) {
    for ev in events.read() {
        let Ok((mut hover_state, mut hover_lc)) = q.get_mut(ev.entity) else {
            continue;
        };
        // Accept any in-flight response that hasn't been superseded
        // by a more recent reply we've already displayed. rust-analyzer
        // hover round-trips can take seconds on a cold workspace, and
        // by then the move observer may have armed several more
        // requests at nearby positions; a strict id-equality check
        // would drop every one of them.
        if !hover_lc.accept_response(ev.id) {
            continue;
        }
        hover_state.content = ev.content.clone();
        hover_state.kind = ev.kind.clone();
        hover_state.range = ev.range;
        // Mark visible even when content is empty: sync_hover_popup may
        // still produce a popup if a diagnostic covers the trigger
        // position (VSCode shows diagnostics over squiggles even when
        // the server has no hover content for that position).
        hover_state.visible = true;
        // Publish the range as the hot zone so the move observer
        // doesn't re-arm or dismiss while the pointer wanders within
        // the identifier the popup describes.
        hover_lc.hot_zone = ev.range;
    }
}
