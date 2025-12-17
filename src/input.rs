//! Input handling for the code editor
//!
//! This module provides keyboard and mouse input handling with support
//! for customizable keybindings.

use bevy::prelude::*;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::MouseWheel;
use bevy::window::PrimaryWindow;
use crate::types::*;
use crate::settings::EditorSettings;
#[cfg(feature = "lsp")]
use crate::lsp::{self, LspMessage, reset_hover_state}; // Import the lsp module and LspMessage
use std::collections::HashMap;

#[cfg(feature = "clipboard")]
use arboard::Clipboard;

/// Context for executing editor actions
pub struct ActionContext<'a> {
    pub state: &'a mut CodeEditorState,
    pub settings: &'a EditorSettings,
    #[cfg(feature = "lsp")]
    pub lsp_client: &'a lsp::LspClient,
    #[cfg(feature = "lsp")]
    pub completion_state: &'a mut lsp::CompletionState,
    #[cfg(feature = "lsp")]
    pub sync_state: &'a mut lsp::LspSyncState,
}

/// Editor action that can be triggered by keybindings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorAction {
    // Character insertion
    InsertChar,
    InsertNewline,
    InsertTab,

    // Deletion
    DeleteBackward,
    DeleteForward,
    DeleteWordBackward,
    DeleteWordForward,
    DeleteLine,

    // Cursor movement
    MoveCursorLeft,
    MoveCursorRight,
    MoveCursorUp,
    MoveCursorDown,
    MoveCursorWordLeft,
    MoveCursorWordRight,
    MoveCursorLineStart,
    MoveCursorLineEnd,
    MoveCursorDocumentStart,
    MoveCursorDocumentEnd,
    MoveCursorPageUp,
    MoveCursorPageDown,

    // Selection
    SelectLeft,
    SelectRight,
    SelectUp,
    SelectDown,
    SelectWordLeft,
    SelectWordRight,
    SelectLineStart,
    SelectLineEnd,
    SelectAll,
    ClearSelection,

    // Clipboard
    Copy,
    Cut,
    Paste,

    // Undo/Redo
    Undo,
    Redo,

    // Scrolling
    ScrollUp,
    ScrollDown,
    ScrollPageUp,
    ScrollPageDown,

    // Search
    Find,
    FindNext,
    FindPrevious,
    Replace,

    // LSP
    RequestCompletion,
    GotoDefinition,
}

/// Key combination for triggering actions
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyBinding {
    pub key: KeyCode,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

impl KeyBinding {
    /// Create a simple key binding (no modifiers)
    pub fn key(key: KeyCode) -> Self {
        Self {
            key,
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        }
    }

    /// Create a key binding with Ctrl
    pub fn ctrl(key: KeyCode) -> Self {
        Self {
            key,
            ctrl: true,
            shift: false,
            alt: false,
            meta: false,
        }
    }

    /// Create a key binding with Shift
    pub fn shift(key: KeyCode) -> Self {
        Self {
            key,
            ctrl: false,
            shift: true,
            alt: false,
            meta: false,
        }
    }

    /// Create a key binding with Ctrl+Shift
    pub fn ctrl_shift(key: KeyCode) -> Self {
        Self {
            key,
            ctrl: true,
            shift: true,
            alt: false,
            meta: false,
        }
    }

    /// Create a key binding with Alt
    pub fn alt(key: KeyCode) -> Self {
        Self {
            key,
            ctrl: false,
            shift: false,
            alt: true,
            meta: false,
        }
    }

    /// Check if current keyboard state matches this binding
    pub fn matches(&self, key: KeyCode, keyboard: &ButtonInput<KeyCode>) -> bool {
        if key != self.key {
            return false;
        }

        let ctrl_pressed = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
        let shift_pressed = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
        let alt_pressed = keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight);
        let meta_pressed = keyboard.pressed(KeyCode::SuperLeft) || keyboard.pressed(KeyCode::SuperRight);

        ctrl_pressed == self.ctrl
            && shift_pressed == self.shift
            && alt_pressed == self.alt
            && meta_pressed == self.meta
    }
}

/// Keybinding configuration
#[derive(Resource, Clone)]
pub struct Keybindings {
    bindings: HashMap<KeyBinding, EditorAction>,
}

/// Key repeat state for smooth navigation
#[derive(Resource)]
pub struct KeyRepeatState {
    /// Currently held key
    pub held_key: Option<KeyCode>,
    /// Time when key was first pressed
    pub press_time: f64,
    /// Time of last repeat
    pub last_repeat_time: f64,
}

impl Default for KeyRepeatState {
    fn default() -> Self {
        Self {
            held_key: None,
            press_time: 0.0,
            last_repeat_time: 0.0,
        }
    }
}

/// Mouse drag state for selection
#[derive(Resource, Default)]
pub struct MouseDragState {
    /// Whether we're currently dragging
    pub is_dragging: bool,
    /// Position where drag started (character index)
    pub drag_start_pos: Option<usize>,
}

impl Default for Keybindings {
    fn default() -> Self {
        Self::default_bindings()
    }
}

