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
//! ([`crate::queries::own_ended_runs`]). An automation's runs are NOT here —
//! their home is the Automations tab's "Recent automated runs" (EXP-676), and
//! listing them twice is the duplication that split. EXP-923 moved it behind
//! the Agent page's history button, into the left column
//! ([`RecentRunsNav`], `shell::LeftOccupant::RecentRuns`) — the composer's
//! page is a composer and nothing else.
//!
//! EXP-827/EXP-996: both NEST — a run started by another run through
//! `exponential_sessions_start` sits under it, a resume succession collapses
//! into ONE row, and the runs of one workflow or one PR stack fold under a
//! GROUP row ([`domain::session_tree::session_tree`], the ×4 rule; EXP-1049
//! draws it here). Everything folds behind the same chevron, keyed by
//! [`domain::session_tree::session_tree_node_key`], so a group folds exactly
//! like a parent run. EXP-965: the nesting draws a tree connector
//! ([`domain::tree_guides`]).

use std::collections::HashSet;

use gpui::{
    div, App, AppContext as _, Entity, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Task, Window,
};
use gpui_component::{scroll::ScrollableElement as _, v_flex};

use crate::coding_flow::{LocalSessionHost, LocalSessions};
use crate::navigation::{active_team_id, nav_for_window, Navigation, Screen, TabOrigin};
use crate::queries;
use crate::run_rows::{self, PastRunFacts, PastRunSpec, RunRowFold};
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
    /// The run is waiting on a person (`needs_input`) — the mark wears the
    /// yellow corner badge, the rail's one status signal here.
    pub(crate) attention: bool,
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

/// EXP-996 — one flattened row of a session TREE: a built run row, or the
/// GROUP row its workflow / stack folds under. `key` is
/// [`domain::session_tree::session_tree_node_key`]'s, which is what
/// [`drop_collapsed`] and [`fold_for`] are keyed by — a group folds exactly
/// like a parent run.
pub(crate) struct SessionTreeRow<T> {
    pub(crate) key: String,
    pub(crate) depth: usize,
    pub(crate) has_children: bool,
    pub(crate) kind: SessionTreeRowKind<T>,
}

pub(crate) enum SessionTreeRowKind<T> {
    Run(T),
    /// A workflow / stack band: structure, not work (no dot, no device, no
    /// Stop).
    Group(run_rows::SessionGroupFacts),
}

impl<T> SessionTreeRow<T> {
    /// The run the row draws, `None` on a group row.
    pub(crate) fn run(&self) -> Option<&T> {
        match &self.kind {
            SessionTreeRowKind::Run(run) => Some(run),
            SessionTreeRowKind::Group(_) => None,
        }
    }
}

/// The user's live sessions: the ones on OTHER machines
/// ([`queries::remote_session_rows`], the retired dock's projection) union the
/// ones this process hosts, as the EXP-996 TREE — resume successions collapsed,
/// children under their parent, workflow and stack runs under group rows,
/// top level newest activity first.
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
    build: impl Fn(&domain::rows::CodingSession, Option<&LocalSessionHost>, i64, &App) -> T,
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
    let inputs = queries::session_tree_inputs(cx, &rows);
    flatten_session_tree(rows, inputs, |session| {
        let host = hosts
            .iter()
            .find(|(id, _)| id == &session.id)
            .map(|(_, host)| host);
        build(session, host, now, cx)
    })
}

