//! EXP-1324 — THE Trace, the desktop's own gpui port of shadcn's Trace
//! (`fixtures/trace.json`, pixel-exact off its `geometry`): a LINE is
//! `[icon][gap][title][gap][description]` in one row of `lineHeight`; a
//! COLLAPSIBLE line appends the chevron (`chevronGap` after the line, rotated
//! 90° while open over motion `standard`) and opens onto a railed content
//! column. A running title wears the `motion.shimmer` paint; `error` tints
//! the line destructive; `pending` dims it. The session transcript, the
//! issue activity and the Queue all draw through here.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use gpui::{
    div, percentage, px, Animation, AnimationExt as _, AnyElement, App, ClickEvent, ElementId,
    HighlightStyle, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Rgba, SharedString, StatefulInteractiveElement as _, Styled as _, StyledText, TextRun,
    Transformation, Window,
};
use gpui::prelude::FluentBuilder as _;
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon};
use steer::trace::{geometry, TraceGeometry, TraceStatus};

use crate::icons::{registry, ExpIcon};

/// The two Trace scales (`geometry.default` / `geometry.sm`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TraceSize {
    /// The styleguide's text-sm scale.
    Default,
    /// The session transcript, the issue activity and the Queue.
    Sm,
}

impl TraceSize {
    pub(crate) fn geometry(self) -> TraceGeometry {
        match self {
            TraceSize::Default => geometry::DEFAULT,
            TraceSize::Sm => geometry::SM,
        }
    }
}

/// A concept icon name (`trace-terminal`) → its glyph; an unknown one is
/// the generic tool.
pub(crate) fn trace_icon(concept: &str) -> ExpIcon {
    registry::concept_by_name(concept).unwrap_or(registry::TRACE_TOOL)
}

/// One Trace line, ready to paint.
#[derive(Clone)]
pub(crate) struct TraceLine {
    pub id: ElementId,
    pub icon: Icon,
    pub title: SharedString,
    pub description: Option<SharedString>,
    /// Paint the description as the outline Badge (a subagent's duration).
    pub badge: bool,
    /// Append a static right chevron (a line that opens elsewhere: a
    /// subagent's tab), web's trailing `TraceIcon`.
    pub chevron: bool,
    pub status: Option<TraceStatus>,
    pub size: TraceSize,
}

impl TraceLine {
    pub(crate) fn new(id: impl Into<ElementId>, icon: impl Into<Icon>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            icon: icon.into(),
            title: title.into(),
            description: None,
            badge: false,
            chevron: false,
            status: None,
            size: TraceSize::Sm,
        }
    }

    /// From the shared model (`steer::trace::TraceLine`).
    pub(crate) fn from_model(id: impl Into<ElementId>, model: &steer::trace::TraceLine) -> Self {
        Self {
            description: model.description.clone().map(SharedString::from),
            status: model.status,
            ..Self::new(id, Icon::from(trace_icon(&model.icon)), model.title.clone())
        }
    }

    pub(crate) fn description(mut self, description: Option<impl Into<SharedString>>) -> Self {
        self.description = description.map(Into::into);
        self
    }

    pub(crate) fn badge(mut self, badge: bool) -> Self {
        self.badge = badge;
        self
    }

    pub(crate) fn chevron(mut self, chevron: bool) -> Self {
        self.chevron = chevron;
        self
    }

    pub(crate) fn status(mut self, status: Option<TraceStatus>) -> Self {
        self.status = status;
        self
    }

    pub(crate) fn size(mut self, size: TraceSize) -> Self {
        self.size = size;
        self
    }
}

/// The text colour a line's status paints its title in.
fn tint(status: Option<TraceStatus>, cx: &App) -> Hsla {
    match status {
        Some(TraceStatus::Error) => cx.theme().danger,
        _ => cx.theme().muted_foreground,
    }
}

