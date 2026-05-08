//! LSP (Language Server Protocol) integration example with egui+armas overlays.
//!
//! Demonstrates how a host application can render the editor's LSP popup
//! data-components (completion, hover, signature help, code actions, rename)
//! through `bevy_egui` styled by `armas`. The editor crate itself does not
//! depend on egui/armas — the renderer wiring lives entirely here in the
//! example so applications can copy/adapt it.
//!
//! The data-components produced by `bevy_code_editor::plugin::LspPlugin` are
//! the host-renderer-agnostic API; this example reads `bevy_lsp` state through
//! the editor's `lsp_ui` re-exports and turns it into egui frames each frame.
//!
//! Run: `cargo run --example lsp --features lsp`. Requires `rust-analyzer` on
//! `PATH` (`rustup component add rust-analyzer`).

use armas::prelude::*;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy_code_editor::lsp_ui::components::{
    DocumentHighlightData, InlayHintData, InlayHintKind, LspUiVisual,
};
use bevy_code_editor::lsp_ui::state::{
    LspCodeActionsPopup, LspCompletionPopup, LspHoverPopup, LspRenamePopup, LspSignatureHelpPopup,
};
use bevy_code_editor::prelude::*;
use bevy_text_engine::FontConfig;
use bevy_code_editor::text_view::TextViewViewport;
use bevy_code_editor::types::{CodeEditor, CursorState};
use bevy_egui::{egui, EguiContexts};
use bevy_lsp::{LspClient, LspDocument, LspMessage};
use bevy_markdown::{MarkdownDoc, MarkdownLinks, MarkdownViewerPlugin};
use bevy_text_engine::{ContentMetrics, DisplayLayout, ScrollState, TextBuffer, TextView};
#[cfg(feature = "tree-sitter")]
use bevy_tree_sitter::Language;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "LSP Integration Example (egui+armas)".to_string(),
            resolution: (1200, 800).into(),
            ..default()
        }),
        ..default()
    }));

    // EguiPlugin must be added BEFORE editor plugins so contexts are available
    // to the egui render systems below.
    app.add_plugins(bevy_egui::EguiPlugin::default());

    app.add_plugins(CodeEditorPlugins);

    // LspPlugin is part of `CodeEditorPlugins` under the `lsp` feature; we
    // just supply the egui-based UI layer here.
    app.add_plugins(LspEguiUiPlugin);

    // Markdown rendering for LSP hover popups: when a server returns
    // Markdown content, `render_hover_markdown` spawns a child TextView
    // entity that the markdown plugin lays out as styled text.
    app.add_plugins(MarkdownViewerPlugin);

    app.add_systems(Startup, setup_camera)
        .add_systems(PostStartup, setup_editor)
        .add_systems(Update, (display_lsp_info, auto_request_completion))
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(ThemeConfig::default().background),
            ..default()
        },
    ));
}

/// LSP UI plugin using egui/armas overlays for popup rendering, plus inline
/// sprite decorations for inlay hints and document highlights.
///
/// The library crate ships only the data-component side of LSP UI (sync +
/// event listeners); rendering is the host's responsibility. This plugin is
/// a self-contained reference renderer: egui for interactive popups,
/// sprite-based for inline decorations on the editor surface.
struct LspEguiUiPlugin;

impl Plugin for LspEguiUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LspEguiViewportOffset>();
        app.insert_resource(InlineDecorationsTheme::default());

        app.add_systems(
            Update,
            (
                render_completion_egui,
                render_hover_egui,
                render_hover_markdown,
                render_signature_help_egui,
                render_code_actions_egui,
                render_rename_egui,
                render_inlay_hints,
                // Document highlights push `RectOverlay` into the editor's
                // `TextViewOverlays`, which the engine then paints in the
                // same draw call as glyphs. Must run before
                // `TextViewRenderSet` (which consumes overlays) — same
                // ordering requirement as selection / cursor.
                render_document_highlights.before(bevy_text_engine::TextViewRenderSet),
            )
                .in_set(LspUiRenderSet),
        );
    }
}

/// Local SystemSet for grouping the example's render systems. Replaces the
/// (now-removed) library `LspUiRenderSet`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct LspUiRenderSet;

/// Marker for inlay hint visual entities (sprite path).
#[derive(Component)]
struct InlayHintText {
    #[allow(dead_code)]
    line: u32,
    #[allow(dead_code)]
    character: u32,
}

/// Theme for the sprite-rendered inline decorations (inlay hints, document
/// highlights). The library no longer ships a theme — this is the example's
/// local copy of the relevant subset of the old `LspUiTheme`.
#[derive(Resource, Clone, Debug)]
struct InlineDecorationsTheme {
    inlay_hints: InlayHintsTheme,
    document_highlights: DocumentHighlightsTheme,
}

impl Default for InlineDecorationsTheme {
    fn default() -> Self {
        Self {
            inlay_hints: InlayHintsTheme::default(),
            document_highlights: DocumentHighlightsTheme::default(),
        }
    }
}

#[derive(Clone, Debug)]
struct InlayHintsTheme {
    type_color: Color,
    parameter_color: Color,
    default_color: Color,
    font_size_multiplier: f32,
    z_index: f32,
}

