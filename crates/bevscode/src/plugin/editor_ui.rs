//! Editor UI plugin for rendering editor visual elements

use bevy::prelude::*;
use bevy_instanced_text::{MonoCellWidth, TextOverlays, TextUnderlays};

use crate::settings::*;
use crate::types::{
    BracketMatchRects, CaretRects, CodeEditor, CursorLineRects, FoldHighlightRects,
    IndentGuideRects, RulerRects, SelectionRects, Separator, WhitespaceRects,
};

use super::gutter_decorations::{
    drive_chevron_rotation, setup_icon_atlas, sync_fold_chevron_icons, sync_gutter_decoration_bars,
    sync_gutter_icons, update_glyph_margin_overlays, update_line_decoration_overlays,
    GlyphMarginRects, LineDecorationRects,
};
use super::links::{update_link_overlays, LinkRects};
use super::{
    setup_gutter_text_view, sync_gutter_container, sync_gutter_text_font, sync_gutter_text_view,
    to_bevy_coords_left_aligned, update_cursor_line_highlight, update_fold_highlights,
    update_indent_guides, update_rulers, update_selection_highlight, update_whitespace_markers,
    EditorSetupSet,
};
use bevy_instanced_text::gpu::GlyphAtlas;

use super::{update_bracket_highlight, update_bracket_match};

/// Editor UI plugin: renders line numbers, separator, cursor, selection.
/// Added automatically by `CodeEditorPlugin`.
#[derive(Default)]
pub struct EditorUiPlugin;

impl Plugin for EditorUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_editor_ui.after(EditorSetupSet));
        // Gutter icons need bevy_resvg's pipeline, which in turn needs
        // `Assets<Image>` from `bevy::image::ImagePlugin`. Test apps that
        // omit ImagePlugin (e.g. `MinimalPlugins` setups) will skip icon
        // rendering — they'd panic on bevy_resvg's `Assets<Image>` access
        // otherwise.
        if app.is_plugin_added::<bevy::image::ImagePlugin>() {
            if !app.is_plugin_added::<bevy_resvg::plugin::SvgPlugin>() {
                app.add_plugins(bevy_resvg::plugin::SvgPlugin);
            }
            app.add_systems(PreStartup, setup_icon_atlas);
            app.add_systems(
                Update,
                (
                    sync_gutter_icons,
                    sync_gutter_decoration_bars,
                    sync_fold_chevron_icons,
                    drive_chevron_rotation.after(sync_fold_chevron_icons),
                )
                    .after(setup_gutter_text_view),
            );
        }

        // Gutter setup runs every frame because the editor's required
        // components may not all be present during Startup; idempotent
        // via its `existing` guard.
        app.add_systems(Update, setup_gutter_text_view);

        // AutoResizeViewport: keep Node::width/height in Val::Px sync with the window.
        // This runs every frame so window resizes are picked up automatically.
        app.add_systems(Update, sync_node_from_window);

        // Mirror bevscode's `Indentation` into the widget crate's `IndentConfig`
        // (which the typing/tab handlers read). `Indentation` is the
        // Monaco-shaped surface; `IndentConfig` is the underlying handler input.
        // Observer-driven so the schedule has zero per-frame cost — fires
        // only on the entity that just had `Indentation` inserted or replaced.
        app.add_observer(sync_indent_config_on_change);
        app.add_observer(disable_scroll_beyond_last_line);

        app.add_observer(detect_indentation_on_buffer_insert);
        app.add_observer(detect_indentation_on_first_edit);
        app.add_systems(Update, sync_cursor_icon);
        app.add_systems(Update, sync_automatic_layout);

        // Update separator position when the viewport resizes OR when
        // the resolved gutter width changes (digit-count grew, glyph
        // margin toggled, fold controls toggled, …).
        app.add_systems(
            Update,
            update_separator_on_resize.run_if(viewport_or_gutter_changed),
        );

        app.add_systems(
            PostUpdate,
            (
                resolve_gutter_layout,
                apply_editor_padding.after(resolve_gutter_layout),
                sync_gutter_container.after(resolve_gutter_layout),
                sync_gutter_text_font.after(resolve_gutter_layout),
                sync_gutter_text_view.after(sync_gutter_text_font),
            )
                .before(bevy_instanced_text::LayoutProduceSet),
        );

        app.add_systems(
            PostUpdate,
            update_font_metrics
                .run_if(bevy_instanced_text::gpu::atlas_ready)
                .in_set(super::RenderingSet),
        );

        app.add_systems(
            PostUpdate,
            (update_selection_highlight, update_cursor_line_highlight).in_set(super::RenderingSet),
        );

        app.add_systems(
            PostUpdate,
            (
                update_indent_guides,
                update_rulers,
                update_fold_highlights,
                update_link_overlays,
                update_whitespace_markers,
                update_glyph_margin_overlays,
                update_line_decoration_overlays,
            )
                .in_set(super::RenderingSet),
        );

        #[cfg(feature = "lsp")]
        app.add_systems(
            Update,
            super::gutter_decorations::sync_lsp_glyph_markers.in_set(super::ApplyStateSet),
        );

        #[cfg(feature = "lsp")]
        app.add_systems(
            PostUpdate,
            super::diagnostic_underlines::update_diagnostic_underlines.in_set(super::RenderingSet),
        );

        // State update stays in Update; overlay producer reads DisplayLayout so it runs in PostUpdate.
        app.add_systems(Update, update_bracket_match.in_set(super::ApplyStateSet));
        app.add_systems(
            PostUpdate,
            update_bracket_highlight
                .after(update_indent_guides)
                .in_set(super::RenderingSet),
        );

        app.add_systems(
            PostUpdate,
            merge_overlay_components
                .after(super::RenderingSet)
                .before(bevy_instanced_text::TextViewRenderSet),
        );
    }
}

