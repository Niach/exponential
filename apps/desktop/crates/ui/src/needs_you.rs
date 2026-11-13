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

/// One session's share of the fold's input: its row, and the waiting cards
/// of its digest. `None` = not in the team's list.
type SessionShare = Option<(NeedsYouSessionInput, Vec<NeedsYouMessageInput>)>;

fn session_share(session: &domain::rows::SessionRow, team_id: &str, team_ids: &[String], me: Option<&str>, cx: &App) -> SessionShare {
    if !crate::session_data::in_team_or_personal(session, team_id, team_ids, me) {
        return None;
    }
    let agents = Store::global(cx).collections().agents.read(cx);
    let input = NeedsYouSessionInput {
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
                .or_else(|| tracing::trace_span!("needs_you.title").in_scope(|| crate::session_data::session_title_of(&session.id, cx)))
                .unwrap_or_default()
        },
        status: session.status.clone().unwrap_or_default(),
        agent_id: session.agent_id.clone(),
        user_id: session.user_id.clone(),
        busy: session.busy,
    };
    let _span = tracing::trace_span!("needs_you.waiting").entered();
    let waiting = crate::session_data::digest_messages_of(&[session], cx)
        .iter()
        .flat_map(|rows| rows.iter())
        .filter(|message| is_waiting_kind(message.card.as_ref()))
        .map(|message| NeedsYouMessageInput {
            id: message.id.clone(),
            session_id: message.session_id.clone(),
            author_session_id: message.author_session_id.clone(),
            body: message.body.clone().unwrap_or_default(),
            created_at: message.created_at.clone().unwrap_or_default(),
            answered_at: message.answered_at.clone(),
            card: message.card.clone(),
        })
        .collect();
    Some((input, waiting))
}

/// #PERF-3: what every share was read under besides its own row (a share
/// titles its root off the links, issues and agents).
#[derive(Clone, PartialEq, Eq)]
struct SharesKey {
    team_id: String,
    me: Option<String>,
    team_ids: Vec<String>,
    sessions: gpui::EntityId,
}

/// #PERF-3: each session's share, kept per sessions write: a write derives
/// the shares of the rows it names, never the team's every row.
#[derive(Default)]
struct Shares {
    key: Option<SharesKey>,
    /// The `sessions`, `agents`, `session_issues` and `issues` revisions the
    /// shares are current to (#PERF-10: each write kind re-derives only
    /// the shares it names).
    revisions: [u64; 4],
    /// Every link: id → (session, issue) (a deleted link still names its
    /// session).
    links: HashMap<String, (String, String)>,
    /// Each session's share and its device (a stuck PR reads its opener's).
    shares: HashMap<String, (SessionShare, Option<String>)>,
    /// Moves only when a share (or a device) did: a sessions write that
    /// moved neither (a stamp, a digest's plain message) folds nothing.
    generation: u64,
    /// The fold's session list, patched in place per write (never cloned),
    /// and each id's place in it.
    team: Vec<NeedsYouSessionInput>,
    place: HashMap<String, usize>,
    /// The team's PRs as the fold reads them, per change of the PRs.
    pulls: Option<(Rc<Vec<domain::rows::PullRequestRow>>, Vec<NeedsYouPullRequestInput>)>,
}

impl Shares {
    /// `id`'s entry in the fold's session list follows its share.
    fn place_share(&mut self, id: &str) {
        let input = self.shares.get(id).and_then(|(share, _)| share.as_ref()).map(|(input, _)| input.clone());
        match (self.place.get(id).copied(), input) {
            (Some(at), Some(input)) => self.team[at] = input,
            (None, Some(input)) => {
                self.place.insert(id.to_string(), self.team.len());
                self.team.push(input);
            }
            (Some(at), None) => {
                self.place.remove(id);
                self.team.swap_remove(at);
                if let Some(moved) = self.team.get(at) {
                    self.place.insert(moved.id.clone(), at);
                }
            }
            (None, None) => {}
        }
    }
}

