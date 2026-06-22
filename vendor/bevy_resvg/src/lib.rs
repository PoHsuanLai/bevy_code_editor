#![doc = include_str!("../README.md")]

/// Runtime modification of [`SVG`](https://en.wikipedia.org/wiki/SVG) files.
pub mod effects;
/// Error utilities for this crate.
pub mod error;
/// The [`Plugin`](bevy::app::Plugin) for initialising the Bevy logic and
/// configuration provided by this crate.
pub mod plugin;
/// Import this module as `use bevy_resvg::prelude::*` to get convenient
/// imports.
pub mod prelude;
/// Tools and helpers for loading, rastering, and rendering
/// [`SVG`](https://en.wikipedia.org/wiki/SVG) files.
pub mod raster;
/// [`AssetLoader`](bevy::asset::AssetLoader) settings for
/// [`SvgFile`](crate::prelude::SvgFile)
pub mod settings;
/// Tools and helpers for loading [`SVG`](https://en.wikipedia.org/wiki/SVG)
/// files.
pub(crate) mod vector;

pub use resvg;
