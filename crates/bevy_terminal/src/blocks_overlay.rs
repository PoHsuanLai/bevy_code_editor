//! Per-frame: paint a rounded background for each terminal block.

use bevy::prelude::*;
use bevy_text_engine::{
    for_each_row_in_buffer_span, BlockDecorTheme, DisplayLayout, RectOverlay, RowVertical,
    TextViewOverlays,
};

use crate::types::{BlockStatus, TerminalBlockState, TerminalColorPalette};

const BLOCK_OVERLAY_Z: i8 = -1;

pub fn paint_block_overlays(
    mut q: Query<(
        &TerminalBlockState,
        &TerminalColorPalette,
        &DisplayLayout,
        &BlockDecorTheme,
        &mut TextViewOverlays,
    )>,
) {
    for (state, palette, layout, decor, mut overlays) in q.iter_mut() {
        overlays.rects.retain(|r| r.z != BLOCK_OVERLAY_Z);

        for block in &state.blocks {
            let color = match block.status {
                BlockStatus::Running => palette.block_active,
                BlockStatus::Completed => palette.block_completed,
                BlockStatus::Pending => continue,
            };
            let first = block.prompt_row.max(0) as u32;
            let last = block.end_row.max(block.prompt_row).max(0) as u32;
            for_each_row_in_buffer_span(layout, first, last, |display_row, pos| {
                overlays.rects.push(RectOverlay {
                    display_row,
                    x_range: 0.0..f32::MAX,
                    vertical: RowVertical::FullLeaded,
                    color,
                    z: BLOCK_OVERLAY_Z,
                    corners: pos.corners(decor.code_corner_radius),
                });
            });
        }

        overlays.version = overlays.version.wrapping_add(1);
    }
}
