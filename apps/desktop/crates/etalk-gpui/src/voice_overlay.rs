//! NEW-77 — the voice overlay capsule (styleguide `voice-call-overlay`, the
//! "Voice overlay" entry): the voice device's one face while it listens or
//! speaks. It is THE bar (the bulk bar's glass capsule, the computer-use
//! banner's `glass_bar` with its `item` and `value`), holding `VoiceWave`
//! (md, the brand mark as its child), the state as the bold value, what
//! was heard or said as a muted item, and one pill. S-3: a call no longer
//! rides the capsule: it is the PHONE-CALL surface below ([`call_panel`] in
//! the agent chat, [`call_pill`] floating while no IDE window is focused),
//! with the round call buttons ([`call_buttons`], styleguide
//! `call-controls`). The window around the capsule
//! (transparent, always on top, never focused, bottom centre of the display
//! under the pointer) is push to talk's (`push_to_talk::overlay`, #NEW-54);
//! this is the capsule both draw, its words from `fixtures/voice.json`
//! (`pttCopy`, `overlaySpecimens`).

use eui_core::voice_wave::Activity;
use etalk::call::{CallButton, CallView, SurfaceSpecimen, VOICE_CALL};
use eui_gpui::controls::{
    call_button, glass_bar, glass_pill, glass_pill_button_primary, voice_wave, CallSize, CallTone, PillMode, PillSize, VoiceWaveChild,
    VoiceWaveColors,
};
use gpui::{
    div, prelude::FluentBuilder as _, px, size, AnyElement, App, Div, ElementId, FontWeight, InteractiveElement as _, IntoElement as _, ParentElement as _, Pixels,
    StatefulInteractiveElement as _,
    SharedString, Size, Stateful, Styled as _,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use crate::bar::{
    item, rem_size, shaped_width, value, BAR_GAP_REMS, BAR_SIDE_REMS, BAR_SLACK, BAR_STROKE, BAR_TOP_REMS, ITEM_SIDE, LABEL_SIDE,
};
use crate::host::{icon as host_icon, TalkIcon};

/// The wave's edge (`VoiceWave` size `md`).
const WAVE: f32 = 56.;
/// The pill's sides (`PillSize::Sm` and the primary twin's `web_xs`: 8 px;
/// only the glass pill wears the 1 px stroke).
const PILL_SIDE: f32 = 8.;
/// The brand mark's own edge; the child rule scales it into the md box.
const MARK: f32 = 24.;

/// What one capsule shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Capsule {
    pub activity: Activity,
    pub level: f32,
    pub bands: Vec<f32>,
    /// The state line (Listening · Heard · Speaking · Microphone muted).
    pub state: SharedString,
    /// What was heard or said.
    pub line: SharedString,
    /// The one pill: the agent, Stop or Unmute.
    pub pill: SharedString,
    /// Stop while speaking is the one primary pill.
    pub primary: bool,
}

/// NEW-55: a pill's click.
pub type PillClick = std::rc::Rc<dyn Fn(&mut App)>;

/// The ring colours: `info` / `primary` / `primary` (the catalog's neutral
/// recipe), the muted foreground when idle or muted.
fn colors(activity: Activity, cx: &App) -> VoiceWaveColors {
    let theme = cx.theme();
    match activity {
        Activity::Idle | Activity::Muted => VoiceWaveColors::uniform(theme.muted_foreground),
        _ => VoiceWaveColors { outer: theme.info, mid: theme.primary, inner: theme.primary },
    }
}

