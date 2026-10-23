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

use exponential_ui_gpui::host::{ActionEvent, HostPlugin};
use exponential_ui_gpui::view::SurfaceView;
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

    fn url_policy(&self) -> Option<exponential_ui::host::UrlPolicy> {
        self.inner.url_policy()
    }

    fn media_options(&self) -> exponential_ui::host::MediaOptions {
        self.inner.media_options()
    }

    fn media_request(&self, src: &str) -> Option<exponential_ui::host::MediaRequest> {
        self.inner.media_request(src)
    }

    fn open_url(&self, url: &str, cx: &mut App) {
        self.inner.open_url(url, cx)
    }

    fn open_media_file(&self, path: &std::path::Path, cx: &mut App) {
        self.inner.open_media_file(path, cx)
    }

    fn pick_files(&self, request: &exponential_ui_gpui::host::FilePickRequest, cx: &mut App) -> bool {
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

/// The frame's look `elapsed` into its entrance: (opacity, rise px, glow
/// 0..1 = the primary hairline's share); `None` = settled.
pub(crate) fn enter_frame(elapsed: Duration) -> Option<(f32, f32, f32)> {
    let enter = &CHATS.surface_card.enter;
    let frame_d = token_duration(&enter.frame.duration);
    let glow_d = token_duration(&enter.glow.duration);
    if elapsed >= frame_d.max(glow_d) {
        return None;
    }
    let t = (elapsed.as_secs_f32() / frame_d.as_secs_f32()).min(1.);
    let e = token_ease(&enter.frame.ease)(t);
    let opacity = enter.frame.from_opacity + (1. - enter.frame.from_opacity) * e;
    let rise = enter.frame.from_translate_y * (1. - e);
    let g = (elapsed.as_secs_f32() / glow_d.as_secs_f32()).min(1.);
    let glow = 1. - token_ease(&enter.glow.ease)(g);
    Some((opacity, rise, glow))
}

/// THE surface card: the title caption (else `copy.preview.surface`) over
/// the surface in its rounded glass frame.
pub(crate) fn surface_row(
    id: &str,
    session_id: &str,
    title: Option<&str>,
    surface: &CardSurface,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    ensure_view(id, session_id, surface, window, cx);
    let cards = cx.default_global::<SurfaceCards>();
    let view = cards.views.get(id).cloned().flatten();
    let arrival = cards.arrivals.get(id).copied();
    let look = arrival.and_then(|at| enter_frame(at.elapsed()));
    if look.is_some() {
        window.request_animation_frame();
    } else if arrival.is_some() {
        cx.default_global::<SurfaceCards>().arrivals.remove(id);
    }
    surface_frame(id, title, view.map(|view| view.into_any_element()), look, cx)
}

/// The card's frame around `body` (`None` = the skeleton-height box until
/// the view exists); `look` = the ENTER state ([`enter_frame`]).
pub(crate) fn surface_frame(
    id: &str,
    title: Option<&str>,
    body: Option<AnyElement>,
    look: Option<(f32, f32, f32)>,
    cx: &App,
) -> AnyElement {
    let card = domain::plan_card::card();
    let stroke = theme::tokens::glass::STROKE_CARD.to_hsla();
    let glow_from = CHATS.surface_card.enter.glow.from_opacity;
    let (opacity, rise, glow) = look.unwrap_or((1., 0., 0.));
    let border = if glow > 0. {
        let primary = cx.theme().primary.opacity(glow_from);
        stroke.blend(primary.opacity(glow * glow_from))
    } else {
        stroke
    };
    let caption = title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or(&CHATS.copy.preview.surface)
        .to_string();
    v_flex()
        .id(SharedString::from(format!("{}-{id}", CHATS.test_ids.surface_card)))
        .relative()
        .top(px(rise))
        .opacity(opacity)
        .w_full()
        .min_w_0()
        .gap(px(card.surface_label_gap))
        .child(
            div()
                .text_2xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(caption)),
        )
        .child(
            div()
                .w_full()
                .min_w_0()
                .p(px(card.surface_padding))
                .rounded(px(card.surface_radius))
                .border_1()
                .border_color(border)
                .bg(theme::tokens::glass::FILL_ROW.to_hsla())
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

/// The `session_self_*` display row of the call that posts a card of this
/// row kind (`plan` → `session_self_plan`, a question → `session_self_ask`,
/// a surface → `session_self_show`); `None` for any other row.
pub(crate) fn posting_call(row: &SessionThreadRow) -> Option<&'static str> {
    match row {
        SessionThreadRow::Plan { .. } => Some("session_self_plan"),
        SessionThreadRow::Question { .. } => Some("session_self_ask"),
        SessionThreadRow::Surface { .. } => Some("session_self_show"),
        _ => None,
    }
}

/// Where a session's conversation cards sit in its relay feed (web
/// `anchorToolCards` / `anchorShowCards`): each card right after the
/// SETTLED call that posted it, paired newest back per call kind; a card
/// with no such call in the feed (the call is above the loaded window, or
/// the card arrived first) trails the transcript (`unanchored`, card order).
/// `calls` = the feed's settled, unfailed Exponential calls in feed order:
/// (feed item id, display row). Returns card INDICES into `cards`.
pub(crate) fn anchor_cards(
    calls: &[(u64, &str)],
    cards: &[SessionThreadRow],
) -> (HashMap<u64, Vec<usize>>, Vec<usize>) {
    let mut anchors: HashMap<u64, Vec<usize>> = HashMap::new();
    let mut unanchored = Vec::new();
    let mut used: HashMap<&str, usize> = HashMap::new();
    for (index, card) in cards.iter().enumerate().rev() {
        let Some(kind) = posting_call(card) else { continue };
        let skip = used.entry(kind).or_default();
        let call = calls.iter().rev().filter(|(_, row)| *row == kind).nth(*skip);
        *skip += 1;
        match call {
            Some((id, _)) => anchors.entry(*id).or_default().insert(0, index),
            None => unanchored.insert(0, index),
        }
    }
    (anchors, unanchored)
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
    /// call left trails the transcript.
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
        let (opacity, rise, glow) = enter_frame(Duration::ZERO).unwrap();
        assert_eq!(opacity, enter.frame.from_opacity);
        assert_eq!(rise, enter.frame.from_translate_y);
        assert_eq!(glow, 1.);
        assert!(enter_frame(Duration::from_secs(5)).is_none());
    }
}
