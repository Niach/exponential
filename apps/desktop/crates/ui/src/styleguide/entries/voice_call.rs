//! NEW-55 — `voice-call` (3 Special components): the call surface phones
//! (full screen, V5) and web (a centred card, V6) draw. The IDE draws a
//! call in THE capsule (`voice-call-overlay`), so this twin paints the web
//! entry's cards statically (`apps/styleguide/src/entries/voice-call.tsx`):
//! a glass card, `VoiceWave` xl with the brand mark, the state, the line,
//! then End call (the one primary pill) and Mute, the rows of
//! `fixtures/voice-call.json` `surfaceSpecimens`.

use exponential_ui_gpui::controls::{glass_pill, glass_pill_button_primary, voice_wave, PillMode, PillSize, VoiceWaveChild, VoiceWaveColors};
use gpui::{div, px, App, Div, FontWeight, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon};

use domain::voice_call::{SurfaceSpecimen, VOICE_CALL};

use crate::icons::ExpIcon;

pub(crate) const ID: &str = "voice-call";
pub(crate) const OWNER: &str = "NEW-55";

/// The card's width (the web card's 22rem).
const CARD_W: f32 = 352.;
/// `VoiceWave` xl.
const WAVE: f32 = 200.;
/// The brand mark inside it (the web island's 56).
const MARK: f32 = 56.;

fn card(i: usize, row: &SurfaceSpecimen, cx: &App) -> Div {
    let theme = cx.theme();
    let activity = crate::push_to_talk::overlay_activity(&row.activity);
    let colors = match row.activity.as_str() {
        "muted" => VoiceWaveColors::uniform(theme.muted_foreground),
        _ => VoiceWaveColors { outer: theme.info, mid: theme.primary, inner: theme.primary },
    };
    let wave = voice_wave(
        ("voice-call-wave", i),
        activity,
        row.level,
        &row.bands,
        WAVE,
        Some(VoiceWaveChild::icon(Icon::from(ExpIcon::Logo), MARK, theme.foreground)),
        colors,
        Some("Voice".into()),
        cx,
    );
    crate::surface::glass_card()
        .w(px(CARD_W))
        .px(px(24.))
        .py(px(24.))
        .child(
            v_flex()
                .items_center()
                .gap(px(16.))
                .child(wave)
                .child(
                    v_flex()
                        .items_center()
                        .gap(px(4.))
                        .text_sm()
                        .child(div().font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child(SharedString::from(row.state.clone())))
                        .child(div().text_center().text_color(theme.muted_foreground).child(SharedString::from(row.line.clone()))),
                )
                .child(
                    h_flex()
                        .gap(px(8.))
                        .child(glass_pill_button_primary(("voice-call-end", i), PillSize::Md).label(VOICE_CALL.copy.end_call.clone()))
                        .child(glass_pill(("voice-call-mute", i), PillSize::Md, PillMode::Action, cx).child(SharedString::from(row.mute.clone()))),
                ),
        )
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let mut row = h_flex().flex_wrap().items_start().gap(px(16.));
    for (i, specimen) in VOICE_CALL.surface_specimens.iter().enumerate() {
        row = row.child(card(i, specimen, cx).into_any_element());
    }
    row
}
