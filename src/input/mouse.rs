use bevy::prelude::*;
use bevy::input::mouse::MouseWheel;
use bevy::window::PrimaryWindow;
use crate::types::*;
use crate::settings::EditorSettings;

#[cfg(feature = "lsp")]
use crate::lsp::{LspMessage, reset_hover_state};

/// Mouse drag state for selection
#[derive(Resource, Default)]
pub struct MouseDragState {
    /// Whether we're currently dragging
    pub is_dragging: bool,
    /// Position where drag started (character index)
    pub drag_start_pos: Option<usize>,
}

/// Convert screen coordinates to character position in the editor
fn screen_to_char_pos(
    screen_pos: Vec2,
    state: &CodeEditorState,
    settings: &EditorSettings,
    _viewport_width: f32,
    _viewport_height: f32,
    offset_x: f32,
    fold_state: &FoldState,
) -> usize {
    // Calculate the clicked position relative to code start, accounting for sidebar offset
    // Note: scroll_offset is negative when scrolled down, and screen_pos.y is 0 at top in window coords
    // But Bevy's cursor_position() returns (0,0) at top-left, so we need to account for that
    let relative_x = screen_pos.x - settings.ui.layout.code_margin_left - offset_x;

    // scroll_offset is negative when scrolled, so -scroll_offset gives how many pixels we've scrolled
    // screen_pos.y starts at 0 at top of window
    let relative_y = screen_pos.y - settings.ui.layout.margin_top - state.scroll_offset;

    // Calculate line and column from pixel position
    let line_height = settings.font.line_height;
    let char_width = settings.font.size * 0.6; // Approximate monospace width

    let display_row = (relative_y / line_height).max(0.0) as usize;
    let col = (relative_x / char_width).max(0.0) as usize;

    // Convert display row to buffer line (accounting for folds)
    let buffer_line = fold_state.display_to_actual_line(display_row);

    // Convert line/col to character position
    let line_count = state.rope.len_lines();
    if buffer_line >= line_count {
        // Click below last line - go to end of document
        return state.rope.len_chars();
    }

    let line_start_char = state.rope.line_to_char(buffer_line);
    let line_len = state.rope.line(buffer_line).len_chars().saturating_sub(1); // Exclude newline
    let char_in_line = col.min(line_len);

    line_start_char + char_in_line
}

