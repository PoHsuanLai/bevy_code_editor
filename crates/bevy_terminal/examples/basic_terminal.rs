//! Phase-2 demo: spawn a terminal, render the alacritty grid via
//! `bevy_text_engine`. No keyboard input yet — that arrives in Phase 3.
//!
//! Run with: `cargo run -p bevy_terminal --example basic_terminal`

use bevy::prelude::*;
use bevy_terminal::prelude::*;
use bevy_text_engine::{FontConfig, TextEnginePlugins, TextViewViewport};

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
    mut titles: MessageReader<TerminalTitleChanged>,
    mut bells: MessageReader<TerminalBellRang>,
    mut exits: MessageReader<TerminalExited>,
) {
    for ev in titles.read() {
        info!("title({:?}): {:?}", ev.entity, ev.title);
    }
    for ev in bells.read() {
        info!("bell({:?})", ev.entity);
    }
    for ev in exits.read() {
        info!("exit: {:?}", ev);
    }
}
