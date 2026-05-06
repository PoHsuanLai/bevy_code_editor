//! `BevyTerminalPlugin`: register types + messages, declare system sets,
//! wire spawn / drain / shutdown, idempotently add input-focus and
//! picking infrastructure.
//!
//! System set order mirrors the editor's pattern:
//!
//! ```text
//! TerminalInputSet            ← keyboard + leafwing tick
//! TerminalActionDispatchSet   ← TerminalAction → typed *Requested messages
//! TerminalPtyDrainSet         ← drain crossbeam Receiver, mutate Term
//! TerminalApplyStateSet       ← scroll animation, mode flag updates
//! TerminalSnapshotSet         ← Term → GridSnapshot + LineStyles + BlockList
//! TerminalRenderingSet        ← engine's render set (we run after Snapshot)
//! ```

use bevy::prelude::*;

use crate::drain::drain_pty_events;
use crate::messages::*;
use crate::session::{on_terminal_added, TerminalEventLoopRegistry};
use crate::types::{
    BevyTerminal, TerminalBlockState, TerminalGridSnapshot, TerminalInputMode,
    TerminalScrollback, TerminalShellInfo, TerminalThemeConfig,
};

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalInputSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalActionDispatchSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalPtyDrainSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalApplyStateSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalSnapshotSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalRenderingSet;

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

        // Resources.
        app.init_resource::<TerminalEventLoopRegistry>();

        // System-set chain. Engine's render set runs naturally — we just
        // ensure our snapshot finishes before it picks up `LineStyles`.
        app.configure_sets(
            Update,
            (
                TerminalInputSet,
                TerminalActionDispatchSet.after(TerminalInputSet),
                TerminalPtyDrainSet.after(TerminalActionDispatchSet),
                TerminalApplyStateSet.after(TerminalPtyDrainSet),
                TerminalSnapshotSet.after(TerminalApplyStateSet),
                TerminalRenderingSet.after(TerminalSnapshotSet),
            )
                .chain(),
        );

        // Drain alacritty events into ECS once per frame.
        app.add_systems(Update, drain_pty_events.in_set(TerminalPtyDrainSet));

        // Spawn observer fires on `commands.spawn(BevyTerminal)`. We don't
        // register a Remove observer yet — the EventLoop thread exits when
        // its PTY is closed (which happens when the OS reaps the process).
        // Phase 4 may revisit when explicit despawn-during-runtime matters.
        app.add_observer(on_terminal_added);
    }
}
