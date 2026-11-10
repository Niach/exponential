//! EXP-1681 — `call-pill` (3 Special components), the "Call pill": THE
//! floating call pill (`voice_overlay::call_pill`, `pill.width` ×
//! `pill.height`) for each of `voice-call.json` `surfaceSpecimens`, with the
//! compact buttons (mute and the red end), the same rows as the other platforms'
//! captures. The desktop shows it while
//! no IDE window is focused; the phones while a call is minimized.

use etalk::call::{CallButton, VOICE_CALL};
use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};

use etalk_gpui::voice_overlay::{call_pill, specimen_face, CallActions};

pub(crate) const ID: &str = "call-pill";
pub(crate) const OWNER: &str = "EXP-1681";

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let actions = CallActions::none();
    let mut column = div().flex().flex_col().items_start().gap(px(16.));
    for (i, row) in VOICE_CALL.surface_specimens.iter().enumerate() {
        let mut face = specimen_face(row);
        face.view.buttons.retain(|button| *button != CallButton::Speaker);
        column = column.child(call_pill(("call-pill-specimen", i), &face, &actions, cx));
    }
    column
}
