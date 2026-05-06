//! Single-pane terminal demo. Spawns a `BevyTerminal`, fills the
//! window. Type into it; resize it; drag-select; Cmd+C / Cmd+V (or
//! Ctrl+Shift+C / Ctrl+Shift+V on Linux/Windows).
//!
//! Run with: `cargo run -p bevy_terminal --example basic_terminal`

use bevy::prelude::*;
use bevy_terminal::prelude::*;
use bevy_text_engine::TextEnginePlugins;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "bevy_terminal — basic".into(),
                resolution: [960u32, 600u32].into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(TextEnginePlugins)
        .add_plugins(BevyTerminalPlugin)
        .add_systems(Startup, (setup_camera, spawn_terminal))
        .add_systems(Update, log_events)
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn spawn_terminal(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    windows: Query<&Window>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    let regular: Handle<bevy::text::Font> = asset_server.load("fonts/FiraMono-Regular.ttf");
    let bold: Handle<bevy::text::Font> = asset_server.load("fonts/FiraMono-Medium.ttf");

    let font = FontConfig::from_size(14.0)
        .with_font(regular)
        .with_bold_font(bold);

    let logical_w = window.width() as u32;
    let logical_h = window.height() as u32;

    commands.spawn((
        BevyTerminal,
        font,
        TextViewViewport {
            width: logical_w,
            height: logical_h,
            text_area_left: 12.0,
            text_area_top: 12.0,
            ..default()
        },
    ));

    info!("Spawned BevyTerminal");
}

fn log_events(
    mut ready: MessageReader<TerminalReady>,
    mut titles: MessageReader<TerminalTitleChanged>,
    mut bells: MessageReader<TerminalBellRang>,
    mut exits: MessageReader<TerminalExited>,
    mut cwd: MessageReader<TerminalCwdChanged>,
    mut finished: MessageReader<TerminalBlockFinished>,
    mut selected: MessageReader<TerminalBlockSelected>,
) {
    for ev in ready.read() {
        info!("ready({:?}): {}x{}", ev.entity, ev.cols, ev.rows);
    }
    for ev in titles.read() {
        info!("title({:?}): {:?}", ev.entity, ev.title);
    }
    for ev in bells.read() {
        info!("bell({:?})", ev.entity);
    }
    for ev in exits.read() {
        info!("exit: {:?}", ev);
    }
    for ev in cwd.read() {
        info!("cwd({:?}): {}", ev.entity, ev.cwd);
    }
    for ev in finished.read() {
        info!(
            "block_finished({:?}): id={} exit={:?}",
            ev.entity, ev.block_id, ev.exit_code
        );
    }
    for ev in selected.read() {
        info!("block_selected({:?}): id={}", ev.entity, ev.block_id);
    }
}
