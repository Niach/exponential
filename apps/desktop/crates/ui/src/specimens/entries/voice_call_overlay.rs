//! NEW-77 — `voice-call-overlay` (3 Special components), the "Voice overlay":
//! the four states of THE capsule (`voice_overlay::capsule`) the push-to-talk
//! window draws, the same rows as the other platforms'
//! captures, all read from
//! `fixtures/voice.json` `overlaySpecimens` (#NEW-54); S-3: then the
//! FLOATING CALL PILL (`voice_overlay::call_pill`, `pill.width` ×
//! `pill.height`) for each of `voice-call.json` `surfaceSpecimens`, with the
//! desktop's buttons (mute and the red end).

use etalk::voice::VOICE;
use etalk::call::{CallButton, VOICE_CALL};
use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};

use etalk_gpui::voice_overlay::{call_pill, capsule, capsule_size, specimen_face, CallActions, Capsule};

pub(crate) const ID: &str = "voice-call-overlay";
pub(crate) const OWNER: &str = "NEW-77";

fn rows() -> Vec<Capsule> {
    VOICE
        .overlay_specimens
        .iter()
        .map(|row| Capsule {
            activity: etalk_gpui::push_to_talk::overlay_activity(&row.activity),
            level: row.level,
            bands: row.bands.clone(),
            state: row.state.clone().into(),
            line: row.line.clone().into(),
            pill: row.pill.clone().into(),
            primary: row.primary,
        })
        .collect()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    // Each capsule hugs its words (web's entry; the overlay window's own
    // `capsule_size`), never the column's width.
    let mut column = div().flex().flex_col().items_start().gap(px(12.));
    for (i, row) in rows().iter().enumerate() {
        let width = capsule_size(row, f32::MAX, cx).width;
        column = column.child(capsule(("voice-overlay-specimen", i), row, cx).w(width));
    }
    let actions = CallActions::none();
    for (i, row) in VOICE_CALL.surface_specimens.iter().enumerate() {
        let mut face = specimen_face(row);
        face.view.buttons.retain(|button| *button != CallButton::Speaker);
        column = column.child(call_pill(("voice-call-specimen", i), &face, &actions, cx));
    }
    column
}
