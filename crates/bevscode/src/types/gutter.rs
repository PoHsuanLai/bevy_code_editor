//! Gutter, cursor, separator and selection-highlight marker components.

use bevy::prelude::*;

/// Buffer line currently under the pointer, used by gutter chevrons under
/// `Folding::show_controls::Mouseover`. `None` when the pointer is outside
/// the editor or has not moved since the last frame.
#[derive(Component, Default, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct HoveredGutterLine(pub Option<usize>);

/// `true` when the pointer is over the gutter strip (line numbers,
/// chevrons), not the text area. Drives `sync_cursor_icon` so the OS
/// arrow shows over the gutter and the I-beam over text — matching
/// VSCode / Sublime behavior.
#[derive(Component, Default, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct HoveredInGutter(pub bool);

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct EditorCursor {
    /// 0 = primary cursor; higher indices are multi-cursor additions.
    pub cursor_index: usize,
}

#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct LineNumbers;

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct GutterContainer {
    pub editor: Entity,
}

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct GutterTextView {
    /// Editor entity this gutter belongs to.
    pub editor: Entity,
}

#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct Separator;

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct SelectionHighlight {
    pub line_index: usize,
    /// Index of the cursor this selection belongs to (0 = primary cursor).
    pub cursor_index: usize,
}
