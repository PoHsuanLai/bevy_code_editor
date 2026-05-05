//! Trait Components that plug into [`super::layout_builder::produce_layouts`].
//!
//! The engine's layout system queries each `TextView` entity for these
//! components and uses them to decide which buffer lines to skip (folding)
//! and how to color each line (syntax / markdown). They're optional: an
//! entity without `LineFilter` shows every line; one without
//! `LineStyleSource` renders with `DisplayLayout::default_fg`.
//!
//! Both Components wrap `Arc<dyn Trait + Send + Sync>` so cloning is cheap
//! and consumers can share the styling state across systems (e.g. the
//! editor's tree-sitter writer and the engine's reader both hold the same
//! `Arc`).
//!
//! Reflection isn't supported — `dyn Trait` isn't reflectable. The engine
//! still registers the concrete component type with bevy so app inspectors
//! see it; the inner trait object is opaque.

use bevy::prelude::*;
use std::sync::Arc;

use super::snapshot::StyleRun;

/// Per-line visibility predicate. Implementors return `false` for buffer
/// lines that are folded (or otherwise hidden) and should be skipped during
/// layout.
///
/// Called many times per frame in tight loops. Implementations should keep
/// the body short — typically a `HashSet`/`Vec` lookup. Mutable state goes
/// behind interior mutability (e.g. `RwLock`); the `&self` receiver lets
/// the engine call it without locking the surrounding `Query`.
pub trait LineVisibility: Send + Sync + 'static {
    fn is_visible(&self, buffer_line: usize) -> bool;
}

/// Per-line styling source. Returns the styled runs (with their text
/// payloads) for a buffer line; the layout builder concatenates the run
/// texts to produce the line that gets shaped.
///
/// `version()` is the engine's invalidation hook — bump it whenever the
/// styling output for any line might have changed (e.g. tree-sitter
/// reparsed). The engine folds it into the layout fingerprint so a finished
/// parse triggers a re-layout without explicit signalling.
pub trait LineStyling: Send + Sync + 'static {
    /// Compute styled runs for a buffer line.
    ///
    /// Empty `Vec` ⇒ render the line plain — the renderer falls back to
    /// `DisplayLayout::default_fg`. The engine ignores `byte_range` on the
    /// returned runs (it overwrites them based on `text` concatenation).
    fn style(&self, buffer_line: usize, text: &str) -> Vec<RunWithText>;

    /// Monotonic version counter. Bumped when the styling output may have
    /// changed; folded into the engine's layout-fingerprint cache.
    fn version(&self) -> u64 {
        0
    }
}

/// Optional Component on a `TextView` entity selecting which buffer lines
/// the engine renders. Without this Component every line is visible.
///
/// Cheap to clone (`Arc` bump). Not Reflect: wraps a `dyn Trait`.
#[derive(Component, Clone)]
pub struct LineFilter(pub Arc<dyn LineVisibility>);

impl LineFilter {
    /// Convenience wrapper for `Arc::new(value)`.
    pub fn new<T: LineVisibility>(value: T) -> Self {
        Self(Arc::new(value))
    }
}

/// Optional Component on a `TextView` entity producing styled runs per
/// line. Without this Component every line renders with `default_fg`.
///
/// Cheap to clone (`Arc` bump). Not Reflect: wraps a `dyn Trait`.
#[derive(Component, Clone)]
pub struct LineStyleSource(pub Arc<dyn LineStyling>);

impl LineStyleSource {
    /// Convenience wrapper for `Arc::new(value)`.
    pub fn new<T: LineStyling>(value: T) -> Self {
        Self(Arc::new(value))
    }
}

/// Soft-wrap configuration Component.
///
/// `budget_px = None` disables wrap (one display row per visible buffer
/// line). When set, lines wider than `budget_px` split into multiple
/// continuation rows, each inset by `indent_px`.
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component, Default)]
pub struct LayoutWrap {
    /// Pixel width budget for a row. `None` ⇒ no wrap.
    pub budget_px: Option<f32>,
    /// Continuation-row left inset in pixels.
    pub indent_px: f32,
}

impl Default for LayoutWrap {
    fn default() -> Self {
        Self {
            budget_px: None,
            indent_px: 0.0,
        }
    }
}

/// One styled run plus its text payload, returned by
/// [`LineStyling::style`]. The producer concatenates `text` payloads to
/// form the line that gets shaped, then rebases each run's `byte_range` to
/// match.
///
/// `run.byte_range` on input is ignored — the producer overwrites it with
/// the correct range based on the position of `text` in the concatenation.
/// Set it to `0..0` (or anything) when constructing.
#[derive(Clone, Debug)]
pub struct RunWithText {
    pub text: String,
    pub run: StyleRun,
}
