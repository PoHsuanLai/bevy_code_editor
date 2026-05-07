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
//!     .add_plugins(CodeEditorPlugins)
//!     .add_systems(Startup, |mut commands: Commands| {
//!         commands.spawn(Camera2d);
//!     })
//!     .run();
//! ```
//!
//! [`CodeEditorPlugins`] is the full bundle: GPU rendering, interaction,
//! editor sub-plugins (cursor / syntax / folding / brackets / scrollbar
//! / UI), and the editor's own [`CodeEditorPlugin`] core. The bare
//! [`CodeEditorPlugin`] is the editor logic on its own — for hosts that
//! compose with their own [`bevy_text_engine::TextEnginePlugins`],
//! [`EditorUiPlugin`], etc., and need to avoid double-adds. Disable
//! individual plugins in the group with
//! `CodeEditorPlugins.build().disable::<EditorUiPlugin>()`.
//!
//! Per-entity configuration lives on components — to start with a
//! different font size, override
//! [`bevy_text_engine::FontConfig`] when spawning:
//!
//! ```rust,no_run
//! # use bevy::prelude::*;
//! # use bevy_code_editor::prelude::*;
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(CodeEditorPlugins)
//!     .add_systems(Startup, |mut commands: Commands| {
//!         commands.spawn((
//!             CodeEditor,
//!             FontConfig::from_size(18.0).with_line_height_multiplier(1.4),
//!         ));
//!     })
//!     .run();
//! ```
//!
//! Customizing keybindings: spawn an `EditorInputManager` with your own
//! `InputMap<EditorAction>` *before* `PostStartup`; the plugin's default
//! input manager is gated on no existing one being present.

pub mod display_map;
pub mod input;
pub mod plugin;
pub mod settings;
pub mod syntax;
pub mod text_view;
pub mod types;

#[cfg(feature = "lsp")]
pub mod lsp_ui;

pub mod prelude {
    //! Convenient re-exports for common editor usage.
    //!
    //! Engine-side primitives (`TextView`, `FontConfig`, `DisplayLayout`,
    //! `TextBuffer`, `ScrollState`, `ContentMetrics`, `TextViewViewport`,
    //! `TextEnginePlugin`, `TextEnginePlugins`) come in via
    //! `bevy_text_engine::prelude::*`. The
    //! editor adds: the editor plugin (+ `standalone()`'s plugin group), the
    //! UI plugin, the interaction plugin, the `CodeEditor` marker, and the
    //! handful of file/save events + scroll config that hosts touch
    //! day-to-day. Lower-level types (display map points, fold/wrap state,
    //! shaped lines, history) live on the crate path
    //! (`bevy_code_editor::types::*`, `::display_map::*`, etc.) for hosts
    //! that need them.

    // Engine surface — TextEnginePlugins, TextEnginePlugin, TextView,
    // FontConfig, DisplayLayout, TextBuffer, ScrollState, ContentMetrics,
    // TextViewViewport.
    pub use bevy_text_engine::prelude::*;

    // Editor plugin + its standalone PluginGroup, and the interaction +
    // UI plugins that hosts compose with.
    pub use crate::plugin::{CodeEditorPlugin, CodeEditorPlugins, EditorCamera, EditorUiPlugin};

    // Editor marker + save/open events.
    pub use crate::types::editor::{CodeEditor, OpenRequested, SaveRequested};
    pub use crate::types::events::EditorFoldStateChanged;
    #[cfg(feature = "tree-sitter")]
    pub use crate::types::events::SetLanguageRequested;

    // Editable-text widget types from `bevy_text_editor`. Re-exported so
    // prelude users get them without a separate import.
    pub use bevy_text_editor::{
        ScrollConfig, TextEditor, TextEditorPlugin, TextInteractionPlugin,
    };

    // Selection / multi-cursor types and the EditorAction enum.
    pub use crate::input::EditorAction;
    pub use crate::types::{Selection, SelectionCollection};

    // Theme — hosts that match the editor's clear color in their own
    // Camera2d setup grab `ThemeConfig::default().background`.
    pub use crate::settings::ThemeConfig;
}
