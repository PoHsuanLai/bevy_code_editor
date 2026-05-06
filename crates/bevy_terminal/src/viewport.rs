//! Viewport-change → PTY + Term resize.
//!
//! Whenever the rendered viewport (size or font cell metrics) changes,
//! recompute (cols, rows) and resize both the alacritty `Term` (so the
//! grid reshapes immediately) and the PTY (so the child process sees a
//! `SIGWINCH`). A change-detection query (`Changed<TextViewViewport>`)
//! gates the system; a fingerprint guard inside catches font changes too.

use alacritty_terminal::event::{OnResize, WindowSize};
use alacritty_terminal::grid::Dimensions;
use bevy::prelude::*;
use bevy_text_engine::{FontConfig, TextViewViewport};

use crate::types::TerminalSession;

/// Tiny `Dimensions` impl so we can pass cols/rows to `Term::resize`.
struct Dims {
    cols: usize,
    rows: usize,
}

impl Dimensions for Dims {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

const MIN_COLS: u16 = 2;
const MIN_ROWS: u16 = 1;

/// Reads viewport + font, reshapes Term + PTY when (cols, rows) changes.
/// Lives in `TerminalApplyStateSet`.
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

        if session.size.num_cols == cols && session.size.num_lines == rows {
            continue;
        }

        let new_size = WindowSize {
            num_cols: cols,
            num_lines: rows,
            cell_width: font.char_width.round() as u16,
            cell_height: font.line_height.round() as u16,
        };

        // Resize the Term (cell grid) under the lock.
        {
            let mut term = session.terminal.lock();
            term.resize(Dims {
                cols: cols as usize,
                rows: rows as usize,
            });
        }
        // Resize the PTY — alacritty's EventLoop handles the actual
        // `SIGWINCH` via `Notifier::on_resize`.
        session.notifier.on_resize(new_size);
        session.size = new_size;
    }
}
