//! Regression tests for scroll-state flicker bugs.
//!
//! These tests spawn a `CodeEditor` entity, drive it through a few frames,
//! and assert invariants on `VerticalScroll.target` / `.current`. When a
//! test fails, the diagnostic output names the tick at which an invariant
//! was violated — enough to identify which system wrote a bad value.

#![cfg(test)]
#![allow(clippy::field_reassign_with_default)]

use bevy::math::{Affine2, Vec2, Vec3};
use bevy::picking::backend::HitData;
use bevy::picking::events::{Pointer, Scroll};
use bevy::picking::pointer::{Location, PointerId};
use bevy::input::mouse::MouseScrollUnit;
use bevy::prelude::*;
use bevy::ui::ui_transform::UiGlobalTransform;
use bevy::ui::ComputedNode;
use bevy_instanced_text::view::measurement::LayoutTuning;
use bevy_instanced_text::{
    ContentMetrics, DisplayLayout, HiddenLines, HorizontalScroll, LineStyles, MonoCellWidth,
    TextBounds, TextBuffer, TextOverlays, TextUnderlays, VerticalScroll,
};
use bevy_text_editor::{RopeBuffer, TextViewDragState};

use crate::plugin::{ApplyStateSet, InputSet};
use crate::settings::{
    BracketConfig, CursorLine, EditorTheme, EditorUi, GutterConfig, Indentation, Performance,
    SyntaxColors, Wrapping,
};
use crate::types::{
    BracketMatchRects, BracketMatchState, CaretRects, CodeEditor, CursorLineRects, CursorState,
    FoldState, IndentGuideRects, SelectionRects, SelectionState,
};

fn make_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(bevy::asset::AssetPlugin::default());
    app.add_plugins(bevy::input::InputPlugin);
    app.init_resource::<Assets<bevy::text::Font>>();
    app.add_message::<crate::types::events::TextEdited>();
    // Window events are read by `bevy_picking::input::mouse_pick_events` (a
    // system in `bevy_picking`). Headless tests don't get `WindowPlugin`, so
    // register the message manually.
    app.add_message::<bevy::window::WindowEvent>();
    app.configure_sets(
        Update,
        (
            InputSet,
            bevy_text_editor::EditEmitSet.after(InputSet),
            ApplyStateSet.after(bevy_text_editor::EditEmitSet),
        )
            .chain(),
    );
    app
}

#[allow(clippy::too_many_arguments)]
fn spawn_editor(app: &mut App, text: &str) -> Entity {
    let mut computed = ComputedNode::default();
    computed.size = Vec2::new(800.0, 600.0);
    computed.inverse_scale_factor = 1.0;

    let font_bundle = (
        TextFont::from_font_size(14.0),
        bevy::text::LineHeight::Px(21.0),
        MonoCellWidth { px: 8.0 },
        bevy_instanced_text::MonoFontFaces::default(),
        bevy::text::TextLayout::default(),
    );
    let scroll_bundle = (
        VerticalScroll::default(),
        HorizontalScroll::default(),
    );
    let layout_bundle = (
        TextBuffer::new(RopeBuffer::new(text)),
        ContentMetrics::default(),
        computed,
        DisplayLayout::default(),
        TextUnderlays::default(),
        TextOverlays::default(),
        TextBounds::default(),
        LayoutTuning::default(),
        HiddenLines::default(),
        LineStyles::default(),
        UiGlobalTransform::from(Affine2::from_translation(Vec2::new(400.0, 300.0))),
    );
    let settings_bundle = (
        EditorTheme::default(),
        SyntaxColors::default(),
        EditorUi::default(),
        Indentation::default(),
        Wrapping::default(),
        Performance::default(),
        GutterConfig::default(),
        CursorLine::default(),
    );
    let editor_state_bundle = (
        SelectionState::default(),
        CursorState::default(),
        FoldState::default(),
        BracketMatchState::default(),
        BracketConfig::default(),
        TextViewDragState::default(),
    );
    let overlay_bundle = (
        SelectionRects::default(),
        IndentGuideRects::default(),
        CursorLineRects::default(),
        CaretRects::default(),
        BracketMatchRects::default(),
    );
    app.world_mut()
        .spawn((CodeEditor, Name::new("TestEditor")))
        .insert(font_bundle)
        .insert(scroll_bundle)
        .insert(layout_bundle)
        .insert(settings_bundle)
        .insert(editor_state_bundle)
        .insert(overlay_bundle)
        .id()
}

