//! Helpers shared by the editor's syntax-styling producer system.
//!
//! Converts the editor's internal `LineSegment` shape into the engine's
//! `RunWithText` payload. The producer system `produce_line_styles` calls
//! `EditorSyntaxState::highlight_range` to get segments, then runs each
//! per-line slice through `segs_to_runs` before stuffing it into a
//! [`bevy_instanced_text::LineStyles`] map.

use bevy_instanced_text::{RunWithText, StyleRun, TextDecoration};

use crate::types::LineSegment;

/// Convert a slice of `LineSegment`s (the editor's per-line styling shape)
/// into the engine's `RunWithText` payloads.
///
/// The engine overwrites `byte_range` after concatenation; we leave it
/// `0..0`. Empty-text segments are dropped because the engine indexes by
/// byte length and zero-len entries would shift later run boundaries
/// without effect.
pub(crate) fn segs_to_runs(segs: &[LineSegment]) -> Vec<RunWithText> {
    segs.iter()
        .filter(|s| !s.text.is_empty())
        .map(|s| RunWithText {
            text: s.text.clone(),
            run: StyleRun {
                byte_range: 0..0,
                fg: s.color,
                bg: s.background,
                font_scale: s.font_scale,
                skew: s.skew,
                corner_radius: s.corner_radius,
                font_weight: None,
                italic: false,
                font: None,
                decoration: TextDecoration::empty(),
                link: None,
            },
        })
        .collect()
}
