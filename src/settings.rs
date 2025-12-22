//! Editor settings and configuration

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Complete editor settings
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct EditorSettings {
    /// Font configuration
    pub font: FontSettings,

    /// Color theme
    pub theme: Theme,

    /// Show/hide UI elements
    pub ui: UISettings,

    /// Tab and indentation
    pub indentation: IndentationSettings,

    /// Line wrapping
    pub wrapping: WrappingSettings,

    /// Cursor and selection
    pub cursor: CursorSettings,

    /// Cursor line highlighting (VSCode-style borders and word highlight)
    pub cursor_line: CursorLineSettings,

    /// Scrolling behavior
    pub scrolling: ScrollSettings,

    /// Auto-completion settings
    #[cfg(feature = "lsp")]
    pub completion: CompletionSettings,

    /// Hover information settings
    #[cfg(feature = "lsp")]
    pub hover: HoverSettings,

    /// Bracket matching
    pub brackets: BracketSettings,

    /// Search and replace
    pub search: SearchSettings,

    /// Rendering optimizations
    pub performance: PerformanceSettings,

    /// Minimap settings
    pub minimap: MinimapSettings,
}

// ===== Font Settings =====
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FontSettings {
    /// Font family path or name
    pub family: String,

    /// Font size in pixels
    pub size: f32,

    /// Character width (for monospace calculations)
    pub char_width: f32,

    /// Line height in pixels
    pub line_height: f32,

    /// Font weight (100-900)
    pub weight: u16,

    /// Letter spacing adjustment
    pub letter_spacing: f32,

    /// Cached font handle (set at runtime)
    #[serde(skip)]
    pub handle: Option<Handle<Font>>,
}

// ===== Theme Settings =====
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Theme {
    /// Background color
    pub background: Color,

    /// Text color (default)
    pub foreground: Color,

    /// Cursor color
    pub cursor: Color,

    /// Selection background
    pub selection_background: Color,

    /// Selection foreground (optional)
    pub selection_foreground: Option<Color>,

    /// Line highlight (current line)
    pub line_highlight: Option<Color>,

    /// Line numbers color
    pub line_numbers: Color,

    /// Active line number color
    pub line_numbers_active: Color,

    /// Gutter background
    pub gutter_background: Color,

    /// Separator line color
    pub separator: Color,

    /// Indent guide line color
    pub indent_guide: Color,

    /// Minimap background color
    pub minimap_background: Color,

    /// Minimap slider/viewport indicator color (on hover)
    pub minimap_slider: Color,

    /// Minimap viewport highlight color (subtle, always visible)
    pub minimap_viewport_highlight: Color,

    /// Matching bracket highlight color
    pub bracket_match: Color,

    /// Find/search match highlight color
    pub find_match: Color,

    /// Current find match highlight color (the selected one)
    pub find_match_current: Color,

    /// Syntax highlighting colors
    pub syntax: SyntaxTheme,

    /// Diagnostic colors (errors, warnings)
    #[cfg(feature = "lsp")]
    pub diagnostics: DiagnosticColors,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyntaxTheme {
    pub keyword: Color,
    pub function: Color,
    pub method: Color,
    pub string: Color,
    pub number: Color,
    pub comment: Color,
    pub variable: Color,
    pub operator: Color,
    pub constant: Color,
    pub type_name: Color,
    pub parameter: Color,
    pub property: Color,
    pub punctuation: Color,
    pub label: Color,
    pub constructor: Color,
    pub escape: Color,
    pub embedded: Color,
}

#[cfg(feature = "lsp")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiagnosticColors {
    pub error: Color,
    pub warning: Color,
    pub info: Color,
    pub hint: Color,
}