/// Move the cursor below the viewport once, then tick the schedule 10
/// times with no input. The expected behavior: tick 1 sets
/// `VerticalScroll.target` to the cursor's row, and from tick 2 onward the
/// target stays put (because `last_cursor_pos` was synced).
///
/// If this test sees `.target` move past tick 1, the auto-scroll loop is
/// mis-detecting cursor movement and re-firing every frame.
#[test]
fn auto_scroll_settles_after_single_cursor_move() {
    let mut app = make_test_app();
    app.add_systems(
        Update,
        crate::plugin::ui_elements::auto_scroll_to_cursor
            .run_if(crate::plugin::ui_elements::should_auto_scroll)
            .in_set(ApplyStateSet),
    );

    // 200-line buffer; place cursor on line 100 so it's well below the
    // visible 800x600 viewport (28 lines at 21px line-height).
    let mut text = String::new();
    for i in 0..200 {
        text.push_str(&format!("line {i}\n"));
    }
    let entity = spawn_editor(&mut app, &text);

    // Place the cursor on line 100 (char offset ~700).
    let target_char = text
        .char_indices()
        .filter(|(_, c)| *c == '\n')
        .nth(100)
        .map(|(i, _)| i)
        .unwrap();
    app.world_mut()
        .get_mut::<CursorState>(entity)
        .unwrap()
        .cursor_pos = target_char;

    let mut history: Vec<f32> = Vec::new();
    for _ in 0..10 {
        app.update();
        let st = app.world().get::<VerticalScroll>(entity).unwrap();
        history.push(st.target);
    }

    let first = history[0];
    let later_changes: Vec<(usize, f32, f32)> = history
        .windows(2)
        .enumerate()
        .filter(|(_, w)| (w[0] - w[1]).abs() > 0.01)
        .map(|(i, w)| (i + 1, w[0], w[1]))
        .collect();

    assert!(first > 0.0, "Tick 1 should have moved VerticalScroll.target off zero (got {first})");
    assert!(
        later_changes.is_empty(),
        "VerticalScroll.target kept changing after tick 1 — auto_scroll_to_cursor doesn't settle.\n\
         Changes: {later_changes:?}\nFull history: {history:?}",
    );
}

/// The minimum reproduction: `auto_scroll_to_cursor` registered as a system
/// in `ApplyStateSet`, run for 10 ticks with no input. `ScrollTarget` must
/// stay at `0.0`.
#[test]
fn auto_scroll_does_not_move_scroll_target_on_idle_frames() {
    let mut app = make_test_app();
    app.add_systems(
        Update,
        crate::plugin::ui_elements::auto_scroll_to_cursor
            .run_if(crate::plugin::ui_elements::should_auto_scroll)
            .in_set(ApplyStateSet),
    );

    let entity = spawn_editor(&mut app, "fn main() {\n    println!(\"hi\");\n}\n");

    let mut history: Vec<f32> = vec![0.0];
    for _ in 0..10 {
        app.update();
        let st = app.world().get::<VerticalScroll>(entity).unwrap();
        history.push(st.target);
    }

    let moved: Vec<(usize, f32)> = history
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, &v)| v != 0.0)
        .map(|(i, &v)| (i, v))
        .collect();

    assert!(
        moved.is_empty(),
        "VerticalScroll.target moved on idle frames: {moved:?}\nFull history: {history:?}",
    );
}

/// Same as above, but with the *full* plugin set (`CodeEditorPlugins`
/// PluginGroup minus rendering bits we can't run headlessly). This is the
/// production surface — if only this one fails (and the minimal one passes),
/// the culprit is some other system the group adds.
#[test]
fn full_code_editor_plugin_does_not_move_scroll_target_on_idle_frames() {
    let mut app = make_test_app();
    // Pull in everything the editor plugin needs to *function*; skip the
    // GPU-render plugins (`GlyphAtlasPlugin`, `InstancedTextRenderPlugin`)
    // and `InstancedTextPlugin` since they require a render device.
    app.add_plugins(bevy::input_focus::InputDispatchPlugin);
    app.add_plugins(bevy_text_editor::InstancedTextEditPlugin::without_typing_observer());
    app.add_plugins(leafwing_input_manager::plugin::InputManagerPlugin::<
        crate::input::EditorAction,
    >::default());
    app.add_plugins(crate::plugin::CodeEditorPlugin);
    app.add_plugins(crate::plugin::CursorPlugin);
    app.add_plugins(crate::plugin::SyntaxPlugin);
    #[cfg(feature = "tree-sitter")]
    app.add_plugins(crate::plugin::FoldingPlugin);
    app.add_plugins(crate::plugin::BracketPlugin);
    app.add_plugins(crate::display_map::DisplayMapPlugin);

    let entity = spawn_editor(&mut app, "fn main() {\n    println!(\"hi\");\n}\n");

    let mut history: Vec<f32> = vec![0.0];
    for _ in 0..10 {
        app.update();
        let st = app.world().get::<VerticalScroll>(entity).unwrap();
        history.push(st.target);
    }

    let moved: Vec<(usize, f32)> = history
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, &v)| v != 0.0)
        .map(|(i, &v)| (i, v))
        .collect();

    assert!(
        moved.is_empty(),
        "VerticalScroll.target moved on idle frames under full plugin set: {moved:?}\nFull history: {history:?}",
    );
}