impl Default for InlayHintsTheme {
    fn default() -> Self {
        Self {
            type_color: Color::srgba(0.5, 0.7, 0.9, 0.7),
            parameter_color: Color::srgba(0.7, 0.6, 0.9, 0.7),
            default_color: Color::srgba(0.6, 0.6, 0.6, 0.7),
            font_size_multiplier: 0.85,
            z_index: 50.0,
        }
    }
}

#[derive(Clone, Debug)]
struct DocumentHighlightsTheme {
    read_color: Color,
    write_color: Color,
}

impl Default for DocumentHighlightsTheme {
    fn default() -> Self {
        Self {
            read_color: Color::srgba(0.5, 0.6, 0.8, 0.25),
            write_color: Color::srgba(0.8, 0.5, 0.3, 0.3),
        }
    }
}

/// Render inlay hints from marker component data (inlined from the
/// library's old `lsp_ui::render::render_inlay_hints`).
///
/// Routes through `RowMetricsParam` so the hint sits on the same
/// baseline the engine paints the surrounding code at; LSP-side
/// `sync.rs` publishes `(line, character)` and we re-derive the world
/// position here, ignoring the legacy `position` field.
fn render_inlay_hints(
    mut commands: Commands,
    // No `Added` filter: scrolling/resize must reposition the hint
    // even when the data is unchanged. See `render_document_highlights`
    // for the same pattern.
    hint_query: Query<(Entity, &InlayHintData)>,
    editors: Query<(Entity, &FontConfig), With<CodeEditor>>,
    metrics: bevy_text_engine::RowMetricsParam,
    theme: Res<InlineDecorationsTheme>,
) {
    let Ok((editor_entity, font)) = editors.single() else {
        return;
    };
    let m = metrics.get_or_panic(editor_entity);

    for (entity, hint) in hint_query.iter() {
        let color = match hint.kind {
            InlayHintKind::Type => theme.inlay_hints.type_color,
            InlayHintKind::Parameter => theme.inlay_hints.parameter_color,
            InlayHintKind::Other => theme.inlay_hints.default_color,
        };

        // Anchor on the row's glyph band middle so the hint sits where
        // the surrounding text reads — `Anchor::CENTER_LEFT` then makes
        // the hint extend rightward from this point.
        let band = m.row_glyph_band(hint.line);
        let cell_left = m
            .cell_world_pos_at_x(hint.line, hint.character as f32 * font.char_width)
            .x;
        let pos = Vec3::new(
            cell_left,
            (band.min.y + band.max.y) * 0.5,
            theme.inlay_hints.z_index,
        );

        let Ok(mut entity_cmd) = commands.get_entity(entity) else {
            continue;
        };
        entity_cmd.queue_silenced(bevy::ecs::system::entity_command::insert(
            (
                Text2d::new(&hint.label),
                TextFont {
                    font: font.font.clone(),
                    font_size: font.font_size * theme.inlay_hints.font_size_multiplier,
                    ..default()
                },
                TextColor(color),
                Transform::from_translation(pos),
                Anchor::CENTER_LEFT,
                InlayHintText {
                    line: hint.line,
                    character: hint.character,
                },
                LspUiVisual,
            ),
            bevy::ecs::bundle::InsertMode::Replace,
        ));
    }
}