// ===== UI Settings =====
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UISettings {
    /// Show line numbers
    pub show_line_numbers: bool,

    /// Show relative line numbers (vim-style)
    pub relative_line_numbers: bool,

    /// Show gutter (area for line numbers, breakpoints, etc.)
    pub show_gutter: bool,

    /// Show indent guides
    pub show_indent_guides: bool,

    /// Show whitespace characters
    pub show_whitespace: WhitespaceMode,

    /// Show control characters (tabs, newlines)
    pub show_control_characters: bool,

    /// Highlight current line
    pub highlight_active_line: bool,

    /// Show ruler (vertical line at column)
    pub rulers: Vec<usize>,

    /// Show separator line between gutter and code
    pub show_separator: bool,

    /// Layout configuration
    pub layout: LayoutSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WhitespaceMode {
    None,
    Selection,  // Only in selected text
    Trailing,   // Only trailing spaces
    All,
}

/// Layout settings for editor positioning
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutSettings {
    /// Left margin for line numbers (pixels)
    pub line_number_margin_left: f32,

    /// Top margin for content (pixels)
    pub margin_top: f32,

    /// Separator X position (pixels from left)
    pub separator_x: f32,

    /// Code margin left (pixels, should be separator_x + gap)
    pub code_margin_left: f32,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            line_number_margin_left: 20.0,
            margin_top: 30.0,
            separator_x: 50.0,        // Moved left from 60.0
            code_margin_left: 60.0,   // Moved left from 70.0 (10px gap)
        }
    }
}

// ===== Indentation Settings =====
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndentationSettings {
    /// Use spaces or tabs
    pub use_spaces: bool,

    /// Tab size (number of spaces per tab)
    pub tab_size: usize,

    /// Indent size (can differ from tab_size)
    pub indent_size: usize,

    /// Auto-indent on new line
    pub auto_indent: bool,

    /// Detect indentation from file
    pub detect_indentation: bool,
}

// ===== Wrapping Settings =====
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WrappingSettings {
    /// Enable word wrap
    pub enabled: bool,

    /// Wrap mode
    pub mode: WrapMode,

    /// Wrap column (if WrapMode::Column)
    pub column: usize,

    /// Indent wrapped continuation lines
    pub indent_wrapped_lines: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WrapMode {
    None,
    Viewport,  // Wrap at viewport edge
    Column,    // Wrap at specific column
}

// ===== Cursor Settings =====
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CursorSettings {
    /// Cursor style
    pub style: CursorStyle,

    /// Cursor blink rate (Hz, 0 = no blink)
    pub blink_rate: f32,

    /// Smooth cursor animation
    pub smooth_caret: bool,

    /// Multi-cursor support
    pub multi_cursor: bool,

    /// Width in pixels
    pub width: f32,

    /// Height multiplier (relative to line height)
    pub height_multiplier: f32,

    /// Key repeat settings
    pub key_repeat: KeyRepeatSettings,
}

/// Key repeat behavior settings
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyRepeatSettings {
    /// Initial delay before key repeat starts (seconds)
    pub initial_delay: f64,

    /// Interval between repeats (seconds)
    pub repeat_interval: f64,
}

impl Default for KeyRepeatSettings {
    fn default() -> Self {
        Self {
            initial_delay: 0.5,      // 500ms initial delay
            repeat_interval: 0.03,   // 30ms between repeats (~33 repeats/sec)
        }
    }
}

// ===== Cursor Line Settings =====
/// Settings for cursor line highlighting (VSCode-style)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CursorLineSettings {
    /// Enable cursor line highlighting
    pub enabled: bool,

    /// Show border lines above and below current line
    pub show_border: bool,

    /// Border line thickness in pixels
    pub border_thickness: f32,

    /// Border color (if None, uses line_highlight color with increased alpha)
    pub border_color: Option<Color>,

    /// Border alpha multiplier (applied to line_highlight color)
    pub border_alpha_multiplier: f32,

    /// Highlight the word under cursor
    pub highlight_word: bool,

    /// Word highlight color (if None, uses line_highlight color from theme)
    pub word_highlight_color: Option<Color>,
}

