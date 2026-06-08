use bevy::prelude::*;
use bevy::ui::ComputedNode;
use bevy_instanced_text::MonoCellWidth;

use crate::backend;
use crate::text::TerminalSession;

pub const MIN_COLS: u16 = 2;
pub const MIN_ROWS: u16 = 1;

/// Convert a computed node + cell dimensions into a (cols, rows) cell count, or `None`
/// if the viewport hasn't been laid out yet (zero-area).
pub fn cells_from_viewport(
    computed: &ComputedNode,
    char_width: f32,
    line_height: f32,
) -> Option<(u16, u16)> {
    let inv = computed.inverse_scale_factor();
    let usable_w = (computed.size().x * inv - computed.content_inset().min_inset.x * inv).max(0.0);
    let usable_h = (computed.size().y * inv - computed.content_inset().min_inset.y * inv).max(0.0);
    if usable_w <= 0.0 || usable_h <= 0.0 || char_width <= 0.0 || line_height <= 0.0 {
        return None;
    }
    let cols = (usable_w / char_width).floor() as u16;
    let rows = (usable_h / line_height).floor() as u16;
    Some((cols, rows))
}

#[derive(bevy::ecs::query::QueryData)]
#[query_data(mutable)]
pub(crate) struct TerminalSizeRow {
    computed: &'static ComputedNode,
    font: &'static TextFont,
    line_height: &'static bevy::text::LineHeight,
    mono: &'static MonoCellWidth,
    session: &'static mut TerminalSession,
}

type TerminalSizeChanged = Or<(Changed<ComputedNode>, Changed<MonoCellWidth>)>;

pub fn sync_terminal_size(
    mut q: Query<TerminalSizeRow, TerminalSizeChanged>,
    windows: Query<&bevy::window::Window, With<bevy::window::PrimaryWindow>>,
) {
    let scale = windows
        .single()
        .map(|w| w.scale_factor() as u32)
        .unwrap_or(1)
        .max(1);

    for mut row in q.iter_mut() {
        let (computed, font, lh, mono) = (row.computed, row.font, row.line_height, row.mono);
        let session = &mut row.session;
        let line_height = bevy_instanced_text::resolve_line_height(*lh, font.font_size);
        let char_width = mono.px;
        let Some((raw_cols, raw_rows)) = cells_from_viewport(computed, char_width, line_height)
        else {
            continue;
        };
        let cols = raw_cols.max(MIN_COLS);
        let rows = raw_rows.max(MIN_ROWS);

        if session.size.cols as u16 == cols && session.size.rows as u16 == rows {
            continue;
        }

        let cell_w = char_width.round().max(1.0) as u16;
        let cell_h = line_height.round().max(1.0) as u16;
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
