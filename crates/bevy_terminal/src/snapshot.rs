//! Wezterm grid → `TextBuffer.rope` + per-line `LineStyles`.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_text_engine::{
    FontConfig, LineStyles, RenderTheme, RunWithText, ScrollState, StyleRun, TextBuffer,
    TextViewViewport,
};
use ropey::Rope;

use crate::backend::{
    ColorAttribute, CursorVisibility, Intensity, Underline as VtUnderline,
};
use crate::types::{TerminalColorPalette, TerminalGridSnapshot, TerminalSession};

/// Per-entity tracker for `sync_grid_snapshot`. Kept in a `Local` rather than
/// a Component because nothing else in the system needs to read it; it would
/// just add reflection noise to the public API.
pub struct SyncState {
    /// `true` when the view should stay pinned to the bottom on new output.
    /// Flips to `false` the moment the user wheels away from the bottom; flips
    /// back when their scroll position lands within a row of the bottom.
    stick_to_bottom: bool,
    /// `target_scroll_offset` we last wrote, so we can spot the user moving
    /// the scroll out from under us.
    last_applied_target: f32,
    /// Last `Term::current_seqno()` we rebuilt against. We still re-anchor the
    /// scroll on every frame (viewport resizes change `max_scroll`), but skip
    /// the (expensive) rope + style rebuild when nothing in the term changed.
    last_rebuild_seqno: Option<usize>,
    /// Last visible-row count we rebuilt against. A resize changes which rows
    /// are visible without bumping `current_seqno()`, so we force a rebuild
    /// whenever this changes.
    last_rows: usize,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            stick_to_bottom: true,
            last_applied_target: 0.0,
            last_rebuild_seqno: None,
            last_rows: 0,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn sync_grid_snapshot(
    mut q: Query<(
        Entity,
        &TerminalSession,
        &mut TextBuffer,
        &TextViewViewport,
        &FontConfig,
        &mut ScrollState,
        &TerminalColorPalette,
        &RenderTheme,
        &mut LineStyles,
        &mut TerminalGridSnapshot,
    )>,
    mut sync: Local<HashMap<Entity, SyncState>>,
) {
    sync.retain(|e, _| q.contains(*e));
    for (
        entity,
        session,
        mut buffer,
        viewport,
        font,
        mut scroll,
        palette,
        render,
        mut line_styles,
        mut snapshot,
    ) in q.iter_mut()
    {
        let state = sync.entry(entity).or_default();

        let term = session.terminal.lock();
        let screen = term.screen();
        let cols = screen.physical_cols;
        let rows = screen.physical_rows;
        let total_lines = screen.scrollback_rows();
        let scrollback_offset = total_lines.saturating_sub(rows);
        let seqno = term.current_seqno();
        let needs_rebuild = state.last_rebuild_seqno != Some(seqno) || state.last_rows != rows;

        if !needs_rebuild {
            drop(term);
            anchor_scroll_to_bottom(&mut scroll, viewport, font, total_lines, state);
            continue;
        }

        let mut text = String::with_capacity(total_lines * (cols + 1));
        let mut by_line: HashMap<u32, Vec<RunWithText>> = HashMap::with_capacity(total_lines);

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
                // Only emit a per-cell bg quad when the shell actually set a
                // non-default background. Default-bg cells let the view's
                // base `RenderTheme.background` show through.
                let bg = match attrs.background() {
                    ColorAttribute::Default => None,
                    other => Some(resolve_color(other, palette, render, false)),
                };

                let run_proto = StyleRun {
                    byte_range: 0..0,
                    fg,
                    bg,
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

        *line_styles = LineStyles::new(by_line, 0..total_lines as u32);

        let cursor = term.cursor_pos();
        drop(term);

        snapshot.version = snapshot.version.wrapping_add(1);
        snapshot.cols = cols as u16;
        snapshot.rows = rows as u16;
        let cursor_row_in_buffer =
            scrollback_offset as u32 + cursor.y.max(0) as u32;
        let max_row = total_lines.saturating_sub(1) as u32;
        let max_col = (cols as u16).saturating_sub(1);
        snapshot.cursor_row = cursor_row_in_buffer.min(max_row);
        snapshot.cursor_col = (cursor.x as u16).min(max_col);
        snapshot.cursor_hidden = matches!(cursor.visibility, CursorVisibility::Hidden);

        state.last_rebuild_seqno = Some(seqno);
        state.last_rows = rows;
        anchor_scroll_to_bottom(&mut scroll, viewport, font, total_lines, state);
    }
}

/// Keep the bottom of the buffer pinned to the bottom of the viewport when the
/// user is already at (or within one row of) the bottom — terminals follow the
/// latest output by default. If they've wheeled up into scrollback we leave
/// the scroll position alone so they can read history. Wheeling back to within
/// one row of the bottom re-engages follow.
///
/// Convention (from `on_pointer_scroll`): `scroll_offset` is in `[max_scroll, 0]`,
/// where `0` = top of buffer and `max_scroll` (the most-negative value) = bottom.
fn anchor_scroll_to_bottom(
    scroll: &mut ScrollState,
    viewport: &TextViewViewport,
    font: &FontConfig,
    total_lines: usize,
    state: &mut SyncState,
) {
    let line_height = font.line_height;
    if line_height <= 0.0 {
        return;
    }
    let content_height = total_lines as f32 * line_height;
    let viewport_height = viewport.height as f32;
    let max_scroll = (-(content_height - viewport_height + viewport.text_area_top)).min(0.0);
    let stick_threshold = line_height;

    // If the target moved away from where we last anchored, the user wheeled.
    // (`on_pointer_scroll` writes `target_scroll_offset` directly.) Clamp the
    // detection window so floating-point jitter from the smooth-scroll
    // animator doesn't trip us.
    if (scroll.target_scroll_offset - state.last_applied_target).abs() > 0.5 {
        state.stick_to_bottom = scroll.target_scroll_offset - max_scroll <= stick_threshold;
    }

    if state.stick_to_bottom {
        scroll.scroll_offset = max_scroll;
        scroll.target_scroll_offset = max_scroll;
        state.last_applied_target = max_scroll;
    } else {
        // Re-engage if the user wheeled back to within one row of the bottom.
        if scroll.target_scroll_offset - max_scroll <= stick_threshold {
            state.stick_to_bottom = true;
            scroll.scroll_offset = max_scroll;
            scroll.target_scroll_offset = max_scroll;
            state.last_applied_target = max_scroll;
        } else {
            state.last_applied_target = scroll.target_scroll_offset;
        }
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
