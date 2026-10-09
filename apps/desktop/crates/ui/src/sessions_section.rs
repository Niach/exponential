//! EXP-746/EXP-923 — MY agent runs, as two lists.
//!
//! **Running** (the ACTIVE TEAM's live sessions of mine, here and on every
//! other machine — EXP-1075) moved OUT of a rendered section and into the RAIL
//! (EXP-923): a live run is navigation now, not a document, so it lives in the
//! sidebar beside the boards and never opens a top tab. What is left here is
//! its data half, [`rail_running_rows`] — the one projection (remote ∪ local,
//! nested) the rail draws.
//!
//! **Recent** is the ×4 section (web/iOS/Android have their own): own,
//! person-started, ENDED rows in the active team, newest end first
//! ([`crate::queries::own_ended_runs`]). A TRIGGERED run is NOT here — its
//! home is its action page's Runs section (SLOP-2; EXP-676 split it out), and
//! listing it twice is the duplication that split. EXP-923 moved it behind
//! the Agent page's history button; EXP-1192 made that panel a second
//! sidebar INSIDE the content card ([`RecentRunsNav`],
//! `navigation::SecondSidebar::RecentRuns`) — the composer's page is a
//! composer and nothing else.
//!
//! EXP-827/EXP-996: both NEST — a run started by another run through
//! `exponential_sessions_start` sits under it, a resume succession collapses
//! into ONE row ([`domain::session_tree::session_tree`], the ×4 rule; EXP-1049
//! draws it here). EXP-965: the nesting draws a tree connector
//! ([`domain::tree_guides`]). EXP-1248: no fold — children always show, and
//! every row is [`run_rows::run_row`] (small on the rail, big in Recent).
//! EXP-1246: a Recent row opens its run with the Recent origin
//! ([`crate::navigation::recent_runs_origin`]), so the list stays beside it.

use std::collections::HashSet;

use gpui::{
    div, App, AppContext as _, Entity, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription,
    Task, Window,
};
use gpui_component::{scroll::ScrollableElement as _, v_flex, ActiveTheme as _};

use crate::coding_flow::{LocalSessionHost, LocalSessions};
use crate::navigation::{active_team_id, nav_for_window, Navigation, Screen};
use crate::queries;
use crate::run_rows::{self, PastRunFacts, RunRowSize};
use crate::surface::glass_section_header;

/// EXP-862 — how often a list re-derives itself on the CLOCK. Both lists
/// render relative times ("2 minutes ago") and a liveness that expires with
/// `last_seen_at`, neither of which produces a collection delta to observe.
const TICK: std::time::Duration = std::time::Duration::from_secs(5);

/// The empty Running band's copy, byte-identical ×4. The DESKTOP has no empty
/// state for it any more (EXP-923: the rail's Running section is hidden
/// outright while nothing runs), but the string is a ×4 contract the phones
/// still render, so it stays locked here rather than drifting on three
/// clients at once.
#[allow(dead_code)]
const NO_RUNNING_COPY: &str = "No agents running right now.";

/// EXP-923 — the number of Recent rows the history panel keeps. It is a
/// backstop behind a composer, not an archive.
const RECENT_CAP: usize = 20;

/// EXP-1192 — the Recent band's empty line: the panel is a column of its own
/// now, and a bare band over nothing read as still loading.
const NO_RECENT_COPY: &str = "No recent runs.";

// ---------------------------------------------------------------------------
// Running — the RAIL's section (EXP-923)
// ---------------------------------------------------------------------------

