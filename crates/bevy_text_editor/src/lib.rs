//! Rope-backed text editor on top of [`bevy_instanced_text`] +
//! [`bevy_text_interaction`].
//!
//! Spawn a [`TextEditor`] entity with a [`bevy_instanced_text::TextBuffer`]
//! holding a [`RopeBuffer`]; the crate's plugin wires up edit history,
//! cursor movement, typed-char insertion, copy/cut/paste, undo/redo, and
//! anchors. Pair with [`InstancedTextEditPlugin`].
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use bevy_instanced_text::prelude::*;
//! use bevy_text_editor::{InstancedTextEditPlugin, RopeBuffer, TextEditor};
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(InstancedTextPlugins)
//!     .add_plugins(InstancedTextEditPlugin::default())
//!     .add_systems(Startup, |mut commands: Commands| {
//!         commands.spawn((
//!             TextEditor,
//!             TextBuffer::new(RopeBuffer::new("edit me")),
//!             TextFont::default(),
//!         ));
//!     })
//!     .run();
//! ```

pub mod cursor_movement;
pub mod edit;
pub mod editing_events;
pub mod handlers;
pub mod history;
pub mod plugin;
pub mod rope_content;
pub mod state;
pub mod typing;

pub use cursor_movement::*;
pub use edit::{point_at_byte, EditOutcome};
pub use editing_events::*;
pub use history::{EditHistory, EditKind, EditOperation, EditTransaction};
pub use plugin::{EditEmitSet, InstancedTextEditPlugin};
pub use rope_content::RopeBuffer;
pub use state::{
    EditDelta, EditHistoryState, EditPoint, IndentConfig, OnEdit, SnapshotPreEdit, TextEditor,
};

pub mod prelude {
    //! Common types for spawning editable text views.
    pub use crate::{
        EditDelta, EditEmitSet, EditHistoryState, EditKind, EditOperation, EditOutcome,
        EditPoint, EditTransaction, IndentConfig, InstancedTextEditPlugin, OnEdit, RopeBuffer,
        SnapshotPreEdit, TextEditor,
    };
    pub use bevy_text_interaction::prelude::*;
}
