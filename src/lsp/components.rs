//! LSP UI marker components
//!
//! These components contain all data needed to render LSP UI elements.
//! Default render systems query these components and spawn visual entities.
//! Users can disable the default systems and provide their own implementations.
//!
//! # Architecture
//!
//! The component-based UI follows this pattern:
//!
//! 1. **State resources** (e.g., `CompletionState`) hold the raw LSP data
//! 2. **Sync systems** create/update marker entities with `*PopupData` components
//! 3. **Render systems** query marker components and spawn visual children
//!
//! This separation allows users to:
//! - Replace render systems with custom implementations
//! - Query the same marker components as the default systems
//! - Use the theme resource or provide their own styling
//!
//! # Example
//!
//! ```rust,ignore
//! // Custom render system for completion popup
//! fn my_completion_renderer(
//!     query: Query<(Entity, &CompletionPopupData)>,
//!     theme: Res<LspUiTheme>,
//!     mut commands: Commands,
//! ) {
//!     for (entity, popup) in query.iter() {
//!         // Your custom rendering logic
//!     }
//! }
//! ```

use bevy::prelude::*;

use super::state::UnifiedCompletionItem;

/// Marker component for the completion popup entity.
/// Contains all data needed to render the completion UI.
#[derive(Component, Clone, Debug)]
pub struct CompletionPopupData {
    /// Position in screen space (top-left of popup)
    pub position: Vec2,
    /// List of completion items to display
    pub items: Vec<CompletionItemData>,
    /// Index of the currently selected item
    pub selected_index: usize,
    /// Scroll offset (first visible item index)
    pub scroll_offset: usize,
    /// Maximum number of visible items
    pub max_visible: usize,
    /// Calculated popup width
    pub width: f32,
    /// Calculated popup height
    pub height: f32,
}

/// Data for a single completion item
#[derive(Clone, Debug)]
pub struct CompletionItemData {
    /// Display label
    pub label: String,
    /// Optional detail text
    pub detail: Option<String>,
    /// Kind icon (e.g., "ƒ" for function)
    pub kind_icon: String,
    /// Whether this is a word completion (vs LSP)
    pub is_word: bool,
    /// Text to insert when selected
    pub insert_text: String,
}

impl From<&UnifiedCompletionItem> for CompletionItemData {
    fn from(item: &UnifiedCompletionItem) -> Self {
        Self {
            label: item.label().to_string(),
            detail: item.detail().map(|s| s.to_string()),
            kind_icon: item.kind_icon().to_string(),
            is_word: item.is_word(),
            insert_text: item.insert_text().to_string(),
        }
    }
}

/// Marker component for the hover popup entity.
/// Contains all data needed to render the hover UI.
#[derive(Component, Clone, Debug)]
pub struct HoverPopupData {
    /// Position in screen space (top-left of popup)
    pub position: Vec2,
    /// Content to display (markdown/plain text)
    pub content: String,
    /// Calculated popup width
    pub width: f32,
    /// Calculated popup height
    pub height: f32,
}

/// Marker component for the signature help popup entity.
/// Contains all data needed to render signature help.
#[derive(Component, Clone, Debug)]
pub struct SignatureHelpPopupData {
    /// Position in screen space (bottom-left of popup, shows above cursor)
    pub position: Vec2,
    /// Signature label text
    pub label: String,
    /// Active parameter index (for highlighting)
    pub active_parameter: usize,
    /// Parameter ranges in the label (start, end) for highlighting
    pub parameter_ranges: Vec<(usize, usize)>,
    /// Total number of signatures (for "1/3" indicator)
    pub total_signatures: usize,
    /// Current signature index
    pub current_index: usize,
    /// Calculated popup width
    pub width: f32,
    /// Calculated popup height
    pub height: f32,
}

/// Marker component for the code actions popup entity.
/// Contains all data needed to render the code actions menu.
#[derive(Component, Clone, Debug)]
pub struct CodeActionsPopupData {
    /// Position in screen space (near the gutter)
    pub position: Vec2,
    /// List of code actions
    pub actions: Vec<CodeActionItemData>,
    /// Index of the currently selected action
    pub selected_index: usize,
    /// Calculated popup width
    pub width: f32,
    /// Calculated popup height
    pub height: f32,
}

/// Data for a single code action
#[derive(Clone, Debug)]
pub struct CodeActionItemData {
    /// Display title
    pub title: String,
    /// Action kind icon
    pub icon: String,
    /// Whether this is a preferred/quick fix action
    pub is_preferred: bool,
}

/// Marker component for the rename input entity.
/// Contains all data needed to render the inline rename dialog.
#[derive(Component, Clone, Debug)]
pub struct RenameInputData {
    /// Position in screen space (at the symbol location)
    pub position: Vec2,
    /// Current input text
    pub text: String,
    /// Original symbol text (for placeholder/comparison)
    pub original_text: String,
    /// Cursor position within the text
    pub cursor_position: usize,
    /// Calculated input width
    pub width: f32,
    /// Calculated input height
    pub height: f32,
}

/// Marker component for a single inlay hint.
/// Contains all data needed to render one inlay hint.
#[derive(Component, Clone, Debug)]
pub struct InlayHintData {
    /// Position in screen space
    pub position: Vec2,
    /// Hint label text
    pub label: String,
    /// Hint kind for coloring
    pub kind: InlayHintKind,
    /// Line number (for tracking)
    pub line: u32,
    /// Character position (for tracking)
    pub character: u32,
}

/// Kind of inlay hint (for styling)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlayHintKind {
    /// Type annotation hint
    Type,
    /// Parameter name hint
    Parameter,
    /// Other/unknown hint
    Other,
}

/// Marker component for a single document highlight.
/// Contains all data needed to render one highlight rectangle.
#[derive(Component, Clone, Debug)]
pub struct DocumentHighlightData {
    /// Position in screen space (center of highlight)
    pub position: Vec2,
    /// Width of the highlight
    pub width: f32,
    /// Height of the highlight
    pub height: f32,
    /// Whether this is a write reference
    pub is_write: bool,
    /// Line number (for tracking)
    pub line: u32,
}

/// Marker for entities that are part of the LSP UI.
/// Used for cleanup and querying all LSP UI entities.
#[derive(Component, Clone, Copy, Debug)]
pub struct LspUiElement;

/// Marker for the visual/rendered part of an LSP UI element.
/// The data component (e.g., `CompletionPopupData`) is on the parent entity,
/// and this marker is on the spawned visual children.
#[derive(Component, Clone, Copy, Debug)]
pub struct LspUiVisual;