/// EXP-923 — one row of the rail's Running section.
///
/// Deliberately flat: the render pass owns everything it draws, because the
/// collection read that derives it borrows `cx` immutably while the row's own
/// callbacks need it mutably.
pub(crate) struct RailRunRow {
    pub(crate) session_id: String,
    /// The issue the run belongs to — the row stays highlighted while EITHER
    /// face of that work is on screen (EXP-870's Issue | Run pair).
    pub(crate) issue_id: Option<String>,
    /// EXP-876: the issue's identifier, a batch's `EXP-874 +2`, else none.
    pub(crate) identifier: Option<SharedString>,
    pub(crate) title: SharedString,
    /// EXP-923: the leading glyph is the AGENT's brand mark, not a state dot
    /// — a rail row says WHAT is running; the state rides the badge.
    pub(crate) agent: coding::CodingAgent,
    /// EXP-1184: what the run is doing ([`queries::coding_session_display`])
    /// — the working mark, or the mark with its state badge; `None` while
    /// its host is offline (paused): the bare mark.
    pub(crate) state: Option<queries::CodingSessionDisplay>,
    /// The host machine's glyph (`icons::device_icon`, the Devices list's
    /// own resolver) and its label, which the glyph's tooltip names.
    pub(crate) device_icon: crate::icons::ExpIcon,
    pub(crate) device_label: Option<SharedString>,
    /// `Some` while THIS process hosts the run — a kill goes straight to the
    /// host instead of out through the relay.
    pub(crate) local: Option<LocalSessionHost>,
    /// Web `ownsLiveRow`: a paused host is never killed (it resumes when the
    /// lid opens), so its row offers no Stop.
    pub(crate) paused: bool,
}

/// EXP-996 — one flattened row of a session TREE: a built run row at its
/// depth.
pub(crate) struct SessionTreeRow<T> {
    pub(crate) depth: usize,
    pub(crate) run: T,
}

/// The user's live sessions: the ones on OTHER machines
/// ([`queries::remote_session_rows`], the retired dock's projection) union the
/// ones this process hosts, as the EXP-996 TREE — resume successions collapsed,
/// children under their parent, top level newest activity first.
///
/// EXP-1075 — TEAM-SCOPED: one team's board, one team's runs. The rail shows
/// the ACTIVE team's live runs only; the other teams stay visible through the
/// team switcher's dot ([`queries::own_live_runs_by_team`], byte-equal with
/// what this draws after the switch). A row whose `team_id` did not decode is
/// never shown — the column is NOT NULL server-side, so `None` is a gap, not a
/// wildcard ([`queries::own_ended_runs`]'s rule).
fn live_run_tree<T>(
    nav: &Entity<Navigation>,
    cx: &mut App,
    build: impl Fn(
        &domain::rows::CodingSession,
        Option<&LocalSessionHost>,
        i64,
        &App,
    ) -> T,
) -> Vec<SessionTreeRow<T>> {
    // Everything that needs `&mut App` first — the collection reads below
    // borrow it immutably for the rest of the function.
    let Some(me) = queries::active_account(cx).map(|account| account.user_id) else {
        return Vec::new();
    };
    let Some(team_id) = crate::navigation::active_team_id(nav, cx) else {
        return Vec::new();
    };
    let own_device_id = queries::own_device_id(cx);
    // A remote row can only be opened when there is a relay to open it
    // through; without one the row would lead to a dead feed (the dock's
    // chip rule). A LOCAL row needs no relay at all.
    let relay = queries::remote_start_enabled(cx);
    let local_sessions = LocalSessions::global_ref(cx);
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections().clone();
    let now = chrono::Utc::now().timestamp();

    let hosts: Vec<(String, LocalSessionHost)> = local_sessions
        .as_ref()
        .map(|sessions| {
            let sessions = sessions.read(cx);
            sessions
                .session_ids()
                .into_iter()
                .filter_map(|id| {
                    let host = sessions.session_by_id(&id)?.host.clone();
                    Some((id, host))
                })
                .collect()
        })
        .unwrap_or_default();
    let local_ids: HashSet<String> = hosts.iter().map(|(id, _)| id.clone()).collect();

    let sessions = collections.coding_sessions.read(cx);
    let mut rows: Vec<&domain::rows::CodingSession> = if relay {
        queries::remote_session_rows(sessions.iter(), &me, &own_device_id, &local_ids, now)
            .into_iter()
            .filter(|session| session.team_id.as_deref() == Some(team_id.as_str()))
            .collect()
    } else {
        Vec::new()
    };
    // The local half: rows this process hosts, live by the same rule.
    rows.extend(
        sessions
            .iter()
            .filter(|session| local_ids.contains(&session.id))
            .filter(|session| queries::coding_session_is_live(session, now))
            .filter(|session| session.team_id.as_deref() == Some(team_id.as_str())),
    );
    if rows.is_empty() {
        return Vec::new();
    }
    // The tree orders itself (activity, newest first); this only makes the
    // input stable, so its first-wins tie-breaks never depend on the
    // collection's iteration order.
    rows.sort_by(|a, b| b.started_at.cmp(&a.started_at).then_with(|| b.id.cmp(&a.id)));
    flatten_session_tree(rows, |node| {
        let session: &domain::rows::CodingSession = node.session();
        let host = hosts
            .iter()
            .find(|(id, _)| id == &session.id)
            .map(|(_, host)| host);
        build(session, host, now, cx)
    })
}

