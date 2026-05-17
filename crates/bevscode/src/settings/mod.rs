//! Per-editor settings components for the code editor.
//!
//! All settings are `Component`s cascaded onto every `CodeEditor` entity
//! via `#[require]`, so each editor in a multi-editor app carries its own
//! independent copy. Override any subset at spawn time:
//!
//! ```rust,ignore
//! commands.spawn((
//!     CodeEditor,
//!     EditorUi { show_line_numbers: false, ..default() },
//!     Indentation { use_spaces: false, tab_width: 2, ..default() },
//! ));
//! ```
//!
//! Or mutate at runtime via `Query<&mut EditorUi, With<CodeEditor>>`.
//!
//! `EditorTheme` and `SyntaxColors` follow the same pattern and are defined
//! in `core` and `syntax` respectively.

mod cursor;
mod performance;
mod syntax;
mod theme;
mod ui;
mod wrapping;

#[cfg(feature = "lsp")]
mod lsp;

pub use cursor::*;
pub use performance::*;
pub use syntax::*;
pub use theme::*;
pub use ui::*;
pub use wrapping::*;

// Re-export hoisted timing primitives so existing callers keep importing
// them through `crate::settings::*` and `crate::settings::KeyRepeatSettings`.
pub use bevy_instanced_text_editor::KeyRepeatSettings;

#[cfg(feature = "lsp")]
pub use lsp::*;
