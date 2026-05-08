//! Text wrapping settings

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Soft-wrap settings. Disabled by default; enable and set `wrap_column`
/// to a fixed column count, or leave it `None` to wrap at the viewport edge.
#[derive(Clone, Debug, Resource, Serialize, Deserialize, Reflect)]
#[reflect(Resource, Default, Debug)]
pub struct WrappingSettings {
    pub enabled: bool,
    /// `None` = wrap at viewport width.
    pub wrap_column: Option<usize>,
    pub indent_wrapped_lines: bool,
}

impl Default for WrappingSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            wrap_column: None,
            indent_wrapped_lines: true,
        }
    }
}
