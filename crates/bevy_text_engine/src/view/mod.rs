//! View primitives: rope-backed text state, viewport, paint-ready layout, overlays, renderer.

pub mod font;
pub mod layout;
pub mod layout_builder;
pub mod overlay;
pub mod plugin;
pub mod render;
pub mod snapshot;
pub mod state;
pub mod styling;
pub mod viewport;

pub use font::FontConfig;
pub use layout::DisplayLayout;
pub use layout_builder::{
    approx_display_rows_for_line, slice_runs, wrap_into_rows, LayoutProduceSet, WrapRow,
};
pub use overlay::{RectOverlay, RowVertical, TextViewOverlays};
pub use plugin::{TextEnginePlugin, TextEnginePlugins, TextView, TextViewBatchEntity, TextViewRenderSet};
pub use render::{render_layout, GlyphBatchComponent, GlyphInstance, TextViewBatch};
pub use snapshot::{
    trivial_layout, InlineObject, ShapedLine, SimpleTheme, StyleRun, TextDecoration,
};
pub use state::TextViewState;
pub use styling::{LayoutWrap, LineFilter, LineStyleSource, LineStyling, LineVisibility, RunWithText};
pub use viewport::{TextViewViewport, ViewportOrigin};
