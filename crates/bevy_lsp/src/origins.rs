//! Request-origin tracking for shared `LspClient` entities.

use bevy_ecs::prelude::*;
use std::collections::HashMap;

/// Tracks which entity originated each outgoing LSP request.
///
/// When multiple entities share one [`crate::LspClient`] (e.g. several
/// editors pointing at the same language server), attach this component
/// to the `LspClient` entity. [`crate::LspPlugin`] will then:
///
/// - Record `(request_id → origin)` when dispatching requests that
///   carry an [`LspRequest::origin`](crate::LspRequest::origin).
/// - Resolve the origin in [`drain_lsp_responses`](crate::plugin) so
///   ID-bearing response messages are stamped with the originating
///   entity instead of the `LspClient` entity.
#[derive(Component, Default, Debug)]
pub struct LspRequestOrigins {
    pending: HashMap<u64, Entity>,
}

impl LspRequestOrigins {
    pub fn track(&mut self, request_id: u64, origin: Entity) {
        self.pending.insert(request_id, origin);
    }

    pub fn resolve(&mut self, request_id: u64) -> Option<Entity> {
        self.pending.remove(&request_id)
    }
}
