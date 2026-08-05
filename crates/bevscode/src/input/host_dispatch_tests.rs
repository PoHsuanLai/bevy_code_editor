//! Proof that a host without leafwing can still drive the editor.
//!
//! The `leafwing` feature is meant to gate a *dependency*, not behaviour: with
//! it off, [`execute_editor_action`] should still turn an `EditorAction` into
//! the matching `*Requested` message, so a host that owns its input pipeline
//! loses nothing but the keybinding layer.
//!
//! That claim is only worth as much as a test that runs with the feature off —
//! so these deliberately do **not** name a leafwing type, and the module is not
//! feature-gated. They stand in for the host we are about to write in dawai.

#![cfg(test)]

use bevy::prelude::*;

use super::dispatch::{execute_editor_action, DispatchParams};
use super::keybindings::EditorAction;
use crate::types::marker::CodeEditor;

/// Drive one action per run, set by the test through this resource.
#[derive(Resource)]
struct RequestedAction(Option<EditorAction>);

/// A host-owned dispatch system: decides an action by whatever means it likes
/// (here, a resource the test writes) and calls into bevscode. This is the
/// shape a real host's `KeyCode -> EditorAction` system would take.
fn host_dispatch_system(
    mut requested: ResMut<RequestedAction>,
    mut params: DispatchParams,
    #[cfg(feature = "lsp")] pending: ResMut<
        crate::input::handlers::lsp_followup::PendingActionFollowup,
    >,
    #[cfg(feature = "lsp")] editor_q: Query<
        (
            &mut crate::types::CursorState,
            &mut crate::text_view::InstancedText<bevy_instanced_text_editor::RopeBuffer>,
            &mut crate::types::GotoLineState,
        ),
        With<CodeEditor>,
    >,
    #[cfg(not(feature = "lsp"))] editor_q: Query<
        (
            &crate::types::CursorState,
            &crate::text_view::InstancedText<bevy_instanced_text_editor::RopeBuffer>,
            &mut crate::types::GotoLineState,
        ),
        With<CodeEditor>,
    >,
    #[cfg(feature = "lsp")] lsp_q: super::dispatch::DispatchLspQuery,
) {
    let Some(action) = requested.0.take() else {
        return;
    };
    execute_editor_action(
        action,
        &mut params,
        #[cfg(feature = "lsp")]
        pending,
        editor_q,
        #[cfg(feature = "lsp")]
        lsp_q,
    );
}

fn make_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    // `InputPlugin` registers the KeyboardInput/MouseWheel/GamepadButtonChanged
    // messages that `InputFocusPlugin`'s dispatchers read. Note this is Bevy's
    // own input, not leafwing — bevscode still needs a focus notion regardless
    // of who binds the keys.
    app.add_plugins(bevy::input::InputPlugin);
    app.add_plugins(bevy::input_focus::InputFocusPlugin);
    app.insert_resource(RequestedAction(None));
    #[cfg(feature = "lsp")]
    app.init_resource::<crate::input::handlers::lsp_followup::PendingActionFollowup>();
    // `bevy_picking`'s input systems read `WindowEvent`, and a headless app has
    // no `WindowPlugin` to register it. Same workaround as `scroll_flicker_tests`.
    app.add_message::<bevy::window::WindowEvent>();
    // The two plugins that between them register every `*Requested` message
    // `execute_editor_action` writes: the editing plugin for the cursor/edit
    // families, `CodeEditorPlugin` for the IDE ones plus Save/Open.
    //
    // Deliberately *not* `EditorDispatchPlugin` — that is the keybinding
    // frontend, and the whole point is that a host works without it. Using the
    // real plugins rather than a hand-copied `add_message` list also means this
    // test breaks if the message set drifts, instead of silently under-covering.
    app.add_plugins(bevy_instanced_text_editor::InstancedTextEditPlugin::without_typing_observer());
    app.add_plugins(crate::plugin::CodeEditorPlugin);
    app.add_systems(Update, host_dispatch_system);
    app
}