impl Default for CursorLineSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            show_border: true,
            border_thickness: 0.5,
            border_color: None,
            border_alpha_multiplier: 1.5,
            highlight_word: true,
            word_highlight_color: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CursorStyle {
    Line,      // Vertical bar (default)
    Block,     // Block cursor
    Underline, // Underscore
}

// ===== Scroll Settings =====
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScrollSettings {
    /// Smooth scrolling
    pub smooth_scrolling: bool,

    /// Scroll speed multiplier
    pub speed: f32,

    /// Scroll beyond last line
    pub scroll_beyond_last_line: bool,

    /// Cursor margin (lines to keep visible above/below cursor)
    pub cursor_margin: usize,
}

// ===== Completion Settings =====
#[cfg(feature = "lsp")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletionSettings {
    /// Enable auto-completion
    pub enabled: bool,

    /// Trigger on typing
    pub auto_trigger: bool,

    /// Trigger characters (e.g., '.', ':')
    pub trigger_characters: Vec<char>,

    /// Minimum word length before auto-triggering (e.g., 3 = trigger after typing 3 chars)
    pub min_word_length: usize,

    /// Maximum visible items in completion popup
    pub max_visible_items: usize,

    /// Show documentation in completion
    pub show_documentation: bool,

    /// Commit on enter
    pub commit_on_enter: bool,

    /// Commit on tab
    pub commit_on_tab: bool,
}

// ===== Hover Settings =====
#[cfg(feature = "lsp")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HoverSettings {
    /// Enable hover information
    pub enabled: bool,

    /// Delay before showing hover (milliseconds)
    pub delay_ms: u64,
}

// ===== Bracket Settings =====
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BracketSettings {
    /// Highlight matching brackets
    pub highlight_matching: bool,

    /// Use bounding box style (border) instead of filled highlight
    pub use_box_style: bool,

    /// Border thickness for box style (in pixels)
    pub box_border_thickness: f32,

    /// Auto-close brackets
    pub auto_close: bool,

    /// Auto-close quotes
    pub auto_close_quotes: bool,

    /// Bracket pairs to match
    pub pairs: Vec<(char, char)>,

    /// Rainbow brackets (nested brackets with different colors)
    pub rainbow: bool,
}

// ===== Search Settings =====
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchSettings {
    /// Case sensitive by default
    pub case_sensitive: bool,

    /// Regex enabled by default
    pub regex: bool,

    /// Whole word match
    pub whole_word: bool,

    /// Highlight all matches
    pub highlight_matches: bool,

    /// Incremental search (search as you type)
    pub incremental: bool,
}

// ===== Minimap Settings =====
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MinimapSettings {
    /// Enable minimap
    pub enabled: bool,

    /// Width of the minimap in pixels
    pub width: f32,

    /// Scale factor for text rendering (how much to shrink)
    pub scale: f32,

    /// Show slider/viewport indicator on hover
    pub show_slider: bool,

    /// Only show slider on hover (like VSCode)
    pub slider_on_hover_only: bool,

    /// Show a subtle viewport highlight always (even when not hovering)
    pub show_viewport_highlight: bool,

    /// Render actual characters (vs simplified blocks)
    pub render_characters: bool,

    /// Maximum number of columns to render
    pub max_column: usize,

    /// Side of the editor to show minimap (true = right, false = left)
    pub show_on_right: bool,

    /// Vertical alignment when content is shorter than viewport
    /// true = center, false = top-aligned
    pub center_when_short: bool,

    /// Scrollbar width in pixels
    pub scrollbar_width: f32,

    /// Scrollbar track color
    pub scrollbar_track_color: Color,

    /// Scrollbar thumb color
    pub scrollbar_thumb_color: Color,

    /// Minimum scrollbar thumb height
    pub scrollbar_min_thumb_height: f32,

    /// Scrollbar Z-index
    pub scrollbar_z_index: f32,

    /// Scrollbar border radius
    pub scrollbar_border_radius: f32,

    /// Line height in minimap (pixels per line)
    pub line_height: f32,

    /// Font size in minimap
    pub font_size: f32,

    /// Minimum height for viewport indicator
    pub min_indicator_height: f32,

    /// Horizontal padding/margin
    pub padding: f32,

    /// Spacing between minimap and scrollbar
    pub scrollbar_spacing: f32,

    /// Z-index for minimap background
    pub background_z_index: f32,

    /// Z-index for viewport highlight
    pub viewport_highlight_z_index: f32,

    /// Z-index for slider
    pub slider_z_index: f32,
}

