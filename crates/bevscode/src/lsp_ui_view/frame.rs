//! Shared popup framing: positioning + base [`Node`] style + background +
//! border. Every popup renderer calls [`apply_frame`] to set the absolute
//! position and chrome, then fills in its own children.

use bevy::prelude::*;

use crate::settings::EditorTheme;

use super::anchor::{PopupAnchor, PopupPlacement, PopupRect};

/// Position + base styling for a popup `Node`.
///
/// - Sets `position_type = Absolute`, `width`, `height`, `flex_direction =
///   Column`, `overflow = clip`, and a small interior padding.
/// - Applies `BackgroundColor` and `BorderColor` from [`EditorTheme`].
/// - Hides the popup (`display = None`) when the anchor isn't resolvable
///   yet (e.g. layout not produced this frame). The renderer should then
///   skip rebuilding children.
///
/// Returns the resolved [`PopupRect`] when placement succeeded, or
/// `None` when the popup is hidden this frame.
#[allow(clippy::too_many_arguments)]
pub fn apply_frame(
    commands: &mut Commands,
    entity: Entity,
    node: &mut Node,
    anchor: &PopupAnchor,
    theme: Option<&EditorTheme>,
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
    node.padding = UiRect::all(Val::Px(4.0));

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

    let bg = theme
        .map(|t| t.background)
        .unwrap_or(Color::srgb(0.117, 0.117, 0.117));
    let border = theme
        .map(|t| t.separator)
        .unwrap_or(Color::srgb(0.2, 0.2, 0.2));
    commands
        .entity(entity)
        .insert((BackgroundColor(bg), BorderColor::all(border)));

    rect
}

/// Despawn every child of `entity` whose id is listed. Convenience for
/// the "tear down old children, rebuild list" loop every popup uses.
pub fn clear_children(commands: &mut Commands, children: Option<&Children>) {
    let Some(children) = children else { return };
    for child in children.iter() {
        commands.entity(child).despawn();
    }
}