/// Render document highlights as `RectOverlay`s on the editor's
/// `TextViewOverlays`.
///
/// Going through the engine's overlay path (rather than spawning Bevy
/// `Sprite`s) means the highlight quads ship in the **same draw call
/// and same instance buffer as the glyphs**, sharing the engine's
/// row-anchor convention by definition. That's why selections never
/// drift: they take the same path. Sprites, by contrast, are rendered
/// by Bevy's stock 2D pipeline and can land on slightly different
/// pixels under the engine's clip-projection convention — which was
/// the "half row below" symptom in the previous renderer.
///
/// Re-runs every frame: drains the previous frame's highlight rects
/// (identified by their dedicated `z = -2` slot, which sits below
/// selection at `z = -1`) and pushes a fresh batch. `(line,
/// start_character, end_character)` come from semantic
/// `DocumentHighlightData` and resolve to display rows via
/// `DisplayLayout::buffer_to_display`, so soft-wrapped lines and
/// folded regions are handled by the engine's own coordinate system.
fn render_document_highlights(
    highlight_query: Query<&DocumentHighlightData>,
    mut editors: Query<
        (&FontConfig, &DisplayLayout, &mut bevy_text_engine::TextViewOverlays),
        With<CodeEditor>,
    >,
    theme: Res<InlineDecorationsTheme>,
) {
    let Ok((font, layout, mut overlays)) = editors.single_mut() else {
        return;
    };

    // Drain previous-frame highlight rects. `z = -2` is reserved for
    // document highlights so this drain doesn't touch selections (`-1`),
    // line-bg/cursor decorations (`0`), or carets (`+1`).
    let had_highlights = overlays.rects.iter().any(|r| r.z == -2);
    overlays.rects.retain(|r| r.z != -2);

    let mut pushed = 0usize;
    for highlight in highlight_query.iter() {
        let color = if highlight.is_write {
            theme.document_highlights.write_color
        } else {
            theme.document_highlights.read_color
        };

        // LSP gives `(line, character)` in buffer coords. Translate to
        // display coords through the engine's authoritative map; this
        // honors soft-wrap and hidden (folded) lines automatically.
        // ASCII-only character→byte for now; non-ASCII LSP highlights
        // would need bevy_lsp's UTF-16 conversion.
        let buffer_row = highlight.line;
        let start_byte = highlight.start_character as usize;
        let Some((display_row, start_byte_in_row)) =
            layout.buffer_to_display(buffer_row, start_byte)
        else {
            continue;
        };

        let start_x = layout
            .x_at_byte(display_row, start_byte_in_row)
            .unwrap_or(highlight.start_character as f32 * font.char_width);

        // Resolve the end x. `u32::MAX` (multi-line range continuation)
        // means "to end of line" — use the row's measured text width.
        let end_x = if highlight.end_character == u32::MAX {
            // ShapedLine has the row's pixel-width; reach for it through
            // the layout iterator since `DisplayLayout` doesn't expose a
            // direct accessor.
            layout
                .lines
                .iter()
                .find(|l| l.display_row == display_row)
                .and_then(|l| layout.x_at_byte(display_row, l.text.len()))
                .unwrap_or_else(|| {
                    start_x
                        + highlight
                            .end_character
                            .saturating_sub(highlight.start_character)
                            .min(200) as f32
                            * font.char_width
                })
        } else {
            let end_byte = highlight.end_character as usize;
            // Same row — end byte must also resolve through `buffer_to_display`,
            // but for single-row highlights the display row is the same.
            layout
                .x_at_byte(display_row, end_byte.saturating_sub(start_byte) + start_byte_in_row)
                .unwrap_or(start_x + (highlight.end_character - highlight.start_character) as f32 * font.char_width)
        };

        if end_x <= start_x {
            continue;
        }

        overlays.rects.push(bevy_text_engine::RectOverlay {
            display_row,
            x_range: start_x..end_x,
            vertical: bevy_text_engine::RowVertical::Full,
            color,
            z: -2,
            corners: bevy_text_engine::CornerRadii::ZERO,
        });
        pushed += 1;
    }

    // Bump the version only when something actually changed, so the
    // engine can skip the GPU re-upload otherwise.
    if pushed > 0 || had_highlights {
        overlays.version = overlays.version.wrapping_add(1);
    }
}

/// Screen-space offset for positioning egui overlays relative to the editor panel.
///
/// The consuming app must update this each frame with the editor panel's screen
/// position. If the editor fills the whole window, this stays at (0, 0).
#[derive(Resource, Default, Debug, Clone)]
struct LspEguiViewportOffset {
    /// Top-left corner of the editor panel in screen pixels.
    screen_offset: Vec2,
}

/// Cursor screen position (x, y at top of cursor line) for an egui
/// popup anchor.
///
/// Thin wrapper over [`bevy_text_engine::BufferAnchorParam`] that adds
/// the host's panel-screen offset on top of the editor-relative anchor
/// the engine returns. The engine knows where things are inside the
/// editor; the host knows where the editor is on screen — they
/// compose at this seam.
fn cursor_screen_pos(
    char_index: usize,
    editor: Entity,
    anchors: &bevy_text_engine::BufferAnchorParam,
    viewport_offset: &LspEguiViewportOffset,
) -> (f32, f32) {
    let Some(anchor) = anchors.at_rope_char_index(editor, char_index) else {
        return (
            viewport_offset.screen_offset.x,
            viewport_offset.screen_offset.y,
        );
    };
    (
        viewport_offset.screen_offset.x + anchor.screen_top_left.x,
        viewport_offset.screen_offset.y + anchor.screen_top_left.y,
    )
}

/// Position a popup below (preferred) or above the cursor line, clamped to viewport.
fn position_popup(
    cursor_x: f32,
    cursor_y: f32,
    popup_width: f32,
    popup_height: f32,
    line_height: f32,
    viewport_offset: &LspEguiViewportOffset,
    viewport: &TextViewViewport,
    prefer_above: bool,
) -> egui::Pos2 {
    let vp_left = viewport_offset.screen_offset.x;
    let vp_top = viewport_offset.screen_offset.y;
    let vp_right = vp_left + viewport.width as f32;
    let vp_bottom = vp_top + viewport.height as f32;

    let below_y = cursor_y + line_height;
    let above_y = cursor_y - popup_height;

    let y = if prefer_above {
        if above_y >= vp_top {
            above_y
        } else {
            below_y
        }
    } else if below_y + popup_height <= vp_bottom {
        below_y
    } else if above_y >= vp_top {
        above_y
    } else if (vp_bottom - below_y) > (cursor_y - vp_top) {
        below_y
    } else {
        above_y.max(vp_top)
    };

    let x = cursor_x.max(vp_left).min(vp_right - popup_width);

    egui::pos2(x, y)
}

