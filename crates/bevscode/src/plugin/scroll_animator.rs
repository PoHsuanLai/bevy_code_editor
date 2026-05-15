//! Smooth-scroll animator for code-editor entities.
//!
//! Hosts that want VS-Code-style eased scrolling attach a [`ScrollAnimator`]
//! to an entity carrying `bevy::ui::ScrollPosition`. Set
//! [`ScrollAnimator::target`] to request a scroll position; the animator
//! tweens `ScrollPosition` toward it each frame using the same two-stage
//! composite easing the engine used to ship internally.
//!
//! Instant scrolls don't go through the animator at all — write
//! `ScrollPosition` directly and the animator stops on the next frame
//! (it always re-anchors against the current `ScrollPosition`).

use bevy::prelude::*;
use bevy::ui::ScrollPosition;

/// Per-entity smooth-scroll state. Hosts write `target`; the
/// [`drive_scroll_animator`] system advances `ScrollPosition` toward it.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct ScrollAnimator {
    /// Where the host wants the viewport to land. Set this to scroll.
    pub target: Vec2,
    /// Per-axis easing length in seconds. `0.0` = instant.
    pub duration: f32,
    #[reflect(ignore)]
    anim: Option<AxisAnim>,
}

impl ScrollAnimator {
    pub fn with_duration(duration: f32) -> Self {
        Self { target: Vec2::ZERO, duration, anim: None }
    }
}

#[derive(Clone, Debug)]
struct AxisAnim {
    from: Vec2,
    to: Vec2,
    elapsed: f32,
    duration: f32,
    composite_y: Option<CompositeStops>,
}

#[derive(Clone, Debug)]
struct CompositeStops {
    stop1: f32,
    stop2: f32,
    split: f32,
}

const SCROLL_BACKDATE_SECS: f32 = 0.010;
const SCROLL_BACKDATE_DURATION: f32 = 0.010;
const COMPOSITE_SPLIT: f32 = 0.33;
const COMPOSITE_VIEWPORT_THRESHOLD: f32 = 2.5;
const COMPOSITE_STOP_INSET: f32 = 0.75;

fn build_composite(from: f32, to: f32, viewport_size: f32) -> Option<CompositeStops> {
    if viewport_size > 0.0 && (to - from).abs() > COMPOSITE_VIEWPORT_THRESHOLD * viewport_size {
        let inset = COMPOSITE_STOP_INSET * viewport_size;
        let (stop1, stop2) = if from < to {
            (from + inset, to - inset)
        } else {
            (from - inset, to + inset)
        };
        Some(CompositeStops { stop1, stop2, split: COMPOSITE_SPLIT })
    } else {
        None
    }
}

#[inline]
fn ease_out_cubic(t: f32) -> f32 {
    let inv = 1.0 - t;
    1.0 - inv * inv * inv
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn sample_axis(from: f32, to: f32, t: f32, composite: Option<&CompositeStops>) -> f32 {
    match composite {
        None => lerp(from, to, ease_out_cubic(t)),
        Some(c) => {
            if t < c.split {
                let local = t / c.split;
                lerp(from, c.stop1, ease_out_cubic(local))
            } else {
                let local = (t - c.split) / (1.0 - c.split);
                lerp(c.stop2, to, ease_out_cubic(local))
            }
        }
    }
}

/// Plugin registering the scroll-animator system. Add this to your app once.
pub struct ScrollAnimatorPlugin;

impl Plugin for ScrollAnimatorPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<ScrollAnimator>()
            .add_systems(Update, drive_scroll_animator);
    }
}

/// Advances every animator one frame, writing the eased value to
/// `ScrollPosition`. Only entities carrying both components are touched.
pub fn drive_scroll_animator(
    time: Res<Time>,
    mut q: Query<(&mut ScrollAnimator, &mut ScrollPosition, &bevy::ui::ComputedNode)>,
) {
    let dt = time.delta_secs();
    for (mut animator, mut scroll, computed) in q.iter_mut() {
        let viewport = computed.size() * computed.inverse_scale_factor();
        let target = animator.target;

        // Restart the anim whenever the target changed since we last saw it.
        let needs_new_anim = match &animator.anim {
            Some(a) => (a.to - target).length() > f32::EPSILON,
            None => (scroll.0 - target).length() > 0.5,
        };

        if needs_new_anim {
            let composite_y = build_composite(scroll.y, target.y, viewport.y);
            animator.anim = Some(AxisAnim {
                from: scroll.0,
                to: target,
                elapsed: SCROLL_BACKDATE_SECS,
                duration: (animator.duration + SCROLL_BACKDATE_DURATION).max(0.001),
                composite_y,
            });
        }

        let Some(anim) = animator.anim.as_mut() else {
            continue;
        };
        anim.elapsed += dt;
        if anim.elapsed >= anim.duration {
            scroll.0 = anim.to;
            animator.anim = None;
            continue;
        }
        let t = (anim.elapsed / anim.duration).clamp(0.0, 1.0);
        let new_y = sample_axis(anim.from.y, anim.to.y, t, anim.composite_y.as_ref());
        let new_x = sample_axis(anim.from.x, anim.to.x, t, None);
        scroll.0 = Vec2::new(new_x, new_y);
    }
}