/// THE capsule.
pub fn capsule(id: impl Into<ElementId>, c: &Capsule, cx: &App) -> Stateful<Div> {
    let id: ElementId = id.into();
    let on_pill: Option<PillClick> = None;
    let theme = cx.theme();
    let dim = matches!(c.activity, Activity::Idle | Activity::Muted);
    let mark = if c.activity == Activity::Muted { theme.muted_foreground } else { theme.foreground };
    let wave = div().flex_none().when(dim, |d| d.opacity(0.35)).child(voice_wave(
        (id.clone(), "wave"),
        c.activity,
        c.level,
        &c.bands,
        WAVE,
        Some(VoiceWaveChild::icon(host_icon(TalkIcon::Brand, cx), MARK, mark)),
        colors(c.activity, cx),
        Some("Voice".into()),
        cx,
    ));
    let pill: AnyElement = if c.primary {
        glass_pill_button_primary((id.clone(), "pill"), PillSize::Sm)
            .label(c.pill.clone())
            .when_some(on_pill, |button, on_click| button.on_click(move |_, _, cx| on_click(cx)))
            .into_any_element()
    } else {
        glass_pill((id.clone(), "pill"), PillSize::Sm, PillMode::Action, cx)
            .child(c.pill.clone())
            .when_some(on_pill, |pill, on_click| pill.on_click(move |_, _, cx| on_click(cx)))
            .into_any_element()
    };
    glass_bar(cx)
        .id(id)
        .flex_shrink_0()
        .max_w_full()
        .child(wave)
        .child(value(c.state.clone(), cx).flex_none())
        .child(item(cx).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(c.line.clone()))
        .child(pill)
}

/// The bar's width around these shaped words at `rem`, whole pixels up:
/// the wave, the state as the value, the line as an item, the pill, three
/// gaps, inside `glass_bar`'s padding and stroke.
fn capsule_width(state: f32, line: f32, pill: f32, primary: bool, rem: f32) -> f32 {
    let bar = 2. * (BAR_SIDE_REMS * rem + BAR_STROKE);
    let value = LABEL_SIDE + state + LABEL_SIDE;
    let item = ITEM_SIDE + line + ITEM_SIDE;
    let pill = PILL_SIDE + pill + PILL_SIDE + if primary { 0. } else { 2. * BAR_STROKE };
    (bar + WAVE + value + item + pill + 3. * BAR_GAP_REMS * rem).ceil()
}

/// The bar's height at `rem`: the wave between the padding and the stroke.
fn capsule_height(rem: f32) -> f32 {
    (WAVE + 2. * (BAR_TOP_REMS * rem + BAR_STROKE)).ceil()
}

/// The capsule's size for `c` (Exponential's computer-use banner's
/// `banner_size` rule): its words shaped in the theme's font plus [`BAR_SLACK`], capped
/// at `max_width` (a longer line ellipsizes).
pub fn capsule_size(c: &Capsule, max_width: f32, cx: &App) -> Size<Pixels> {
    let rem = rem_size(cx);
    // `text_sm` (the value and the item) and `text_xs` (the pill).
    let (sm, xs) = (rem * 0.875, rem * 0.75);
    let width = capsule_width(
        shaped_width(&c.state, sm, FontWeight::SEMIBOLD, cx),
        shaped_width(&c.line, sm, FontWeight::NORMAL, cx),
        shaped_width(&c.pill, xs, FontWeight::MEDIUM, cx),
        c.primary,
        f32::from(rem),
    );
    size(px((width + BAR_SLACK).min(max_width.max(0.))), px(capsule_height(f32::from(rem))))
}

// ---------------------------------------------------------------------------
// S-3: the PHONE-CALL surface (voice-call.json PHONE-CALL UI, `callView`).
// ---------------------------------------------------------------------------

/// The agent's mark on the call's avatar (its icon and colour; none = the
/// host's brand mark, the Assistant's).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CallMark {
    pub icon: Option<String>,
    pub color: Option<String>,
}

impl CallMark {
    fn icon(&self, cx: &App) -> Icon {
        match self.icon.as_deref().filter(|icon| !icon.is_empty()) {
            Some(icon) => host_icon(TalkIcon::Agent { icon, color: self.color.as_deref() }, cx),
            None => host_icon(TalkIcon::Brand, cx),
        }
    }
}

/// What one call surface shows: `callView` plus the agent, the timer and
/// the live microphone level.
#[derive(Clone, Debug, PartialEq)]
pub struct CallFace {
    pub agent: SharedString,
    pub mark: CallMark,
    pub view: CallView,
    /// `callTimer(ms since answered)`, shown while `view.status` is None.
    pub timer: SharedString,
    pub level: f32,
    pub bands: Vec<f32>,
}