impl Keybindings {
    /// Create empty keybindings
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    /// Default VSCode-like keybindings
    pub fn default_bindings() -> Self {
        let mut bindings = HashMap::new();

        // Basic editing
        bindings.insert(KeyBinding::key(KeyCode::Backspace), EditorAction::DeleteBackward);
        bindings.insert(KeyBinding::key(KeyCode::Delete), EditorAction::DeleteForward);
        bindings.insert(KeyBinding::ctrl(KeyCode::Backspace), EditorAction::DeleteWordBackward);
        bindings.insert(KeyBinding::ctrl(KeyCode::Delete), EditorAction::DeleteWordForward);
        bindings.insert(KeyBinding::ctrl(KeyCode::KeyK), EditorAction::DeleteLine);
        bindings.insert(KeyBinding::key(KeyCode::Enter), EditorAction::InsertNewline);
        bindings.insert(KeyBinding::key(KeyCode::Tab), EditorAction::InsertTab);

        // Cursor movement
        bindings.insert(KeyBinding::key(KeyCode::ArrowLeft), EditorAction::MoveCursorLeft);
        bindings.insert(KeyBinding::key(KeyCode::ArrowRight), EditorAction::MoveCursorRight);
        bindings.insert(KeyBinding::key(KeyCode::ArrowUp), EditorAction::MoveCursorUp);
        bindings.insert(KeyBinding::key(KeyCode::ArrowDown), EditorAction::MoveCursorDown);
        bindings.insert(KeyBinding::ctrl(KeyCode::ArrowLeft), EditorAction::MoveCursorWordLeft);
        bindings.insert(KeyBinding::ctrl(KeyCode::ArrowRight), EditorAction::MoveCursorWordRight);
        bindings.insert(KeyBinding::key(KeyCode::Home), EditorAction::MoveCursorLineStart);
        bindings.insert(KeyBinding::key(KeyCode::End), EditorAction::MoveCursorLineEnd);
        bindings.insert(KeyBinding::ctrl(KeyCode::Home), EditorAction::MoveCursorDocumentStart);
        bindings.insert(KeyBinding::ctrl(KeyCode::End), EditorAction::MoveCursorDocumentEnd);
        bindings.insert(KeyBinding::key(KeyCode::PageUp), EditorAction::MoveCursorPageUp);
        bindings.insert(KeyBinding::key(KeyCode::PageDown), EditorAction::MoveCursorPageDown);

        // Selection
        bindings.insert(KeyBinding::shift(KeyCode::ArrowLeft), EditorAction::SelectLeft);
        bindings.insert(KeyBinding::shift(KeyCode::ArrowRight), EditorAction::SelectRight);
        bindings.insert(KeyBinding::shift(KeyCode::ArrowUp), EditorAction::SelectUp);
        bindings.insert(KeyBinding::shift(KeyCode::ArrowDown), EditorAction::SelectDown);
        bindings.insert(KeyBinding::ctrl_shift(KeyCode::ArrowLeft), EditorAction::SelectWordLeft);
        bindings.insert(KeyBinding::ctrl_shift(KeyCode::ArrowRight), EditorAction::SelectWordRight);
        bindings.insert(KeyBinding::shift(KeyCode::Home), EditorAction::SelectLineStart);
        bindings.insert(KeyBinding::shift(KeyCode::End), EditorAction::SelectLineEnd);
        bindings.insert(KeyBinding::ctrl(KeyCode::KeyA), EditorAction::SelectAll);
        bindings.insert(KeyBinding::key(KeyCode::Escape), EditorAction::ClearSelection);

        // Clipboard
        bindings.insert(KeyBinding::ctrl(KeyCode::KeyC), EditorAction::Copy);
        bindings.insert(KeyBinding::ctrl(KeyCode::KeyX), EditorAction::Cut);
        bindings.insert(KeyBinding::ctrl(KeyCode::KeyV), EditorAction::Paste);

        // Undo/Redo
        bindings.insert(KeyBinding::ctrl(KeyCode::KeyZ), EditorAction::Undo);
        bindings.insert(KeyBinding::ctrl(KeyCode::KeyY), EditorAction::Redo);
        bindings.insert(KeyBinding::ctrl_shift(KeyCode::KeyZ), EditorAction::Redo);

        // Find
        bindings.insert(KeyBinding::ctrl(KeyCode::KeyF), EditorAction::Find);
        bindings.insert(KeyBinding::key(KeyCode::F3), EditorAction::FindNext);
        bindings.insert(KeyBinding::shift(KeyCode::F3), EditorAction::FindPrevious);
        bindings.insert(KeyBinding::ctrl(KeyCode::KeyH), EditorAction::Replace);

        // LSP
        bindings.insert(KeyBinding::ctrl(KeyCode::Space), EditorAction::RequestCompletion);

        Self { bindings }
    }

    /// Vim-like keybindings
    pub fn vim_bindings() -> Self {
        // TODO: Implement vim keybindings
        Self::default_bindings()
    }

    /// Add or override a keybinding
    pub fn bind(&mut self, key: KeyBinding, action: EditorAction) {
        self.bindings.insert(key, action);
    }

    /// Remove a keybinding
    pub fn unbind(&mut self, key: &KeyBinding) {
        self.bindings.remove(key);
    }

    /// Get action for a key combination
    pub fn get_action(&self, key: KeyCode, keyboard: &ButtonInput<KeyCode>) -> Option<EditorAction> {
        for (binding, action) in &self.bindings {
            if binding.matches(key, keyboard) {
                return Some(*action);
            }
        }
        None
    }
}

