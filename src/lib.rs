//! # Bevy Code Editor
//!
//! Code editor plugin for Bevy.
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use bevy_code_editor::prelude::*;
//!
//! fn main() {
//!     // Define your keybindings
//!     let input_map = InputMap::default()
//!         .with(EditorAction::MoveCursorLeft, KeyCode::ArrowLeft)
//!         .with(EditorAction::MoveCursorRight, KeyCode::ArrowRight)
//!         .with(EditorAction::Copy, ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::KeyC]))
//!         .with(EditorAction::Paste, ButtonlikeChord::new([KeyCode::ControlLeft, KeyCode::KeyV]));
//!
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugins(CodeEditorPlugin::new(input_map))
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
//!     let input_map = InputMap::default();
//!     let settings = EditorSettings::minimal();
//!
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugins(CodeEditorPlugin::new(input_map).with_settings(settings))
//!         .run();
//! }
//! ```

pub mod plugin;
pub mod settings;
pub mod types;
pub mod input;
pub mod display_map;
pub mod line_width;
pub mod gpu_text;
pub mod syntax;
pub mod events;

#[cfg(feature = "lsp")]
pub mod lsp;

pub mod prelude {
    //! Convenient re-exports for common usage
    pub use crate::plugin::{
        CodeEditorPlugin, EditorInputManager, EditorUiPlugin,
        ScrollbarPlugin, Scrollbar,
    };
    pub use crate::settings::*;
    pub use crate::types::*;
    pub use crate::input::*;
    pub use crate::events::*;

    // Selective re-exports from display_map to avoid name conflicts with types.rs
    pub use crate::display_map::{
        LayeredDisplayMap, DisplaySnapshot, DisplayMapLayer,
        BufferPoint, FoldPoint, WrapPoint, DisplayPoint, Point,
        FoldMap, WrapMap, TabMap,
        BufferRowDisplayInfo, DisplayRowInfo,
    };

    #[cfg(feature = "lsp")]
    pub use crate::lsp::*;

    // Re-export LSP plugins (feature-gated)
    #[cfg(feature = "lsp")]
    pub use crate::plugin::{LspPlugin, LspUiPlugin};
}
