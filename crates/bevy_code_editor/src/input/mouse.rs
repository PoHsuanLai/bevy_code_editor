use crate::settings::*;
use crate::text_view::{TextViewState, TextViewViewport};
use crate::types::*;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ropey::Rope;

#[cfg(feature = "lsp")]
use crate::lsp::{reset_hover_state, LspMessage};

/// Mouse drag state for selection
#[derive(Resource, Default)]
pub struct MouseDragState {
    /// Whether we're currently dragging
    pub is_dragging: bool,
    /// Position where drag started (character index)
    pub drag_start_pos: Option<usize>,
    /// Scroll offset when drag started (to prevent auto-scroll from affecting selection)
    pub drag_start_scroll_offset: f32,
    /// Last screen position of the mouse (to detect actual mouse movement vs scroll changes)
    pub last_screen_pos: Option<Vec2>,
}

/// Convert screen coordinates to character position in the editor
/// screen_pos should already be in viewport-local coordinates (0,0 at top-left of viewport)
/// scroll_offset_override: if provided, use this instead of current scroll_offset (for stable drag selection)
fn screen_to_char_pos(
    screen_pos: Vec2,
    rope: &Rope,
    current_scroll_offset: f32,
    font: &FontSettings,
    viewport: &TextViewViewport,
    fold_state: &FoldState,
    scroll_offset_override: Option<f32>,
) -> usize {
    // Calculate the clicked position relative to code start
    // Note: screen_pos is already viewport-local, so no offset_x needed
    // scroll_offset is negative when scrolled down
    let relative_x = screen_pos.x - viewport.text_area_left;

    // Use override if provided (for drag operations), otherwise use current scroll
    let scroll_offset = scroll_offset_override.unwrap_or(current_scroll_offset);

    // scroll_offset is negative when scrolled, so -scroll_offset gives how many pixels we've scrolled
    // screen_pos.y starts at 0 at top of viewport
    let relative_y = screen_pos.y - viewport.text_area_top - scroll_offset;

    // Calculate line and column from pixel position
    let line_height = font.line_height;
    let char_width = font.size * 0.6; // Approximate monospace width

    let display_row = (relative_y / line_height).max(0.0) as usize;
    let col = (relative_x / char_width).max(0.0) as usize;

    // Convert display row to buffer line (accounting for folds)
    let buffer_line = fold_state.display_to_actual_line(display_row);

    // Convert line/col to character position
    let line_count = rope.len_lines();
    if buffer_line >= line_count {
        // Click below last line - go to end of document
        return rope.len_chars();
    }

    let line_start_char = rope.line_to_char(buffer_line);
    let line_len = rope.line(buffer_line).len_chars().saturating_sub(1); // Exclude newline
    let char_in_line = col.min(line_len);

    line_start_char + char_in_line
}

