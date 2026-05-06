//! TextViewViewport — per-instance viewport dimensions and layout

use bevy::prelude::*;

/// How the viewport's top-left maps to world coordinates.
///
/// Replaces the old `screen_position: Vec2` + `Vec2::ZERO` sentinel pattern,
/// which silently mis-classified views legitimately rendered at world (0,0)
/// as "centered ortho" and forced every consumer to re-implement the branch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Default, PartialEq)]
pub enum ViewportOrigin {
    /// Render-to-texture / centered orthographic camera: viewport's top-left
    /// in world space is `(-width/2, +height/2)`. Computed at access time
    /// because it depends on the viewport size.
    #[default]
    CenteredOrtho,
    /// Explicit world-space top-left position (e.g. windowed UI panel).
    ScreenAbsolute(Vec2),
}

/// Per-entity viewport dimensions. Component (not Resource) so each text view
/// has its own.
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component, Debug)]
pub struct TextViewViewport {
    pub width: u32,
    pub height: u32,
    /// How the viewport's top-left maps to world coords. Resolved via
    /// [`origin_position`](Self::origin_position) instead of a per-glyph branch.
    pub origin: ViewportOrigin,
    /// Screen-space hit-test position — set this even for render-to-texture views.
    pub hit_test_position: bevy::math::Vec2,
    pub text_area_left: f32,
    pub text_area_top: f32,
    /// 0 for views without a gutter. Editor IDE chrome (the line numbers
    /// gutter) draws its separator at this x; non-editor views ignore it.
    pub gutter_width: f32,
}

impl Default for TextViewViewport {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            origin: ViewportOrigin::CenteredOrtho,
            hit_test_position: bevy::math::Vec2::ZERO,
            text_area_left: 0.0,
            text_area_top: 8.0,
            gutter_width: 0.0,
        }
    }
}

impl TextViewViewport {
    pub fn origin_position(&self) -> Vec2 {
        match self.origin {
            ViewportOrigin::CenteredOrtho => Vec2::new(
                -(self.width as f32) / 2.0,
                self.height as f32 / 2.0,
            ),
            ViewportOrigin::ScreenAbsolute(p) => p,
        }
    }

    pub fn world_left(&self) -> f32 {
        self.origin_position().x
    }

    pub fn world_top(&self) -> f32 {
        self.origin_position().y
    }
}
