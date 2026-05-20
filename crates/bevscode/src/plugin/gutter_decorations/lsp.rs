//! LSP bridge: fan diagnostic markers (spawned by the LSP layer) out
//! into per-editor [`GlyphMarkers`] + [`GutterDecorations`]. Severity
//! → icon kind + colour via the editor's `DiagnosticColors`.

use bevy::prelude::*;
use lsp_types::DiagnosticSeverity;

use crate::types::CodeEditor;

use super::bars::{DecorationKind, GutterDecorations, LineDecoration};
use super::markers::{GlyphKind, GlyphMarker, GlyphMarkers};

#[allow(clippy::type_complexity)]
pub(crate) fn sync_lsp_glyph_markers(
    diagnostics: Query<&crate::lsp_ui::systems::DiagnosticMarker>,
    mut editors: Query<
        (
            &crate::settings::DiagnosticColors,
            &mut GlyphMarkers,
            &mut GutterDecorations,
        ),
        With<CodeEditor>,
    >,
) {
    let mut per_line: std::collections::HashMap<usize, DiagnosticSeverity> = Default::default();
    for diag in diagnostics.iter() {
        let entry = per_line.entry(diag.line).or_insert(diag.severity);
        if severity_rank(diag.severity) > severity_rank(*entry) {
            *entry = diag.severity;
        }
    }
    for (colors, mut markers, mut decorations) in editors.iter_mut() {
        let mut new_markers: Vec<GlyphMarker> = Vec::with_capacity(per_line.len());
        let mut new_bars: Vec<LineDecoration> = Vec::with_capacity(per_line.len());
        for (&line, &severity) in &per_line {
            let (kind, color) = match severity {
                DiagnosticSeverity::ERROR => (GlyphKind::DiagnosticError, colors.error),
                DiagnosticSeverity::WARNING => (GlyphKind::DiagnosticWarning, colors.warning),
                DiagnosticSeverity::INFORMATION => (GlyphKind::DiagnosticInfo, colors.info),
                _ => (GlyphKind::DiagnosticHint, colors.hint),
            };
            new_markers.push(GlyphMarker { line, kind, color });
            new_bars.push(LineDecoration {
                line,
                kind: DecorationKind::DiagnosticBar,
                color,
            });
        }
        markers.0.retain(|m| {
            !matches!(
                m.kind,
                GlyphKind::DiagnosticError
                    | GlyphKind::DiagnosticWarning
                    | GlyphKind::DiagnosticInfo
                    | GlyphKind::DiagnosticHint
            )
        });
        decorations
            .0
            .retain(|d| !matches!(d.kind, DecorationKind::DiagnosticBar));
        markers.0.extend(new_markers);
        decorations.0.extend(new_bars);
    }
}

fn severity_rank(severity: DiagnosticSeverity) -> u8 {
    match severity {
        DiagnosticSeverity::ERROR => 4,
        DiagnosticSeverity::WARNING => 3,
        DiagnosticSeverity::INFORMATION => 2,
        _ => 1,
    }
}
