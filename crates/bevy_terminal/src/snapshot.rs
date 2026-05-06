//! Wezterm grid → `TextBuffer.rope` + per-line `LineStyles`.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_text_engine::{
    LineStyles, RenderTheme, RunWithText, StyleRun, TextBuffer, TextViewViewport,
};
use ropey::Rope;

use crate::backend::{
    ColorAttribute, CursorVisibility, Intensity, Underline as VtUnderline,
};
use crate::types::{TerminalColorPalette, TerminalGridSnapshot, TerminalSession};

#[allow(clippy::too_many_arguments)]
pub fn sync_grid_snapshot(
    mut q: Query<(
        &TerminalSession,
        &mut TextBuffer,
        &TextViewViewport,
        &TerminalColorPalette,
        &RenderTheme,
        &mut LineStyles,
        &mut TerminalGridSnapshot,
    )>,
) {
    for (session, mut buffer, _viewport, palette, render, mut line_styles, mut snapshot) in
        q.iter_mut()
    {
        let term = session.terminal.lock();
        let screen = term.screen();
        let cols = screen.physical_cols;
        let rows = screen.physical_rows;

        let mut text = String::with_capacity(rows * (cols + 1));
        let mut by_line: HashMap<u32, Vec<RunWithText>> = HashMap::with_capacity(rows);

        screen.for_each_phys_line(|phys_y, line| {
            let mut line_text = String::with_capacity(cols);
            let mut runs: Vec<RunWithText> = Vec::new();

            let mut current: Option<(StyleRun, String)> = None;
            for cell in line.visible_cells() {
                let cell_str = cell.str();
                let ch = cell_str.chars().next().unwrap_or(' ');
                line_text.push(ch);

                let attrs = cell.attrs();
                let fg = resolve_color(attrs.foreground(), palette, render, true);
                let bg = resolve_color(attrs.background(), palette, render, false);

                let run_proto = StyleRun {
                    byte_range: 0..0,
                    fg,
                    bg: Some(bg),
                    font_scale: 1.0,
                    skew: 0.0,
                    corner_radius: 0.0,
                    font_weight: match attrs.intensity() {
                        Intensity::Bold => Some(700),
                        Intensity::Half => Some(300),
                        Intensity::Normal => None,
                    },
                    italic: attrs.italic(),
                    font_family: None,
                    decoration: if !matches!(attrs.underline(), VtUnderline::None) {
                        Some(bevy_text_engine::TextDecoration::Underline)
                    } else if attrs.strikethrough() {
                        Some(bevy_text_engine::TextDecoration::Strikethrough)
                    } else {
                        None
                    },
                    link: None,
                };

                match current.as_mut() {
                    Some((prev, buf)) if style_run_matches(prev, &run_proto) => {
                        buf.push(ch);
                    }
                    _ => {
                        if let Some((run, buf)) = current.take() {
                            runs.push(RunWithText { text: buf, run });
                        }
                        let mut buf = String::new();
                        buf.push(ch);
                        current = Some((run_proto, buf));
                    }
                }
            }
            while line_text.chars().count() < cols {
                line_text.push(' ');
            }
            if let Some((run, buf)) = current.take() {
                runs.push(RunWithText { text: buf, run });
            }

            text.push_str(&line_text);
            text.push('\n');
            by_line.insert(phys_y as u32, runs);
        });

        let new_rope = Rope::from_str(&text);
        if rope_text_differs(&buffer.rope, &new_rope) {
            buffer.rope = new_rope;
            buffer.content_version = buffer.content_version.wrapping_add(1);
        }

        *line_styles = LineStyles::new(by_line, 0..rows as u32);

        let cursor = term.cursor_pos();
        snapshot.version = snapshot.version.wrapping_add(1);
        snapshot.cols = cols as u16;
        snapshot.rows = rows as u16;
        let max_row = (rows as u16).saturating_sub(1);
        let max_col = (cols as u16).saturating_sub(1);
        snapshot.cursor_row = (cursor.y.max(0) as u16).min(max_row);
        snapshot.cursor_col = (cursor.x as u16).min(max_col);
        snapshot.cursor_hidden = matches!(cursor.visibility, CursorVisibility::Hidden);
    }
}

fn style_run_matches(a: &StyleRun, b: &StyleRun) -> bool {
    a.fg == b.fg
        && a.bg == b.bg
        && a.font_weight == b.font_weight
        && a.italic == b.italic
        && a.decoration == b.decoration
}

fn rope_text_differs(rope: &Rope, candidate: &Rope) -> bool {
    rope.len_chars() != candidate.len_chars() || rope != candidate
}

fn resolve_color(
    color: ColorAttribute,
    palette: &TerminalColorPalette,
    render: &RenderTheme,
    is_fg: bool,
) -> Color {
    match color {
        ColorAttribute::Default => {
            if is_fg {
                render.foreground
            } else {
                render.background
            }
        }
        ColorAttribute::PaletteIndex(idx) => {
            if (idx as usize) < palette.ansi.len() {
                palette.ansi[idx as usize]
            } else {
                indexed_to_color(idx)
            }
        }
        ColorAttribute::TrueColorWithPaletteFallback(color, _)
        | ColorAttribute::TrueColorWithDefaultFallback(color) => {
            Color::srgba(color.0, color.1, color.2, color.3)
        }
    }
}

fn indexed_to_color(idx: u8) -> Color {
    if idx < 16 {
        return Color::srgb(0.5, 0.5, 0.5);
    }
    if idx < 232 {
        let n = idx - 16;
        let r = (n / 36) % 6;
        let g = (n / 6) % 6;
        let b = n % 6;
        let to_f = |c: u8| if c == 0 { 0.0 } else { (40.0 * c as f32 + 55.0) / 255.0 };
        return Color::srgb(to_f(r), to_f(g), to_f(b));
    }
    let step = idx - 232;
    let v = (8.0 + 10.0 * step as f32) / 255.0;
    Color::srgb(v, v, v)
}
