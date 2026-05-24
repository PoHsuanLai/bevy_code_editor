//! Shared popup chrome — tempera-token-styled background, border, radius,
//! padding — plus position resolution via [`PopupAnchor`].
//!
//! Every popup renderer in this module calls [`apply_chrome`] to set its
//! own `Node`'s position and chrome, then fills in its own children. The
//! palette and metrics come from tempera's [`ColorPalette`], [`Spacing`],
//! and [`MenuTokens`] resources via [`PopupChrome`], so changing the
//! palette in the app re-tints every popup the same frame.

use bevy::prelude::*;

use crate::ui_kit::PopupChrome;

use super::anchor::{PopupAnchor, PopupPlacement, PopupRect};

/// Position + tempera-styled chrome for a popup `Node`.
///
/// - Sets `position_type = Absolute`, the requested size, column layout,
///   clipping overflow, a 1px border, and tempera's corner radius +
///   interior padding.
/// - Applies `BackgroundColor` from `palette.popover` and `BorderColor`
///   from `palette.border`.
/// - Hides the popup (`display = None`) when the anchor isn't resolvable
///   yet (e.g. layout not produced this frame). The renderer should then
///   skip rebuilding children.
///
/// Returns the resolved [`PopupRect`] when placement succeeded, or
/// `None` when the popup is hidden this frame.
#[allow(clippy::too_many_arguments)]
pub fn apply_chrome(
    commands: &mut Commands,
    entity: Entity,
    node: &mut Node,
    anchor: &PopupAnchor,
    chrome: &PopupChrome,
    editor: Entity,
    line: u32,
    character: u32,
    size: Vec2,
    placement: PopupPlacement,
) -> Option<PopupRect> {
    node.position_type = PositionType::Absolute;
    node.width = Val::Px(size.x);
    node.height = Val::Px(size.y);
    node.flex_direction = FlexDirection::Column;
    node.overflow = Overflow::clip();
    node.padding = UiRect::all(Val::Px(chrome.spacing.xs));
    node.border = UiRect::all(Val::Px(chrome.menu.border_width));
    node.border_radius = BorderRadius::all(Val::Px(chrome.spacing.corner_radius_small));

    let rect = anchor.place(editor, line, character, size, placement);
    match rect {
        Some(r) => {
            node.left = r.left;
            node.top = r.top;
            node.display = Display::Flex;
        }
        None => {
            node.display = Display::None;
        }
    }

    commands.entity(entity).insert((
        BackgroundColor(chrome.palette.popover),
        BorderColor::all(chrome.palette.border),
    ));

    rect
}

/// Despawn every child of `entity`. Convenience for the "tear down old
/// children, rebuild list" loop every popup uses.
pub fn clear_children(commands: &mut Commands, children: Option<&Children>) {
    let Some(children) = children else { return };
    for child in children.iter() {
        commands.entity(child).despawn();
    }
}
