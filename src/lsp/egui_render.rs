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

use super::client::LspClient;
use super::messages::LspMessage;
use super::state::*;
use crate::settings::FontSettings;
use crate::text_view::{TextViewState, TextViewViewport};
use crate::types::{CodeEditor, CodeEditorState, CursorState};

/// Screen-space offset for positioning egui overlays relative to the editor panel.
///
/// The consuming app must update this each frame with the editor panel's screen position.
/// If the editor fills the whole window, this stays at (0, 0).
#[derive(Resource, Default, Debug, Clone)]
pub struct LspEguiViewportOffset {
    /// Top-left corner of the editor panel in screen pixels
    pub screen_offset: Vec2,
}

/// Calculate cursor screen position (x, y at bottom of cursor line)
fn cursor_screen_pos(
    char_index: usize,
    editor_state: &CodeEditorState,
    font: &FontSettings,
    viewport_offset: &LspEguiViewportOffset,
    viewport: &ViewportDimensions,
) -> (f32, f32) {
    let char_index = char_index.min(editor_state.rope.len_chars());
    let line_index = editor_state.rope.char_to_line(char_index);
    let line_start = editor_state.rope.line_to_char(line_index);
    let col_index = char_index - line_start;

    let x = viewport_offset.screen_offset.x
        + viewport.text_area_left
        + (col_index as f32 * font.char_width)
        - editor_state.horizontal_scroll_offset;
    let y = viewport_offset.screen_offset.y
        + viewport.text_area_top
        + ((line_index as f32 - editor_state.scroll_offset / font.line_height) * font.line_height);

    (x, y)
}

/// Position a popup below (preferred) or above the cursor line, clamped to viewport.
/// Returns the final (x, y) position.
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

    // Vertical: prefer below cursor (or above if prefer_above)
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
    } else {
        // Neither fits perfectly — pick whichever has more space
        if (vp_bottom - below_y) > (cursor_y - vp_top) {
            below_y
        } else {
            above_y.max(vp_top)
        }
    };

    // Horizontal: clamp right edge to viewport, but don't go past left edge
    let x = cursor_x.max(vp_left).min(vp_right - popup_width);

    egui::pos2(x, y)
}

