//! #FEED-191 NEEDS YOU (`fixtures/needs-you.json`, its `$comment` is the spec;
//! web `packages/ui/src/needs-you.tsx` + `hooks/use-needs-you.ts`, iOS ExpCore
//! NeedsYou.swift, Android domain/NeedsYou.kt): every plan and question waiting
//! on the viewer and (#EXP-1699) every PR ready to merge. The model is
//! `domain::needs_you::needs_you` over the team's conversation DIGESTS (never a
//! transcript: EXP-1380), sessions and pull requests; the rail shows the entry
//! while a row exists, [`NeedsYouView`] the one reading column in the content
//! card. A press opens the chat by the row's `opens`.

use std::rc::Rc;

use gpui::{
    div, px, AnyElement, App, ClickEvent, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};
use sync::Store;

use domain::needs_you::{
    needs_you, NeedsYouInput, NeedsYouKind, NeedsYouMessageInput, NeedsYouOpens, NeedsYouPullRequestInput, NeedsYouRow,
    NeedsYouSessionInput, NEEDS_YOU,
};
use domain::waiting_cards::WAITING_CARDS;

use crate::icons::{registry, ExpIcon};
use crate::navigation::{active_team_id, nav_for_window, Navigation};

/// The page's reading column (the Chats page's, web `max-w-3xl`).
const COLUMN_W: f32 = 768.;

fn is_waiting_kind(card: Option<&serde_json::Value>) -> bool {
    matches!(card.and_then(|card| card.get("kind")).and_then(|kind| kind.as_str()), Some("plan" | "question" | "ask"))
}

/// THE fold's input off the store.
fn input(team_id: &str, now: i64, cx: &App) -> NeedsYouInput {
    let me = crate::queries::active_account(cx).map(|account| account.user_id);
    let store = Store::global(cx);
    let collections = store.collections();
    let sessions = collections.sessions.read(cx);
    let agents = collections.agents.read(cx);
    let team: Vec<NeedsYouSessionInput> = sessions
        .iter()
        .filter(|session| session.team_id.as_deref() == Some(team_id))
        .map(|session| NeedsYouSessionInput {
            id: session.id.clone(),
            root_id: session.root_id_or_self().to_string(),
            // An agent conversation = the agent's name, else THE session title.
            name: session
                .agent_id
                .as_deref()
                .and_then(|agent_id| agents.get(agent_id))
                .and_then(|agent| agent.name.clone())
                .or_else(|| crate::session_data::session_title_of(&session.id, cx))
                .unwrap_or_default(),
            status: session.status.clone().unwrap_or_default(),
            agent_id: session.agent_id.clone(),
            user_id: session.user_id.clone(),
            busy: session.busy,
        })
        .collect();
    let ids: std::collections::HashSet<&str> = team.iter().map(|session| session.id.as_str()).collect();
    let messages: Vec<NeedsYouMessageInput> = crate::session_data::digest_messages(cx, |owner| ids.contains(owner))
        .into_iter()
        .filter(|message| is_waiting_kind(message.card.as_ref()))
        .map(|message| NeedsYouMessageInput {
            id: message.id,
            session_id: message.session_id,
            author_session_id: message.author_session_id,
            body: message.body.unwrap_or_default(),
            created_at: message.created_at.unwrap_or_default(),
            answered_at: message.answered_at,
            card: message.card,
        })
        .collect();
    let pull_requests: Vec<NeedsYouPullRequestInput> = collections
        .pull_requests
        .read(cx)
        .iter()
        .filter(|pull| pull.team_id.as_deref() == Some(team_id))
        .map(|pull| NeedsYouPullRequestInput {
            id: pull.id.clone(),
            root_session_id: pull.root_session_id.clone(),
            number: pull.number.unwrap_or_default(),
            title: pull.title.clone().unwrap_or_default(),
            state: pull.state.clone().unwrap_or_default(),
            checks_state: pull.checks_state.clone(),
            base_pull_request_id: pull.base_pull_request_id.clone(),
            created_at: pull.created_at.clone().unwrap_or_default(),
        })
        .collect();
    NeedsYouInput { now, me, sessions: team, messages, pull_requests }
}

