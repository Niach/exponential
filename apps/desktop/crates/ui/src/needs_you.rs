//! #FEED-191 NEEDS YOU (`fixtures/needs-you.json`, its `$comment` is the spec;
//! web `packages/ui/src/needs-you.tsx` + `hooks/use-needs-you.ts`, iOS ExpCore
//! NeedsYou.swift, Android domain/NeedsYou.kt): every plan and question waiting
//! on the viewer and (#EXP-1699) every PR ready to merge. The model is
//! `domain::needs_you::needs_you` over the team's conversation DIGESTS (never a
//! transcript: EXP-1380), sessions and pull requests; the rail shows the entry
//! while a row exists, [`NeedsYouView`] the one reading column in the content
//! card. A press opens the chat by the row's `opens`; an `approval`-mode ready
//! PR row's press is THE status pill's Needs approval confirm (#EXP-1710).
//!
//! #EXP-1710 (merge-train.json `stuckRow`, web `withStuckRows`): a PR whose
//! auto-merge is stuck (held by the limits, or a fix waiting 2 h+ on the
//! opener's offline device) is ONE `pr` row `Auto-merge stuck on #N`, meta =
//! the reason, with THE Button sm row (Retry, Merge now for owners, Close PR)
//! under it; it replaces that PR's ready row and sorts with the PR rows,
//! newest first, under the decisions.

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    div, px, AnyElement, App, ClickEvent, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window,
};
use gpui_component::{button::Button, h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};
use sync::Store;

use domain::needs_you::{
    needs_you, NeedsYouInput, NeedsYouKind, NeedsYouMessageInput, NeedsYouOpens, NeedsYouPullRequestInput, NeedsYouRow,
    NeedsYouSessionInput, NEEDS_YOU,
};
use domain::merge_train::{StuckAction, MERGE_TRAIN};
use domain::waiting_cards::WAITING_CARDS;

use crate::controls::WebControl as _;
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
    // #EXP-1714: plus the viewer's personal-team sessions (their global
    // Assistant's conversation).
    let team_ids = crate::session_data::synced_team_ids(cx);
    let team: Vec<NeedsYouSessionInput> = sessions
        .iter()
        .filter(|session| crate::session_data::in_team_or_personal(session, team_id, &team_ids, me.as_deref()))
        .map(|session| NeedsYouSessionInput {
            id: session.id.clone(),
            root_id: session.root_id_or_self().to_string(),
            // An agent conversation = the agent's name, else THE session title.
            // #PERF-9: only a ROOT's name is ever shown (a row's `chat`): a
            // boot titled every session of the team (1.6k) on the main thread.
            name: if session.root_id_or_self() != session.id {
                String::new()
            } else {
                session
                    .agent_id
                    .as_deref()
                    .and_then(|agent_id| agents.get(agent_id))
                    .and_then(|agent| agent.name.clone())
                    .or_else(|| crate::session_data::session_title_of(&session.id, cx))
                    .unwrap_or_default()
            },
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
    let merge_mode = collections.teams.read(cx).get(team_id).map(|row| row.merge_mode().to_string());
    NeedsYouInput { now, me, sessions: team, messages, pull_requests, merge_mode }
}

/// A stuck PR's row: its PR and its actions (merge-train.json `stuckRow`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StuckPr {
    pub pull_request_id: String,
    pub actions: Vec<StuckAction>,
}

/// THE team's Needs you rows; `stuck` = the stuck PRs' actions by row id
/// (the PR's id).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NeedsYouRows {
    pub rows: Vec<NeedsYouRow>,
    pub stuck: HashMap<String, StuckPr>,
}

