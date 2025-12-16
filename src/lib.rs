//! # Bevy Code Editor
//!
//! High-performance GPU-accelerated code editor plugin for Bevy.
//!
//! ## Features
//!
//! - GPU-accelerated rendering using Bevy's text rendering
//! - Efficient rope data structure for text editing (via `ropey`)
//! - Optional syntax highlighting (via `tree-sitter`)
//! - Optional LSP support
//! - Viewport culling for large files
//! - Entity pooling for performance
//! - Configurable themes and settings
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use bevy_code_editor::prelude::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugins(CodeEditorPlugin::default())
//!         .run();
//! }
//! ```
//!
//! ## Customization
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use bevy_code_editor::prelude::*;
//!
//! fn main() {
//!     let settings = EditorSettings::minimal();
//!
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugins(CodeEditorPlugin::with_settings(settings))
//!         .run();
//! }
//! ```

pub mod plugin;
pub mod settings;
pub mod types;
pub mod input;

#[cfg(feature = "lsp")]
pub mod lsp;

pub mod prelude {
    //! Convenient re-exports for common usage
    pub use crate::plugin::CodeEditorPlugin;
    pub use crate::settings::*;
    pub use crate::types::*;
    pub use crate::input::*;

    #[cfg(feature = "lsp")]
    pub use crate::lsp::*;
}
