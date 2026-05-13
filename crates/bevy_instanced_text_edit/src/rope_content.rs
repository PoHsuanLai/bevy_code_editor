//! Rope-backed [`TextContent`] implementation for editable text views.
//!
//! [`RopeBuffer`] wraps a `ropey::Rope` and implements both [`TextContent`]
//! (so the engine can render it) and `Deref<Target = Rope>` (so all
//! existing edit call sites that call rope methods directly continue to work).

use std::borrow::Cow;
use std::ops::{Deref, DerefMut};

use bevy::prelude::*;
use bevy_instanced_text::TextContent;
use ropey::Rope;

/// A `ropey::Rope` wrapped as a Bevy `Component` with [`TextContent`] support.
///
/// This is the concrete content type for editable text views. Derefs to
/// `Rope` so all rope call sites (`buf.insert(...)`, `buf.char_to_line(...)`,
/// etc.) work without changes through `TextBuffer<RopeBuffer>`.
#[derive(Component, Clone, Default)]
pub struct RopeBuffer(pub Rope);

impl RopeBuffer {
    pub fn new(text: &str) -> Self {
        Self(Rope::from_str(text))
    }

    /// Access the inner rope. Convenient for call sites that pass `&Rope`
    /// to helpers like cursor_movement.
    pub fn rope(&self) -> &Rope {
        &self.0
    }
}

impl Deref for RopeBuffer {
    type Target = Rope;
    fn deref(&self) -> &Rope { &self.0 }
}

impl DerefMut for RopeBuffer {
    fn deref_mut(&mut self) -> &mut Rope { &mut self.0 }
}

impl TextContent for RopeBuffer {
    fn line_count(&self) -> usize {
        self.0.len_lines()
    }

    fn line(&self, i: usize) -> Cow<'_, str> {
        if i >= self.0.len_lines() {
            return Cow::Borrowed("");
        }
        Cow::Owned(self.0.line(i).to_string())
    }

    fn line_len_chars(&self, i: usize) -> usize {
        if i >= self.0.len_lines() {
            return 0;
        }
        let l = self.0.line(i);
        let mut n = l.len_chars();
        if n > 0 && l.char(n - 1) == '\n' {
            n -= 1;
        }
        n
    }
}
