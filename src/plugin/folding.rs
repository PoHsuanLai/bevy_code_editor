//! Code folding

use bevy::prelude::*;
use crate::settings::EditorSettings;
use crate::types::*;
use super::to_bevy_coords_left_aligned;

#[cfg(feature = "tree-sitter")]
use tree_sitter::{Parser, QueryCursor, Node};
#[cfg(feature = "tree-sitter")]
use tree_sitter::Query as TsQuery;

pub(crate) fn detect_foldable_regions(
    state: Res<CodeEditorState>,
    mut fold_state: ResMut<FoldState>,
    syntax: Res<super::SyntaxResource>,
) {
    // Only update when content changes
    if fold_state.content_version == state.content_version as usize {
        return;
    }

    fold_state.content_version = state.content_version as usize;

    // Get the tree-sitter tree from syntax resource
    #[cfg(feature = "tree-sitter")]
    let tree = match syntax.tree() {
        Some(t) => t,
        None => return,
    };

    #[cfg(not(feature = "tree-sitter"))]
    return;

    let mut regions: Vec<FoldRegion> = Vec::new();
    let root = tree.root_node();
    // OPTIMIZATION: Use rope chunks instead of full to_string() conversion
    let chunk_text: String = state.rope.chunks().collect();
    let text_bytes = chunk_text.as_bytes();

    // Walk the tree and find foldable nodes
    collect_foldable_regions(&root, text_bytes, &state.rope, &mut regions, false);

    // Preserve fold state for existing regions
    let old_regions = std::mem::take(&mut fold_state.regions);
    for mut region in regions {
        // Check if this region was previously folded
        if let Some(old) = old_regions.iter().find(|r| r.start_line == region.start_line && r.end_line == region.end_line) {
            region.is_folded = old.is_folded;
        }
        fold_state.regions.push(region);
    }

    fold_state.enabled = true;
}

#[cfg(feature = "tree-sitter")]
pub(crate) fn collect_foldable_regions(
    node: &tree_sitter::Node,
    text: &[u8],
    rope: &ropey::Rope,
    regions: &mut Vec<FoldRegion>,
    parent_is_foldable_construct: bool,
) {
    let kind = node.kind();

    // Check if this is a function-like or class-like construct that contains a body
    let is_foldable_construct = matches!(kind,
        // Function-like constructs
        "function_item" | "function_definition" | "function_declaration" |
        "method_definition" | "method_declaration" | "function_expression" |
        "arrow_function" | "lambda" | "closure_expression" |
        // Class-like constructs
        "class_definition" | "class_declaration" | "struct_item" |
        "enum_item" | "interface_declaration" | "trait_item" | "impl_item"
    );

    // Skip block/body nodes that are direct children of foldable constructs
    // to avoid creating duplicate fold regions at the same line
    let skip_this_node = parent_is_foldable_construct && matches!(kind,
        "block" | "compound_statement" | "statement_block" | "body" |
        "field_declaration_list" | "declaration_list" | "enum_variant_list"
    );

    if !skip_this_node {
        // Check if this node is foldable
        if let Some(region) = node_to_fold_region(node, text, rope) {
            // Only add regions that span multiple lines
            if region.end_line > region.start_line {
                regions.push(region);
            }
        }
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_foldable_regions(&child, text, rope, regions, is_foldable_construct);
    }
}

