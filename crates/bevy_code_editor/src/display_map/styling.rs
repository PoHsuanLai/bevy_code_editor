//! Editor implementations of the engine's [`LineVisibility`] /
//! [`LineStyling`] traits.
//!
//! - [`FoldFilter`] holds a snapshot of buffer-line indices hidden by the
//!   editor's [`FoldState`]. The `sync_fold_filter` system in
//!   `display_map::plugin` refreshes it on `Changed<FoldState>`.
//! - [`SyntaxStyling`] wraps a clone of `SyntaxResource`'s
//!   `Arc<RwLock<SyntaxInner>>` plus the foreground color + theme. It
//!   produces styled runs for the engine's `produce_layouts` system.
//!
//! Both store mutable state behind `RwLock` so the engine can call
//! `is_visible` / `style` through a `&self` receiver while editor systems
//! mutate the state through the same `Arc` from a separate system. Read
//! and write contention is zero by construction: editor sync systems run
//! `.before(LayoutProduceSet)`, and the engine's reads happen inside
//! `LayoutProduceSet`.

use bevy::prelude::*;
use bevy_text_engine::{LineStyling, LineVisibility, RunWithText};
#[cfg(feature = "tree-sitter")]
use bevy_text_engine::StyleRun;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use crate::plugin::syntax_highlighting::SyntaxInner;
use crate::settings::SyntaxTheme;
#[cfg(feature = "tree-sitter")]
use crate::types::LineSegment;

/// `LineVisibility` impl backed by a `HashSet` snapshot of hidden lines.
/// Refreshed by the editor's `sync_fold_filter` system.
pub(crate) struct FoldFilter {
    inner: RwLock<FoldFilterInner>,
}

#[derive(Default)]
struct FoldFilterInner {
    hidden_lines: HashSet<usize>,
}

impl FoldFilter {
    pub(crate) fn new() -> Self {
        Self {
            inner: RwLock::new(FoldFilterInner::default()),
        }
    }

    /// Replace the hidden-line snapshot. Called from `sync_fold_filter`
    /// before `LayoutProduceSet`.
    pub(crate) fn set_hidden_lines(&self, lines: HashSet<usize>) {
        self.inner.write().unwrap().hidden_lines = lines;
    }
}

impl LineVisibility for FoldFilter {
    fn is_visible(&self, buffer_line: usize) -> bool {
        !self.inner.read().unwrap().hidden_lines.contains(&buffer_line)
    }
}

/// `LineStyling` impl backed by the shared `SyntaxResource` Arc.
///
/// Holds the styling-relevant pieces of editor state behind an `RwLock`
/// (theme, foreground color) so the editor's `sync_syntax_styling` system
/// can keep them up to date as theme settings change. The shared
/// `SyntaxInner` is read-locked per `style()` call and dropped before
/// returning — no lock is held across the engine's outer iteration.
pub(crate) struct SyntaxStyling {
    #[allow(dead_code)] // unused without `tree-sitter`; kept so the field always exists
    syntax: Arc<RwLock<SyntaxInner>>,
    theme_state: RwLock<ThemeState>,
    style_epoch: std::sync::atomic::AtomicU64,
}

struct ThemeState {
    theme: SyntaxTheme,
    default_color: Color,
}

