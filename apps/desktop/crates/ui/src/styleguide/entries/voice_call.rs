//! NEW-55 / S-3 — `voice-call` (3 Special components): THE phone-call
//! surface (voice-call.json PHONE-CALL UI): phones full screen, web a
//! full-height overlay, the IDE the in-window CALL PANEL over the agent
//! chat's body (`voice_overlay::call_panel`). The twin paints the panel
//! for each of `fixtures/voice-call.json` `surfaceSpecimens` (ringing,
//! getting ready, speaking, listening, muted) in a narrow portrait frame
//! with the desktop's buttons (mute and the red end), the
//! same rows as the web entry (`apps/styleguide/src/entries/voice-call.tsx`).

use gpui::{div, px, App, Div, IntoElement as _, ParentElement as _, Styled as _, Window};
use gpui_component::{h_flex, ActiveTheme as _};

use etalk::call::VOICE_CALL;

use etalk_gpui::voice_overlay::{call_panel, specimen_face, CallActions};

pub(crate) const ID: &str = "voice-call";
pub(crate) const OWNER: &str = "NEW-55";

/// One specimen's frame (a narrow portrait: all five in one row), tall
/// enough for the panel at full size with a 4-line caption.
const FRAME_W: f32 = 208.;
const FRAME_H: f32 = etalk_gpui::voice_overlay::PANEL_FULL_H + 8.;

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let actions = CallActions::none();
    let mut row = h_flex().flex_wrap().items_start().gap(px(12.));
    for (i, specimen) in VOICE_CALL.surface_specimens.iter().enumerate() {
        let mut face = specimen_face(specimen);
        // The IDE's panel: no speaker (voice-call.json `callView(.., desktop)`).
        face.view.buttons.retain(|button| *button != etalk::call::CallButton::Speaker);
        let frame = div()
            .w(px(FRAME_W))
            .h(px(FRAME_H))
            .rounded(px(24.))
            .overflow_hidden()
            .border_1()
            .border_color(cx.theme().border)
            .child(call_panel(("voice-call-specimen", i), &face, &actions, cx));
        row = row.child(frame.into_any_element());
    }
    row
}
