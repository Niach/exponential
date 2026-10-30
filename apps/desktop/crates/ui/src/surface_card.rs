//! Thread 58cb32df (#EXP-1373, `fixtures/chats.json`): the transcript's two
//! new rows on the IDE (the twins of web `packages/ui` surface-card +
//! transcript-divider, iOS / Android the same):
//!
//! * the DIVIDER ([`divider_row`]): a person's `/new` or `/compact` line =
//!   the centred glass pill `copy.dividers.<kind> · HH:mm` (icon `ui-clean`
//!   / `coding-compact`), never a bubble;
//! * the SURFACE CARD ([`surface_row`]): an agent's inline Exponential UI
//!   preview (`exponential_session_self {op: "show"}`), painted by THE plan
//!   card's surface painter ([`crate::plan_card::card_surface_view`], theme
//!   exponential, dark) in its own rounded glass frame, the column's width.
//!   It answers nothing: a press whose action is the `reply` event with
//!   text posts that text as the person's next message
//!   ([`domain::chats::surface_reply`]); every other event is ignored.
//!
//! ENTER (`surfaceCard.enter`, motion tokens only): a surface card that
//! ARRIVES while its chat is open (its id was not among the rows the screen
//! first synced, [`sync_rows`]) plays once: the frame fades in, rises and
//! settles its hairline from the primary glow to the normal stroke. History,
//! a reopen or a scroll never animates. gpui has no per-node opacity inside
//! a painted surface and no scale on a layout box, so the root's children
//! stagger and the frame's `fromScale` are not drawn here (the frame's fade
//! + rise + glow carry the entrance).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use eui_gpui::host::{ActionEvent, HostPlugin};
use eui_gpui::view::SurfaceView;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, Entity, EntityId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, Styled as _, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use crate::controls::WebText as _;

use domain::chats::{DividerKind, CHATS};
use domain::session_tree::{CardSurface, SessionThreadRow};

use crate::icons::registry;

// ---------------------------------------------------------------------------
// The divider
// ---------------------------------------------------------------------------

/// The divider's words: `<copy> · HH:mm` (the time dropped when unknown).
pub(crate) fn divider_label(kind: DividerKind, time: &str) -> String {
    if time.trim().is_empty() {
        kind.label().to_string()
    } else {
        format!("{} · {}", kind.label(), time.trim())
    }
}

/// THE divider pill, centred in the column.
pub(crate) fn divider_row(id: &str, kind: DividerKind, at: &str, cx: &App) -> AnyElement {
    divider_pill(id, kind, &crate::thread_screen::clock(Some(at)), cx)
}

/// [`divider_row`] with the time already formatted (the styleguide's).
pub(crate) fn divider_pill(id: &str, kind: DividerKind, time: &str, cx: &App) -> AnyElement {
    use crate::surface::{glass_pill, PillMode, PillSize};
    let muted = cx.theme().muted_foreground;
    let icon = registry::concept_by_name(kind.icon()).unwrap_or(registry::UI_INFO);
    h_flex()
        .w_full()
        .justify_center()
        .child(
            glass_pill(
                SharedString::from(format!("{}-{id}", CHATS.test_ids.divider)),
                PillSize::Sm,
                PillMode::Readonly,
                cx,
            )
            .child(Icon::new(icon).with_size(px(PillSize::Sm.glyph())).text_color(muted))
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(divider_label(kind, time))),
            ),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// The surface card
// ---------------------------------------------------------------------------

/// The surface card's host: the plan card's sandbox ([`crate::plan_card::
/// CardSurfaceHost`]: media, urls, no pickers), plus the reply rule.
struct ReplyHost {
    inner: crate::plan_card::CardSurfaceHost,
    session_id: String,
}

