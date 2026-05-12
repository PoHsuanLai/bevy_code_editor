//! Tests for the editor's syntax → layout → render pipeline.
//!
//! The tests bisect the pipeline into three layers and walk them in order:
//! 1. `LineStyles.by_line[row]` — what tree-sitter said.
//! 2. `DisplayLayout.lines[i].runs[j].fg` — what `produce_layouts` baked.
//! 3. `GlyphBatchComponent.instances[k].color` — what `update_text_views`
//!    sent to the GPU.
//!
//! When a test fails, the failure message identifies *which* layer dropped
//! or corrupted the color, not just "colors are wrong."

#![cfg(all(test, feature = "tree-sitter"))]
// `ComputedNode` has no constructor that accepts size/scale — tests must
// build it via `default()` then assign fields.
#![allow(clippy::field_reassign_with_default)]

use super::plugin::DisplayMapPlugin;
use crate::plugin::syntax_highlighting::SyntaxPlugin;
use crate::settings::{
    BracketConfig, EditorTheme, EditorUi, Indentation, Performance, SyntaxColors, Wrapping,
};
use crate::types::events::TextEdited;
use crate::types::{BracketMatchState, CodeEditor, CursorState, FoldState, SelectionState};
use bevy::asset::{AssetId, Assets, Handle};
use bevy::ecs::message::Messages;
use bevy::ecs::system::RunSystemOnce;
use bevy::image::Image;
use bevy::math::{Affine2, Vec2};
use bevy::prelude::*;
use bevy::text::{Font, DEFAULT_FONT_DATA};
use bevy::time::TimePlugin;
use bevy::ui::ui_transform::UiGlobalTransform;
use bevy::ui::ComputedNode;
use bevy_instanced_text::gpu::{GlyphAtlas, GlyphAtlasPlugin};
use bevy_instanced_text::view::layout_builder::produce_layouts;
use bevy_instanced_text::view::plugin::{sync_viewport_from_node, update_text_views};
use bevy_instanced_text::view::render::{GlyphBatchComponent, GlyphInstance};
use bevy_instanced_text::view::tuning::LayoutTuning;
use bevy_instanced_text::{
    DisplayLayout, LineStyles, ScrollState, TextBounds, TextBuffer, TextFont, TextView,
    TextViewBatchEntity, TextViewOverlays, TextViewport,
};
use bevy_instanced_text_edit::{BlinkPhase, EditDelta, EditPoint};
use bevy_tree_sitter::{Language, SyntaxTree, TreeSitterPlugin};
use std::time::{Duration, Instant};

// ── Shared helpers ───────────────────────────────────────────────────────

/// Build a test app with the minimum plugins needed to drive the syntax /
/// layout pipeline headlessly: `TreeSitterPlugin`, `SyntaxPlugin`, and
/// `DisplayMapPlugin`, with the same `Update` set ordering `CodeEditorPlugin`
/// configures in production.
fn make_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.build().disable::<TimePlugin>());
    app.add_plugins(TimePlugin);
    app.add_plugins(bevy::asset::AssetPlugin::default());
    app.init_resource::<Assets<Font>>();
    app.configure_sets(
        Update,
        (
            crate::plugin::InputSet,
            bevy_instanced_text_edit::EditEmitSet.after(crate::plugin::InputSet),
            crate::plugin::ApplyStateSet.after(bevy_instanced_text_edit::EditEmitSet),
        )
            .chain(),
    );
    app.add_message::<TextEdited>();
    app.add_plugins(TreeSitterPlugin);
    app.add_plugins(SyntaxPlugin);
    app.add_plugins(DisplayMapPlugin);
    app
}