#[derive(Clone, PartialEq, Eq)]
struct RowsKey {
    team_id: String,
    revisions: [u64; 4],
    /// The ages move with the minute.
    minute: i64,
}

#[derive(Default)]
struct RowsMemo(Option<(RowsKey, Rc<Vec<NeedsYouRow>>)>);

impl gpui::Global for RowsMemo {}

/// THE team's rows, memoised per store change and minute: the rail asks every
/// frame.
pub(crate) fn rows_in_team(team_id: &str, cx: &mut App) -> Rc<Vec<NeedsYouRow>> {
    let Some(store) = Store::try_global(cx) else {
        return Rc::new(Vec::new());
    };
    let collections = store.collections();
    let now = crate::session_data::now_ms();
    let key = RowsKey {
        team_id: team_id.to_string(),
        revisions: [
            collections.sessions.read(cx).revision(),
            collections.conversations.read(cx).revision(),
            collections.pull_requests.read(cx).revision(),
            collections.agents.read(cx).revision(),
        ],
        minute: now / 60_000,
    };
    if let Some((cached, rows)) = &cx.default_global::<RowsMemo>().0 {
        if *cached == key {
            return rows.clone();
        }
    }
    let rows = Rc::new(needs_you(&input(team_id, now, cx)));
    cx.global_mut::<RowsMemo>().0 = Some((key, rows.clone()));
    rows
}

/// A press opens the chat by `opens` (needs-you.json): a plan = its Plan tab on
/// that plan, a card = the Conversation scrolled to it (THE waiting chip's way),
/// results = the Results tab (a PR's block), chat = the agent's chat.
pub(crate) fn open_row(row: &NeedsYouRow, window: &mut Window, cx: &mut App) {
    match row.opens {
        NeedsYouOpens::Plan => {
            crate::plan_page::open(&row.session_id, &row.message_id, window, cx);
            return;
        }
        NeedsYouOpens::Card => crate::thread_screen::request_thread_card(&row.root_id, &row.message_id, cx),
        NeedsYouOpens::Results => crate::thread_screen::request_thread_results(&row.root_id, cx),
        NeedsYouOpens::Chat => {}
    }
    let screen = crate::navigation::session_screen_for(&row.root_id, cx);
    crate::navigation::navigate(window, cx, screen);
}

/// The leading glyph (waiting-cards.json `icons`; a PR's `pr-open`).
pub(crate) fn row_glyph(kind: NeedsYouKind) -> ExpIcon {
    let concept = match kind {
        NeedsYouKind::Plan => WAITING_CARDS.icons.plan.as_str(),
        NeedsYouKind::Question => WAITING_CARDS.icons.question.as_str(),
        NeedsYouKind::Pr => "pr-open",
    };
    registry::concept_by_name(concept).unwrap_or(registry::NAV_NEEDS_YOU)
}

/// Line 2: `chat`, then ` · meta` when set.
pub(crate) fn second_line(row: &NeedsYouRow) -> String {
    if row.meta.is_empty() {
        row.chat.clone()
    } else {
        format!("{} · {}", row.chat, row.meta)
    }
}

/// THE row: THE inbox page's reading row (`list` density): the glyph in the
/// needs-you tone hanging from the top, line 1 the title (medium, truncated) +
/// the age muted trailing, line 2 the chat, muted, truncated. The caller hangs
/// the press.
pub(crate) fn needs_you_row(row: &NeedsYouRow, cx: &App) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    crate::surface::flat_row()
        .id(SharedString::from(format!("{}-{}", NEEDS_YOU.test_ids.row, row.message_id)))
        .flex()
        .w_full()
        .min_w_0()
        .items_start()
        .gap_3()
        .px_3()
        .py_2()
        .child(div().flex_shrink_0().mt(px(2.)).child(Icon::new(row_glyph(row.kind)).with_size(px(16.)).text_color(theme::tokens::YELLOW.to_hsla())))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.foreground)
                                .child(SharedString::from(row.title.clone())),
                        )
                        .child(div().flex_shrink_0().pl_2().text_xs().text_color(theme.muted_foreground).child(SharedString::from(row.age.clone()))),
                )
                .child(div().mt(px(2.)).truncate().text_xs().text_color(theme.muted_foreground).child(SharedString::from(second_line(row)))),
        )
}

