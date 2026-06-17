//! URL detection and ctrl-click open for `Misc::links`.
//!
//! - [`detection`]: the scan state machine ([`find_urls`]), a port of
//!   Monaco's `linkComputer.ts` (no regex dep).
//! - [`overlay`]: [`update_link_overlays`], the visible-window producer
//!   that writes underline overlays ([`LinkRects`]) and per-URL hit-test
//!   ranges ([`LinkRanges`]).
//! - [`input`]: the hover and ctrl/cmd-click observers.
//!
//! The data components ([`LinkRange`], [`LinkRanges`], [`LinkRects`],
//! [`HoveredLink`]) live here so all three submodules can share them.

mod detection;
mod input;
mod overlay;

pub use input::{on_ctrl_click_open_url, on_pointer_move_for_link_hover};
pub(crate) use overlay::update_link_overlays;

use bevy::prelude::*;
use bevy_instanced_text::RectOverlay;

/// One detected URL inside the buffer.
#[derive(Clone, Debug, Reflect)]
#[reflect(Debug)]
pub struct LinkRange {
    /// Buffer line (not display row) the URL starts on.
    pub buffer_line: usize,
    /// Inclusive char offset within the line.
    pub start_char: usize,
    /// Exclusive char offset within the line.
    pub end_char: usize,
    /// The URL itself.
    pub url: String,
}

/// Per-URL hit-test data — written by `update_link_overlays`, read by the
/// link input observers.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct LinkRanges(pub Vec<LinkRange>);

/// Underline overlays per visible URL — written by `update_link_overlays`,
/// merged into `TextOverlays` by `merge_overlay_components`.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct LinkRects(pub Vec<RectOverlay>);

/// Index of the link in [`LinkRanges`] currently under the pointer, or
/// `None` when no link is hovered. Drives the dotted/solid underline swap
/// (hover-only → dotted dim, hover + ctrl → solid bright) and the
/// `Pointer` cursor icon over an active link.
#[derive(Component, Default, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct HoveredLink(pub Option<usize>);
