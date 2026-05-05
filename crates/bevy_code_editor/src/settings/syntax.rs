//! Per-entity syntax highlighting palette.
//!
//! Component on the `CodeEditor` entity (cascaded via `#[require]` when
//! `tree-sitter` is on). Capture name → color mapping lives in
//! `crate::syntax::map_highlight_color`.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct SyntaxTheme {
    pub keyword: Color,
    pub function: Color,
    pub method: Color,
    pub string: Color,
    pub number: Color,
    pub comment: Color,
    pub variable: Color,
    pub operator: Color,
    pub constant: Color,
    pub type_name: Color,
    pub parameter: Color,
    pub property: Color,
    pub punctuation: Color,
    pub label: Color,
    pub constructor: Color,
    pub escape: Color,
    pub embedded: Color,
}

impl Default for SyntaxTheme {
    fn default() -> Self {
        Self {
            keyword: Color::srgb(0.847, 0.486, 0.659),
            function: Color::srgb(0.863, 0.863, 0.549),
            method: Color::srgb(0.863, 0.863, 0.549),
            string: Color::srgb(0.808, 0.616, 0.502),
            number: Color::srgb(0.698, 0.843, 0.749),
            comment: Color::srgb(0.384, 0.514, 0.376),
            variable: Color::srgb(0.608, 0.788, 0.933),
            operator: Color::srgb(0.827, 0.827, 0.827),
            constant: Color::srgb(0.298, 0.686, 0.914),
            type_name: Color::srgb(0.298, 0.686, 0.914),
            parameter: Color::srgb(0.608, 0.788, 0.933),
            property: Color::srgb(0.608, 0.788, 0.933),
            punctuation: Color::srgb(0.827, 0.827, 0.827),
            label: Color::srgb(0.847, 0.486, 0.659),
            constructor: Color::srgb(0.298, 0.686, 0.914),
            escape: Color::srgb(0.863, 0.863, 0.549),
            embedded: Color::srgb(0.827, 0.827, 0.827),
        }
    }
}
