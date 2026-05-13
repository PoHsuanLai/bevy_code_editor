//! Gutter line-number rendering via a child `TextView` entity.
//!
//! Each `CodeEditor` entity gets one `GutterTextView` child spawned in
//! `setup_gutter_text_view` (Startup). `sync_gutter_text_view` runs in
//! PostUpdate before `LayoutProduceSet` and keeps the gutter in sync with
//! the editor: content, scroll, hidden lines, and per-line colors.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use bevy::text::Justify;
use bevy::ui::ScrollPosition;
use bevy_instanced_text::{
    view::snapshot::TextDecoration, HiddenLines, LineStyles, MonoFontFaces, RunWithText,
    SmoothScroll, StyleRun, TextBuffer, TextSpan,
};
use bevy_text_editor::RopeBuffer;

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
            Option<&bevy_camera::visibility::RenderLayers>,
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
            GutterTextView { editor: editor_entity },
            TextBuffer::<TextSpan>::default(),
            font.clone(),
            faces.clone(),
            bevy::text::TextLayout {
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
            &SmoothScroll,
            &GutterConfig,
            Ref<FoldState>,
            &EditorTheme,
            &EditorUi,
        ),
        (With<CodeEditor>, Without<GutterTextView>),
    >,
    mut gutter_query: Query<
        (
            &GutterTextView,
            &mut TextBuffer<TextSpan>,
            &mut ScrollPosition,
        &mut SmoothScroll,
            &mut HiddenLines,
            &mut LineStyles,
            &mut Node,
            &mut Visibility,
            &mut bevy_instanced_text::TextColor,
        ),
        Without<CodeEditor>,
    >,
) {
    for (editor_entity, sel, buffer, scroll_pos, _smooth, gutter, fold_state, theme, ui) in editor_query.iter() {
        let Some((
            _,
            mut g_buffer,
            mut g_scroll_pos,
            mut g_smooth,
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

        let target_vis = if ui.show_line_numbers {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *g_vis != target_vis {
            *g_vis = target_vis;
        }

        if !ui.show_line_numbers {
            continue;
        }

        let target_width = Val::Px(gutter.gutter_width);
        if g_node.width != target_width {
            g_node.width = target_width;
        }

        // Keep default color in sync with theme (theme might change at runtime).
        let default_color = bevy_instanced_text::TextColor(theme.line_numbers);
        if g_color.0 != default_color.0 {
            *g_color = default_color;
        }

        // Mirror vertical scroll.
        if (g_scroll_pos.y - scroll_pos.y).abs() > 1e-4 {
            g_scroll_pos.y = scroll_pos.y;
            g_smooth.target_y = scroll_pos.y;
        }

        // Rebuild content when line count or fold state changes.
        let line_count = buffer.len_lines();
        let content_stale = fold_state.is_changed()
            || g_buffer.0.0.is_empty()
            || bevy_instanced_text::TextContent::line_count(&g_buffer.0).saturating_sub(1) != line_count.saturating_sub(1);

        if content_stale {
            let mut text = String::with_capacity(line_count * 4);
            for i in 1..=line_count {
                if i > 1 {
                    text.push('\n');
                }
                text.push_str(&i.to_string());
            }
            g_buffer.0 = TextSpan(text);

            // Collect hidden lines using the fold API that works with and without tree-sitter.
            let hidden: HashSet<usize> = (0..line_count)
                .filter(|&l| fold_state.is_line_hidden(l))
                .collect();
            *g_hidden = HiddenLines::new(hidden);
        }

        // Per-line styles: active line number color for cursor lines.
        let cursor_lines: HashSet<usize> = sel
            .selections
            .iter()
            .map(|s| {
                let pos = s.head_offset().min(buffer.len_chars());
                buffer.char_to_line(pos)
            })
            .collect();

        let active_color = theme.line_numbers_active;
        let mut by_line: HashMap<u32, Vec<RunWithText>> = HashMap::new();
        for &line in &cursor_lines {
            if line < line_count {
                let num_str = (line + 1).to_string();
                let byte_len = num_str.len();
                by_line.insert(
                    line as u32,
                    vec![RunWithText {
                        text: num_str,
                        run: StyleRun {
                            byte_range: 0..byte_len,
                            fg: active_color,
                            bg: None,
                            font_scale: 0.0,
                            skew: 0.0,
                            corner_radius: 0.0,
                            font_weight: None,
                            italic: false,
                            font: None,
                            decoration: TextDecoration::empty(),
                            link: None,
                        },
                    }],
                );
            }
        }

        *g_styles = LineStyles::new(by_line);
    }
}
