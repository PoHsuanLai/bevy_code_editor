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
pub mod theme;
pub mod viewport;

pub use font::{FontConfig, FontSynthesis};
pub use layout::DisplayLayout;
pub use layout_builder::{
    approx_display_rows_for_line, slice_runs, visible_buffer_range, wrap_into_rows,
    LayoutProduceSet, WrapRow,
};
pub use overlay::{CornerRadii, RectOverlay, RowVertical, TextViewOverlays};
pub use plugin::{TextEnginePlugin, TextEnginePlugins, TextView, TextViewBatchEntity, TextViewRenderSet};
pub use render::{render_layout, FontFaces, GlyphBatchComponent, GlyphInstance, TextViewBatch};
pub use snapshot::{
    trivial_layout, Block, BlockBorder, BlockDecoration, BlockLayoutConfig, BlockRect,
    ShapedLine, StyleRun, TextDecoration,
};
pub use state::{ContentMetrics, ScrollState, TextBuffer};
pub use styling::{BlockList, HiddenLines, LayoutWrap, LineStyles, RunWithText};
pub use theme::RenderTheme;
pub use viewport::{TextViewViewport, ViewportOrigin};
