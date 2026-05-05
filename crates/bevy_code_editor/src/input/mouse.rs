use crate::text_view::{TextViewState, TextViewViewport};
use crate::types::*;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_text_engine::{DisplayLayout, FontConfig};
use bevy_text_editor::ScrollConfig;
use ropey::Rope;

#[cfg(feature = "lsp")]
use crate::lsp_ui::reset_hover_state;
#[cfg(feature = "lsp")]
use bevy_lsp::LspMessage;

/// Read-only context needed to translate a viewport-local pixel position to
/// a character position in the rope. Bundled because the two call sites in
/// `handle_mouse_input` thread the same set of refs through.
struct HitTestCtx<'a> {
    rope: &'a Rope,
    layout: Option<&'a DisplayLayout>,
    font: &'a FontConfig,
    viewport: &'a TextViewViewport,
    fold_state: &'a FoldState,
    /// Live scroll offset (read from `TextViewState`).
    current_scroll_offset: f32,
    /// When `Some`, overrides `current_scroll_offset` — used during drag
    /// selection to anchor the hit-test to the offset at drag-start, so
    /// auto-scroll doesn't drift the selection origin mid-drag.
    scroll_offset_override: Option<f32>,
}

/// Convert screen coordinates to character position in the editor.
///
/// `screen_pos` is viewport-local (0,0 at top-left). The layout in `ctx`,
/// when present, is consulted for shaped per-glyph hit-testing — this is
/// what makes proportional fonts click correctly.
fn screen_to_char_pos(screen_pos: Vec2, ctx: &HitTestCtx<'_>) -> usize {
    let relative_x = screen_pos.x - ctx.viewport.text_area_left;
    let scroll_offset = ctx
        .scroll_offset_override
        .unwrap_or(ctx.current_scroll_offset);
    // scroll_offset is negative when scrolled down; subtracting flips it positive
    // so y=0 is the document origin (i.e. display row 0 is at relative_y == 0).
    let relative_y = screen_pos.y - ctx.viewport.text_area_top - scroll_offset;

    let display_row = (relative_y / ctx.font.line_height).max(0.0) as usize;
    let buffer_line = ctx.fold_state.display_to_actual_line(display_row);

    let line_count = ctx.rope.len_lines();
    if buffer_line >= line_count {
        return ctx.rope.len_chars();
    }

    let line_start_char = ctx.rope.line_to_char(buffer_line);

    // Shaped path: ask the layout where pixel `relative_x` falls inside the
    // display row. With soft wrap, the row's `text` is a slice of the buffer
    // line starting at `buffer_byte_offset`; we translate the row-local byte
    // offset back to a buffer-line byte using that field.
    if let Some(layout) = ctx.layout {
        if let Some(byte_in_row) = layout.byte_at_x(display_row as u32, relative_x) {
            let row = layout
                .lines
                .iter()
                .find(|l| l.display_row == display_row as u32);
            let row_buffer_line = row.map(|r| r.buffer_row as usize).unwrap_or(buffer_line);
            let buffer_byte_offset = row.map(|r| r.buffer_byte_offset).unwrap_or(0);
            let line_start_byte = ctx.rope.line_to_byte(row_buffer_line);
            let line_end_byte = if row_buffer_line + 1 < line_count {
                ctx.rope.line_to_byte(row_buffer_line + 1)
            } else {
                ctx.rope.len_bytes()
            };
            let abs_byte =
                (line_start_byte + buffer_byte_offset + byte_in_row).min(line_end_byte);
            return ctx.rope.byte_to_char(abs_byte);
        }
    }

    let col = (relative_x / ctx.font.char_width).max(0.0) as usize;
    let line_len = ctx.rope.line(buffer_line).len_chars().saturating_sub(1);
    let char_in_line = col.min(line_len);
    line_start_char + char_in_line
}