/// System to handle keyboard input
pub fn handle_keyboard_input(
    mut state: ResMut<CodeEditorState>,
    mut char_events: MessageReader<KeyboardInput>,
    keyboard: Res<ButtonInput<KeyCode>>,
    keybindings: Res<Keybindings>,
    settings: Res<EditorSettings>,
    _viewport: Res<ViewportDimensions>,
    mut repeat_state: ResMut<KeyRepeatState>,
    time: Res<Time>,
    #[cfg(feature = "lsp")] lsp_client: Res<crate::lsp::LspClient>,
    #[cfg(feature = "lsp")] mut completion_state: ResMut<crate::lsp::CompletionState>,
    #[cfg(feature = "lsp")] _sync_state: ResMut<crate::lsp::LspSyncState>,
) {
    let current_time = time.elapsed_secs_f64();

    // Only process input if editor is focused
    if !state.is_focused {
        return;
    }

    // Handle keybinding actions with key repeat (all actions are repeatable)
    let mut action_to_execute: Option<EditorAction> = None;

    // Check just pressed keys first (immediate response)
    for key in keyboard.get_just_pressed() {
        if let Some(action) = keybindings.get_action(*key, &keyboard) {
            action_to_execute = Some(action);

            // Reset repeat state for new key press (all actions repeat)
            repeat_state.held_key = Some(*key);
            repeat_state.press_time = current_time;
            repeat_state.last_repeat_time = current_time;
            break;
        }
    }

    // Handle character input (for printable characters)
    // Only process if no keybinding action was triggered
    if action_to_execute.is_none() {
        for event in char_events.read() {
            // Only handle key presses with text
            if event.state.is_pressed() {
                match &event.logical_key {
                    bevy::input::keyboard::Key::Character(ref text) => {
                        for c in text.chars() {
                            // Skip control characters (they're handled by keybindings)
                            if c.is_control() {
                                continue;
                            }
                            insert_char(&mut state, c);

                            // Notify LSP of text change
                            #[cfg(feature = "lsp")]
                            send_did_change(&mut state, &lsp_client);

                            // Auto-trigger completion on trigger chars, OR update filter if already visible
                            #[cfg(feature = "lsp")]
                            if settings.completion.enabled {
                                if settings.completion.trigger_characters.contains(&c) {
                                    // Trigger character (. or ::) - open new completion
                                    // Mark completion as not visible to force start_char_index reset
                                    completion_state.visible = false;
                                    request_completion(&mut state, &lsp_client, &mut completion_state);
                                } else if completion_state.visible && (c.is_alphanumeric() || c == '_') {
                                    // Completion visible and typing identifier chars - update filter
                                    update_completion_filter(&state, &mut completion_state);
                                } else if !completion_state.visible && (c.is_alphanumeric() || c == '_') {
                                    // Not visible yet - check if we should auto-trigger after N chars
                                    // Find the start of the current word
                                    let word_start = find_word_start(&state.rope, state.cursor_pos);
                                    let word_len = state.cursor_pos - word_start;

                                    // Trigger after min_word_length characters (configurable, like VSCode's 3)
                                    if word_len >= settings.completion.min_word_length {
                                        // Set start_char_index to word start so filter works correctly
                                        completion_state.start_char_index = word_start;
                                        request_completion(&mut state, &lsp_client, &mut completion_state);
                                    }
                                }
                            }
                        }
                    }
                    // Bevy sends Space as a separate variant, not Character(" ")
                    bevy::input::keyboard::Key::Space => {
                        insert_char(&mut state, ' ');
                        // Notify LSP of text change
                        #[cfg(feature = "lsp")]
                        send_did_change(&mut state, &lsp_client);
                        // Dismiss completion on space
                        #[cfg(feature = "lsp")]
                        {
                            completion_state.visible = false;
                        }
                    }
                    _ => {}
                }
            }
        }
    } else {
        // Drain character events when a keybinding was triggered
        // This prevents the character from being inserted
        for _ in char_events.read() {}
    }

    // Handle key repeat for held keys
    if action_to_execute.is_none() {
        if let Some(held_key) = repeat_state.held_key {
            if keyboard.pressed(held_key) {
                // Check if enough time has passed for repeat
                let time_since_press = current_time - repeat_state.press_time;
                let time_since_last_repeat = current_time - repeat_state.last_repeat_time;

                // Read repeat parameters from settings
                let initial_delay = settings.cursor.key_repeat.initial_delay;
                let repeat_interval = settings.cursor.key_repeat.repeat_interval;

                if time_since_press >= initial_delay
                    && time_since_last_repeat >= repeat_interval
                {
                    // Trigger repeat
                    if let Some(action) = keybindings.get_action(held_key, &keyboard) {
                        action_to_execute = Some(action);
                        repeat_state.last_repeat_time = current_time;
                    }
                }
            } else {
                // Key was released, clear repeat state
                repeat_state.held_key = None;
            }
        }
    }

    // Execute the action if we have one
    if let Some(action) = action_to_execute {
        #[cfg(not(feature = "lsp"))]
        execute_action(&mut state, action, &settings);
        #[cfg(feature = "lsp")]
        execute_action(&mut state, action, &settings, &lsp_client, &mut completion_state);
    }
}

/// Insert a character at cursor position
fn insert_char(state: &mut CodeEditorState, c: char) {
    // Delete selection if exists
    if state.selection_start.is_some() && state.selection_end.is_some() {
        delete_selection(state);
    }

    state.insert_char(c);
}

