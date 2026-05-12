use bevy::prelude::*;
use bevy_instanced_text::{TextFont, TextViewport};

use crate::backend;
use crate::types::TerminalSession;

pub const MIN_COLS: u16 = 2;
pub const MIN_ROWS: u16 = 1;

/// Convert a viewport + font into a (cols, rows) cell count, or `None`
/// if the viewport hasn't been laid out yet (zero-area).
pub fn cells_from_viewport(viewport: &TextViewport, font: &TextFont) -> Option<(u16, u16)> {
    let usable_w = (viewport.width as f32 - viewport.text_area_left).max(0.0);
    let usable_h = (viewport.height as f32 - viewport.text_area_top).max(0.0);
    if usable_w <= 0.0 || usable_h <= 0.0 || font.char_width <= 0.0 || font.line_height <= 0.0 {
        return None;
    }
    let cols = (usable_w / font.char_width).floor() as u16;
    let rows = (usable_h / font.line_height).floor() as u16;
    Some((cols, rows))
}

#[allow(clippy::type_complexity)]
pub fn sync_terminal_size(
    mut q: Query<
        (&TextViewport, &TextFont, &mut TerminalSession),
        Or<(Changed<TextViewport>, Changed<TextFont>)>,
    >,
    windows: Query<&bevy::window::Window, With<bevy::window::PrimaryWindow>>,
) {
    let scale = windows
        .single()
        .map(|w| w.scale_factor() as u32)
        .unwrap_or(1)
        .max(1);

    for (viewport, font, mut session) in q.iter_mut() {
        let Some((raw_cols, raw_rows)) = cells_from_viewport(viewport, font) else {
            continue;
        };
        let cols = raw_cols.max(MIN_COLS);
        let rows = raw_rows.max(MIN_ROWS);

        if session.size.cols as u16 == cols && session.size.rows as u16 == rows {
            continue;
        }

        let cell_w = font.char_width.round().max(1.0) as u16;
        let cell_h = font.line_height.round().max(1.0) as u16;
        let new_size = backend::TerminalSize {
            cols: cols as usize,
            rows: rows as usize,
            pixel_width: (cols * cell_w) as usize,
            pixel_height: (rows * cell_h) as usize,
            dpi: scale,
        };

        session.terminal.lock().resize(new_size);
        session.size = new_size;
    }
}