/// Render the completion popup as an egui overlay using armas styling.
fn render_completion_egui(
    mut contexts: EguiContexts,
    query: Query<
        (Entity, &LspCompletionPopup, &CursorState, &FontConfig),
        With<CodeEditor>,
    >,
    viewport_offset: Res<LspEguiViewportOffset>,
    viewport: Single<&TextViewViewport, With<CodeEditor>>,
    anchors: bevy_text_engine::BufferAnchorParam,
) {
    let Ok((editor, completion_state, cursor_state, font)) = query.single() else {
        return;
    };
    let filtered_items = completion_state.filtered_items();
    if !completion_state.visible || filtered_items.is_empty() {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(c) => c,
        Err(e) => {
            warn!("LSP egui completion: failed to get context: {e:?}");
            return;
        }
    };

    let (cursor_x, cursor_y) =
        cursor_screen_pos(cursor_state.cursor_pos, editor, &anchors, &viewport_offset);

    let theme = ctx.armas_theme();
    let max_visible = 10;
    let visible_count = filtered_items.len().min(max_visible);
    let item_height = font.line_height.max(20.0);
    let popup_height = visible_count as f32 * item_height + 8.0;

    let max_label_width = filtered_items
        .iter()
        .take(max_visible)
        .map(|item| {
            let label_len = item.label().chars().count();
            let detail_len = item.detail().map(|d| d.chars().count()).unwrap_or(0);
            label_len + detail_len + 5
        })
        .max()
        .unwrap_or(20);
    let popup_width = (max_label_width as f32 * font.char_width + 40.0).clamp(200.0, 500.0);

    let pos = position_popup(
        cursor_x,
        cursor_y,
        popup_width,
        popup_height,
        font.line_height,
        &viewport_offset,
        *viewport,
        false,
    );

    egui::Area::new(egui::Id::new("lsp_completion"))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(theme.card())
                .stroke(egui::Stroke::new(1.0, theme.border()))
                .corner_radius(egui::CornerRadius::same(theme.spacing.corner_radius))
                .inner_margin(egui::Margin::same(4))
                .show(ui, |ui| {
                    ui.set_width(popup_width);
                    ui.set_max_height(popup_height);

                    let scroll_offset = completion_state.scroll_offset;
                    let visible_items = filtered_items.iter().skip(scroll_offset).take(max_visible);

                    for (i, item) in visible_items.enumerate() {
                        let absolute_index = scroll_offset + i;
                        let is_selected = absolute_index == completion_state.selected_index;

                        let bg = if is_selected {
                            theme.accent()
                        } else {
                            egui::Color32::TRANSPARENT
                        };

                        let text_color = if is_selected {
                            theme.accent_foreground()
                        } else {
                            theme.foreground()
                        };

                        let detail_color = if is_selected {
                            theme.accent_foreground()
                        } else {
                            theme.muted_foreground()
                        };

                        egui::Frame::NONE
                            .fill(bg)
                            .corner_radius(egui::CornerRadius::same(
                                theme.spacing.corner_radius_small,
                            ))
                            .inner_margin(egui::Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.set_width(popup_width - 8.0);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(item.kind_icon())
                                            .color(detail_color)
                                            .size(font.font_size * 0.9),
                                    );

                                    ui.label(
                                        egui::RichText::new(item.label())
                                            .color(text_color)
                                            .size(font.font_size),
                                    );

                                    if let Some(detail) = item.detail() {
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new(detail)
                                                        .color(detail_color)
                                                        .size(font.font_size * 0.8),
                                                );
                                            },
                                        );
                                    }
                                });
                            });
                    }
                });
        });

    // Side docs panel: when the server resolved documentation for the
    // selected item, show it in a panel anchored to the right of the
    // completion popup. Mirrors Zed's "completion + docs" layout.
    let selected_label = filtered_items
        .get(completion_state.selected_index)
        .map(|item| item.label().to_string());
    let resolved_docs = selected_label
        .as_deref()
        .and_then(|label| completion_state.resolved.get(label))
        .and_then(|item| item.documentation.as_ref())
        .map(|doc| match doc {
            lsp_types::Documentation::String(s) => s.clone(),
            lsp_types::Documentation::MarkupContent(m) => m.value.clone(),
        })
        .filter(|s| !s.is_empty());

    if let Some(docs) = resolved_docs {
        let docs_pos = egui::pos2(pos.x + popup_width + 4.0, pos.y);
        egui::Area::new(egui::Id::new("lsp_completion_docs"))
            .fixed_pos(docs_pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(theme.card())
                    .stroke(egui::Stroke::new(1.0, theme.border()))
                    .corner_radius(egui::CornerRadius::same(theme.spacing.corner_radius))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(360.0);
                        ui.set_max_height(popup_height);
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(docs)
                                    .color(theme.foreground())
                                    .size(font.font_size * 0.9),
                            );
                        });
                    });
            });
    }
}

