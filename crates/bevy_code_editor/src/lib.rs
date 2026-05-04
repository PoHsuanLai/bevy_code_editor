//! # Bevy Code Editor
//!
//! Code editor plugin for Bevy. The editor is one consumer of the
//! [`bevy_text_engine`] text rendering primitives — see also
//! [`crate::text_view`] for the lower-level `TextView` API.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use bevy_code_editor::prelude::*;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(CodeEditorPlugin)
//!     .run();
//! ```
//!
//! `CodeEditorPlugin` is a `Default` unit struct: it registers default
//! settings resources and spawns one `CodeEditor` entity at startup, with
//! a default key binding map. Per-entity configuration lives on
//! components — to start with a different font size, override
//! [`bevy_text_engine::FontConfig`] when spawning:
//!
//! ```rust,no_run
//! # use bevy::prelude::*;
//! # use bevy_code_editor::prelude::*;
//! fn spawn_editor(mut commands: Commands) {
//!     commands.spawn((
//!         CodeEditor,
//!         FontConfig::from_size(18.0).with_line_height_multiplier(1.4),
//!     ));
//! }
//! ```
//!
//! Customizing keybindings: spawn an `EditorInputManager` with your own
//! `InputMap<EditorAction>` *before* `PostStartup`; the plugin's default
//! input manager is gated on no existing one being present.

pub mod display_map;
pub mod input;
pub mod language;
pub mod plugin;
pub mod settings;
pub mod syntax;
pub mod text_view;
pub mod types;

#[cfg(feature = "lsp")]
pub mod lsp;

pub mod prelude {
    //! Convenient re-exports for common usage
    pub use crate::input::*;
    pub use crate::language::{Language, TreeSitterConfig};
    pub use crate::plugin::{
        ApplyStateSet, CodeEditorPlugin, EditorInputManager, EditorSetupSet, EditorUiPlugin,
        HighlightCache, InputSet, RenderingSet, SyntaxPlugin, SyntaxResource,
    };
    pub use crate::types::events::*;

    pub use crate::plugin::{Scrollbar, ScrollbarPlugin};
    pub use crate::settings::*;
    pub use crate::text_view::{
        copy_selection, screen_to_char_pos, GlyphBatchComponent, GlyphInstance, TextView,
        TextViewBatch, TextViewBatchEntity, TextViewDragState, TextViewPlugin, TextViewRenderSet,
        TextViewSelectionState, TextViewState, TextViewViewport, ViewportOrigin,
    };
    // User-facing editor components
    pub use crate::types::editor::{
        CodeEditor, CursorState, EditHistoryState, EditorDisplayState,
        EditorScrollControl, KeyRepeatState, OpenRequested, SaveRequested, ScrollConfig,
        SelectionState, SyntaxCacheState, ViewportConfig, ViewportDimensions,
    };
    // Engine-side per-entity font configuration (re-exported for convenience).
    pub use bevy_text_engine::FontConfig;
    // User-facing data types
    pub use crate::types::display_map::LineSegment;
    pub use crate::types::fold::{FoldState, GotoLineState};
    pub use crate::types::selection::Cursor;

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
    pub use crate::lsp::egui_render::LspEguiViewportOffset;
    #[cfg(feature = "egui-overlays")]
    pub use crate::plugin::lsp_egui_ui_plugin::LspEguiUiPlugin;
}