/// The team's stuck PRs (merge-train `stuckRow`): a live PR of a chat
/// whose opener (`pull_requests.session_id`, else the root) is held by the
/// limits, or waits on a fix while its device is offline (that devices row's
/// `last_seen_at`).
fn stuck_rows(team_id: &str, input: &NeedsYouInput, owner: bool, cx: &App) -> Vec<(NeedsYouRow, StuckPr)> {
    let store = Store::global(cx);
    let collections = store.collections();
    let sessions = collections.sessions.read(cx);
    let devices = collections.devices.read(cx);
    let names: HashMap<&str, &str> = input.sessions.iter().map(|session| (session.id.as_str(), session.name.as_str())).collect();
    let mut rows = Vec::new();
    for pull in collections.pull_requests.read(cx).iter() {
        if pull.team_id.as_deref() != Some(team_id) || !(pull.is_open() || pull.state.as_deref() == Some(domain::contract::PR_STATE_DRAFT)) {
            continue;
        }
        let Some(root_id) = pull.root_session_id.as_deref() else { continue };
        let opener = sessions.get(pull.session_id.as_deref().unwrap_or(root_id));
        let offline_since = opener_offline_since(pull, opener, devices.iter(), input.now);
        let Some(view) = domain::merge_train::stuck_row(
            pull.number.unwrap_or_default(),
            domain::merge_train::pr_merge_state(pull),
            pull.merge_hold.as_deref(),
            offline_since,
            input.now,
            owner,
        ) else {
            continue;
        };
        let created_at = pull.created_at.clone().unwrap_or_default();
        let created_ms = domain::session_tree::iso_ms(&created_at).unwrap_or(0);
        rows.push((
            NeedsYouRow {
                message_id: pull.id.clone(),
                session_id: root_id.to_string(),
                root_id: root_id.to_string(),
                kind: NeedsYouKind::Pr,
                opens: NeedsYouOpens::Results,
                title: view.title,
                chat: names.get(root_id).map(|name| name.to_string()).unwrap_or_default(),
                meta: view.meta,
                age: domain::results_tab::result_age(created_ms, input.now),
                created_at,
            },
            StuckPr { pull_request_id: pull.id.clone(), actions: view.actions },
        ));
    }
    rows
}

/// merge-train `offlineSince` off the store's rows: the opener (`opener`,
/// the PR's `session_id` session, else its root) without a device = None;
/// its device online (THE presence: a heartbeat inside the online window) =
/// None; else that row's `last_seen_at`, and an UNKNOWN last-seen (no row
/// synced, or `last_seen_at` null) = the PR's `updated_at` (else its
/// `created_at`): offline since the PR went conflict/failing.
pub(crate) fn opener_offline_since<'a>(
    pull: &domain::rows::PullRequestRow,
    opener: Option<&domain::rows::SessionRow>,
    devices: impl Iterator<Item = &'a domain::rows::DeviceRow>,
    now: i64,
) -> Option<i64> {
    let has_device = opener.and_then(|opener| opener.device_id.as_deref()).is_some_and(|id| !id.is_empty());
    let device = opener.and_then(|opener| crate::queries::session_device_row(opener, devices)).map(|row| {
        domain::merge_train::OfflineDevice {
            online: crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now),
            last_seen_at_ms: row.last_seen_at.as_deref().and_then(crate::comments::parse_epoch).map(|seconds| seconds * 1_000),
        }
    });
    let updated_at = pull.updated_at.as_deref().or(pull.created_at.as_deref()).and_then(crate::comments::parse_epoch)?;
    domain::merge_train::offline_since(has_device, device, updated_at * 1_000)
}

/// Fold the stuck PRs into `needs_you()`'s rows (web `withStuckRows`): a
/// stuck PR replaces its ready-to-merge row and sorts with the PR rows
/// (newest first) under every decision.
pub(crate) fn with_stuck_rows(base: Vec<NeedsYouRow>, stuck: Vec<(NeedsYouRow, StuckPr)>) -> NeedsYouRows {
    if stuck.is_empty() {
        return NeedsYouRows { rows: base, stuck: HashMap::new() };
    }
    let ids: std::collections::HashSet<String> = stuck.iter().map(|(row, _)| row.message_id.clone()).collect();
    let (decisions, ready): (Vec<NeedsYouRow>, Vec<NeedsYouRow>) =
        base.into_iter().partition(|row| row.kind != NeedsYouKind::Pr);
    let mut actions = HashMap::new();
    let mut prs: Vec<NeedsYouRow> = ready.into_iter().filter(|row| !ids.contains(&row.message_id)).collect();
    for (row, pr) in stuck {
        actions.insert(row.message_id.clone(), pr);
        prs.push(row);
    }
    let ms = |row: &NeedsYouRow| domain::session_tree::iso_ms(&row.created_at).unwrap_or(0);
    prs.sort_by(|a, b| ms(b).cmp(&ms(a)));
    let mut rows = decisions;
    rows.extend(prs);
    NeedsYouRows { rows, stuck: actions }
}

