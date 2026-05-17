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
    for (editor_entity, sel, buffer, editor_scroll, gutter, fold_state, theme, ui, padding) in
        editor_query.iter()
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

        // Ropey counts a phantom empty line after a trailing '\n'; subtract it
        // so the gutter shows exactly as many numbers as there are real lines.
        let raw_line_count = buffer.len_lines();
        let line_count = if raw_line_count > 0
            && bevy_instanced_text::TextContent::line(&**buffer, raw_line_count - 1)
                .trim()
                .is_empty()
        {
            raw_line_count - 1
        } else {
            raw_line_count
        }
        .max(1);

        // Update line-number string when line count changes (never on fold-only changes).
        let old_count = if g_buffer.0 .0.is_empty() {
            0
        } else {
            bevy_instanced_text::TextContent::line_count(&g_buffer.0)
        };
        let count_stale = old_count != line_count;

        if count_stale {
            if old_count > 0 && line_count > old_count {
                // Lines added: append the new numbers.
                let s = &mut g_buffer.0 .0;
                for i in (old_count + 1)..=line_count {
                    s.push('\n');
                    s.push_str(&i.to_string());
                }
            } else if old_count > line_count && line_count > 0 {
                // Lines removed: find the byte offset of line `line_count` and truncate.
                // Scan forward to find the Nth '\n' rather than repeated rfind.
                let s = &mut g_buffer.0 .0;
                let cut = s
                    .char_indices()
                    .filter(|&(_, c)| c == '\n')
                    .nth(line_count - 1)
                    .map(|(i, _)| i)
                    .unwrap_or(s.len());
                s.truncate(cut);
            } else {
                // Initial population or edge case.
                let mut text = String::with_capacity(line_count * 4);
                for i in 1..=line_count {
                    if i > 1 {
                        text.push('\n');
                    }
                    text.push_str(&i.to_string());
                }
                g_buffer.0 = TextSpan(text);
            }
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

        if cursor_lines != current_active || count_stale {
            let active_color = theme.line_numbers_active;
            let mut by_line: HashMap<u32, Vec<FormattedSpan>> = HashMap::new();
            for &line in &cursor_lines {
                if line < line_count {
                    let num_str = (line + 1).to_string();
                    let byte_len = num_str.len();
                    by_line.insert(
                        line as u32,
                        vec![FormattedSpan {
                            text: num_str,
                            format: TextFormat::fg(0..byte_len, active_color),
                        }],
                    );
                }
            }
            *g_styles = LineStyles::new(by_line);
        }
    }
}