/// EXP-996 — the shared half of every session list: the rows as a TREE
/// ([`domain::session_tree::session_tree`]), flattened with their depths, each
/// row turned into whatever the list draws. The collapsed set is applied
/// LATER, in the render ([`drop_collapsed`]), so folding costs no re-derive.
fn flatten_session_tree<T>(
    rows: Vec<&domain::rows::CodingSession>,
    inputs: queries::SessionTreeInputs,
    mut build_run: impl FnMut(&domain::rows::CodingSession) -> T,
) -> Vec<SessionTreeRow<T>> {
    let tree = domain::session_tree::session_tree(
        rows,
        |session: &&domain::rows::CodingSession| domain::session_tree::coding_session_facts(session),
        &inputs.context(),
    );
    domain::session_tree::visible_session_tree_rows(&tree, &HashSet::new())
        .into_iter()
        .map(|flat| SessionTreeRow {
            key: flat.key,
            depth: flat.depth,
            has_children: flat.has_children,
            kind: match flat.node.session() {
                Some(session) => SessionTreeRowKind::Run(build_run(session)),
                None => SessionTreeRowKind::Group(
                    run_rows::SessionGroupFacts::from_node(flat.node)
                        .expect("a node that is not a session is a group"),
                ),
            },
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
        let display = queries::coding_session_display(
            session,
            issue
                .as_ref()
                .and_then(|issue| issue.pr_state.as_deref())
                .or(session.pr_state.as_deref()),
        );
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
            attention: display == queries::CodingSessionDisplay::NeedsInput,
            device_icon: crate::icons::device_icon(
                device.as_ref().and_then(|row| row.icon.as_deref()),
                device.as_ref().is_some_and(|row| row.is_server()),
            ),
            device_label: presentation.label.clone().map(SharedString::from),
            local: host.cloned(),
            paused: queries::session_is_paused(display, &presentation),
        }
    })
}

// ---------------------------------------------------------------------------
// Recent (the `Past*` names predate EXP-886's rename)
// ---------------------------------------------------------------------------

/// One finished row, flattened off the collections like the rail's — EXP-996:
/// a run, or the workflow / stack group row above it.
#[derive(Clone, PartialEq)]
struct PastRow {
    /// The tree's node key ([`domain::session_tree::session_tree_node_key`]) —
    /// what the fold and [`drop_collapsed`] are keyed by.
    key: String,
    depth: usize,
    has_children: bool,
    kind: PastRowKind,
}

#[derive(Clone, PartialEq)]
enum PastRowKind {
    Run(PastRunFacts),
    Group(run_rows::SessionGroupFacts),
}

pub(crate) struct PastSessionsSection {
    nav: Entity<Navigation>,
    /// EXP-862: the rows, derived on a data change or the clock (EXP-832).
    rows: Vec<PastRow>,
    /// EXP-827: the parent rows whose sub-sessions are folded away. Per
    /// view, never persisted.
    collapsed: HashSet<String>,
    /// EXP-923: the LIST this section is, when it renders as the left
    /// column's panel — a row then opens its run pinned to it, so the column
    /// keeps showing the rows the click came from.
    list_origin: Option<TabOrigin>,
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
            collapsed: HashSet::new(),
            list_origin: None,
            _subscriptions: subscriptions,
            _tick: tick(cx, |this: &mut Self, cx| this.refresh(cx)),
        }
    }

    /// EXP-923: pin the list this section renders as (the left column's
    /// Recent-runs panel). Called by the renderer, which is the only thing
    /// that knows where the section ended up.
    pub(crate) fn set_list_origin(&mut self, origin: Option<TabOrigin>) {
        self.list_origin = origin;
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
        // under the run that started it, a resume succession as one row, the
        // runs of one workflow or stack under a group row.
        let inputs = queries::session_tree_inputs(cx, &rows);
        flatten_session_tree(rows, inputs, |session| {
            run_rows::past_run_facts(session, now, cx)
        })
        .into_iter()
        .map(|row| PastRow {
            key: row.key,
            depth: row.depth,
            has_children: row.has_children,
            kind: match row.kind {
                SessionTreeRowKind::Run(facts) => PastRowKind::Run(facts),
                SessionTreeRowKind::Group(facts) => PastRowKind::Group(facts),
            },
        })
        .collect()
    }
}

