//! TextViewViewport — per-instance viewport dimensions and layout

use bevy::prelude::*;

/// How the viewport's top-left maps to world coordinates.
///
/// Replaces the old `screen_position: Vec2` + `Vec2::ZERO` sentinel pattern,
/// which silently mis-classified views legitimately rendered at world (0,0)
/// as "centered ortho" and forced every consumer to re-implement the branch.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(PartialEq)]
pub enum ViewportOrigin {
    /// Render-to-texture / centered orthographic camera: viewport's top-left
    /// in world space is `(-width/2, +height/2)`. Computed at access time
    /// because it depends on the viewport size.
    CenteredOrtho,
    /// Explicit world-space top-left position (e.g. windowed UI panel).
    ScreenAbsolute(Vec2),
}

impl Default for ViewportOrigin {
    fn default() -> Self {
        Self::CenteredOrtho
    }
}

/// Viewport dimensions and layout for a single text view instance.
///
/// This is a Component (not a Resource) so each text view entity has its own viewport.
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component, Debug)]
pub struct TextViewViewport {
    /// Viewport width in pixels
    pub width: u32,
    /// Viewport height in pixels
    pub height: u32,

    /// How this viewport's top-left maps to world coordinates.
    /// Resolved once via [`origin_position`](Self::origin_position) instead of
    /// the per-glyph branch the old `screen_position` field required.
    pub origin: ViewportOrigin,

    /// Screen-space hit-test rect for mouse interaction (independent of rendering coords).
    /// Set this to the actual screen position even for render-to-texture views.
    pub hit_test_position: bevy::math::Vec2,

    // === Computed Layout ===
    /// Left margin/padding before text starts
    pub text_area_left: f32,
    /// Top margin/padding before text starts
    pub text_area_top: f32,
    /// Width of the gutter area (line numbers, etc.) — 0 for non-editor views
    pub gutter_width: f32,
    /// X position of the separator line between gutter and code
    pub separator_x: f32,
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
            separator_x: 0.0,
        }
    }
}

impl TextViewViewport {
    /// World-space top-left of the viewport, resolving the origin enum.
    /// Use this when you need the raw `(x, y)` in world coords.
    pub fn origin_position(&self) -> Vec2 {
        match self.origin {
            ViewportOrigin::CenteredOrtho => Vec2::new(
                -(self.width as f32) / 2.0,
                self.height as f32 / 2.0,
            ),
            ViewportOrigin::ScreenAbsolute(p) => p,
        }
    }

    /// Calculate the world coordinate of the viewport's left edge
    pub fn world_left(&self) -> f32 {
        self.origin_position().x
    }

    /// Calculate the world coordinate of the viewport's top edge
    pub fn world_top(&self) -> f32 {
        self.origin_position().y
    }
}
