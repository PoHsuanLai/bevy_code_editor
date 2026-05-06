//! Phase-1 smoke test: spawn a `BevyTerminal`, verify the PTY thread
//! starts, watch for `TerminalReady` / `TerminalTitleChanged` /
//! `TerminalExited`. No rendering yet — Phase 2 hooks the grid.
//!
//! Run with: `cargo run -p bevy_terminal --example smoke_pty`

use bevy::prelude::*;
use bevy_terminal::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "bevy_terminal smoke".into(),
                resolution: [640u32, 360u32].into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(BevyTerminalPlugin)
        .add_systems(Startup, spawn_terminal)
        .add_systems(Update, log_events)
        .run();
}

fn spawn_terminal(mut commands: Commands) {
    commands.spawn(BevyTerminal);
    info!("Spawned BevyTerminal");
}

fn log_events(
    mut titles: MessageReader<TerminalTitleChanged>,
    mut bells: MessageReader<TerminalBellRang>,
    mut exits: MessageReader<TerminalExited>,
) {
    for ev in titles.read() {
        info!("title -> {:?}: {:?}", ev.entity, ev.title);
    }
    for ev in bells.read() {
        info!("bell -> {:?}", ev.entity);
    }
    for ev in exits.read() {
        info!("exit -> {:?}", ev);
    }
}