impl Render for PastSessionsSection {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if self.rows.is_empty() {
            return v_flex();
        }
        let open_session = open_session_id(window, cx);
        let rows = drop_collapsed(
            self.rows.iter().collect::<Vec<_>>(),
            &self.collapsed,
            |row| row.key.as_str(),
            |row| row.depth,
        );
        // EXP-965: the connector, off the VISIBLE depth sequence.
        let guides =
            domain::tree_guides::guides_for(&rows.iter().map(|row| row.depth).collect::<Vec<_>>());
        let mut column = v_flex().min_w_0();
        let list_origin = self.list_origin.clone();
        for (index, row) in rows.iter().enumerate() {
            let list_origin = list_origin.clone();
            let fold = fold_for(row.key.clone(), row.has_children, &self.collapsed, cx);
            let guides = guides.get(index).cloned().unwrap_or_default();
            column = column.child(match &row.kind {
                // EXP-996: the group row above its runs — a workflow opens, a
                // stack only folds.
                PastRowKind::Group(facts) => run_rows::render_group_row(
                    run_rows::GroupRowSpec {
                        id_prefix: "past-run",
                        index,
                        guides,
                        fold,
                        facts: facts.clone(),
                        on_open: group_row_open(facts),
                    },
                    cx,
                ),
                PastRowKind::Run(facts) => {
                    let open_id = facts.session_id.clone();
                    let active = open_session.as_deref() == Some(facts.session_id.as_str());
                    run_rows::render_past_run_row(
                        PastRunSpec {
                            id_prefix: "past-run",
                            index,
                            guides,
                            fold,
                            facts: facts.clone(),
                            // EXP-773: a plain link. The transcript and Resume
                            // live in the fullscreen session view now.
                            on_open: Box::new(move |_, window, cx| {
                                crate::session_screen::open_session_with_origin(
                                    &open_id,
                                    list_origin.clone(),
                                    window,
                                    cx,
                                );
                            }),
                        },
                        active,
                        cx,
                    )
                }
            });
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

/// EXP-923 — the Agent page's history, in the left column: the Recent rows
/// behind the composer's history button
/// (`shell::LeftOccupant::RecentRuns`). Hidden by default, opened by that
/// button alone, and gone the moment the window leaves the Chat screen.
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
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // EXP-923: a row opens its run beside THIS panel — the Agent page is
        // no longer a list, so nothing else would name one.
        self.past.update(cx, |section, _| {
            section.set_list_origin(None);
        });
        div()
            .id("recent-runs-scroll")
            .size_full()
            .min_h_0()
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

/// EXP-996 — what a GROUP row opens: a workflow group row navigates to its
/// workflow, a stack group row has no screen of its own and only folds.
pub(crate) fn group_row_open(
    facts: &run_rows::SessionGroupFacts,
) -> Option<run_rows::RunRowAction> {
    match &facts.kind {
        run_rows::SessionGroupKind::Workflow { workflow_id } => {
            let workflow_id = workflow_id.clone();
            Some(Box::new(move |_, window, cx| {
                crate::navigation::navigate(
                    window,
                    cx,
                    Screen::Workflow {
                        workflow_id: workflow_id.clone(),
                    },
                );
            }))
        }
        run_rows::SessionGroupKind::Stack => None,
    }
}

/// The session the window is SHOWING, so its row can wear the selected paint.
fn open_session_id(window: &Window, cx: &mut App) -> Option<String> {
    let nav = nav_for_window(window, cx);
    match crate::navigation::resolved_screen(&nav, cx) {
        Some(Screen::Session { session_id }) => Some(session_id),
        _ => None,
    }
}

/// EXP-827: the rows a nested list actually DRAWS — everything under a
/// collapsed parent is dropped, at any depth (the rail's `hidden_below` walk).
/// The sequence is already in tree order, so one pass over the depths is the
/// whole rule. Pure, so the folding is unit-tested without a window. EXP-897
/// reuses it for the Reviews page's PR stacks — the same fold, one rule.
pub(crate) fn drop_collapsed<R>(
    rows: Vec<R>,
    collapsed: &HashSet<String>,
    id: impl Fn(&R) -> &str,
    depth: impl Fn(&R) -> usize,
) -> Vec<R> {
    let mut out = Vec::with_capacity(rows.len());
    let mut hidden_below: Option<usize> = None;
    for row in rows {
        if let Some(at) = hidden_below {
            if depth(&row) > at {
                continue;
            }
            hidden_below = None;
        }
        if collapsed.contains(id(&row)) {
            hidden_below = Some(depth(&row));
        }
        out.push(row);
    }
    out
}

/// EXP-827: the fold control for a row — `None` unless it HAS children. The
/// click toggles the section's collapsed set, which is view state, so this is a
/// listener rather than a plain closure. EXP-996: `key` is the TREE's node key
/// ([`domain::session_tree::session_tree_node_key`]) — a session's id, or
/// `workflow:`/`stack:` for a group row, so both fold through one set.
pub(crate) fn fold_for<V: Collapsible + 'static>(
    key: String,
    has_children: bool,
    collapsed: &HashSet<String>,
    cx: &mut gpui::Context<V>,
) -> Option<RunRowFold> {
    has_children.then(|| RunRowFold {
        collapsed: collapsed.contains(&key),
        on_toggle: Box::new(cx.listener(move |this: &mut V, _, _window, cx| {
            let collapsed = this.collapsed_mut();
            if !collapsed.insert(key.clone()) {
                collapsed.remove(&key);
            }
            cx.notify();
        })),
    })
}

/// A section that folds its nested rows away ([`fold_for`]).
pub(crate) trait Collapsible {
    fn collapsed_mut(&mut self) -> &mut HashSet<String>;
}

impl Collapsible for PastSessionsSection {
    fn collapsed_mut(&mut self) -> &mut HashSet<String> {
        &mut self.collapsed
    }
}

// EXP-897: the Automations page's run log folds exactly like these two.
impl Collapsible for crate::automations_view::AutomationsView {
    fn collapsed_mut(&mut self) -> &mut HashSet<String> {
        self.collapsed_runs_mut()
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
        // EXP-996: the GROUP rows — which workflow a run is a node of, and the
        // workflow row the group row takes its name from.
        cx.observe(&collections.workflows, move |this, _, cx| refresh(this, cx)),
        cx.observe(&collections.workflow_nodes, move |this, _, cx| {
            refresh(this, cx)
        }),
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
    }

