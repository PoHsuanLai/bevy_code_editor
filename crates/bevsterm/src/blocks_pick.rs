//! `On<Pointer<Press>>` observer on the terminal root: hit-test the click
//! against `TerminalBlockState` and emit `TerminalBlockSelected`.

use bevy::prelude::*;
use bevy_instanced_text::{DisplayLayout, ScrollState, TextViewViewport};

use crate::messages::TerminalBlockSelected;
use crate::types::TerminalBlockState;

pub fn on_terminal_block_press(
    trigger: On<Pointer<Press>>,
    q: Query<(
        &TerminalBlockState,
        &DisplayLayout,
        &ScrollState,
        &TextViewViewport,
    )>,
    mut selected_w: MessageWriter<TerminalBlockSelected>,
) {
    if trigger.event().button != PointerButton::Primary {
        return;
    }
    let entity = trigger.event().entity;
    let Ok((state, layout, scroll, viewport)) = q.get(entity) else {
        return;
    };
    let Some(local) = trigger.event().hit.position.map(|p| Vec2::new(p.x, p.y)) else {
        return;
    };

    let click_y = local.y - viewport.text_area_top + scroll.scroll_offset;
    let default_lh = layout.line_height;
    let Some(line) = layout.lines.iter().find(|l| {
        let lh = l.line_height.unwrap_or(default_lh);
        click_y >= l.y_top && click_y < l.y_top + lh
    }) else {
        return;
    };

    let buffer_row = line.buffer_row as i64;
    let Some(block) = state
        .blocks
        .iter()
        .find(|b| buffer_row >= b.prompt_row && buffer_row <= b.end_row)
    else {
        return;
    };

    selected_w.write(TerminalBlockSelected {
        entity,
        block_id: block.id,
    });
}
