use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

pub fn default_input_map() -> InputMap<EditorAction> {
    let mut input_map = InputMap::default();

    // Deletion
    input_map.insert(EditorAction::DeleteBackward, KeyCode::Backspace);
    input_map.insert(EditorAction::DeleteForward, KeyCode::Delete);
    input_map.insert(
        EditorAction::DeleteWordBackward,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::Backspace]),
    );
    input_map.insert(
        EditorAction::DeleteWordForward,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::Delete]),
    );

    // Special insertion
    input_map.insert(EditorAction::InsertNewline, KeyCode::Enter);
    input_map.insert(EditorAction::InsertTab, KeyCode::Tab);

    // Cursor movement
    input_map.insert(EditorAction::MoveCursorLeft, KeyCode::ArrowLeft);
    input_map.insert(EditorAction::MoveCursorRight, KeyCode::ArrowRight);
    input_map.insert(EditorAction::MoveCursorUp, KeyCode::ArrowUp);
    input_map.insert(EditorAction::MoveCursorDown, KeyCode::ArrowDown);
    input_map.insert(
        EditorAction::MoveCursorWordLeft,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::ArrowLeft]),
    );
    input_map.insert(
        EditorAction::MoveCursorWordRight,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::ArrowRight]),
    );
    input_map.insert(EditorAction::MoveCursorLineStart, KeyCode::Home);
    input_map.insert(EditorAction::MoveCursorLineEnd, KeyCode::End);
    input_map.insert(
        EditorAction::MoveCursorDocumentStart,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::Home]),
    );
    input_map.insert(
        EditorAction::MoveCursorDocumentEnd,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::End]),
    );
    input_map.insert(EditorAction::MoveCursorPageUp, KeyCode::PageUp);
    input_map.insert(EditorAction::MoveCursorPageDown, KeyCode::PageDown);

    // Selection (Shift + movement)
    input_map.insert(
        EditorAction::SelectLeft,
        ButtonlikeChord::new([KeyCode::ShiftLeft, KeyCode::ArrowLeft]),
    );
    input_map.insert(
        EditorAction::SelectRight,
        ButtonlikeChord::new([KeyCode::ShiftLeft, KeyCode::ArrowRight]),
    );
    input_map.insert(
        EditorAction::SelectUp,
        ButtonlikeChord::new([KeyCode::ShiftLeft, KeyCode::ArrowUp]),
    );
    input_map.insert(
        EditorAction::SelectDown,
        ButtonlikeChord::new([KeyCode::ShiftLeft, KeyCode::ArrowDown]),
    );
    input_map.insert(
        EditorAction::SelectWordLeft,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::ShiftLeft, KeyCode::ArrowLeft]),
    );
    input_map.insert(
        EditorAction::SelectWordRight,
        ButtonlikeChord::new([
            KeyCode::ControlLeft,
            KeyCode::ShiftLeft,
            KeyCode::ArrowRight,
        ]),
    );
    input_map.insert(
        EditorAction::SelectLineStart,
        ButtonlikeChord::new([KeyCode::ShiftLeft, KeyCode::Home]),
    );
    input_map.insert(
        EditorAction::SelectLineEnd,
        ButtonlikeChord::new([KeyCode::ShiftLeft, KeyCode::End]),
    );
    input_map.insert(
        EditorAction::SelectAll,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::KeyA]),
    );
    input_map.insert(EditorAction::ClearSelection, KeyCode::Escape);

    // Clipboard
    input_map.insert(
        EditorAction::Copy,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::KeyC]),
    );
    input_map.insert(
        EditorAction::Cut,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::KeyX]),
    );
    input_map.insert(
        EditorAction::Paste,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::KeyV]),
    );

    // Undo/Redo
    input_map.insert(
        EditorAction::Undo,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::KeyZ]),
    );
    input_map.insert(
        EditorAction::Redo,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::KeyY]),
    );
    input_map.insert(
        EditorAction::Redo,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::ShiftLeft, KeyCode::KeyZ]),
    );

    // Navigation
    input_map.insert(
        EditorAction::GotoLine,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::KeyG]),
    );

    // LSP
    input_map.insert(
        EditorAction::RequestCompletion,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::Space]),
    );
    input_map.insert(EditorAction::RenameSymbol, KeyCode::F2);

    // Multi-cursor
    input_map.insert(
        EditorAction::AddCursorAtNextOccurrence,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::KeyD]),
    );
    input_map.insert(
        EditorAction::AddCursorAbove,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::AltLeft, KeyCode::ArrowUp]),
    );
    input_map.insert(
        EditorAction::AddCursorBelow,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::AltLeft, KeyCode::ArrowDown]),
    );

    // Code folding
    input_map.insert(
        EditorAction::ToggleFold,
        ButtonlikeChord::new([
            KeyCode::ControlLeft,
            KeyCode::ShiftLeft,
            KeyCode::BracketLeft,
        ]),
    );
    input_map.insert(
        EditorAction::Fold,
        ButtonlikeChord::new([
            KeyCode::ControlLeft,
            KeyCode::ShiftLeft,
            KeyCode::BracketLeft,
        ]),
    );
    input_map.insert(
        EditorAction::Unfold,
        ButtonlikeChord::new([
            KeyCode::ControlLeft,
            KeyCode::ShiftLeft,
            KeyCode::BracketRight,
        ]),
    );
    input_map.insert(
        EditorAction::FoldAll,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::AltLeft, KeyCode::BracketLeft]),
    );
    input_map.insert(
        EditorAction::UnfoldAll,
        ButtonlikeChord::new([
            KeyCode::ControlLeft,
            KeyCode::AltLeft,
            KeyCode::BracketRight,
        ]),
    );

    // File operations
    input_map.insert(
        EditorAction::Save,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::KeyS]),
    );
    input_map.insert(
        EditorAction::Open,
        ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::KeyO]),
    );

    input_map
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect, Actionlike)]
#[reflect(Debug, Hash, PartialEq)]
pub enum EditorAction {
    // Deletion
    DeleteBackward,
    DeleteForward,
    DeleteWordBackward,
    DeleteWordForward,
    DeleteLine,

    // Special insertion
    InsertNewline,
    InsertTab,

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

    // Navigation
    GotoLine,

    // LSP
    RequestCompletion,
    GotoDefinition,
    RenameSymbol,

    // Multi-cursor
    AddCursorAtNextOccurrence,
    AddCursorAbove,
    AddCursorBelow,
    ClearSecondaryCursors,

    // Code folding
    ToggleFold,
    Fold,
    Unfold,
    FoldAll,
    UnfoldAll,

    // File operations — emit events for the host app to handle
    Save,
    Open,
}

impl EditorAction {
    pub fn is_repeatable(&self) -> bool {
        matches!(
            self,
            EditorAction::DeleteBackward
                | EditorAction::DeleteForward
                | EditorAction::DeleteWordBackward
                | EditorAction::DeleteWordForward
                | EditorAction::MoveCursorLeft
                | EditorAction::MoveCursorRight
                | EditorAction::MoveCursorUp
                | EditorAction::MoveCursorDown
                | EditorAction::MoveCursorWordLeft
                | EditorAction::MoveCursorWordRight
                | EditorAction::SelectLeft
                | EditorAction::SelectRight
                | EditorAction::SelectUp
                | EditorAction::SelectDown
                | EditorAction::SelectWordLeft
                | EditorAction::SelectWordRight
                | EditorAction::Undo
                | EditorAction::Redo
                | EditorAction::InsertNewline
        )
    }
}