impl CallFace {
    /// The line under the name: the status, or the timer.
    fn status(&self) -> SharedString {
        self.view.status.clone().map(SharedString::from).unwrap_or_else(|| self.timer.clone())
    }

    fn activity(&self) -> Activity {
        crate::push_to_talk::overlay::activity(self.view.activity)
    }
}

/// A styleguide row (`surfaceSpecimens`) as the face the surfaces draw.
pub fn specimen_face(row: &SurfaceSpecimen) -> CallFace {
    let activity = match row.activity.as_str() {
        "ringing" => "ringing",
        "listening" => "listening",
        "thinking" => "thinking",
        "speaking" => "speaking",
        _ => "muted",
    };
    CallFace {
        agent: SharedString::from(row.agent.clone()),
        mark: CallMark::default(),
        view: CallView {
            activity,
            status: row.status.clone(),
            caption: row.caption.clone(),
            muted: row.muted,
            speaker: row.speaker,
            buttons: row.buttons.clone(),
            end: row.end.clone(),
        },
        timer: SharedString::from(row.timer.clone().unwrap_or_default()),
        level: row.level,
        bands: row.bands.clone(),
    }
}

/// The surface's two clicks (Mute toggles, End ends or cancels).
#[derive(Clone)]
pub struct CallActions {
    pub on_mute: PillClick,
    pub on_end: PillClick,
}

impl CallActions {
    /// A styleguide specimen's (nothing happens).
    pub fn none() -> Self {
        let noop: PillClick = std::rc::Rc::new(|_| {});
        CallActions { on_mute: noop.clone(), on_end: noop }
    }
}

/// THE round call buttons (`call-controls`) in `view.buttons` order: mute
/// (toggled = the inverted fill), speaker (phones only, never here) and the
/// red end; while ringing (`Lg`) the end's label shows UNDER it.
pub fn call_buttons(id: impl Into<ElementId>, view: &CallView, size: CallSize, actions: &CallActions, cx: &App) -> Div {
    let id: ElementId = id.into();
    let copy = &VOICE_CALL.copy;
    let icons = &VOICE_CALL.icons;
    let concept = |name: &str| host_icon(TalkIcon::Concept(name), cx);
    let ringing = view.activity == "ringing";
    // A press on a button is never the pill's drag.
    let mut row = h_flex()
        .items_start()
        .gap(px(if size == CallSize::Lg { 32. } else { 8. }))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
    for button in &view.buttons {
        let element: AnyElement = match button {
            CallButton::Mute => {
                let (icon, label, tone) = if view.muted {
                    (&icons.unmute, &copy.unmute, CallTone::Toggled)
                } else {
                    (&icons.mute, &copy.mute, CallTone::Neutral)
                };
                let on_mute = actions.on_mute.clone();
                call_button((id.clone(), "mute"), concept(icon), tone, size, cx)
                    .tooltip(label.clone())
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        on_mute(cx)
                    })
                    .into_any_element()
            }
            CallButton::Speaker => {
                let (icon, label) = if view.speaker { (&icons.speaker, &copy.speaker) } else { (&icons.speaker_off, &copy.speaker_off) };
                call_button((id.clone(), "speaker"), concept(icon), CallTone::Neutral, size, cx)
                    .tooltip(label.clone())
                    .into_any_element()
            }
            CallButton::End => {
                let on_end = actions.on_end.clone();
                let end = call_button((id.clone(), "end"), concept(&icons.end), CallTone::Destructive, size, cx)
                    .tooltip(view.end.clone())
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        on_end(cx)
                    });
                if ringing && size == CallSize::Lg {
                    v_flex()
                        .items_center()
                        .gap(px(8.))
                        .child(end)
                        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(SharedString::from(view.end.clone())))
                        .into_any_element()
                } else {
                    end.into_any_element()
                }
            }
        };
        row = row.child(element);
    }
    row
}

