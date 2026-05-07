//! Pluggable clipboard backend.
//!
//! Copy / cut / paste handlers go through the [`ClipboardProvider`]
//! resource so embedders running headlessly, in CI, on the web, or under
//! a custom protocol (Wayland-only paste, sandboxed multi-window IPC) can
//! plug in their own implementation.
//!
//! The default implementation, [`SystemClipboard`], wraps `arboard` and
//! matches the historical behavior: each operation constructs a fresh
//! `arboard::Clipboard`, succeeds-or-silently-no-ops on failure. Hosts
//! that want different semantics insert their own resource:
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use bevy_text_editor::{ClipboardProvider, ClipboardResource};
//!
//! #[derive(Default)]
//! struct InMemoryClipboard(std::sync::Mutex<String>);
//! impl ClipboardProvider for InMemoryClipboard {
//!     fn get_text(&self) -> Option<String> {
//!         Some(self.0.lock().ok()?.clone())
//!     }
//!     fn set_text(&self, text: &str) {
//!         if let Ok(mut g) = self.0.lock() {
//!             *g = text.to_owned();
//!         }
//!     }
//! }
//!
//! App::new()
//!     .insert_resource(ClipboardResource::new(InMemoryClipboard::default()))
//!     .run();
//! ```

use bevy::prelude::*;

/// Backing implementation for clipboard get / set. Methods take `&self`
/// (interior mutability) so the resource can stay `Res` rather than
/// `ResMut` and not serialize handlers behind a single mutable borrow.
pub trait ClipboardProvider: Send + Sync + 'static {
    /// Read the current clipboard text, or `None` if unavailable
    /// (no clipboard backend, paste blocked, headless, etc.).
    fn get_text(&self) -> Option<String>;

    /// Write text to the clipboard. Failures are swallowed — clipboard
    /// writes are best-effort; a missing backend should never propagate
    /// to the caller.
    fn set_text(&self, text: &str);
}

/// Resource holding the active clipboard backend. Inserted by
/// `TextInteractionPlugin` with [`SystemClipboard`] as the default;
/// override by inserting a custom one before plugin setup.
#[derive(Resource)]
pub struct ClipboardResource(Box<dyn ClipboardProvider>);

impl ClipboardResource {
    pub fn new<P: ClipboardProvider>(provider: P) -> Self {
        Self(Box::new(provider))
    }

    pub fn get_text(&self) -> Option<String> {
        self.0.get_text()
    }

    pub fn set_text(&self, text: &str) {
        self.0.set_text(text);
    }
}

impl Default for ClipboardResource {
    fn default() -> Self {
        Self::new(SystemClipboard)
    }
}

/// Default `arboard`-backed clipboard. A fresh `arboard::Clipboard` is
/// created per call: matches the original behavior and avoids holding a
/// platform handle across frames (which `arboard` documents as fragile
/// on X11 / Wayland).
pub struct SystemClipboard;

impl ClipboardProvider for SystemClipboard {
    fn get_text(&self) -> Option<String> {
        arboard::Clipboard::new().ok()?.get_text().ok()
    }

    fn set_text(&self, text: &str) {
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(text.to_owned());
        }
    }
}
