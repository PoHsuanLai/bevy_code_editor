//! Editor-side cursor settings.
//!
//! `CursorSettings` and `CursorStyle` (caret shape, blink, key-repeat
//! timing) live in `bevy_text_editor`; the terminal needs them too. We
//! re-export them here so `crate::settings::*` callers don't notice.
//!
//! `CursorLine` (the editor's VSCode-style line-highlight band)
//! stays here — it's editor chrome, not a text-widget primitive.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub use bevy_text_editor::{CursorSettings, CursorStyle};

/// Per-editor VSCode-style cursor-line highlighting.
#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct CursorLine {
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

/// Visual style for the cursor-line band.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum CursorLineStyle {
    /// No highlight.
    None,
    /// Filled background band behind the active line.
    Background,
    /// Top/bottom border lines only.
    Border,
    /// Both background fill and border.
    Both,
}

impl Default for CursorLine {
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