/// Delete selected text
fn delete_selection(state: &mut CodeEditorState) {
    if let (Some(start), Some(end)) = (state.selection_start, state.selection_end) {
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };

        // Remove selected text
        let start_byte = state.rope.char_to_byte(start);
        let end_byte = state.rope.char_to_byte(end);
        state.rope.remove(start_byte..end_byte);

        // Move cursor to start of selection
        state.cursor_pos = start;

        // Clear selection
        state.selection_start = None;
        state.selection_end = None;

        state.pending_update = true;
        state.dirty_lines = None;
            state.previous_line_count = state.rope.len_lines();
            }
        }
        
        /// Execute an editor action (Non-LSP)
        #[cfg(not(feature = "lsp"))]
        fn execute_action(
            state: &mut CodeEditorState,
            action: EditorAction,
            settings: &EditorSettings,
        ) {
            match action {
                EditorAction::InsertChar => { /* Handled by char_events */ }
                EditorAction::InsertNewline => insert_char(state, '\n'),
                EditorAction::InsertTab => {
                    for _ in 0..settings.indentation.tab_size {
                        insert_char(state, ' ');
                    }
                }
                EditorAction::DeleteBackward => {
                    if state.selection_start.is_some() { delete_selection(state); }
                    else { state.delete_backward(); }
                }
                EditorAction::DeleteForward => {
                    if state.selection_start.is_some() { delete_selection(state); }
                    else { state.delete_forward(); }
                }
                EditorAction::DeleteWordBackward => state.delete_backward(), // TODO
                EditorAction::DeleteWordForward => state.delete_forward(), // TODO
                EditorAction::DeleteLine => {}, // TODO
                EditorAction::MoveCursorLeft => {
                    state.selection_start = None; state.selection_end = None;
                    state.move_cursor(-1);
                }
                EditorAction::MoveCursorRight => {
                    state.selection_start = None; state.selection_end = None;
                    state.move_cursor(1);
                }
                EditorAction::MoveCursorUp => {
                    state.selection_start = None; state.selection_end = None;
                    move_cursor_up(state);
                }
                EditorAction::MoveCursorDown => {
                    state.selection_start = None; state.selection_end = None;
                    move_cursor_down(state);
                }
                EditorAction::MoveCursorWordLeft => {
                    state.selection_start = None; state.selection_end = None;
                    state.move_cursor(-1); // TODO
                }
                EditorAction::MoveCursorWordRight => {
                    state.selection_start = None; state.selection_end = None;
                    state.move_cursor(1); // TODO
                }
                EditorAction::MoveCursorLineStart => {
                    state.selection_start = None; state.selection_end = None;
                    move_cursor_line_start(state);
                }
                EditorAction::MoveCursorLineEnd => {
                    state.selection_start = None; state.selection_end = None;
                    move_cursor_line_end(state);
                }
                EditorAction::MoveCursorDocumentStart => {
                    state.selection_start = None; state.selection_end = None;
                    state.cursor_pos = 0;
                }
                EditorAction::MoveCursorDocumentEnd => {
                    state.selection_start = None; state.selection_end = None;
                    state.cursor_pos = state.rope.len_chars();
                }
                EditorAction::MoveCursorPageUp => {}, // TODO
                EditorAction::MoveCursorPageDown => {}, // TODO
                EditorAction::SelectLeft => {
                    init_selection(state); state.move_cursor(-1);
                    state.selection_end = Some(state.cursor_pos);
                }
                EditorAction::SelectRight => {
                    init_selection(state); state.move_cursor(1);
                    state.selection_end = Some(state.cursor_pos);
                }
                EditorAction::SelectUp => {
                    init_selection(state); move_cursor_up(state);
                    state.selection_end = Some(state.cursor_pos);
                }
                EditorAction::SelectDown => {
                    init_selection(state); move_cursor_down(state);
                    state.selection_end = Some(state.cursor_pos);
                }
                EditorAction::SelectWordLeft => {
                    init_selection(state); state.move_cursor(-1); // TODO
                    state.selection_end = Some(state.cursor_pos);
                }
                EditorAction::SelectWordRight => {
                    init_selection(state); state.move_cursor(1); // TODO
                    state.selection_end = Some(state.cursor_pos);
                }
                EditorAction::SelectLineStart => {
                    init_selection(state); move_cursor_line_start(state);
                    state.selection_end = Some(state.cursor_pos);
                }
                EditorAction::SelectLineEnd => {
                    init_selection(state); move_cursor_line_end(state);
                    state.selection_end = Some(state.cursor_pos);
                }
                EditorAction::SelectAll => {
                    state.selection_start = Some(0);
                    state.selection_end = Some(state.rope.len_chars());
                    state.cursor_pos = state.rope.len_chars();
                }
                EditorAction::ClearSelection => {
                    state.selection_start = None; state.selection_end = None;
                }
                EditorAction::Copy => {
                    #[cfg(feature = "clipboard")]
                    if let (Some(s), Some(e)) = (state.selection_start, state.selection_end) {
                        let (start, end) = if s < e { (s, e) } else { (e, s) };
                        let start = start.min(state.rope.len_chars());
                        let end = end.min(state.rope.len_chars());
                        let text = state.rope.slice(start..end).to_string();
                        if let Ok(mut c) = Clipboard::new() { let _ = c.set_text(text); }
                    }
                }
                EditorAction::Cut => {
                    #[cfg(feature = "clipboard")]
                    if let (Some(s), Some(e)) = (state.selection_start, state.selection_end) {
                        let (start, end) = if s < e { (s, e) } else { (e, s) };
                        let start = start.min(state.rope.len_chars());
                        let end = end.min(state.rope.len_chars());
                        let text = state.rope.slice(start..end).to_string();
                        if let Ok(mut c) = Clipboard::new() { let _ = c.set_text(text); }
                        let sb = state.rope.char_to_byte(start);
                        let eb = state.rope.char_to_byte(end);
                        state.rope.remove(sb..eb);
                        state.cursor_pos = start;
                        state.selection_start = None; state.selection_end = None;
                        state.pending_update = true;
                        let nl = state.rope.len_lines();
                        let li = state.rope.char_to_line(start);
                        state.dirty_lines = Some(li..nl);
                        state.previous_line_count = nl;
                    }
                }
                EditorAction::Paste => {
                    #[cfg(feature = "clipboard")]
                    if let Ok(mut c) = Clipboard::new() {
                        if let Ok(text) = c.get_text() {
                            if let (Some(s), Some(e)) = (state.selection_start, state.selection_end) {
                                let (start, end) = if s < e { (s, e) } else { (e, s) };
                                let start = start.min(state.rope.len_chars());
                                let end = end.min(state.rope.len_chars());
                                let sb = state.rope.char_to_byte(start);
                                let eb = state.rope.char_to_byte(end);
                                state.rope.remove(sb..eb);
                                state.cursor_pos = start;
                                state.selection_start = None; state.selection_end = None;
                            }
                            let pos = state.cursor_pos.min(state.rope.len_chars());
                            state.rope.insert(pos, &text);
                            state.cursor_pos += text.chars().count();
                            state.pending_update = true;
                            let nl = state.rope.len_lines();
                            let li = state.rope.char_to_line(pos);
                            state.dirty_lines = Some(li..nl);
                            state.previous_line_count = nl;
                        }
                    }
                }
                EditorAction::Undo => {},
                EditorAction::Redo => {},
                EditorAction::ScrollUp => {
                    state.scroll_offset += settings.font.line_height;
                    state.needs_scroll_update = true;
                }
                EditorAction::ScrollDown => {
                    state.scroll_offset -= settings.font.line_height;
                    state.needs_scroll_update = true;
                }
                EditorAction::ScrollPageUp => {},
                EditorAction::ScrollPageDown => {},
                EditorAction::Find => {},
                EditorAction::FindNext => {},
                        EditorAction::FindPrevious => {},
                        EditorAction::Replace => {},
                        EditorAction::RequestCompletion => {},
                        EditorAction::GotoDefinition => {},
                    }
                }        
        /// Execute an editor action (LSP enabled)