use bevy_instanced_text::RectOverlay;

struct OverlaySources<'a> {
    sel: &'a [RectOverlay],
    guides: &'a [RectOverlay],
    rulers: &'a [RectOverlay],
    fold_hl: &'a [RectOverlay],
    cursor_line: &'a [RectOverlay],
    carets: &'a [RectOverlay],
    brackets: &'a [RectOverlay],
    links: &'a [RectOverlay],
    whitespace: &'a [RectOverlay],
    glyph_margin: &'a [RectOverlay],
    line_dec: &'a [RectOverlay],
}

fn assemble_overlays(
    src: &OverlaySources<'_>,
    underlays: &mut TextUnderlays,
    overlays: &mut TextOverlays,
) {
    underlays.0.clear();
    underlays.0.extend_from_slice(src.guides);
    underlays.0.extend_from_slice(src.rulers);
    underlays.0.extend_from_slice(src.fold_hl);
    underlays.0.extend_from_slice(src.sel);
    underlays.0.extend_from_slice(src.line_dec);

    overlays.0.clear();
    overlays.0.extend_from_slice(src.cursor_line);
    overlays.0.extend_from_slice(src.carets);
    overlays.0.extend_from_slice(src.brackets);
    overlays.0.extend_from_slice(src.links);
    overlays.0.extend_from_slice(src.whitespace);
    overlays.0.extend_from_slice(src.glyph_margin);
}

#[cfg(not(feature = "lsp"))]
#[allow(clippy::type_complexity)]
fn merge_overlay_components(
    mut query: Query<
        (
            &SelectionRects,
            &IndentGuideRects,
            &RulerRects,
            &FoldHighlightRects,
            &CursorLineRects,
            &CaretRects,
            &BracketMatchRects,
            &LinkRects,
            &WhitespaceRects,
            &GlyphMarginRects,
            &LineDecorationRects,
            &mut TextUnderlays,
            &mut TextOverlays,
        ),
        (
            With<CodeEditor>,
            Or<(
                Changed<SelectionRects>,
                Changed<IndentGuideRects>,
                Changed<RulerRects>,
                Changed<FoldHighlightRects>,
                Changed<CursorLineRects>,
                Changed<CaretRects>,
                Changed<BracketMatchRects>,
                Changed<LinkRects>,
                Changed<WhitespaceRects>,
                Changed<GlyphMarginRects>,
                Changed<LineDecorationRects>,
            )>,
        ),
    >,
) {
    for (
        sel, guides, rulers, fold_hl, cursor_line, carets, brackets, links, whitespace,
        glyph_margin, line_dec, mut underlays, mut overlays,
    ) in &mut query
    {
        assemble_overlays(
            &OverlaySources {
                sel: &sel.0, guides: &guides.0, rulers: &rulers.0, fold_hl: &fold_hl.0,
                cursor_line: &cursor_line.0, carets: &carets.0, brackets: &brackets.0,
                links: &links.0, whitespace: &whitespace.0, glyph_margin: &glyph_margin.0,
                line_dec: &line_dec.0,
            },
            &mut underlays, &mut overlays,
        );
    }
}

