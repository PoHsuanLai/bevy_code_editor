//! Embeddable code editor plugin for Bevy.
//!
//! Designed to be dropped into any Bevy application as a self-contained
//! widget. The plugin handles the mechanics — input dispatch, cursor movement,
//! edit history, syntax highlighting, code folding, bracket matching — and
//! exposes everything as readable ECS components so the host app can react to
//! or extend editor state without forking the plugin.
//!
//! ## Spawning an editor
//!
//! Spawn [`crate::types::editor::CodeEditor`] with any overrides; the
//! `#[require]` cascade fills in sensible defaults for everything else:
//!
//! ```rust,no_run
//! # use bevy::prelude::*;
//! # use bevy_code_editor::prelude::*;
//! # use bevy_text_engine::FontConfig;
//! // Minimal — auto-sizes to window, default theme and keybindings.
//! commands.spawn(CodeEditor);
//!
//! // With overrides.
//! commands.spawn((
//!     CodeEditor,
//!     FontConfig::from_size(18.0),
//!     ThemeConfig { background: bevy::color::palettes::css::DARK_SLATE_GRAY.into(), ..default() },
//! ));
//! ```
//!
//! All settings are per-entity components (see [`settings`]): two editors in
//! the same app can have different fonts, themes, indentation rules, or LSP
//! configurations.
//!
//! ## State the host can read
//!
//! The plugin writes these components every frame — hosts can `Query` them
//! freely without any coupling to the plugin internals:
//!
//! - [`crate::types::fold::FoldState`] — which line ranges are currently
//!   folded. Drive fold/unfold via the [`crate::types::events`] messages or
//!   listen to [`crate::types::events::EditorFoldStateChanged`] for transitions.
//! - [`crate::plugin::syntax_highlighting::EditorSyntaxState`] — the current
//!   highlight ranges, keyed by capture name. Useful for outline panels, AI
//!   context extraction, or custom overlays.
//! - [`crate::types::editor::BracketMatchState`] — the matched bracket pair
//!   under the cursor, if any.
//! - [`bevy_text_editor::CursorState`], [`bevy_text_editor::SelectionState`],
//!   [`bevy_text_editor::EditHistoryState`] — cursor position, selections, and
//!   undo stack from the editing layer.
//! - [`bevy_text_engine::DisplayLayout`] — the shaped-line snapshot used for
//!   rendering; also useful for hit-testing and overlay placement.
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
//! ## Plugin composition
//!
//! [`crate::plugin::CodeEditorPlugins`] is the full bundle. Hosts that already
//! add parts of the rendering stack can use [`crate::plugin::CodeEditorPlugin`]
//! alone and compose only the sub-plugins they need, or disable specific ones:
//! `CodeEditorPlugins.build().disable::<EditorUiPlugin>()`.
//!
//! ## Keybindings
//!
//! Spawn an `EditorInputManager` with your own `InputMap<EditorAction>` before
//! `PostStartup`; the plugin's default input manager is only added if none exists.

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
