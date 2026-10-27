//! NEW-77 — the voice overlay capsule (styleguide `voice-call-overlay`, the
//! "Voice overlay" entry): the voice device's one face while it listens or
//! speaks. It is THE bar (the bulk bar's glass capsule, the computer-use
//! banner's `glass_bar` with its `item` and `value`), holding `VoiceWave`
//! (md, the brand mark as its child), the state as the bold value, what
//! was heard or said as a muted item, and one pill. NEW-55: a call adds a
//! LEAD pill before it (Mute / Unmute; End call is then the primary pill),
//! and each pill takes its own click ([`call_capsule`]). The window around it
//! (transparent, always on top, never focused, bottom centre of the display
//! under the pointer) is push to talk's (`push_to_talk::overlay`, #NEW-54);
//! this is the capsule both draw, its words from `fixtures/voice.json`
//! (`pttCopy`, `overlaySpecimens`).

use exponential_ui::voice_wave::Activity;
use exponential_ui_gpui::controls::{glass_pill, glass_pill_button_primary, voice_wave, PillMode, PillSize, VoiceWaveChild, VoiceWaveColors};
use gpui::{
    div, prelude::FluentBuilder as _, px, size, AnyElement, App, Div, ElementId, FontWeight, InteractiveElement as _, IntoElement as _, ParentElement as _, Pixels,
    StatefulInteractiveElement as _,
    SharedString, Size, Stateful, Styled as _,
};
use gpui_component::{ActiveTheme as _, Icon};

use crate::computer_indicator::{
    item, rem_size, shaped_width, value, BAR_GAP_REMS, BAR_SIDE_REMS, BAR_SLACK, BAR_STROKE, BAR_TOP_REMS, ITEM_SIDE, LABEL_SIDE,
};
use crate::icons::ExpIcon;
use crate::surface::glass_bar;

/// The wave's edge (`VoiceWave` size `md`).
const WAVE: f32 = 56.;
/// The pill's sides (`PillSize::Sm` and the primary twin's `web_xs`: 8 px;
/// only the glass pill wears the 1 px stroke).
const PILL_SIDE: f32 = 8.;
/// The brand mark's own edge; the child rule scales it into the md box.
const MARK: f32 = 24.;

/// What one capsule shows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Capsule {
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
    /// NEW-55: a call's Mute / Unmute pill, before `pill` (End call).
    pub lead: Option<SharedString>,
}

/// NEW-55: a pill's click.
pub(crate) type PillClick = std::rc::Rc<dyn Fn(&mut App)>;

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
pub(crate) fn capsule(id: impl Into<ElementId>, c: &Capsule, cx: &App) -> Stateful<Div> {
    bar(id.into(), c, None, None, cx)
}

/// NEW-55: THE capsule of a call: the lead pill (Mute / Unmute) and the
/// primary pill (End call) each take their own click.
pub(crate) fn call_capsule(id: impl Into<ElementId>, c: &Capsule, on_lead: PillClick, on_pill: PillClick, cx: &App) -> Stateful<Div> {
    bar(id.into(), c, Some(on_lead), Some(on_pill), cx)
}

fn bar(id: ElementId, c: &Capsule, on_lead: Option<PillClick>, on_pill: Option<PillClick>, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let dim = matches!(c.activity, Activity::Idle | Activity::Muted);
    let mark = if c.activity == Activity::Muted { theme.muted_foreground } else { theme.foreground };
    let wave = div().flex_none().when(dim, |d| d.opacity(0.35)).child(voice_wave(
        (id.clone(), "wave"),
        c.activity,
        c.level,
        &c.bands,
        WAVE,
        Some(VoiceWaveChild::icon(Icon::from(ExpIcon::Logo), MARK, mark)),
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
    let lead = c.lead.clone().map(|lead| {
        glass_pill((id.clone(), "lead"), PillSize::Sm, PillMode::Action, cx)
            .flex_none()
            .child(lead)
            .when_some(on_lead, |pill, on_click| pill.on_click(move |_, _, cx| on_click(cx)))
    });
    glass_bar(cx)
        .id(id)
        .flex_shrink_0()
        .max_w_full()
        .child(wave)
        .child(value(c.state.clone(), cx).flex_none())
        .child(item(cx).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(c.line.clone()))
        .children(lead)
        .child(pill)
}

/// The bar's width around these shaped words at `rem`, whole pixels up:
/// the wave, the state as the value, the line as an item, the pill (and
/// NEW-55's lead pill, one more gap), three gaps, inside `glass_bar`'s
/// padding and stroke.
fn capsule_width(state: f32, line: f32, pill: f32, primary: bool, lead: Option<f32>, rem: f32) -> f32 {
    let bar = 2. * (BAR_SIDE_REMS * rem + BAR_STROKE);
    let value = LABEL_SIDE + state + LABEL_SIDE;
    let item = ITEM_SIDE + line + ITEM_SIDE;
    let pill = PILL_SIDE + pill + PILL_SIDE + if primary { 0. } else { 2. * BAR_STROKE };
    let lead = lead.map_or(0., |lead| PILL_SIDE + lead + PILL_SIDE + 2. * BAR_STROKE + BAR_GAP_REMS * rem);
    (bar + WAVE + value + item + pill + lead + 3. * BAR_GAP_REMS * rem).ceil()
}

/// The bar's height at `rem`: the wave between the padding and the stroke.
fn capsule_height(rem: f32) -> f32 {
    (WAVE + 2. * (BAR_TOP_REMS * rem + BAR_STROKE)).ceil()
}

/// The capsule's size for `c` (the `computer_indicator::banner_size`
/// rule): its words shaped in the theme's font plus [`BAR_SLACK`], capped
/// at `max_width` (a longer line ellipsizes).
pub(crate) fn capsule_size(c: &Capsule, max_width: f32, cx: &App) -> Size<Pixels> {
    let rem = rem_size(cx);
    // `text_sm` (the value and the item) and `text_xs` (the pill).
    let (sm, xs) = (rem * 0.875, rem * 0.75);
    let width = capsule_width(
        shaped_width(&c.state, sm, FontWeight::SEMIBOLD, cx),
        shaped_width(&c.line, sm, FontWeight::NORMAL, cx),
        shaped_width(&c.pill, xs, FontWeight::MEDIUM, cx),
        c.primary,
        c.lead.as_ref().map(|lead| shaped_width(lead, xs, FontWeight::MEDIUM, cx)),
        f32::from(rem),
    );
    size(px((width + BAR_SLACK).min(max_width.max(0.))), px(capsule_height(f32::from(rem))))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use domain::voice::VOICE;
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
                lead: None,
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
            theme::init(cx);
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
            theme::init(cx);
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
        assert_eq!(capsule_width(60., 100., 30., false, None, 16.), 22. + 56. + 68. + 116. + 48. + 12.);
        // The primary pill (a Button) has no stroke.
        assert_eq!(capsule_width(60., 100., 30., true, None, 16.), 22. + 56. + 68. + 116. + 46. + 12.);
        // NEW-55: a call's lead pill adds itself, its stroke and one gap.
        assert_eq!(capsule_width(60., 100., 30., true, Some(20.), 16.), 22. + 56. + 68. + 116. + 46. + 38. + 4. + 12.);
        // At 14 the bar's own spacing shrinks: 19.5 sides, 3.5 gaps.
        assert_eq!(capsule_width(60., 100., 30., false, None, 14.), (19.5 + 56. + 68. + 116. + 48. + 10.5_f32).ceil());
        assert_eq!(capsule_height(16.), 74.);
        assert_eq!(capsule_height(14.), 72.);
    }
}
