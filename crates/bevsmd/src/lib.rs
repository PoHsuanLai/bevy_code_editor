//! Embeddable CommonMark viewer plugin for Bevy.
//!
//! Parses a Markdown string via [pulldown-cmark](https://docs.rs/pulldown-cmark)
//! and renders it as a scrollable, selectable text view. Designed to be
//! embedded inside any Bevy application — spawn a [`MarkdownViewerBundle`] and
//! the plugin manages the layout and rendering. No knowledge of the rendering
//! layer is required.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use bevsmd::prelude::*;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(MarkdownViewerPlugins)
//!     .add_systems(Startup, |mut commands: Commands| {
//!         commands.spawn(MarkdownViewerBundle {
//!             doc: MarkdownDoc::new("# Hello\n\nA *markdown* viewer."),
//!             ..default()
//!         });
//!     })
//!     .run();
//! ```
//!
//! ## Appearance
//!
//! Controlled via [`MarkdownTheme`], a component cascaded automatically onto
//! every [`MarkdownDoc`] entity. Override at spawn time or mutate at runtime.
//!
//! Supported CommonMark: headings, bold, italic, inline code, fenced code
//! blocks, blockquotes, unordered/ordered lists, horizontal rules, and links.
//!
//! ## State the host can read
//!
//! - **[`MarkdownLinks`]** — all links in the document with their display text
//!   and href. The plugin does not navigate links; hosts query this component
//!   and handle clicks however fits their app.

pub mod layout;
pub mod parse;
pub mod plugin;
pub mod theme;
pub mod view;

pub use plugin::{
    MarkdownFont, MarkdownViewerBundle, MarkdownViewerPlugin, MarkdownViewerPlugins,
    MarkdownViewport,
};
pub use theme::{BlockDecorTheme, MarkdownTheme};
pub use view::{MarkdownCodeFont, MarkdownDoc, MarkdownLinks};

// Re-exported so hosts never need to import from the rendering layer directly.
pub use bevy_instanced_text::{DisplayLayout, ScrollState};

pub mod prelude {
    //! Common types for embedding a markdown viewer.
    pub use crate::plugin::{
        MarkdownFont, MarkdownViewerBundle, MarkdownViewerPlugin, MarkdownViewerPlugins,
        MarkdownViewport,
    };
    pub use crate::theme::{BlockDecorTheme, MarkdownTheme};
    pub use crate::view::{MarkdownCodeFont, MarkdownDoc, MarkdownLinks};
    pub use bevy_instanced_text::{DisplayLayout, ScrollState};
}
