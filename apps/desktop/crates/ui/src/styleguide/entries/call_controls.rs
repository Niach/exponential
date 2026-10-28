//! S-3 — `call-controls` (2 Primitives): THE round icon-only call buttons
//! (`voice_overlay::call_buttons` on `exponential_ui_gpui::call_button`,
//! eUI `CallButton`), the same rows as the web entry
//! (`apps/styleguide/src/entries/call-controls.tsx`): each of
//! `fixtures/voice-call.json` `surfaceSpecimens`' button sets at the call
//! panel's 56 px (ringing: the red end alone, Cancel under it; in call:
//! mute, speaker on the phones, the red end; muted: the inverted fill),
//! then the compact row (the desktop's floating pill, 32 px buttons).

use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use domain::voice_call::VOICE_CALL;
use exponential_ui_gpui::controls::CallSize;

use crate::voice_overlay::{call_buttons, call_pill, specimen_face, CallActions};

pub(crate) const ID: &str = "call-controls";
pub(crate) const OWNER: &str = "S-3";

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let actions = CallActions::none();
    let mut sets = h_flex().flex_wrap().items_start().gap(px(40.));
    for (i, row) in VOICE_CALL.surface_specimens.iter().enumerate() {
        let face = specimen_face(row);
        sets = sets.child(
            v_flex()
                .items_center()
                .gap(px(12.))
                .child(call_buttons(("call-controls-lg", i), &face.view, CallSize::Lg, &actions, cx))
                .child(div().text_xs().text_color(cx.theme().muted_foreground).child(SharedString::from(row.name.clone()))),
        );
    }
    let mut pills = v_flex().gap(px(12.));
    for (i, row) in VOICE_CALL.surface_specimens.iter().enumerate() {
        let mut face = specimen_face(row);
        // The pill is the desktop's: no speaker.
        face.view.buttons.retain(|button| *button != domain::voice_call::CallButton::Speaker);
        pills = pills.child(call_pill(("call-controls-pill", i), &face, &actions, cx));
    }
    v_flex().gap(px(32.)).child(sets).child(pills)
}
