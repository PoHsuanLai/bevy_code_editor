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

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;
use bevy_instanced_text::view::layout_builder::LayoutProduceSet;

use crate::drain::drain_pty_events;
use crate::messages::*;
use crate::session::{on_terminal_removed, open_pending_sessions, TerminalEventLoopRegistry};
use crate::types::{
    BevyTerminal, TerminalBlockState, TerminalColorPalette, TerminalConfig, TerminalGridSnapshot,
    TerminalInputMode, TerminalScrollFollow, TerminalScrollback, TerminalShellInfo,
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
        if !app.is_plugin_added::<bevy::input_focus::InputDispatchPlugin>() {
            app.add_plugins(bevy::input_focus::InputDispatchPlugin);
        }
        if !app.is_plugin_added::<bevy_instanced_text_edit::InstancedTextInteractionPlugin>() {
            app.add_plugins(bevy_instanced_text_edit::InstancedTextInteractionPlugin);
        }

        app.register_type::<BevyTerminal>()
            .register_type::<TerminalConfig>()
            .register_type::<TerminalGridSnapshot>()
            .register_type::<TerminalShellInfo>()
            .register_type::<TerminalInputMode>()
            .register_type::<TerminalBlockState>()
            .register_type::<TerminalColorPalette>()
            .register_type::<TerminalScrollback>()
            .register_type::<TerminalScrollFollow>()
            .register_type::<TerminalExited>()
            .register_type::<TerminalTitleChanged>()
            .register_type::<TerminalBell>()
            .register_type::<TerminalReady>()
            .register_type::<TerminalSpawnFailed>()
            .register_type::<TerminalCwdChanged>()
            .register_type::<TerminalBlockFinished>()
            .register_type::<TerminalBlockSelected>()
            .register_type::<TerminalScrollFollowChanged>()
            .register_type::<TerminalWriteBytes>()
            .register_type::<TerminalRunCommand>()
            .register_type::<TerminalCopySelection>()
            .register_type::<TerminalPaste>()
            .register_type::<TerminalResize>()
            .register_type::<TerminalScrollTo>()
            .register_type::<TerminalScrollToBottom>()
            .register_type::<TerminalScrollToTop>()
            .register_type::<TerminalSendSignal>()
            .register_type::<TerminalFocus>()
            .register_type::<TerminalClear>();
        // `TerminalKeyInput` carries non-Reflect wezterm types and is intentionally
        // not registered for Reflect — it only needs to flow on the bus.

        // Message buses.
        app.add_message::<TerminalExited>()
            .add_message::<TerminalTitleChanged>()
            .add_message::<TerminalBell>()
            .add_message::<TerminalReady>()
            .add_message::<TerminalSpawnFailed>()
            .add_message::<TerminalCwdChanged>()
            .add_message::<TerminalBlockFinished>()
            .add_message::<TerminalBlockSelected>()
            .add_message::<TerminalScrollFollowChanged>()
            .add_message::<TerminalWriteBytes>()
            .add_message::<TerminalRunCommand>()
            .add_message::<TerminalCopySelection>()
            .add_message::<TerminalPaste>()
            .add_message::<TerminalResize>()
            .add_message::<TerminalScrollTo>()
            .add_message::<TerminalScrollToBottom>()
            .add_message::<TerminalScrollToTop>()
            .add_message::<TerminalKeyInput>()
            .add_message::<TerminalSendSignal>()
            .add_message::<TerminalFocus>()
            .add_message::<TerminalClear>();

        app.init_resource::<TerminalEventLoopRegistry>();
        app.register_type::<bevy_instanced_text_edit::CursorSettings>();
        app.register_type::<bevy_instanced_text_edit::CursorStyle>();
        app.register_type::<bevy_instanced_text::TextColor>();
        app.register_type::<bevy_instanced_text_edit::TextCursorColor>();
        app.register_type::<bevy_instanced_text_edit::TextSelectionColor>();

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

        app.add_systems(Update, drain_pty_events.in_set(TerminalPtyDrainSet));
        // Open the PTY before resizing it: spawn first, sync second.
        app.add_systems(
            Update,
            (
                open_pending_sessions,
                crate::viewport::sync_terminal_size.after(open_pending_sessions),
            )
                .in_set(TerminalApplyStateSet),
        );
        app.add_systems(
            Update,
            (
                crate::clipboard::handle_copy_selection,
                crate::clipboard::handle_paste,
                crate::clipboard::handle_write_bytes,
                crate::clipboard::handle_run_command,
                crate::clipboard::handle_resize,
                crate::clipboard::handle_scroll_to,
                crate::clipboard::handle_scroll_to_bottom,
                crate::clipboard::handle_scroll_to_top,
                crate::clipboard::handle_key_input,
                crate::clipboard::handle_send_signal,
                crate::clipboard::handle_focus,
                crate::clipboard::handle_clear,
            )
                .in_set(TerminalApplyStateSet),
        );
        // Runs after the snapshot tick so it sees the freshly-mutated follow
        // state and emits at most one event per actual transition per frame.
        app.add_systems(
            Update,
            crate::clipboard::emit_scroll_follow_changed
                .in_set(TerminalSnapshotSet)
                .after(crate::snapshot::sync_grid_snapshot),
        );
        app.add_systems(
            Update,
            crate::snapshot::sync_grid_snapshot.in_set(TerminalSnapshotSet),
        );
        app.add_systems(
            Update,
            crate::blocks::extract_blocks
                .in_set(TerminalSnapshotSet)
                .after(crate::snapshot::sync_grid_snapshot),
        );

        app.register_type::<crate::cursor::TerminalCursorCell>();
        app.register_type::<bevy_instanced_text_edit::BlinkPhase>();
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

        app.add_observer(on_terminal_removed);
        app.add_observer(crate::input::on_focused_terminal_keyboard);
        app.add_observer(crate::blocks_pick::on_terminal_block_press);
    }
}

/// Full bundle for embedders: registers `BevyTerminalPlugin` plus the
/// supporting plugins terminals need (text engine GPU + view, input
/// dispatch, text interaction). Mirrors `bevy_code_editor::CodeEditorPlugins`.
///
/// Disable individual entries with `.build().disable::<T>()` when composing
/// with hosts that already own one of the dependencies.
pub struct BevyTerminalPlugins;

impl PluginGroup for BevyTerminalPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(bevy_instanced_text::gpu::GlyphAtlasPlugin)
            .add(bevy_instanced_text::gpu::InstancedTextRenderPlugin)
            .add(bevy_instanced_text::view::plugin::InstancedTextPlugin)
            .add(bevy::input_focus::InputDispatchPlugin)
            .add(bevy_instanced_text_edit::InstancedTextInteractionPlugin)
            .add(BevyTerminalPlugin)
    }
}
