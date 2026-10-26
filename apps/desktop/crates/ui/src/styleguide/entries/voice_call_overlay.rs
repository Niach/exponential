//! NEW-77 — `voice-call-overlay` (3 Special components), the "Voice overlay":
//! the four states of THE capsule (`voice_overlay::capsule`) the push-to-talk
//! window draws, the same rows as the web entry
//! (`apps/styleguide/src/entries/voice-call-overlay.tsx`), both read from
//! `fixtures/voice.json` `overlaySpecimens` (#NEW-54). Caller mode (V4)
//! brings its call card here.

use domain::voice::VOICE;
use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};

use crate::voice_overlay::{capsule, Capsule};

pub(crate) const ID: &str = "voice-call-overlay";
pub(crate) const OWNER: &str = "NEW-77";

fn rows() -> Vec<Capsule> {
    VOICE
        .overlay_specimens
        .iter()
        .map(|row| Capsule {
            activity: crate::push_to_talk::overlay_activity(&row.activity),
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
    let mut column = div().flex().flex_col().gap(px(12.));
    for (i, row) in rows().iter().enumerate() {
        column = column.child(capsule(("voice-overlay-specimen", i), row, cx));
    }
    column
}
