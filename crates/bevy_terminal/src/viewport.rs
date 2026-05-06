//! Viewport-change → PTY + `Terminal` resize.

use bevy::prelude::*;
use bevy_text_engine::{FontConfig, TextViewViewport};
use portable_pty::PtySize;

use crate::backend;
use crate::types::TerminalSession;

const MIN_COLS: u16 = 2;
const MIN_ROWS: u16 = 1;

#[allow(clippy::type_complexity)]
pub fn sync_terminal_size(
    mut q: Query<
        (&TextViewViewport, &FontConfig, &mut TerminalSession),
        Or<(Changed<TextViewViewport>, Changed<FontConfig>)>,
    >,
) {
    for (viewport, font, mut session) in q.iter_mut() {
        let usable_w = (viewport.width as f32 - viewport.text_area_left).max(0.0);
        let usable_h = (viewport.height as f32 - viewport.text_area_top).max(0.0);
        let cols = ((usable_w / font.char_width).floor() as u16).max(MIN_COLS);
        let rows = ((usable_h / font.line_height).floor() as u16).max(MIN_ROWS);

        if session.size.cols as u16 == cols && session.size.rows as u16 == rows {
            continue;
        }

        let cell_w = font.char_width.round() as u16;
        let cell_h = font.line_height.round() as u16;
        let new_size = backend::TerminalSize {
            cols: cols as usize,
            rows: rows as usize,
            pixel_width: (cols * cell_w) as usize,
            pixel_height: (rows * cell_h) as usize,
            dpi: 0,
        };
        let pty_size = PtySize {
            cols,
            rows,
            pixel_width: cols * cell_w,
            pixel_height: rows * cell_h,
        };

        session.terminal.lock().resize(new_size);
        let _ = session.pty_master.lock().resize(pty_size);
        session.size = new_size;
    }
}
