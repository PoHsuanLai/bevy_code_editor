//! Modular settings system for the code editor.
//!
//! Each settings group is its own `Resource` with a sensible `Default`.
//! `CodeEditorPlugin` registers all of them at startup. To customize, the
//! host can either insert its own resource value before adding the plugin
//! (Bevy's `init_resource` is a no-op when the resource already exists),
//! or mutate the resource at runtime.
//!
//! Per-entity values that vary across editors (font size, scroll
//! behaviour) live on the `FontConfig` and `ScrollConfig` components, not
//! here.

mod core;
mod cursor;
mod performance;
mod scrolling;
mod search;
mod syntax;
mod ui;
mod wrapping;

mod scrollbar;

#[cfg(feature = "lsp")]
mod lsp;

pub use core::*;
pub use cursor::*;
pub use performance::*;
pub use scrolling::*;
pub use search::*;
pub use syntax::*;
pub use ui::*;
pub use wrapping::*;

pub use scrollbar::*;

#[cfg(feature = "lsp")]
pub use lsp::*;
