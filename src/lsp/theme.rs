//! LSP UI theme configuration
//!
//! This module provides theming for LSP UI elements (completion popup, hover, etc.).
//! Users can customize colors, sizing, and styling by modifying the `LspUiTheme` resource.

use bevy::prelude::*;

/// Theme configuration for LSP UI elements
///
/// This resource controls the visual appearance of all LSP-related UI:
/// - Completion popup
/// - Hover popup
/// - Signature help
/// - Code actions
/// - Inlay hints
/// - Document highlights
/// - Rename input
///
/// # Example
/// ```rust,ignore
/// fn setup(mut commands: Commands) {
///     commands.insert_resource(LspUiTheme {
///         completion: CompletionTheme {
///             background: Color::srgba(0.1, 0.1, 0.1, 0.95),
///             selected_background: Color::srgba(0.3, 0.5, 0.9, 0.8),
///             ..default()
///         },
///         ..default()
///     });
/// }
/// ```
#[derive(Resource, Clone, Debug)]
pub struct LspUiTheme {
    /// Theme for completion popup
    pub completion: CompletionTheme,
    /// Theme for hover popup
    pub hover: HoverTheme,
    /// Theme for signature help
    pub signature_help: SignatureHelpTheme,
    /// Theme for code actions
    pub code_actions: CodeActionsTheme,
    /// Theme for inlay hints
    pub inlay_hints: InlayHintsTheme,
    /// Theme for document highlights
    pub document_highlights: DocumentHighlightsTheme,
    /// Theme for rename input
    pub rename: RenameTheme,
    /// Common styling
    pub common: CommonTheme,
}

impl Default for LspUiTheme {
    fn default() -> Self {
        Self {
            completion: CompletionTheme::default(),
            hover: HoverTheme::default(),
            signature_help: SignatureHelpTheme::default(),
            code_actions: CodeActionsTheme::default(),
            inlay_hints: InlayHintsTheme::default(),
            document_highlights: DocumentHighlightsTheme::default(),
            rename: RenameTheme::default(),
            common: CommonTheme::default(),
        }
    }
}


/// Theme for completion popup
#[derive(Clone, Debug)]
pub struct CompletionTheme {
    /// Background color
    pub background: Color,
    /// Border color
    pub border: Color,
    /// Border width in pixels
    pub border_width: f32,
    /// Selected item background
    pub selected_background: Color,
    /// Text color for labels
    pub text_color: Color,
    /// Text color for word completions (dimmer)
    pub word_text_color: Color,
    /// Text color for detail text
    pub detail_color: Color,
    /// Icon/kind indicator color
    pub icon_color: Color,
    /// Minimum popup width
    pub min_width: f32,
    /// Maximum popup width
    pub max_width: f32,
    /// Padding inside the popup
    pub padding: f32,
    /// Z-index for layering
    pub z_index: f32,
}

impl Default for CompletionTheme {
    fn default() -> Self {
        Self {
            background: Color::srgba(0.15, 0.15, 0.15, 0.95),
            border: Color::srgba(0.3, 0.3, 0.3, 1.0),
            border_width: 1.0,
            selected_background: Color::srgba(0.2, 0.4, 0.8, 0.8),
            text_color: Color::WHITE,
            word_text_color: Color::srgba(0.9, 0.9, 0.8, 1.0),
            detail_color: Color::srgba(0.7, 0.7, 0.7, 1.0),
            icon_color: Color::srgba(0.6, 0.6, 0.6, 1.0),
            min_width: 200.0,
            max_width: 600.0,
            padding: 5.0,
            z_index: 100.0,
        }
    }
}

/// Theme for hover popup
#[derive(Clone, Debug)]
pub struct HoverTheme {
    /// Background color
    pub background: Color,
    /// Border color
    pub border: Color,
    /// Border width in pixels
    pub border_width: f32,
    /// Text color
    pub text_color: Color,
    /// Code block background
    pub code_background: Color,
    /// Minimum popup width
    pub min_width: f32,
    /// Maximum popup width
    pub max_width: f32,
    /// Padding inside the popup
    pub padding: f32,
    /// Z-index for layering
    pub z_index: f32,
}

impl Default for HoverTheme {
    fn default() -> Self {
        Self {
            background: Color::srgba(0.1, 0.1, 0.1, 0.95),
            border: Color::srgba(0.3, 0.3, 0.3, 1.0),
            border_width: 1.0,
            text_color: Color::WHITE,
            code_background: Color::srgba(0.08, 0.08, 0.08, 1.0),
            min_width: 100.0,
            max_width: 600.0,
            padding: 10.0,
            z_index: 100.0,
        }
    }
}

/// Theme for signature help popup
#[derive(Clone, Debug)]
pub struct SignatureHelpTheme {
    /// Background color
    pub background: Color,
    /// Border color
    pub border: Color,
    /// Border width in pixels
    pub border_width: f32,
    /// Text color
    pub text_color: Color,
    /// Active parameter highlight color
    pub active_param_color: Color,
    /// Signature counter color (e.g., "1/3")
    pub counter_color: Color,
    /// Padding inside the popup
    pub padding: f32,
    /// Z-index for layering
    pub z_index: f32,
}