#[cfg(feature = "tree-sitter")]
pub(crate) fn node_to_fold_region(
    node: &tree_sitter::Node,
    _text: &[u8],
    rope: &ropey::Rope,
) -> Option<FoldRegion> {
    let kind = node.kind();

    // Map tree-sitter node kinds to FoldKind
    // These mappings work for most languages (Rust, JavaScript, TypeScript, Python, etc.)
    let fold_kind = match kind {
        // Function-like constructs
        "function_item" | "function_definition" | "function_declaration" |
        "method_definition" | "method_declaration" | "function_expression" |
        "arrow_function" | "lambda" | "closure_expression" => Some(FoldKind::Function),

        // Class-like constructs
        "class_definition" | "class_declaration" | "struct_item" |
        "enum_item" | "interface_declaration" | "trait_item" |
        "impl_item" => Some(FoldKind::Class),

        // Block constructs
        "block" | "compound_statement" | "statement_block" |
        "if_expression" | "if_statement" | "match_expression" |
        "switch_statement" | "for_statement" | "for_expression" |
        "while_statement" | "while_expression" | "loop_expression" |
        "try_statement" | "catch_clause" | "finally_clause" => Some(FoldKind::Block),

        // Import/use statements (when grouped)
        "use_declaration" | "import_statement" | "import_declaration" => Some(FoldKind::Imports),

        // Comments
        "comment" | "block_comment" | "line_comment" | "doc_comment" => Some(FoldKind::Comment),

        // String literals (multi-line)
        "string_literal" | "raw_string_literal" | "template_string" => Some(FoldKind::Literal),

        // Region markers (e.g., #region in C#)
        "region" | "preproc_region" => Some(FoldKind::Region),

        // Array/object literals (when multi-line)
        "array" | "array_expression" | "object" | "object_expression" |
        "struct_expression" | "tuple_expression" => Some(FoldKind::Other),

        _ => None,
    };

    fold_kind.and_then(|kind| {
        let start_line = node.start_position().row;
        let end_line = node.end_position().row;

        // Bounds check: tree might have stale line numbers after text deletion
        let line_count = rope.len_lines();
        if start_line >= line_count || end_line >= line_count {
            return None;
        }

        // Calculate indent level from the start of the line
        let _line_start = rope.line_to_char(start_line);
        let line = rope.line(start_line);
        let mut indent_level = 0;
        for c in line.chars() {
            match c {
                ' ' => indent_level += 1,
                '\t' => indent_level += 4,
                _ => break,
            }
        }
        indent_level /= 4; // Convert to indent levels

        Some(FoldRegion {
            start_line,
            end_line,
            is_folded: false,
            kind,
            indent_level,
        })
    })
}

/// Fallback for when tree-sitter is not enabled
#[cfg(not(feature = "tree-sitter"))]
pub(crate) fn detect_foldable_regions(
    state: Res<CodeEditorState>,
    mut fold_state: ResMut<FoldState>,
) {
    // Only update when content changes
    if fold_state.content_version == state.content_version as usize {
        return;
    }

    fold_state.content_version = state.content_version as usize;

    // Simple brace-matching based folding as fallback
    let mut regions: Vec<FoldRegion> = Vec::new();
    let mut brace_stack: Vec<(usize, usize)> = Vec::new(); // (line, indent_level)

    for line_idx in 0..state.rope.len_lines() {
        let line = state.rope.line(line_idx);
        let line_str: String = line.chars().collect();

        // Calculate indent level
        let mut indent_level = 0;
        for c in line_str.chars() {
            match c {
                ' ' => indent_level += 1,
                '\t' => indent_level += 4,
                _ => break,
            }
        }
        indent_level /= 4;

        // Look for opening braces at end of line
        let trimmed = line_str.trim_end();
        if trimmed.ends_with('{') || trimmed.ends_with('[') || trimmed.ends_with('(') {
            brace_stack.push((line_idx, indent_level));
        }

        // Look for closing braces at start of line (after whitespace)
        let trimmed_start = line_str.trim_start();
        if trimmed_start.starts_with('}') || trimmed_start.starts_with(']') || trimmed_start.starts_with(')') {
            if let Some((start_line, start_indent)) = brace_stack.pop() {
                if line_idx > start_line {
                    regions.push(FoldRegion {
                        start_line,
                        end_line: line_idx,
                        is_folded: false,
                        kind: FoldKind::Block,
                        indent_level: start_indent,
                    });
                }
            }
        }
    }

    // Preserve fold state for existing regions
    let old_regions = std::mem::take(&mut fold_state.regions);
    for mut region in regions {
        if let Some(old) = old_regions.iter().find(|r| r.start_line == region.start_line && r.end_line == region.end_line) {
            region.is_folded = old.is_folded;
        }
        fold_state.regions.push(region);
    }

    fold_state.enabled = true;
}