/// LSP-enabled variant: same as the base merger, plus
/// [`DiagnosticUnderlineRects`] folded into `TextOverlays`.
#[cfg(feature = "lsp")]
#[allow(clippy::type_complexity)]
fn merge_overlay_components(
    mut query: Query<
        (
            &SelectionRects,
            &IndentGuideRects,
            &RulerRects,
            &FoldHighlightRects,
            &CursorLineRects,
            &CaretRects,
            &BracketMatchRects,
            &LinkRects,
            &WhitespaceRects,
            &GlyphMarginRects,
            &LineDecorationRects,
            &super::diagnostic_underlines::DiagnosticUnderlineRects,
            &mut TextUnderlays,
            &mut TextOverlays,
        ),
        (
            With<CodeEditor>,
            Or<(
                Changed<SelectionRects>,
                Changed<IndentGuideRects>,
                Changed<RulerRects>,
                Changed<FoldHighlightRects>,
                Changed<CursorLineRects>,
                Changed<CaretRects>,
                Changed<BracketMatchRects>,
                Changed<LinkRects>,
                Changed<WhitespaceRects>,
                Changed<GlyphMarginRects>,
                Changed<LineDecorationRects>,
                Changed<super::diagnostic_underlines::DiagnosticUnderlineRects>,
            )>,
        ),
    >,
) {
    for (
        sel, guides, rulers, fold_hl, cursor_line, carets, brackets, links, whitespace,
        glyph_margin, line_dec, diag_underlines, mut underlays, mut overlays,
    ) in &mut query
    {
        assemble_overlays(
            &OverlaySources {
                sel: &sel.0, guides: &guides.0, rulers: &rulers.0, fold_hl: &fold_hl.0,
                cursor_line: &cursor_line.0, carets: &carets.0, brackets: &brackets.0,
                links: &links.0, whitespace: &whitespace.0, glyph_margin: &glyph_margin.0,
                line_dec: &line_dec.0,
            },
            &mut underlays, &mut overlays,
        );
        overlays.0.extend_from_slice(&diag_underlines.0);
    }
}

/// Opt-in marker: editors with this component have their `Node` automatically
/// sized to the primary window via a full-screen `Val::Percent(100.0)` node.
/// Hosts that manage layout themselves (multi-pane, render-to-texture) omit this.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct AutoResizeViewport;

/// Keep `Node` pixel size in sync with the primary window for `AutoResizeViewport` editors.
/// Val::Px is used (not Val::Percent) so Bevy UI layout can resolve the size without
/// needing a UI camera to compute percentages against.
/// Set the editor's `Node` to fill 100% of the primary window viewport.
/// Writes `Val::Vw(100)` / `Val::Vh(100)` once when the marker is first
/// observed — Bevy's layout pass resolves viewport-relative units against
/// the live window on every resize, so no explicit resize listener is
/// needed.
fn sync_node_from_window(
    mut editors: Query<&mut Node, (With<CodeEditor>, With<AutoResizeViewport>)>,
) {
    let target_w = Val::Vw(100.0);
    let target_h = Val::Vh(100.0);
    for mut node in editors.iter_mut() {
        if node.width != target_w {
            node.width = target_w;
        }
        if node.height != target_h {
            node.height = target_h;
        }
    }
}