/// Render the hover popup as an egui overlay.
///
/// Skipped when the LSP server returned Markdown — `render_hover_markdown`
/// (the sprite + TextView path) handles that case to render real rich
/// text. Plain-text hovers stay on the egui path because there's nothing
/// to gain from involving the markdown renderer.
fn render_hover_egui(
    mut contexts: EguiContexts,
    query: Query<
        (Entity, &LspHoverPopup, &LspCompletionPopup, &FontConfig),
        With<CodeEditor>,
    >,
    viewport_offset: Res<LspEguiViewportOffset>,
    viewport: Single<&TextViewViewport, With<CodeEditor>>,
    anchors: bevy_text_engine::BufferAnchorParam,
) {
    let Ok((editor, hover_state, completion_state, font)) = query.single() else {
        return;
    };
    if !hover_state.visible || hover_state.content.is_empty() {
        return;
    }
    // Markdown hovers route through the bevy_markdown viewer instead.
    if hover_state.kind == lsp_types::MarkupKind::Markdown {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(c) => c,
        Err(e) => {
            warn!("LSP egui hover: failed to get context: {e:?}");
            return;
        }
    };

    let (cursor_x, cursor_y) = cursor_screen_pos(
        hover_state.trigger_char_index,
        editor,
        &anchors,
        &viewport_offset,
    );

    let theme = ctx.armas_theme();

    let hover_line_count = hover_state.content.lines().count().max(1) as f32;
    let estimated_height = hover_line_count * font.line_height + 24.0;

    // If completion is visible, prefer showing hover above cursor to avoid overlap
    let prefer_above = completion_state.visible;

    let pos = position_popup(
        cursor_x,
        cursor_y,
        500.0,
        estimated_height,
        font.line_height,
        &viewport_offset,
        *viewport,
        prefer_above,
    );

    egui::Area::new(egui::Id::new("lsp_hover"))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(theme.card())
                .stroke(egui::Stroke::new(1.0, theme.border()))
                .corner_radius(egui::CornerRadius::same(theme.spacing.corner_radius))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_max_width(500.0);
                    ui.label(
                        egui::RichText::new(&hover_state.content)
                            .color(theme.card_foreground())
                            .size(font.font_size * 0.9),
                    );
                });
        });
}

/// Marker for the markdown hover popup entity. Carries the popup's
/// background sprite + the bundle of components that drive the
/// `bevy_markdown` viewer (parses + relayouts on `Changed<MarkdownDoc>`).
#[derive(Component)]
struct MarkdownHoverPopup;

/// Render the hover popup as a sprite + child TextView when the LSP
/// server returned Markdown. Mirrors `render_hover_egui`'s position
/// math but uses world-space sprites so the markdown viewer's GPU
/// pipeline draws into the same frame as the editor.
///
/// Exists alongside (not replacing) `render_hover_egui`: that one
/// handles plain-text hovers; this one handles markdown. The two are
/// mutually exclusive at runtime — both check `hover_state.kind` and
/// short-circuit when the format isn't theirs.
fn render_hover_markdown(
    mut commands: Commands,
    editors: Query<(Entity, &LspHoverPopup, &FontConfig), With<CodeEditor>>,
    viewport_offset: Res<LspEguiViewportOffset>,
    viewport: Single<&TextViewViewport, With<CodeEditor>>,
    anchors: bevy_text_engine::BufferAnchorParam,
    existing: Query<Entity, With<MarkdownHoverPopup>>,
) {
    let Ok((editor, hover_state, font)) = editors.single() else {
        return;
    };

    let visible = hover_state.visible
        && !hover_state.content.is_empty()
        && hover_state.kind == lsp_types::MarkupKind::Markdown;

    if !visible {
        for entity in existing.iter() {
            commands
                .entity(entity)
                .queue_silenced(bevy::ecs::system::entity_command::despawn());
        }
        return;
    }

    let (cursor_x, cursor_y) =
        cursor_screen_pos(hover_state.trigger_char_index, editor, &anchors, &viewport_offset);

    let popup_width = 520.0_f32;
    let popup_height = estimate_markdown_popup_height(&hover_state.content, font);
    let pos = position_popup_world(
        cursor_x,
        cursor_y,
        popup_width,
        popup_height,
        font.line_height,
        *viewport,
    );

    let viewport_for_layout = TextViewViewport {
        width: popup_width as u32,
        height: popup_height as u32,
        text_area_left: 12.0,
        text_area_top: 12.0,
        ..default()
    };

    if let Some(entity) = existing.iter().next() {
        // Update path: just replace the doc + transform. The plugin's
        // `rebuild_markdown_layout` re-parses on `Changed<MarkdownDoc>`.
        let pos_with_z = Vec3::new(pos.x + popup_width * 0.5, pos.y - popup_height * 0.5, 100.0);
        commands.entity(entity).insert((
            MarkdownDoc::new(hover_state.content.clone()),
            viewport_for_layout,
            Transform::from_translation(pos_with_z),
            Sprite::from_color(
                Color::srgba(0.10, 0.11, 0.13, 0.96),
                Vec2::new(popup_width, popup_height),
            ),
        ));
    } else {
        let pos_with_z = Vec3::new(pos.x + popup_width * 0.5, pos.y - popup_height * 0.5, 100.0);
        commands.spawn((
            Name::new("LspHoverMarkdown"),
            MarkdownHoverPopup,
            LspUiVisual,
            // Sprite acts as the popup's background card. The child
            // TextView paints on top via the engine's render pass.
            Sprite::from_color(
                Color::srgba(0.10, 0.11, 0.13, 0.96),
                Vec2::new(popup_width, popup_height),
            ),
            Transform::from_translation(pos_with_z),
            TextView,
            TextBuffer::default(),
            ScrollState::default(),
            ContentMetrics::default(),
            viewport_for_layout,
            font.clone(),
            MarkdownDoc::new(hover_state.content.clone()),
            DisplayLayout::default(),
            MarkdownLinks::default(),
        ));
    }
}

