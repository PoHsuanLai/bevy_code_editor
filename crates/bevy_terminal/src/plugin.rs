//! `BevyTerminalPlugin`: register types + messages, declare system sets,
//! wire spawn / drain / shutdown, idempotently add input-focus and
//! picking infrastructure.
//!
//! System sets — chained, each is a phase of one frame:
//!
//! ```text
//! TerminalPtyDrainSet      ← drain crossbeam Receiver, mirror term mode
//! TerminalApplyStateSet    ← clipboard handlers, viewport-driven resize
//! TerminalSnapshotSet      ← Term grid → LineStyles + caret overlay
//!                           (engine's LayoutProduceSet runs after this)
//! ```

use bevy::prelude::*;
use bevy_text_engine::view::layout_builder::LayoutProduceSet;

use crate::drain::drain_pty_events;
use crate::messages::*;
use crate::session::{on_terminal_added, TerminalEventLoopRegistry};
use crate::types::{
    BevyTerminal, TerminalBlockState, TerminalGridSnapshot, TerminalInputMode,
    TerminalScrollback, TerminalShellInfo, TerminalThemeConfig,
};

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalPtyDrainSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalApplyStateSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalSnapshotSet;

#[derive(Default)]
pub struct BevyTerminalPlugin;

impl Plugin for BevyTerminalPlugin {
    fn build(&self, app: &mut App) {
        // Idempotently bring in the picking + input-focus + interaction
        // plumbing the engine + editor crates rely on. Mirrors the editor.
        if !app.is_plugin_added::<bevy::input_focus::InputDispatchPlugin>() {
            app.add_plugins(bevy::input_focus::InputDispatchPlugin);
        }
        if !app.is_plugin_added::<bevy_text_editor::TextInteractionPlugin>() {
            app.add_plugins(bevy_text_editor::TextInteractionPlugin);
        }

        // Type registration — every plain-data Component & Message reflectable.
        app.register_type::<BevyTerminal>()
            .register_type::<TerminalGridSnapshot>()
            .register_type::<TerminalShellInfo>()
            .register_type::<TerminalInputMode>()
            .register_type::<TerminalBlockState>()
            .register_type::<TerminalThemeConfig>()
            .register_type::<TerminalScrollback>()
            .register_type::<TerminalReady>()
            .register_type::<TerminalExited>()
            .register_type::<TerminalTitleChanged>()
            .register_type::<TerminalBellRang>()
            .register_type::<TerminalCwdChanged>()
            .register_type::<TerminalBlockFinished>()
            .register_type::<TerminalWriteBytes>()
            .register_type::<TerminalRunCommand>()
            .register_type::<TerminalResize>()
            .register_type::<TerminalScrollTo>()
            .register_type::<TerminalClear>()
            .register_type::<TerminalCopySelection>()
            .register_type::<TerminalPaste>();

        // Message buses.
        app.add_message::<TerminalReady>()
            .add_message::<TerminalExited>()
            .add_message::<TerminalTitleChanged>()
            .add_message::<TerminalBellRang>()
            .add_message::<TerminalCwdChanged>()
            .add_message::<TerminalBlockFinished>()
            .add_message::<TerminalWriteBytes>()
            .add_message::<TerminalRunCommand>()
            .add_message::<TerminalResize>()
            .add_message::<TerminalScrollTo>()
            .add_message::<TerminalClear>()
            .add_message::<TerminalCopySelection>()
            .add_message::<TerminalPaste>();

        app.init_resource::<TerminalEventLoopRegistry>();
        app.init_resource::<bevy_text_editor::CursorSettings>();
        app.register_type::<bevy_text_editor::CursorSettings>();
        app.register_type::<bevy_text_editor::CursorStyle>();
        app.register_type::<bevy_text_engine::RenderTheme>();
        app.register_type::<bevy_text_editor::EditTheme>();

        // System-set chain. Engine's `LayoutProduceSet` reads our LineStyles
        // and TextBuffer, so it must run *after* our SnapshotSet.
        app.configure_sets(
            Update,
            (
                TerminalPtyDrainSet,
                TerminalApplyStateSet.after(TerminalPtyDrainSet),
                TerminalSnapshotSet.after(TerminalApplyStateSet),
            )
                .chain(),
        );
        app.configure_sets(Update, LayoutProduceSet.after(TerminalSnapshotSet));

        // Drain alacritty events into ECS once per frame.
        app.add_systems(Update, drain_pty_events.in_set(TerminalPtyDrainSet));

        // Viewport changes drive Term + PTY resize.
        app.add_systems(
            Update,
            crate::viewport::sync_terminal_size.in_set(TerminalApplyStateSet),
        );

        // Clipboard + raw-write message handlers. These run in
        // ApplyStateSet so any subsequent snapshot picks up the writes.
        app.add_systems(
            Update,
            (
                crate::clipboard::handle_copy_selection,
                crate::clipboard::handle_paste,
                crate::clipboard::handle_write_bytes,
                crate::clipboard::handle_run_command,
            )
                .in_set(TerminalApplyStateSet),
        );

        // Build the per-frame grid snapshot + LineStyles for the engine.
        app.add_systems(
            Update,
            crate::snapshot::sync_grid_snapshot.in_set(TerminalSnapshotSet),
        );

        // Caret tracking + overlay push.
        app.register_type::<crate::cursor::TerminalCursorBlink>();
        app.add_systems(
            Update,
            (
                crate::cursor::track_cursor_blink,
                crate::cursor::push_terminal_caret,
            )
                .chain()
                .in_set(TerminalSnapshotSet)
                .after(crate::snapshot::sync_grid_snapshot),
        );

        // Spawn observer fires on `commands.spawn(BevyTerminal)`. We don't
        // register a Remove observer yet — the EventLoop thread exits when
        // its PTY is closed (which happens when the OS reaps the process).
        // Phase 4 may revisit when explicit despawn-during-runtime matters.
        app.add_observer(on_terminal_added);

        // Keyboard firehose: per-event observer that translates keystrokes
        // to PTY bytes. Routed by `bevy::input_focus::FocusedInput<KeyboardInput>`
        // — the focused entity is the recipient.
        app.add_observer(crate::input::on_focused_terminal_keyboard);
    }
}