/// Spawn a `CodeEditor` entity carrying every Component the syntax /
/// styling / layout plumbing reads — the same bundle the editor's host
/// app would spawn in production.
fn spawn_test_editor(app: &mut App, text: &str) -> Entity {
    let engine_bundle = (
        TextView,
        TextBuffer::new(text),
        ScrollState::default(),
        TextViewport {
            width: 800,
            height: 600,
            hit_test_position: Vec2::ZERO,
            text_area_left: 0.0,
            text_area_top: 0.0,
            gutter_width: 0.0,
        },
        TextFont {
            font: Handle::default(),
            font_size: 14.0,
            line_height: 21.0,
            char_width: 8.0,
            font_bold: None,
            font_italic: None,
            font_bold_italic: None,
            font_synthesis: Default::default(),
        },
        DisplayLayout::default(),
        TextViewOverlays::default(),
        TextBounds::default(),
        LayoutTuning::default(),
    );
    let settings_bundle = (
        EditorTheme::default(),
        SyntaxColors::default(),
        EditorUi::default(),
        Indentation::default(),
        Wrapping::default(),
        Performance::default(),
    );
    let editor_state_bundle = (
        SelectionState::default(),
        CursorState::default(),
        FoldState::default(),
        BracketMatchState::default(),
        BracketConfig::default(),
    );
    let language = Language::from_grammar(
        "rust",
        tree_sitter_rust::LANGUAGE.into(),
        tree_sitter_rust::HIGHLIGHTS_QUERY,
    );
    let entity = app
        .world_mut()
        .spawn((
            CodeEditor,
            Name::new("TestEditor"),
            engine_bundle,
            settings_bundle,
            editor_state_bundle,
            language,
        ))
        .id();
    // Run Startup once so `init_editor_syntax` attaches `EditorSyntaxState` /
    // `ParseSourceComp` / `SyntaxTree` to the entity.
    app.update();
    entity
}

/// Install `GlyphAtlas` + a usable default font into the test app. Needed
/// by every test that drives `produce_layouts` or `update_text_views`.
fn install_atlas_and_font(app: &mut App) {
    let world = app.world_mut();
    world.init_resource::<Assets<Image>>();
    world.init_resource::<Assets<Font>>();
    let atlas = {
        let mut images = world.resource_mut::<Assets<Image>>();
        GlyphAtlas::new(&mut images)
    };
    world.insert_resource(atlas);
    let font = Font::try_from_bytes(DEFAULT_FONT_DATA.to_vec()).unwrap();
    world
        .resource_mut::<Assets<Font>>()
        .insert(AssetId::default(), font)
        .unwrap();
}