/// Crude height estimate for the markdown popup, mirroring the egui
/// path's heuristic: line count × line_height plus some chrome. Real
/// height after layout may differ; the sprite background is sized
/// from this estimate and clipping is left to the popup's max width.
fn estimate_markdown_popup_height(content: &str, font: &FontConfig) -> f32 {
    let line_count = content.lines().count().max(1) as f32;
    // Headings + code blocks inflate height — pad generously.
    (line_count * font.line_height * 1.2 + 40.0).clamp(64.0, 480.0)
}

/// World-space popup placement: returns the popup's top-left corner in
/// the same coordinate space `Transform::from_translation` expects (Y
/// increases upward, origin at viewport center). Mirrors
/// `position_popup`'s clamping but skips the viewport_offset since
/// world-space sprites don't need the screen-offset shift.
fn position_popup_world(
    cursor_x: f32,
    cursor_y: f32,
    popup_width: f32,
    popup_height: f32,
    line_height: f32,
    viewport: &TextViewViewport,
) -> Vec2 {
    let half_w = viewport.width as f32 * 0.5;
    let half_h = viewport.height as f32 * 0.5;

    // cursor_screen_pos returns top-left-origin screen pixels relative
    // to the editor panel; convert to Bevy world (center origin, Y up).
    let world_cursor_x = cursor_x - half_w;
    let world_cursor_y = half_h - cursor_y;

    // Prefer below cursor; flip above when the popup would overflow.
    let below_top_y = world_cursor_y - line_height;
    let above_top_y = world_cursor_y + line_height + popup_height;
    let top_y = if below_top_y - popup_height >= -half_h {
        below_top_y
    } else if above_top_y <= half_h {
        above_top_y
    } else {
        below_top_y
    };

    // Clamp horizontally so the popup stays on-screen.
    let max_left = half_w - popup_width;
    let left_x = world_cursor_x.clamp(-half_w, max_left.max(-half_w));

    Vec2::new(left_x, top_y)
}

/// Render the signature help popup as an egui overlay.
fn render_signature_help_egui(
    mut contexts: EguiContexts,
    query: Query<
        (Entity, &LspSignatureHelpPopup, &CursorState, &FontConfig),
        With<CodeEditor>,
    >,
    viewport_offset: Res<LspEguiViewportOffset>,
    viewport: Single<&TextViewViewport, With<CodeEditor>>,
    anchors: bevy_text_engine::BufferAnchorParam,
) {
    let Ok((editor, sig_state, cursor_state, font)) = query.single() else {
        return;
    };
    if !sig_state.visible || sig_state.signatures.is_empty() {
        return;
    }

    let Some(signature) = sig_state.current_signature() else {
        return;
    };

    let ctx = match contexts.ctx_mut() {
        Ok(c) => c,
        Err(e) => {
            warn!("LSP egui signature: failed to get context: {e:?}");
            return;
        }
    };

    let (cursor_x, cursor_y) =
        cursor_screen_pos(cursor_state.cursor_pos, editor, &anchors, &viewport_offset);

    let theme = ctx.armas_theme();

    // Signature help prefers above cursor (like VS Code)
    let estimated_height = font.line_height + 20.0;
    let pos = position_popup(
        cursor_x,
        cursor_y,
        400.0,
        estimated_height,
        font.line_height,
        &viewport_offset,
        *viewport,
        true,
    );

    let active_param_range: Option<(usize, usize)> = signature
        .parameters
        .as_ref()
        .and_then(|params| params.get(sig_state.active_parameter))
        .and_then(|param| match &param.label {
            lsp_types::ParameterLabel::LabelOffsets([start, end]) => {
                Some((*start as usize, *end as usize))
            }
            lsp_types::ParameterLabel::Simple(s) => signature
                .label
                .find(s.as_str())
                .map(|pos| (pos, pos + s.len())),
        });

    egui::Area::new(egui::Id::new("lsp_signature_help"))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(theme.card())
                .stroke(egui::Stroke::new(1.0, theme.border()))
                .corner_radius(egui::CornerRadius::same(theme.spacing.corner_radius))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let label = &signature.label;
                        let text_size = font.font_size * 0.9;

                        if let Some((start, end)) = active_param_range {
                            let before = label.get(..start).unwrap_or(label);
                            let active = label.get(start..end).unwrap_or("");
                            let after = label.get(end..).unwrap_or("");

                            if !before.is_empty() {
                                ui.label(
                                    egui::RichText::new(before)
                                        .color(theme.muted_foreground())
                                        .size(text_size),
                                );
                            }
                            if !active.is_empty() {
                                ui.label(
                                    egui::RichText::new(active)
                                        .color(theme.card_foreground())
                                        .strong()
                                        .size(text_size),
                                );
                            }
                            if !after.is_empty() {
                                ui.label(
                                    egui::RichText::new(after)
                                        .color(theme.muted_foreground())
                                        .size(text_size),
                                );
                            }
                        } else {
                            ui.label(
                                egui::RichText::new(label)
                                    .color(theme.card_foreground())
                                    .size(text_size),
                            );
                        }

                        if sig_state.signatures.len() > 1 {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}/{}",
                                    sig_state.active_signature + 1,
                                    sig_state.signatures.len()
                                ))
                                .color(theme.muted_foreground())
                                .size(font.font_size * 0.75),
                            );
                        }
                    });
                });
        });
}