/// The avatar's circle and the ring that pulses with the activity
/// (ringing: a soft halo; listening: the success ring glowing with the
/// level; speaking: the wave; thinking: dim; muted: none).
fn call_avatar(id: ElementId, face: &CallFace, edge: f32, cx: &App) -> AnyElement {
    use gpui::AnimationExt as _;
    let theme = cx.theme();
    let activity = face.activity();
    let disc = edge * 0.56;
    let mark = face.mark.icon(cx).with_size(px(disc * 0.5));
    let avatar = div()
        .size(px(disc))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.secondary)
        .border_1()
        .border_color(theme.border)
        .when(face.view.activity == "thinking", |d| d.opacity(0.6))
        .child(mark);
    let centred = |child: AnyElement| div().absolute().top_0().left_0().size(px(edge)).flex().items_center().justify_center().child(child);
    let ring: Option<AnyElement> = match face.view.activity {
        "ringing" => {
            let halo = theme.foreground;
            let reduced = cx.reduce_motion();
            let pulse = div().size(px(disc)).rounded_full().bg(halo.opacity(0.08));
            Some(if reduced {
                pulse.size(px(disc * 1.4)).into_any_element()
            } else {
                pulse
                    .with_animation(
                        (id.clone(), "halo"),
                        gpui::Animation::new(std::time::Duration::from_millis(1600)).repeat(),
                        move |pulse, delta| {
                            let grow = disc * (1. + 0.6 * delta);
                            pulse.size(px(grow)).bg(halo.opacity(0.14 * (1. - delta)))
                        },
                    )
                    .into_any_element()
            })
        }
        "listening" => {
            let glow = disc + 8. + 28. * face.level.clamp(0., 1.);
            Some(
                div()
                    .size(px(glow))
                    .rounded_full()
                    .bg(theme.success.opacity(0.12 + 0.18 * face.level.clamp(0., 1.)))
                    .border_2()
                    .border_color(theme.success.opacity(0.7))
                    .into_any_element(),
            )
        }
        "speaking" => Some(voice_wave(
            (id.clone(), "wave"),
            activity,
            face.level,
            &face.bands,
            edge,
            None,
            colors(activity, cx),
            Some("Voice".into()),
            cx,
        )),
        _ => None,
    };
    div()
        .relative()
        .flex_none()
        .size(px(edge))
        .children(ring.map(|ring| centred(ring)))
        .child(centred(avatar.into_any_element()))
        .into_any_element()
}

/// S-3: the desktop's in-window CALL PANEL over the agent chat's body (testId
/// `panel`): the avatar large and centred, the name, the status or the
/// timer, the caption (at most 4 lines), the round buttons at the bottom.
pub fn call_panel(id: impl Into<ElementId>, face: &CallFace, actions: &CallActions, cx: &App) -> Stateful<Div> {
    let id: ElementId = id.into();
    let theme = cx.theme();
    let timer = face.view.status.is_none();
    v_flex()
        .id(id.clone())
        .size_full()
        .items_center()
        .px(px(32.))
        .pb(px(48.))
        .bg(theme.background)
        // The top inset is the one part that gives way in a short panel
        // (72 down to 16): the caption never shrinks, so no line of it is
        // ever cut through.
        .child(div().flex_shrink_1().h(px(PANEL_TOP)).min_h(px(16.)))
        .child(call_avatar(id.clone(), face, AVATAR_LG, cx))
        .child(
            div()
                .mt(px(16.))
                .flex_none()
                .max_w_full()
                .truncate()
                .text_2xl()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.foreground)
                .child(face.agent.clone()),
        )
        .child(
            div()
                // testId `timer` while it counts.
                .id(if timer { ElementId::from(SharedString::from(VOICE_CALL.test_ids.timer.clone())) } else { (id.clone(), "status").into() })
                .mt(px(4.))
                .flex_none()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(face.status()),
        )
        .child(
            div()
                // Centred, at most `CAPTION_LINES` lines (voice-call.json
                // PHONE-CALL UI): whole lines, the last one ellipsised.
                .mt(px(32.))
                .flex_none()
                .max_w(px(480.))
                .text_center()
                .text_base()
                .line_height(px(CAPTION_LINE_H))
                .text_ellipsis()
                .line_clamp(CAPTION_LINES)
                .text_color(theme.foreground)
                .child(SharedString::from(face.view.caption.clone())),
        )
        .child(div().flex_1())
        .child(call_buttons((id, "buttons"), &face.view, CallSize::Lg, actions, cx).flex_none().mt(px(16.)))
}

