//! Push the terminal caret into the engine's `TextViewOverlays`.
//!
//! Reuses [`bevy_instanced_text_edit::caret_overlay`] so blink + shape match the
//! editor. Drains z=1 caret rects from prior frames before pushing fresh
//! ones (same convention the editor uses).

use bevy::prelude::*;
use bevy_instanced_text_edit::{caret_overlay, cursor_blink_visible, BlinkPhase, CursorSettings, EditTheme};
use bevy_instanced_text::{FontConfig, TextViewOverlays};

use crate::types::TerminalGridSnapshot;

/// Per-terminal grid-cell cache used to detect cursor moves. Paired with
/// [`bevy_instanced_text_edit::BlinkPhase`] on the same entity, which holds the
/// shared timestamp the blink helper reads.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct TerminalCursorCell {
    pub last_row: u32,
    pub last_col: u16,
}

/// Track cursor moves so the blink phase resets on movement (matches the
/// editor's behavior: cursor stays solid for half a second after a move).
pub fn track_cursor_blink(
    time: Res<Time>,
    mut q: Query<(&TerminalGridSnapshot, &mut TerminalCursorCell, &mut BlinkPhase)>,
) {
    for (snapshot, mut cell, mut blink) in q.iter_mut() {
        if snapshot.cursor_row != cell.last_row || snapshot.cursor_col != cell.last_col {
            blink.last_change_secs = time.elapsed_secs_f64();
            cell.last_row = snapshot.cursor_row;
            cell.last_col = snapshot.cursor_col;
        }
    }
}

/// Push the caret rect into the engine's overlays.
pub fn push_terminal_caret(
    time: Res<Time>,
    mut q: Query<(
        &TerminalGridSnapshot,
        &BlinkPhase,
        &FontConfig,
        &EditTheme,
        &CursorSettings,
        &mut TextViewOverlays,
    )>,
) {
    for (snapshot, blink, font, theme, cursor_settings, mut overlays) in q.iter_mut() {
        // Drain previous-frame caret (z=1) — same convention as the editor.
        overlays.rects.retain(|r| r.z != 1);

        if snapshot.cursor_hidden {
            overlays.version = overlays.version.wrapping_add(1);
            continue;
        }
        if !cursor_blink_visible(
            cursor_settings.blink_rate,
            cursor_settings.blink_pause_secs,
            time.elapsed_secs_f64(),
            blink.last_change_secs,
        ) {
            overlays.version = overlays.version.wrapping_add(1);
            continue;
        }

        let x_left = snapshot.cursor_col as f32 * font.char_width;
        overlays.rects.push(caret_overlay(
            snapshot.cursor_row,
            x_left,
            cursor_settings,
            theme.cursor,
        ));
        overlays.version = overlays.version.wrapping_add(1);
    }
}