impl Default for MinimapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            width: 80.0,
            scale: 0.1,
            show_slider: true,
            slider_on_hover_only: true, // Like VSCode - only show on hover
            show_viewport_highlight: true, // Subtle always-visible viewport indicator
            render_characters: false, // Use simplified blocks by default
            max_column: 120,
            show_on_right: true,
            center_when_short: true, // Center minimap content when document is short
            scrollbar_width: 10.0,
            scrollbar_track_color: Color::srgba(0.2, 0.2, 0.2, 0.3),
            scrollbar_thumb_color: Color::srgba(0.5, 0.5, 0.5, 0.6),
            scrollbar_min_thumb_height: 30.0,
            scrollbar_z_index: 6.0,
            scrollbar_border_radius: 5.0,
            line_height: 4.0,
            font_size: 3.0,
            min_indicator_height: 20.0,
            padding: 2.0,
            scrollbar_spacing: 2.0,
            background_z_index: 5.0,
            viewport_highlight_z_index: 5.3,
            slider_z_index: 5.5,
        }
    }
}

impl MinimapSettings {
    pub fn minimal() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }
}

// ===== Performance Settings =====
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PerformanceSettings {
    /// Viewport culling (only render visible lines)
    pub viewport_culling: bool,

    /// Entity pooling (reuse text entities)
    pub entity_pooling: bool,

    /// Debounce interval for updates (ms)
    pub debounce_ms: f64,

    /// Max file size to syntax highlight (bytes)
    pub max_syntax_highlight_size: usize,

    /// Lazy syntax highlighting (only visible range)
    pub lazy_syntax_highlighting: bool,

    /// Number of extra lines to render above/below the visible viewport.
    /// Higher values prevent "blackouts" during fast scrolling but use more memory.
    /// Zed uses about 2-3 lines, but we use more for safety with Bevy's rendering model.
    pub viewport_buffer_lines: u32,
}

// ===== Default Implementations =====

impl Default for EditorSettings {
    fn default() -> Self {
        Self::vscode_like()
    }
}

impl EditorSettings {
    /// VSCode-like defaults (familiar to most developers)
    pub fn vscode_like() -> Self {
        Self {
            font: FontSettings::default(),
            theme: Theme::dark(),
            ui: UISettings::default(),
            indentation: IndentationSettings::default(),
            wrapping: WrappingSettings::default(),
            cursor: CursorSettings::default(),
            cursor_line: CursorLineSettings::default(),
            scrolling: ScrollSettings::default(),
            #[cfg(feature = "lsp")]
            completion: CompletionSettings::default(),
            #[cfg(feature = "lsp")]
            hover: HoverSettings::default(),
            brackets: BracketSettings::default(),
            search: SearchSettings::default(),
            performance: PerformanceSettings::default(),
            minimap: MinimapSettings::default(),
        }
    }

    /// Minimal settings for maximum performance
    pub fn minimal() -> Self {
        Self {
            font: FontSettings::minimal(),
            theme: Theme::minimal(),
            ui: UISettings::minimal(),
            indentation: IndentationSettings::default(),
            wrapping: WrappingSettings::minimal(),
            cursor: CursorSettings::minimal(),
            cursor_line: CursorLineSettings::default(),
            scrolling: ScrollSettings::minimal(),
            #[cfg(feature = "lsp")]
            completion: CompletionSettings::minimal(),
            #[cfg(feature = "lsp")]
            hover: HoverSettings::minimal(),
            brackets: BracketSettings::minimal(),
            search: SearchSettings::default(),
            performance: PerformanceSettings::aggressive(),
            minimap: MinimapSettings::minimal(),
        }
    }
}

