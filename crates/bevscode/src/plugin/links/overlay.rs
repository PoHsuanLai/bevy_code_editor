//! Builds per-URL underline overlays + hit-test ranges for the visible
//! window. The scan output feeds both the rendered underlines
//! ([`LinkRects`]) and the hit-test data ([`LinkRanges`]) the input
//! observers read.

use bevy::prelude::*;
use bevy_instanced_text::{
    visible_buffer_range, CornerRadii, HiddenLines, RectOverlay, RowVertical, TextBounds,
};

use crate::settings::*;
use crate::types::*;

use super::detection::find_urls;
use super::{HoveredLink, LinkRange, LinkRanges, LinkRects};

/// Build per-URL underline overlays + hit-test ranges for the visible
/// window. Mirrors `update_indent_guides` / `update_rulers` in shape.
///
/// Three visual states, matching Monaco:
/// - **Idle** (no hover): no overlay.
/// - **Hover, no ctrl/cmd**: dotted dim underline drawn as multiple short
///   `Underline` segments (engine has no native dotted variant).
/// - **Hover + ctrl/cmd**: solid bright underline (the active state, also
///   the clickable state — observer fires on click).
///
/// `LinkRanges` is populated regardless of hover so the hover observer and
/// the click observer have hit-test data.
#[derive(bevy::ecs::query::QueryData)]
#[query_data(mutable)]
pub(crate) struct LinkOverlayRow {
    rv: EditorRenderView,
    hidden: Option<&'static HiddenLines>,
    bounds: Option<&'static TextBounds>,
    theme: &'static EditorTheme,
    misc: &'static Misc,
    hovered: &'static HoveredLink,
    link_rects: &'static mut LinkRects,
    link_ranges: &'static mut LinkRanges,
}

pub(crate) fn update_link_overlays(
    mut editor_query: Query<LinkOverlayRow, With<CodeEditor>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    let ctrl_held = keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight)
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight);

    for mut row in editor_query.iter_mut() {
        if !row.misc.links {
            if !row.link_rects.0.is_empty() {
                row.link_rects.0.clear();
            }
            if !row.link_ranges.0.is_empty() {
                row.link_ranges.0.clear();
            }
            continue;
        }

        let m = row.rv.metrics();
        let theme = row.theme;
        let hovered = row.hovered;
        let wrap_cfg = row.bounds.copied().unwrap_or_default();
        let visible = visible_buffer_range(
            &**row.rv.buffer,
            row.rv.scroll.y,
            m.viewport_height,
            m.text_area_top,
            m.line_height,
            m.char_width,
            wrap_cfg,
            row.hidden,
        );
        let rv = &row.rv;

        let mut new_rects: Vec<RectOverlay> = Vec::new();
        let mut new_ranges: Vec<LinkRange> = Vec::new();
        let dim = with_alpha(theme.link, 0.45);

        if visible.start < visible.end {
            for buffer_line in visible.start..visible.end {
                if rv.fold.is_line_hidden(buffer_line) {
                    continue;
                }
                let line = rv.buffer.line(buffer_line);
                let line_text = line.to_string();
                let matches = find_urls(&line_text);
                if matches.is_empty() {
                    continue;
                }
                for (start_char, end_char) in matches {
                    let url: String = line_text
                        .chars()
                        .skip(start_char)
                        .take(end_char - start_char)
                        .collect();
                    let this_idx = new_ranges.len();
                    new_ranges.push(LinkRange {
                        buffer_line,
                        start_char,
                        end_char,
                        url,
                    });

                    let is_hovered = hovered.0 == Some(this_idx);
                    if !is_hovered {
                        continue;
                    }

                    let s_byte = line.slice(..start_char).len_bytes();
                    let e_byte = line.slice(..end_char).len_bytes();

                    let (start_row, start_byte_in_row) = rv
                        .layout
                        .and_then(|l| l.buffer_to_display(buffer_line as u32, s_byte))
                        .unwrap_or_else(|| {
                            (rv.fold.actual_to_display_line(buffer_line) as u32, s_byte)
                        });
                    let (end_row, end_byte_in_row) = rv
                        .layout
                        .and_then(|l| l.buffer_to_display(buffer_line as u32, e_byte))
                        .unwrap_or_else(|| {
                            (rv.fold.actual_to_display_line(buffer_line) as u32, e_byte)
                        });
                    let start_x = rv
                        .layout
                        .and_then(|l| l.x_at_byte(start_row, start_byte_in_row))
                        .unwrap_or(start_char as f32 * m.char_width);
                    let end_x = if start_row == end_row {
                        rv.layout
                            .and_then(|l| {
                                l.x_after_source_range(end_row, start_byte_in_row, end_byte_in_row)
                            })
                            .unwrap_or(end_char as f32 * m.char_width)
                    } else {
                        rv.layout
                            .and_then(|l| l.x_at_byte(end_row, end_byte_in_row))
                            .unwrap_or(end_char as f32 * m.char_width)
                    };

                    let push =
                        |out: &mut Vec<RectOverlay>, row: u32, range: std::ops::Range<f32>| {
                            if ctrl_held {
                                out.push(underline_rect(row, range, theme.link));
                            } else {
                                push_dotted_underline(out, row, range, dim, m.char_width);
                            }
                        };

                    if start_row == end_row {
                        push(&mut new_rects, start_row, start_x..end_x);
                    } else {
                        let start_row_end = rv
                            .layout
                            .and_then(|l| {
                                l.lines
                                    .iter()
                                    .find(|line| line.display_row == start_row)
                                    .and_then(|line| l.x_at_byte(start_row, line.text.len()))
                            })
                            .unwrap_or(end_char as f32 * m.char_width);
                        push(&mut new_rects, start_row, start_x..start_row_end);
                        for r in (start_row + 1)..end_row {
                            push(&mut new_rects, r, 0.0..start_row_end);
                        }
                        push(&mut new_rects, end_row, 0.0..end_x);
                    }
                }
            }
        }

        if row.link_rects.0 != new_rects {
            row.link_rects.0 = new_rects;
        }
        if !range_lists_equal(&row.link_ranges.0, &new_ranges) {
            row.link_ranges.0 = new_ranges;
        }
    }
}

fn with_alpha(c: Color, a: f32) -> Color {
    let s = c.to_srgba();
    Color::srgba(s.red, s.green, s.blue, a)
}

/// Fake a dotted underline by emitting short `Underline` segments along
/// `range`. Each segment is ~⅓ of a character cell with an equal gap, so
/// the dash period scales naturally with the font size.
fn push_dotted_underline(
    out: &mut Vec<RectOverlay>,
    row: u32,
    range: std::ops::Range<f32>,
    color: Color,
    char_width: f32,
) {
    let dash = (char_width * 0.30).max(1.5);
    let gap = (char_width * 0.30).max(1.5);
    let mut x = range.start;
    while x < range.end {
        let seg_end = (x + dash).min(range.end);
        out.push(underline_rect(row, x..seg_end, color));
        x = seg_end + gap;
    }
}

fn range_lists_equal(a: &[LinkRange], b: &[LinkRange]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b.iter()).all(|(x, y)| {
            x.buffer_line == y.buffer_line
                && x.start_char == y.start_char
                && x.end_char == y.end_char
                && x.url == y.url
        })
}

fn underline_rect(display_row: u32, x_range: std::ops::Range<f32>, color: Color) -> RectOverlay {
    RectOverlay {
        display_row,
        x_range,
        vertical: RowVertical::Underline {
            thickness: 1.0,
            gap: 3.0,
        },
        color,
        z: 0,
        corners: CornerRadii::ZERO,
    }
}