#[derive(Clone, PartialEq, Eq)]
struct RowsKey {
    team_id: String,
    revisions: [u64; 5],
    owner: bool,
    /// The ages move with the minute.
    minute: i64,
}

#[derive(Default)]
struct RowsMemo(Option<(RowsKey, Rc<NeedsYouRows>)>);

impl gpui::Global for RowsMemo {}

/// THE team's rows, memoised per store change and minute: the rail asks every
/// frame.
pub(crate) fn rows_in_team(team_id: &str, cx: &mut App) -> Rc<NeedsYouRows> {
    let Some(store) = Store::try_global(cx) else {
        return Rc::new(NeedsYouRows::default());
    };
    let owner = crate::settings::is_owner(cx, team_id);
    let collections = store.collections();
    let now = crate::session_data::now_ms();
    let key = RowsKey {
        team_id: team_id.to_string(),
        revisions: [
            collections.sessions.read(cx).revision(),
            collections.pull_requests.read(cx).revision(),
            collections.agents.read(cx).revision(),
            collections.devices.read(cx).revision(),
            collections.teams.read(cx).revision(),
        ],
        owner,
        minute: now / 60_000,
    };
    if let Some((cached, rows)) = &cx.default_global::<RowsMemo>().0 {
        if *cached == key {
            return rows.clone();
        }
    }
    let input = input(team_id, now, cx);
    let stuck = stuck_rows(team_id, &input, owner, cx);
    let rows = Rc::new(with_stuck_rows(needs_you(&input), stuck));
    cx.global_mut::<RowsMemo>().0 = Some((key, rows.clone()));
    rows
}

/// What a row's press does (#EXP-1710): an `approval`-mode ready PR row
/// (a `pr` row that is not stuck: `needs_you()` lists ready PRs only outside
/// `auto` mode) asks THE status pill's Needs approval confirm; every other
/// row, a stuck PR's included, opens by `opens`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowPress {
    Open,
    Approve,
}

pub(crate) fn row_press(row: &NeedsYouRow, stuck: &HashMap<String, StuckPr>) -> RowPress {
    if row.kind == NeedsYouKind::Pr && !stuck.contains_key(&row.message_id) {
        RowPress::Approve
    } else {
        RowPress::Open
    }
}

/// Press a row: Needs approval = THE pill's confirm (the plain or the stack
/// confirm; Merge queues it, Cancel leaves the row), else [`open_row`].
pub(crate) fn press_row(row: &NeedsYouRow, press: RowPress, window: &mut Window, cx: &mut App) {
    match press {
        RowPress::Approve => crate::pr_merge::ask_approve(&row.message_id, window, cx),
        RowPress::Open => open_row(row, window, cx),
    }
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
        NeedsYouOpens::Card => crate::session_chat::request_chat_card(&row.root_id, &row.message_id, cx),
        NeedsYouOpens::Results => crate::session_chat::request_chat_results(&row.root_id, cx),
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
/// the press. `actions` = a stuck PR's Button sm row, under line 2.
pub(crate) fn needs_you_row(row: &NeedsYouRow, actions: Option<AnyElement>, cx: &App) -> gpui::Stateful<gpui::Div> {
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
                .child(div().mt(px(2.)).truncate().text_xs().text_color(theme.muted_foreground).child(SharedString::from(second_line(row))))
                .children(actions),
        )
}

/// A stuck PR's actions as THE Button sm row (outline, wrapping; web
/// `mt-2 flex-wrap gap-2`): Retry (= Resume auto-merge), Merge now (owners:
/// THE shared alert card `merge-now-pr`), Close PR (the close confirm). A
/// press never opens the row.
pub(crate) fn stuck_actions(stuck: &StuckPr) -> AnyElement {
    let id = stuck.pull_request_id.clone();
    h_flex()
        .id(SharedString::from(format!("{}-{id}", MERGE_TRAIN.test_ids.stuck_row)))
        .mt_2()
        .flex_wrap()
        .gap_2()
        .children(stuck.actions.iter().map(|action| {
            let action = *action;
            let id = id.clone();
            Button::new(SharedString::from(format!("{}-{}-{id}", MERGE_TRAIN.test_ids.stuck_row, action.id())))
                .outline()
                .web_sm()
                .label(action.label())
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    match action {
                        StuckAction::Retry => {
                            crate::pr_merge::run_train_op(id.clone(), crate::pr_merge::TrainOp::Hold(false), cx)
                        }
                        StuckAction::MergeNow => crate::pr_merge::ask_merge_now(&id, window, cx),
                        StuckAction::ClosePr => crate::issue_header::prompt_close_pr(
                            id.clone(),
                            crate::issue_header::close_pr_description(linked_others(&id, cx)),
                            window,
                            cx,
                        ),
                    }
                })
        }))
        .into_any_element()
}