/// The panel's avatar box (the halo and the wave inside it).
pub const AVATAR_LG: f32 = 200.;
/// The panel's top inset (it shrinks to 16 px in a short panel).
const PANEL_TOP: f32 = 72.;
/// The caption's clamp (voice-call.json: at most 4 lines) and line box.
pub const CAPTION_LINES: usize = 4;
const CAPTION_LINE_H: f32 = 24.;
/// The height the panel needs to show every part at full size with a
/// 4-line caption: inset, avatar, name, status, caption, the round buttons
/// (`CallSize::Lg`), bottom inset (the styleguide frame's floor).
pub const PANEL_FULL_H: f32 =
    PANEL_TOP + AVATAR_LG + 16. + 32. + 4. + 20. + 32. + CAPTION_LINES as f32 * CAPTION_LINE_H + 16. + 56. + 48.;
/// The pill's avatar.
const AVATAR_SM: f32 = 28.;

/// S-3: the FLOATING CALL PILL (testId `pill`), `pill.width` × `pill.height`
/// fixed: a drag handle, the avatar small, the name over the timer (both
/// truncate), the VoiceWave sm, the mute and end buttons.
pub fn call_pill(id: impl Into<ElementId>, face: &CallFace, actions: &CallActions, cx: &App) -> Stateful<Div> {
    let id: ElementId = id.into();
    let theme = cx.theme();
    let activity = face.activity();
    let pill = &VOICE_CALL.pill;
    let mark = face.mark.icon(cx).with_size(px(AVATAR_SM * 0.55));
    let avatar = div()
        .flex_none()
        .size(px(AVATAR_SM))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.secondary)
        .child(mark);
    let wave = div().flex_none().when(matches!(activity, Activity::Idle | Activity::Muted), |d| d.opacity(0.35)).child(voice_wave(
        (id.clone(), "wave"),
        activity,
        face.level,
        &face.bands,
        PILL_WAVE,
        None,
        colors(activity, cx),
        Some("Voice".into()),
        cx,
    ));
    glass_bar(cx)
        .id(id.clone())
        .w(px(pill.width))
        .h(px(pill.height))
        .flex_none()
        .rounded_full()
        .gap(px(8.))
        .pl(px(8.))
        .pr(px(12.))
        .cursor_grab()
        .child(grip(theme.muted_foreground))
        .child(avatar)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().truncate().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(theme.foreground).child(face.agent.clone()))
                .child(div().truncate().text_xs().text_color(theme.muted_foreground).child(face.status())),
        )
        // Ringing has no wave yet (the microphone is not open).
        .when(face.view.activity != "ringing", |pill| pill.child(wave))
        .child(call_buttons((id, "buttons"), &face.view, CallSize::Sm, actions, cx).items_center())
}

/// The pill's wave (`VoiceWave` sm).
const PILL_WAVE: f32 = 32.;