/// Mirror `Indentation` (Monaco-shaped surface in bevscode) into
/// `IndentConfig` (the widget-layer Component the Tab / typing handlers
/// read). Fires on insert *and* on replace of `Indentation`, so the
/// initial `#[require]` cascade and any later mutation both trip it.
/// Default `ScrollConfig::scroll_beyond_last_line` to `false` for code
/// editors. The engine-side default mirrors Monaco (`true`) which is
/// "park the last line at the top of the viewport" — fine in Monaco
/// where a scrollbar tells you you've gone past the end, but in a
/// scrollbar-less embed it just looks like the file scrolled into
/// nothing. Hosts that want the Monaco behavior can re-enable on the
/// `ScrollConfig` Component after spawn.
///
/// Fires on `CodeEditor` insertion (rather than on `ScrollConfig`
/// insertion) so the cascade order doesn't matter — by the time
/// `CodeEditor` is in place, every Component it requires (including
/// `ScrollConfig` via the `TextEditor` chain) is present.
fn disable_scroll_beyond_last_line(
    trigger: On<bevy::ecs::lifecycle::Insert, CodeEditor>,
    mut editors: Query<&mut bevy_instanced_text_editor::ScrollConfig>,
) {
    let Ok(mut cfg) = editors.get_mut(trigger.event().entity) else {
        return;
    };
    if cfg.scroll_beyond_last_line {
        cfg.scroll_beyond_last_line = false;
    }
}

