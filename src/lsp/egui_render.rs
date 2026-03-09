//! egui/armas-based render systems for LSP UI overlays.
//!
//! These systems replace the Bevy Sprite-based render systems with egui overlays
//! using armas components for consistent, theme-aware styling.
//!
//! # Usage
//!
//! Use `LspEguiUiPlugin` instead of `LspUiPlugin`:
//!
//! ```rust,ignore
//! app.add_plugins(CodeEditorPlugin::default())
//!     .add_plugins(LspPlugin::default())
//!     .add_plugins(LspEguiUiPlugin::default());
//! ```

use armas::prelude::*;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::components::*;
use super::state::*;
use crate::settings::FontSettings;
use crate::types::{CodeEditorState, ViewportDimensions};

/// Screen-space offset for positioning egui overlays relative to the editor panel.
///
/// The consuming app must update this each frame with the editor panel's screen position.
/// If the editor fills the whole window, this stays at (0, 0).
#[derive(Resource, Default, Debug, Clone)]
pub struct LspEguiViewportOffset {
    /// Top-left corner of the editor panel in screen pixels
    pub screen_offset: Vec2,
}

/// Render the completion popup as an egui overlay using armas styling.
pub fn render_completion_egui(
    mut contexts: EguiContexts,
    completion_state: Res<CompletionState>,
    editor_state: Res<CodeEditorState>,
    font: Res<FontSettings>,
    viewport_offset: Res<LspEguiViewportOffset>,
    viewport: Res<ViewportDimensions>,
) {
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

    // Calculate position relative to cursor
    let cursor_pos = editor_state.cursor_pos.min(editor_state.rope.len_chars());
    let line_index = editor_state.rope.char_to_line(cursor_pos);
    let line_start = editor_state.rope.line_to_char(line_index);
    let col_index = cursor_pos - line_start;

    let x = viewport_offset.screen_offset.x
        + viewport.text_area_left
        + (col_index as f32 * font.char_width);
    let y = viewport_offset.screen_offset.y
        + viewport.text_area_top
        + ((line_index as f32 - editor_state.scroll_offset / font.line_height) * font.line_height)
        + font.line_height;

    let theme = ctx.armas_theme();
    let max_visible = 10;
    let visible_count = filtered_items.len().min(max_visible);
    let item_height = font.line_height.max(20.0);
    let popup_height = visible_count as f32 * item_height + 8.0;

    // Calculate popup width from content
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
    let popup_width = (max_label_width as f32 * font.char_width + 40.0)
        .max(200.0)
        .min(500.0);

    egui::Area::new(egui::Id::new("lsp_completion"))
        .fixed_pos(egui::pos2(x, y))
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
                    let visible_items =
                        filtered_items.iter().skip(scroll_offset).take(max_visible);

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
                            .corner_radius(egui::CornerRadius::same(theme.spacing.corner_radius_small))
                            .inner_margin(egui::Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.set_width(popup_width - 8.0);
                                ui.horizontal(|ui| {
                                    // Kind icon
                                    ui.label(
                                        egui::RichText::new(item.kind_icon())
                                            .color(detail_color)
                                            .size(font.size * 0.9),
                                    );

                                    // Label
                                    ui.label(
                                        egui::RichText::new(item.label())
                                            .color(text_color)
                                            .size(font.size),
                                    );

                                    // Detail (right-aligned)
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
pub fn render_hover_egui(
    mut contexts: EguiContexts,
    hover_state: Res<HoverState>,
    editor_state: Res<CodeEditorState>,
    font: Res<FontSettings>,
    viewport_offset: Res<LspEguiViewportOffset>,
    viewport: Res<ViewportDimensions>,
) {
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

    let trigger_char_index = hover_state
        .trigger_char_index
        .min(editor_state.rope.len_chars());
    let line_index = editor_state.rope.char_to_line(trigger_char_index);
    let line_start = editor_state.rope.line_to_char(line_index);
    let col_index = trigger_char_index - line_start;

    let x = viewport_offset.screen_offset.x
        + viewport.text_area_left
        + (col_index as f32 * font.char_width);
    let y = viewport_offset.screen_offset.y
        + viewport.text_area_top
        + ((line_index as f32 - editor_state.scroll_offset / font.line_height) * font.line_height)
        + font.line_height;

    let theme = ctx.armas_theme();

    egui::Area::new(egui::Id::new("lsp_hover"))
        .fixed_pos(egui::pos2(x, y))
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
pub fn render_signature_help_egui(
    mut contexts: EguiContexts,
    sig_state: Res<SignatureHelpState>,
    editor_state: Res<CodeEditorState>,
    font: Res<FontSettings>,
    viewport_offset: Res<LspEguiViewportOffset>,
    viewport: Res<ViewportDimensions>,
) {
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

    let cursor_pos = editor_state.cursor_pos.min(editor_state.rope.len_chars());
    let line_index = editor_state.rope.char_to_line(cursor_pos);
    let line_start = editor_state.rope.line_to_char(line_index);
    let col_index = cursor_pos - line_start;

    // Position ABOVE the cursor line
    let x = viewport_offset.screen_offset.x
        + viewport.text_area_left
        + (col_index as f32 * font.char_width);
    let y = viewport_offset.screen_offset.y
        + viewport.text_area_top
        + ((line_index as f32 - editor_state.scroll_offset / font.line_height) * font.line_height)
        - font.line_height * 1.5;

    let theme = ctx.armas_theme();

    egui::Area::new(egui::Id::new("lsp_signature_help"))
        .fixed_pos(egui::pos2(x, y.max(0.0)))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(theme.card())
                .stroke(egui::Stroke::new(1.0, theme.border()))
                .corner_radius(egui::CornerRadius::same(theme.spacing.corner_radius))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&signature.label)
                                .color(theme.card_foreground())
                                .size(font.size * 0.9),
                        );

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
pub fn render_code_actions_egui(
    mut contexts: EguiContexts,
    action_state: Res<CodeActionState>,
    editor_state: Res<CodeEditorState>,
    font: Res<FontSettings>,
    viewport_offset: Res<LspEguiViewportOffset>,
    viewport: Res<ViewportDimensions>,
) {
    if !action_state.visible || action_state.actions.is_empty() {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let cursor_pos = editor_state.cursor_pos.min(editor_state.rope.len_chars());
    let line_index = editor_state.rope.char_to_line(cursor_pos);

    // Position near the gutter
    let x = viewport_offset.screen_offset.x + viewport.text_area_left - 20.0;
    let y = viewport_offset.screen_offset.y
        + viewport.text_area_top
        + ((line_index as f32 - editor_state.scroll_offset / font.line_height + 1.0)
            * font.line_height);

    let theme = ctx.armas_theme();
    let item_height = font.line_height.max(22.0);

    egui::Area::new(egui::Id::new("lsp_code_actions"))
        .fixed_pos(egui::pos2(x, y))
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
                            super::messages::CodeActionOrCommand::Action(a) => {
                                let icon = match &a.kind {
                                    Some(kind) if kind.as_str().starts_with("quickfix") => "W",
                                    Some(kind) if kind.as_str().starts_with("refactor") => "R",
                                    Some(kind) if kind.as_str().starts_with("source") => "S",
                                    _ => "A",
                                };
                                (icon, a.title.as_str())
                            }
                            super::messages::CodeActionOrCommand::Command(c) => {
                                ("C", c.title.as_str())
                            }
                        };

                        egui::Frame::NONE
                            .fill(bg)
                            .corner_radius(egui::CornerRadius::same(theme.spacing.corner_radius_small))
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

/// Render the rename input as an egui overlay.
pub fn render_rename_egui(
    mut contexts: EguiContexts,
    rename_state: Res<RenameState>,
    editor_state: Res<CodeEditorState>,
    font: Res<FontSettings>,
    viewport_offset: Res<LspEguiViewportOffset>,
    viewport: Res<ViewportDimensions>,
) {
    if !rename_state.visible {
        return;
    }

    let Some(range) = &rename_state.range else {
        return;
    };

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let line = range.start.line as usize;
    let character = range.start.character as usize;

    let x = viewport_offset.screen_offset.x
        + viewport.text_area_left
        + (character as f32 * font.char_width);
    let y = viewport_offset.screen_offset.y
        + viewport.text_area_top
        + ((line as f32 - editor_state.scroll_offset / font.line_height) * font.line_height);

    let theme = ctx.armas_theme();

    // We display the current rename text but since RenameState is not mutable here,
    // we show it as a read-only styled label. The actual editing happens through
    // the editor's keyboard input system which updates RenameState.
    let display_text = if rename_state.new_name.is_empty() {
        &rename_state.original_text
    } else {
        &rename_state.new_name
    };

    egui::Area::new(egui::Id::new("lsp_rename"))
        .fixed_pos(egui::pos2(x, y))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(theme.card())
                .stroke(egui::Stroke::new(2.0, theme.ring()))
                .corner_radius(egui::CornerRadius::same(theme.spacing.corner_radius_small))
                .inner_margin(egui::Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(display_text)
                            .color(theme.card_foreground())
                            .size(font.size),
                    );
                });
        });
}