#[cfg(feature = "lsp")]
fn execute_action(
    state: &mut CodeEditorState,
    action: EditorAction,
    settings: &EditorSettings,
    lsp_client: &lsp::LspClient,
    completion_state: &mut lsp::CompletionState,
) {
    // Handle Completion UI Navigation
    let filtered_count = completion_state.filtered_items().len();
    let max_visible = settings.completion.max_visible_items;
    if completion_state.visible && filtered_count > 0 {
        match action {
            EditorAction::MoveCursorUp => {
                if completion_state.selected_index > 0 {
                    completion_state.selected_index -= 1;
                } else {
                    completion_state.selected_index = filtered_count.saturating_sub(1);
                }
                completion_state.ensure_selected_visible_with_max(max_visible);
                return; // Don't move cursor in text
            }
            EditorAction::MoveCursorDown => {
                if completion_state.selected_index + 1 < filtered_count {
                    completion_state.selected_index += 1;
                } else {
                    completion_state.selected_index = 0;
                }
                completion_state.ensure_selected_visible_with_max(max_visible);
                return; // Don't move cursor in text
            }
            EditorAction::InsertNewline | EditorAction::InsertTab => {
                apply_completion(state, completion_state);
                // Also trigger a didChange since text likely changed
                send_did_change(state, lsp_client);
                return;
            }
            EditorAction::ClearSelection => {
                completion_state.visible = false;
                completion_state.filter.clear();
                completion_state.scroll_offset = 0;
                return;
            }
            _ => {}
        }
    }

    let mut text_changed = false;

    match action {
        EditorAction::InsertChar => {
            // Handled by char_events
        }
        EditorAction::InsertNewline => {
            insert_char(state, '\n');
            text_changed = true;
        }
        EditorAction::InsertTab => {
            // Insert spaces based on tab_size
            for _ in 0..settings.indentation.tab_size {
                insert_char(state, ' ');
            }
            text_changed = true;
        }

        EditorAction::DeleteBackward => {
            if state.selection_start.is_some() {
                delete_selection(state);
            } else {
                state.delete_backward();
            }
            text_changed = true;

            // Update filter if visible
            if completion_state.visible {
                if state.cursor_pos > completion_state.start_char_index {
                    // Still have characters after trigger - update filter
                    update_completion_filter(state, completion_state);
                } else if state.cursor_pos == completion_state.start_char_index {
                    // Deleted back to trigger position - clear filter but keep completion open
                    completion_state.filter.clear();
                    completion_state.selected_index = 0;
                } else {
                    // Deleted past trigger - dismiss
                    completion_state.visible = false;
                    completion_state.filter.clear();
                }
            }
        }
        EditorAction::DeleteForward => {
            if state.selection_start.is_some() {
                delete_selection(state);
            } else {
                state.delete_forward();
            }
            text_changed = true;
        }
        EditorAction::DeleteWordBackward => {
            // TODO: Implement word deletion
            state.delete_backward();
            text_changed = true;
        }
        EditorAction::DeleteWordForward => {
            // TODO: Implement word deletion
            state.delete_forward();
            text_changed = true;
        }
        EditorAction::DeleteLine => {
            // TODO: Implement line deletion
            text_changed = true;
        }

        EditorAction::MoveCursorLeft => {
            state.selection_start = None;
            state.selection_end = None;
            state.move_cursor(-1);
            // Dismiss completion when moving cursor horizontally
            completion_state.visible = false;
        }
        EditorAction::MoveCursorRight => {
            state.selection_start = None;
            state.selection_end = None;
            state.move_cursor(1);
            // Dismiss completion when moving cursor horizontally
            completion_state.visible = false;
        }
        EditorAction::MoveCursorUp => {
            state.selection_start = None;
            state.selection_end = None;
            move_cursor_up(state);
        }
        EditorAction::MoveCursorDown => {
            state.selection_start = None;
            state.selection_end = None;
            move_cursor_down(state);
        }
        EditorAction::MoveCursorWordLeft => {
            state.selection_start = None;
            state.selection_end = None;
            // TODO: Implement word movement
            state.move_cursor(-1);
        }
        EditorAction::MoveCursorWordRight => {
            state.selection_start = None;
            state.selection_end = None;
            // TODO: Implement word movement
            state.move_cursor(1);
        }
        EditorAction::MoveCursorLineStart => {
            state.selection_start = None;
            state.selection_end = None;
            move_cursor_line_start(state);
        }
        EditorAction::MoveCursorLineEnd => {
            state.selection_start = None;
            state.selection_end = None;
            move_cursor_line_end(state);
        }
        EditorAction::MoveCursorDocumentStart => {
            state.selection_start = None;
            state.selection_end = None;
            state.cursor_pos = 0;
        }
        EditorAction::MoveCursorDocumentEnd => {
            state.selection_start = None;
            state.selection_end = None;
            state.cursor_pos = state.rope.len_chars();
        }
        EditorAction::MoveCursorPageUp => {
            state.selection_start = None;
            state.selection_end = None;
            // TODO: Implement page up
        }
        EditorAction::MoveCursorPageDown => {
            state.selection_start = None;
            state.selection_end = None;
            // TODO: Implement page down
        }

        EditorAction::SelectLeft => {
            init_selection(state);
            state.move_cursor(-1);
            state.selection_end = Some(state.cursor_pos);
        }
        EditorAction::SelectRight => {
            init_selection(state);
            state.move_cursor(1);
            state.selection_end = Some(state.cursor_pos);
        }
        EditorAction::SelectUp => {
            init_selection(state);
            move_cursor_up(state);
            state.selection_end = Some(state.cursor_pos);
        }
        EditorAction::SelectDown => {
            init_selection(state);
            move_cursor_down(state);
            state.selection_end = Some(state.cursor_pos);
        }
        EditorAction::SelectWordLeft => {
            init_selection(state);
            // TODO: Implement word movement
            state.move_cursor(-1);
            state.selection_end = Some(state.cursor_pos);
        }
        EditorAction::SelectWordRight => {
            init_selection(state);
            // TODO: Implement word movement
            state.move_cursor(1);
            state.selection_end = Some(state.cursor_pos);
        }
        EditorAction::SelectLineStart => {
            init_selection(state);
            move_cursor_line_start(state);
            state.selection_end = Some(state.cursor_pos);
        }
        EditorAction::SelectLineEnd => {
            init_selection(state);
            move_cursor_line_end(state);
            state.selection_end = Some(state.cursor_pos);
        }
        EditorAction::SelectAll => {
            state.selection_start = Some(0);
            state.selection_end = Some(state.rope.len_chars());
            state.cursor_pos = state.rope.len_chars();
        }
        EditorAction::ClearSelection => {
            state.selection_start = None;
            state.selection_end = None;
        }

        EditorAction::Copy => {
            #[cfg(feature = "clipboard")]
            {
                if let (Some(start), Some(end)) = (state.selection_start, state.selection_end) {
                    let (start, end) = if start < end { (start, end) } else { (end, start) };
                    let start = start.min(state.rope.len_chars());
                    let end = end.min(state.rope.len_chars());

                    let selected_text = state.rope.slice(start..end).to_string();

                    if let Ok(mut clipboard) = Clipboard::new() {
                        let _ = clipboard.set_text(selected_text);
                    }
                }
            }
        }
        EditorAction::Cut => {
            #[cfg(feature = "clipboard")]
            {
                if let (Some(start), Some(end)) = (state.selection_start, state.selection_end) {
                    let (start, end) = if start < end { (start, end) } else { (end, start) };
                    let start = start.min(state.rope.len_chars());
                    let end = end.min(state.rope.len_chars());

                    let selected_text = state.rope.slice(start..end).to_string();

                    // Copy to clipboard
                    if let Ok(mut clipboard) = Clipboard::new() {
                        let _ = clipboard.set_text(selected_text);
                    }

                    // Delete the selection
                    let start_byte = state.rope.char_to_byte(start);
                    let end_byte = state.rope.char_to_byte(end);
                    state.rope.remove(start_byte..end_byte);
                    state.cursor_pos = start;
                    state.selection_start = None;
                    state.selection_end = None;
                    state.pending_update = true;

                    let new_line_count = state.rope.len_lines();
                    let line_idx = state.rope.char_to_line(start);
                    state.dirty_lines = Some(line_idx..new_line_count);
                    state.previous_line_count = new_line_count;
                    
                    text_changed = true;
                }
            }
        }
        EditorAction::Paste => {
            #[cfg(feature = "clipboard")]
            {
                if let Ok(mut clipboard) = Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        // Delete selection if any
                        if let (Some(start), Some(end)) = (state.selection_start, state.selection_end) {
                            let (start, end) = if start < end { (start, end) } else { (end, start) };
                            let start = start.min(state.rope.len_chars());
                            let end = end.min(state.rope.len_chars());

                            let start_byte = state.rope.char_to_byte(start);
                            let end_byte = state.rope.char_to_byte(end);
                            state.rope.remove(start_byte..end_byte);
                            state.cursor_pos = start;
                            state.selection_start = None;
                            state.selection_end = None;
                        }

                        // Insert pasted text
                        let cursor_pos = state.cursor_pos.min(state.rope.len_chars());
                        let line_idx = state.rope.char_to_line(cursor_pos);

                        state.rope.insert(cursor_pos, &text);
                        state.cursor_pos += text.chars().count();
                        state.pending_update = true;

                        let new_line_count = state.rope.len_lines();
                        state.dirty_lines = Some(line_idx..new_line_count);
                        state.previous_line_count = new_line_count;
                        
                        text_changed = true;
                    }
                }
            }
        }

        EditorAction::Undo => {
            // TODO: Implement undo
        }
        EditorAction::Redo => {
            // TODO: Implement redo
        }

        EditorAction::ScrollUp => {
            state.scroll_offset += settings.font.line_height;
            state.needs_scroll_update = true;
        }
        EditorAction::ScrollDown => {
            state.scroll_offset -= settings.font.line_height;
            state.needs_scroll_update = true;
        }
        EditorAction::ScrollPageUp => {
            // TODO: Implement page scroll
        }
        EditorAction::ScrollPageDown => {
            // TODO: Implement page scroll
        }

        EditorAction::Find => {
            // TODO: Implement find
        }
        EditorAction::FindNext => {
            // TODO: Implement find next
        }
        EditorAction::FindPrevious => {
            // TODO: Implement find previous
        }
                EditorAction::Replace => {
                    // TODO: Implement replace
                }
                EditorAction::RequestCompletion => {
                    request_completion(state, lsp_client, completion_state);
                }
                EditorAction::GotoDefinition => { /* Handled by mouse input */ }
            }

    if text_changed {
        send_did_change(state, lsp_client);
    }
}