/// System to handle mouse input
#[allow(clippy::too_many_arguments)]
pub fn handle_mouse_input(
    mut editor_query: Query<
        (
            Entity,
            &mut SelectionState,
            &mut CursorState,
            &mut TextViewState,
            &TextViewViewport,
            &mut FoldState,
            &FontConfig,
            Option<&DisplayLayout>,
        ),
        With<CodeEditor>,
    >,
    #[cfg(feature = "lsp")] mut lsp_query: Query<
        (
            Entity,
            &bevy_lsp::LspClient,
            Option<&bevy_lsp::LspDocument>,
            &mut crate::lsp_ui::state::LspHoverPopup,
        ),
        With<CodeEditor>,
    >,
    mut input_focus: ResMut<bevy::input_focus::InputFocus>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    #[cfg(feature = "lsp")] time: Res<Time>,
    #[cfg(feature = "lsp")] hover_settings: Res<crate::settings::LspSettings>,
) {
    // Get cursor position
    let cursor_pos_screen = window_query
        .iter()
        .next()
        .and_then(|window| window.cursor_position());

    for (
        editor_entity,
        mut sel,
        mut cursor,
        tv,
        viewport,
        mut fold_state,
        font,
        layout,
    ) in editor_query.iter_mut()
    {
        // LSP-side state for this editor (separate query because the main
        // editor_query already exceeds Bevy's filter tuple size with the
        // LSP feature on).
        #[cfg(feature = "lsp")]
        let Ok((_, lsp_client, lsp_document, mut hover_state)) =
            lsp_query.get_mut(editor_entity)
        else {
            continue;
        };

    // Calculate char position if mouse is over the editor
    let char_pos = if let Some(cursor_pos_screen) = cursor_pos_screen {
        let viewport_width = viewport.width as f32;
        let viewport_height = viewport.height as f32;
        let viewport_left = viewport.hit_test_position.x;
        let viewport_top = viewport.hit_test_position.y;
        let viewport_right = viewport_left + viewport_width;
        let viewport_bottom = viewport_top + viewport_height;

        // Check if mouse is within the editor viewport area
        let mouse_in_editor_area = cursor_pos_screen.x >= viewport_left
            && cursor_pos_screen.x <= viewport_right
            && cursor_pos_screen.y >= viewport_top
            && cursor_pos_screen.y <= viewport_bottom;

        if mouse_in_editor_area {
            let viewport_local_pos = Vec2::new(
                cursor_pos_screen.x - viewport_left,
                cursor_pos_screen.y - viewport_top,
            );

            Some(screen_to_char_pos(
                viewport_local_pos,
                &HitTestCtx {
                    rope: &tv.rope,
                    layout: layout.as_deref(),
                    font,
                    viewport,
                    fold_state: &fold_state,
                    current_scroll_offset: tv.scroll_offset,
                    scroll_offset_override: None,
                },
            ))
        } else {
            None
        }
    } else {
        None
    };

    // --- LSP Hover logic ---
    #[cfg(feature = "lsp")]
    {
        // Only process hover if enabled in settings
        if hover_settings.hover.enabled {
            if let Some(current_char_pos) = char_pos {
                // If mouse moved to a different character
                if hover_state.trigger_char_index != current_char_pos {
                    hover_state.trigger_char_index = current_char_pos;
                    // Use delay_ms from settings
                    hover_state.timer = Some(Timer::new(
                        std::time::Duration::from_millis(hover_settings.hover.delay_ms),
                        TimerMode::Once,
                    ));
                    hover_state.visible = false; // Hide previous hover immediately
                    hover_state.request_sent = false; // Reset request flag
                }

                // If timer finished and we haven't sent a request yet, request hover
                if let Some(timer) = &mut hover_state.timer {
                    timer.tick(time.delta());
                    if timer.just_finished() && !hover_state.request_sent {
                        // Clamp to last char of line (exclude newline) so the
                        // server doesn't see a position past the line end.
                        let line_index = tv.rope.char_to_line(current_char_pos);
                        let line_start = tv.rope.line_to_char(line_index);
                        let line_len = tv.rope.line(line_index).len_chars();
                        let clamped = line_start
                            + (current_char_pos - line_start).min(line_len.saturating_sub(1));
                        let lsp_position = bevy_lsp::rope_char_to_lsp_position(
                            &tv.rope,
                            clamped,
                            bevy_lsp::PositionEncoding::Utf16,
                        );

                        if let Some(doc) = lsp_document {
                            lsp_client.send(LspMessage::Hover {
                                uri: doc.uri.clone(),
                                position: lsp_position,
                            });
                            hover_state.request_sent = true;
                            hover_state.pending_char_index = Some(current_char_pos);
                        }
                    }
                }
            } else {
                // Mouse is not over the editor, reset hover
                reset_hover_state(&mut hover_state);
            }
        } else {
            // Hover disabled - ensure it's hidden
            reset_hover_state(&mut hover_state);
        }
    }

    // Handle mouse button press
    if mouse_button.just_pressed(MouseButton::Left) {
        // Check for fold indicator click (in the fold gutter area)
        if let Some(cursor_pos_screen) = cursor_pos_screen {
            let _viewport_width = viewport.width as f32;
            let _viewport_height = viewport.height as f32;
            let line_height = font.line_height;

            let viewport_left = viewport.hit_test_position.x;
            let viewport_top = viewport.hit_test_position.y;

            let local_x = cursor_pos_screen.x - viewport_left;
            let local_y = cursor_pos_screen.y - viewport_top;

            // Fold gutter is a narrow area just before the separator (where fold indicators are)
            // Fold indicators are positioned at: separator_x - 12.0
            {
                let gutter_start = viewport.separator_x - 18.0;
                let gutter_end = viewport.separator_x + 5.0;

                // Check if click is in the fold gutter area (horizontally) using viewport-local coords
                if local_x >= gutter_start && local_x < gutter_end {
                    // Calculate which display row was clicked
                    let relative_y = local_y - viewport.text_area_top + tv.scroll_offset;
                    let display_row = (relative_y / line_height).max(0.0) as usize;

                    // Convert display row to buffer line
                    let buffer_line = fold_state.display_to_actual_line(display_row);

                    // Check if there's a foldable region starting at this line
                    if fold_state.is_foldable_line(buffer_line) {
                        fold_state.toggle_fold_at_line(buffer_line);
                        input_focus.set(editor_entity);

                        // Hide hover on click
                        #[cfg(feature = "lsp")]
                        reset_hover_state(&mut hover_state);

                        continue; // Consume the click
                    }
                }
            }
        }

        if let Some(char_pos) = char_pos {
            // Focus editor on click
            input_focus.set(editor_entity);

            #[cfg(feature = "lsp")]
            {
                // Go to definition on Ctrl + Click
                if keyboard_input.pressed(KeyCode::ControlLeft)
                    || keyboard_input.pressed(KeyCode::ControlRight)
                {
                    let lsp_position = bevy_lsp::rope_char_to_lsp_position(
                        &tv.rope,
                        char_pos,
                        bevy_lsp::PositionEncoding::Utf16,
                    );
                    if let Some(doc) = lsp_document {
                        lsp_client.send(LspMessage::GotoDefinition {
                            uri: doc.uri.clone(),
                            position: lsp_position,
                        });
                    }
                    continue; // Consume the click, don't start drag or move cursor normally
                }
            }

            // Alt+click: add a new secondary cursor at the clicked position.
            // The plain-click cursor move is handled by bevy_text_editor's
            // `on_pointer_press` observer, which skips writing selection when
            // a modifier (Alt / Ctrl / Cmd) is held — so we own this branch
            // exclusively.
            let alt_pressed = keyboard_input.pressed(KeyCode::AltLeft)
                || keyboard_input.pressed(KeyCode::AltRight);

            if alt_pressed {
                sel.add_cursor_at(&tv, char_pos);
                sel.refresh_primary_cursor(&mut cursor);
                #[cfg(feature = "lsp")]
                reset_hover_state(&mut hover_state);
                continue;
            }

            // Plain-click cursor placement and drag-select are owned by
            // `bevy_text_editor::interaction::on_pointer_press` /
            // `on_pointer_drag`. We just hide the LSP hover on click here.
            #[cfg(feature = "lsp")]
            reset_hover_state(&mut hover_state);
        } else {
            // Clicked outside any editor, lose focus only if this editor was the focused one.
            if input_focus.get() == Some(editor_entity) {
                input_focus.clear();
            }
        }
    }
    }
}