/// System to handle mouse input
pub fn handle_mouse_input(
    mut state: ResMut<CodeEditorState>,
    mut drag_state: ResMut<MouseDragState>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut fold_state: ResMut<FoldState>,
    #[cfg(feature = "lsp")] time: Res<Time>,
    #[cfg(feature = "lsp")] lsp_client: Res<crate::lsp::LspClient>,
    #[cfg(feature = "lsp")] lsp_sync: Res<crate::lsp::LspSyncState>,
    #[cfg(feature = "lsp")] mut hover_state: ResMut<crate::lsp::HoverState>,
) {
    // Get cursor position
    let cursor_pos_screen = window_query.iter().next()
        .and_then(|window| window.cursor_position());

    // Calculate char position if mouse is over the editor
    let char_pos = if let Some(cursor_pos_screen) = cursor_pos_screen {
        // Check if mouse is within the editor area
        // cursor_position() returns window coordinates: (0,0) at top-left, Y increases downward
        let viewport_width = viewport.width as f32;
        let viewport_height = viewport.height as f32;

        // Window coordinate bounds (0 to width, 0 to height)
        let mouse_in_editor_area = cursor_pos_screen.x >= 0.0 && cursor_pos_screen.x <= viewport_width &&
                                 cursor_pos_screen.y >= 0.0 && cursor_pos_screen.y <= viewport_height;

        if mouse_in_editor_area {
            Some(screen_to_char_pos(
                cursor_pos_screen,
                &state,
                &settings,
                viewport_width,
                viewport_height,
                viewport.offset_x,
                &fold_state,
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
        if settings.hover.enabled {
            if let Some(current_char_pos) = char_pos {
                // If mouse moved to a different character
                if hover_state.trigger_char_index != current_char_pos {
                    hover_state.trigger_char_index = current_char_pos;
                    // Use delay_ms from settings
                    hover_state.timer = Some(Timer::new(
                        std::time::Duration::from_millis(settings.hover.delay_ms),
                        TimerMode::Once
                    ));
                    hover_state.visible = false; // Hide previous hover immediately
                    hover_state.request_sent = false; // Reset request flag
                }

                // If timer finished and we haven't sent a request yet, request hover
                if let Some(timer) = &mut hover_state.timer {
                    timer.tick(time.delta());
                    if timer.just_finished() && !hover_state.request_sent {
                        let line_index = state.rope.char_to_line(current_char_pos);
                        let line_start = state.rope.line_to_char(line_index);
                        let line_len = state.rope.line(line_index).len_chars();
                        // Clamp column to actual line length (excluding newline)
                        let char_in_line_index = (current_char_pos - line_start).min(line_len.saturating_sub(1));

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
                            hover_state.pending_char_index = Some(current_char_pos); // Remember which position we requested
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
            let line_height = settings.font.line_height;

            // Fold gutter is a narrow area just before the separator (where fold indicators are)
            // Fold indicators are positioned at: separator_x - 12.0
            let gutter_start = settings.ui.layout.separator_x - 18.0;
            let gutter_end = settings.ui.layout.separator_x + 5.0;

            // Check if click is in the fold gutter area (horizontally)
            if cursor_pos_screen.x >= gutter_start && cursor_pos_screen.x < gutter_end {
                // Calculate which display row was clicked
                let relative_y = cursor_pos_screen.y - settings.ui.layout.margin_top + state.scroll_offset;
                let display_row = (relative_y / line_height).max(0.0) as usize;

                // Convert display row to buffer line
                let buffer_line = fold_state.display_to_actual_line(display_row);

                // Check if there's a foldable region starting at this line
                if fold_state.is_foldable_line(buffer_line) {
                    fold_state.toggle_fold_at_line(buffer_line);
                    state.pending_update = true;
                    state.is_focused = true;

                    // Hide hover on click
                    #[cfg(feature = "lsp")]
                    reset_hover_state(&mut hover_state);

                    return; // Consume the click
                }
            }
        }

        if let Some(char_pos) = char_pos {
            // Focus editor on click
            state.is_focused = true;

            #[cfg(feature = "lsp")]
            {
                // Go to definition on Ctrl + Click
                if keyboard_input.pressed(KeyCode::ControlLeft) || keyboard_input.pressed(KeyCode::ControlRight) {
                    use lsp_types::Position;

                    let line_index = state.rope.char_to_line(char_pos);
                    let char_in_line_index = char_pos - state.rope.line_to_char(line_index);

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
            let alt_pressed = keyboard_input.pressed(KeyCode::AltLeft) || keyboard_input.pressed(KeyCode::AltRight);

            if alt_pressed {
                // Add cursor at clicked position
                state.sync_cursors_from_primary();
                state.add_cursor(char_pos);
                // Hide hover on click
                #[cfg(feature = "lsp")]
                reset_hover_state(&mut hover_state);
                return;
            }

            // Start drag
            drag_state.is_dragging = true;
            drag_state.drag_start_pos = Some(char_pos);

            // Clear secondary cursors on regular click
            if state.has_multiple_cursors() {
                state.clear_secondary_cursors();
            }

            // Update cursor and clear selection
            state.cursor_pos = char_pos;
            state.selection_start = None;
            state.selection_end = None;
            state.sync_cursors_from_primary();
            state.pending_update = true;

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
        if let (Some(cursor_pos_screen), Some(start_pos)) = (cursor_pos_screen, drag_state.drag_start_pos) {
            let current_pos = screen_to_char_pos(
                cursor_pos_screen,
                &state,
                &settings,
                viewport.width as f32,
                viewport.height as f32,
                viewport.offset_x,
                &fold_state,
            );

            // Only update if position changed
            if current_pos != state.cursor_pos {
                state.cursor_pos = current_pos;
                state.selection_start = Some(start_pos);
                state.selection_end = Some(current_pos);
                state.pending_update = true;
            }
        }
    }
}

/// System to handle mouse wheel scrolling
pub fn handle_mouse_wheel(
    mut state: ResMut<CodeEditorState>,
    mut mouse_wheel_events: MessageReader<MouseWheel>,
    _keyboard: Res<ButtonInput<KeyCode>>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
) {
    for event in mouse_wheel_events.read() {
        let mut scrolled = false;
        let use_smooth = settings.scrolling.smooth_scrolling;

        // Horizontal scrolling (using event.x)
        if event.x.abs() > 0.0 {
            // Only allow horizontal scrolling if content width exceeds available text area
            let viewport_width = viewport.width as f32;
            // Calculate available width for text (excluding line numbers margin and code margin)
            let available_text_width = viewport_width - settings.ui.layout.code_margin_left;

            if state.max_content_width > available_text_width {
                // Positive x = scroll right (content moves left, horizontal_scroll_offset increases)
                // Negative x = scroll left (content moves right, horizontal_scroll_offset decreases)
                let scroll_delta = event.x * settings.font.char_width * settings.scrolling.speed;

                if use_smooth {
                    // Update target for smooth scrolling
                    state.target_horizontal_scroll_offset += scroll_delta;
                } else {
                    // Direct update
                    state.horizontal_scroll_offset += scroll_delta;
                }

                // Clamp horizontal scroll:
                // Minimum is 0 (can't scroll left past column 0)
                let max_horizontal_scroll = (state.max_content_width - available_text_width).max(0.0);

                if use_smooth {
                    state.target_horizontal_scroll_offset = state.target_horizontal_scroll_offset
                        .max(0.0)
                        .min(max_horizontal_scroll);
                } else {
                    state.horizontal_scroll_offset = state.horizontal_scroll_offset
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
            let scroll_delta = event.y * settings.font.line_height * settings.scrolling.speed;

            // Calculate scroll bounds
            let line_count = state.rope.len_lines();
            let content_height = line_count as f32 * settings.font.line_height;
            let viewport_height = viewport.height as f32;
            let max_scroll = -(content_height - viewport_height + settings.ui.layout.margin_top);

            if use_smooth {
                // Update target for smooth scrolling
                state.target_scroll_offset += scroll_delta;
                state.target_scroll_offset = state.target_scroll_offset
                    .min(0.0)
                    .max(max_scroll.min(0.0));
            } else {
                // Direct update
                state.scroll_offset += scroll_delta;
                state.scroll_offset = state.scroll_offset
                    .min(0.0)
                    .max(max_scroll.min(0.0));
            }

            scrolled = true;
        }

        if scrolled {
            // Horizontal scrolling requires full update (text content changes due to culling)
            // Vertical scrolling only needs transform updates
            if event.x.abs() > 0.0 {
                state.needs_update = true;
            } else {
                state.needs_scroll_update = true;
            }
        }
    }
}
