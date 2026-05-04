//! Installs the shared tokio runtime that the async-lsp transport drives on.

use bevy::prelude::*;
use bevy_tokio_tasks::{TokioTasksPlugin, TokioTasksRuntime};

/// Adds [`TokioTasksPlugin`] iff no [`TokioTasksRuntime`] is already present,
/// so a host running its own tokio integration can share the runtime.
#[derive(Default)]
pub struct LspPlugin;

impl Plugin for LspPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<TokioTasksRuntime>() {
            app.add_plugins(TokioTasksPlugin::default());
        }
    }
}