impl HostPlugin for ReplyHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        self.inner.icon(name)
    }

    fn on_action(&self, event: &ActionEvent, cx: &mut App) {
        if let Some(text) = domain::chats::surface_reply(Some(&event.name), Some(&event.context)) {
            crate::chat_actions::send_surface_reply(&self.session_id, &text, cx);
        }
    }

    fn url_policy(&self) -> Option<eui_core::host::UrlPolicy> {
        self.inner.url_policy()
    }

    fn media_options(&self) -> eui_core::host::MediaOptions {
        self.inner.media_options()
    }

    fn media_request(&self, src: &str) -> Option<eui_core::host::MediaRequest> {
        self.inner.media_request(src)
    }

    fn open_url(&self, url: &str, cx: &mut App) {
        self.inner.open_url(url, cx)
    }

    fn open_media_file(&self, path: &std::path::Path, cx: &mut App) {
        self.inner.open_media_file(path, cx)
    }

    fn pick_files(&self, request: &eui_gpui::host::FilePickRequest, cx: &mut App) -> bool {
        self.inner.pick_files(request, cx)
    }
}

/// The surface views by message id (built once, on the next frame) and the
/// live arrivals' start instants.
#[derive(Default)]
struct SurfaceCards {
    views: HashMap<String, Option<Entity<SurfaceView>>>,
    building: HashSet<String>,
    /// Per screen view: the surface ids it has synced (first sync = history).
    seen: HashMap<EntityId, HashSet<String>>,
    arrivals: HashMap<String, Instant>,
    /// The arrivals whose children stagger was handed to their view.
    staggered: HashSet<String>,
}

impl gpui::Global for SurfaceCards {}

/// A transcript host calls this whenever its rows change: the FIRST call for
/// `screen` with rows records what is already there (history never animates); a
/// surface row that appears later is a live arrival and plays ENTER once.
pub(crate) fn sync_rows(screen: EntityId, rows: &[SessionThreadRow], cx: &mut App) {
    let ids: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            SessionThreadRow::Surface { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    let cards = cx.default_global::<SurfaceCards>();
    let first = !cards.seen.contains_key(&screen);
    // The rows still syncing: the first NON-EMPTY sync is the history.
    if first && rows.is_empty() {
        return;
    }
    let seen = cards.seen.entry(screen).or_default();
    let now = Instant::now();
    let mut fresh = Vec::new();
    for id in ids {
        if seen.insert(id.to_string()) && !first {
            fresh.push(id.to_string());
        }
    }
    for id in fresh {
        cards.arrivals.entry(id).or_insert(now);
    }
}

/// Build the view for `id` on the next frame (once).
fn ensure_view(id: &str, session_id: &str, surface: &CardSurface, window: &Window, cx: &mut App) {
    let cards = cx.default_global::<SurfaceCards>();
    if cards.views.contains_key(id) || !cards.building.insert(id.to_string()) {
        return;
    }
    let (id, session_id, surface) = (id.to_string(), session_id.to_string(), surface.clone());
    window.defer(cx, move |window, cx| {
        let host = Rc::new(ReplyHost {
            inner: crate::plan_card::CardSurfaceHost::new(cx),
            session_id,
        });
        let view = crate::plan_card::card_surface_view(&format!("surface-card-{id}"), &surface, host, window, cx);
        let cards = cx.default_global::<SurfaceCards>();
        cards.building.remove(&id);
        cards.views.insert(id, view);
        cx.refresh_windows();
    });
}

/// The ENTER spec's timings off the motion tokens.
fn token_duration(token: &str) -> Duration {
    match token {
        "fast" => theme::motion::FAST,
        "slow" => theme::motion::SLOW,
        _ => theme::motion::STANDARD,
    }
}

fn token_ease(token: &str) -> Box<dyn Fn(f32) -> f32> {
    match token {
        "standard" => Box::new(theme::motion::standard()),
        "accelerate" => Box::new(theme::motion::accelerate()),
        _ => Box::new(theme::motion::decelerate()),
    }
}

/// The frame's look mid-entrance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EnterLook {
    pub opacity: f32,
    /// px below its place.
    pub rise: f32,
    /// 0..1: the primary hairline's share.
    pub glow: f32,
    /// The frame's scale (gpui scales no box: the frame narrows about its
    /// centre to this share of the column).
    pub scale: f32,
}

