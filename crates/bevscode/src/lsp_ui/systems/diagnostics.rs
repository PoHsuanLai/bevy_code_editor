//! Diagnostics: drain published diagnostics into `DiagnosticMarker` entities.

use bevy::prelude::*;
use bevy_instanced_text_editor::RopeBuffer;
use lsp_types::*;

use crate::text_view::InstancedText;
use crate::types::CodeEditor;
use bevy_lsp::{LspDiagnosticsUpdated, LspDocument};

/// Diagnostic marker for rendering in editor
#[derive(Component, Clone, Debug)]
pub struct DiagnosticMarker {
    /// URI of the file this diagnostic belongs to — used to scope despawn to
    /// the editor whose document matches the incoming batch's URI.
    pub uri: Url,
    /// Line number (0-indexed)
    pub line: usize,
    /// Diagnostic severity
    pub severity: DiagnosticSeverity,
    /// Diagnostic message
    pub message: String,
    /// Text range
    pub range: Range,
}

/// Replace `DiagnosticMarker` entities for the editor whenever the server
/// publishes a fresh diagnostic batch.
pub fn on_lsp_diagnostics(
    mut commands: Commands,
    mut events: MessageReader<LspDiagnosticsUpdated>,
    diagnostics_q: Query<(Entity, &DiagnosticMarker)>,
    editors: Query<
        (
            &crate::settings::RenderSettings,
            &crate::settings::Misc,
            Option<&LspDocument>,
        ),
        With<CodeEditor>,
    >,
) {
    for ev in events.read() {
        let Ok((render, misc, lsp_document)) = editors.get(ev.entity) else {
            info!(
                "[LSP] on_lsp_diagnostics: dropping event for entity={} (not a CodeEditor with RenderSettings+Misc); diag_count={}",
                ev.entity,
                ev.diagnostics.len(),
            );
            continue;
        };
        // The LSP server publishes diagnostics for every file it knows about,
        // not just the one this editor has open. Skip batches whose URI does
        // not match this editor's document — otherwise we paint another file's
        // line offsets onto this editor's buffer.
        if let Some(doc) = lsp_document {
            if doc.uri != ev.uri {
                info!(
                    "[LSP] on_lsp_diagnostics: skipping entity={} (uri mismatch: ev.uri={} doc.uri={})",
                    ev.entity, ev.uri, doc.uri,
                );
                continue;
            }
            // Drop batches the server computed against an older buffer
            // version: their (line, col) offsets refer to text that no
            // longer exists. The next batch (against the latest didChange)
            // will arrive once the server catches up.
            if let Some(v) = ev.version {
                if v < doc.version() {
                    info!(
                        "[LSP] on_lsp_diagnostics: dropping stale batch entity={} (ev.version={} < doc.version={})",
                        ev.entity,
                        v,
                        doc.version(),
                    );
                    continue;
                }
            }
        }
        let render_decorations = match render.render_validation_decorations {
            crate::settings::RenderValidationDecorations::Off => false,
            crate::settings::RenderValidationDecorations::On => true,
            crate::settings::RenderValidationDecorations::Editable => !misc.read_only,
        };
        info!(
            "[LSP] on_lsp_diagnostics: entity={} uri={} diag_count={} render_decorations={} (mode={:?} read_only={})",
            ev.entity,
            ev.uri,
            ev.diagnostics.len(),
            render_decorations,
            render.render_validation_decorations,
            misc.read_only,
        );
        for (i, d) in ev.diagnostics.iter().enumerate() {
            info!(
                "[LSP]   diag[{}] line={} col={}..{} severity={:?} src={:?} code={:?} msg={:?}",
                i,
                d.range.start.line,
                d.range.start.character,
                d.range.end.character,
                d.severity,
                d.source,
                d.code,
                d.message,
            );
        }
        for (entity, marker) in diagnostics_q.iter() {
            if marker.uri == ev.uri {
                commands
                    .entity(entity)
                    .queue_silenced(bevy::ecs::system::entity_command::despawn());
            }
        }
        if !render_decorations {
            continue;
        }
        for diagnostic in &ev.diagnostics {
            commands.spawn(DiagnosticMarker {
                uri: ev.uri.clone(),
                line: diagnostic.range.start.line as usize,
                severity: diagnostic.severity.unwrap_or(DiagnosticSeverity::HINT),
                message: diagnostic.message.clone(),
                range: diagnostic.range,
            });
        }
    }
}

/// Drop `DiagnosticMarker` entities whose URI matches a recently-edited
/// editor's document. Stored line numbers refer to the pre-edit buffer; once
/// the user types, those numbers no longer match the current text, so the
/// squiggle would land on the wrong line. We clear them and wait for the
/// next `publishDiagnostics` from the server.
pub fn clear_stale_diagnostics_on_edit(
    mut commands: Commands,
    diagnostics_q: Query<(Entity, &DiagnosticMarker)>,
    editors: Query<(Ref<InstancedText<RopeBuffer>>, &LspDocument), With<CodeEditor>>,
) {
    for (buffer, doc) in editors.iter() {
        if !buffer.is_changed() {
            continue;
        }
        for (entity, marker) in diagnostics_q.iter() {
            if marker.uri == doc.uri {
                commands
                    .entity(entity)
                    .queue_silenced(bevy::ecs::system::entity_command::despawn());
            }
        }
    }
}
