//! Gutter line-number rendering via a child `TextView` entity.
//!
//! Each `CodeEditor` entity gets one `GutterTextView` child spawned in
//! `setup_gutter_text_view` (Startup). `sync_gutter_text_view` runs in
//! PostUpdate before `LayoutProduceSet` and keeps the gutter in sync with
//! the editor: content, scroll, hidden lines, and per-line colors.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use bevy::text::{Justify, TextLayout};
use bevy::ui::ScrollPosition;
use bevy_instanced_text::{
    FormattedSpan, HiddenLines, LineStyles, MonoFontFaces, TextBuffer, TextFormat, TextSpan,
};
use bevy_instanced_text_editor::RopeBuffer;

use crate::settings::*;
use crate::types::*;

/// Spawns a `GutterTextView` child entity for each new `CodeEditor`.
pub(crate) fn setup_gutter_text_view(
    mut commands: Commands,
    editors: Query<
        (
            Entity,
            &TextFont,
            &MonoFontFaces,
            &EditorTheme,
            Option<&bevy::camera::visibility::RenderLayers>,
        ),
        With<CodeEditor>,
    >,
    existing: Query<&GutterTextView>,
) {
    for (editor_entity, font, faces, theme, render_layers) in editors.iter() {
        if existing.iter().any(|g| g.editor == editor_entity) {
            continue;
        }

        let mut gutter_cmds = commands.spawn((
            GutterTextView {
                editor: editor_entity,
            },
            TextBuffer::<TextSpan>::default(),
            font.clone(),
            faces.clone(),
            TextLayout {
                justify: Justify::Right,
                ..default()
            },
            bevy_instanced_text::TextColor(theme.line_numbers),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Px(0.0),
                height: Val::Percent(100.0),
                padding: UiRect::right(Val::Px(8.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            // Disable picking so the gutter doesn't swallow pointer events meant for the editor.
            bevy::picking::Pickable::IGNORE,
            Name::new("GutterTextView"),
        ));

        if let Some(layers) = render_layers {
            gutter_cmds.insert(layers.clone());
        }

        let gutter_id = gutter_cmds.id();
        commands.entity(editor_entity).add_child(gutter_id);
    }
}

/// Keeps each gutter `TextView` in sync with its editor.
///
/// Runs in PostUpdate before `LayoutProduceSet`.
pub(crate) fn sync_gutter_text_view(
    editor_query: Query<
        (
            Entity,
            &SelectionState,
            &TextBuffer<RopeBuffer>,
            &ScrollPosition,
            &GutterConfig,
            Ref<FoldState>,
            &EditorTheme,
            &EditorUi,
            &crate::settings::Padding,
            &crate::settings::RenderSettings,
            &crate::settings::Folding,
        ),
        (With<CodeEditor>, Without<GutterTextView>),
    >,
    mut gutter_query: Query<
        (
            &GutterTextView,
            &mut TextBuffer<TextSpan>,
            &mut ScrollPosition,
            &mut HiddenLines,
            &mut LineStyles,
            &mut Node,
            &mut Visibility,
            &mut bevy_instanced_text::TextColor,
        ),
        Without<CodeEditor>,
    >,
) {
    use crate::settings::LineNumbers as LineNumbersMode;
    use crate::settings::RenderFinalNewline;
    use crate::settings::ShowFoldingControls;
    for (
        editor_entity,
        sel,
        buffer,
        editor_scroll,
        gutter,
        fold_state,
        theme,
        ui,
        padding,
        render,
        folding,
    ) in editor_query.iter()
    {
        let Some((
            _,
            mut g_buffer,
            mut g_scroll,
            mut g_hidden,
            mut g_styles,
            mut g_node,
            mut g_vis,
            mut g_color,
        )) = gutter_query
            .iter_mut()
            .find(|(g, ..)| g.editor == editor_entity)
        else {
            continue;
        };

        let show_numbers = !matches!(ui.line_numbers, crate::settings::LineNumbers::Off);
        let target_vis = if show_numbers {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *g_vis != target_vis {
            *g_vis = target_vis;
        }

        if !show_numbers {
            continue;
        }

        let target_width = Val::Px(gutter.gutter_width);
        if g_node.width != target_width {
            g_node.width = target_width;
        }

        let target_top = Val::Px(padding.top);
        if g_node.padding.top != target_top {
            g_node.padding.top = target_top;
        }

        // Keep default color in sync with theme (theme might change at runtime).
        let default_color = bevy_instanced_text::TextColor(theme.line_numbers);
        if g_color.0 != default_color.0 {
            *g_color = default_color;
        }

        // Mirror the editor's scroll onto the gutter's. The gutter doesn't
        // animate independently — it tracks the editor's current position.
        if (g_scroll.y - editor_scroll.y).abs() > 1e-4 {
            g_scroll.y = editor_scroll.y;
        }

        let raw_line_count = buffer.len_lines();
        let trailing_empty = raw_line_count > 0
            && bevy_instanced_text::TextContent::line(&**buffer, raw_line_count - 1)
                .trim()
                .is_empty();
        let strip_trailing =
            trailing_empty && matches!(render.render_final_newline, RenderFinalNewline::Off);
        let line_count = if strip_trailing {
            raw_line_count.saturating_sub(1)
        } else {
            raw_line_count
        }
        .max(1);

        let cursor_line = sel
            .selections
            .primary()
            .head_offset()
            .min(buffer.len_chars());
        let cursor_line_idx = buffer.char_to_line(cursor_line);

        let mode = ui.line_numbers;
        let show_chevrons = matches!(folding.show_controls, ShowFoldingControls::Always);
        let old_count = if g_buffer.0 .0.is_empty() {
            0
        } else {
            bevy_instanced_text::TextContent::line_count(&g_buffer.0)
        };
        let count_stale = old_count != line_count;
        let needs_full_rebuild = count_stale
            || matches!(mode, LineNumbersMode::Relative)
            || (show_chevrons && fold_state.is_changed());

        if needs_full_rebuild {
            let mut text = String::with_capacity(line_count * 6);
            for i in 0..line_count {
                if i > 0 {
                    text.push('\n');
                }
                if show_chevrons {
                    let chevron = if fold_state.is_folded_line(i) {
                        "\u{25B6} "
                    } else if fold_state.is_foldable_line(i) {
                        "\u{25BC} "
                    } else {
                        "  "
                    };
                    text.push_str(chevron);
                }
                let label = match mode {
                    LineNumbersMode::Relative => {
                        if i == cursor_line_idx {
                            (i + 1).to_string()
                        } else {
                            (i as isize - cursor_line_idx as isize)
                                .unsigned_abs()
                                .to_string()
                        }
                    }
                    LineNumbersMode::Interval => {
                        let n = i + 1;
                        if n % 10 == 0 || i == cursor_line_idx {
                            n.to_string()
                        } else {
                            String::new()
                        }
                    }
                    _ => (i + 1).to_string(),
                };
                text.push_str(&label);
            }
            g_buffer.0 = TextSpan(text);
        }

        // Update hidden lines when fold state changes (independent of line count).
        if fold_state.is_changed() || count_stale {
            // Walk folded regions directly. The previous form scanned every
            // line and asked each region — O(line_count × regions), which
            // froze the editor for seconds on large files with many folds.
            let mut hidden: HashSet<usize> = HashSet::new();
            for region in &fold_state.regions {
                if !region.is_folded {
                    continue;
                }
                let start = region.start_line.saturating_add(1);
                let end = region.end_line.min(line_count.saturating_sub(1));
                for line in start..=end {
                    hidden.insert(line);
                }
            }
            *g_hidden = HiddenLines::new(hidden);
        }

        // Per-line styles: active line number color for cursor lines.
        // Only rewrite g_styles when the set of cursor lines changes — an
        // unconditional write creates a fresh Arc every frame, triggering
        // layout_miss_styles in the gutter's produce_layouts on every tick.
        let cursor_lines: HashSet<usize> = sel
            .selections
            .iter()
            .map(|s| {
                let pos = s.head_offset().min(buffer.len_chars());
                buffer.char_to_line(pos)
            })
            .collect();

        let current_active: HashSet<usize> = g_styles.by_line.keys().map(|&k| k as usize).collect();

        if cursor_lines != current_active
            || count_stale
            || (show_chevrons && fold_state.is_changed())
        {
            let active_color = theme.line_numbers_active;
            let mut by_line: HashMap<u32, Vec<FormattedSpan>> = HashMap::new();
            for &line in &cursor_lines {
                if line < line_count {
                    let mut payload = String::with_capacity(8);
                    if show_chevrons {
                        let chevron = if fold_state.is_folded_line(line) {
                            "\u{25B6} "
                        } else if fold_state.is_foldable_line(line) {
                            "\u{25BC} "
                        } else {
                            "  "
                        };
                        payload.push_str(chevron);
                    }
                    payload.push_str(&(line + 1).to_string());
                    let byte_len = payload.len();
                    by_line.insert(
                        line as u32,
                        vec![FormattedSpan {
                            text: payload,
                            format: TextFormat::fg(0..byte_len, active_color),
                        }],
                    );
                }
            }
            *g_styles = LineStyles::new(by_line);
        }
    }
}
