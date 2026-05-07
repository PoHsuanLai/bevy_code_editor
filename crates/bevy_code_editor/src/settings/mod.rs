//! Settings groups for the code editor.
//!
//! Most groups are global `Resource`s (theme, UI, performance) — values
//! the user expects to be the same across every editor in the app.
//! Per-entity values that vary across editors (font size, scroll
//! behaviour) live on the `FontConfig` and `ScrollConfig` components, not
//! here.
//!
//! `CodeEditorPlugin` registers each group at startup. To customize, the
//! host can either insert its own resource value before adding the plugin
//! (Bevy's `init_resource` is a no-op when the resource already exists),
//! or mutate the resource at runtime.

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
pub use bevy_text_editor::KeyRepeatSettings;

#[cfg(feature = "lsp")]
pub use lsp::*;
