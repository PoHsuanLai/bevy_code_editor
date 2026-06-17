//! Pointer observers for links: track the hovered URL and open a URL on
//! ctrl/cmd-click.

use bevy::picking::events::{Pointer, Press};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;

use crate::settings::*;
use crate::types::*;

use super::{HoveredLink, LinkRanges};

/// Mouse-move observer: track which link index in [`LinkRanges`] (if any)
/// the pointer is currently over, writing it to [`HoveredLink`]. The
/// producer reads this every frame to swap underline styling, and
/// `sync_cursor_icon` reads it (together with the Ctrl/Cmd modifier) to
/// flip to a pointer cursor.
#[derive(bevy::ecs::query::QueryData)]
#[query_data(mutable)]
pub struct LinkHoverRow {
    view: EditorRenderView,
    link_ranges: &'static LinkRanges,
    misc: &'static Misc,
    hovered: &'static mut HoveredLink,
}

pub fn on_pointer_move_for_link_hover(
    trigger: On<bevy::picking::events::Pointer<bevy::picking::events::Move>>,
    mut editor_query: Query<LinkHoverRow, With<CodeEditor>>,
) {
    let entity = trigger.event().entity;
    let Ok(mut row) = editor_query.get_mut(entity) else {
        return;
    };

    // Resolve the hovered link index (or `None`) from the read-only columns
    // before touching the mutable `hovered` field, so the read borrows are
    // released before the single write below.
    let next = link_index_at_pointer(&row, &trigger.event().hit);
    if row.hovered.0 != next {
        row.hovered.0 = next;
    }
}

/// Resolve which link index in [`LinkRanges`] the pointer is over, or
/// `None` when links are off / the layout isn't ready / the hit misses
/// any link.
fn link_index_at_pointer(
    row: &LinkHoverRowItem<'_, '_>,
    hit: &bevy::picking::backend::HitData,
) -> Option<usize> {
    if !row.misc.links {
        return None;
    }
    let local_pos = crate::input::mouse::hit_to_local_px(hit, row.view.computed)?;
    let inv = row.view.computed.inverse_scale_factor();
    let text_area_left = row.view.computed.content_inset().min_inset.x * inv;
    let relative_x = local_pos.x - text_area_left;
    let layout = row.view.layout?;
    let buffer_line = crate::plugin::gutter_decorations::buffer_line_at_y(layout, local_pos.y)?;
    let rope = row.view.buffer.rope();
    if buffer_line >= rope.len_lines() {
        return None;
    }
    let line = rope.line(buffer_line);
    let col = (relative_x / row.view.mono.px).max(0.0) as usize;
    let col = col.min(line.len_chars().saturating_sub(1));

    row.link_ranges
        .0
        .iter()
        .position(|r| r.buffer_line == buffer_line && col >= r.start_char && col < r.end_char)
}

/// Ctrl+click observer: when the click lands on a detected URL, open it
/// with the platform's default handler and short-circuit any other
/// ctrl-click behavior. Registered before `on_ctrl_click_goto_definition`
/// so the URL path wins when both could match — and `goto_definition`
/// itself checks `LinkRanges` to back off when a URL covers the click.
#[derive(bevy::ecs::query::QueryData)]
pub struct LinkHitView {
    view: EditorRenderView,
    link_ranges: &'static LinkRanges,
    misc: &'static Misc,
}

pub fn on_ctrl_click_open_url(
    trigger: On<Pointer<Press>>,
    editor_query: Query<LinkHitView, With<CodeEditor>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    if trigger.event().button != PointerButton::Primary {
        return;
    }
    if !(keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight)
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight))
    {
        return;
    }
    let entity = trigger.event().entity;
    let Ok(view) = editor_query.get(entity) else {
        return;
    };
    if !view.misc.links {
        return;
    }
    let Some(local_pos) =
        crate::input::mouse::hit_to_local_px(&trigger.event().hit, view.view.computed)
    else {
        return;
    };

    let inv = view.view.computed.inverse_scale_factor();
    let text_area_left = view.view.computed.content_inset().min_inset.x * inv;
    let relative_x = local_pos.x - text_area_left;
    let Some(layout) = view.view.layout else {
        return;
    };
    let Some(buffer_line) =
        crate::plugin::gutter_decorations::buffer_line_at_y(layout, local_pos.y)
    else {
        return;
    };

    let line = view.view.buffer.rope().line(buffer_line);
    let col = (relative_x / view.view.mono.px).max(0.0) as usize;
    let col = col.min(line.len_chars().saturating_sub(1));

    let Some(hit) = view
        .link_ranges
        .0
        .iter()
        .find(|r| r.buffer_line == buffer_line && col >= r.start_char && col < r.end_char)
    else {
        return;
    };
    open_url(&hit.url);
}

/// Open a URL via the platform default handler. Failures are swallowed —
/// URL launching is best-effort and we don't want a missing handler to
/// propagate as a panic.
fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let cmd = ("open", &[url]);
    #[cfg(target_os = "windows")]
    let cmd = ("cmd", &["/C", "start", "", url]);
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = ("xdg-open", &[url]);

    let _ = std::process::Command::new(cmd.0).args(cmd.1).spawn();
}