/// The line's row itself: icon, title, description — no chevron.
fn line_row(line: &TraceLine, interactive: bool, window: &Window, cx: &App) -> gpui::Div {
    let g = line.size.geometry();
    let color = tint(line.status, cx);
    let foreground = cx.theme().foreground;
    let title: AnyElement = if line.status == Some(TraceStatus::Running) {
        shimmer_text(
            SharedString::from(format!("trace-shimmer-{:?}", line.id)),
            line.title.clone(),
            color,
            g.font_size,
            window,
        )
    } else {
        StyledText::new(line.title.clone()).into_any_element()
    };
    let mut row = h_flex()
        .h(px(g.line_height))
        .min_w_0()
        .flex_shrink_1()
        .gap(px(g.gap))
        .items_center()
        .text_size(px(g.font_size))
        .line_height(px(g.line_height))
        .text_color(color)
        .child(
            div()
                .flex_shrink_0()
                .size(px(g.icon))
                .flex()
                .items_center()
                .justify_center()
                .child(line.icon.clone().size(px(g.icon)).text_color(color)),
        )
        .child(
            div()
                .min_w_0()
                .flex_shrink_1()
                .truncate()
                .when(interactive, |this| this.group_hover("trace-row", move |s| s.text_color(foreground)))
                .child(title),
        );
    if let Some(description) = line.description.clone() {
        row = row.child(if line.badge {
            badge(description, line.size, cx)
        } else {
            div()
                .flex_shrink_0()
                .text_color(color.opacity(geometry::DESCRIPTION_ALPHA))
                .child(description)
                .into_any_element()
        });
    }
    if line.chevron {
        row = row.child(
            div()
                .flex_shrink_0()
                .size(px(g.icon))
                .flex()
                .items_center()
                .justify_center()
                .child(Icon::new(registry::TRACE_CHEVRON).size(px(g.icon)).text_color(color)),
        );
    }
    row
}


/// The outline Badge a subagent line's duration paints as (shadcn `Badge
/// variant=outline`, at the line's scale).
fn badge(text: SharedString, size: TraceSize, cx: &App) -> AnyElement {
    // Web: rounded-full border px-1.5 text-[10px] leading-4, the
    // description's muted ink (12 px at the default size, as on iOS).
    let font = size.geometry().font_size - 2.;
    div()
        .flex_shrink_0()
        .px(px(6.))
        .h(px(18.))
        .flex()
        .items_center()
        .rounded_full()
        .border_1()
        .border_color(cx.theme().border)
        .text_size(px(font))
        .line_height(px(16.))
        .text_color(cx.theme().muted_foreground.opacity(geometry::DESCRIPTION_ALPHA))
        .child(text)
        .into_any_element()
}

/// A plain (non-collapsible) Trace line.
pub(crate) fn trace_line(line: TraceLine, window: &Window, cx: &App) -> AnyElement {
    let pending = line.status == Some(TraceStatus::Pending);
    line_row(&line, false, window, cx)
        .w_full()
        .when(pending, |this| this.opacity(geometry::PENDING_OPACITY))
        .into_any_element()
}

/// A plain line that is still a click target (a subagent line opening its
/// tab): the title brightens under the pointer.
pub(crate) fn trace_link(
    line: TraceLine,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let pending = line.status == Some(TraceStatus::Pending);
    let id = line.id.clone();
    h_flex()
        .id(id)
        .group("trace-row")
        .w_full()
        .min_w_0()
        .cursor_pointer()
        .when(pending, |this| this.opacity(geometry::PENDING_OPACITY))
        .child(line_row(&line, true, window, cx))
        .on_click(on_click)
        .into_any_element()
}

thread_local! {
    /// When each collapsible last toggled — the chevron animates only right
    /// after a toggle, never when a row scrolls back into the list.
    static TOGGLED: RefCell<HashMap<ElementId, Instant>> = RefCell::new(HashMap::new());
}

fn note_toggle(id: &ElementId) {
    TOGGLED.with(|toggled| {
        let mut toggled = toggled.borrow_mut();
        if toggled.len() > 256 {
            toggled.retain(|_, at| at.elapsed() < theme::motion::STANDARD);
        }
        toggled.insert(id.clone(), Instant::now());
    });
}

fn toggled_recently(id: &ElementId) -> Option<Instant> {
    TOGGLED.with(|toggled| {
        toggled
            .borrow()
            .get(id)
            .copied()
            .filter(|at| at.elapsed() < theme::motion::STANDARD)
    })
}

/// The chevron: right while closed, rotated 90° (down) while open, turning
/// over motion `standard` right after a toggle.
fn chevron(id: &ElementId, open: bool, size: TraceSize, color: Hsla) -> AnyElement {
    let g = size.geometry();
    let glyph = Icon::new(registry::TRACE_CHEVRON).size(px(g.icon)).text_color(color);
    let (from, to) = if open { (0.0, 0.25) } else { (0.25, 0.0) };
    match toggled_recently(id) {
        Some(at) => glyph
            .with_animation(
                SharedString::from(format!("trace-chevron-{id:?}-{at:?}")),
                Animation::new(theme::motion::STANDARD).with_easing(theme::motion::standard()),
                move |this, delta| {
                    let delta = theme::motion::settle(delta);
                    this.transform(Transformation::rotate(percentage(from + (to - from) * delta)))
                },
            )
            .into_any_element(),
        None => glyph.transform(Transformation::rotate(percentage(to))).into_any_element(),
    }
}

