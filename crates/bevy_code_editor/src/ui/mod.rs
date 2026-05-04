//! Reusable UI components
//!
//! This module provides standalone UI components that can be used
//! independently of the code editor.

#[cfg(feature = "scrollbar")]
mod scrollbar;

#[cfg(feature = "scrollbar")]
pub use scrollbar::*;