thread_local! {
    /// Per team (#FEED-232 folds every synced team's rows, the picker's
    /// counts and the app icon's badge).
    static SHARES: std::cell::RefCell<HashMap<String, Shares>> = Default::default();
}

/// The shares brought up to the store; answers their generation.
fn shares_generation(team_id: &str, me: Option<&str>, cx: &App) -> u64 {
    let collections = Store::global(cx).collections();
    let sessions = collections.sessions.read(cx);
    let agents = collections.agents.read(cx);
    let links = collections.session_issues.read(cx);
    let issues = collections.issues.read(cx);
    // #EXP-1714: plus the viewer's personal-team sessions (their global
    // Assistant's conversation).
    let team_ids = crate::session_data::synced_team_ids(cx);
    // A whole read only where no row says what moved: the first one, another
    // store, team or viewer, the synced teams themselves (which sessions
    // count as personal), or a change log that no longer reaches back.
    let key = SharesKey { team_id: team_id.to_string(), me: me.map(str::to_string), team_ids: team_ids.clone(), sessions: collections.sessions.entity_id() };
    let revisions = [sessions.revision(), agents.revision(), links.revision(), issues.revision()];
    let ids = |keys: Vec<&sync::protocol::RowKey>| -> Vec<String> {
        keys.into_iter()
            .filter_map(|key| match key {
                sync::protocol::RowKey::Single(id) => Some(id.clone()),
                _ => None,
            })
            .collect()
    };
    SHARES.with(|teams| {
        let mut teams = teams.borrow_mut();
        let cache = teams.entry(team_id.to_string()).or_default();
        let changed = (cache.key.as_ref() == Some(&key))
            .then(|| {
                Some((
                    ids(sessions.changed_since(cache.revisions[0])?),
                    ids(agents.changed_since(cache.revisions[1])?),
                    ids(links.changed_since(cache.revisions[2])?),
                    ids(issues.changed_since(cache.revisions[3])?),
                ))
            })
            .flatten();
        let share_of = |row: &domain::rows::SessionRow| (session_share(row, team_id, &team_ids, me, cx), row.device_id.clone());
        match changed {
            Some((written_sessions, written_agents, written_links, written_issues)) => {
                let mut stale: std::collections::HashSet<String> = written_sessions.into_iter().collect();
                // An agent's name titles its conversations.
                if !written_agents.is_empty() {
                    let written: std::collections::HashSet<&str> = written_agents.iter().map(String::as_str).collect();
                    stale.extend(sessions.iter().filter(|row| row.agent_id.as_deref().is_some_and(|agent| written.contains(agent))).map(|row| row.id.clone()));
                }
                // A link (before and after the write) titles its session.
                for id in &written_links {
                    if let Some((session, _)) = cache.links.remove(id) {
                        stale.insert(session);
                    }
                    if let Some(link) = links.get(id) {
                        cache.links.insert(id.clone(), (link.session_id.clone(), link.issue_id.clone()));
                        stale.insert(link.session_id.clone());
                    }
                }
                // …and so does every issue it links.
                if !written_issues.is_empty() {
                    let written: std::collections::HashSet<&str> = written_issues.iter().map(String::as_str).collect();
                    let linking: Vec<String> = cache.links.values().filter(|(_, issue)| written.contains(issue.as_str())).map(|(session, _)| session.clone()).collect();
                    stale.extend(linking);
                }
                let mut moved = false;
                for id in stale {
                    let share = sessions.get(&id).map_or((None, None), share_of);
                    if cache.shares.get(&id) != Some(&share) {
                        moved = true;
                        cache.shares.insert(id.clone(), share);
                        cache.place_share(&id);
                    }
                }
                if moved {
                    cache.generation += 1;
                }
            }
            None => {
                let _span = tracing::trace_span!("needs_you.shares_whole").entered();
                cache.shares = sessions.iter().map(|row| (row.id.clone(), share_of(row))).collect();
                cache.links = links.iter().map(|link| (link.id.clone(), (link.session_id.clone(), link.issue_id.clone()))).collect();
                cache.team.clear();
                cache.place.clear();
                let ids: Vec<String> = sessions.iter().map(|row| row.id.clone()).collect();
                for id in ids {
                    cache.place_share(&id);
                }
                cache.generation += 1;
            }
        }
        cache.key = Some(key);
        cache.revisions = revisions;
        cache.generation
    })
}

