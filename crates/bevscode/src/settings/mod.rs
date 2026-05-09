//! Per-editor settings components for the code editor.
//!
//! All settings are `Component`s cascaded onto every `CodeEditor` entity
//! via `#[require]`, so each editor in a multi-editor app carries its own
//! independent copy. Override any subset at spawn time:
//!
//! ```rust,ignore
//! commands.spawn((
//!     CodeEditor,
//!     UiSettings { show_line_numbers: false, ..default() },
//!     IndentationSettings { use_spaces: false, tab_width: 2, ..default() },
//! ));
//! ```
//!
//! Or mutate at runtime via `Query<&mut UiSettings, With<CodeEditor>>`.
//!
//! `ThemeConfig` and `SyntaxTheme` follow the same pattern and are defined
//! in `core` and `syntax` respectively.

mod core;
mod cursor;
mod performance;
mod syntax;
mod ui;
mod wrapping;

#[cfg(feature = "lsp")]
mod lsp;

pub use core::*;
pub use cursor::*;
pub use performance::*;
pub use syntax::*;
pub use ui::*;
pub use wrapping::*;

// Re-export hoisted timing primitives so existing callers keep importing
// them through `crate::settings::*` and `crate::settings::KeyRepeatSettings`.
pub use bevy_instanced_text_edit::KeyRepeatSettings;

#[cfg(feature = "lsp")]
pub use lsp::*;