/// Fire a `Pointer<Scroll>` at the editor and verify `on_pointer_scroll`
/// accumulates the delta into `VerticalScroll.target`. This is the input
/// boundary; the monotonic-easing-toward-target part lives in
/// [`animator_drives_current_to_target_within_duration`].
#[test]
fn pointer_scroll_event_accumulates_into_target() {
    let mut app = make_test_app();
    app.add_plugins(bevy_instanced_text::view::plugin::InstancedTextPlugin);
    app.add_plugins(bevy::input_focus::InputDispatchPlugin);
    app.add_plugins(bevy_text_editor::InstancedTextEditPlugin::without_typing_observer());
    app.add_plugins(leafwing_input_manager::plugin::InputManagerPlugin::<
        crate::input::EditorAction,
    >::default());
    app.add_plugins(crate::plugin::CodeEditorPlugin);

    // 200-line buffer so there's room to scroll.
    let mut text = String::new();
    for i in 0..200 {
        text.push_str(&format!("line {i}\n"));
    }
    let entity = spawn_editor(&mut app, &text);
    app.update();

    let dummy_window = app.world_mut().spawn_empty().id();
    let dummy_camera = app.world_mut().spawn_empty().id();
    let normalized_window = bevy::window::WindowRef::Entity(dummy_window)
        .normalize(None)
        .unwrap();
    // Three small swipes down: each `dy = -10` (Pixel) -> target += 10.
    for _ in 0..3 {
        let scroll_event = Pointer::<Scroll>::new(
            PointerId::Mouse,
            Location {
                target: bevy::camera::NormalizedRenderTarget::Window(normalized_window),
                position: Vec2::new(400.0, 300.0),
            },
            Scroll {
                unit: MouseScrollUnit::Pixel,
                x: 0.0,
                y: -10.0,
                hit: HitData::new(dummy_camera, 0.0, Some(Vec3::ZERO), None),
            },
            entity,
        );
        app.world_mut().trigger(scroll_event);
    }

    let target = app.world().get::<VerticalScroll>(entity).unwrap().target;
    assert!(
        (target - 30.0).abs() < 0.01,
        "Three Pixel scrolls of dy=-10 should accumulate target to 30.0, got {target}"
    );
}

/// Set `VerticalScroll.target` once and tick the animator forward. `current`
/// must (a) move monotonically toward `target`, (b) actually reach it within
/// roughly `duration` seconds of frames.
///
/// This is the test we wished we had — it would have caught the missing
/// `vertical_anim` write-back in the previous animator on the very first run,
/// because `current` froze partway and never reached `target`.
#[test]
fn animator_drives_current_to_target_within_duration() {
    let mut app = make_test_app();
    // `InstancedTextPlugin` registers `animate_vertical_scroll` /
    // `animate_horizontal_scroll` — without it the axis just sits at 0.
    app.add_plugins(bevy_instanced_text::view::plugin::InstancedTextPlugin);
    app.add_plugins(bevy::input_focus::InputDispatchPlugin);
    app.add_plugins(bevy_text_editor::InstancedTextEditPlugin::without_typing_observer());
    app.add_plugins(leafwing_input_manager::plugin::InputManagerPlugin::<
        crate::input::EditorAction,
    >::default());
    app.add_plugins(crate::plugin::CodeEditorPlugin);
    app.add_plugins(crate::plugin::EditorUiPlugin);

    let entity = spawn_editor(&mut app, "fn main() {}\n");

    // Run one tick so apply_instant_scroll syncs the duration from
    // ScrollConfig (default 0.125s) into the axis.
    app.update();

    let target_y = 200.0;
    {
        let mut axis = app.world_mut().get_mut::<VerticalScroll>(entity).unwrap();
        axis.target = target_y;
    }

    let mut history: Vec<f32> = Vec::new();
    // 30 frames at ~16ms ≈ 0.48s — well past the 0.125s default duration.
    for _ in 0..30 {
        app.update();
        history.push(app.world().get::<VerticalScroll>(entity).unwrap().current);
    }

    // (a) Monotonic non-decreasing.
    let regressions: Vec<(usize, f32, f32)> = history
        .windows(2)
        .enumerate()
        .filter_map(|(i, w)| (w[1] + 0.01 < w[0]).then_some((i + 1, w[0], w[1])))
        .collect();
    let dump = || {
        history
            .iter()
            .enumerate()
            .map(|(i, c)| format!("  tick {i:>2}: current={c:>7.2}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert!(
        regressions.is_empty(),
        "current regressed during animation (target={target_y}):\n{}\nRegressions: {regressions:?}",
        dump(),
    );

    // (b) Reaches `target` by the end. Allow 0.5px slop for f32 jitter.
    let final_v = *history.last().unwrap();
    assert!(
        (final_v - target_y).abs() < 0.5,
        "current never reached target after 30 frames: final={final_v:.2}, target={target_y:.2}\n{}",
        dump(),
    );
}
