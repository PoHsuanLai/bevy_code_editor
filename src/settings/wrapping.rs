//! Text wrapping settings

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Text wrapping settings
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct WrappingSettings {
    /// Enable line wrapping
    pub enabled: bool,

    /// Wrap column (None = wrap at viewport width)
    pub wrap_column: Option<usize>,

    /// Indent wrapped lines
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