/// THE list: one reading column of rows, else THE EmptyState.
pub(crate) fn needs_you_list(rows: &[NeedsYouRow], cx: &App) -> AnyElement {
    if rows.is_empty() {
        return crate::controls::empty_state(Icon::new(registry::NAV_NEEDS_YOU), NEEDS_YOU.copy.empty.clone(), None, cx).into_any_element();
    }
    let hover = cx.theme().list_hover;
    v_flex()
        .id(SharedString::from(NEEDS_YOU.test_ids.list.clone()))
        .w_full()
        .min_w_0()
        .children(rows.iter().map(|row| {
            let open = row.clone();
            needs_you_row(row, cx)
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| open_row(&open, window, cx))
                .into_any_element()
        }))
        .into_any_element()
}

/// THE page in the content card (no header, wide).
pub struct NeedsYouView {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl NeedsYouView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        subscriptions.extend(crate::session_data::observe_session_collections(cx));
        Self { nav, scroll: ScrollHandle::new(), _subscriptions: subscriptions }
    }
}

impl Render for NeedsYouView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let ready = Store::try_global(cx).is_some_and(|store| store.collections().sessions.read(cx).is_ready());
        let column: AnyElement = if ready {
            let rows = active_team_id(&self.nav, cx).map(|team_id| rows_in_team(&team_id, cx)).unwrap_or_default();
            needs_you_list(&rows, cx)
        } else {
            // Skeletons, never "Loading…" (feedback-states.json).
            crate::feedback::skeleton_rows("needs-you-skeleton", crate::feedback::SKELETON_ROWS, crate::settings::SETTINGS_ROW_H, cx).into_any_element()
        };
        v_flex()
            .size_full()
            .child(crate::scroll_pane::page_scaffold_with("needs-you-scroll", &self.scroll, v_flex().min_w_0().child(column), COLUMN_W))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: NeedsYouKind, meta: &str) -> NeedsYouRow {
        NeedsYouRow {
            message_id: "m".into(),
            session_id: "s".into(),
            root_id: "r".into(),
            kind,
            opens: NeedsYouOpens::Card,
            title: "t".into(),
            chat: "Chat".into(),
            meta: meta.into(),
            age: "1m".into(),
            created_at: "2026-11-05T00:00:00.000Z".into(),
        }
    }

    /// Line 2 = the chat, then ` · meta` when set (a PR's number).
    #[test]
    fn the_second_line_adds_the_meta() {
        assert_eq!(second_line(&row(NeedsYouKind::Question, "")), "Chat");
        assert_eq!(second_line(&row(NeedsYouKind::Pr, "#12")), "Chat · #12");
    }

    /// waiting-cards.json `icons`, a PR's `pr-open`.
    #[test]
    fn each_kind_wears_its_glyph() {
        assert!(matches!(row_glyph(NeedsYouKind::Plan), ExpIcon::ClipboardList));
        assert!(matches!(row_glyph(NeedsYouKind::Question), ExpIcon::CircleQuestionMark));
        assert!(matches!(row_glyph(NeedsYouKind::Pr), ExpIcon::GitPullRequest));
    }

    #[test]
    fn only_waiting_kinds_feed_the_fold() {
        assert!(is_waiting_kind(Some(&serde_json::json!({ "kind": "plan" }))));
        assert!(is_waiting_kind(Some(&serde_json::json!({ "kind": "ask" }))));
        assert!(!is_waiting_kind(Some(&serde_json::json!({ "kind": "surface" }))));
        assert!(!is_waiting_kind(None));
    }
}