/// Update fold gutter indicators (arrows/chevrons)
pub(crate) fn update_fold_indicators(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    fold_state: Res<FoldState>,
    mut indicator_query: Query<(Entity, &FoldIndicator, &mut Transform, &mut Text2d, &mut Visibility)>,
) {
    // Hide all if folding is disabled
    if !fold_state.enabled || !settings.ui.show_line_numbers {
        for (_, _, _, _, mut visibility) in indicator_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    let line_height = settings.font.line_height;
    let font_size = settings.font.size;
    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    // Calculate visible line range
    let visible_start_line = ((-state.scroll_offset) / line_height).floor() as usize;
    let visible_lines = ((viewport_height / line_height).ceil() as usize) + 2;
    let visible_end_line = (visible_start_line + visible_lines).min(state.rope.len_lines());

    // Collect fold regions that start within visible range
    let visible_regions: Vec<_> = fold_state.regions.iter()
        .filter(|r| r.start_line >= visible_start_line && r.start_line < visible_end_line)
        .collect();

    // Collect existing indicators
    let mut existing_indicators: std::collections::HashMap<usize, Entity> = std::collections::HashMap::new();
    for (entity, indicator, _, _, _) in indicator_query.iter() {
        existing_indicators.insert(indicator.line_index, entity);
    }

    let mut used_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Calculate hidden lines for proper display positioning
    // We need to count how many lines are hidden before each fold region
    let count_hidden_lines_before = |line: usize| -> usize {
        fold_state.regions.iter()
            .filter(|r| r.is_folded && r.start_line < line)
            .map(|r| r.end_line.saturating_sub(r.start_line))
            .sum()
    };

    for region in visible_regions {
        let line_idx = region.start_line;

        // Skip if this region's start line is hidden by another fold
        if fold_state.is_line_hidden(line_idx) {
            continue;
        }

        used_indices.insert(line_idx);

        // Calculate display line by subtracting hidden lines above
        let hidden_above = count_hidden_lines_before(line_idx);
        let display_line = line_idx.saturating_sub(hidden_above);

        // Position in fold gutter (between line numbers and separator)
        // In VSCode style, this is a narrow gutter just before the separator
        let x_offset = settings.ui.layout.separator_x - 12.0; // Just before the separator
        let y_offset = settings.ui.layout.margin_top + state.scroll_offset + (display_line as f32 * line_height);

        let translation = to_bevy_coords_left_aligned(
            x_offset,
            y_offset,
            viewport_width,
            viewport_height,
            viewport.offset_x,
            0.0,
        );

        // Choose indicator character based on fold state
        let indicator_char = if region.is_folded { "▶" } else { "▼" };

        if let Some(entity) = existing_indicators.get(&line_idx) {
            // Update existing indicator
            if let Ok((_, _, mut transform, mut text, mut visibility)) = indicator_query.get_mut(*entity) {
                transform.translation = translation;
                text.0 = indicator_char.to_string();
                *visibility = Visibility::Visible;
            }
        } else {
            // Spawn new indicator
            let text_font = TextFont {
                font: settings.font.handle.clone().unwrap_or_default(),
                font_size: font_size * 0.7,
                ..default()
            };

            commands.spawn((
                Text2d::new(indicator_char.to_string()),
                text_font,
                TextColor(settings.theme.line_numbers.with_alpha(0.8)),
                Transform::from_translation(translation),
                
                FoldIndicator { line_index: line_idx },
                Name::new(format!("FoldIndicator_{}", line_idx)),
                Visibility::Visible,
            ));
        }
    }

    // Hide unused indicators
    for (_entity, indicator, _, _, mut visibility) in indicator_query.iter_mut() {
        if !used_indices.contains(&indicator.line_index) {
            *visibility = Visibility::Hidden;
        }
    }
}
