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
use bevy_code_editor::plugin::editor_ui_plugin::EditorRenderConfig;
use bevy_code_editor::prelude::*;
use bevy_text_engine::FontConfig;
use bevy_code_editor::text_view::TextViewViewport;
use bevy_code_editor::types::{
    CodeEditor, CursorState, EditHistoryState, EditorDisplayState, SelectionState,
    SyntaxCacheState, ViewportDimensions,
};
use bevy_egui::{egui, EguiContexts};
use bevy_lsp::{LspClient, LspDocument, LspMessage};
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

    app.add_plugins(CodeEditorPlugin::standalone());

    // LspPlugin is auto-added by CodeEditorPlugin under the `lsp` feature; we
    // just supply the egui-based UI layer here.
    app.add_plugins(LspEguiUiPlugin);

    app.add_systems(PostStartup, setup_editor)
        .add_systems(Update, (display_lsp_info, auto_request_completion))
        .run();
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
                render_signature_help_egui,
                render_code_actions_egui,
                render_rename_egui,
                render_inlay_hints,
                render_document_highlights,
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

/// Marker for document highlight visual entities (sprite path).
#[derive(Component)]
struct DocumentHighlightMarker {
    #[allow(dead_code)]
    line: u32,
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
fn render_inlay_hints(
    mut commands: Commands,
    hint_query: Query<(Entity, &InlayHintData), Added<InlayHintData>>,
    editor_query: Query<(&TextViewViewport, &FontConfig), With<CodeEditor>>,
    theme: Res<InlineDecorationsTheme>,
    render_config: Res<EditorRenderConfig>,
) {
    let Ok((viewport, font)) = editor_query.single() else {
        return;
    };

    for (entity, hint) in hint_query.iter() {
        let color = match hint.kind {
            InlayHintKind::Type => theme.inlay_hints.type_color,
            InlayHintKind::Parameter => theme.inlay_hints.parameter_color,
            InlayHintKind::Other => theme.inlay_hints.default_color,
        };

        let pos = Vec3::new(
            viewport.world_left() + hint.position.x,
            viewport.world_top() - hint.position.y,
            theme.inlay_hints.z_index,
        );

        let mut entity_cmd = commands.entity(entity);
        entity_cmd.insert((
            Text2d::new(&hint.label),
            TextFont {
                font: font.font.clone().unwrap_or_default(),
                font_size: font.size * theme.inlay_hints.font_size_multiplier,
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
        ));

        if let Some(layers) = &render_config.render_layers {
            entity_cmd.insert(layers.clone());
        }
    }
}

/// Render document highlights from marker component data (inlined from the
/// library's old `lsp_ui::render::render_document_highlights`).
fn render_document_highlights(
    mut commands: Commands,
    highlight_query: Query<(Entity, &DocumentHighlightData), Added<DocumentHighlightData>>,
    viewport_query: Query<&TextViewViewport, With<CodeEditor>>,
    theme: Res<InlineDecorationsTheme>,
    render_config: Res<EditorRenderConfig>,
) {
    let Ok(viewport) = viewport_query.single() else {
        return;
    };

    for (entity, highlight) in highlight_query.iter() {
        let color = if highlight.is_write {
            theme.document_highlights.write_color
        } else {
            theme.document_highlights.read_color
        };

        let pos = Vec3::new(
            viewport.world_left() + highlight.position.x,
            viewport.world_top() - highlight.position.y,
            5.0, // Behind text
        );

        let mut entity_cmd = commands.entity(entity);
        entity_cmd.insert((
            Sprite {
                color,
                custom_size: Some(Vec2::new(highlight.width, highlight.height)),
                ..default()
            },
            Transform::from_translation(pos),
            DocumentHighlightMarker {
                line: highlight.line,
            },
            LspUiVisual,
        ));

        if let Some(layers) = &render_config.render_layers {
            entity_cmd.insert(layers.clone());
        }
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

/// Cursor screen position (x, y at top of cursor line).
fn cursor_screen_pos(
    char_index: usize,
    tv: &TextViewState,
    font: &FontConfig,
    viewport_offset: &LspEguiViewportOffset,
    viewport: &ViewportDimensions,
) -> (f32, f32) {
    let char_index = char_index.min(tv.rope.len_chars());
    let line_index = tv.rope.char_to_line(char_index);
    let line_start = tv.rope.line_to_char(line_index);
    let col_index = char_index - line_start;

    let x = viewport_offset.screen_offset.x
        + viewport.text_area_left
        + (col_index as f32 * font.char_width)
        - tv.horizontal_scroll_offset;
    let y = viewport_offset.screen_offset.y
        + viewport.text_area_top
        + ((line_index as f32 - tv.scroll_offset / font.line_height) * font.line_height);

    (x, y)
}

/// Position a popup below (preferred) or above the cursor line, clamped to viewport.
fn position_popup(
    cursor_x: f32,
    cursor_y: f32,
    popup_width: f32,
    popup_height: f32,
    line_height: f32,
    viewport_offset: &LspEguiViewportOffset,
    viewport: &ViewportDimensions,
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
        (&LspCompletionPopup, &CursorState, &TextViewState, &FontConfig),
        With<CodeEditor>,
    >,
    viewport_offset: Res<LspEguiViewportOffset>,
    viewport: Res<ViewportDimensions>,
) {
    let Ok((completion_state, cursor_state, tv, font)) = query.single() else {
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

    let (cursor_x, cursor_y) = cursor_screen_pos(
        cursor_state.cursor_pos,
        tv,
        font,
        &viewport_offset,
        &viewport,
    );

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
        &viewport,
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
                                            .size(font.size * 0.9),
                                    );

                                    ui.label(
                                        egui::RichText::new(item.label())
                                            .color(text_color)
                                            .size(font.size),
                                    );

                                    if let Some(detail) = item.detail() {
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new(detail)
                                                        .color(detail_color)
                                                        .size(font.size * 0.8),
                                                );
                                            },
                                        );
                                    }
                                });
                            });
                    }
                });
        });
}

