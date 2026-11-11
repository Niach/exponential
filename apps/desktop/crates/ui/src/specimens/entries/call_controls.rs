//! S-3 — `call-controls` (2 Primitives): THE round icon-only call buttons
//! (`voice_overlay::call_buttons` on `eui_gpui::call_button`,
//! eUI `CallButton`), the same gallery as
//! `@exp/ui` `CallControlsSpecimen`: the tones (neutral, pressed, speaker, speaker
//! off, destructive, ringing: label under), then the button rows (phones in
//! call, web + desktop in call muted, ringing), then the compact row (the
//! desktop's floating pill), each with its caption under it.

use gpui::{div, px, AnyElement, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use etalk::call::{CallButton, CallView, VOICE_CALL};
use eui_gpui::controls::CallSize;

use etalk_gpui::voice_overlay::{call_buttons, call_pill, specimen_face, CallActions};

pub(crate) const ID: &str = "call-controls";
pub(crate) const OWNER: &str = "S-3";

/// A button set as a call view (the panel's 56 px buttons).
fn view(activity: &'static str, buttons: Vec<CallButton>, muted: bool, speaker: bool, end: &str) -> CallView {
    CallView { activity, status: None, caption: String::new(), muted, speaker, buttons, end: end.to_string() }
}

/// Web's `Labelled`: the specimen (at least 80 px tall, top-aligned) with
/// its caption centred under it.
fn labelled(label: &str, child: AnyElement, cx: &App) -> Div {
    v_flex()
        .items_center()
        .gap(px(12.))
        .child(h_flex().min_h(px(80.)).items_start().child(child))
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(SharedString::from(label.to_string())))
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let actions = CallActions::none();
    let copy = &VOICE_CALL.copy;
    let set = |id: &'static str, view: &CallView, cx: &App| call_buttons(id, view, CallSize::Lg, &actions, cx).into_any_element();

    let tones = [
        ("neutral", "call-controls-neutral", view("speaking", vec![CallButton::Mute], false, true, &copy.end_call)),
        ("pressed", "call-controls-pressed", view("muted", vec![CallButton::Mute], true, true, &copy.end_call)),
        ("speaker", "call-controls-speaker", view("speaking", vec![CallButton::Speaker], false, true, &copy.end_call)),
        ("speaker off", "call-controls-speaker-off", view("speaking", vec![CallButton::Speaker], false, false, &copy.end_call)),
        ("destructive", "call-controls-destructive", view("speaking", vec![CallButton::End], false, true, &copy.end_call)),
        ("ringing: label under", "call-controls-label-under", view("ringing", vec![CallButton::End], false, true, &copy.cancel)),
    ];
    let mut first = h_flex().flex_wrap().items_start().gap(px(32.));
    for (label, id, view) in &tones {
        first = first.child(labelled(label, set(id, view, cx), cx));
    }

    let sets = [
        (
            "phones in call",
            "call-controls-phones",
            view("speaking", vec![CallButton::Mute, CallButton::Speaker, CallButton::End], false, true, &copy.end_call),
        ),
        (
            "web + desktop in call, muted",
            "call-controls-muted",
            view("muted", vec![CallButton::Mute, CallButton::End], true, true, &copy.end_call),
        ),
        ("ringing", "call-controls-ringing", view("ringing", vec![CallButton::End], false, true, &copy.cancel)),
    ];
    let mut second = h_flex().flex_wrap().items_start().gap(px(48.));
    for (label, id, view) in &sets {
        second = second.child(labelled(label, set(id, view, cx), cx));
    }

    // The compact row: the in-call (speaking) specimen's pill, mute + end.
    let speaking = VOICE_CALL
        .surface_specimens
        .iter()
        .find(|row| row.name == "in call, speaking")
        .unwrap_or(&VOICE_CALL.surface_specimens[0]);
    let mut pill_face = specimen_face(speaking);
    pill_face.view.buttons = vec![CallButton::Mute, CallButton::End];
    pill_face.view.muted = false;
    pill_face.view.end = copy.end_call.clone();
    let pill = call_pill("call-controls-pill", &pill_face, &actions, cx).into_any_element();
    // Web's grid stretches this figure: the pill centred on the full width.
    let compact = labelled("compact row: the floating call pill", pill, cx).w_full();

    v_flex().w_full().items_start().gap(px(32.)).child(first).child(second).child(compact)
}
