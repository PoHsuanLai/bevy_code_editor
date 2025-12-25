//! Search and replace settings

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Search settings
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct SearchSettings {
    /// Case sensitive search by default
    pub case_sensitive: bool,

    /// Whole word search by default
    pub whole_word: bool,

    /// Regular expression search by default
    pub regex: bool,

    /// Wrap around when reaching end/start
    pub wrap_around: bool,

    /// Highlight all matches
    pub highlight_all: bool,
}

impl Default for SearchSettings {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            whole_word: false,
            regex: false,
            wrap_around: true,
            highlight_all: true,
        }
    }
}
