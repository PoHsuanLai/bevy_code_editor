//! Walk the alacritty `Term` once per frame, build a fresh
//! `TextViewState.rope` of cell text, and write per-line `LineStyles`
//! capturing fg/bg/bold/italic/underline. Engine's `produce_layouts`
//! picks them up automatically.

use std::collections::HashMap;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use bevy::prelude::*;
use bevy_text_engine::{
    LineStyles, RenderTheme, RunWithText, StyleRun, TextViewState, TextViewViewport,
};
use ropey::Rope;
use vte::ansi::{Color as AnsiColor, NamedColor, Rgb};

use crate::types::{TerminalGridSnapshot, TerminalSession, TerminalThemeConfig};

/// Per-frame snapshot system: take the term lock, copy the visible grid
/// into the entity's rope + `LineStyles`, bump `TerminalGridSnapshot.version`.
///
/// Runs in `TerminalSnapshotSet`, after `TerminalApplyStateSet`.
#[allow(clippy::too_many_arguments)]
pub fn sync_grid_snapshot(
    mut q: Query<(
        &TerminalSession,
        &mut TextViewState,
        &TextViewViewport,
        &TerminalThemeConfig,
        &RenderTheme,
        &mut LineStyles,
        &mut TerminalGridSnapshot,
    )>,
) {
    for (session, mut tv, _viewport, palette, render, mut line_styles, mut snapshot) in
        q.iter_mut()
    {
        let term = session.terminal.lock();
        let cols = term.columns();
        let rows = term.screen_lines();

        // Build text + styles in one pass over visible rows. Rope content
        // is one row per line, terminated with `\n` so the rope's line
        // count matches what the engine expects.
        let mut text = String::with_capacity(rows * (cols + 1));
        let mut by_line: HashMap<u32, Vec<RunWithText>> = HashMap::with_capacity(rows);

        for buffer_line in 0..rows {
            let row = &term.grid()[Line(buffer_line as i32)];
            let mut line_text = String::with_capacity(cols);
            let mut runs: Vec<RunWithText> = Vec::new();

            // Coalesce consecutive cells with identical (fg, bg, flags) into
            // a single run. Saves shaping work and overlay churn.
            let mut current: Option<(StyleRun, String)> = None;
            for column in 0..cols {
                let cell = &row[Column(column)];
                let ch = cell.c;
                line_text.push(ch);

                let fg = resolve_color(cell.fg, palette, render);
                let bg = resolve_color(cell.bg, palette, render);
                let flags = cell.flags;

                let run_proto = StyleRun {
                    byte_range: 0..0, // engine rebases
                    fg,
                    bg: Some(bg),
                    font_scale: 1.0,
                    skew: 0.0,
                    corner_radius: 0.0,
                    font_weight: if flags.contains(Flags::BOLD) {
                        Some(700)
                    } else {
                        None
                    },
                    italic: flags.contains(Flags::ITALIC),
                    font_family: None,
                    decoration: if flags.contains(Flags::UNDERLINE) {
                        Some(bevy_text_engine::TextDecoration::Underline)
                    } else if flags.contains(Flags::STRIKEOUT) {
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
            if let Some((run, buf)) = current.take() {
                runs.push(RunWithText { text: buf, run });
            }

            text.push_str(&line_text);
            text.push('\n');
            by_line.insert(buffer_line as u32, runs);
        }

        let new_rope = Rope::from_str(&text);
        // Only swap if changed — avoids spurious DisplayLayout invalidation.
        if rope_text_differs(&tv.rope, &new_rope) {
            tv.rope = new_rope;
            tv.content_version = tv.content_version.wrapping_add(1);
        }

        *line_styles = LineStyles::new(by_line, 0..rows as u32);

        // Cursor — display offset shifts the visible window during scrollback.
        let cursor = term.grid().cursor.point;
        let display_offset = term.grid().display_offset() as i32;
        let cursor_display_row = cursor.line.0 + display_offset;
        snapshot.version = snapshot.version.wrapping_add(1);
        snapshot.cols = cols as u16;
        snapshot.rows = rows as u16;
        snapshot.cursor_row = cursor_display_row.clamp(0, rows as i32 - 1) as u16;
        snapshot.cursor_col = (cursor.column.0 as u16).min(cols as u16 - 1);
        snapshot.cursor_hidden = !term.mode().contains(TermMode::SHOW_CURSOR);
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

/// Map an alacritty `Color` (Named / Spec / Indexed) to a Bevy `Color`
/// using the per-entity `TerminalThemeConfig` palette and engine
/// `RenderTheme` defaults.
pub fn resolve_color(
    c: AnsiColor,
    palette: &TerminalThemeConfig,
    render: &RenderTheme,
) -> Color {
    match c {
        AnsiColor::Named(named) => named_color(named, palette, render),
        AnsiColor::Spec(rgb) => rgb_to_color(rgb),
        AnsiColor::Indexed(idx) => {
            if (idx as usize) < palette.ansi.len() {
                palette.ansi[idx as usize]
            } else {
                indexed_to_color(idx)
            }
        }
    }
}

fn named_color(n: NamedColor, palette: &TerminalThemeConfig, render: &RenderTheme) -> Color {
    use NamedColor as N;
    match n {
        N::Black => palette.ansi[0],
        N::Red => palette.ansi[1],
        N::Green => palette.ansi[2],
        N::Yellow => palette.ansi[3],
        N::Blue => palette.ansi[4],
        N::Magenta => palette.ansi[5],
        N::Cyan => palette.ansi[6],
        N::White => palette.ansi[7],
        N::BrightBlack => palette.ansi[8],
        N::BrightRed => palette.ansi[9],
        N::BrightGreen => palette.ansi[10],
        N::BrightYellow => palette.ansi[11],
        N::BrightBlue => palette.ansi[12],
        N::BrightMagenta => palette.ansi[13],
        N::BrightCyan => palette.ansi[14],
        N::BrightWhite => palette.ansi[15],
        N::Foreground | N::BrightForeground | N::DimForeground => render.foreground,
        N::Background => render.background,
        N::Cursor => render.foreground,
        N::DimBlack => palette.ansi[0],
        N::DimRed => palette.ansi[1],
        N::DimGreen => palette.ansi[2],
        N::DimYellow => palette.ansi[3],
        N::DimBlue => palette.ansi[4],
        N::DimMagenta => palette.ansi[5],
        N::DimCyan => palette.ansi[6],
        N::DimWhite => palette.ansi[7],
    }
}

fn rgb_to_color(rgb: Rgb) -> Color {
    Color::srgb_u8(rgb.r, rgb.g, rgb.b)
}

/// Fallback for indexed colors outside the 16-slot palette (xterm 256
/// extension): the 6×6×6 cube and the 24-step grayscale ramp.
fn indexed_to_color(idx: u8) -> Color {
    if idx < 16 {
        // Caller already handled this via palette[idx]; keep a safe fallback.
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
    // 24 grayscale steps
    let step = idx - 232;
    let v = (8.0 + 10.0 * step as f32) / 255.0;
    Color::srgb(v, v, v)
}