/// EXP-996 — the shared half of every session list: the rows as a TREE
/// ([`domain::session_tree::session_tree`]), flattened with their depths, each
/// row turned into whatever the list draws (EXP-1248: children always show).
pub(crate) fn flatten_session_tree<T>(
    rows: Vec<&domain::rows::CodingSession>,
    mut build_run: impl FnMut(&domain::session_tree::SessionNode<&domain::rows::CodingSession>) -> T,
) -> Vec<SessionTreeRow<T>> {
    let tree = domain::session_tree::session_tree(
        rows,
        |session: &&domain::rows::CodingSession| domain::session_tree::coding_session_facts(session),
    );
    domain::session_tree::visible_session_tree_rows(&tree, &HashSet::new())
        .into_iter()
        .map(|flat| SessionTreeRow {
            depth: flat.depth,
            run: build_run(flat.node),
        })
        .collect()
}

/// EXP-923 — the rail's Running rows. Derived per paint like the rail's other
/// live reads (its observers already cover sessions, devices and the local
/// host set; [`tick`] covers the clock).
pub(crate) fn rail_running_rows(
    nav: &Entity<Navigation>,
    cx: &mut App,
) -> Vec<SessionTreeRow<RailRunRow>> {
    live_run_tree(nav, cx, |session, host, now, cx| {
        let collections = sync::Store::try_global(cx).map(|store| store.collections().clone());
        let issue = collections.as_ref().and_then(|collections| {
            session
                .issue_id
                .as_deref()
                .and_then(|id| collections.issues.read(cx).get(id).cloned())
        });
        // EXP-876: a batch row names itself after the issues it covers.
        let batch_issues = run_rows::batch_run_issues(session, cx);
        let device = collections.as_ref().and_then(|collections| {
            queries::session_device_row(session, collections.devices.read(cx).iter()).cloned()
        });
        let presentation = match collections.as_ref() {
            Some(collections) => queries::session_device_presentation(
                session,
                collections.devices.read(cx).iter(),
                now * 1_000,
            ),
            None => queries::SessionDevicePresentation {
                label: session.device_label.clone(),
                offline: false,
            },
        };
        // EXP-734: an issue-less run (action/chat) carries its own PR state.
        let pr_state = issue
            .as_ref()
            .and_then(|issue| issue.pr_state.as_deref())
            .or(session.pr_state.as_deref());
        // EXP-848: a run THIS process hosts reads its engine's own turn
        // signal, every other one the synced column.
        let busy = queries::session_agent_busy(
            session,
            host.map(|host| host.session.agent_busy()),
            now,
        );
        let display = queries::coding_session_display(session, busy, pr_state);
        let paused = queries::session_is_paused(display, session.status.as_deref(), &presentation);
        RailRunRow {
            session_id: session.id.clone(),
            issue_id: session.issue_id.clone(),
            identifier: run_rows::run_identifier(session, issue.as_ref(), &batch_issues),
            title: run_rows::run_title(session, issue.as_ref(), &batch_issues),
            // EXP-877's fallback, kept: an unknown or absent agent id is
            // claude, `codingSessions.start`'s own default.
            agent: session
                .agent
                .as_deref()
                .and_then(coding::CodingAgent::parse)
                .unwrap_or_default(),
            state: (!paused).then_some(display),
            device_icon: crate::icons::device_icon(
                device.as_ref().and_then(|row| row.icon.as_deref()),
                device.as_ref().is_some_and(|row| row.is_server()),
            ),
            device_label: presentation.label.clone().map(SharedString::from),
            local: host.cloned(),
            paused,
        }
    })
}

