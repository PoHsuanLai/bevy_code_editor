//! Scrollbar settings

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Resource, Serialize, Deserialize, Reflect)]
#[reflect(Resource, Default, Debug)]
pub struct ScrollbarSettings {
    pub enabled: bool,
    pub width: f32,
    pub background_color: Color,
    pub thumb_color: Color,
    pub thumb_hover_color: Color,
    pub auto_hide: bool,
    pub fade_duration: f32,
}

impl Default for ScrollbarSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            width: 12.0,
            background_color: Color::srgba(0.2, 0.2, 0.2, 0.3),
            thumb_color: Color::srgba(0.5, 0.5, 0.5, 0.5),
            thumb_hover_color: Color::srgba(0.6, 0.6, 0.6, 0.7),
            auto_hide: false,
            fade_duration: 0.3,
        }
    }
}
