//! `LspPlugin` — installs the shared tokio runtime that the async-lsp
//! transport drives its main loop on.
//!
//! Per-document state ([`crate::LspClient`], [`crate::LspDocument`],
//! [`crate::ServerCapabilities`]) is per-entity and inserted by the host (or
//! via the editor's `#[require]` cascade), so this plugin only registers the
//! [`bevy_tokio_tasks::TokioTasksPlugin`] resource. If the host already added
//! it, the redundant insert is a no-op (the resource is keyed by type).

use bevy::prelude::*;
use bevy_tokio_tasks::{TokioTasksPlugin, TokioTasksRuntime};

/// LSP transport plugin.
///
/// Adds [`TokioTasksPlugin`] iff no [`TokioTasksRuntime`] resource is present.
/// This lets a host that runs its own tokio integration share the runtime; a
/// host that doesn't gets the default multi-threaded runtime.
#[derive(Default)]
pub struct LspPlugin;

impl Plugin for LspPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<TokioTasksRuntime>() {
            app.add_plugins(TokioTasksPlugin::default());
        }
    }
}
