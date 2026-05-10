//! Text wrapping settings

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Per-editor soft-wrap settings. Disabled by default; enable and set
/// `wrap_column` to a fixed column count, or leave it `None` to wrap at the viewport edge.
#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct Wrapping {
    pub enabled: bool,
    /// `None` = wrap at viewport width.
    pub wrap_column: Option<usize>,
    pub indent_wrapped_lines: bool,
}

impl Default for Wrapping {
    fn default() -> Self {
        Self {
            enabled: false,
            wrap_column: None,
            indent_wrapped_lines: true,
        }
    }
}