/// The pill's drag handle: two columns of three dots (no icon concept: it
/// is a texture, not a glyph).
fn grip(color: gpui::Hsla) -> Div {
    let dot = || div().size(px(2.)).rounded_full().bg(color);
    let column = || v_flex().gap(px(2.)).child(dot()).child(dot()).child(dot());
    h_flex().flex_none().gap(px(2.)).px(px(2.)).child(column()).child(column())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use etalk::voice::VOICE;
    use gpui::{Context, IntoElement, Render, Window};

    use super::*;

    /// A line longer than any display (the cap ellipsizes it).
    const LONG: &str = "“move NEW-54 to in review, assign it to me, label it voice and add a comment that the overlay is the bar now”";

    /// The fixture's four styleguide rows plus the long line.
    fn capsules() -> Vec<Capsule> {
        let mut rows: Vec<Capsule> = VOICE
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
            .collect();
        let long = Capsule { line: LONG.into(), ..rows[0].clone() };
        rows.push(long);
        rows
    }

    /// The capsule with a probe on it: the drawn bar's size.
    struct Probe {
        capsule: Capsule,
        drawn: Rc<Cell<(f32, f32)>>,
    }

    impl Render for Probe {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            window.set_rem_size(rem_size(cx));
            let drawn = self.drawn.clone();
            // Unconstrained (an 8000 px row), so the bar takes its own width.
            div().w(px(8000.)).flex().items_start().font_family(cx.theme().font_family.clone()).child(
                div().relative().flex_shrink_0().child(capsule("probe", &self.capsule, cx)).child(
                    gpui::canvas(
                        move |bounds, _, _| drawn.set((f32::from(bounds.size.width), f32::from(bounds.size.height))),
                        |_, _: (), _, _| {},
                    )
                    .absolute()
                    .size_full(),
                ),
            )
        }
    }

    /// #NEW-54: the measured size holds the drawn capsule (never narrower,
    /// at most the slack wider) for every styleguide row and a long line.
    #[gpui::test]
    async fn the_capsule_size_holds_the_drawn_capsule(cx: &mut gpui::TestAppContext) {
        // NOTE: the test platform's NoopTextSystem shapes every glyph 0.6
        // em, so this checks the box arithmetic, not real font metrics.
        cx.update(|cx| {
            gpui_component::init(cx);
            gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
        });
        for c in capsules() {
            let drawn = Rc::new(Cell::new((0., 0.)));
            let probe = Probe { capsule: c.clone(), drawn: drawn.clone() };
            let (_view, window) = cx.add_window_view(|_, _| probe);
            window.update(|window, cx| window.draw(cx).clear(cx));
            let size = window.update(|_, cx| capsule_size(&c, f32::MAX, cx));
            let (width, height) = drawn.get();
            assert!(width > 0., "{}: the capsule draws", c.line);
            let measured = f32::from(size.width);
            assert!(measured >= width && measured <= width + BAR_SLACK + 1., "{}: measured {measured}, drawn {width}", c.line);
            assert_eq!(f32::from(size.height), height, "{}: the capsule's height", c.line);
        }
    }

    #[gpui::test]
    async fn a_longer_line_widens_the_capsule_up_to_the_cap(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
        });
        let rows = capsules();
        let (short, long) = (&rows[0], &rows[4]);
        let short_width = f32::from(cx.update(|cx| capsule_size(short, f32::MAX, cx)).width);
        let long_width = f32::from(cx.update(|cx| capsule_size(long, f32::MAX, cx)).width);
        assert!(long_width > short_width, "long {long_width}, short {short_width}");
        // The capped capsule leaves the line to ellipsize.
        assert_eq!(f32::from(cx.update(|cx| capsule_size(long, 400., cx)).width), 400.);
    }

    #[test]
    fn the_capsule_width_adds_every_box_around_the_words() {
        // At 16: 22 bar sides + 56 wave + 8 value sides + 16 item sides +
        // 18 pill sides and stroke + 3 gaps of 4, around the words.
        assert_eq!(capsule_width(60., 100., 30., false, 16.), 22. + 56. + 68. + 116. + 48. + 12.);
        // The primary pill (a Button) has no stroke.
        assert_eq!(capsule_width(60., 100., 30., true, 16.), 22. + 56. + 68. + 116. + 46. + 12.);
        // At 14 the bar's own spacing shrinks: 19.5 sides, 3.5 gaps.
        assert_eq!(capsule_width(60., 100., 30., false, 14.), (19.5 + 56. + 68. + 116. + 48. + 10.5_f32).ceil());
        assert_eq!(capsule_height(16.), 74.);
        assert_eq!(capsule_height(14.), 72.);
    }
}
