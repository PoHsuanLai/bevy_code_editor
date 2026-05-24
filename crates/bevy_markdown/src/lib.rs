//! CommonMark renderer for `bevy_ui` + `bevy_text`.
//!
//! Spawn a [`Markdown`] component on a UI [`Node`]; the plugin's reactive
//! system rebuilds the entity's children whenever the source string or
//! any theme component changes. Theme is split into four small
//! `Component`s ([`MarkdownFonts`], [`MarkdownColors`], [`MarkdownSpacing`],
//! [`MarkdownScales`]) and any of them not explicitly inserted falls
//! back to its `Default` via `#[require]`.
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use bevy_markdown::prelude::*;
//!
//! fn setup(mut commands: Commands, assets: Res<AssetServer>) {
//!     commands.spawn(Camera2d);
//!     commands.spawn((
//!         Markdown { source: "# Hello\n\nSome **bold** text.".into() },
//!         MarkdownFonts {
//!             body: assets.load("fonts/Inter.ttf"),
//!             mono: assets.load("fonts/FiraMono.ttf"),
//!             ..default()
//!         },
//!         Node {
//!             flex_direction: FlexDirection::Column,
//!             padding: UiRect::all(Val::Px(16.0)),
//!             ..default()
//!         },
//!     ));
//! }
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(BevyMarkdownPlugin)
//!     .add_systems(Startup, setup)
//!     .run();
//! ```

pub mod highlight;
pub mod parse;
pub mod spawn;
pub mod theme;
pub mod tree_sitter;

use bevy::prelude::*;

pub use highlight::{CodeHighlighter, MarkdownHighlighter, NoHighlight};
pub use parse::{parse, Block, Inline, InlineStyle};
pub use spawn::{spawn_markdown, MarkdownLink};
pub use theme::{MarkdownColors, MarkdownFonts, MarkdownScales, MarkdownSpacing};

/// Source string to render. The plugin rebuilds children whenever this
/// component (or any of the required theme components) changes.
///
/// The host's `Node` should be a column flex container — children are
/// stacked vertically. `#[require(Node, ...)]` guarantees every theme
/// piece exists at default values so a bare `commands.spawn(Markdown { source })`
/// renders correctly (using Bevy's default font).
#[derive(Component, Clone, Debug)]
#[require(Node, MarkdownFonts, MarkdownColors, MarkdownSpacing, MarkdownScales)]
pub struct Markdown {
    pub source: String,
}

pub struct BevyMarkdownPlugin;

impl Plugin for BevyMarkdownPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, rebuild_markdown);
    }
}

#[allow(clippy::type_complexity)]
fn rebuild_markdown(
    mut commands: Commands,
    targets: Query<
        (
            Entity,
            &Markdown,
            &MarkdownFonts,
            &MarkdownColors,
            &MarkdownSpacing,
            &MarkdownScales,
        ),
        Or<(
            Changed<Markdown>,
            Changed<MarkdownFonts>,
            Changed<MarkdownColors>,
            Changed<MarkdownSpacing>,
            Changed<MarkdownScales>,
        )>,
    >,
    highlighter: Option<Res<MarkdownHighlighter>>,
) {
    for (entity, md, fonts, colors, spacing, scales) in &targets {
        // The host can despawn the markdown root between when this
        // system observed `Changed<Markdown>` and when its commands
        // apply (popup teardown is a common case). Silenced queues
        // skip the rebuild if the entity is gone instead of panicking.
        let Ok(mut entity) = commands.get_entity(entity) else {
            continue;
        };
        entity.queue_silenced(|mut e: bevy::ecs::world::EntityWorldMut| {
            e.despawn_related::<bevy::prelude::Children>();
        });
        let highlighter = highlighter.as_deref();
        entity.with_children(|parent| {
            spawn::spawn_markdown(parent, &md.source, fonts, colors, spacing, scales, highlighter);
        });
    }
}

pub mod prelude {
    pub use crate::{
        BevyMarkdownPlugin, CodeHighlighter, Markdown, MarkdownColors, MarkdownFonts,
        MarkdownHighlighter, MarkdownLink, MarkdownScales, MarkdownSpacing, NoHighlight,
    };
}
