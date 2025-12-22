//! Syntax highlighting trait and utilities

use bevy::prelude::*;
use crate::types::LineSegment;

/// Trait for syntax highlighting providers
///
/// This allows different syntax highlighting backends (tree-sitter, regex, TextMate, etc.)
/// to be plugged in without coupling to the core editor state.
pub trait SyntaxProvider: Send + Sync {
    /// Highlight a range of lines and return colored segments
    ///
    /// # Arguments
    /// * `text` - The text content to highlight (for the specified line range)
    /// * `start_line` - Starting line index
    /// * `end_line` - Ending line index (exclusive)
    /// * `start_byte` - Starting byte offset in the full document (for tree-sitter queries)
    /// * `theme` - Syntax color theme
    /// * `default_color` - Fallback color for unhighlighted text
    ///
    /// # Returns
    /// Vec of LineSegments for each line in the range
    fn highlight_range(
        &mut self,
        text: &str,
        start_line: usize,
        end_line: usize,
        start_byte: usize,
        theme: &crate::settings::SyntaxTheme,
        default_color: Color,
    ) -> Vec<Vec<LineSegment>>;

    /// Notify the provider that text was edited (for incremental highlighting)
    ///
    /// # Arguments
    /// * `start_byte` - Starting byte offset of the edit
    /// * `old_end_byte` - Ending byte offset before the edit
    /// * `new_end_byte` - Ending byte offset after the edit
    fn notify_edit(&mut self, start_byte: usize, old_end_byte: usize, new_end_byte: usize);

    /// Check if highlighting is available
    fn is_available(&self) -> bool;
}

/// Map tree-sitter highlight type to theme color
pub fn map_highlight_color(
    highlight_type: Option<&str>,
    syntax_theme: &crate::settings::SyntaxTheme,
    default_color: Color,
) -> Color {
    let hl_type = match highlight_type {
        Some(t) => t,
        None => return default_color,
    };

    let base_category = hl_type.split('.').next().unwrap_or(hl_type);

    match base_category {
        "keyword" | "conditional" | "repeat" | "exception" => syntax_theme.keyword,
        "function" | "method" => syntax_theme.function,
        "type" | "class" | "interface" | "struct" | "enum" => syntax_theme.type_name,
        "variable" | "parameter" | "field" => syntax_theme.variable,
        "constant" | "boolean" | "number" | "float" => syntax_theme.constant,
        "string" | "character" => syntax_theme.string,
        "comment" | "note" | "warning" | "danger" => syntax_theme.comment,
        "operator" => syntax_theme.operator,
        "punctuation" | "delimiter" | "bracket" | "special" => syntax_theme.punctuation,
        "property" | "attribute" | "tag" | "decorator" => syntax_theme.property,
        "constructor" => syntax_theme.constructor,
        "label" => syntax_theme.label,
        "escape" => syntax_theme.escape,
        "embedded" | "include" | "preproc" => syntax_theme.embedded,
        "namespace" | "module" => syntax_theme.type_name,
        _ => default_color,
    }
}
