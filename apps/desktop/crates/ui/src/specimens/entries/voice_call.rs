//! NEW-55 / S-3 — `voice-call` (3 Special components): THE phone-call
//! surface (voice-call.json PHONE-CALL UI): phones full screen, web a
//! full-height overlay, the IDE the in-window CALL PANEL over the agent
//! chat's body (`voice_overlay::call_panel`). The twin paints the panel
//! for each of `fixtures/voice-call.json` `surfaceSpecimens` (ringing,
//! getting ready, speaking, listening, muted, EXP-1857 falling back: the
//! `Voice on {device}` line under the timer) in a narrow portrait frame
//! with the desktop's buttons (mute and the red end), its `name` as the
//! caption centred under it: the same row as the web's
//! `VoiceCallSpecimen`. FEED-272: under them the call's two TABS ("in call,
//! speaking"): Chat = the tabs, the pill's row docked, the chat under it;
//! Voice = the tabs over the panel (web `VoiceCallTabsSpecimen`).

use gpui::{div, px, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use etalk::call::VOICE_CALL;

use etalk::call::CallTab;
use etalk_gpui::voice_overlay::{call_panel, call_pill_docked, call_tabs, specimen_face, CallActions, TabClick};

pub(crate) const ID: &str = "voice-call";
pub(crate) const OWNER: &str = "NEW-55";

/// One specimen's frame (a narrow portrait: all six in one row of the
/// 1440 px capture window), tall enough for the panel at full size with a
/// 4-line caption.
const FRAME_W: f32 = 172.;
const GAP: f32 = 12.;
const FRAME_H: f32 = etalk_gpui::voice_overlay::PANEL_FULL_H + 8.;

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let actions = CallActions::none();
    let mut row = h_flex().flex_wrap().items_start().gap(px(GAP));
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
        let caption = div()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(SharedString::from(specimen.name.clone()));
        row = row.child(v_flex().items_center().gap(px(8.)).child(frame).child(caption).into_any_element());
    }
    v_flex().gap(px(24.)).child(row).child(tabs_row(window, cx))
}

/// FEED-272: the two tabs side by side.
const TABS_FRAME_W: f32 = 520.;

fn tabs_row(window: &mut Window, cx: &mut App) -> Div {
    let actions = CallActions::none();
    let Some(row) = VOICE_CALL.surface_specimens.iter().find(|row| row.name == "in call, speaking") else { return div() };
    let mut face = specimen_face(row);
    face.view.buttons.retain(|button| *button != etalk::call::CallButton::Speaker);
    let noop: TabClick = std::rc::Rc::new(|_, _| {});
    let transcript = super::chat_transcript::render(window, cx);
    let frame = || {
        v_flex()
            .w(px(TABS_FRAME_W))
            .h(px(FRAME_H))
            .rounded(px(24.))
            .overflow_hidden()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
    };
    let chat = frame()
        .child(
            v_flex()
                .flex_none()
                .gap(px(12.))
                .px(px(16.))
                .pt(px(12.))
                .child(call_tabs(CallTab::Chat, noop.clone(), cx))
                .child(call_pill_docked("voice-call-tabs-specimen-pill", &face, &actions, cx)),
        )
        .child(div().flex_1().min_h_0().overflow_hidden().px(px(16.)).pt(px(16.)).child(transcript));
    let voice = frame()
        .child(div().flex_none().px(px(16.)).pt(px(12.)).child(call_tabs(CallTab::Voice, noop, cx)))
        .child(div().flex_1().min_h_0().child(call_panel("voice-call-tabs-specimen-panel", &face, &actions, cx)));
    let caption = |text: &'static str| div().text_xs().text_color(cx.theme().muted_foreground).child(text);
    h_flex()
        .items_start()
        .gap(px(GAP))
        .child(v_flex().items_center().gap(px(8.)).child(chat).child(caption("Chat tab")))
        .child(v_flex().items_center().gap(px(8.)).child(voice).child(caption("Voice tab")))
}
