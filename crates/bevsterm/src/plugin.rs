use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;
use bevy_instanced_text::view::layout_builder::LayoutProduceSet;

use crate::drain::drain_pty_events;
use crate::messages::*;
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

/// Terminal renderer plugin. Handles VT parsing, grid snapshotting, input,
/// selection, and all ECS state. Does not spawn any PTY — add
/// [`BevyTerminalPtyPlugin`] alongside this for native PTY support, or supply
/// your own [`crate::types::TerminalSession`] and
/// [`crate::types::TerminalEventChannel`] for WASM / custom IO backends.
#[derive(Default)]
pub struct BevyTerminalPlugin;

impl Plugin for BevyTerminalPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<bevy::input_focus::InputDispatchPlugin>() {
            app.add_plugins(bevy::input_focus::InputDispatchPlugin);
        }
        app.add_plugins(
            bevy_text_interaction::InstancedTextInteractionPlugin::<bevy_instanced_text::TextSpan>::default(),
        );

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

        app.register_type::<bevy_text_interaction::CursorSettings>();
        app.register_type::<bevy_text_interaction::CursorStyle>();
        app.register_type::<bevy_instanced_text::TextColor>();
        app.register_type::<bevy_text_interaction::TextCursorColor>();
        app.register_type::<bevy_text_interaction::TextSelectionColor>();

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
        app.add_systems(
            Update,
            crate::viewport::sync_terminal_size.in_set(TerminalApplyStateSet),
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
                crate::clipboard::handle_focus,
                crate::clipboard::handle_clear,
            )
                .in_set(TerminalApplyStateSet),
        );
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
        app.register_type::<bevy_text_interaction::BlinkPhase>();
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

        app.add_observer(crate::input::on_focused_terminal_keyboard);
        app.add_observer(crate::blocks_pick::on_terminal_block_press);
    }
}

/// Full bundle for native hosts: text engine GPU + view, input dispatch,
/// text interaction, renderer, and PTY backend.
///
/// Disable individual entries with `.build().disable::<T>()` when composing
/// with a host that already owns one of the dependencies.
pub struct BevyTerminalPlugins;

impl PluginGroup for BevyTerminalPlugins {
    fn build(self) -> PluginGroupBuilder {
        let mut group = PluginGroupBuilder::start::<Self>()
            .add(bevy_instanced_text::gpu::GlyphAtlasPlugin)
            .add(bevy_instanced_text::gpu::InstancedTextRenderPlugin)
            .add(bevy_instanced_text::view::plugin::InstancedTextPlugin)
            .add(bevy::input_focus::InputDispatchPlugin)
            .add(bevy_text_interaction::InstancedTextInteractionPlugin::<bevy_instanced_text::TextSpan>::default())
            .add(BevyTerminalPlugin);
        #[cfg(feature = "pty")]
        {
            group = group.add(crate::session::BevyTerminalPtyPlugin);
        }
        group
    }
}