/// The frame's look `elapsed` into its entrance (`surfaceCard.enter.frame`
/// + `glow`); `None` = settled. `reduced` (Reduce motion): a plain opacity
/// fade of the frame, no rise, scale or glow.
pub(crate) fn enter_frame(elapsed: Duration, reduced: bool) -> Option<EnterLook> {
    let enter = &CHATS.surface_card.enter;
    let frame_d = token_duration(&enter.frame.duration);
    let glow_d = if reduced { frame_d } else { token_duration(&enter.glow.duration) };
    if elapsed >= frame_d.max(glow_d) {
        return None;
    }
    let t = (elapsed.as_secs_f32() / frame_d.as_secs_f32()).min(1.);
    let e = token_ease(&enter.frame.ease)(t);
    let opacity = enter.frame.from_opacity + (1. - enter.frame.from_opacity) * e;
    if reduced {
        return Some(EnterLook { opacity, rise: 0., glow: 0., scale: 1. });
    }
    let rise = enter.frame.from_translate_y * (1. - e);
    let scale = enter.frame.from_scale + (1. - enter.frame.from_scale) * e;
    let g = (elapsed.as_secs_f32() / glow_d.as_secs_f32()).min(1.);
    let glow = 1. - token_ease(&enter.glow.ease)(g);
    Some(EnterLook { opacity, rise, glow, scale })
}

/// The surface ROOT's first-level children's entrance
/// (`surfaceCard.enter.children`): one by one after `startAfterMs`,
/// `staggerMs` apart (past `maxStaggered` they share the last delay), each
/// an opacity + `fromTranslateY` rise over its token duration. `None` when
/// the root has no children.
pub(crate) fn children_entrance(started: Instant, surface: &CardSurface) -> Option<eui_gpui::view::NodeEntrance> {
    let enter = &CHATS.surface_card.enter;
    let ids: Vec<String> = surface
        .root
        .get("children")?
        .as_array()?
        .iter()
        .filter_map(|child| child.get("id")?.as_str().map(str::to_string))
        .collect();
    if ids.is_empty() {
        return None;
    }
    let delays_ms = (0..ids.len()).map(|index| enter.child_delay_ms(index) as f32).collect();
    let easing = match enter.children.ease.as_str() {
        "standard" => theme::tokens::motion::ease::STANDARD,
        "accelerate" => theme::tokens::motion::ease::ACCELERATE,
        _ => theme::tokens::motion::ease::DECELERATE,
    };
    Some(eui_gpui::view::NodeEntrance {
        started,
        ids,
        from_opacity: enter.children.from_opacity,
        from_translate_y: enter.children.from_translate_y,
        duration_ms: token_duration(&enter.children.duration).as_secs_f32() * 1000.,
        easing,
        delays_ms,
    })
}

/// THE surface card: the title caption (none when untitled) over the
/// surface in its rounded glass frame.
pub(crate) fn surface_row(
    id: &str,
    session_id: &str,
    title: Option<&str>,
    surface: &CardSurface,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    ensure_view(id, session_id, surface, window, cx);
    let reduced = theme::motion::reduce_motion();
    let cards = cx.default_global::<SurfaceCards>();
    let view = cards.views.get(id).cloned().flatten();
    let arrival = cards.arrivals.get(id).copied();
    // The root's children stagger in once the view exists (never under
    // Reduce motion), timed from the arrival like the frame.
    if let (Some(at), Some(view), false) = (arrival, view.as_ref(), reduced) {
        if cx.default_global::<SurfaceCards>().staggered.insert(id.to_string()) {
            let entrance = children_entrance(at, surface);
            view.update(cx, |view, cx| view.set_entrance(entrance, cx));
        }
    }
    let look = arrival.and_then(|at| enter_frame(at.elapsed(), reduced));
    if look.is_some() {
        window.request_animation_frame();
    } else if arrival.is_some() {
        cx.default_global::<SurfaceCards>().arrivals.remove(id);
    }
    surface_frame(id, title, view.map(|view| view.into_any_element()), look, cx)
}