/// Render the completion popup as an egui overlay using armas styling.
pub fn render_completion_egui(
    mut contexts: EguiContexts,
    completion_state: Res<CompletionState>,
    query: Query<(&CodeEditorState, &CursorState, &TextViewState, &TextViewViewport), With<CodeEditor>>,
    font: Res<FontSettings>,
    viewport_offset: Res<LspEguiViewportOffset>,
) {
    let Ok((editor_state, cursor_state, tv, vp)) = query.single() else { return };
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
    let cursor_pos = cursor_state.cursor_pos.min(tv.rope.len_chars());
    let line_index = tv.rope.char_to_line(cursor_pos);
    let line_start = tv.rope.line_to_char(line_index);
    let col_index = cursor_pos - line_start;

    let x = viewport_offset.screen_offset.x
        + vp.text_area_left
        + (col_index as f32 * font.char_width);
    let y = viewport_offset.screen_offset.y
        + vp.text_area_top
        + ((line_index as f32 - tv.scroll_offset / font.line_height) * font.line_height)
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
    query: Query<(&TextViewState, &TextViewViewport), With<CodeEditor>>,
    font: Res<FontSettings>,
    viewport_offset: Res<LspEguiViewportOffset>,
) {
    let Ok((tv, vp)) = query.single() else { return };

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
        .min(tv.rope.len_chars());
    let line_index = tv.rope.char_to_line(trigger_char_index);
    let line_start = tv.rope.line_to_char(line_index);
    let col_index = trigger_char_index - line_start;

    let x = viewport_offset.screen_offset.x
        + vp.text_area_left
        + (col_index as f32 * font.char_width);
    let y = viewport_offset.screen_offset.y
        + vp.text_area_top
        + ((line_index as f32 - tv.scroll_offset / font.line_height) * font.line_height)
        + font.line_height;

    let theme = ctx.armas_theme();

    // Estimate hover popup size (max 500px wide, ~line_height * line_count tall)
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
pub fn render_signature_help_egui(
    mut contexts: EguiContexts,
    sig_state: Res<SignatureHelpState>,
    query: Query<(&CodeEditorState, &CursorState, &TextViewState, &TextViewViewport), With<CodeEditor>>,
    font: Res<FontSettings>,
    viewport_offset: Res<LspEguiViewportOffset>,
) {
    let Ok((editor_state, cursor_state, tv, vp)) = query.single() else { return };

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

    let cursor_pos = cursor_state.cursor_pos.min(tv.rope.len_chars());
    let line_index = tv.rope.char_to_line(cursor_pos);
    let line_start = tv.rope.line_to_char(line_index);
    let col_index = cursor_pos - line_start;

    // Position ABOVE the cursor line
    let x = viewport_offset.screen_offset.x
        + vp.text_area_left
        + (col_index as f32 * font.char_width);
    let y = viewport_offset.screen_offset.y
        + vp.text_area_top
        + ((line_index as f32 - tv.scroll_offset / font.line_height) * font.line_height)
        - font.line_height * 1.5;

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

    // Resolve active parameter offsets in the signature label for highlighting
    let active_param_range: Option<(usize, usize)> = signature
        .parameters
        .as_ref()
        .and_then(|params| params.get(sig_state.active_parameter))
        .and_then(|param| match &param.label {
            lsp_types::ParameterLabel::LabelOffsets([start, end]) => {
                Some((*start as usize, *end as usize))
            }
            lsp_types::ParameterLabel::Simple(s) => {
                // Find the substring in the signature label
                signature.label.find(s.as_str()).map(|pos| (pos, pos + s.len()))
            }
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
                            // Render in three segments: before, active param (bold), after
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
                            // No active parameter — show full label plainly
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
pub fn render_code_actions_egui(
    mut contexts: EguiContexts,
    action_state: Res<CodeActionState>,
    query: Query<(&CodeEditorState, &CursorState, &TextViewState, &TextViewViewport), With<CodeEditor>>,
    font: Res<FontSettings>,
    viewport_offset: Res<LspEguiViewportOffset>,
) {
    let Ok((editor_state, cursor_state, tv, vp)) = query.single() else { return };

    if !action_state.visible || action_state.actions.is_empty() {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let cursor_pos = cursor_state.cursor_pos.min(tv.rope.len_chars());
    let line_index = tv.rope.char_to_line(cursor_pos);

    // Position near the gutter
    let x = viewport_offset.screen_offset.x + vp.text_area_left - 20.0;
    let y = viewport_offset.screen_offset.y
        + vp.text_area_top
        + ((line_index as f32 - tv.scroll_offset / font.line_height + 1.0)
            * font.line_height);

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

/// Render the rename input as an interactive egui overlay.
pub fn render_rename_egui(
    mut contexts: EguiContexts,
    rename_state: Res<RenameState>,
    query: Query<(&TextViewState, &TextViewViewport), With<CodeEditor>>,
    font: Res<FontSettings>,
    viewport_offset: Res<LspEguiViewportOffset>,
) {
    let Ok((tv, vp)) = query.single() else { return };

    if !rename_state.visible {
        return;
    }

    let range = match rename_state.range.clone() {
        Some(r) => r,
        None => return,
    };

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // Convert range start to char index for positioning
    let line = range.start.line as usize;
    let character = range.start.character as usize;

    let x = viewport_offset.screen_offset.x
        + vp.text_area_left
        + (character as f32 * font.char_width);
    let y = viewport_offset.screen_offset.y
        + vp.text_area_top
        + ((line as f32 - tv.scroll_offset / font.line_height) * font.line_height);

    let theme = ctx.armas_theme();

    // We display the current rename text but since RenameState is not mutable here,
    // we show it as a read-only styled label. The actual editing happens through
    // the editor's keyboard input system which updates RenameState.
    let display_text = if rename_state.new_name.is_empty() {
        &rename_state.original_text
    } else {
        editor_state.rope.len_chars()
    };

    let (cursor_x, cursor_y) = cursor_screen_pos(
        char_index,
        &editor_state,
        &font,
        &viewport_offset,
        &viewport,
    );

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
        if let Some(uri) = &lsp_sync.document_uri {
            lsp_client.send(LspMessage::Rename {
                uri: uri.clone(),
                position: range.start,
                new_name: rename_state.new_name.clone(),
            });
        }
    }
}