/// The fold's sessions and their waiting cards (tests).
#[cfg(test)]
fn session_inputs(team_id: &str, _me: Option<&str>, cx: &App) -> (Vec<NeedsYouSessionInput>, Vec<NeedsYouMessageInput>) {
    with_input(team_id, 0, cx, |input| (input.sessions.clone(), input.messages.clone()))
}

/// THE fold's input off the store, lent to `f`: the session list is the
/// kept one (#PERF-3), the waiting cards and the PRs read off the shares.
fn with_input<R>(team_id: &str, now: i64, cx: &App, f: impl FnOnce(&NeedsYouInput) -> R) -> R {
    let me = crate::queries::active_account(cx).map(|account| account.user_id);
    shares_generation(team_id, me.as_deref(), cx);
    let pulls = crate::session_data::team_pulls(team_id, cx);
    let merge_mode = Store::global(cx).collections().teams.read(cx).get(team_id).map(|row| row.merge_mode().to_string());
    let input = SHARES.with(|teams| {
        let mut teams = teams.borrow_mut();
        let cache = teams.entry(team_id.to_string()).or_default();
        if !cache.pulls.as_ref().is_some_and(|(at, _)| Rc::ptr_eq(at, &pulls)) {
            let rows = pulls
                .iter()
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
            cache.pulls = Some((pulls.clone(), rows));
        }
        let messages: Vec<NeedsYouMessageInput> = cache
            .team
            .iter()
            .filter_map(|session| cache.shares.get(&session.id))
            .filter_map(|(share, _)| share.as_ref())
            .flat_map(|(_, waiting)| waiting.iter().cloned())
            .collect();
        NeedsYouInput {
            now,
            me,
            sessions: std::mem::take(&mut cache.team),
            messages,
            pull_requests: cache.pulls.as_mut().map(|(_, rows)| std::mem::take(rows)).unwrap_or_default(),
            merge_mode,
        }
    });
    let answer = f(&input);
    SHARES.with(|teams| {
        let mut teams = teams.borrow_mut();
        let cache = teams.entry(team_id.to_string()).or_default();
        cache.team = input.sessions;
        if let Some((_, rows)) = cache.pulls.as_mut() {
            *rows = input.pull_requests;
        }
    });
    answer
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
    /// The shares' generation (the sessions, the agents that name them, the
    /// links and issues that title them), the PRs, the devices.
    revisions: [u64; 3],
    /// #PERF-10: the team's merge mode (a rename of the team folds nothing).
    merge_mode: Option<String>,
    owner: bool,
    /// The ages move with the minute.
    minute: i64,
}

/// Per team: #FEED-232's team picker asks for every team.
#[derive(Default)]
struct RowsMemo(HashMap<String, (RowsKey, Rc<NeedsYouRows>)>);

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
    // #PERF-3: a sessions write folds again only when it moved a share of
    // the input (a session's row as the fold reads it, its waiting cards).
    let me = crate::queries::active_account(cx).map(|account| account.user_id);
    let shares = tracing::trace_span!("needs_you.shares").in_scope(|| shares_generation(team_id, me.as_deref(), cx));
    let key = RowsKey {
        team_id: team_id.to_string(),
        revisions: [
            shares,
            collections.pull_requests.read(cx).revision(),
            collections.devices.read(cx).revision(),
        ],
        merge_mode: collections.teams.read(cx).get(team_id).map(|row| row.merge_mode().to_string()),
        owner,
        minute: now / 60_000,
    };
    if let Some((cached, rows)) = cx.default_global::<RowsMemo>().0.get(team_id) {
        if *cached == key {
            return rows.clone();
        }
    }
    let _span = tracing::trace_span!("needs_you.fold").entered();
    let rows = Rc::new(with_input(team_id, now, cx, |input| {
        let stuck = stuck_rows(team_id, input, owner, cx);
        with_stuck_rows(needs_you(input), stuck)
    }));
    cx.global_mut::<RowsMemo>().0.insert(team_id.to_string(), (key, rows.clone()));
    rows
}

