//! View primitives: rope-backed text state, viewport, paint-ready layout, overlays, renderer.

pub mod font;
pub mod layout;
pub mod line_width;
pub mod overlay;
pub mod render;
pub mod snapshot;
pub mod state;
pub mod viewport;

pub use font::FontConfig;
pub use layout::DisplayLayout;
pub use line_width::LineWidthTracker;
pub use overlay::{RectOverlay, RowVertical, TextViewOverlays};
pub use render::{render_layout, GlyphBatchComponent, GlyphInstance, TextViewBatch};
pub use snapshot::{
    trivial_layout, InlineObject, ShapedLine, SimpleTheme, StyleRun, TextDecoration,
};
pub use state::TextViewState;
pub use viewport::{TextViewViewport, ViewportOrigin};
