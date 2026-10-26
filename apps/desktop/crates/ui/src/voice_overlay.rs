//! NEW-77 — the voice overlay capsule (styleguide `voice-call-overlay`, the
//! "Voice overlay" entry): the voice device's one face while it listens or
//! speaks. `VoiceWave` (md, the brand mark as its child), the state as one
//! line, what was heard or said under it, and one pill. The window around it
//! (transparent, always on top, never focused, bottom centre of the display
//! under the pointer) comes with push to talk (#NEW-54); this is the capsule
//! both draw.

use exponential_ui::voice_wave::Activity;
use exponential_ui_gpui::chrome::Chrome;
use exponential_ui_gpui::controls::{glass_pill, glass_pill_button_primary, voice_wave, PillMode, PillSize, VoiceWaveChild, VoiceWaveColors};
use gpui::{div, prelude::FluentBuilder as _, px, AnyElement, App, ElementId, FontWeight, IntoElement as _, ParentElement as _, SharedString, Stateful, Styled as _, Div, InteractiveElement as _};
use gpui_component::{ActiveTheme as _, Icon};

use crate::icons::ExpIcon;

/// The capsule's width (web: `w-[26.25rem]`).
pub(crate) const CAPSULE_W: f32 = 420.;
/// The wave's edge (`VoiceWave` size `md`).
const WAVE: f32 = 56.;
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
}

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
    let id: ElementId = id.into();
    let chrome = Chrome::global(cx);
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
        glass_pill_button_primary((id.clone(), "pill"), PillSize::Sm).label(c.pill.clone()).into_any_element()
    } else {
        glass_pill((id.clone(), "pill"), PillSize::Sm, PillMode::Action, cx).child(c.pill.clone()).into_any_element()
    };
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(12.))
        .w(px(CAPSULE_W))
        .py(px(8.))
        .pl(px(8.))
        .pr(px(14.))
        .rounded_full()
        .border_1()
        .border_color(chrome.stroke_strong)
        .bg(theme.popover)
        .shadow_lg()
        .child(wave)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child(c.state.clone()))
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(c.line.clone()),
                ),
        )
        .child(pill)
}