impl Default for FontSettings {
    fn default() -> Self {
        Self {
            family: "fonts/FiraMono-Regular.ttf".to_string(),
            size: 16.0,
            char_width: 9.6,
            line_height: 22.0,
            weight: 400,
            letter_spacing: 0.0,
            handle: None,
        }
    }
}

impl FontSettings {
    pub fn minimal() -> Self {
        Self {
            family: "fonts/FiraMono-Regular.ttf".to_string(),
            size: 18.0,
            char_width: 14.0,
            line_height: 28.0,
            weight: 400,
            letter_spacing: 0.0,
            handle: None,
        }
    }
}

impl Theme {
    /// Dark theme (Liquid Chrome inspired)
    pub fn dark() -> Self {
        Self {
            background: Color::srgb(0.0, 0.0, 0.0),
            foreground: Color::srgba(0.9, 0.9, 0.9, 0.95),
            cursor: Color::srgb(1.0, 1.0, 1.0),
            selection_background: Color::srgba(0.2, 0.4, 0.8, 0.3),
            selection_foreground: None,
            line_highlight: Some(Color::srgba(1.0, 1.0, 1.0, 0.05)),
            line_numbers: Color::srgba(0.5, 0.5, 0.5, 0.8),
            line_numbers_active: Color::srgba(0.9, 0.9, 0.9, 1.0),
            gutter_background: Color::srgba(0.0, 0.0, 0.0, 0.0),
            separator: Color::srgba(0.3, 0.3, 0.3, 0.6),
            indent_guide: Color::srgba(0.3, 0.3, 0.3, 0.3),
            minimap_background: Color::srgba(0.05, 0.05, 0.05, 0.8),
            minimap_slider: Color::srgba(0.5, 0.5, 0.5, 0.3),
            minimap_viewport_highlight: Color::srgba(0.4, 0.4, 0.4, 0.15),
            bracket_match: Color::srgba(0.5, 0.7, 1.0, 0.4),
            find_match: Color::srgba(0.9, 0.7, 0.2, 0.3),
            find_match_current: Color::srgba(0.9, 0.7, 0.2, 0.6),
            syntax: SyntaxTheme::liquid_chrome(),
            #[cfg(feature = "lsp")]
            diagnostics: DiagnosticColors::default(),
        }
    }

    /// Minimal theme (no syntax highlighting)
    pub fn minimal() -> Self {
        let base_color = Color::srgba(0.9, 0.9, 0.9, 0.95);
        Self {
            background: Color::srgb(0.0, 0.0, 0.0),
            foreground: base_color,
            cursor: Color::srgb(1.0, 1.0, 1.0),
            selection_background: Color::srgba(0.3, 0.3, 0.3, 0.5),
            selection_foreground: None,
            line_highlight: None,
            line_numbers: Color::srgba(0.5, 0.5, 0.5, 0.8),
            line_numbers_active: Color::srgba(0.7, 0.7, 0.7, 1.0),
            gutter_background: Color::srgba(0.0, 0.0, 0.0, 0.0),
            separator: Color::srgba(0.2, 0.2, 0.2, 0.6),
            indent_guide: Color::srgba(0.2, 0.2, 0.2, 0.2),
            minimap_background: Color::srgba(0.03, 0.03, 0.03, 0.8),
            minimap_slider: Color::srgba(0.4, 0.4, 0.4, 0.3),
            minimap_viewport_highlight: Color::srgba(0.3, 0.3, 0.3, 0.1),
            bracket_match: Color::srgba(0.5, 0.5, 0.5, 0.3),
            find_match: Color::srgba(0.7, 0.7, 0.3, 0.3),
            find_match_current: Color::srgba(0.7, 0.7, 0.3, 0.5),
            syntax: SyntaxTheme::minimal(),
            #[cfg(feature = "lsp")]
            diagnostics: DiagnosticColors::default(),
        }
    }
}