/// Render the hover popup as an egui overlay.
fn render_hover_egui(
    mut contexts: EguiContexts,
    query: Query<
        (&LspHoverPopup, &LspCompletionPopup, &TextViewState, &FontConfig),
        With<CodeEditor>,
    >,
    viewport_offset: Res<LspEguiViewportOffset>,
    viewport: Res<ViewportDimensions>,
) {
    let Ok((hover_state, completion_state, tv, font)) = query.single() else {
        return;
    };
    if !hover_state.visible || hover_state.content.is_empty() {
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
        tv,
        font,
        &viewport_offset,
        &viewport,
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
        &viewport,
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
                            .size(font.size * 0.9),
                    );
                });
        });
}

/// Render the signature help popup as an egui overlay.
fn render_signature_help_egui(
    mut contexts: EguiContexts,
    query: Query<
        (
            &LspSignatureHelpPopup,
            &CursorState,
            &TextViewState,
            &FontConfig,
        ),
        With<CodeEditor>,
    >,
    viewport_offset: Res<LspEguiViewportOffset>,
    viewport: Res<ViewportDimensions>,
) {
    let Ok((sig_state, cursor_state, tv, font)) = query.single() else {
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

    let (cursor_x, cursor_y) = cursor_screen_pos(
        cursor_state.cursor_pos,
        tv,
        font,
        &viewport_offset,
        &viewport,
    );

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
        &viewport,
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
                        let text_size = font.size * 0.9;

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
                                .size(font.size * 0.75),
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
        (&LspCodeActionsPopup, &CursorState, &TextViewState, &FontConfig),
        With<CodeEditor>,
    >,
    viewport_offset: Res<LspEguiViewportOffset>,
    viewport: Res<ViewportDimensions>,
) {
    let Ok((action_state, cursor_state, tv, font)) = query.single() else {
        return;
    };
    if !action_state.visible || action_state.actions.is_empty() {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let (_, cursor_y) = cursor_screen_pos(
        cursor_state.cursor_pos,
        tv,
        font,
        &viewport_offset,
        &viewport,
    );

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
        &viewport,
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
                                            .size(font.size * 0.85),
                                    );
                                    ui.label(
                                        egui::RichText::new(title)
                                            .color(text_color)
                                            .size(font.size),
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
            &mut LspRenamePopup,
            &TextViewState,
            &LspClient,
            Option<&LspDocument>,
            &FontConfig,
        ),
        With<CodeEditor>,
    >,
    viewport_offset: Res<LspEguiViewportOffset>,
    viewport: Res<ViewportDimensions>,
) {
    let Ok((mut rename_state, tv, lsp_client, lsp_document, font)) = query.single_mut() else {
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

    let line = range.start.line as usize;
    let char_index = if line < tv.rope.len_lines() {
        let line_start = tv.rope.line_to_char(line);
        line_start + range.start.character as usize
    } else {
        tv.rope.len_chars()
    };

    let (cursor_x, cursor_y) =
        cursor_screen_pos(char_index, tv, font, &viewport_offset, &viewport);

    let theme = ctx.armas_theme();
    let rename_width = (rename_state.new_name.len() as f32 * font.char_width + 40.0).max(150.0);
    let pos = position_popup(
        cursor_x,
        cursor_y,
        rename_width,
        font.line_height + 10.0,
        font.line_height,
        &viewport_offset,
        &viewport,
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
                            .font(egui::FontId::proportional(font.size))
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
            });
        }
    }
}

fn setup_editor(
    mut commands: Commands,
    mut editor_query: Query<
        (
            Entity,
            &mut CursorState,
            &mut TextViewState,
            &mut EditHistoryState,
            &mut SelectionState,
            &mut SyntaxCacheState,
            &mut EditorDisplayState,
            &mut LspClient,
        ),
        With<CodeEditor>,
    >,
    runtime: Res<bevy_lsp::bevy_tokio_tasks::TokioTasksRuntime>,
) {
    let Ok((
        editor_entity,
        mut cursor,
        mut tv,
        mut hist,
        mut sel,
        mut syntax_cache,
        mut display,
        mut lsp_client,
    )) = editor_query.single_mut()
    else {
        return;
    };

    // Read the source code of this example file. CARGO_MANIFEST_DIR is set
    // by cargo to the package root at build time — use it instead of
    // current_dir so the example works regardless of where it's launched
    // from (workspace root vs. package dir).
    let example_file_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/lsp.rs");
    let rust_code =
        std::fs::read_to_string(&example_file_path).expect("Failed to read example file");

    bevy_code_editor::input::editing::set_text(
        &mut sel,
        &mut hist,
        &mut syntax_cache,
        &mut display,
        &mut cursor,
        &mut tv,
        &rust_code,
    );

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
    editor_query: Query<(&CursorState, Ref<TextViewState>), With<CodeEditor>>,
    mut writer: MessageWriter<bevy_code_editor::types::events::RequestCompletionEvent>,
) {
    let Ok((cursor, tv)) = editor_query.single() else {
        return;
    };

    // pending_update was removed in the snapshot-Arc refactor. Drive completion
    // off Bevy's Changed detection on TextViewState — fires when the rope or
    // scroll moves, which is the same trigger condition.
    if !tv.is_changed() {
        return;
    }

    let cursor_pos = cursor.cursor_pos.min(tv.rope.len_chars());
    if cursor_pos == 0 {
        return;
    }
    writer.write(bevy_code_editor::types::events::RequestCompletionEvent::new(cursor_pos));
}