// ---------------------------------------------------------------------------
// Recent (the `Past*` names predate EXP-886's rename)
// ---------------------------------------------------------------------------

/// One finished row, flattened off the collections like the rail's.
#[derive(Clone, PartialEq)]
struct PastRow {
    depth: usize,
    facts: PastRunFacts,
}

pub(crate) struct PastSessionsSection {
    nav: Entity<Navigation>,
    /// EXP-862: the rows, derived on a data change or the clock (EXP-832).
    rows: Vec<PastRow>,
    _subscriptions: Vec<Subscription>,
    _tick: Task<()>,
}

impl PastSessionsSection {
    pub(crate) fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let mut subscriptions = watch_run_collections(cx, |this: &mut Self, cx| this.refresh(cx));
        subscriptions.push(cx.observe(&nav, |this, _, cx| this.refresh(cx)));
        let rows = Self::derive(&nav, cx);
        Self {
            nav,
            rows,
            _subscriptions: subscriptions,
            _tick: tick(cx, |this: &mut Self, cx| this.refresh(cx)),
        }
    }

    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        let nav = self.nav.clone();
        let next = Self::derive(&nav, cx);
        if next != self.rows {
            self.rows = next;
            cx.notify();
        }
    }

    fn derive(nav: &Entity<Navigation>, cx: &mut App) -> Vec<PastRow> {
        let Some(me) = queries::active_account(cx).map(|account| account.user_id) else {
            return Vec::new();
        };
        let Some(team_id) = active_team_id(nav, cx) else {
            return Vec::new();
        };
        let Some(store) = sync::Store::try_global(cx) else {
            return Vec::new();
        };
        let collections = store.collections().clone();
        let now = chrono::Utc::now().timestamp();
        let sessions = collections.coding_sessions.read(cx);
        let mut rows = queries::own_ended_runs(sessions.iter(), &me, &team_id);
        if rows.is_empty() {
            return Vec::new();
        }
        // EXP-923: history behind a composer, capped — the roots are newest
        // first, so the cut takes the oldest.
        rows.truncate(RECENT_CAP);
        // EXP-996: the ONE tree the rail draws too — a finished sub-session
        // under the run that started it, a resume succession as one row.
        flatten_session_tree(rows, |node| run_rows::past_run_facts(node.session(), now, cx))
        .into_iter()
        .map(|row| PastRow {
            depth: row.depth,
            facts: row.run,
        })
        .collect()
    }
}

impl Render for PastSessionsSection {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if self.rows.is_empty() {
            // EXP-1192: the band stays — it is the panel's only title.
            return v_flex()
                .min_w_0()
                .child(glass_section_header("Recent", None, cx))
                .child(
                    div()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(NO_RECENT_COPY),
                );
        }
        let open_session = open_session_id(window, cx);
        // EXP-965: the connector, off the depth sequence.
        let guides = domain::tree_guides::guides_for(
            &self.rows.iter().map(|row| row.depth).collect::<Vec<_>>(),
        );
        let mut column = v_flex().min_w_0();
        for (index, row) in self.rows.iter().enumerate() {
            let guides = guides.get(index).cloned().unwrap_or_default();
            let facts = row.facts.clone();
            let open_id = facts.session_id.clone();
            let active = open_session.as_deref() == Some(facts.session_id.as_str());
            column = column.child(run_rows::run_row(
                facts.row_spec(
                    "past-run",
                    index,
                    RunRowSize::Big,
                    guides,
                    active,
                    // EXP-1246: list-detail like the Inbox — the run opens to
                    // the right and this list stays beside it.
                    Box::new(move |_, window, cx| {
                        crate::session_screen::open_session_with_origin(
                            &open_id,
                            Some(crate::navigation::recent_runs_origin()),
                            window,
                            cx,
                        );
                    }),
                ),
                cx,
            ));
        }
        // ×4 copy: the section is "Recent" on every client (EXP-886).
        // EXP-923 took its fold away — a panel you opened on purpose has
        // nothing to gain from a second click to see what is in it.
        v_flex()
            .min_w_0()
            .child(glass_section_header("Recent", None, cx))
            .child(column)
    }
}

