//! Display map types for soft line wrapping

use bevy::prelude::*;

/// A segment of text with a specific color on a specific line.
#[derive(Clone, Debug)]
pub struct LineSegment {
    pub text: String,
    pub color: Color,
    /// Optional background color rendered as a solid rectangle behind the text.
    pub background: Option<Color>,
}

/// A visual row in the display; multiple wrapped rows can come from one buffer line.
#[derive(Clone, Debug)]
pub struct WrappedRow {
    pub buffer_line: usize,
    /// Character offset within the buffer line where this row starts
    pub start_offset: usize,
    /// Character offset within the buffer line where this row ends (exclusive)
    pub end_offset: usize,
    /// True if this is a continuation of the previous line (wrapped)
    pub is_continuation: bool,
    pub segments: Vec<LineSegment>,
}

/// Maps between buffer lines and display rows, handling soft line wrapping.
#[derive(Clone, Debug, Default)]
pub struct DisplayMap {
    pub rows: Vec<WrappedRow>,
    /// In characters; 0 means no wrapping
    pub wrap_width: usize,
    pub version: u64,
}

impl DisplayMap {
    pub fn new(wrap_width: usize) -> Self {
        Self {
            rows: Vec::new(),
            wrap_width,
            version: 0,
        }
    }

    pub fn clear(&mut self) {
        self.rows.clear();
        self.version += 1;
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn buffer_to_display(&self, buffer_line: usize, buffer_col: usize) -> (usize, usize) {
        let mut display_row = 0;

        for row in &self.rows {
            if row.buffer_line == buffer_line {
                if buffer_col >= row.start_offset && buffer_col < row.end_offset {
                    return (display_row, buffer_col - row.start_offset);
                } else if buffer_col < row.start_offset {
                    return (display_row.saturating_sub(1), buffer_col);
                }
            } else if row.buffer_line > buffer_line {
                break;
            }
            display_row += 1;
        }

        (display_row.saturating_sub(1), buffer_col)
    }

    pub fn display_to_buffer(&self, display_row: usize, display_col: usize) -> (usize, usize) {
        if let Some(row) = self.rows.get(display_row) {
            let buffer_col = row.start_offset + display_col;
            (
                row.buffer_line,
                buffer_col.min(row.end_offset.saturating_sub(1)),
            )
        } else if let Some(last_row) = self.rows.last() {
            (last_row.buffer_line, last_row.end_offset)
        } else {
            (0, 0)
        }
    }

    pub fn buffer_line_to_first_row(&self, buffer_line: usize) -> usize {
        for (i, row) in self.rows.iter().enumerate() {
            if row.buffer_line == buffer_line && !row.is_continuation {
                return i;
            }
        }
        self.rows.len().saturating_sub(1)
    }

    /// Get the buffer line for a display row
    pub fn row_to_buffer_line(&self, display_row: usize) -> usize {
        self.rows
            .get(display_row)
            .map(|r| r.buffer_line)
            .unwrap_or(0)
    }

    /// Check if a display row is a continuation (wrapped) row
    pub fn is_continuation(&self, display_row: usize) -> bool {
        self.rows
            .get(display_row)
            .map(|r| r.is_continuation)
            .unwrap_or(false)
    }

    /// Build the display map from buffer lines
    pub fn rebuild(&mut self, lines: &[Vec<LineSegment>], wrap_width: usize, _char_width: f32) {
        self.rows.clear();
        self.wrap_width = wrap_width;

        if wrap_width == 0 {
            // No wrapping - each buffer line is one display row
            for (line_idx, segments) in lines.iter().enumerate() {
                let total_chars: usize = segments.iter().map(|s| s.text.chars().count()).sum();
                self.rows.push(WrappedRow {
                    buffer_line: line_idx,
                    start_offset: 0,
                    end_offset: total_chars,
                    is_continuation: false,
                    segments: segments.clone(),
                });
            }
        } else {
            // Wrap lines at wrap_width characters
            for (line_idx, segments) in lines.iter().enumerate() {
                self.wrap_line(line_idx, segments, wrap_width);
            }
        }

        self.version += 1;
    }

    /// Wrap a single line into multiple rows
    fn wrap_line(&mut self, buffer_line: usize, segments: &[LineSegment], wrap_width: usize) {
        // Collect all text and track segment boundaries
        let mut all_text = String::new();
        let mut segment_boundaries: Vec<(usize, Color)> = Vec::new();
        let mut current_pos = 0;

        for seg in segments {
            segment_boundaries.push((current_pos, seg.color));
            all_text.push_str(&seg.text);
            current_pos += seg.text.chars().count();
        }

        let total_chars = all_text.chars().count();

        if total_chars == 0 {
            // Empty line
            self.rows.push(WrappedRow {
                buffer_line,
                start_offset: 0,
                end_offset: 0,
                is_continuation: false,
                segments: vec![],
            });
            return;
        }

        let chars: Vec<char> = all_text.chars().collect();
        let mut start = 0;
        let mut is_first_row = true;

        while start < total_chars {
            // Find where to break
            let mut end = (start + wrap_width).min(total_chars);

            // Try to break at word boundary (space) if not at end
            if end < total_chars && wrap_width > 0 {
                // Look backwards for a space to break at
                let search_start = start;
                let mut break_pos = end;
                for i in (search_start..end).rev() {
                    if chars[i] == ' ' || chars[i] == '\t' {
                        break_pos = i + 1; // Break after the space
                        break;
                    }
                }
                // Only use word break if it's not too far back (at least half the wrap width)
                if break_pos > start + wrap_width / 2 {
                    end = break_pos;
                }
            }

            // Build segments for this row
            let row_segments = self.build_row_segments(&chars, start, end, &segment_boundaries);

            self.rows.push(WrappedRow {
                buffer_line,
                start_offset: start,
                end_offset: end,
                is_continuation: !is_first_row,
                segments: row_segments,
            });

            start = end;
            is_first_row = false;
        }
    }

    /// Build segments for a row, respecting color boundaries
    fn build_row_segments(
        &self,
        chars: &[char],
        start: usize,
        end: usize,
        segment_boundaries: &[(usize, Color)],
    ) -> Vec<LineSegment> {
        let mut result = Vec::new();
        let mut current_pos = start;

        while current_pos < end {
            // Find which segment we're in
            let mut seg_color = Color::WHITE;
            let mut seg_end = end;

            for (i, (boundary_start, color)) in segment_boundaries.iter().enumerate() {
                if *boundary_start <= current_pos {
                    seg_color = *color;
                    // Find where this segment ends
                    if let Some((next_boundary, _)) = segment_boundaries.get(i + 1) {
                        seg_end = (*next_boundary).min(end);
                    } else {
                        seg_end = end;
                    }
                }
                if *boundary_start > current_pos {
                    seg_end = (*boundary_start).min(end);
                    break;
                }
            }

            // Extract the text for this segment
            let text: String = chars[current_pos..seg_end].iter().collect();
            if !text.is_empty() {
                result.push(LineSegment {
                    text,
                    color: seg_color,
                    background: None,
                });
            }

            current_pos = seg_end;
        }

        result
    }
}

/// Token with its highlight type and text content
#[derive(Clone, Debug)]
pub struct HighlightedToken {
    pub text: String,
    pub highlight_type: Option<String>,
}