    /// EXP-827: a collapsed parent takes its WHOLE subtree off the list — its
    /// grandchildren included — and nothing else.
    #[test]
    fn a_collapsed_parent_hides_its_subtree() {
        // root-a ├ child-a1 │ └ grandchild, └ child-a2 ; root-b
        let rows: Vec<(&str, usize)> = vec![
            ("root-a", 0),
            ("child-a1", 1),
            ("grandchild", 2),
            ("child-a2", 1),
            ("root-b", 0),
        ];
        let visible = |collapsed: &[&str]| -> Vec<String> {
            let collapsed: HashSet<String> =
                collapsed.iter().map(|id| id.to_string()).collect();
            drop_collapsed(rows.clone(), &collapsed, |row| row.0, |row| row.1)
                .into_iter()
                .map(|(id, _)| id.to_string())
                .collect()
        };
        assert_eq!(
            visible(&[]),
            vec!["root-a", "child-a1", "grandchild", "child-a2", "root-b"]
        );
        assert_eq!(visible(&["root-a"]), vec!["root-a", "root-b"]);
        assert_eq!(
            visible(&["child-a1"]),
            vec!["root-a", "child-a1", "child-a2", "root-b"]
        );
        // A collapsed id that is not in the list changes nothing.
        assert_eq!(visible(&["gone"]).len(), rows.len());
    }
}