impl Default for SignatureHelpTheme {
    fn default() -> Self {
        Self {
            background: Color::srgba(0.12, 0.12, 0.12, 0.95),
            border: Color::srgba(0.3, 0.3, 0.3, 1.0),
            border_width: 1.0,
            text_color: Color::WHITE,
            active_param_color: Color::srgba(0.4, 0.6, 1.0, 1.0),
            counter_color: Color::srgba(0.6, 0.6, 0.6, 1.0),
            padding: 8.0,
            z_index: 100.0,
        }
    }
}

/// Theme for code actions menu
#[derive(Clone, Debug)]
pub struct CodeActionsTheme {
    /// Background color
    pub background: Color,
    /// Border color
    pub border: Color,
    /// Border width in pixels
    pub border_width: f32,
    /// Selected item background
    pub selected_background: Color,
    /// Text color
    pub text_color: Color,
    /// Minimum popup width
    pub min_width: f32,
    /// Maximum popup width
    pub max_width: f32,
    /// Padding inside the popup
    pub padding: f32,
    /// Z-index for layering
    pub z_index: f32,
}

impl Default for CodeActionsTheme {
    fn default() -> Self {
        Self {
            background: Color::srgba(0.15, 0.15, 0.15, 0.95),
            border: Color::srgba(0.3, 0.3, 0.3, 1.0),
            border_width: 1.0,
            selected_background: Color::srgba(0.2, 0.4, 0.8, 0.8),
            text_color: Color::WHITE,
            min_width: 200.0,
            max_width: 400.0,
            padding: 5.0,
            z_index: 100.0,
        }
    }
}

/// Theme for inlay hints
#[derive(Clone, Debug)]
pub struct InlayHintsTheme {
    /// Color for type hints
    pub type_color: Color,
    /// Color for parameter hints
    pub parameter_color: Color,
    /// Default color for other hints
    pub default_color: Color,
    /// Font size multiplier (relative to editor font)
    pub font_size_multiplier: f32,
    /// Z-index for layering
    pub z_index: f32,
}

impl Default for InlayHintsTheme {
    fn default() -> Self {
        Self {
            type_color: Color::srgba(0.5, 0.7, 0.9, 0.7),
            parameter_color: Color::srgba(0.7, 0.6, 0.9, 0.7),
            default_color: Color::srgba(0.6, 0.6, 0.6, 0.7),
            font_size_multiplier: 0.85,
            z_index: 50.0,
        }
    }
}

/// Theme for document highlights
#[derive(Clone, Debug)]
pub struct DocumentHighlightsTheme {
    /// Color for read references
    pub read_color: Color,
    /// Color for write references
    pub write_color: Color,
}

impl Default for DocumentHighlightsTheme {
    fn default() -> Self {
        Self {
            read_color: Color::srgba(0.5, 0.6, 0.8, 0.25),
            write_color: Color::srgba(0.8, 0.5, 0.3, 0.3),
        }
    }
}

/// Theme for rename input
#[derive(Clone, Debug)]
pub struct RenameTheme {
    /// Background color
    pub background: Color,
    /// Border color (focused)
    pub border: Color,
    /// Border width in pixels
    pub border_width: f32,
    /// Text color
    pub text_color: Color,
    /// Cursor color
    pub cursor_color: Color,
    /// Minimum input width
    pub min_width: f32,
    /// Maximum input width
    pub max_width: f32,
    /// Padding inside the input
    pub padding_x: f32,
    /// Vertical padding
    pub padding_y: f32,
    /// Z-index for layering
    pub z_index: f32,
}

impl Default for RenameTheme {
    fn default() -> Self {
        Self {
            background: Color::srgba(0.15, 0.15, 0.18, 1.0),
            border: Color::srgba(0.0, 0.48, 0.8, 1.0), // VSCode blue
            border_width: 1.0,
            text_color: Color::WHITE,
            cursor_color: Color::WHITE,
            min_width: 100.0,
            max_width: 300.0,
            padding_x: 4.0,
            padding_y: 2.0,
            z_index: 150.0,
        }
    }
}

/// Common theme settings
#[derive(Clone, Debug)]
pub struct CommonTheme {
    /// Default border width
    pub border_width: f32,
    /// Corner radius for popups (note: sprites don't support this directly)
    pub corner_radius: f32,
    /// Shadow offset
    pub shadow_offset: Vec2,
    /// Shadow color
    pub shadow_color: Color,
    /// Animation duration in seconds
    pub animation_duration: f32,
}

impl Default for CommonTheme {
    fn default() -> Self {
        Self {
            border_width: 1.0,
            corner_radius: 3.0,
            shadow_offset: Vec2::new(2.0, -2.0),
            shadow_color: Color::srgba(0.0, 0.0, 0.0, 0.3),
            animation_duration: 0.1,
        }
    }
}
