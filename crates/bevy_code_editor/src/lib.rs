//! Full-featured code editor plugin for Bevy.
//!
//! Composes the rendering, editing, syntax, and LSP layers into a
//! ready-to-use IDE widget. Spawn [`CodeEditor`] and the `#[require]`
//! cascade brings in everything — cursor, selection, edit history, syntax
//! highlighting, bracket matching, code folding, and (with the `lsp` feature)
//! completion, hover, diagnostics, and rename.
//!
//! All configuration is per-entity: two editors in the same app can have
//! different fonts, themes, indentation rules, or keymaps. See [`settings`]
//! for the full list of configurable components.
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
//!     .run();
//! ```
//!
//! The default spawn (from `CodeEditorPlugin::build`) creates one `CodeEditor`
//! entity that auto-sizes to the window. Override at spawn time to customize:
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
//!             ThemeConfig { background: bevy::color::palettes::css::DARK_SLATE_GRAY.into(), ..default() },
//!         ));
//!     })
//!     .run();
//! ```
//!
//! ## Plugin composition
//!
//! [`crate::plugin::CodeEditorPlugins`] is the full bundle. For hosts that
//! already add parts of the stack (e.g. a custom [`bevy_text_engine::TextEnginePlugins`]),
//! use [`crate::plugin::CodeEditorPlugin`] alone and add only the sub-plugins
//! you need, or disable specific ones:
//! `CodeEditorPlugins.build().disable::<EditorUiPlugin>()`.
//!
//! ## Keybindings
//!
//! Spawn an `EditorInputManager` with your own `InputMap<EditorAction>` before
//! `PostStartup`; the plugin's default input manager is gated on no existing
//! one being present.

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
