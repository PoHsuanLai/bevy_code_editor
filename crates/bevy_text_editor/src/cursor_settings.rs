//! Per-app cursor appearance + caret/blink helpers.
//!
//! The shape primitives + blink math live here so any text widget — editor,
//! terminal, REPL — can render its own caret without re-deriving the timing
//! curve. The cursor's display position (rope offset → glyph pixel) is the
//! caller's responsibility; this module just turns "I want to draw a caret
//! at row R, x X" into a `RectOverlay` and answers "should the caret be
//! visible right now?".

use bevy::prelude::*;
use bevy_text_engine::{CornerRadii, RectOverlay, RowVertical};
use serde::{Deserialize, Serialize};

use crate::key_repeat::KeyRepeatSettings;

/// App-wide cursor appearance + key-repeat timing. A `Resource` rather
/// than a `Component` because users typically want a single global look —
/// but this could be split into a per-entity component later if needed.
#[derive(Clone, Debug, Resource, Serialize, Deserialize, Reflect)]
#[reflect(Resource, Default, Debug)]
pub struct CursorSettings {
    pub style: CursorStyle,
    /// In pixels; for `Line` and `Underline` styles.
    pub width: f32,
    /// Fraction of line height.
    pub height_multiplier: f32,
    /// Seconds per blink cycle; 0 = no blink.
    pub blink_rate: f32,
    pub smooth_animation: bool,
    pub animation_speed: f32,
    pub key_repeat: KeyRepeatSettings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum CursorStyle {
    Line,
    Block,
    Underline,
}

impl Default for CursorSettings {
    fn default() -> Self {
        Self {
            style: CursorStyle::Line,
            width: 2.0,
            height_multiplier: 1.0,
            blink_rate: 0.5,
            smooth_animation: true,
            animation_speed: 10.0,
            key_repeat: KeyRepeatSettings::default(),
        }
    }
}

/// Half-second pause after movement before the caret begins blinking.
const BLINK_PAUSE_SECS: f64 = 0.5;

/// Returns whether the caret should be drawn this frame.
///
/// `now_secs` is the current time (e.g., `time.elapsed_secs_f64()`).
/// `last_move_secs` is when the cursor last moved (in the same clock).
/// `blink_rate` of `0.0` disables blinking — the caret stays visible.
pub fn cursor_blink_visible(blink_rate: f32, now_secs: f64, last_move_secs: f64) -> bool {
    if blink_rate == 0.0 {
        return true;
    }
    let time_since_move = now_secs - last_move_secs;
    if time_since_move < BLINK_PAUSE_SECS {
        return true;
    }
    let blink_time = (time_since_move - BLINK_PAUSE_SECS) as f32;
    let phase = (blink_time * blink_rate) % 1.0;
    phase < 0.5
}

/// Build a caret `RectOverlay` for the given display row + horizontal pixel.
///
/// Uses `z = 1` to draw above text. The caller is expected to drain previous-
/// frame caret rects (those with `z == 1`) before pushing a new one.
pub fn caret_overlay(
    display_row: u32,
    x_left: f32,
    settings: &CursorSettings,
    color: Color,
) -> RectOverlay {
    let x_right = x_left + caret_width(settings);
    RectOverlay {
        display_row,
        x_range: x_left..x_right,
        vertical: caret_vertical(settings),
        color,
        z: 1,
        corners: CornerRadii::ZERO,
    }
}

fn caret_width(settings: &CursorSettings) -> f32 {
    match settings.style {
        CursorStyle::Line => settings.width,
        CursorStyle::Block | CursorStyle::Underline => settings.width.max(1.0),
    }
}

fn caret_vertical(settings: &CursorSettings) -> RowVertical {
    match settings.style {
        CursorStyle::Line | CursorStyle::Block => RowVertical::Caret {
            height_fraction: settings.height_multiplier,
        },
        CursorStyle::Underline => RowVertical::BottomBand {
            thickness: settings.height_multiplier.max(1.0),
        },
    }
}
