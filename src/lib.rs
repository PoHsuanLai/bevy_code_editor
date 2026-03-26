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

pub mod display_map;
pub mod events;
pub mod gpu_text;
pub mod input;
pub mod language;
pub mod line_width;
pub mod plugin;
pub mod settings;
pub mod syntax;
pub mod text_view;
pub mod types;

#[cfg(feature = "lsp")]
pub mod lsp;

pub mod prelude {
    //! Convenient re-exports for common usage
    pub use crate::events::*;
    pub use crate::input::*;
    pub use crate::language::{Language, TreeSitterConfig};
    pub use crate::plugin::{
        ApplyStateSet, CodeEditorPlugin, EditorInputManager, EditorSetupSet, EditorUiPlugin,
        InputSet, RenderingSet, SyntaxResource, SyntaxPlugin, HighlightCache,
    };

    #[cfg(feature = "scrollbar")]
    pub use crate::plugin::{Scrollbar, ScrollbarPlugin};
    pub use crate::settings::*;
    pub use crate::text_view::{
        TextView, TextViewBatchEntity, TextViewPlugin, TextViewRenderSet,
        TextViewState, TextViewViewport, GlyphBatchComponent, GlyphInstance, TextViewBatch,
    };
    pub use crate::types::*;

    // Selective re-exports from display_map to avoid name conflicts with types.rs
    pub use crate::display_map::{
        BufferPoint, BufferRowDisplayInfo, DisplayMapLayer, DisplayPoint, DisplayRowInfo,
        DisplaySnapshot, FoldMap, FoldPoint, LayeredDisplayMap, Point, TabMap, WrapMap, WrapPoint,
    };

    #[cfg(feature = "lsp")]
    pub use crate::lsp::*;

    // Re-export LSP plugins (feature-gated)
    #[cfg(feature = "lsp")]
    pub use crate::plugin::{LspPlugin, LspUiPlugin};

    // Re-export egui LSP UI plugin (feature-gated)
    #[cfg(feature = "egui-overlays")]
    pub use crate::plugin::lsp_egui_ui_plugin::LspEguiUiPlugin;
    #[cfg(feature = "egui-overlays")]
    pub use crate::lsp::egui_render::LspEguiViewportOffset;
}
