//! LSP (Language Server Protocol) settings

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// LSP settings
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct LspSettings {
    /// Auto-completion settings
    pub completion: CompletionSettings,

    /// Hover information settings
    pub hover: HoverSettings,
}

/// Auto-completion settings
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletionSettings {
    /// Enable auto-completion
    pub enabled: bool,

    /// Trigger characters that auto-open completion
    pub trigger_characters: Vec<String>,

    /// Delay before showing completion (milliseconds)
    pub delay_ms: u64,

    /// Maximum number of completion items to show
    pub max_items: usize,

    /// Completion window width (pixels)
    pub window_width: f32,

    /// Completion window background color
    pub window_background: Color,

    /// Selected item background color
    pub selected_background: Color,

    /// Completion text color
    pub text_color: Color,

    /// Selected item text color
    pub selected_text_color: Color,
}

/// Hover information settings
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HoverSettings {
    /// Enable hover information
    pub enabled: bool,

    /// Delay before showing hover (milliseconds)
    pub delay_ms: u64,

    /// Hover window max width (pixels)
    pub max_width: f32,

    /// Hover window background color
    pub background_color: Color,

    /// Hover text color
    pub text_color: Color,

    /// Hover border color
    pub border_color: Color,

    /// Hover border width (pixels)
    pub border_width: f32,
}

impl Default for LspSettings {
    fn default() -> Self {
        Self {
            completion: CompletionSettings::default(),
            hover: HoverSettings::default(),
        }
    }
}

impl Default for CompletionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            trigger_characters: vec![".".to_string(), "::".to_string()],
            delay_ms: 100,
            max_items: 10,
            window_width: 300.0,
            window_background: Color::srgba(0.15, 0.15, 0.15, 0.95),
            selected_background: Color::srgb(0.25, 0.35, 0.5),
            text_color: Color::srgb(0.85, 0.85, 0.85),
            selected_text_color: Color::srgb(1.0, 1.0, 1.0),
        }
    }
}

impl Default for HoverSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            delay_ms: 300,
            max_width: 500.0,
            background_color: Color::srgba(0.15, 0.15, 0.15, 0.95),
            text_color: Color::srgb(0.85, 0.85, 0.85),
            border_color: Color::srgb(0.3, 0.3, 0.3),
            border_width: 1.0,
        }
    }
}
