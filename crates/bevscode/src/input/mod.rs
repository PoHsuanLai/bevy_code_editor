//! Keyboard and mouse input handling.
//!
//! Input arrives in two stages, and only the first knows what a
//! *keybinding* is:
//!
//! 1. **Selection** — some frontend decides that an [`EditorAction`] should
//!    happen. Under the `leafwing` feature that is [`dispatch_action_events`],
//!    polling leafwing's `ActionState` and handling key repeat. A host with
//!    its own input pipeline replaces this stage wholesale.
//! 2. **Execution** — [`execute_editor_action`] turns one `EditorAction` into
//!    the `*Requested` messages in [`action_events`]. Read-only gating,
//!    tab-focus mode, goto-line and completion-popup interception, auto-indent
//!    and the save/open special cases all live here.
//!
//! Every handler downstream reads only the messages, never an `ActionState`,
//! so stage 2 and everything after it are leafwing-free. That is what makes
//! the feature a dependency gate rather than a behaviour switch.

pub mod action_events;
pub mod actions;
pub mod auto_indent;
pub mod dispatch;
pub mod editing;
pub mod handlers;
mod host_dispatch_tests;
pub mod keybindings;
pub mod keyboard;
pub mod mouse;
pub mod picking_backend;
pub mod word_boundary;

#[cfg(feature = "leafwing")]
pub use dispatch::dispatch_action_events;
pub use dispatch::execute_editor_action;
pub use editing::on_edit_invalidate_caches;
#[cfg(feature = "leafwing")]
pub use keybindings::default_input_map;
pub use keybindings::EditorAction;
pub use keyboard::on_focused_keyboard;
pub use mouse::{
    on_alt_click, on_click_past_eol_unfold, on_fold_gutter_press, on_pointer_move_for_gutter_hover,
};
#[cfg(feature = "lsp")]
pub use mouse::{
    on_ctrl_click_goto_definition, on_pointer_move_for_hover, on_pointer_out_for_hover,
    tick_lsp_hover_timer,
};

// Re-exported so a host building a custom `InputMap<EditorAction>` need not
// depend on leafwing directly *and* risk a second semver-incompatible copy —
// two versions of `Actionlike` make the derive on `EditorAction` unusable from
// the host's side. Feature-gated: with `leafwing` off, no leafwing type appears
// anywhere in this crate's public API.
#[cfg(feature = "leafwing")]
pub use leafwing_input_manager::prelude::{ActionState, Actionlike, ButtonlikeChord, InputMap};
