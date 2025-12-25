//! Cursor and selection settings

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Cursor settings
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct CursorSettings {
    /// Cursor style
    pub style: CursorStyle,

    /// Cursor width in pixels (for line/underline styles)
    pub width: f32,

    /// Cursor height as multiplier of line height
    pub height_multiplier: f32,

    /// Blink rate in seconds (0 = no blink)
    pub blink_rate: f32,

    /// Smooth cursor animation
    pub smooth_animation: bool,

    /// Animation speed (higher = faster)
    pub animation_speed: f32,

    /// Key repeat settings
    pub key_repeat: KeyRepeatSettings,
}

/// Key repeat settings for cursor movement
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyRepeatSettings {
    /// Initial delay before repeat starts (milliseconds)
    pub initial_delay_ms: u64,

    /// Delay between repeats (milliseconds)
    pub repeat_delay_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

/// Cursor line highlighting settings (VSCode-style)
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct CursorLineSettings {
    /// Enable cursor line highlighting
    pub enabled: bool,

    /// Highlight style
    pub style: CursorLineStyle,

    /// Border width (for Border style)
    pub border_width: f32,

    /// Border thickness (for Border style)
    pub border_thickness: f32,

    /// Border alpha multiplier
    pub border_alpha_multiplier: f32,

    /// Border color
    pub border_color: Color,

    /// Show border
    pub show_border: bool,

    /// Highlight word under cursor
    pub highlight_word: bool,

    /// Word highlight color
    pub word_highlight_color: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