/// Render the code actions popup as an egui overlay.
fn render_code_actions_egui(
    mut contexts: EguiContexts,
    query: Query<
        (Entity, &LspCodeActionsPopup, &CursorState, &FontConfig),
        With<CodeEditor>,
    >,
    viewport_offset: Res<LspEguiViewportOffset>,
    viewport: Single<&TextViewViewport, With<CodeEditor>>,
    anchors: bevy_text_engine::BufferAnchorParam,
) {
    let Ok((editor, action_state, cursor_state, font)) = query.single() else {
        return;
    };
    if !action_state.visible || action_state.actions.is_empty() {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let (_, cursor_y) =
        cursor_screen_pos(cursor_state.cursor_pos, editor, &anchors, &viewport_offset);

    let theme = ctx.armas_theme();
    let item_height = font.line_height.max(22.0);

    // Position near the gutter, below cursor
    let gutter_x = viewport_offset.screen_offset.x + viewport.text_area_left - 20.0;
    let action_count = action_state.actions.len().min(10);
    let popup_height = action_count as f32 * item_height + 12.0;

    let pos = position_popup(
        gutter_x,
        cursor_y,
        300.0,
        popup_height,
        font.line_height,
        &viewport_offset,
        *viewport,
        false,
    );

    egui::Area::new(egui::Id::new("lsp_code_actions"))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(theme.card())
                .stroke(egui::Stroke::new(1.0, theme.border()))
                .corner_radius(egui::CornerRadius::same(theme.spacing.corner_radius))
                .inner_margin(egui::Margin::same(4))
                .show(ui, |ui| {
                    for (i, action) in action_state.actions.iter().take(10).enumerate() {
                        let is_selected = i == action_state.selected_index;

                        let bg = if is_selected {
                            theme.accent()
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        let text_color = if is_selected {
                            theme.accent_foreground()
                        } else {
                            theme.foreground()
                        };

                        let (icon, title) = match action {
                            bevy_lsp::CodeActionOrCommand::Action(a) => {
                                let icon = match &a.kind {
                                    Some(kind) if kind.as_str().starts_with("quickfix") => "W",
                                    Some(kind) if kind.as_str().starts_with("refactor") => "R",
                                    Some(kind) if kind.as_str().starts_with("source") => "S",
                                    _ => "A",
                                };
                                (icon, a.title.as_str())
                            }
                            bevy_lsp::CodeActionOrCommand::Command(c) => ("C", c.title.as_str()),
                        };

                        egui::Frame::NONE
                            .fill(bg)
                            .corner_radius(egui::CornerRadius::same(
                                theme.spacing.corner_radius_small,
                            ))
                            .inner_margin(egui::Margin::symmetric(8, 2))
                            .show(ui, |ui| {
                                ui.set_height(item_height);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(icon)
                                            .color(theme.muted_foreground())
                                            .size(font.font_size * 0.85),
                                    );
                                    ui.label(
                                        egui::RichText::new(title)
                                            .color(text_color)
                                            .size(font.font_size),
                                    );
                                });
                            });
                    }
                });
        });
}

/// Render the rename input as an interactive egui overlay.
fn render_rename_egui(
    mut contexts: EguiContexts,
    mut query: Query<
        (
            Entity,
            &mut LspRenamePopup,
            &LspClient,
            Option<&LspDocument>,
            &FontConfig,
        ),
        With<CodeEditor>,
    >,
    viewport_offset: Res<LspEguiViewportOffset>,
    viewport: Single<&TextViewViewport, With<CodeEditor>>,
    anchors: bevy_text_engine::BufferAnchorParam,
) {
    let Ok((editor, mut rename_state, lsp_client, lsp_document, font)) = query.single_mut() else {
        return;
    };
    if !rename_state.visible {
        return;
    }

    let range = match rename_state.range {
        Some(r) => r,
        None => return,
    };

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // LSP-flavored anchor: `(line, character)` straight from the
    // server's response, no rope-arithmetic in the host.
    let Some(anchor) = anchors.at_buffer_pos(editor, range.start.line, range.start.character)
    else {
        return;
    };
    let cursor_x = viewport_offset.screen_offset.x + anchor.screen_top_left.x;
    let cursor_y = viewport_offset.screen_offset.y + anchor.screen_top_left.y;

    let theme = ctx.armas_theme();
    let rename_width = (rename_state.new_name.len() as f32 * font.char_width + 40.0).max(150.0);
    let pos = position_popup(
        cursor_x,
        cursor_y,
        rename_width,
        font.line_height + 10.0,
        font.line_height,
        &viewport_offset,
        *viewport,
        false,
    );

    let mut submit = false;
    let mut cancel = false;

    egui::Area::new(egui::Id::new("lsp_rename"))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(theme.card())
                .stroke(egui::Stroke::new(2.0, theme.ring()))
                .corner_radius(egui::CornerRadius::same(theme.spacing.corner_radius_small))
                .inner_margin(egui::Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut rename_state.new_name)
                            .font(egui::FontId::proportional(font.font_size))
                            .desired_width(rename_width - 20.0),
                    );
                    response.request_focus();

                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        submit = true;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        cancel = true;
                    }
                });
        });

    if cancel {
        rename_state.reset();
    } else if submit && rename_state.can_submit() {
        if let Some(doc) = lsp_document {
            lsp_client.send(LspMessage::Rename {
                uri: doc.uri.clone(),
                position: range.start,
                new_name: rename_state.new_name.clone(),
                id: 0,
            });
        }
    }
}