/// A collapsible Trace line: the whole row toggles, the chevron follows the
/// line, and while open `content` sits on the rail under it.
pub(crate) fn trace_collapsible(
    line: TraceLine,
    open: bool,
    on_toggle: impl Fn(&mut Window, &mut App) + 'static,
    content: Option<AnyElement>,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let g = line.size.geometry();
    let pending = line.status == Some(TraceStatus::Pending);
    let id = line.id.clone();
    let toggle_id = id.clone();
    let color = tint(line.status, cx);
    let header = h_flex()
        .id(id.clone())
        .group("trace-row")
        .w_full()
        .min_w_0()
        .cursor_pointer()
        .child(line_row(&line, true, window, cx))
        .child(
            div()
                .flex_shrink_0()
                .ml(px(g.chevron_gap))
                .size(px(g.icon))
                .flex()
                .items_center()
                .justify_center()
                .child(chevron(&id, open, line.size, color)),
        )
        .on_click(move |_: &ClickEvent, window, cx| {
            note_toggle(&toggle_id);
            on_toggle(window, cx);
        });
    v_flex()
        .w_full()
        .min_w_0()
        .when(pending, |this| this.opacity(geometry::PENDING_OPACITY))
        .child(header)
        .when_some(content.filter(|_| open), |this, content| {
            this.child(trace_content(content, line.size, cx))
        })
        .into_any_element()
}

/// A collapsible's open CONTENT: `contentTop` under the row, `contentBottom`
/// over what follows, inset `contentInset`/`contentTrailing`, the hairline
/// rail centred under the icon.
pub(crate) fn trace_content(content: AnyElement, size: TraceSize, cx: &App) -> AnyElement {
    let g = size.geometry();
    div()
        .relative()
        .w_full()
        .min_w_0()
        .mt(px(g.content_top))
        .mb(px(g.content_bottom))
        .pl(px(g.content_inset))
        .pr(px(g.content_trailing))
        .text_size(px(g.font_size))
        .line_height(px(g.line_height))
        .text_color(cx.theme().muted_foreground)
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(g.rail_inset))
                .w(px(g.rail_width))
                .bg(cx.theme().border),
        )
        .child(content)
        .into_any_element()
}

/// Lines stacked in ONE Trace, `lineGap` apart.
pub(crate) fn trace_stack(lines: Vec<AnyElement>, size: TraceSize) -> gpui::Div {
    v_flex().w_full().min_w_0().gap(px(size.geometry().line_gap)).children(lines)
}

// ── The shimmer ──────────────────────────────────────────────────────────────

fn hsla_rgba(color: Hsla) -> Rgba {
    Rgba::from(color)
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0., 1.);
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1. / 2.4) - 0.055 }
}

/// sRGB → OKLab.
fn oklab(color: Rgba) -> (f32, f32, f32) {
    let (r, g, b) = (srgb_to_linear(color.r), srgb_to_linear(color.g), srgb_to_linear(color.b));
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    (
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    )
}