/// Apply selected completion item
#[cfg(feature = "lsp")]
fn apply_completion(
    state: &mut CodeEditorState,
    completion_state: &mut lsp::CompletionState,
) {
    // Get filtered items and select from that list
    let filtered = completion_state.filtered_items();
    if let Some(item) = filtered.get(completion_state.selected_index) {
        let start = completion_state.start_char_index;
        let end = state.cursor_pos;
        let label = item.label.clone(); // Clone to avoid borrow issues

        // Ensure valid range
        if start <= end && end <= state.rope.len_chars() {
            let start_byte = state.rope.char_to_byte(start);
            let end_byte = state.rope.char_to_byte(end);
            state.rope.remove(start_byte..end_byte);
            state.rope.insert(start, &label);

            state.cursor_pos = start + label.chars().count();
            state.pending_update = true;

            // Mark lines as dirty for highlighting update
            let line_idx = state.rope.char_to_line(start);
            let new_line_count = state.rope.len_lines();
            state.dirty_lines = Some(line_idx..new_line_count);
            state.previous_line_count = new_line_count;
        }
    }
    completion_state.visible = false;
    completion_state.filter.clear();
    completion_state.scroll_offset = 0;
}

/// Find the start of the current word (for auto-triggering completion)
#[cfg(feature = "lsp")]
fn find_word_start(rope: &ropey::Rope, cursor_pos: usize) -> usize {
    if cursor_pos == 0 {
        return 0;
    }

    let mut pos = cursor_pos;
    while pos > 0 {
        let prev_char = rope.char(pos - 1);
        if prev_char.is_alphanumeric() || prev_char == '_' {
            pos -= 1;
        } else {
            break;
        }
    }
    pos
}

