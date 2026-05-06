//! Cursor and selection settings

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Default, Debug)]
pub struct KeyRepeatSettings {
    pub initial_delay_ms: u64,
    pub repeat_delay_ms: u64,
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

impl Default for KeyRepeatSettings {
    fn default() -> Self {
        Self {
            initial_delay_ms: 500,
            repeat_delay_ms: 50,
        }
    }
}

/// VSCode-style cursor-line highlighting.
#[derive(Clone, Debug, Resource, Serialize, Deserialize, Reflect)]
#[reflect(Resource, Default, Debug)]
pub struct CursorLineSettings {
    pub enabled: bool,
    pub style: CursorLineStyle,
    pub border_width: f32,
    pub border_thickness: f32,
    pub border_alpha_multiplier: f32,
    pub border_color: Color,
    pub show_border: bool,
    pub highlight_word: bool,
    pub word_highlight_color: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum CursorLineStyle {
    None,
    Background,
    Border,
    Both,
}

impl Default for CursorLineSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            style: CursorLineStyle::Border,
            border_width: 1.0,
            border_thickness: 1.0,
            border_alpha_multiplier: 1.0,
            border_color: Color::srgba(0.4, 0.4, 0.4, 0.3),
            show_border: true,
            highlight_word: true,
            word_highlight_color: Color::srgba(0.4, 0.4, 0.4, 0.2),
        }
    }
}