impl SyntaxTheme {
    /// Liquid Chrome color scheme
    pub fn liquid_chrome() -> Self {
        Self {
            keyword: Color::srgba(0.961, 0.961, 0.980, 0.95),      // --chrome-white
            function: Color::srgba(0.706, 0.784, 0.863, 0.95),     // --chrome-blue
            method: Color::srgba(0.706, 0.784, 0.863, 0.95),       // --chrome-blue
            string: Color::srgba(0.922, 0.902, 0.863, 0.95),       // --chrome-beige
            number: Color::srgba(0.588, 0.667, 0.765, 0.95),       // --chrome-steel
            comment: Color::srgba(0.392, 0.451, 0.529, 0.80),      // --chrome-dark
            variable: Color::srgba(0.863, 0.882, 0.922, 0.95),     // --chrome-light
            operator: Color::srgba(0.706, 0.784, 0.863, 0.95),     // --chrome-blue
            constant: Color::srgba(0.922, 0.902, 0.863, 0.95),     // --chrome-beige
            type_name: Color::srgba(0.588, 0.667, 0.765, 0.95),    // --chrome-steel
            parameter: Color::srgba(0.863, 0.882, 0.922, 0.95),    // --chrome-light
            property: Color::srgba(0.588, 0.667, 0.765, 0.95),     // --chrome-steel
            punctuation: Color::srgba(0.667, 0.667, 0.667, 0.85),  // light gray
            label: Color::srgba(0.922, 0.902, 0.863, 0.95),        // --chrome-beige
            constructor: Color::srgba(0.961, 0.961, 0.980, 0.95),  // --chrome-white
            escape: Color::srgba(0.706, 0.784, 0.863, 0.95),       // --chrome-blue
            embedded: Color::srgba(0.863, 0.882, 0.922, 0.95),     // --chrome-light
        }
    }

    /// Minimal theme (all text same color)
    pub fn minimal() -> Self {
        let base = Color::srgba(0.9, 0.9, 0.9, 0.95);
        Self {
            keyword: base,
            function: base,
            method: base,
            string: base,
            number: base,
            comment: Color::srgba(0.5, 0.5, 0.5, 0.8),
            variable: base,
            operator: base,
            constant: base,
            type_name: base,
            parameter: base,
            property: base,
            punctuation: base,
            label: base,
            constructor: base,
            escape: base,
            embedded: base,
        }
    }
}

#[cfg(feature = "lsp")]
impl Default for DiagnosticColors {
    fn default() -> Self {
        Self {
            error: Color::srgba(1.0, 0.3, 0.3, 0.9),
            warning: Color::srgba(1.0, 0.8, 0.2, 0.9),
            info: Color::srgba(0.3, 0.7, 1.0, 0.9),
            hint: Color::srgba(0.5, 0.5, 0.5, 0.7),
        }
    }
}

impl Default for UISettings {
    fn default() -> Self {
        Self {
            show_line_numbers: true,
            relative_line_numbers: false,
            show_gutter: true,
            show_indent_guides: true,
            show_whitespace: WhitespaceMode::None,
            show_control_characters: false,
            highlight_active_line: true,
            rulers: vec![],
            show_separator: false,  // Modern editors don't show separators
            layout: LayoutSettings::default(),
        }
    }
}

impl UISettings {
    pub fn minimal() -> Self {
        Self {
            show_line_numbers: true,
            relative_line_numbers: false,
            show_gutter: true,
            show_indent_guides: true,
            show_whitespace: WhitespaceMode::None,
            show_control_characters: false,
            highlight_active_line: false,
            rulers: vec![],
            show_separator: false,
            layout: LayoutSettings::default(),
        }
    }
}

impl Default for IndentationSettings {
    fn default() -> Self {
        Self {
            use_spaces: true,
            tab_size: 4,
            indent_size: 4,
            auto_indent: true,
            detect_indentation: true,
        }
    }
}

impl Default for WrappingSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: WrapMode::None,
            column: 80,
            indent_wrapped_lines: true,
        }
    }
}

