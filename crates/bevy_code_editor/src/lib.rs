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
    //! Convenient re-exports for common editor usage.
    //!
    //! Engine-side primitives (`TextView`, `FontConfig`, `DisplayLayout`,
    //! `TextViewState`, `TextViewViewport`, `TextEnginePlugin`,
    //! `TextEnginePlugins`) come in via `bevy_text_engine::prelude::*`. The
    //! editor adds: the editor plugin (+ `standalone()`'s plugin group), the
    //! UI plugin, the interaction plugin, the `CodeEditor` marker, and the
    //! handful of file/save events + scroll config that hosts touch
    //! day-to-day. Lower-level types (display map points, fold/wrap state,
    //! shaped lines, history) live on the crate path
    //! (`bevy_code_editor::types::*`, `::display_map::*`, etc.) for hosts
    //! that need them.

    // Engine surface — TextEnginePlugins, TextEnginePlugin, TextView,
    // FontConfig, DisplayLayout, TextViewState, TextViewViewport.
    pub use bevy_text_engine::prelude::*;

    // Editor plugin + its standalone PluginGroup, and the interaction +
    // UI plugins that hosts compose with.
    pub use crate::plugin::{
        CodeEditorPlugin, CodeEditorStandalone, EditorUiPlugin,
    };
    pub use crate::text_view::TextInteractionPlugin;

    // Editor marker + save/open events.
    pub use crate::types::editor::{CodeEditor, OpenRequested, SaveRequested};

    // Per-entity scroll config (lives in `bevy_text_interaction` since
    // Phase 6, re-exported here so prelude users keep getting it).
    pub use bevy_text_interaction::ScrollConfig;

    // Cursor (the editor-side cursor data type) and the EditorAction enum.
    pub use crate::input::EditorAction;
    pub use crate::types::selection::Cursor;
}