fn sync_indent_config_on_change(
    trigger: On<bevy::ecs::lifecycle::Insert, crate::settings::Indentation>,
    mut editors: Query<
        (
            &crate::settings::Indentation,
            &mut bevy_instanced_text_editor::IndentConfig,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((indent, mut cfg)) = editors.get_mut(trigger.event().entity) else {
        return;
    };
    let next_tab = indent.indent_size.resolve(indent.tab_size);
    if cfg.tab_width != next_tab {
        cfg.tab_width = next_tab;
    }
    if cfg.use_spaces != indent.insert_spaces {
        cfg.use_spaces = indent.insert_spaces;
    }
    if cfg.use_tab_stops != indent.use_tab_stops {
        cfg.use_tab_stops = indent.use_tab_stops;
    }
    if cfg.sticky_tab_stops != indent.sticky_tab_stops {
        cfg.sticky_tab_stops = indent.sticky_tab_stops;
    }
    if cfg.trim_whitespace_on_delete != indent.trim_whitespace_on_delete {
        cfg.trim_whitespace_on_delete = indent.trim_whitespace_on_delete;
    }
}

/// Marker preventing repeat detection on the same editor after the first run.
#[derive(Component, Default)]
struct DetectIndentationDone;

/// Spawn-with-content path: when an editor's `TextBuffer` is inserted with
/// non-empty content, run detection immediately. The `#[require]`-cascaded
/// empty buffer hits this with len=0 and is no-op'd; hosts that spawn
/// `(CodeEditor, TextBuffer::from(content))` get detection at spawn time.
fn detect_indentation_on_buffer_insert(
    trigger: On<
        bevy::ecs::lifecycle::Insert,
        bevy_instanced_text::TextBuffer<bevy_instanced_text_editor::RopeBuffer>,
    >,
    commands: Commands,
    editors: Query<
        (
            &bevy_instanced_text::TextBuffer<bevy_instanced_text_editor::RopeBuffer>,
            &mut crate::settings::Indentation,
        ),
        (With<CodeEditor>, Without<DetectIndentationDone>),
    >,
) {
    run_detect_indentation(trigger.event().entity, commands, editors);
}

/// Post-spawn path: hosts that mutate the buffer after spawn (file open via
/// `set_text`, paste-from-clipboard onto an empty editor) trigger detection
/// on the first emitted `OnEdit`.
fn detect_indentation_on_first_edit(
    trigger: On<bevy_instanced_text_editor::OnEdit>,
    commands: Commands,
    editors: Query<
        (
            &bevy_instanced_text::TextBuffer<bevy_instanced_text_editor::RopeBuffer>,
            &mut crate::settings::Indentation,
        ),
        (With<CodeEditor>, Without<DetectIndentationDone>),
    >,
) {
    run_detect_indentation(trigger.event().entity, commands, editors);
}

fn run_detect_indentation(
    entity: Entity,
    mut commands: Commands,
    mut editors: Query<
        (
            &bevy_instanced_text::TextBuffer<bevy_instanced_text_editor::RopeBuffer>,
            &mut crate::settings::Indentation,
        ),
        (With<CodeEditor>, Without<DetectIndentationDone>),
    >,
) {
    let Ok((buffer, mut indent)) = editors.get_mut(entity) else {
        return;
    };
    if buffer.len_chars() == 0 {
        return;
    }
    commands.entity(entity).insert(DetectIndentationDone);
    if !indent.detect_indentation {
        return;
    }

    let mut tab_count = 0usize;
    let mut space_widths: std::collections::HashMap<u32, usize> = Default::default();
    let line_count = buffer.len_lines().min(100);
    for line_idx in 0..line_count {
        let line = buffer.line(line_idx);
        let mut spaces = 0u32;
        let mut starts_with_tab = false;
        for c in line.chars() {
            match c {
                '\t' => {
                    starts_with_tab = true;
                    break;
                }
                ' ' => spaces += 1,
                _ => break,
            }
        }
        if starts_with_tab {
            tab_count += 1;
        } else if spaces > 0 {
            *space_widths.entry(spaces).or_insert(0) += 1;
        }
    }

    let total_space_runs: usize = space_widths.values().copied().sum();
    if tab_count > total_space_runs {
        indent.insert_spaces = false;
    } else if total_space_runs > 0 {
        indent.insert_spaces = true;
        let common = [2u32, 4, 8]
            .into_iter()
            .max_by_key(|w| {
                space_widths
                    .iter()
                    .filter(|(s, _)| *s % w == 0)
                    .map(|(_, c)| *c)
                    .sum::<usize>()
            })
            .unwrap_or(4);
        indent.tab_size = common;
    }
}

/// Toggle [`AutoResizeViewport`] on every `CodeEditor` based on
/// `Misc::automatic_layout`.
fn sync_automatic_layout(
    mut commands: Commands,
    editors: Query<
        (Entity, &crate::settings::Misc, Has<AutoResizeViewport>),
        (With<CodeEditor>, Changed<crate::settings::Misc>),
    >,
) {
    for (entity, misc, has_marker) in editors.iter() {
        match (misc.automatic_layout, has_marker) {
            (true, false) => {
                commands.entity(entity).insert(AutoResizeViewport);
            }
            (false, true) => {
                commands.entity(entity).remove::<AutoResizeViewport>();
            }
            _ => {}
        }
    }
}

/// Mirror the focused editor's `Misc::mouse_style` to the primary window's
/// [`CursorIcon`]. The OS arrow shows over the gutter; `mouse_style` (the
/// text cursor, by default) shows over the text area. Unfocused editors
/// don't drive the OS cursor.
fn sync_cursor_icon(
    mut commands: Commands,
    input_focus: Res<bevy::input_focus::InputFocus>,
    editors: Query<
        (
            &crate::settings::Misc,
            &crate::types::HoveredInGutter,
            &crate::plugin::HoveredLink,
        ),
        With<CodeEditor>,
    >,
    windows: Query<(Entity, Option<&bevy::window::CursorIcon>), With<bevy::window::PrimaryWindow>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((misc, in_gutter, hovered_link)) = editors.get(entity) else {
        return;
    };
    let Ok((window_entity, current)) = windows.single() else {
        return;
    };
    let ctrl_held = keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight)
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight);
    let target = if in_gutter.0 {
        bevy::window::SystemCursorIcon::Default
    } else if misc.links && hovered_link.0.is_some() && ctrl_held {
        bevy::window::SystemCursorIcon::Pointer
    } else {
        match misc.mouse_style {
            crate::settings::MouseStyle::Text => bevy::window::SystemCursorIcon::Text,
            crate::settings::MouseStyle::Default => bevy::window::SystemCursorIcon::Default,
            crate::settings::MouseStyle::Copy => bevy::window::SystemCursorIcon::Copy,
        }
    };
    let next = bevy::window::CursorIcon::System(target);
    if current.map(|c| *c != next).unwrap_or(true) {
        commands.entity(window_entity).insert(next);
    }
}