/// System to handle mouse wheel scrolling
pub fn handle_mouse_wheel(
    mut editor_query: Query<
        (
            &mut SelectionState,
            &mut CursorState,
            &mut TextViewState,
            &TextViewViewport,
            &FontConfig,
            &ScrollConfig,
        ),
        With<CodeEditor>,
    >,
    mut mouse_wheel_events: MessageReader<MouseWheel>,
    _keyboard: Res<ButtonInput<KeyCode>>,
) {
    let events: Vec<_> = mouse_wheel_events.read().copied().collect();
    for (_sel, _cursor, mut tv, viewport, font, scroll_cfg) in editor_query.iter_mut() {
    for event in events.iter() {
        let mut scrolled = false;
        let use_smooth = scroll_cfg.smooth;

        // Horizontal scrolling (using event.x)
        if event.x.abs() > 0.0 {
            // Only allow horizontal scrolling if content width exceeds available text area
            let viewport_width = viewport.width as f32;
            // Calculate available width for text (excluding line numbers margin and code margin)
            let available_text_width = viewport_width - viewport.text_area_left;

            if tv.max_content_width > available_text_width {
                // Positive x = scroll right (content moves left, horizontal_scroll_offset increases)
                // Negative x = scroll left (content moves right, horizontal_scroll_offset decreases)
                let scroll_delta = event.x * font.char_width * scroll_cfg.speed;

                if use_smooth {
                    // Update target for smooth scrolling
                    tv.target_horizontal_scroll_offset += scroll_delta;
                } else {
                    // Direct update
                    tv.horizontal_scroll_offset += scroll_delta;
                }

                // Clamp horizontal scroll:
                // Minimum is 0 (can't scroll left past column 0)
                let max_horizontal_scroll = (tv.max_content_width - available_text_width).max(0.0);

                if use_smooth {
                    tv.target_horizontal_scroll_offset = tv
                        .target_horizontal_scroll_offset
                        .max(0.0)
                        .min(max_horizontal_scroll);
                } else {
                    tv.horizontal_scroll_offset = tv
                        .horizontal_scroll_offset
                        .max(0.0)
                        .min(max_horizontal_scroll);
                }

                scrolled = true;
            }
        }

        // Vertical scrolling (using event.y)
        if event.y.abs() > 0.0 {
            // Positive y = scroll up (content moves down, scroll_offset increases)
            // Negative y = scroll down (content moves up, scroll_offset decreases)
            let scroll_delta = event.y * font.line_height * scroll_cfg.speed;

            // Calculate scroll bounds
            let line_count = tv.rope.len_lines();
            let content_height = line_count as f32 * font.line_height;
            let viewport_height = viewport.height as f32;
            let max_scroll = -(content_height - viewport_height + viewport.text_area_top);

            if use_smooth {
                // Update target for smooth scrolling
                tv.target_scroll_offset += scroll_delta;
                tv.target_scroll_offset = tv.target_scroll_offset.min(0.0).max(max_scroll.min(0.0));
            } else {
                // Direct update
                tv.scroll_offset += scroll_delta;
                tv.scroll_offset = tv.scroll_offset.min(0.0).max(max_scroll.min(0.0));
            }

            scrolled = true;
        }

        // `scrolled` is intentionally unused — scroll-driven text reflow
        // happens through `DisplayLayout`'s `Changed` detection in the
        // display-map producer, not through any explicit per-event update
        // here.
        let _ = scrolled;
    }
    }
}