impl SyntaxStyling {
    pub(crate) fn new(syntax: Arc<RwLock<SyntaxInner>>) -> Self {
        Self {
            syntax,
            theme_state: RwLock::new(ThemeState {
                theme: SyntaxTheme::default(),
                default_color: Color::WHITE,
            }),
            style_epoch: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Update the theme + default foreground color. Called from
    /// `sync_syntax_styling` before `LayoutProduceSet`.
    pub(crate) fn set_theme(&self, theme: SyntaxTheme, default_color: Color) {
        let mut t = self.theme_state.write().unwrap();
        t.theme = theme;
        t.default_color = default_color;
    }

    /// Bump the local epoch counter folded into `version()`. Call this
    /// from sync systems that change the styling's effective output but
    /// don't bump `tree_version` (e.g. theme swap).
    pub(crate) fn bump_style_epoch(&self) {
        self.style_epoch
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

impl LineStyling for SyntaxStyling {
    #[cfg(not(feature = "tree-sitter"))]
    fn style(&self, _buffer_line: usize, _text: &str) -> Vec<RunWithText> {
        Vec::new()
    }

    #[cfg(feature = "tree-sitter")]
    fn style(&self, buffer_line: usize, text: &str) -> Vec<RunWithText> {
        let theme_state = self.theme_state.read().unwrap();
        let theme = theme_state.theme.clone();
        let default_color = theme_state.default_color;
        drop(theme_state);

        // Take a write lock — `provider.highlight_range` mutates the
        // provider's internal `query_cursor` state. The lock is released
        // before returning; the engine's outer loop calls us once per
        // line, never re-entering. Resolve the line's document byte offset
        // from the provider's cached_rope (kept in sync by the parse
        // pipeline) so tree queries land at the right place.
        let mut guard = self.syntax.write().unwrap();
        let Some(provider) = &mut guard.provider else {
            return Vec::new();
        };
        use bevy_tree_sitter::SyntaxProvider;
        let start_byte = provider
            .cached_rope
            .as_ref()
            .filter(|r| buffer_line < r.len_lines())
            .map(|r| r.line_to_byte(buffer_line))
            .unwrap_or(0);
        let line_no_nl = text.strip_suffix('\n').unwrap_or(text);
        let mut hl =
            provider.highlight_range(line_no_nl, buffer_line, buffer_line + 1, start_byte);
        let per_line = hl.pop().unwrap_or_default();
        drop(guard);

        let segs = single_line_segments(line_no_nl, start_byte, &per_line, &theme, default_color);
        segs_to_runs(&segs)
    }

    fn version(&self) -> u64 {
        let epoch = self
            .style_epoch
            .load(std::sync::atomic::Ordering::Relaxed);
        #[cfg(feature = "tree-sitter")]
        {
            // Pack tree_version (high 32 bits) | style_epoch (low 32). The
            // engine compares for equality, not ordering — overlap on
            // overflow is harmless.
            let tv = self.syntax.read().unwrap().tree_version;
            (tv << 32) | (epoch & 0xFFFF_FFFF)
        }
        #[cfg(not(feature = "tree-sitter"))]
        {
            epoch
        }
    }
}

/// Build `LineSegment`s for a single line, given the per-line
/// `HighlightRange`s from the provider. The provider returns
/// document-absolute byte ranges; subtract `start_byte` to index into
/// `line`.
#[cfg(feature = "tree-sitter")]
fn single_line_segments(
    line: &str,
    start_byte: usize,
    ranges: &[bevy_tree_sitter::HighlightRange],
    theme: &SyntaxTheme,
    default_color: Color,
) -> Vec<LineSegment> {
    let line_len = line.len();
    let line_end_doc = start_byte + line_len;
    let mut segments: Vec<LineSegment> = Vec::with_capacity(ranges.len() + 1);
    let mut cursor = 0usize;

    for range in ranges {
        // Convert document-absolute → line-local, clamping to the line
        // bounds (interpolated trees can have nodes that span across the
        // queried window).
        let r_start = range
            .byte_range
            .start
            .max(start_byte)
            .min(line_end_doc)
            .saturating_sub(start_byte);
        let r_end = range
            .byte_range
            .end
            .max(start_byte)
            .min(line_end_doc)
            .saturating_sub(start_byte);

        if r_start > cursor {
            let slice = &line[cursor..r_start];
            if !slice.is_empty() {
                segments.push(LineSegment {
                    text: slice.to_string(),
                    color: default_color,
                    background: None,
                    corner_radius: 0.0,
                    font_scale: 0.0,
                    skew: 0.0,
                });
            }
            cursor = r_start;
        }

        if r_end > cursor {
            let slice = &line[cursor..r_end];
            if !slice.is_empty() {
                let color = crate::syntax::map_highlight_color(
                    Some(&range.capture_name),
                    theme,
                    default_color,
                );
                segments.push(LineSegment {
                    text: slice.to_string(),
                    color,
                    background: None,
                    corner_radius: 0.0,
                    font_scale: 0.0,
                    skew: 0.0,
                });
            }
            cursor = r_end;
        }
    }

    if cursor < line_len {
        let slice = &line[cursor..];
        if !slice.is_empty() {
            segments.push(LineSegment {
                text: slice.to_string(),
                color: default_color,
                background: None,
                corner_radius: 0.0,
                font_scale: 0.0,
                skew: 0.0,
            });
        }
    }

    // Whitespace-only lines render plain (engine renderer handles the
    // empty-runs fallback).
    if segments.iter().all(|s| s.text.trim().is_empty()) {
        Vec::new()
    } else {
        segments
    }
}

/// Convert `LineSegment`s into the engine's `RunWithText` payloads. The
/// engine overwrites `byte_range` after concatenation; we leave it `0..0`.
#[cfg(feature = "tree-sitter")]
fn segs_to_runs(segs: &[LineSegment]) -> Vec<RunWithText> {
    segs.iter()
        .filter(|s| !s.text.is_empty())
        .map(|s| RunWithText {
            text: s.text.clone(),
            run: StyleRun {
                byte_range: 0..0,
                fg: s.color,
                bg: s.background,
                font_scale: s.font_scale,
                skew: s.skew,
                corner_radius: s.corner_radius,
                font_weight: None,
                font_family: None,
                decoration: None,
                link: None,
            },
        })
        .collect()
}