/// Resolve every gutter column's width and left-edge for each editor
/// in one top-down pass, then publish the result as a `GutterConfig`.
///
/// Monaco parity (`src/vs/editor/common/config/editorOptions.ts`,
/// `EditorLayoutInfoComputer.computeLayout`):
///   `[ pad_l | glyph | numbers | decorations(chevron|bar) | pad_r ]`
///
/// The left chain is built strictly left-to-right — a width change in
/// any upstream band cascades to every downstream `left` in the same
/// frame, with no per-decoration recomputation downstream.
///
/// Runs every frame (not `Changed`-filtered) so async `char_width`
/// updates from `update_font_metrics` land immediately.
fn resolve_gutter_layout(
    mut editors: Query<
        (
            &mut GutterConfig,
            &MonoCellWidth,
            &TextFont,
            &bevy::text::LineHeight,
            &EditorUi,
            &crate::settings::Folding,
            &bevy_instanced_text::TextBuffer<bevy_instanced_text_editor::RopeBuffer>,
        ),
        With<CodeEditor>,
    >,
) {
    for (mut cfg, mono, font, line_height, ui, folding, buffer) in editors.iter_mut() {
        let show_numbers = !matches!(ui.line_numbers, crate::settings::LineNumbers::Off);
        let folding_enabled = matches!(
            folding.show_controls,
            crate::settings::ShowFoldingControls::Always
                | crate::settings::ShowFoldingControls::Mouseover
        );
        let line_count = buffer.len_lines().max(1) as u32;
        let digit_count = digit_count_for(line_count);
        let min_chars = digit_count.max(ui.line_numbers_min_chars).max(1) as f32;
        let line_height_px = bevy_instanced_text::resolve_line_height(*line_height, font.font_size);

        // 1) Widths — each band independent, then a chevron-bearing
        //    decorations band combining the chevron sub-column and the
        //    user-configured bar width.
        let glyph_w = if ui.glyph_margin {
            // Monaco sizes the glyph margin to line-height; `ui.glyph_margin_width`
            // is a floor so small text never squashes icons below readability.
            line_height_px.max(ui.glyph_margin_width)
        } else {
            0.0
        };
        let numbers_w = if show_numbers {
            mono.px * min_chars
        } else {
            0.0
        };
        // Chevron column scales with line-height (Monaco does the same)
        // with a 16 px floor for tiny font sizes.
        let chevron_w = if folding_enabled {
            line_height_px.max(16.0)
        } else {
            0.0
        };
        let bar_w = ui.line_decorations_width.max(0.0);
        let decorations_w = chevron_w + bar_w;
        let bands_w = glyph_w + numbers_w + decorations_w;
        let gutter_width = if bands_w > 0.0 {
            ui.gutter_padding_left + ui.gutter_padding_right + bands_w
        } else {
            0.0
        };

        // 2) Left chain — single pass, every band starts where the
        //    previous ended.
        let mut x = ui.gutter_padding_left;
        let glyph = GutterBand {
            left: x,
            width: glyph_w,
        };
        x += glyph_w;
        let numbers = GutterBand {
            left: x,
            width: numbers_w,
        };
        x += numbers_w;
        let decorations = GutterBand {
            left: x,
            width: decorations_w,
        };
        let chevron = GutterBand {
            left: x,
            width: chevron_w,
        };
        x += chevron_w;
        let bar = GutterBand {
            left: x,
            width: bar_w,
        };

        let next = GutterConfig {
            gutter_width,
            editor_padding_left: gutter_width + ui.code_margin_left,
            line_height_px,
            glyph,
            numbers,
            decorations,
            chevron,
            bar,
        };
        // PartialEq is exact, but every input is rounded or clamped
        // before arriving here (font-size px, mono.px which is itself
        // measured + dirty-thresholded). Avoids the change-detection
        // flicker of the old per-field epsilon comparison.
        if *cfg != next {
            *cfg = next;
        }
    }
}

