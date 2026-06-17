//! Popup dismiss timers: tick each feature's grace timer and reset on expiry.

use bevy::prelude::*;

use crate::types::CodeEditor;

use super::super::completion::LspCompletionPopup;
use super::super::state::{
    LspCodeActionsPopup, LspHoverPopup, LspRenamePopup, LspSignatureHelpPopup,
};

/// Returns `true` when the lifecycle's `dismiss_after` timer expired
/// this frame *and* the pointer is not inside the popup chrome — the
/// caller should then run its feature-specific dismiss. The lifecycle
/// `dismiss()` is called here so the timer can't fire twice.
fn tick_dismiss_grace(
    lc: &mut super::super::state::PopupLifecycleData,
    dt: std::time::Duration,
) -> bool {
    let Some(timer) = lc.dismiss_after.as_mut() else {
        return false;
    };
    timer.tick(dt);
    if !timer.just_finished() {
        return false;
    }
    if lc.pointer_in_popup {
        // Pointer arrived after we armed the grace — leave the popup
        // up, drop the timer; the next out-event will re-arm.
        lc.dismiss_after = None;
        return false;
    }
    lc.dismiss();
    true
}

pub fn tick_popup_dismiss_hover(
    time: Res<Time>,
    mut q: Query<(&mut super::super::state::HoverLifecycle, &mut LspHoverPopup), With<CodeEditor>>,
) {
    for (mut lc, mut state) in q.iter_mut() {
        if tick_dismiss_grace(&mut lc, time.delta()) {
            state.reset();
        }
    }
}

pub fn tick_popup_dismiss_completion(
    time: Res<Time>,
    mut q: Query<
        (
            &mut super::super::state::CompletionLifecycle,
            &mut LspCompletionPopup,
        ),
        With<CodeEditor>,
    >,
) {
    for (mut lc, mut state) in q.iter_mut() {
        if tick_dismiss_grace(&mut lc, time.delta()) {
            state.dismiss();
        }
    }
}

pub fn tick_popup_dismiss_signature(
    time: Res<Time>,
    mut q: Query<
        (
            &mut super::super::state::SignatureLifecycle,
            &mut LspSignatureHelpPopup,
        ),
        With<CodeEditor>,
    >,
) {
    for (mut lc, mut state) in q.iter_mut() {
        if tick_dismiss_grace(&mut lc, time.delta()) {
            state.dismiss();
        }
    }
}

pub fn tick_popup_dismiss_code_actions(
    time: Res<Time>,
    mut q: Query<
        (
            &mut super::super::state::CodeActionsLifecycle,
            &mut LspCodeActionsPopup,
        ),
        With<CodeEditor>,
    >,
) {
    for (mut lc, mut state) in q.iter_mut() {
        if tick_dismiss_grace(&mut lc, time.delta()) {
            state.dismiss();
        }
    }
}

pub fn tick_popup_dismiss_rename(
    time: Res<Time>,
    mut q: Query<
        (
            &mut super::super::state::RenameLifecycle,
            &mut LspRenamePopup,
        ),
        With<CodeEditor>,
    >,
) {
    for (mut lc, mut state) in q.iter_mut() {
        if tick_dismiss_grace(&mut lc, time.delta()) {
            state.reset();
        }
    }
}
