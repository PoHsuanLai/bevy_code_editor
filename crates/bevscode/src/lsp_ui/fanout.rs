//! Notification fanout for shared [`LspSession`] entities.
//!
//! Server-initiated notifications (diagnostics, crash, refresh, init)
//! arrive stamped with the service entity. These systems re-emit them
//! to each editor whose [`LspSession`] points at that service.

use bevy::prelude::*;
use bevy_lsp::messages::*;

use super::session::LspSession;
use crate::types::CodeEditor;

macro_rules! fanout_broadcast {
    ($fn_name:ident, $msg_ty:ident) => {
        pub(crate) fn $fn_name(
            mut messages: ParamSet<(MessageReader<$msg_ty>, MessageWriter<$msg_ty>)>,
            editors: Query<(Entity, &LspSession), With<CodeEditor>>,
        ) {
            let mut fanned = Vec::new();
            for ev in messages.p0().read() {
                let service = ev.entity;
                if editors.get(service).is_ok() {
                    continue;
                }
                for (editor, session) in editors.iter() {
                    if session.0 == service {
                        let mut cloned = ev.clone();
                        cloned.entity = editor;
                        fanned.push(cloned);
                    }
                }
            }
            if !fanned.is_empty() {
                messages.p1().write_batch(fanned);
            }
        }
    };
}

fanout_broadcast!(fanout_initialized, LspServerInitialized);
fanout_broadcast!(fanout_crashed, LspServerCrashed);
fanout_broadcast!(fanout_semantic_refresh, LspSemanticTokensRefreshRequested);
fanout_broadcast!(fanout_inlay_refresh, LspInlayHintRefreshRequested);
fanout_broadcast!(fanout_diagnostics_refresh, LspDiagnosticsRefreshRequested);

pub(crate) fn fanout_diagnostics(
    mut messages: ParamSet<(
        MessageReader<LspDiagnosticsUpdated>,
        MessageWriter<LspDiagnosticsUpdated>,
    )>,
    editors: Query<(Entity, &LspSession, Option<&bevy_lsp::LspDocument>), With<CodeEditor>>,
) {
    let mut fanned = Vec::new();
    for ev in messages.p0().read() {
        let service = ev.entity;
        if editors.get(service).is_ok() {
            continue;
        }
        for (editor, session, doc) in editors.iter() {
            if session.0 != service {
                continue;
            }
            if doc.is_some_and(|d| d.uri == ev.uri) {
                fanned.push(LspDiagnosticsUpdated {
                    entity: editor,
                    ..ev.clone()
                });
            }
        }
    }
    if !fanned.is_empty() {
        messages.p1().write_batch(fanned);
    }
}