/// System to handle mouse input
pub fn handle_mouse_input(
    mut editor_query: Query<
        (
            &mut CodeEditorState,
            &mut SelectionState,
            &mut CursorState,
            &mut TextViewState,
            &TextViewViewport,
            &mut FoldState,
        ),
        With<CodeEditor>,
    >,
    mut drag_state: ResMut<MouseDragState>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    font: Res<FontSettings>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    #[cfg(feature = "lsp")] time: Res<Time>,
    #[cfg(feature = "lsp")] lsp_client: Res<crate::lsp::LspClient>,
    #[cfg(feature = "lsp")] lsp_sync: Res<crate::lsp::LspSyncState>,
    #[cfg(feature = "lsp")] mut hover_state: ResMut<crate::lsp::HoverState>,
    #[cfg(feature = "lsp")] hover_settings: Res<crate::settings::LspSettings>,
) {
    let Ok((mut state, mut sel, mut cursor, mut tv, viewport, mut fold_state)) =
        editor_query.single_mut()
    else {
        return;
    };

    // Get cursor position
    let cursor_pos_screen = window_query
        .iter()
        .next()
        .and_then(|window| window.cursor_position());

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
                &tv.rope,
                tv.scroll_offset,
                &font,
                viewport,
                &fold_state,
                None, // Use current scroll offset for initial position calculation
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
        use crate::lsp::reset_hover_state;
        use lsp_types::Position;

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
                        let line_index = tv.rope.char_to_line(current_char_pos);
                        let line_start = tv.rope.line_to_char(line_index);
                        let line_len = tv.rope.line(line_index).len_chars();
                        // Clamp column to actual line length (excluding newline)
                        let char_in_line_index =
                            (current_char_pos - line_start).min(line_len.saturating_sub(1));

                        let lsp_position = Position {
                            line: line_index as u32,
                            character: char_in_line_index as u32,
                        };

                        if let Some(uri) = &lsp_sync.document_uri {
                            lsp_client.send(LspMessage::Hover {
                                uri: uri.clone(),
                                position: lsp_position,
                            });
                            hover_state.request_sent = true;
                            hover_state.pending_char_index = Some(current_char_pos);
                            // Remember which position we requested
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
                        state.is_focused = true;

                        // Hide hover on click
                        #[cfg(feature = "lsp")]
                        reset_hover_state(&mut hover_state);

                        return; // Consume the click
                    }
                }
            }
        }

        if let Some(char_pos) = char_pos {
            // Focus editor on click
            state.is_focused = true;

            #[cfg(feature = "lsp")]
            {
                // Go to definition on Ctrl + Click
                if keyboard_input.pressed(KeyCode::ControlLeft)
                    || keyboard_input.pressed(KeyCode::ControlRight)
                {
                    use lsp_types::Position;

                    let line_index = tv.rope.char_to_line(char_pos);
                    let char_in_line_index = char_pos - tv.rope.line_to_char(line_index);

                    let lsp_position = Position {
                        line: line_index as u32,
                        character: char_in_line_index as u32,
                    };

                    if let Some(uri) = &lsp_sync.document_uri {
                        lsp_client.send(LspMessage::GotoDefinition {
                            uri: uri.clone(),
                            position: lsp_position,
                        });
                    }
                    return; // Consume the click, don't start drag or move cursor normally
                }
            }

            // Check for Alt+Click to add a new cursor
            let alt_pressed = keyboard_input.pressed(KeyCode::AltLeft)
                || keyboard_input.pressed(KeyCode::AltRight);

            if alt_pressed {
                // Add cursor at clicked position
                sel.sync_cursors_from_primary(&mut cursor);
                sel.add_cursor(&mut cursor, &mut tv, char_pos);
                // Hide hover on click
                #[cfg(feature = "lsp")]
                reset_hover_state(&mut hover_state);
                return;
            }

            // Start drag - capture scroll offset and screen position to prevent auto-scroll from affecting selection
            drag_state.is_dragging = true;
            drag_state.drag_start_pos = Some(char_pos);
            drag_state.drag_start_scroll_offset = tv.scroll_offset;
            drag_state.last_screen_pos = cursor_pos_screen;

            // Clear secondary cursors on regular click
            if sel.has_multiple_cursors(&cursor) {
                sel.clear_secondary_cursors(&mut cursor, &mut tv);
            }

            // Update cursor and clear selection
            cursor.cursor_pos = char_pos;
            sel.selection_start = None;
            sel.selection_end = None;
            sel.sync_cursors_from_primary(&mut cursor);

            // Hide hover on click
            #[cfg(feature = "lsp")]
            reset_hover_state(&mut hover_state);
        } else {
            // Clicked outside editor, lose focus
            state.is_focused = false;
        }
    }

    // Handle mouse button release
    if mouse_button.just_released(MouseButton::Left) {
        drag_state.is_dragging = false;
        drag_state.drag_start_pos = None;
    }

    // Handle dragging (mouse held and moving)
    if drag_state.is_dragging && mouse_button.pressed(MouseButton::Left) {
        if let (Some(cursor_pos_screen), Some(start_pos)) =
            (cursor_pos_screen, drag_state.drag_start_pos)
        {
            // Only process drag if mouse actually moved (prevents auto-scroll from creating selections)
            let mouse_moved = drag_state
                .last_screen_pos
                .map(|last| (cursor_pos_screen - last).length() > 2.0)
                .unwrap_or(false);

            if mouse_moved {
                // Update last screen position
                drag_state.last_screen_pos = Some(cursor_pos_screen);

                let _viewport_width = viewport.width as f32;
                let _viewport_height = viewport.height as f32;
                let viewport_local_pos = Vec2::new(
                    cursor_pos_screen.x - viewport.hit_test_position.x,
                    cursor_pos_screen.y - viewport.hit_test_position.y,
                );

                // Use the scroll offset from drag start to prevent auto-scroll from affecting selection
                let current_pos = screen_to_char_pos(
                    viewport_local_pos,
                    &tv.rope,
                    tv.scroll_offset,
                    &font,
                    viewport,
                    &fold_state,
                    Some(drag_state.drag_start_scroll_offset),
                );

                // Only update if position changed
                if current_pos != cursor.cursor_pos {
                    cursor.cursor_pos = current_pos;
                    sel.selection_start = Some(start_pos);
                    sel.selection_end = Some(current_pos);
                }
            }
        }
    }
}

/// System to handle mouse wheel scrolling
pub fn handle_mouse_wheel(
    mut editor_query: Query<
        (
            &mut CodeEditorState,
            &mut SelectionState,
            &mut CursorState,
            &mut TextViewState,
            &TextViewViewport,
        ),
        With<CodeEditor>,
    >,
    mut mouse_wheel_events: MessageReader<MouseWheel>,
    _keyboard: Res<ButtonInput<KeyCode>>,
    font: Res<FontSettings>,
    scrolling: Res<ScrollingSettings>,
) {
    let Ok((_state, _sel, _cursor, mut tv, viewport)) = editor_query.single_mut() else {
        return;
    };

    for event in mouse_wheel_events.read() {
        let mut scrolled = false;
        let use_smooth = scrolling.smooth;

        // Horizontal scrolling (using event.x)
        if event.x.abs() > 0.0 {
            // Only allow horizontal scrolling if content width exceeds available text area
            let viewport_width = viewport.width as f32;
            // Calculate available width for text (excluding line numbers margin and code margin)
            let available_text_width = viewport_width - viewport.text_area_left;

            if tv.max_content_width > available_text_width {
                // Positive x = scroll right (content moves left, horizontal_scroll_offset increases)
                // Negative x = scroll left (content moves right, horizontal_scroll_offset decreases)
                let scroll_delta = event.x * font.char_width * scrolling.speed;

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
            let scroll_delta = event.y * font.line_height * scrolling.speed;

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

        if scrolled {
            // Horizontal scrolling requires full update (text content changes due to culling)
            // Vertical scrolling only needs transform updates
            if event.x.abs() > 0.0 {
            } else {
            }
        }
    }
}