/// Update the completion filter based on text typed since start_char_index
#[cfg(feature = "lsp")]
fn update_completion_filter(
    state: &CodeEditorState,
    completion_state: &mut lsp::CompletionState,
) {
    let cursor_pos = state.cursor_pos.min(state.rope.len_chars());
    let start = completion_state.start_char_index;

    if cursor_pos > start && start <= state.rope.len_chars() {
        // Extract the filter text from start_char_index to cursor
        let filter_text: String = state.rope.slice(start..cursor_pos).chars().collect();
        completion_state.filter = filter_text;
        // Reset selection and scroll when filter changes
        completion_state.selected_index = 0;
        completion_state.scroll_offset = 0;

        #[cfg(debug_assertions)]
        eprintln!("[LSP] Filter updated: '{}'", completion_state.filter);
    } else {
        completion_state.filter.clear();
        completion_state.scroll_offset = 0;
    }
}

/// Request completion from LSP
/// If completion is already visible, this re-requests with updated position (for filtering)
/// If completion is not visible, this opens new completion
#[cfg(feature = "lsp")]
fn request_completion(
    state: &mut CodeEditorState,
    lsp_client: &lsp::LspClient,
    completion_state: &mut lsp::CompletionState,
) {
    use lsp_types::Position;

    let cursor_pos = state.cursor_pos.min(state.rope.len_chars());
    let line_index = state.rope.char_to_line(cursor_pos);
    let char_in_line_index = cursor_pos - state.rope.line_to_char(line_index);

    let lsp_position = Position {
        line: line_index as u32,
        character: char_in_line_index as u32,
    };

    if let Some(uri) = &state.document_uri {
        #[cfg(debug_assertions)]
        eprintln!("[LSP] Requesting completion at line={}, char={}, visible={}, start_idx={}",
            lsp_position.line, lsp_position.character, completion_state.visible, completion_state.start_char_index);

        lsp_client.send(LspMessage::Completion {
            uri: uri.clone(),
            position: lsp_position,
        });

        // Only set start_char_index when first opening completion
        // This preserves the trigger position for proper text replacement
        if !completion_state.visible {
            completion_state.start_char_index = cursor_pos;
            completion_state.items.clear();
            completion_state.selected_index = 0;
            completion_state.filter.clear();
        }
        completion_state.visible = true;
    } else {
        eprintln!("[bevy_code_editor] Cannot request completion: No document URI set");
    }
}

/// Send textDocument/didChange notification to LSP
#[cfg(feature = "lsp")]
fn send_did_change(
    state: &mut CodeEditorState,
    lsp_client: &lsp::LspClient,
) {
    if let Some(uri) = &state.document_uri {
        // Increment version for each change
        state.document_version += 1;
        let version = state.document_version;

        // Full text sync for simplicity
        let change = lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: state.rope.to_string(),
        };

        lsp_client.send(lsp::LspMessage::DidChange {
            uri: uri.clone(),
            version,
            changes: vec![change],
        });

        #[cfg(debug_assertions)]
        eprintln!("[LSP] DidChange sent, version={}", version);
    }
}

/// Initialize selection if not already started
fn init_selection(state: &mut CodeEditorState) {
    if state.selection_start.is_none() {
        state.selection_start = Some(state.cursor_pos);
        state.selection_end = Some(state.cursor_pos);
    }
}

/// Move cursor up one line
fn move_cursor_up(state: &mut CodeEditorState) {
    if state.cursor_pos > 0 {
        let line_idx = state.rope.char_to_line(state.cursor_pos);
        if line_idx > 0 {
            let line_start = state.rope.line_to_char(line_idx);
            let col_offset = state.cursor_pos - line_start;
            let prev_line_start = state.rope.line_to_char(line_idx - 1);
            let prev_line_len = state.rope.line(line_idx - 1).len_chars();
            state.cursor_pos = prev_line_start + col_offset.min(prev_line_len.saturating_sub(1));
        }
    }
}

/// Move cursor down one line
fn move_cursor_down(state: &mut CodeEditorState) {
    let line_idx = state.rope.char_to_line(state.cursor_pos);
    if line_idx + 1 < state.rope.len_lines() {
        let line_start = state.rope.line_to_char(line_idx);
        let col_offset = state.cursor_pos - line_start;
        let next_line_start = state.rope.line_to_char(line_idx + 1);
        let next_line_len = state.rope.line(line_idx + 1).len_chars();
        state.cursor_pos = next_line_start + col_offset.min(next_line_len.saturating_sub(1));
    }
}