/// Spawn the minimum an editor entity needs for `execute_editor_action` to
/// reach its `writers.emit(action)` tail, and focus it.
fn spawn_focused_editor(app: &mut App) -> Entity {
    let entity = app
        .world_mut()
        .spawn((
            CodeEditor,
            crate::types::CursorState::default(),
            crate::types::SelectionState::default(),
            crate::types::GotoLineState::default(),
            crate::settings::Misc::default(),
            crate::settings::AutoEdit::default(),
            crate::settings::Indentation::default(),
            crate::text_view::InstancedText::<bevy_instanced_text_editor::RopeBuffer>::new(
                bevy_instanced_text_editor::RopeBuffer::new("hello\nworld\n"),
            ),
        ))
        .id();
    app.world_mut()
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(entity, bevy::input_focus::FocusCause::Navigated);
    entity
}

/// The core claim: no leafwing anywhere, and a navigation action still produces
/// its message.
#[test]
fn host_can_dispatch_without_leafwing() {
    let mut app = make_app();
    spawn_focused_editor(&mut app);

    app.world_mut().resource_mut::<RequestedAction>().0 = Some(EditorAction::MoveCursorRight);
    app.update();

    let messages = app
        .world()
        .resource::<Messages<crate::input::action_events::MoveCursorRightRequested>>();
    let mut cursor = messages.get_cursor();
    assert_eq!(
        cursor.read(messages).count(),
        1,
        "execute_editor_action should emit exactly one MoveCursorRightRequested",
    );
}

/// Mutation guard for the test above: if `execute_editor_action` emitted
/// unconditionally — ignoring the action it was handed — the previous test
/// would still pass. Asking for a *different* action must not produce this
/// message.
#[test]
fn a_different_action_does_not_emit_move_right() {
    let mut app = make_app();
    spawn_focused_editor(&mut app);

    app.world_mut().resource_mut::<RequestedAction>().0 = Some(EditorAction::MoveCursorLeft);
    app.update();

    let messages = app
        .world()
        .resource::<Messages<crate::input::action_events::MoveCursorRightRequested>>();
    let mut cursor = messages.get_cursor();
    assert_eq!(
        cursor.read(messages).count(),
        0,
        "MoveCursorLeft must not emit MoveCursorRightRequested",
    );
}

/// `Misc::read_only` gating lives in the execution half, so it must still work
/// for a host that never touches leafwing. Without this, the read-only flag
/// would silently become leafwing-only.
#[test]
fn read_only_still_gates_mutating_actions_for_a_host() {
    let mut app = make_app();
    let entity = spawn_focused_editor(&mut app);
    app.world_mut()
        .entity_mut(entity)
        .get_mut::<crate::settings::Misc>()
        .unwrap()
        .read_only = true;

    app.world_mut().resource_mut::<RequestedAction>().0 = Some(EditorAction::DeleteBackward);
    app.update();

    let messages = app
        .world()
        .resource::<Messages<crate::input::action_events::DeleteBackwardRequested>>();
    let mut cursor = messages.get_cursor();
    assert_eq!(
        cursor.read(messages).count(),
        0,
        "read_only must suppress a mutating action regardless of what selected it",
    );
}

/// The other side of the gate: a non-mutating action is unaffected by
/// `read_only`, so the test above is measuring the gate and not a dead path.
#[test]
fn read_only_leaves_navigation_alone() {
    let mut app = make_app();
    let entity = spawn_focused_editor(&mut app);
    app.world_mut()
        .entity_mut(entity)
        .get_mut::<crate::settings::Misc>()
        .unwrap()
        .read_only = true;

    app.world_mut().resource_mut::<RequestedAction>().0 = Some(EditorAction::MoveCursorRight);
    app.update();

    let messages = app
        .world()
        .resource::<Messages<crate::input::action_events::MoveCursorRightRequested>>();
    let mut cursor = messages.get_cursor();
    assert_eq!(
        cursor.read(messages).count(),
        1,
        "read_only must not suppress navigation",
    );
}
