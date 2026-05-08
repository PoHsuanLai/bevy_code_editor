//! Read-only CommonMark viewer for Bevy.
//!
//! Parses a Markdown string via [pulldown-cmark](https://docs.rs/pulldown-cmark),
//! converts it to a styled `bevy_text_engine` [`bevy_text_engine::LineStyles`]
//! layout, and renders it through a [`bevy_text_engine::TextView`] entity.
//!
//! ## What this crate provides
//!
//! Spawn a [`MarkdownDoc`] with a CommonMark string and the plugin takes care
//! of the rest. Supported elements: headings, bold, italic, inline code, fenced
//! code blocks, blockquotes, unordered and ordered lists, horizontal rules,
//! and links (rendered as styled text; navigation is a host concern).
//!
//! Scroll and text selection are inherited from the underlying
//! `bevy_text_engine` + `bevy_text_editor` stack. The viewer is read-only —
//! there is no cursor, no edit history, and no keyboard input.
//!
//! Appearance is controlled via [`MarkdownTheme`], which lives as a component
//! on the `MarkdownDoc` entity and can be overridden at spawn time or mutated
//! at runtime.
//!
//! ## What this crate does NOT provide
//!
//! - Link navigation or click handlers — hosts observe [`MarkdownLinks`]
//! - Image rendering
//! - HTML passthrough
//! - Editing
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
