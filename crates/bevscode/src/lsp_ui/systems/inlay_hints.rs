//! Inlay hints: drain responses and request hints for the visible range.

use bevy::prelude::*;
use bevy_instanced_text_editor::RopeBuffer;
use lsp_types::*;

use crate::text_view::InstancedText;
use crate::types::CodeEditor;
use bevy::input_focus::InputFocus;
use bevy::ui::{ComputedNode, ScrollPosition};
use bevy_instanced_text::MonoCellWidth;

use super::super::state::LspInlayHints;
use bevy_lsp::{LspDocument, LspInlayHintsResponse, LspMessage, LspRequest, ServerCapabilities};

type RequestInlayHintsQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ServerCapabilities,
        Ref<'static, InstancedText<RopeBuffer>>,
        Ref<'static, ScrollPosition>,
        Ref<'static, ComputedNode>,
        Option<&'static LspDocument>,
        &'static mut LspInlayHints,
        &'static TextFont,
        &'static bevy::text::LineHeight,
        &'static MonoCellWidth,
        Option<&'static crate::settings::Suggest>,
        Option<&'static crate::lsp_ui::session::LspSession>,
    ),
    With<CodeEditor>,
>;

pub fn on_lsp_inlay_hints(
    mut events: MessageReader<LspInlayHintsResponse>,
    mut q: Query<&mut LspInlayHints, With<CodeEditor>>,
) {
    for ev in events.read() {
        let Ok(mut hint_state) = q.get_mut(ev.entity) else {
            continue;
        };
        #[cfg(debug_assertions)]
        debug!("[LSP] InlayHints: {} hint(s)", ev.hints.len());
        hint_state.hints = ev.hints.clone();
        hint_state.needs_refresh = false;
    }
}

/// System to request inlay hints for visible range
pub fn request_inlay_hints(
    mut query: RequestInlayHintsQuery,
    mut lsp_w: MessageWriter<LspRequest>,
    lsp_ready: crate::lsp_ui::session::LspReady,
    input_focus: Res<InputFocus>,
) {
    let Some(focused) = input_focus.get() else {
        return;
    };
    let Ok((
        entity,
        capabilities,
        buffer,
        scroll,
        computed,
        lsp_document,
        mut hint_state,
        font,
        lh,
        _mono,
        suggest,
        session,
    )) = query.get_mut(focused)
    else {
        return;
    };
    let line_height = bevy_instanced_text::resolve_line_height(*lh, font.font_size);
    if !lsp_ready.is_ready(entity) || !capabilities.supports_inlay_hints() {
        return;
    }
    if let Some(s) = suggest {
        use crate::settings::InlayHintsEnabled;
        if matches!(
            s.inlay_hints.enabled,
            InlayHintsEnabled::Off | InlayHintsEnabled::OnUnlessPressed
        ) {
            return;
        }
    }

    if !hint_state.needs_refresh
        && !buffer.is_changed()
        && !scroll.is_changed()
        && !computed.is_changed()
    {
        return;
    }

    // The buffer changing invalidates the cache by line number: the line
    // that *was* `let mut app = App::new()` (with an `app:` parameter hint)
    // might now be `let zzz = 1`, but the cached `cached_range` still
    // covers the same line indices, so without this the stale hints would
    // stick to the new content until the user scrolled. Clearing here
    // also drops the visible labels immediately while the new request is
    // in flight, instead of leaving wrong text on-screen for a round trip.
    if buffer.is_changed() {
        hint_state.invalidate();
    }

    let Some(lsp_document) = lsp_document else {
        return;
    };

    // Calculate visible range with some buffer
    let inv = computed.inverse_scale_factor();
    let viewport_height = computed.size().y * inv;
    let visible_start_line = (scroll.y / line_height) as u32;
    let visible_lines = (viewport_height / line_height) as u32 + 10;
    let visible_end_line = (visible_start_line + visible_lines).min(buffer.len_lines() as u32);

    let range = Range {
        start: Position {
            line: visible_start_line,
            character: 0,
        },
        end: Position {
            line: visible_end_line,
            character: 0,
        },
    };

    // Check if range is already cached
    if hint_state.is_range_cached(&range) && !hint_state.needs_refresh {
        return;
    }

    lsp_w.write(crate::lsp_ui::session::lsp_request(
        entity,
        session,
        LspMessage::InlayHint {
            uri: lsp_document.uri.clone(),
            range,
            id: 0,
        },
    ));

    hint_state.cached_range = Some(range);
    hint_state.needs_refresh = false;
}