impl WrappingSettings {
    pub fn minimal() -> Self {
        Self {
            enabled: false,
            mode: WrapMode::None,
            column: 80,
            indent_wrapped_lines: true,
        }
    }
}

impl Default for CursorSettings {
    fn default() -> Self {
        Self {
            style: CursorStyle::Line,
            blink_rate: 1.0,
            smooth_caret: false,
            multi_cursor: false,
            width: 2.5,
            height_multiplier: 0.85,
            key_repeat: KeyRepeatSettings::default(),
        }
    }
}

impl CursorSettings {
    pub fn minimal() -> Self {
        Self {
            style: CursorStyle::Line,
            blink_rate: 1.0,
            smooth_caret: false,
            multi_cursor: false,
            width: 2.0,
            height_multiplier: 0.85,
            key_repeat: KeyRepeatSettings::default(),
        }
    }
}

impl Default for ScrollSettings {
    fn default() -> Self {
        Self {
            smooth_scrolling: false,
            speed: 1.0,
            scroll_beyond_last_line: true,
            cursor_margin: 2,
        }
    }
}

impl ScrollSettings {
    pub fn minimal() -> Self {
        Self {
            smooth_scrolling: false,
            speed: 1.0,
            scroll_beyond_last_line: false,
            cursor_margin: 0,
        }
    }
}

#[cfg(feature = "lsp")]
impl Default for CompletionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_trigger: true,
            trigger_characters: vec!['.', ':'],
            min_word_length: 3,
            max_visible_items: 10,
            show_documentation: true,
            commit_on_enter: true,
            commit_on_tab: true,
        }
    }
}

#[cfg(feature = "lsp")]
impl CompletionSettings {
    pub fn minimal() -> Self {
        Self {
            enabled: false,
            auto_trigger: false,
            trigger_characters: vec![],
            min_word_length: 3,
            max_visible_items: 10,
            show_documentation: false,
            commit_on_enter: false,
            commit_on_tab: false,
        }
    }
}

#[cfg(feature = "lsp")]
impl Default for HoverSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            delay_ms: 500,
        }
    }
}

#[cfg(feature = "lsp")]
impl HoverSettings {
    pub fn minimal() -> Self {
        Self {
            enabled: false,
            delay_ms: 500,
        }
    }
}

impl Default for BracketSettings {
    fn default() -> Self {
        Self {
            highlight_matching: true,
            use_box_style: true,
            box_border_thickness: 1.0,
            auto_close: true,
            auto_close_quotes: true,
            pairs: vec![
                ('(', ')'),
                ('[', ']'),
                ('{', '}'),
                ('<', '>'),
            ],
            rainbow: false,
        }
    }
}

impl BracketSettings {
    pub fn minimal() -> Self {
        Self {
            highlight_matching: false,
            use_box_style: true,
            box_border_thickness: 1.0,
            auto_close: false,
            auto_close_quotes: false,
            pairs: vec![],
            rainbow: false,
        }
    }
}

impl Default for SearchSettings {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            regex: false,
            whole_word: false,
            highlight_matches: true,
            incremental: true,
        }
    }
}

impl Default for PerformanceSettings {
    fn default() -> Self {
        Self {
            viewport_culling: true,
            entity_pooling: true,
            debounce_ms: 16.0,  // ~60 FPS
            max_syntax_highlight_size: 1_000_000,  // 1MB
            lazy_syntax_highlighting: true,
            viewport_buffer_lines: 15,  // Render 15 extra lines above/below viewport
        }
    }
}

impl PerformanceSettings {
    pub fn aggressive() -> Self {
        Self {
            viewport_culling: true,
            entity_pooling: true,
            debounce_ms: 33.0,  // ~30 FPS
            max_syntax_highlight_size: 100_000,  // 100KB
            lazy_syntax_highlighting: true,
            viewport_buffer_lines: 10,  // Smaller buffer for aggressive mode
        }
    }
}
