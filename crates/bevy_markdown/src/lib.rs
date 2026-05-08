//! Embeddable CommonMark viewer plugin for Bevy.
//!
//! Parses a Markdown string via [pulldown-cmark](https://docs.rs/pulldown-cmark)
//! and renders it as a scrollable, selectable text view. Designed to be
//! embedded inside any Bevy application — spawn a [`MarkdownDoc`] and the
//! plugin manages the layout and rendering. The host is not required to know
//! anything about the rendering pipeline.
//!
//! ## Spawning a viewer
//!
//! ```rust,no_run
//! # use bevy::prelude::*;
//! # use bevy_markdown::MarkdownDoc;
//! commands.spawn(MarkdownDoc::new("# Hello\n\nSome *markdown*."));
//! ```
//!
//! Appearance is controlled via [`MarkdownTheme`], a component on the
//! `MarkdownDoc` entity. Override at spawn time or mutate at runtime.
//!
//! Supported CommonMark: headings, bold, italic, inline code, fenced code
//! blocks, blockquotes, unordered/ordered lists, horizontal rules, and links.
//!
//! ## State the host can read
//!
//! - **[`MarkdownLinks`]** — all links in the document with their display text
//!   and href. The plugin does not navigate links; hosts query this component
//!   and handle clicks however fits their app.
//!
//! Scroll and text selection come from the underlying rendering layer. The
//! viewer is intentionally read-only — there is no cursor, edit history, or
//! keyboard input.
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
