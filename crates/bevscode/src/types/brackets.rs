//! Bracket-match state and highlight marker.

use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Debug, PartialEq)]
pub struct BracketMatch {
    pub cursor_bracket_pos: usize,
    pub matching_bracket_pos: usize,
}

#[derive(Component, Default, Clone, Debug, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct BracketMatchState {
    pub current_match: Option<BracketMatch>,
}

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct BracketMatchHighlight;
