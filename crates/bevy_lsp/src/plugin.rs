//! Installs the shared tokio runtime that the async-lsp transport drives on.

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_tokio_tasks::{TokioTasksPlugin, TokioTasksRuntime};

use crate::client::LspClient;

/// Adds [`TokioTasksPlugin`] iff no [`TokioTasksRuntime`] is already present,
/// so a host running its own tokio integration can share the runtime.
/// Also wires an `AppExit` observer that gracefully shuts down every
/// `LspClient` before the app terminates.
#[derive(Default)]
pub struct LspPlugin;

impl Plugin for LspPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<TokioTasksRuntime>() {
            app.add_plugins(TokioTasksPlugin::default());
        }
        app.add_systems(Last, shutdown_clients_on_app_exit);
    }
}

/// Send `Shutdown` + `Exit` to every live `LspClient` when the app is
/// exiting. Keeps the language server from being hard-killed mid-request.
fn shutdown_clients_on_app_exit(
    mut exit: MessageReader<AppExit>,
    clients: Query<&LspClient>,
) {
    if exit.read().next().is_none() {
        return;
    }
    for client in clients.iter() {
        client.shutdown();
    }
}
