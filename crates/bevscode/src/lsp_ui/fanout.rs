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
            mut events: MessageReader<$msg_ty>,
            editors: Query<(Entity, &LspSession), With<CodeEditor>>,
            mut writer: MessageWriter<$msg_ty>,
        ) {
            for ev in events.read() {
                let service = ev.entity;
                if editors.get(service).is_ok() {
                    continue;
                }
                for (editor, session) in editors.iter() {
                    if session.0 == service {
                        let mut cloned = ev.clone();
                        cloned.entity = editor;
                        writer.write(cloned);
                    }
                }
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
    mut events: MessageReader<LspDiagnosticsUpdated>,
    editors: Query<(Entity, &LspSession, Option<&bevy_lsp::LspDocument>), With<CodeEditor>>,
    mut writer: MessageWriter<LspDiagnosticsUpdated>,
) {
    for ev in events.read() {
        let service = ev.entity;
        if editors.get(service).is_ok() {
            continue;
        }
        for (editor, session, doc) in editors.iter() {
            if session.0 != service {
                continue;
            }
            if doc.is_some_and(|d| d.uri == ev.uri) {
                writer.write(LspDiagnosticsUpdated {
                    entity: editor,
                    ..ev.clone()
                });
            }
        }
    }
}
