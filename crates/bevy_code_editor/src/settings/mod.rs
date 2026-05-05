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

mod scrollbar;

#[cfg(feature = "lsp")]
mod lsp;

pub use core::*;
pub use cursor::*;
pub use performance::*;
pub use syntax::*;
pub use ui::*;
pub use wrapping::*;

pub use scrollbar::*;

#[cfg(feature = "lsp")]
pub use lsp::*;
