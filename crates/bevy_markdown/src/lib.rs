//! Read-only markdown viewer for Bevy.
//!
//! Parses CommonMark via [`pulldown-cmark`] and renders the result through
//! `bevy_text_engine`'s `TextView` machinery. The viewer is read-only:
//! scroll, selection, and copy are inherited from the engine; editing,
//! click handlers, and image fetching are out of scope.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use bevy_text_engine::TextEnginePlugins;
//! use bevy_markdown::{MarkdownViewerPlugin, MarkdownDoc};
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(TextEnginePlugins)
//!     .add_plugins(MarkdownViewerPlugin)
//!     .add_systems(Startup, |mut commands: Commands| {
//!         commands.spawn(MarkdownDoc::new("# Hello\n\nA *markdown* viewer."));
//!     })
//!     .run();
//! ```

pub mod layout;
pub mod parse;
pub mod plugin;
pub mod theme;
pub mod view;

pub use layout::layout_markdown;
pub use parse::{parse_markdown, Block as MdBlock, Inline as MdInline};
pub use plugin::MarkdownViewerPlugin;
pub use theme::MarkdownTheme;
pub use view::{MarkdownDoc, MarkdownLinks};
