//! Wezterm grid → `TextBuffer.rope` + per-line `LineStyles`.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_instanced_text::{
    FontConfig, LineStyles, RenderTheme, RunWithText, ScrollState, StyleRun, TextBuffer,
    TextViewViewport,
};
use ropey::Rope;
use wezterm_surface::SequenceNo;
use wezterm_term::Line as VtLine;

use crate::backend::{
    ColorAttribute, CursorVisibility, Intensity, Underline as VtUnderline,
};
use crate::types::{
    TerminalColorPalette, TerminalGridSnapshot, TerminalScrollFollow, TerminalSession,
};

/// Cached shape of one phys row from a previous rebuild. We carry these forward
/// across frames so unchanged rows (the common case during cursor blink, or
/// when only one line of the prompt redraws) don't have to walk cells +
/// rebuild `RunWithText` runs again.
#[derive(Clone)]
struct CachedLine {
    /// Padded line text (cols chars, no trailing newline). Concatenated into
    /// the rope with `\n` separators on rebuild.
    text: String,
    /// Style runs covering `text`. Cloned out of the cache when reused.
    runs: Vec<RunWithText>,
}

/// Per-entity rebuild-cache for `sync_grid_snapshot`. Kept in a `Local` because
/// it's pure derived state — hosts never need to read it.
#[doc(hidden)]
#[derive(Default)]
pub struct RebuildCache {
    /// Last `Term::current_seqno()` we rebuilt against. We still re-anchor the
    /// scroll on every frame (viewport resizes change `max_scroll`), but skip
    /// the (expensive) rope + style rebuild when nothing in the term changed.
    last_seqno: Option<SequenceNo>,
    /// Last visible-row count we rebuilt against. A resize changes which rows
    /// are visible without bumping `current_seqno()`, so we force a rebuild
    /// whenever this changes.
    last_rows: usize,
    /// Last column count. A horizontal resize re-pads every line; bypass the
    /// per-row dirty test in that case.
    last_cols: usize,
    /// Last total scrollback row count. When scrollback shifts (a new line
    /// scrolls in, or eviction drops the top), our phys-row index becomes
    /// invalid — bypass the per-row gate and rebuild from scratch.
    last_total_lines: usize,
    /// Per-phys-row shape cache. Indexed by phys row; len == last_total_lines
    /// after a successful rebuild. Cleared whenever total_lines/cols change.
    lines: Vec<CachedLine>,
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
        &mut TerminalScrollFollow,
    )>,
    mut cache: Local<HashMap<Entity, RebuildCache>>,
) {
    cache.retain(|e, _| q.contains(*e));
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
        mut follow,
    ) in q.iter_mut()
    {
        let cache_entry = cache.entry(entity).or_default();

        let term = session.terminal.lock();
        let screen = term.screen();
        let cols = screen.physical_cols;
        let rows = screen.physical_rows;
        let total_lines = screen.scrollback_rows();
        let scrollback_offset = total_lines.saturating_sub(rows);
        let seqno = term.current_seqno();
        let needs_rebuild =
            cache_entry.last_seqno != Some(seqno) || cache_entry.last_rows != rows;

        if !needs_rebuild {
            drop(term);
            anchor_scroll_to_bottom(&mut scroll, viewport, font, total_lines, &mut follow);
            continue;
        }

        // Per-line dirty gate: skip cell→run reshaping for rows whose
        // `Line::changed_since(prev_seqno)` is false. Only valid when the
        // overall shape of the buffer (cols + total_lines) hasn't changed
        // since the last rebuild — otherwise phys-row indices into our cache
        // no longer mean the same thing.
        let cache_valid = cache_entry.last_seqno.is_some()
            && cache_entry.last_cols == cols
            && cache_entry.last_total_lines == total_lines
            && cache_entry.lines.len() == total_lines;
        let prev_seqno = cache_entry.last_seqno.unwrap_or(0);

        let mut text = String::with_capacity(total_lines * (cols + 1));
        let mut by_line: HashMap<u32, Vec<RunWithText>> = HashMap::with_capacity(total_lines);
        let mut next_lines: Vec<CachedLine> = Vec::with_capacity(total_lines);

        screen.for_each_phys_line(|phys_y, line| {
            if cache_valid && !line.changed_since(prev_seqno) {
                let cached = &cache_entry.lines[phys_y];
                text.push_str(&cached.text);
                text.push('\n');
                by_line.insert(phys_y as u32, cached.runs.clone());
                next_lines.push(cached.clone());
                return;
            }

            let (line_text, runs) = shape_phys_line(line, cols, palette, render);
            text.push_str(&line_text);
            text.push('\n');
            by_line.insert(phys_y as u32, runs.clone());
            next_lines.push(CachedLine {
                text: line_text,
                runs,
            });
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

        cache_entry.last_seqno = Some(seqno);
        cache_entry.last_rows = rows;
        cache_entry.last_cols = cols;
        cache_entry.last_total_lines = total_lines;
        cache_entry.lines = next_lines;
        anchor_scroll_to_bottom(&mut scroll, viewport, font, total_lines, &mut follow);
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
    follow: &mut TerminalScrollFollow,
) {
    let line_height = font.line_height;
    if line_height <= 0.0 {
        return;
    }
    let viewport_height = viewport.height as f32;
    let visible_rows = ((viewport_height - viewport.text_area_top) / line_height)
        .floor()
        .max(0.0) as usize;
    let hidden_rows = total_lines.saturating_sub(visible_rows);
    let max_scroll = -(hidden_rows as f32 * line_height);
    let stick_threshold = line_height;

    // If the target moved away from where we last anchored, the user wheeled
    // (or a host write moved it). `on_pointer_scroll` writes
    // `target_scroll_offset` directly. The 0.5 px window keeps smooth-scroll
    // floating-point jitter from tripping us.
    if (scroll.target_scroll_offset - follow.last_applied_target).abs() > 0.5 {
        follow.stick_to_bottom = scroll.target_scroll_offset - max_scroll <= stick_threshold;
    }

    if follow.stick_to_bottom {
        scroll.scroll_offset = max_scroll;
        scroll.target_scroll_offset = max_scroll;
        follow.last_applied_target = max_scroll;
    } else if scroll.target_scroll_offset - max_scroll <= stick_threshold {
        // Re-engage when the user wheeled back to within one row of the bottom.
        follow.stick_to_bottom = true;
        scroll.scroll_offset = max_scroll;
        scroll.target_scroll_offset = max_scroll;
        follow.last_applied_target = max_scroll;
    } else {
        follow.last_applied_target = scroll.target_scroll_offset;
    }
}

/// Walk a single phys row's cells and produce `(padded_text, style_runs)`.
/// Lifted out of the main loop so the per-line dirty gate can reuse cached
/// rows without duplicating the cell-walk logic.
fn shape_phys_line(
    line: &VtLine,
    cols: usize,
    palette: &TerminalColorPalette,
    render: &RenderTheme,
) -> (String, Vec<RunWithText>) {
    let mut line_text = String::with_capacity(cols);
    let mut runs: Vec<RunWithText> = Vec::new();
    let mut current: Option<(StyleRun, String)> = None;

    for cell in line.visible_cells() {
        let cell_str = cell.str();
        let ch = cell_str.chars().next().unwrap_or(' ');
        line_text.push(ch);

        let attrs = cell.attrs();
        let fg = resolve_color(attrs.foreground(), palette, render, true);
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
            font: None,
            decoration: {
                let mut d = bevy_instanced_text::TextDecoration::empty();
                if !matches!(attrs.underline(), VtUnderline::None) { d |= bevy_instanced_text::TextDecoration::UNDERLINE; }
                if attrs.strikethrough() { d |= bevy_instanced_text::TextDecoration::STRIKETHROUGH; }
                d
            },
            link: None,
        };

        match current.as_mut() {
            Some((prev, buf)) if style_run_matches(prev, &run_proto) => buf.push(ch),
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
    (line_text, runs)
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