/// #EXP-1432: THE surface renderer with NO card chrome around it (a story
/// viewer's `surface` frame): the same view [`surface_row`] frames, bare;
/// a 48px box until it exists.
pub(crate) fn surface_bare(id: &str, session_id: &str, surface: &CardSurface, window: &Window, cx: &mut App) -> AnyElement {
    ensure_view(id, session_id, surface, window, cx);
    let view = cx.default_global::<SurfaceCards>().views.get(id).cloned().flatten();
    match view {
        Some(view) => div().w_full().min_w_0().child(view).into_any_element(),
        None => div().w_full().min_h(px(48.)).into_any_element(),
    }
}

/// The card's frame around `body` (`None` = the skeleton-height box until
/// the view exists); `look` = the ENTER state ([`enter_frame`]).
pub(crate) fn surface_frame(
    id: &str,
    title: Option<&str>,
    body: Option<AnyElement>,
    look: Option<EnterLook>,
    cx: &App,
) -> AnyElement {
    let card = domain::plan_card::card();
    let stroke = theme::tokens::glass::STROKE_CARD.to_hsla();
    let glow_from = CHATS.surface_card.enter.glow.from_opacity;
    let EnterLook { opacity, rise, glow, scale } = look.unwrap_or(EnterLook { opacity: 1., rise: 0., glow: 0., scale: 1. });
    let border = if glow > 0. {
        let primary = cx.theme().primary.opacity(glow_from);
        stroke.blend(primary.opacity(glow * glow_from))
    } else {
        stroke
    };
    // The title = the plan card's surface caption (plan-card.json `card`:
    // 2xs medium muted, `surfaceLabelGap` ABOVE the frame) ×4.
    let caption = title.map(str::trim).filter(|title| !title.is_empty()).map(str::to_string);
    v_flex()
        .id(SharedString::from(format!("{}-{id}", CHATS.test_ids.surface_card)))
        .relative()
        .top(px(rise))
        .mx_auto()
        .w(gpui::relative(scale))
        .opacity(opacity)
        .min_w_0()
        .gap(px(card.surface_label_gap))
        .when_some(caption, |card, caption| {
            card.child(
                div()
                    .text_2xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(cx.theme().muted_foreground)
                    .truncate()
                    .child(SharedString::from(caption)),
            )
        })
        .child(
            div()
                .w_full()
                .min_w_0()
                .p(px(card.surface_padding))
                .rounded(px(card.surface_radius))
                .border_1()
                .border_color(border)
                // THE card surface (N-12 ONE gray ×4), the bubbles' and cards' fill.
                .bg(crate::dock::card_fill())
                .overflow_hidden()
                .map(|frame| match body {
                    Some(body) => frame.child(body),
                    None => frame.min_h(px(48.)),
                }),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// The single-session feed: the conversation's cards beside the relay rows
// ---------------------------------------------------------------------------

/// A session card row's kind (`None` for any other row).
fn card_kind(row: &SessionThreadRow) -> Option<domain::ask_dock::SessionCardKind> {
    use domain::ask_dock::SessionCardKind;
    match row {
        SessionThreadRow::Plan { .. } => Some(SessionCardKind::Plan),
        SessionThreadRow::Question { .. } => Some(SessionCardKind::Question),
        SessionThreadRow::Surface { .. } => Some(SessionCardKind::Surface),
        _ => None,
    }
}

/// Where a session's conversation cards sit in its relay feed: THE rule ×4
/// (`domain::ask_dock::anchor_session_cards`, ask-dock.json `anchorCases`):
/// each card right after the SETTLED call that posted it, paired newest back
/// per call kind; a card with no such call in the feed is `unanchored` (card
/// order), painted at the transcript's HEAD before its first row.
/// `calls` = the feed's settled, unfailed Exponential calls in feed order:
/// (feed item id, display row). Returns card INDICES into `cards`.
pub(crate) fn anchor_cards(
    calls: &[(u64, &str)],
    cards: &[SessionThreadRow],
) -> (HashMap<u64, Vec<usize>>, Vec<usize>) {
    let (indices, kinds): (Vec<usize>, Vec<_>) =
        cards.iter().enumerate().filter_map(|(index, card)| card_kind(card).map(|kind| (index, kind))).unzip();
    let (anchors, unanchored) = domain::ask_dock::anchor_session_cards(calls, &kinds);
    let back = |list: Vec<usize>| list.into_iter().map(|i| indices[i]).collect::<Vec<_>>();
    (anchors.into_iter().map(|(call, list)| (call, back(list))).collect(), back(unanchored))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface(id: &str) -> SessionThreadRow {
        SessionThreadRow::Surface {
            id: id.into(),
            session_id: "s".into(),
            title: None,
            surface: CardSurface { root: serde_json::json!({"id": "root"}), data: None },
        }
    }

    fn plan(id: &str) -> SessionThreadRow {
        SessionThreadRow::Plan {
            id: id.into(),
            session_id: "s".into(),
            plan: "p".into(),
            questions: Vec::new(),
            surface: None,
            decision: None,
            fresh: false,
            answers: None,
            feedback: None,
            surface_data: None,
        }
    }

    /// Cards follow their posting calls newest back, per kind; one with no
    /// call left is unanchored (the head, before the first row).
    #[test]
    fn cards_anchor_to_their_calls_newest_back() {
        let calls = [(4, "session_self_show"), (7, "session_self_plan"), (9, "session_self_show")];
        let cards = [surface("a"), surface("b"), surface("c"), plan("p")];
        let (anchors, unanchored) = anchor_cards(&calls, &cards);
        assert_eq!(anchors.get(&9), Some(&vec![2]));
        assert_eq!(anchors.get(&4), Some(&vec![1]));
        assert_eq!(anchors.get(&7), Some(&vec![3]));
        assert_eq!(unanchored, vec![0]);
        let (anchors, unanchored) = anchor_cards(&[], &cards);
        assert!(anchors.is_empty());
        assert_eq!(unanchored, vec![0, 1, 2, 3]);
    }

    #[test]
    fn the_divider_reads_the_copy_and_the_time() {
        assert_eq!(divider_label(DividerKind::New, "09:41"), "New conversation · 09:41");
        assert_eq!(divider_label(DividerKind::Compact, ""), "Conversation compacted");
    }

    /// ENTER starts at the fixture's `from` values and settles.
    #[test]
    fn the_enter_frame_starts_from_the_spec_and_settles() {
        let enter = &CHATS.surface_card.enter;
        let look = enter_frame(Duration::ZERO, false).unwrap();
        assert_eq!(look.opacity, enter.frame.from_opacity);
        assert_eq!(look.rise, enter.frame.from_translate_y);
        assert_eq!(look.scale, enter.frame.from_scale);
        assert_eq!(look.glow, 1.);
        assert!(enter_frame(Duration::from_secs(5), false).is_none());
        // Reduce motion: an opacity fade only.
        let reduced = enter_frame(Duration::ZERO, true).unwrap();
        assert_eq!((reduced.rise, reduced.glow, reduced.scale), (0., 0., 1.));
        assert_eq!(reduced.opacity, enter.frame.from_opacity);
    }

    /// The root's first-level children stagger: `startAfterMs`, then
    /// `staggerMs` apart, the last delay shared past `maxStaggered`.
    #[test]
    fn the_children_stagger_after_the_frame() {
        let enter = &CHATS.surface_card.enter;
        let children: Vec<serde_json::Value> = (0..10).map(|i| serde_json::json!({"id": format!("c{i}")})).collect();
        let surface = CardSurface { root: serde_json::json!({"id": "root", "children": children}), data: None };
        let entrance = children_entrance(Instant::now(), &surface).unwrap();
        assert_eq!(entrance.ids.len(), 10);
        assert_eq!(entrance.delays_ms[0], enter.children.start_after_ms as f32);
        assert_eq!(entrance.delays_ms[1] - entrance.delays_ms[0], enter.children.stagger_ms as f32);
        assert_eq!(entrance.delays_ms[9], entrance.delays_ms[enter.children.max_staggered - 1]);
        assert_eq!(entrance.from_translate_y, enter.children.from_translate_y);
        let leaf = CardSurface { root: serde_json::json!({"id": "root"}), data: None };
        assert!(children_entrance(Instant::now(), &leaf).is_none());
    }
}
