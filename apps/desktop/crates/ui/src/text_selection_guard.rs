//! EXP-837 — the window-level disarm for the text-selection drag.
//!
//! The symptom: drag the WINDOW by its decoration, release the button, and the
//! pointer then selects text across a session transcript as if the button were
//! still down. Once the selection is armed it follows the pointer until the
//! next click.
//!
//! The cause is the mouse-UP that never arrives. The selection drag lives in
//! the window-level `gpui_base::TextSelection` layer (mounted by
//! gpui-component's `Root`): its `mouse_down` arms `is_selecting`, every
//! `mouse_move` extends the selection to the pointer, and only `mouse_up`
//! disarms it. A window move (and a window resize) hands the pointer grab to
//! the compositor, so the release is delivered to the platform, never to the
//! view — and the layer keeps extending on a button nobody is holding.
//!
//! The fix is a HOST guard rather than a patched layer (the pin rule: desktop
//! features fit the public gpui/gpui-component API — see `docs`/CLAUDE.md, no
//! fork): mount [`selection_guard`] once per window and
//!
//! * every mouse MOVE with no primary button down ([`disarms_selection_drag`])
//!   ends the drag through the public `TextSelection::end` — in the CAPTURE
//!   phase, so it lands before the layer's own bubble-phase extension;
//! * so does losing window activation, which is where a drag that began in
//!   this window can no longer be finished in it.
//!
//! `end` keeps whatever was already selected visible (it is not a `clear`) —
//! a real sweep that ends over a gap must not lose its selection.

use gpui::{canvas, px, IntoElement, MouseButton, MouseMoveEvent, Styled as _};
use gpui_base::TextSelection;

/// EXP-837: whether a mouse MOVE must disarm a live selection drag — the
/// primary button is not held, so no drag can legitimately be in progress.
///
/// `pressed_button` is gpui's "which button is down right now" on every move
/// event; a real sweep reports `Some(MouseButton::Left)` for its whole length.
pub(crate) fn disarms_selection_drag(pressed_button: Option<MouseButton>) -> bool {
    pressed_button != Some(MouseButton::Left)
}

/// The guard element. Zero-sized and absolutely positioned — it paints nothing
/// and only installs the window handlers. Mount it once per window (the
/// `Shell`), beside the other root layers.
pub(crate) fn selection_guard() -> impl IntoElement {
    canvas(
        |_, _, _| (),
        |_, _, window, cx| {
            // Losing activation cannot be observed from a mouse event: a drag
            // that began here ends the moment the window is no longer the one
            // the pointer belongs to.
            if !window.is_window_active() {
                TextSelection::end(window, cx);
            }
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if !phase.capture() || !disarms_selection_drag(event.pressed_button) {
                    return;
                }
                // Cheap when nothing is armed: the layer's `end` returns at
                // once unless a drag is actually live.
                TextSelection::end(window, cx);
            });
        },
    )
    .absolute()
    .size(px(0.))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole rule: only a held PRIMARY button keeps a drag armed. A move
    /// with no button (the window-move release we never saw), or with some
    /// other button, disarms it.
    #[test]
    fn only_a_held_primary_button_keeps_the_drag_armed() {
        assert!(!disarms_selection_drag(Some(MouseButton::Left)));
        assert!(disarms_selection_drag(None));
        assert!(disarms_selection_drag(Some(MouseButton::Right)));
        assert!(disarms_selection_drag(Some(MouseButton::Middle)));
        assert!(disarms_selection_drag(Some(MouseButton::Navigate(
            gpui::NavigationDirection::Back
        ))));
    }
}
