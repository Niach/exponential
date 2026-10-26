//! NEW-77 — `voice-call-overlay` (3 Special components), the "Voice overlay":
//! the four states of THE capsule (`voice_overlay::capsule`) the push-to-talk
//! window draws, the same rows as the web entry
//! (`apps/styleguide/src/entries/voice-call-overlay.tsx`). The call card
//! there stays the EXP-1268 draft for caller mode (V4).

use exponential_ui::voice_wave::Activity;
use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};

use crate::voice_overlay::{capsule, Capsule};

pub(crate) const ID: &str = "voice-call-overlay";
pub(crate) const OWNER: &str = "NEW-77";

const HEARD: &str = "“create an issue push to talk breaks under Wayland”";

fn rows() -> [Capsule; 4] {
    [
        Capsule { activity: Activity::Listening, level: 0.7, bands: vec![0.9, 0.6, 1., 0.5], state: "Listening".into(), line: HEARD.into(), pill: "Studio".into(), primary: false },
        Capsule { activity: Activity::Thinking, level: 0., bands: vec![], state: "Heard".into(), line: HEARD.into(), pill: "Studio".into(), primary: false },
        Capsule {
            activity: Activity::Speaking,
            level: 0.5,
            bands: vec![0.7, 1., 0.5, 0.8],
            state: "Speaking".into(),
            line: "Created NEW-81 “Push to talk breaks under Wayland”.".into(),
            pill: "Stop".into(),
            primary: true,
        },
        Capsule { activity: Activity::Muted, level: 0., bands: vec![], state: "Microphone muted".into(), line: "Unmute in the tray to talk".into(), pill: "Unmute".into(), primary: false },
    ]
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let mut column = div().flex().flex_col().gap(px(12.));
    for (i, row) in rows().iter().enumerate() {
        column = column.child(capsule(("voice-overlay-specimen", i), row, cx));
    }
    column
}
