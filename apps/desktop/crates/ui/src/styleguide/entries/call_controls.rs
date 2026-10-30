//! S-3 — `call-controls` (2 Primitives): THE round icon-only call buttons
//! (`voice_overlay::call_buttons` on `exponential_ui_gpui::call_button`,
//! eUI `CallButton`), the same rows as the web entry
//! (`apps/styleguide/src/entries/call-controls.tsx`): each of
//! `fixtures/voice-call.json` `surfaceSpecimens`' button sets at the call
//! panel's 56 px (ringing: the red end alone, Cancel under it; in call:
//! mute, speaker on the phones, the red end; muted: the inverted fill),
//! each beside its caption and followed by the same specimen's compact
//! row (the desktop's floating pill, 32 px buttons).

use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use etalk::call::VOICE_CALL;
use exponential_ui_gpui::controls::CallSize;

use etalk_gpui::voice_overlay::{call_buttons, call_pill, specimen_face, CallActions};

pub(crate) const ID: &str = "call-controls";
pub(crate) const OWNER: &str = "S-3";

/// The caption column (the longest name, "in call, listening", fits).
const CAPTION_W: f32 = 140.;
/// The 56 px button row: three buttons 32 px apart (the widest set), so
/// every specimen's pill starts at the same x.
const BUTTONS_W: f32 = 3. * 56. + 2. * 32.;

/// One row per specimen, like the web entry's grid: the caption beside its
/// own group (centred on the 56 px buttons; the ring's Cancel label hangs
/// under its button), the 56 px set, then the same specimen's compact pill.
pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let actions = CallActions::none();
    let mut rows = v_flex().gap(px(24.));
    for (i, row) in VOICE_CALL.surface_specimens.iter().enumerate() {
        let face = specimen_face(row);
        let mut pill_face = specimen_face(row);
        // The pill is the desktop's: no speaker.
        pill_face.view.buttons.retain(|button| *button != etalk::call::CallButton::Speaker);
        rows = rows.child(
            h_flex()
                .items_start()
                .gap(px(32.))
                .child(
                    h_flex()
                        .flex_none()
                        .w(px(CAPTION_W))
                        .h(px(56.))
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(row.name.clone())),
                )
                .child(
                    div()
                        .flex_none()
                        .w(px(BUTTONS_W))
                        .child(call_buttons(("call-controls-lg", i), &face.view, CallSize::Lg, &actions, cx)),
                )
                .child(
                    h_flex()
                        .flex_none()
                        .h(px(56.))
                        .child(call_pill(("call-controls-pill", i), &pill_face, &actions, cx)),
                ),
        );
    }
    rows
}