fn setup_editor(
    mut commands: Commands,
    mut editor_query: Query<(Entity, &mut LspClient), With<CodeEditor>>,
    runtime: Res<bevy_lsp::bevy_tokio_tasks::TokioTasksRuntime>,
    mut set_text_writer: MessageWriter<bevy_text_editor::SetTextRequested>,
) {
    let Ok((editor_entity, mut lsp_client)) = editor_query.single_mut() else {
        return;
    };

    let example_file_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("editor_lsp.rs");
    let rust_code =
        std::fs::read_to_string(&example_file_path).expect("Failed to read example file");

    set_text_writer.write(bevy_text_editor::SetTextRequested {
        entity: editor_entity,
        text: rust_code.clone(),
    });

    // Component-driven tree-sitter wiring: attach a `Language` to the
    // editor entity. The editor's `react_language_changed` system picks
    // it up and configures the per-entity highlight provider. LSP wiring
    // is the host's responsibility (we call `lsp_client.start` further
    // down).
    #[cfg(feature = "tree-sitter")]
    commands.entity(editor_entity).insert(Language::from_grammar(
        "rust",
        tree_sitter_rust::LANGUAGE.into(),
        tree_sitter_rust::HIGHLIGHTS_QUERY,
    ));

    let file_uri_str = format!("file://{}", example_file_path.to_string_lossy());
    #[cfg(target_os = "windows")]
    let file_uri_str = format!(
        "file:///{}",
        example_file_path.to_string_lossy().replace('\\', "/")
    );

    let doc_uri = lsp_types::Url::parse(&file_uri_str).expect("Failed to parse URI");

    // Make sure 'rust-analyzer' is in your PATH (rustup component add rust-analyzer)
    if let Err(e) = lsp_client.start(&runtime, "rust-analyzer", &[]) {
        error!("Failed to start rust-analyzer: {:?}", e);
        return;
    }

    // rootUri is the project root, which is usually the directory containing Cargo.toml
    let project_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root_uri = lsp_types::Url::from_directory_path(&project_root)
        .expect("Failed to get project root URI");
    let capabilities = lsp_types::ClientCapabilities::default();

    lsp_client.send(LspMessage::Initialize {
        root_uri: root_uri.clone(),
        capabilities,
    });

    lsp_client.send(LspMessage::Initialized);

    lsp_client.send(LspMessage::DidOpen {
        uri: doc_uri.clone(),
        language_id: "rust".to_string(),
        version: 1,
        text: rust_code.to_string(),
    });

    // Insert per-document state on the editor entity. Other LSP-side
    // Components (capabilities, popups, debounce, sync extra) are already on
    // the entity via `CodeEditor`'s `#[require]` cascade; only `LspDocument`
    // requires a URI which the host supplies here.
    commands
        .entity(editor_entity)
        .insert(LspDocument::new(doc_uri, "rust"));

    info!("LSP started for file: {:?}", example_file_path);
}

fn display_lsp_info(query: Query<&LspClient, (With<CodeEditor>, Changed<LspClient>)>) {
    if !query.is_empty() {
        info!("LSP client state changed");
    }
}

/// Auto-trigger completion requests after typing.
///
/// The input system doesn't emit RequestCompletionEvent yet, so this bridges
/// the gap by watching for content changes and firing completion at the cursor.
fn auto_request_completion(
    editor_query: Query<(&CursorState, Ref<TextBuffer>), With<CodeEditor>>,
    mut writer: MessageWriter<bevy_code_editor::types::events::RequestCompletionEvent>,
) {
    let Ok((cursor, buffer)) = editor_query.single() else {
        return;
    };

    // pending_update was removed in the snapshot-Arc refactor. Drive completion
    // off Bevy's Changed detection on TextBuffer — fires when the rope changes,
    // which is the trigger condition we want.
    if !buffer.is_changed() {
        return;
    }

    let cursor_pos = cursor.cursor_pos.min(buffer.rope.len_chars());
    if cursor_pos == 0 {
        return;
    }
    writer.write(bevy_code_editor::types::events::RequestCompletionEvent::new(cursor_pos));
}