// ---------------------------------------------------------------------------
// The Recent-runs panel (EXP-923)
// ---------------------------------------------------------------------------

/// EXP-923 — the Agent page's history: the Recent rows behind the composer's
/// history button. EXP-1192: a second sidebar inside the content card
/// (`navigation::SecondSidebar::RecentRuns`), mounted by the screens panel.
/// Hidden by default, opened and put away by that button alone (it turns
/// into Back while this is up), and gone the moment the window leaves the
/// Chat screen.
pub(crate) struct RecentRunsNav {
    past: Entity<PastSessionsSection>,
}

impl RecentRunsNav {
    pub(crate) fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let past = cx.new(|cx| PastSessionsSection::new(window, cx));
        cx.observe(&past, |_, _, cx| cx.notify()).detach();
        Self { past }
    }
}

impl Render for RecentRunsNav {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // EXP-1192: no back row — the page's own history button (Back while
        // this is up) puts the panel away; the Recent band is its title.
        v_flex()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .child(
                div()
                    .id("recent-runs-scroll")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .min_w_0()
                    .overflow_y_scrollbar()
                    .child(
                        v_flex()
                            .w_full()
                            .min_w_0()
                            .px_2()
                            .pt_2()
                            .pb_2()
                            .child(self.past.clone()),
                    ),
            )
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// EXP-862 — the 5s re-derive every run list rides (EXP-832's pattern with a
/// clock): the rows carry relative times and a liveness that expires, neither
/// of which the collections signal. The refresh short-circuits on unchanged
/// rows, so a quiet list costs one derivation and no repaint.
pub(crate) fn tick<V: 'static>(
    cx: &mut gpui::Context<V>,
    refresh: fn(&mut V, &mut gpui::Context<V>),
) -> Task<()> {
    cx.spawn(async move |this, cx| loop {
        cx.background_executor().timer(TICK).await;
        if this.update(cx, |this, cx| refresh(this, cx)).is_err() {
            return;
        }
    })
}

/// The session the window is SHOWING, so its row can wear the selected paint.
fn open_session_id(window: &Window, cx: &mut App) -> Option<String> {
    let nav = nav_for_window(window, cx);
    match crate::navigation::resolved_screen(&nav, cx) {
        Some(Screen::Session { session_id }) => Some(session_id),
        _ => None,
    }
}

/// The Recent list joins `coding_sessions` with the issues it names and the
/// devices they ran on, so all three deltas RE-DERIVE it (EXP-862: the rows
/// are computed here and in the tick, never in `render`).
fn watch_run_collections<V: 'static>(
    cx: &mut gpui::Context<V>,
    refresh: fn(&mut V, &mut gpui::Context<V>),
) -> Vec<Subscription> {
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections().clone();
    vec![
        cx.observe(&collections.coding_sessions, move |this, _, cx| {
            refresh(this, cx)
        }),
        cx.observe(&collections.issues, move |this, _, cx| refresh(this, cx)),
        cx.observe(&collections.devices, move |this, _, cx| refresh(this, cx)),
        // EXP-874: a running row's action button carries the action's icon.
        cx.observe(&collections.actions, move |this, _, cx| refresh(this, cx)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-862: the empty Running band says exactly this, on every client
    /// that still draws one (EXP-923: the desktop's rail section hides
    /// instead, but the ×4 string stays locked here).
    #[test]
    fn the_empty_running_band_copy_is_locked() {
        assert_eq!(NO_RUNNING_COPY, "No agents running right now.");
        assert_eq!(NO_RECENT_COPY, "No recent runs.");
    }
}