/// Drive `app.update()` until `pred` returns true, or `timeout` elapses.
/// Returns the number of ticks consumed (0 if `pred` was already true).
fn run_until<F>(app: &mut App, entity: Entity, timeout: Duration, mut pred: F) -> usize
where
    F: FnMut(&World, Entity) -> bool,
{
    let start = Instant::now();
    let mut ticks = 0;
    loop {
        if pred(app.world(), entity) {
            return ticks;
        }
        if start.elapsed() > timeout {
            return ticks;
        }
        app.update();
        ticks += 1;
        // Give the AsyncComputeTaskPool a chance to make progress.
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Drive the app until tree-sitter's async parse has populated `LineStyles`
/// for the editor — the precondition for any layer-walking test.
fn await_initial_parse(app: &mut App, entity: Entity) -> usize {
    run_until(app, entity, Duration::from_secs(5), |w, e| {
        let st = w.get::<SyntaxTree>(e).unwrap();
        let ls = w.get::<LineStyles>(e);
        st.tree.is_some() && ls.map(|s| !s.by_line.is_empty()).unwrap_or(false)
    })
}

/// One-shot drive of `produce_layouts` + `update_text_views`, then an
/// `app.update()` to flush the `Commands` that spawn the batch entity.
/// Used after seeding state for tests that don't run a full PostUpdate
/// schedule.
fn drive_layout_and_render_once(app: &mut App) {
    app.world_mut().run_system_once(produce_layouts).unwrap();
    app.world_mut().run_system_once(update_text_views).unwrap();
    app.update();
}

fn to_linear_rgba(c: bevy::color::Color) -> [f32; 4] {
    let l = c.to_linear();
    [l.red, l.green, l.blue, l.alpha]
}

fn color_eq(a: [f32; 4], b: [f32; 4]) -> bool {
    const EPS: f32 = 0.001;
    (a[0] - b[0]).abs() < EPS
        && (a[1] - b[1]).abs() < EPS
        && (a[2] - b[2]).abs() < EPS
        && (a[3] - b[3]).abs() < EPS
}

/// Group `GlyphInstance`s into y-clusters (rows) and map each cluster to
/// the `DisplayLayout` `display_row` it represents.
///
/// Glyph instances on the same row share a y-coordinate (within sub-pixel
/// jitter). We sort by y descending, walk linearly, and start a new cluster
/// whenever the gap exceeds `line_height / 2`. The K-th cluster (largest y
/// first) maps to the K-th non-empty `display_row` in `DisplayLayout`,
/// because both are emitted in increasing display-row order.
fn cluster_instances_to_display_rows<'a>(
    instances: &'a [GlyphInstance],
    layout: &DisplayLayout,
    line_height: f32,
) -> Vec<(u32, Vec<&'a GlyphInstance>)> {
    let mut sorted: Vec<&_> = instances.iter().collect();
    sorted.sort_by(|a, b| b.position.y.partial_cmp(&a.position.y).unwrap());

    let gap_threshold = line_height * 0.5;
    let mut clusters: Vec<Vec<&_>> = Vec::new();
    let mut current: Vec<&_> = Vec::new();
    let mut last_y: Option<f32> = None;
    for inst in sorted {
        let new_cluster = match last_y {
            None => false,
            Some(prev) => (prev - inst.position.y).abs() > gap_threshold,
        };
        if new_cluster && !current.is_empty() {
            clusters.push(std::mem::take(&mut current));
        }
        last_y = Some(inst.position.y);
        current.push(inst);
    }
    if !current.is_empty() {
        clusters.push(current);
    }

    let mut non_empty_rows: Vec<u32> = layout
        .lines
        .iter()
        .filter(|l| !l.runs.is_empty() && !l.is_wrap_continuation)
        .map(|l| l.display_row)
        .collect();
    non_empty_rows.sort();

    clusters
        .into_iter()
        .zip(non_empty_rows)
        .map(|(c, r)| (r, c))
        .collect()
}

/// Diagnostic dump for failing layer assertions. Prints the full
/// `DisplayLayout` and the top-N glyph instances sorted by y.
fn dump_layout_and_batch(
    layout: &DisplayLayout,
    batch: &GlyphBatchComponent,
    kw_linear: [f32; 4],
) {
    eprintln!(
        "\n=== DIAGNOSTIC DUMP ===\n\
         line_height = {}\n\
         keyword color (linear): {:?}\n\
         \n\
         DisplayLayout has {} lines:",
        layout.line_height,
        kw_linear,
        layout.lines.len(),
    );
    for l in layout.lines.iter() {
        eprintln!(
            "  display_row={} buffer_row={} wrap_cont={} y_top={} text={:?}\n    runs: {:?}",
            l.display_row,
            l.buffer_row,
            l.is_wrap_continuation,
            l.y_top,
            l.text,
            l.runs
                .iter()
                .map(|r| (r.byte_range.clone(), r.fg))
                .collect::<Vec<_>>(),
        );
    }
    let mut sorted: Vec<_> = batch.instances.iter().collect();
    sorted.sort_by(|a, b| b.position.y.partial_cmp(&a.position.y).unwrap());
    eprintln!(
        "\nGlyphBatch has {} instances (sorted by y desc, top 40):",
        batch.instances.len(),
    );
    for inst in sorted.iter().take(40) {
        let is_kw = color_eq(inst.color, kw_linear);
        eprintln!(
            "  y={:8.2}  x={:8.2}  color={:?}  {}",
            inst.position.y,
            inst.position.x,
            inst.color,
            if is_kw { "<-- KEYWORD COLOR" } else { "" }
        );
    }
    eprintln!("=== END DIAGNOSTIC DUMP ===\n");
}

/// Walk all three pipeline layers and assert the keyword color flows
/// correctly. Caller specifies which `buffer_row` should contain the `fn`
/// run (Layer 1's text == `"fn"`). Used by the initial + post-edit tests.
fn assert_pipeline_consistent_for_keyword(
    line_styles: &LineStyles,
    display_layout: &DisplayLayout,
    batch: &GlyphBatchComponent,
    expected_fn_buffer_row: u32,
    label: &str,
) {
    let default_fg = EditorTheme::default().foreground;

    // ── Layer 1: LineStyles has an `fn` run on the expected buffer row,
    //   with a non-default fg color (the keyword color).
    let row_styled = line_styles
        .by_line
        .get(&expected_fn_buffer_row)
        .cloned()
        .unwrap_or_default();
    let kw_styled = row_styled
        .iter()
        .find(|r| r.text == "fn")
        .unwrap_or_else(|| {
            panic!(
                "{}: LAYER 1 (LineStyles): buffer_row={} has no run with text='fn'. \
                 tree-sitter didn't classify the keyword. Runs: {:?}",
                label,
                expected_fn_buffer_row,
                row_styled
                    .iter()
                    .map(|r| (r.text.clone(), r.run.fg))
                    .collect::<Vec<_>>()
            );
        });
    let kw_color = kw_styled.run.fg;
    assert_ne!(
        kw_color, default_fg,
        "{}: LAYER 1: `fn` on buffer_row={} is the default foreground color — \
         tree-sitter didn't apply keyword styling",
        label, expected_fn_buffer_row,
    );

    // ── Layer 2: DisplayLayout has a run covering bytes 0..2 on the same
    //   buffer row with the same color.
    let line = display_layout
        .lines
        .iter()
        .find(|l| l.buffer_row == expected_fn_buffer_row && !l.is_wrap_continuation)
        .unwrap_or_else(|| {
            panic!(
                "{}: LAYER 2: no DisplayLayout line with buffer_row={}. Got: {:?}",
                label,
                expected_fn_buffer_row,
                display_layout
                    .lines
                    .iter()
                    .map(|l| (l.buffer_row, l.display_row, l.text.clone()))
                    .collect::<Vec<_>>()
            );
        });
    let kw_run = line
        .runs
        .iter()
        .find(|r| r.byte_range.start == 0 && r.byte_range.end == 2)
        .unwrap_or_else(|| {
            panic!(
                "{}: LAYER 2: line for buffer_row={} has no run covering bytes 0..2. \
                 Runs: {:?}",
                label,
                expected_fn_buffer_row,
                line.runs
                    .iter()
                    .map(|r| (r.byte_range.clone(), r.fg))
                    .collect::<Vec<_>>()
            );
        });
    assert_eq!(
        kw_run.fg, kw_color,
        "{}: LAYER 1→2 DIVERGENCE: LineStyles said `fn` is {:?}, but \
         DisplayLayout has it as {:?}",
        label, kw_color, kw_run.fg,
    );

    // ── Layer 3: GlyphBatch has the keyword color on the display_row that
    //   DisplayLayout placed `fn` on, AND no stale keyword color on any
    //   display_row DisplayLayout says is empty.
    let kw_linear = to_linear_rgba(kw_color);
    let cluster_rows =
        cluster_instances_to_display_rows(&batch.instances, display_layout, display_layout.line_height);
    let kw_rows: std::collections::HashSet<u32> = cluster_rows
        .iter()
        .filter(|(_, is)| is.iter().any(|i| color_eq(i.color, kw_linear)))
        .map(|(r, _)| *r)
        .collect();
    let expected_fn_display_row = line.display_row;
    let empty_rows: std::collections::HashSet<u32> = display_layout
        .lines
        .iter()
        .filter(|l| l.runs.is_empty() && !l.is_wrap_continuation)
        .map(|l| l.display_row)
        .collect();
    let stale_rows: Vec<u32> = kw_rows
        .iter()
        .filter(|r| empty_rows.contains(r))
        .copied()
        .collect();

    if !kw_rows.contains(&expected_fn_display_row) || !stale_rows.is_empty() {
        dump_layout_and_batch(display_layout, batch, kw_linear);
        eprintln!(
            "Cluster→display_row mapping: {:?}\nkw_rows: {:?}\n\
             expected_fn_display_row: {}\nempty_rows: {:?}\nstale_rows: {:?}",
            cluster_rows.iter().map(|(r, i)| (*r, i.len())).collect::<Vec<_>>(),
            kw_rows,
            expected_fn_display_row,
            empty_rows,
            stale_rows,
        );
    }
    assert!(
        kw_rows.contains(&expected_fn_display_row),
        "{}: LAYER 3: GlyphBatch has no keyword-colored instance on \
         display_row={} (where DisplayLayout placed `fn`). Keyword color \
         found on rows: {:?}.",
        label, expected_fn_display_row, kw_rows,
    );
    assert!(
        stale_rows.is_empty(),
        "{}: LAYER 3 STALE: keyword color {:?} appears on display row(s) \
         {:?} which DisplayLayout says are EMPTY. Stale-glyph bug.",
        label, kw_color, stale_rows,
    );
}

// ── Tests ────────────────────────────────────────────────────────────────

/// Smoke check: after Startup, the editor entity should have both
/// `EditorSyntaxState` (with a usable provider) and `SyntaxTree`.
#[test]
fn editor_initializes_with_syntax_provider() {
    use crate::plugin::syntax_highlighting::EditorSyntaxState;

    let mut app = make_test_app();
    let entity = spawn_test_editor(&mut app, "fn main() {}\n");

    let world = app.world();
    let state = world
        .get::<EditorSyntaxState>(entity)
        .expect("EditorSyntaxState attached after Startup");
    assert!(
        state.is_available(),
        "provider must be installed (Language was attached at spawn)"
    );
    assert!(
        world.get::<SyntaxTree>(entity).is_some(),
        "SyntaxTree attached after Startup",
    );
}

/// Subsystem check: `sync_viewport_from_node` converts physical-pixel
/// `ComputedNode` size + `UiGlobalTransform` into logical-pixel
/// `TextViewport` fields.
#[test]
fn sync_viewport_from_node_uses_logical_pixels() {
    let mut app = make_test_app();
    let entity = spawn_test_editor(&mut app, "fn main() {}\n");

    // 1600x1200 physical at 2x DPI → 800x600 logical, top-left at (0,0).
    let physical = Vec2::new(1600.0, 1200.0);
    let mut computed = ComputedNode::default();
    computed.size = physical;
    computed.inverse_scale_factor = 0.5;
    app.world_mut().entity_mut(entity).insert((
        computed,
        UiGlobalTransform::from(Affine2::from_translation(physical * 0.5)),
    ));

    // Clear the TextViewport to a known wrong state.
    {
        let mut vp = app.world_mut().get_mut::<TextViewport>(entity).unwrap();
        vp.width = 0;
        vp.height = 0;
        vp.hit_test_position = Vec2::new(-999.0, -999.0);
    }
    app.world_mut()
        .run_system_once(sync_viewport_from_node)
        .unwrap();

    let vp = app.world().get::<TextViewport>(entity).unwrap();
    assert_eq!(vp.width, 800, "logical width (physical / DPI)");
    assert_eq!(vp.height, 600, "logical height");
    assert!(
        (vp.hit_test_position - Vec2::ZERO).length() < 0.01,
        "top-left in logical pixels, got {:?}",
        vp.hit_test_position
    );
}

/// Walks all three layers (LineStyles → DisplayLayout → GlyphBatch) for a
/// static buffer. First divergence is reported by layer in the failure
/// message, so this localizes any pipeline-color regression.
#[test]
fn pipeline_consistency_initial() {
    let source = "fn main() {\n    let x = 42;\n}\n";
    let mut app = make_test_app();
    install_atlas_and_font(&mut app);
    let entity = spawn_test_editor(&mut app, source);

    await_initial_parse(&mut app, entity);
    drive_layout_and_render_once(&mut app);

    let world = app.world();
    let line_styles = world.get::<LineStyles>(entity).unwrap().clone();
    let display_layout = world.get::<DisplayLayout>(entity).unwrap().clone();
    let batch_entity = world
        .get::<TextViewBatchEntity>(entity)
        .expect("update_text_views must spawn a batch entity")
        .0;
    let batch = world.get::<GlyphBatchComponent>(batch_entity).unwrap().clone();

    assert_pipeline_consistent_for_keyword(
        &line_styles,
        &display_layout,
        &batch,
        /*expected_fn_buffer_row=*/ 0,
        "initial",
    );
}

/// Inserts a `\n` at byte 0, shifting `fn` from buffer_row=0 to
/// buffer_row=1. Verifies the entire pipeline correctly reflects the new
/// row layout — including row 2 (`    let x = 42;`) shifting down from
/// pre-edit row 1. Regression check for the `LineStyles.by_line` index-
/// shift bug (now fixed in `record_edits_for_incremental_parsing` by
/// forcing a full rebuild on line-count-changing edits).
#[test]
fn pipeline_consistency_after_newline_insert() {
    let source = "fn main() {\n    let x = 42;\n}\n";
    let mut app = make_test_app();
    install_atlas_and_font(&mut app);
    let entity = spawn_test_editor(&mut app, source);

    await_initial_parse(&mut app, entity);
    drive_layout_and_render_once(&mut app);

    // EDIT: insert `\n` at byte 0 → `fn` moves from buffer_row=0 to row 1.
    let new_source = "\nfn main() {\n    let x = 42;\n}\n";
    {
        let mut buf = app.world_mut().get_mut::<TextBuffer>(entity).unwrap();
        buf.rope = ropey::Rope::from_str(new_source);
        buf.content_version += 1;
    }
    app.world_mut()
        .resource_mut::<Messages<TextEdited>>()
        .write(TextEdited {
            delta: EditDelta {
                start_byte: 0,
                old_end_byte: 0,
                new_end_byte: 1,
                start_position: EditPoint {
                    row: 0,
                    column_byte: 0,
                },
                old_end_position: EditPoint {
                    row: 0,
                    column_byte: 0,
                },
                new_end_position: EditPoint {
                    row: 1,
                    column_byte: 0,
                },
            },
            content_version: 2,
            pre_edit_rope: None,
        });

    // Wait for `fn` to appear on row 1 in LineStyles (async reparse).
    run_until(&mut app, entity, Duration::from_secs(5), |w, e| {
        w.get::<LineStyles>(e)
            .and_then(|s| s.by_line.get(&1u32).cloned())
            .map(|runs| runs.iter().any(|r| r.text == "fn"))
            .unwrap_or(false)
    });
    drive_layout_and_render_once(&mut app);

    let world = app.world();
    let line_styles = world.get::<LineStyles>(entity).unwrap().clone();
    let display_layout = world.get::<DisplayLayout>(entity).unwrap().clone();
    let batch_entity = world.get::<TextViewBatchEntity>(entity).unwrap().0;
    let batch = world.get::<GlyphBatchComponent>(batch_entity).unwrap().clone();

    // `fn` moved to buffer_row=1.
    assert_pipeline_consistent_for_keyword(
        &line_styles,
        &display_layout,
        &batch,
        /*expected_fn_buffer_row=*/ 1,
        "after newline insert",
    );

    // Index-shift regression: row 2 must hold the shifted-down `let` line,
    // not stale pre-edit row 2 content (`}`).
    let row2_styled = line_styles
        .by_line
        .get(&2u32)
        .cloned()
        .unwrap_or_default();
    assert!(
        row2_styled.iter().any(|r| r.text == "let"),
        "INDEX-SHIFT REGRESSION: row 2 should hold `    let x = 42;` \
         (shifted down from pre-edit row 1). Got: {:?}",
        row2_styled
            .iter()
            .map(|r| (r.text.clone(), r.run.fg))
            .collect::<Vec<_>>()
    );
}

/// Deletes the leading `\n`, shifting `fn` from buffer_row=1 to row=0.
/// This is the regression test for the `LineStyles.by_line` index-shift
/// bug — before the fix, post-edit row 1 still held pre-edit row 1's `fn`
/// runs even though buffer row 1 now contained `    let x = 42;`.
#[test]
fn pipeline_consistency_after_backspace_join() {
    let source = "\nfn main() {\n    let x = 42;\n}\n";
    let mut app = make_test_app();
    install_atlas_and_font(&mut app);
    let entity = spawn_test_editor(&mut app, source);

    // Wait for the initial parse to place `fn` on row 1.
    run_until(&mut app, entity, Duration::from_secs(5), |w, e| {
        w.get::<LineStyles>(e)
            .and_then(|s| s.by_line.get(&1u32).cloned())
            .map(|runs| runs.iter().any(|r| r.text == "fn"))
            .unwrap_or(false)
    });
    drive_layout_and_render_once(&mut app);

    // EDIT: delete the leading `\n` (backspace-join row 1 into row 0).
    let new_source = "fn main() {\n    let x = 42;\n}\n";
    {
        let mut buf = app.world_mut().get_mut::<TextBuffer>(entity).unwrap();
        buf.rope = ropey::Rope::from_str(new_source);
        buf.content_version += 1;
    }
    app.world_mut()
        .resource_mut::<Messages<TextEdited>>()
        .write(TextEdited {
            delta: EditDelta {
                start_byte: 0,
                old_end_byte: 1,
                new_end_byte: 0,
                start_position: EditPoint {
                    row: 0,
                    column_byte: 0,
                },
                old_end_position: EditPoint {
                    row: 1,
                    column_byte: 0,
                },
                new_end_position: EditPoint {
                    row: 0,
                    column_byte: 0,
                },
            },
            content_version: 2,
            pre_edit_rope: None,
        });

    run_until(&mut app, entity, Duration::from_secs(5), |w, e| {
        w.get::<LineStyles>(e)
            .and_then(|s| s.by_line.get(&0u32).cloned())
            .map(|runs| runs.iter().any(|r| r.text == "fn"))
            .unwrap_or(false)
    });
    drive_layout_and_render_once(&mut app);

    let world = app.world();
    let line_styles = world.get::<LineStyles>(entity).unwrap().clone();
    let display_layout = world.get::<DisplayLayout>(entity).unwrap().clone();
    let batch_entity = world.get::<TextViewBatchEntity>(entity).unwrap().0;
    let batch = world.get::<GlyphBatchComponent>(batch_entity).unwrap().clone();

    assert_pipeline_consistent_for_keyword(
        &line_styles,
        &display_layout,
        &batch,
        /*expected_fn_buffer_row=*/ 0,
        "after backspace join",
    );

    // The bug-regression check: row 1 must hold the new `let` line, not the
    // stale `fn main() {` runs that were under by_line[1] pre-edit.
    let row1_styled = line_styles
        .by_line
        .get(&1u32)
        .cloned()
        .unwrap_or_default();
    assert!(
        !row1_styled.iter().any(|r| r.text == "fn"),
        "INDEX-SHIFT REGRESSION: row 1 still has stale `fn` runs after \
         backspace-join: {:?}",
        row1_styled
            .iter()
            .map(|r| (r.text.clone(), r.run.fg))
            .collect::<Vec<_>>()
    );
}

/// Drives the full PostUpdate schedule as it runs in production:
/// `sync_viewport_from_node` → `produce_layouts` → overlay producers →
/// `update_text_views`. Verifies that bevscode's overlay systems
/// (selection, cursor-line highlight, cursor caret, bracket highlight)
/// don't interfere with the color path in the GPU batch.
///
/// If a future refactor reorders these systems and breaks the color path,
/// this is the test that fires.
#[test]
fn full_postupdate_schedule_with_real_overlays() {
    use bevy::input_focus::InputFocus;

    use crate::plugin::brackets::update_bracket_highlight;
    use crate::plugin::cursor::{push_cursor_overlays, update_cursor_line_highlight};
    use crate::plugin::ui_elements::update_selection_highlight;
    use crate::settings::{CursorLine, CursorSettings};

    let mut app = make_test_app();
    install_atlas_and_font(&mut app);
    app.world_mut().init_resource::<InputFocus>();
    app.add_plugins(GlyphAtlasPlugin);
    app.add_systems(
        PostUpdate,
        (
            sync_viewport_from_node,
            produce_layouts.run_if(bevy_instanced_text::gpu::atlas_ready),
            (
                update_selection_highlight,
                update_cursor_line_highlight,
                push_cursor_overlays,
                update_bracket_highlight,
            ),
            update_text_views.run_if(bevy_instanced_text::gpu::atlas_ready),
        )
            .chain(),
    );

    let entity = spawn_test_editor(&mut app, "fn main() {}\n");
    app.world_mut().entity_mut(entity).insert((
        BlinkPhase::default(),
        CursorSettings::default(),
        CursorLine::default(),
    ));

    let mut computed = ComputedNode::default();
    computed.size = Vec2::new(800.0, 600.0);
    computed.inverse_scale_factor = 1.0;
    app.world_mut().entity_mut(entity).insert((
        computed,
        UiGlobalTransform::from(Affine2::from_translation(Vec2::new(400.0, 300.0))),
    ));
    app.world_mut().resource_mut::<InputFocus>().set(entity);

    let timeout = Duration::from_secs(5);
    let start = Instant::now();
    let default_fg_linear = to_linear_rgba(EditorTheme::default().foreground);
    let mut last_instances = 0usize;
    loop {
        app.update();
        std::thread::sleep(Duration::from_millis(2));
        if start.elapsed() > timeout {
            break;
        }
        let world = app.world();
        let Some(bh) = world.get::<TextViewBatchEntity>(entity) else { continue };
        let Some(batch) = world.get::<GlyphBatchComponent>(bh.0) else { continue };
        last_instances = batch.instances.len();
        if !batch.instances.is_empty()
            && batch.instances.iter().any(|i| !color_eq(i.color, default_fg_linear))
        {
            return;
        }
    }
    panic!(
        "after {:?} the GPU batch had no colored instances (last seen len={}). \
         Overlay producers may be interfering with the color path.",
        timeout, last_instances,
    );
}

/// **Real GPU readback.** Drives Bevy's render pipeline against an
/// off-screen `Image` target via `Screenshot::image`, then asserts that
/// rendered pixels differ sufficiently from the editor background — i.e.,
/// the GPU actually drew text. End-to-end visual verification.
///
/// Requires a working wgpu adapter (Metal on macOS, Vulkan/llvmpipe on
/// Linux CI). Marked `#[ignore]` so CI without GPU doesn't fail it; run
/// manually with `--ignored`.
#[test]
#[ignore = "requires GPU; run with --ignored"]
fn gpu_readback_renders_colored_pixels() {
    use bevy::app::ScheduleRunnerPlugin;
    use bevy::camera::{Camera, Camera2d, RenderTarget};
    use bevy::render::render_resource::{TextureFormat, TextureUsages};
    use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
    use bevy::window::{ExitCondition, WindowPlugin};
    use bevy::winit::WinitPlugin;
    use bevy::DefaultPlugins;
    use std::sync::{Arc, Mutex};

    const W: u32 = 800;
    const H: u32 = 600;

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .disable::<WinitPlugin>()
            .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>(),
    );
    app.add_plugins(ScheduleRunnerPlugin::run_once());
    app.add_plugins(bevy_instanced_text::gpu::GlyphAtlasPlugin);
    app.add_plugins(bevy_instanced_text::gpu::InstancedTextRenderPlugin);
    app.add_plugins(bevy_instanced_text::view::plugin::InstancedTextPlugin);
    app.add_plugins(TreeSitterPlugin);
    app.add_plugins(SyntaxPlugin);
    app.add_plugins(DisplayMapPlugin);
    app.configure_sets(
        Update,
        (
            crate::plugin::InputSet,
            bevy_instanced_text_edit::EditEmitSet.after(crate::plugin::InputSet),
            crate::plugin::ApplyStateSet.after(bevy_instanced_text_edit::EditEmitSet),
        )
            .chain(),
    );
    app.add_message::<TextEdited>();

    // `RenderPlugin` initializes the wgpu device asynchronously; wait for
    // plugins to be ready, then `finish + cleanup` installs `RenderDevice`
    // into the main world (which `bevy_pbr::no_automatic_skin_batching`
    // requires).
    while app.plugins_state() == bevy::app::PluginsState::Adding {
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();

    {
        let world = app.world_mut();
        let font = Font::try_from_bytes(DEFAULT_FONT_DATA.to_vec()).unwrap();
        world
            .resource_mut::<Assets<Font>>()
            .insert(AssetId::default(), font)
            .unwrap();
    }
    let target_handle: Handle<Image> = {
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let mut img = Image::new_target_texture(W, H, TextureFormat::bevy_default(), None);
        img.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        images.add(img)
    };
    app.world_mut().spawn((
        Camera2d,
        Camera::default(),
        RenderTarget::Image(target_handle.clone().into()),
    ));

    let entity = spawn_test_editor(&mut app, "fn main() {}\n");
    let mut computed = ComputedNode::default();
    computed.size = Vec2::new(W as f32, H as f32);
    computed.inverse_scale_factor = 1.0;
    app.world_mut().entity_mut(entity).insert((
        computed,
        UiGlobalTransform::from(Affine2::from_translation(Vec2::new(
            W as f32 / 2.0,
            H as f32 / 2.0,
        ))),
    ));

    await_initial_parse(&mut app, entity);
    // A few extra frames so the render world catches up.
    for _ in 0..5 {
        app.update();
        std::thread::sleep(Duration::from_millis(8));
    }

    let captured: Arc<Mutex<Option<Image>>> = Arc::new(Mutex::new(None));
    let sink = captured.clone();
    app.world_mut()
        .spawn(Screenshot::image(target_handle.clone()))
        .observe(move |trigger: On<ScreenshotCaptured>| {
            *sink.lock().unwrap() = Some(trigger.image.clone());
        });

    let start = Instant::now();
    let timeout = Duration::from_secs(10);
    while captured.lock().unwrap().is_none() {
        app.update();
        std::thread::sleep(Duration::from_millis(8));
        if start.elapsed() > timeout {
            panic!("screenshot never landed after {:?}", timeout);
        }
    }
    let img = captured.lock().unwrap().take().unwrap();
    let data = img.data.as_ref().expect("Screenshot image has no data");

    let bg = EditorTheme::default().background;
    let bg_rgba_u8: [u8; 4] = {
        let l = bg.to_linear();
        [
            (l.red * 255.0).clamp(0.0, 255.0) as u8,
            (l.green * 255.0).clamp(0.0, 255.0) as u8,
            (l.blue * 255.0).clamp(0.0, 255.0) as u8,
            (l.alpha * 255.0).clamp(0.0, 255.0) as u8,
        ]
    };
    let mut nonbg = 0usize;
    let mut max_dist = 0i32;
    for px in data.chunks_exact(4) {
        let dr = px[0] as i32 - bg_rgba_u8[0] as i32;
        let dg = px[1] as i32 - bg_rgba_u8[1] as i32;
        let db = px[2] as i32 - bg_rgba_u8[2] as i32;
        let dist = dr.abs() + dg.abs() + db.abs();
        if dist > max_dist {
            max_dist = dist;
        }
        if dist > 60 {
            nonbg += 1;
        }
    }
    assert!(
        nonbg > 100,
        "screenshot has only {} pixels differing from background by >60 \
         (max delta={}) — GPU likely didn't draw the text. Size: {}×{}",
        nonbg, max_dist, img.width(), img.height(),
    );
}