/// Move cursor to line start
fn move_cursor_line_start(state: &mut CodeEditorState) {
    let line_idx = state.rope.char_to_line(state.cursor_pos);
    state.cursor_pos = state.rope.line_to_char(line_idx);
}

/// Move cursor to line end
fn move_cursor_line_end(state: &mut CodeEditorState) {
    let line_idx = state.rope.char_to_line(state.cursor_pos);
    let line_start = state.rope.line_to_char(line_idx);
    let line_len = state.rope.line(line_idx).len_chars();
    state.cursor_pos = line_start + line_len.saturating_sub(1).max(0);
}

/// System to handle mouse input
pub fn handle_mouse_input(
    mut state: ResMut<CodeEditorState>,
    mut drag_state: ResMut<MouseDragState>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    #[cfg(feature = "lsp")] time: Res<Time>,
    #[cfg(feature = "lsp")] lsp_client: Res<crate::lsp::LspClient>,
    #[cfg(feature = "lsp")] mut hover_state: ResMut<crate::lsp::HoverState>,
    #[cfg(feature = "lsp")] keyboard_input: Res<ButtonInput<KeyCode>>, // Add for Ctrl check
) {
    // Get cursor position
    let cursor_pos_screen = window_query.iter().next()
        .and_then(|window| window.cursor_position());

    // Calculate char position if mouse is over the editor
    let char_pos = if let Some(cursor_pos_screen) = cursor_pos_screen {
        // Clamp cursor_pos_screen to viewport bounds (rough check)
        // If the mouse is outside the editor area, we don't care about hover
        let viewport_width = viewport.width as f32;
        let viewport_height = viewport.height as f32;

        let editor_min_x = -viewport_width / 2.0 + viewport.offset_x;
        let editor_max_x = viewport_width / 2.0 + viewport.offset_x;
        let editor_min_y = -viewport_height / 2.0;
        let editor_max_y = viewport_height / 2.0;

        let mouse_in_editor_area = cursor_pos_screen.x >= editor_min_x && cursor_pos_screen.x <= editor_max_x &&
                                 cursor_pos_screen.y >= editor_min_y && cursor_pos_screen.y <= editor_max_y;
        
        if mouse_in_editor_area {
            Some(screen_to_char_pos(
                cursor_pos_screen,
                &state,
                &settings,
                viewport_width,
                viewport_height,
                viewport.offset_x,
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

                        if let Some(uri) = &state.document_uri {
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
                    
                    if let Some(uri) = &state.document_uri {
                        lsp_client.send(LspMessage::GotoDefinition {
                            uri: uri.clone(),
                            position: lsp_position,
                        });
                    }
                    return; // Consume the click, don't start drag or move cursor normally
                }
            }

            // Start drag
            drag_state.is_dragging = true;
            drag_state.drag_start_pos = Some(char_pos);

            // Update cursor and clear selection
            state.cursor_pos = char_pos;
            state.selection_start = None;
            state.selection_end = None;
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

/// Convert screen coordinates to character position in the editor
fn screen_to_char_pos(
    screen_pos: Vec2,
    state: &CodeEditorState,
    settings: &EditorSettings,
    _viewport_width: f32,
    _viewport_height: f32,
    offset_x: f32,
) -> usize {
    // Calculate the clicked position relative to code start, accounting for sidebar offset
    let relative_x = screen_pos.x - settings.ui.layout.code_margin_left - offset_x;
    let relative_y = screen_pos.y - settings.ui.layout.margin_top + state.scroll_offset;

    // Calculate line and column from pixel position
    let line_height = settings.font.line_height;
    let char_width = settings.font.size * 0.6; // Approximate monospace width

    let line = (relative_y / line_height).max(0.0) as usize;
    let col = (relative_x / char_width).max(0.0) as usize;

    // Convert line/col to character position
    let line_count = state.rope.len_lines();
    if line >= line_count {
        // Click below last line - go to end of document
        return state.rope.len_chars();
    }

    let line_start_char = state.rope.line_to_char(line);
    let line_len = state.rope.line(line).len_chars().saturating_sub(1); // Exclude newline
    let char_in_line = col.min(line_len);

    line_start_char + char_in_line
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

                state.horizontal_scroll_offset += scroll_delta;

                // Clamp horizontal scroll:
                // Minimum is 0 (can't scroll left past column 0)
                state.horizontal_scroll_offset = state.horizontal_scroll_offset.max(0.0);

                // Maximum is when the rightmost content reaches the right edge of available text area
                let max_horizontal_scroll = (state.max_content_width - available_text_width).max(0.0);
                state.horizontal_scroll_offset = state.horizontal_scroll_offset.min(max_horizontal_scroll);

                scrolled = true;
            }
        }

        // Vertical scrolling (using event.y)
        if event.y.abs() > 0.0 {
            // Positive y = scroll up (content moves down, scroll_offset increases)
            // Negative y = scroll down (content moves up, scroll_offset decreases)
            let scroll_delta = event.y * settings.font.line_height * settings.scrolling.speed;

            state.scroll_offset += scroll_delta;

            // Clamp scroll_offset to prevent scrolling past top
            // Top limit: 0 (don't scroll above the first line)
            state.scroll_offset = state.scroll_offset.min(0.0);

            // Bottom limit: stop when last line reaches top of viewport
            // Calculate total content height
            let line_count = state.rope.len_lines();
            let content_height = line_count as f32 * settings.font.line_height;
            let viewport_height = viewport.height as f32;

            // Allow scrolling only until last line is at top (plus margin)
            let max_scroll = -(content_height - viewport_height + settings.ui.layout.margin_top);
            state.scroll_offset = state.scroll_offset.max(max_scroll.min(0.0));

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
