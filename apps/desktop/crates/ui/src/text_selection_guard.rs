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
//! fork): mount [`selection_guard`] once per window — EVERY window, since every
//! one of them hosts the layer (`shell`, the two undocked windows,
//! `native_dialog`; gated structurally, see the tests) — and
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
/// and only installs the window handlers.
///
/// Mount it once per WINDOW ROOT, beside the other root layers: the `Shell`
/// (both of its roots — the app and the update-required surface), the two
/// undocked windows and the native dialogs all host the `TextSelection` layer,
/// and a drag armed in any of them outlives a window move the same way. The
/// rule is gated structurally by
/// `tests::every_window_root_mounts_the_selection_guard`.
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

    /// EXP-837 — the STRUCTURAL rule, so a new window cannot forget the guard:
    /// a window root composes the `Root` overlay layers
    /// (`Root::render_dialog_layer`), and every one of those roots hosts the
    /// `TextSelection` layer the guard disarms. So each render that mounts the
    /// layers must mount [`selection_guard`] too — once per root, which is why
    /// this counts call sites per file rather than merely looking for one
    /// (`shell.rs` has two roots: the app and the update-required surface).
    ///
    /// A grep over the crate's own sources is the cheapest honest check there
    /// is: the alternative (a helper every root must call) cannot be enforced
    /// either, since a root that forgets the helper compiles just as happily.
    #[test]
    fn every_window_root_mounts_the_selection_guard() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut checked = 0;
        let mut stack = vec![src];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("the crate's own sources are readable") {
                let path = entry.expect("a readable dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).expect("a readable source file");
                let roots = text.matches("Root::render_dialog_layer(").count();
                if roots == 0 {
                    continue;
                }
                let guards = text.matches("text_selection_guard::selection_guard()").count();
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                assert!(
                    guards >= roots,
                    "{name} composes {roots} window root(s) but mounts {guards} \
                     selection guard(s) — every window hosting the TextSelection \
                     layer needs `text_selection_guard::selection_guard()` (EXP-837)"
                );
                checked += 1;
            }
        }
        // The scan itself must not silently find nothing (a renamed helper, a
        // moved file): shell, the two undocked windows and native dialogs.
        assert!(checked >= 4, "expected every window-root file to be scanned, saw {checked}");
    }

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