/// #FEED-232: every synced team's needs-you rows' length (the team picker,
/// the OS app icon).
pub(crate) fn counts_by_team(cx: &mut App) -> std::collections::BTreeMap<String, usize> {
    let Some(store) = Store::try_global(cx) else {
        return Default::default();
    };
    let teams: Vec<String> = store.collections().teams.read(cx).iter().map(|team| team.id.clone()).collect();
    teams.into_iter().map(|team| {
        let count = rows_in_team(&team, cx).rows.len();
        (team, count)
    }).collect()
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

#[cfg(test)]
mod incremental_tests {
    use serde_json::json;
    use sync::protocol::RowKey;
    use sync::Store;

    fn write(rows: serde_json::Value, cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let sessions = Store::global(cx).collections().sessions.clone();
            sessions.update(cx, |c, cx| {
                for row in rows.as_array().unwrap() {
                    let row: domain::rows::SessionRow = serde_json::from_value(row.clone()).unwrap();
                    c.seed(sync::ShapeRow::key(&row), row);
                }
                cx.notify();
            });
        });
    }

    fn session(id: &str, root: Option<&str>, team: &str, extra: serde_json::Value) -> serde_json::Value {
        let mut row = json!({"id": id, "team_id": team, "root_id": root.unwrap_or(id), "parent_id": root, "status": "idle", "user_id": "u1", "title": format!("Chat {id}")});
        for (key, value) in extra.as_object().unwrap() {
            row[key] = value.clone();
        }
        row
    }

    /// #PERF-3: the shares kept per sessions write equal a whole read of
    /// the store after every kind of write.
    #[gpui::test]
    fn incremental_shares_equal_a_full_read(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let store = Store::open(cx, None, None);
            cx.set_global(store);
        });
        let rows: Vec<serde_json::Value> = (0..30)
            .flat_map(|n| {
                let root = format!("r{n:02}");
                let child = session(&format!("c{n:02}"), Some(&root), "t1", json!({"busy": n % 2 == 0}));
                [session(&root, None, if n % 7 == 0 { "t2" } else { "t1" }, json!({})), child]
            })
            .collect();
        write(serde_json::Value::Array(rows), cx);
        let both = |cx: &mut gpui::TestAppContext| {
            cx.update(|cx| {
                // The kept list's order is its own (the fold keys every
                // lookup by id): compared as sets.
                let sorted = |(mut team, mut messages): (Vec<domain::needs_you::NeedsYouSessionInput>, Vec<domain::needs_you::NeedsYouMessageInput>)| {
                    team.sort_by(|a, b| a.id.cmp(&b.id));
                    messages.sort_by(|a, b| a.id.cmp(&b.id));
                    (team, messages)
                };
                let incremental = sorted(super::session_inputs("t1", Some("u1"), cx));
                super::SHARES.with(|cache| *cache.borrow_mut() = Default::default());
                let full = sorted(super::session_inputs("t1", Some("u1"), cx));
                (incremental, full)
            })
        };
        let (a, b) = both(cx);
        assert_eq!(a, b);
        for (what, rows) in [
            ("an update", json!([session("c03", Some("r03"), "t1", json!({"status": "running", "busy": true}))])),
            ("an insert", json!([session("r40", None, "t1", json!({})), session("c40", Some("r40"), "t1", json!({}))])),
            ("a move between trees", json!([session("c05", Some("r06"), "t1", json!({}))])),
            ("a team move", json!([session("r08", None, "t2", json!({})), session("r07", None, "t1", json!({}))])),
        ] {
            write(rows, cx);
            let (a, b) = both(cx);
            assert_eq!(a, b, "after {what}");
        }
        cx.update(|cx| {
            let sessions = Store::global(cx).collections().sessions.clone();
            sessions.update(cx, |c, cx| {
                c.remove_for_test(&RowKey::Single("c09".into()));
                c.remove_for_test(&RowKey::Single("r10".into()));
                cx.notify();
            });
        });
        let (a, b) = both(cx);
        assert_eq!(a, b, "after deletes");
        assert!(a.0.iter().all(|row| row.id != "c09" && row.id != "r10"));
        // #PERF-10: the writes that title a share: an agent renamed, links
        // added and removed, an issue renamed, a team renamed.
        let with = |cx: &mut gpui::TestAppContext, f: &dyn Fn(&sync::Collections, &mut gpui::App)| {
            cx.update(|cx| {
                let c = Store::global(cx).collections().clone();
                f(&c, cx);
            })
        };
        fn seed<T: sync::ShapeRow + serde::de::DeserializeOwned>(collection: &gpui::Entity<sync::Collection<T>>, rows: serde_json::Value, cx: &mut gpui::App) {
            collection.update(cx, |c, cx| {
                for row in rows.as_array().unwrap() {
                    let row: T = serde_json::from_value(row.clone()).unwrap();
                    c.seed(sync::ShapeRow::key(&row), row);
                }
                cx.notify();
            });
        }
        with(cx, &|c, cx| {
            seed(&c.teams, json!([{"id": "t1", "name": "One", "slug": "one"}, {"id": "t2", "name": "Two", "slug": "two"}]), cx);
            seed(&c.agents, json!([{"id": "a1", "team_id": "t1", "name": "Reviewer"}]), cx);
            seed(&c.boards, json!([{"id": "b1", "team_id": "t1", "name": "App", "slug": "app", "prefix": "APP"}]), cx);
            seed(&c.issues, json!([{"id": "i1", "board_id": "b1", "number": 1, "identifier": "APP-1", "title": "Crash", "status": "in_progress"}]), cx);
        });
        write(json!([session("r11", None, "t1", json!({"agent_id": "a1"}))]), cx);
        let (a, b) = both(cx);
        assert_eq!(a, b, "after the setup");
        let steps: Vec<(&str, Box<dyn Fn(&sync::Collections, &mut gpui::App)>)> = vec![
            ("an agent renamed", Box::new(|c, cx| seed(&c.agents, json!([{"id": "a1", "team_id": "t1", "name": "Senior reviewer"}]), cx))),
            ("a link added", Box::new(|c, cx| seed(&c.session_issues, json!([{"id": "l1", "session_id": "r12", "issue_id": "i1", "created_at": "2026-11-12T09:00:00.000Z"}]), cx))),
            ("an issue renamed", Box::new(|c, cx| seed(&c.issues, json!([{"id": "i1", "board_id": "b1", "number": 1, "identifier": "APP-1", "title": "Crash on launch", "status": "done"}]), cx))),
            ("a link removed", Box::new(|c, cx| c.session_issues.update(cx, |l, cx| {
                l.remove_for_test(&RowKey::Single("l1".into()));
                cx.notify();
            }))),
            ("a team renamed", Box::new(|c, cx| seed(&c.teams, json!([{"id": "t1", "name": "Uno", "slug": "one"}]), cx))),
        ];
        for (what, step) in steps {
            with(cx, &*step);
            let (a, b) = both(cx);
            assert_eq!(a, b, "after {what}");
        }
    }
}