/// OKLab → sRGB (clamped).
fn from_oklab(l: f32, a: f32, b: f32, alpha: f32) -> Rgba {
    let l_ = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m_ = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s_ = (l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    Rgba {
        r: linear_to_srgb(4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_),
        g: linear_to_srgb(-1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_),
        b: linear_to_srgb(-0.0041960863 * l_ - 0.7034186147 * m_ + 1.7076147010 * s_),
        a: alpha,
    }
}

/// `motion.shimmer`'s highlight: the text colour at OKLCH lightness
/// max(`highlightLightnessMin`, l + `highlightLightnessBoost`) and alpha +
/// `highlightAlphaBoost` (each clamped to 1).
pub(crate) fn shimmer_highlight(base: Hsla) -> Hsla {
    use theme::motion as m;
    let rgba = hsla_rgba(base);
    let (l, a, b) = oklab(rgba);
    let lightness = (l + m::SHIMMER_LIGHTNESS_BOOST).max(m::SHIMMER_LIGHTNESS_MIN).min(1.);
    let alpha = (rgba.a + m::SHIMMER_ALPHA_BOOST).min(1.);
    Hsla::from(from_oklab(lightness, a, b, alpha))
}

fn mix(from: Rgba, to: Rgba, t: f32) -> Hsla {
    Hsla::from(Rgba {
        r: from.r + (to.r - from.r) * t,
        g: from.g + (to.g - from.g) * t,
        b: from.b + (to.b - from.b) * t,
        a: from.a + (to.a - from.a) * t,
    })
}

/// The highlight's weight at `x` for a sweep at `progress` over a text
/// `width` wide (`motion.shimmer`: stops base @ ±spread, the half mix at
/// ±spread/2, the highlight at the centre — linear between; the centre runs
/// from −spread to width + spread). Pure, for the tests.
pub(crate) fn shimmer_weight(x: f32, width: f32, progress: f32) -> f32 {
    let spread = theme::motion::SHIMMER_SPREAD_PX;
    let centre = -spread + progress * (width + 2. * spread);
    (1. - (x - centre).abs() / spread).clamp(0., 1.)
}

/// The running title: a text-clipped gradient sweep, one colour per glyph
/// (gpui paints no gradient text), looping over `motion.shimmer.durationMs`.
/// gpui has no reduce-motion signal (see `theme::motion`), so it always runs.
pub(crate) fn shimmer_text(
    id: SharedString,
    text: SharedString,
    base: Hsla,
    font_size: f32,
    window: &Window,
) -> AnyElement {
    let font = window.text_style().font();
    let shaped = window.text_system().shape_line(
        text.clone(),
        px(font_size),
        &[TextRun {
            len: text.len(),
            font,
            color: base,
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        None,
    );
    let width = f32::from(shaped.width);
    // (byte range, centre x) per char.
    let glyphs: Rc<Vec<(std::ops::Range<usize>, f32)>> = Rc::new(
        text.char_indices()
            .map(|(start, ch)| {
                let end = start + ch.len_utf8();
                let x = (f32::from(shaped.x_for_index(start)) + f32::from(shaped.x_for_index(end))) / 2.;
                (start..end, x)
            })
            .collect(),
    );
    let from = hsla_rgba(base);
    let to = hsla_rgba(shimmer_highlight(base));
    let duration = theme::motion::SHIMMER;
    let paint = move |progress: f32| {
        StyledText::new(text.clone()).with_highlights(glyphs.iter().filter_map(|(range, x)| {
            let weight = shimmer_weight(*x, width, progress);
            (weight > 0.).then(|| {
                (
                    range.clone(),
                    HighlightStyle {
                        color: Some(mix(from, to, weight)),
                        ..Default::default()
                    },
                )
            })
        }))
    };
    // Reduce motion: the ambient sweep stops (the plain title stays).
    if theme::motion::reduce_motion() {
        return paint(0.).into_any_element();
    }
    paint(0.)
        .with_animation(id, Animation::new(duration).repeat(), move |_, delta| paint(delta))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sweep_runs_from_before_the_text_to_past_it() {
        let spread = theme::motion::SHIMMER_SPREAD_PX;
        // At the start the centre sits a spread left of the text.
        assert_eq!(shimmer_weight(-spread, 100., 0.), 1.);
        assert_eq!(shimmer_weight(0., 100., 0.), 0.);
        // At the end, a spread past its right edge.
        assert_eq!(shimmer_weight(100. + spread, 100., 1.), 1.);
        // Half a spread off the centre is the 50% mix.
        assert!((shimmer_weight(50. + spread / 2., 100., 0.5) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn the_highlight_is_brighter_and_more_opaque() {
        let base = Hsla::from(Rgba { r: 0.63, g: 0.63, b: 0.63, a: 0.5 });
        let high = hsla_rgba(shimmer_highlight(base));
        assert!(high.r > 0.9, "{high:?}");
        assert!((high.a - 0.9).abs() < 1e-4);
        // An OKLab round trip is the identity.
        let back = {
            let rgba = hsla_rgba(base);
            let (l, a, b) = oklab(rgba);
            from_oklab(l, a, b, rgba.a)
        };
        assert!((back.r - 0.63).abs() < 1e-3);
    }

    #[test]
    fn concepts_resolve_and_fall_back() {
        let same = |a: ExpIcon, b: ExpIcon| std::mem::discriminant(&a) == std::mem::discriminant(&b);
        assert!(same(trace_icon("trace-terminal"), registry::TRACE_TERMINAL));
        assert!(same(trace_icon("nope"), registry::TRACE_TOOL));
    }
}
