//! Modular settings system for the code editor
//!
//! Each major component has its own settings resource that can be configured independently.
//! Use `EditorSettingsBuilder` for convenient initialization.

mod core;
mod cursor;
mod performance;
mod scrolling;
mod search;
mod syntax;
mod ui;
mod wrapping;

#[cfg(feature = "scrollbar")]
mod scrollbar;

#[cfg(feature = "lsp")]
mod lsp;

pub use core::*;
pub use cursor::*;
pub use performance::*;
pub use scrolling::*;
pub use search::*;
pub use syntax::*;
pub use ui::*;
pub use wrapping::*;

#[cfg(feature = "scrollbar")]
pub use scrollbar::*;

#[cfg(feature = "lsp")]
pub use lsp::*;

use bevy::prelude::*;

/// Builder for configuring all editor settings at once
///
/// # Example
/// ```no_run
/// use bevy_code_editor::settings::EditorSettingsBuilder;
///
/// let settings = EditorSettingsBuilder::default()
///     .font_size(16.0)
///     .theme_dark()
///     .build();
/// ```
pub struct EditorSettingsBuilder {
    font: FontSettings,
    theme: ThemeSettings,
    ui: UiSettings,
    indentation: IndentationSettings,
    brackets: BracketSettings,
    cursor: CursorSettings,
    cursor_line: CursorLineSettings,
    scrolling: ScrollingSettings,
    search: SearchSettings,
    syntax: SyntaxSettings,
    performance: PerformanceSettings,
    wrapping: WrappingSettings,

    #[cfg(feature = "scrollbar")]
    scrollbar: ScrollbarSettings,

    #[cfg(feature = "lsp")]
    lsp: LspSettings,
}

impl Default for EditorSettingsBuilder {
    fn default() -> Self {
        Self {
            font: FontSettings::default(),
            theme: ThemeSettings::vscode_dark(),
            ui: UiSettings::default(),
            indentation: IndentationSettings::default(),
            brackets: BracketSettings::default(),
            cursor: CursorSettings::default(),
            cursor_line: CursorLineSettings::default(),
            scrolling: ScrollingSettings::default(),
            search: SearchSettings::default(),
            syntax: SyntaxSettings::default(),
            performance: PerformanceSettings::default(),
            wrapping: WrappingSettings::default(),

            #[cfg(feature = "scrollbar")]
            scrollbar: ScrollbarSettings::default(),

            #[cfg(feature = "lsp")]
            lsp: LspSettings::default(),
        }
    }
}

impl EditorSettingsBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    // Font configuration
    pub fn font_size(mut self, size: f32) -> Self {
        self.font.size = size;
        self.font.line_height = size * 1.5;
        self.font.char_width = size * 0.6;
        self
    }

    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.font.family = family.into();
        self
    }

    // Theme presets
    pub fn theme_dark(mut self) -> Self {
        self.theme = ThemeSettings::vscode_dark();
        self
    }

    pub fn theme_light(mut self) -> Self {
        self.theme = ThemeSettings::vscode_light();
        self
    }

    pub fn theme(mut self, theme: ThemeSettings) -> Self {
        self.theme = theme;
        self
    }

    // Custom settings
    pub fn font(mut self, font: FontSettings) -> Self {
        self.font = font;
        self
    }

    pub fn ui(mut self, ui: UiSettings) -> Self {
        self.ui = ui;
        self
    }

    #[cfg(feature = "scrollbar")]
    pub fn scrollbar(mut self, scrollbar: ScrollbarSettings) -> Self {
        self.scrollbar = scrollbar;
        self
    }

    pub fn cursor(mut self, cursor: CursorSettings) -> Self {
        self.cursor = cursor;
        self
    }

    pub fn scrolling(mut self, scrolling: ScrollingSettings) -> Self {
        self.scrolling = scrolling;
        self
    }

    pub fn search(mut self, search: SearchSettings) -> Self {
        self.search = search;
        self
    }

    pub fn indentation(mut self, indentation: IndentationSettings) -> Self {
        self.indentation = indentation;
        self
    }

    pub fn brackets(mut self, brackets: BracketSettings) -> Self {
        self.brackets = brackets;
        self
    }

    pub fn cursor_line(mut self, cursor_line: CursorLineSettings) -> Self {
        self.cursor_line = cursor_line;
        self
    }

    pub fn syntax(mut self, syntax: SyntaxSettings) -> Self {
        self.syntax = syntax;
        self
    }

    pub fn performance(mut self, performance: PerformanceSettings) -> Self {
        self.performance = performance;
        self
    }

    pub fn wrapping(mut self, wrapping: WrappingSettings) -> Self {
        self.wrapping = wrapping;
        self
    }

    #[cfg(feature = "lsp")]
    pub fn lsp(mut self, lsp: LspSettings) -> Self {
        self.lsp = lsp;
        self
    }

    /// Build and return tuple of all settings resources
    /// Insert these into your Bevy app
    pub fn build(self) -> SettingsBundle {
        SettingsBundle {
            font: self.font,
            theme: self.theme,
            ui: self.ui,
            indentation: self.indentation,
            brackets: self.brackets,
            cursor: self.cursor,
            cursor_line: self.cursor_line,
            scrolling: self.scrolling,
            search: self.search,
            syntax: self.syntax,
            performance: self.performance,
            wrapping: self.wrapping,

            #[cfg(feature = "scrollbar")]
            scrollbar: self.scrollbar,

            #[cfg(feature = "lsp")]
            lsp: self.lsp,
        }
    }
}

/// Bundle of all settings resources
/// Use `insert_into(app)` to add all settings to your Bevy app
#[derive(Clone)]
pub struct SettingsBundle {
    pub font: FontSettings,
    pub theme: ThemeSettings,
    pub ui: UiSettings,
    pub indentation: IndentationSettings,
    pub brackets: BracketSettings,
    pub cursor: CursorSettings,
    pub cursor_line: CursorLineSettings,
    pub scrolling: ScrollingSettings,
    pub search: SearchSettings,
    pub syntax: SyntaxSettings,
    pub performance: PerformanceSettings,
    pub wrapping: WrappingSettings,

    #[cfg(feature = "scrollbar")]
    pub scrollbar: ScrollbarSettings,

    #[cfg(feature = "lsp")]
    pub lsp: LspSettings,
}

impl SettingsBundle {
    /// Insert all settings as resources into the app
    pub fn insert_into(self, app: &mut App) {
        app.insert_resource(self.font);
        app.insert_resource(self.theme);
        app.insert_resource(self.ui);
        app.insert_resource(self.indentation);
        app.insert_resource(self.brackets);
        app.insert_resource(self.cursor);
        app.insert_resource(self.cursor_line);
        app.insert_resource(self.scrolling);
        app.insert_resource(self.search);
        app.insert_resource(self.syntax);
        app.insert_resource(self.performance);
        app.insert_resource(self.wrapping);

        #[cfg(feature = "scrollbar")]
        app.insert_resource(self.scrollbar);

        #[cfg(feature = "lsp")]
        app.insert_resource(self.lsp);
    }
}
