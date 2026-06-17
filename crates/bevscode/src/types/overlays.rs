//! Per-editor overlay rect batches, one component per visual layer. Each holds
//! the `RectOverlay`s a producer system emits; `merge_overlay_components` folds
//! them into the engine's draw lists.

use bevy::prelude::*;
use bevy_instanced_text::RectOverlay;

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct SelectionRects(pub Vec<RectOverlay>);

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct IndentGuideRects(pub Vec<RectOverlay>);

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct RulerRects(pub Vec<RectOverlay>);

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct FoldHighlightRects(pub Vec<RectOverlay>);

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct CaretRects(pub Vec<RectOverlay>);

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct CursorLineRects(pub Vec<RectOverlay>);

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct BracketMatchRects(pub Vec<RectOverlay>);

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct WhitespaceRects(pub Vec<RectOverlay>);