/// Mirror `GutterConfig::editor_padding_left` onto the editor's
/// `Node::padding`, along with the host's vertical padding. Splitting
/// this out of `resolve_gutter_layout` is what stops `Node.padding.left`
/// and `GutterContainer.width` from drifting — both now derive from the
/// same `GutterConfig`.
fn apply_editor_padding(
    mut editors: Query<
        (&mut Node, &GutterConfig, &crate::settings::Padding),
        (
            With<CodeEditor>,
            Or<(Changed<GutterConfig>, Changed<crate::settings::Padding>)>,
        ),
    >,
) {
    for (mut node, cfg, padding) in editors.iter_mut() {
        let l = Val::Px(cfg.editor_padding_left);
        let t = Val::Px(padding.top);
        let b = Val::Px(padding.bottom);
        if node.padding.left != l {
            node.padding.left = l;
        }
        if node.padding.top != t {
            node.padding.top = t;
        }
        if node.padding.bottom != b {
            node.padding.bottom = b;
        }
    }
}

/// Setup UI entities (separator) for each `CodeEditor`.
fn setup_editor_ui(
    mut commands: Commands,
    editor_query: Query<
        (
            &ComputedNode,
            &GutterConfig,
            &EditorTheme,
            &EditorUi,
            Option<&bevy::camera::visibility::RenderLayers>,
        ),
        With<CodeEditor>,
    >,
) {
    for (computed, gutter, theme, ui, render_layers) in editor_query.iter() {
        let inv = computed.inverse_scale_factor();
        let logical = computed.size() * inv;
        let viewport_width = logical.x;
        let viewport_height = logical.y;

        if ui.show_separator {
            let mut cmds = commands.spawn((
                Sprite {
                    color: theme.separator,
                    custom_size: Some(Vec2::new(1.0, viewport_height)),
                    ..default()
                },
                Transform::from_translation(to_bevy_coords_left_aligned(
                    gutter.gutter_width,
                    viewport_height / 2.0,
                    viewport_width,
                    viewport_height,
                    0.0,
                )),
                Separator,
                Name::new("Separator"),
            ));
            if let Some(layers) = render_layers {
                cmds.insert(layers.clone());
            }
        }
    }
}

fn viewport_or_gutter_changed(
    query: Query<
        (),
        (
            With<CodeEditor>,
            Or<(Changed<ComputedNode>, Changed<GutterConfig>)>,
        ),
    >,
) -> bool {
    !query.is_empty()
}

fn update_separator_on_resize(
    viewport_query: Query<(&ComputedNode, &GutterConfig), With<CodeEditor>>,
    mut separator_query: Query<(&mut Sprite, &mut Transform), With<Separator>>,
) {
    let Some((computed, gutter)) = viewport_query.iter().next() else {
        return;
    };

    let inv = computed.inverse_scale_factor();
    let logical = computed.size() * inv;
    let viewport_width = logical.x;
    let viewport_height = logical.y;

    for (mut sprite, mut transform) in separator_query.iter_mut() {
        sprite.custom_size = Some(Vec2::new(1.0, viewport_height));
        transform.translation = to_bevy_coords_left_aligned(
            gutter.gutter_width,
            viewport_height / 2.0,
            viewport_width,
            viewport_height,
            0.0,
        );
    }
}

fn update_font_metrics(
    mut editors: Query<(&TextFont, &mut MonoCellWidth), With<CodeEditor>>,
    mut atlas: ResMut<GlyphAtlas>,
    fonts: Res<Assets<bevy::text::Font>>,
) {
    for (font, mut mono) in editors.iter_mut() {
        let font_id = atlas.ensure_font(&font.font, &fonts);
        let width = atlas.shape_line("0", font.font_size, font_id).width;
        if width > 0.0 && (mono.px - width).abs() > 0.01 {
            info!(
                "Updating char_width from {:.3} to {:.3} (measured)",
                mono.px, width
            );
            mono.px = width;
        }
    }
}

/// Number of decimal digits required to render `n`. Used to size the
/// line-numbers band dynamically — Monaco does the same so small files
/// don't reserve column space for line numbers they'll never reach.
fn digit_count_for(n: u32) -> u32 {
    let mut digits = 1;
    let mut v = n;
    while v >= 10 {
        v /= 10;
        digits += 1;
    }
    digits
}