/// The issues `pull_request_id` closes beyond one (the close confirm's batch
/// line).
fn linked_others(pull_request_id: &str, cx: &App) -> usize {
    Store::try_global(cx)
        .map(|store| {
            store
                .collections()
                .pull_request_issues
                .read(cx)
                .iter()
                .filter(|link| link.pull_request_id == pull_request_id)
                .count()
                .saturating_sub(1)
        })
        .unwrap_or(0)
}

/// THE list: one reading column of rows, else THE EmptyState. `stuck` = the
/// stuck PRs' actions by row id.
pub(crate) fn needs_you_list(rows: &[NeedsYouRow], stuck: &HashMap<String, StuckPr>, cx: &App) -> AnyElement {
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
            let press = row_press(row, stuck);
            needs_you_row(row, stuck.get(&row.message_id).map(stuck_actions), cx)
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| press_row(&open, press, window, cx))
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
        let _span = tracing::trace_span!("render", view = "NeedsYouView").entered();
        let ready = Store::try_global(cx).is_some_and(|store| store.collections().sessions.read(cx).is_ready());
        let column: AnyElement = if ready {
            let rows = active_team_id(&self.nav, cx).map(|team_id| rows_in_team(&team_id, cx)).unwrap_or_default();
            needs_you_list(&rows.rows, &rows.stuck, cx)
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

    fn dated(id: &str, kind: NeedsYouKind, created_at: &str) -> NeedsYouRow {
        NeedsYouRow { message_id: id.into(), created_at: created_at.into(), ..row(kind, "") }
    }

    /// #EXP-1710 (web `withStuckRows`): a stuck PR replaces its ready row
    /// and sorts with the PR rows, newest first, under the decisions.
    #[test]
    fn stuck_rows_fold_under_the_decisions() {
        let base = vec![
            dated("plan", NeedsYouKind::Plan, "2026-11-01T00:00:00.000Z"),
            dated("pr-a", NeedsYouKind::Pr, "2026-11-03T00:00:00.000Z"),
            dated("pr-b", NeedsYouKind::Pr, "2026-11-01T00:00:00.000Z"),
        ];
        let stuck = |id: &str, at: &str| {
            let mut row = dated(id, NeedsYouKind::Pr, at);
            row.title = format!("Auto-merge stuck on {id}");
            (row, StuckPr { pull_request_id: id.into(), actions: vec![StuckAction::Retry, StuckAction::ClosePr] })
        };
        let folded = with_stuck_rows(
            base.clone(),
            vec![stuck("pr-b", "2026-11-01T00:00:00.000Z"), stuck("pr-c", "2026-11-04T00:00:00.000Z")],
        );
        let ids: Vec<&str> = folded.rows.iter().map(|row| row.message_id.as_str()).collect();
        assert_eq!(ids, ["plan", "pr-c", "pr-a", "pr-b"]);
        assert_eq!(folded.rows[3].title, "Auto-merge stuck on pr-b");
        assert_eq!(folded.stuck.len(), 2);
        assert!(!folded.stuck.contains_key("pr-a"));
        // Nothing stuck = the fold's rows as they are.
        assert_eq!(with_stuck_rows(base.clone(), Vec::new()).rows, base);
    }

    /// #EXP-1710: an `approval` ready PR row's press = Needs approval; a
    /// stuck PR row and every decision open by `opens`.
    #[test]
    fn a_ready_pr_row_presses_needs_approval() {
        let stuck: HashMap<String, StuckPr> = [(
            "pr-s".to_string(),
            StuckPr { pull_request_id: "pr-s".into(), actions: vec![StuckAction::Retry, StuckAction::ClosePr] },
        )]
        .into();
        assert_eq!(row_press(&dated("pr-r", NeedsYouKind::Pr, "2026-11-01T00:00:00.000Z"), &stuck), RowPress::Approve);
        assert_eq!(row_press(&dated("pr-s", NeedsYouKind::Pr, "2026-11-01T00:00:00.000Z"), &stuck), RowPress::Open);
        assert_eq!(row_press(&dated("plan", NeedsYouKind::Plan, "2026-11-01T00:00:00.000Z"), &stuck), RowPress::Open);
        assert_eq!(row_press(&dated("q", NeedsYouKind::Question, "2026-11-01T00:00:00.000Z"), &stuck), RowPress::Open);
    }

    fn pull(updated_at: Option<&str>) -> domain::rows::PullRequestRow {
        serde_json::from_value(serde_json::json!({
            "id": "p", "state": "open", "merge_state": "conflict",
            "created_at": "2027-01-15T06:00:00.000Z", "updated_at": updated_at,
        }))
        .unwrap()
    }

    fn opener(device_id: Option<&str>) -> domain::rows::SessionRow {
        serde_json::from_value(serde_json::json!({"id": "s", "user_id": "u", "device_id": device_id})).unwrap()
    }

    fn device(last_seen_at: Option<&str>) -> domain::rows::DeviceRow {
        serde_json::from_value(serde_json::json!({"id": "d", "user_id": "u", "device_id": "dev", "last_seen_at": last_seen_at}))
            .unwrap()
    }

    fn at(iso: &str) -> i64 {
        domain::session_tree::iso_ms(iso).unwrap()
    }

    /// merge-train `offlineSince` off the rows: an unknown last-seen on a
    /// device that is not online = the PR's `updated_at`, never online.
    #[test]
    fn the_stuck_fold_reads_offline_since() {
        let now = at("2027-01-15T10:00:00.000Z");
        let updated = Some("2027-01-15T08:00:00.000Z");
        let online = device(Some("2027-01-15T09:59:59.000Z"));
        let seen = device(Some("2027-01-15T07:30:00.000Z"));
        let unknown = device(None);
        let with_device = opener(Some("dev"));
        // Online = none.
        assert_eq!(opener_offline_since(&pull(updated), Some(&with_device), [&online].into_iter(), now), None);
        // Offline, last seen known = since the last heartbeat.
        assert_eq!(
            opener_offline_since(&pull(updated), Some(&with_device), [&seen].into_iter(), now),
            Some(at("2027-01-15T07:30:00.000Z"))
        );
        // Offline, last seen unknown = since the PR went conflict/failing.
        assert_eq!(
            opener_offline_since(&pull(updated), Some(&with_device), [&unknown].into_iter(), now),
            Some(at("2027-01-15T08:00:00.000Z"))
        );
        // No device row synced: never online.
        assert_eq!(
            opener_offline_since(&pull(updated), Some(&with_device), std::iter::empty(), now),
            Some(at("2027-01-15T08:00:00.000Z"))
        );
        // No updated_at = the created_at.
        assert_eq!(
            opener_offline_since(&pull(None), Some(&with_device), std::iter::empty(), now),
            Some(at("2027-01-15T06:00:00.000Z"))
        );
        // No opener, or an opener without a device = none.
        assert_eq!(opener_offline_since(&pull(updated), None, [&unknown].into_iter(), now), None);
        assert_eq!(opener_offline_since(&pull(updated), Some(&opener(None)), [&unknown].into_iter(), now), None);
        // The unknown last-seen reaches the stuck row once 2 h passed.
        let since = opener_offline_since(&pull(updated), Some(&with_device), [&unknown].into_iter(), now);
        let row = domain::merge_train::stuck_row(12, "conflict", None, since, now, true).expect("stuck");
        assert_eq!(row.meta, domain::merge_train::MERGE_TRAIN.copy.needs_you.offline);
    }

    #[test]
    fn only_waiting_kinds_feed_the_fold() {
        assert!(is_waiting_kind(Some(&serde_json::json!({ "kind": "plan" }))));
        assert!(is_waiting_kind(Some(&serde_json::json!({ "kind": "ask" }))));
        assert!(!is_waiting_kind(Some(&serde_json::json!({ "kind": "surface" }))));
        assert!(!is_waiting_kind(None));
    }
}
