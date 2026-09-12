//! EXP-746 — the Devices screen's two run lists: **Running** (the user's live
//! sessions, here and on every other machine) and **Past** (their finished
//! person-started runs).
//!
//! Running is where the dock's remote chips went. A chip could only ever be
//! opened; a row can be opened AND ended, which is the affordance a session on
//! a machine you are not sitting at actually needs. It lists BOTH halves —
//! runs this process hosts and runs it does not — because "my sessions" is one
//! list, and the dock's projection deliberately excludes the local ones.
//!
//! Past is the ×4 section (web/iOS/Android have their own): own,
//! person-started, ENDED rows in the active team, newest end first
//! ([`crate::queries::own_ended_runs`]). An automation's runs are NOT here —
//! their home is the Automations tab's "Recent automated runs" (EXP-676), and
//! listing them twice is the duplication that split.
//!
//! Both sections vanish entirely when empty: the Devices page already has an
//! empty state, and two empty headers under it read as breakage. EXP-818 moved
//! the pair onto the AGENT page's list column (`sidebar::render_sessions_tool`).
//!
//! EXP-827: both NEST — a run started by another run through
//! `exponential_sessions_start` sits under it
//! ([`domain::session_tree::nest_sessions`], the rail's rule) behind a fold
//! chevron, so a parent's subtree can be collapsed out of the way.

use std::collections::HashSet;

use gpui::{
    div, App, Entity, Hsla, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Task, Window,
};
use gpui_component::{v_flex, ActiveTheme as _};

use crate::coding_flow::{LocalSessionHost, LocalSessions};
use crate::navigation::{active_team_id, nav_for_window, Navigation, Screen, TabOrigin};
use crate::queries::{self, CodingSessionDisplay};
use crate::run_rows::{self, RunRowFold, RunRowKill, RunRowLead, RunRowSpec};
use crate::surface::{glass_section_band_fold, glass_section_header};

/// EXP-862 — how often a list re-derives itself on the CLOCK. Both sections
/// render relative times ("2 minutes ago") and a liveness that expires with
/// `last_seen_at`, neither of which produces a collection delta to observe.
const TICK: std::time::Duration = std::time::Duration::from_secs(5);

/// The Agent page's empty Running band (byte-identical ×4).
const NO_RUNNING_COPY: &str = "No agents running right now.";

// ---------------------------------------------------------------------------
// Running
// ---------------------------------------------------------------------------

/// One live row, flattened off the synced collections so the render pass owns
/// everything it draws (the collection read borrows `cx` immutably; the row
/// callbacks need it mutably).
#[derive(Clone)]
struct RunningRow {
    session_id: String,
    /// EXP-827: where the row sits in the session TREE
    /// ([`domain::session_tree::nest_sessions`]) — a run a run started.
    depth: usize,
    has_children: bool,
    identifier: Option<SharedString>,
    title: SharedString,
    caption: Option<SharedString>,
    /// EXP-850 §8: the run's agent caption (the newest running workflow's) —
    /// the row's second line.
    agent_caption: Option<SharedString>,
    tone: Hsla,
    /// The machine's name, for the kill confirm ("… on Studio").
    device_label: Option<String>,
    /// `Some` while THIS process hosts the run — the kill goes straight to the
    /// host instead of out through the relay.
    local: Option<LocalSessionHost>,
    /// Web `ownsLiveRow`: the row is the caller's and still live, and a paused
    /// host is never killed (it resumes when the lid opens).
    killable: bool,
}

/// EXP-862: the short-circuit for the derived rows. Hand-written because a
/// [`LocalSessionHost`] is a live handle, not a value — what the ROW shows of
/// it is only whether this process hosts the run.
impl PartialEq for RunningRow {
    fn eq(&self, other: &Self) -> bool {
        self.session_id == other.session_id
            && self.depth == other.depth
            && self.has_children == other.has_children
            && self.identifier == other.identifier
            && self.title == other.title
            && self.caption == other.caption
            && self.agent_caption == other.agent_caption
            && self.tone == other.tone
            && self.device_label == other.device_label
            && self.local.is_some() == other.local.is_some()
            && self.killable == other.killable
    }
}

pub(crate) struct RunningSessionsSection {
    /// EXP-862: the rows, derived when the data (or the clock) changes and
    /// never in `render` — the EXP-832 rule. Every repaint of the Agent page
    /// used to re-read three collections and re-format every caption.
    rows: Vec<RunningRow>,
    /// EXP-827: the parent rows whose sub-sessions are folded away (the rail's
    /// `collapsed_sessions`). Per view, never persisted.
    collapsed: HashSet<String>,
    /// EXP-862: the Agent page renders this band even with nothing in it
    /// ("No agents running right now."); the list nav still hides it.
    show_when_empty: bool,
    /// EXP-862: the LIST this section is, when it renders as the left
    /// column's `ListNav` — a row then opens its run pinned to it, so the
    /// column keeps showing the rows the click came from. `None` on the Agent
    /// page, where the breadcrumb rule derives the same answer.
    list_origin: Option<TabOrigin>,
    _subscriptions: Vec<Subscription>,
    _tick: Task<()>,
}

impl RunningSessionsSection {
    /// Not team-scoped, deliberately: "my live sessions" is one list, the way
    /// the dock's strip always read it — a run on another team's board is
    /// still a run of yours that is going right now.
    pub(crate) fn new(cx: &mut gpui::Context<Self>) -> Self {
        Self {
            rows: Self::derive(cx),
            collapsed: HashSet::new(),
            show_when_empty: false,
            list_origin: None,
            _subscriptions: watch_run_collections(cx, |this: &mut Self, cx| this.refresh(cx)),
            _tick: tick(cx, |this: &mut Self, cx| this.refresh(cx)),
        }
    }

    /// EXP-862 — the AGENT PAGE's band: always on screen, empty or not. The
    /// composer's page is where you go to see what is running, so an empty
    /// band that says so beats a page that silently omits it.
    pub(crate) fn always(cx: &mut gpui::Context<Self>) -> Self {
        Self {
            show_when_empty: true,
            ..Self::new(cx)
        }
    }

    /// EXP-862: pin the list this section renders as (the left column's
    /// `ListNav`). Called by the renderer, which is the only thing that knows
    /// where the section ended up.
    pub(crate) fn set_list_origin(&mut self, origin: Option<TabOrigin>) {
        self.list_origin = origin;
    }

    /// Whether the band has anything to show (the composer page centres
    /// itself when both bands are empty).
    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// EXP-862: re-derive and repaint — but only when something actually
    /// changed. The 5s tick fires whether or not a caption moved.
    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        let next = Self::derive(cx);
        if next != self.rows {
            self.rows = next;
            cx.notify();
        }
    }

    /// The user's live sessions: the ones on OTHER machines
    /// ([`queries::remote_session_rows`], the dock's projection) union the
    /// ones this process hosts. Newest start first, so the strip order and
    /// this list agree.
    fn derive(cx: &mut App) -> Vec<RunningRow> {
        // Everything that needs `&mut App` first — the collection reads below
        // borrow it immutably for the rest of the function.
        let Some(me) = queries::active_account(cx).map(|account| account.user_id) else {
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
        } else {
            Vec::new()
        };
        // The local half: rows this process hosts, live by the same rule.
        rows.extend(
            sessions
                .iter()
                .filter(|session| local_ids.contains(&session.id))
                .filter(|session| queries::coding_session_is_live(session, now)),
        );
        if rows.is_empty() {
            return Vec::new();
        }
        rows.sort_by(|a, b| b.started_at.cmp(&a.started_at).then_with(|| b.id.cmp(&a.id)));
        // EXP-827: nest a run started BY a run under it, the ONE rule the rail
        // and the mobile lists use. The sort above is the ROOT order; children
        // follow their parent, oldest first.
        let tree = domain::session_tree::nest_sessions(
            rows,
            |session| session.id.as_str(),
            |session| session.parent_session_id.as_deref(),
            |session| session.started_at.as_deref(),
        );

        let issues = collections.issues.read(cx);
        let devices = collections.devices.read(cx);
        let theme = cx.theme();
        tree.into_iter()
            .map(|tree_row| {
                let depth = tree_row.depth;
                let has_children = tree_row.has_children;
                let session = tree_row.session;
                let issue = session
                    .issue_id
                    .as_deref()
                    .and_then(|issue_id| issues.get(issue_id));
                let presentation =
                    queries::session_device_presentation(session, devices.iter(), now * 1_000);
                let display = queries::coding_session_display(
                    session,
                    // EXP-734: an issue-less run (action/chat) carries its own
                    // PR state on the row.
                    issue
                        .and_then(|issue| issue.pr_state.as_deref())
                        .or(session.pr_state.as_deref()),
                );
                let paused = queries::session_is_paused(display, &presentation);
                // EXP-850 §8: a run this process hosts reads the engine's own
                // caption signal (it WROTE the column; waiting for the echo
                // would only add latency), every other row the synced one —
                // the `session_agent_busy` precedence, one rule.
                let local_caption = hosts
                    .iter()
                    .find(|(id, _)| id == &session.id)
                    .and_then(|(_, host)| host.session.caption_signal().get());
                RunningRow {
                    session_id: session.id.clone(),
                    agent_caption: queries::session_agent_caption(session, local_caption, now)
                        .map(SharedString::from),
                    depth,
                    has_children,
                    identifier: issue.map(|issue| SharedString::from(issue.identifier.clone())),
                    title: session_title(session, issue),
                    caption: Some(SharedString::from(running_caption(
                        presentation.label.as_deref(),
                        display,
                        paused,
                        run_rows::run_started_at(session),
                        now,
                    ))),
                    // EXP-862: the ONE dot mapping, shared with the rail,
                    // the screens and the steer header.
                    tone: queries::session_dot_tone(
                        queries::SessionDotFacts::from_display(display, false, paused),
                        theme.muted_foreground,
                    ),
                    device_label: presentation.label.clone(),
                    local: hosts
                        .iter()
                        .find(|(id, _)| id == &session.id)
                        .map(|(_, host)| host.clone()),
                    killable: !paused,
                }
            })
            .collect()
    }

    /// The confirm before a live run is ended — the session bar's ×
    /// (EXP-769) and this list share it (`session_bar::prompt_kill_session`).
    fn prompt_kill(
        row_local: Option<LocalSessionHost>,
        device_label: Option<String>,
        session_id: String,
        window: &mut Window,
        cx: &mut App,
    ) {
        crate::session_bar::prompt_kill_session(row_local, device_label, session_id, window, cx);
    }
}

impl Render for RunningSessionsSection {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let open_session = open_session_id(window, cx);
        let rows = drop_collapsed(
            self.rows.iter().collect::<Vec<_>>(),
            &self.collapsed,
            |row| row.session_id.as_str(),
            |row| row.depth,
        );
        if rows.is_empty() && !self.show_when_empty {
            return v_flex();
        }
        // NO gap on the headed section (EXP-697): the header's `pb_2` IS the
        // 8px to the list, so the rows live in their own gapped column.
        let mut column = v_flex().min_w_0();
        if rows.is_empty() {
            column = column.child(
                div()
                    .px_1()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(NO_RUNNING_COPY),
            );
        }
        let list_origin = self.list_origin.clone();
        for (index, row) in rows.into_iter().enumerate() {
            let open_id = row.session_id.clone();
            let list_origin = list_origin.clone();
            let fold = fold_for(row.session_id.clone(), row.has_children, &self.collapsed, cx);
            let kill = row.killable.then(|| {
                let session_id = row.session_id.clone();
                let local = row.local.clone();
                let device_label = row.device_label.clone();
                RunRowKill {
                    // EXP-849 fix-up: one verb, wherever the run is hosted.
                    label: SharedString::from("Stop session"),
                    on_kill: Box::new(move |_, window, cx| {
                        Self::prompt_kill(
                            local.clone(),
                            device_label.clone(),
                            session_id.clone(),
                            window,
                            cx,
                        );
                    }),
                }
            });
            // EXP-862: the row whose session is on screen wears the flat
            // list's selected paint, like every other list here.
            let active = open_session.as_deref() == Some(row.session_id.as_str());
            column = column.child(run_rows::render_run_row_active(
                RunRowSpec {
                    id_prefix: "running-run",
                    index,
                    lead: RunRowLead::Live(row.tone),
                    depth: row.depth,
                    fold,
                    identifier: row.identifier.clone(),
                    title: row.title.clone(),
                    caption: row.caption.clone(),
                    subcaption: row.agent_caption.clone(),
                    on_open: Some(Box::new(move |_, window, cx| {
                        crate::session_screen::open_session_with_origin(
                            &open_id,
                            list_origin.clone(),
                            window,
                            cx,
                        );
                    })),
                    kill,
                },
                active,
                cx,
            ));
        }
        // The section carries its OWN top spacing (the page column has no
        // `gap`): a `gap_6` parent would reserve 24px for an empty section
        // too, and both of these render nothing most of the time.
        v_flex()
            .min_w_0()
            .mt_6()
            .child(glass_section_header("Running", None, cx))
            .child(column)
    }
}

// ---------------------------------------------------------------------------
// Past
// ---------------------------------------------------------------------------

/// One finished row, flattened like [`RunningRow`].
#[derive(Clone, PartialEq)]
struct PastRow {
    session_id: String,
    /// EXP-827: the session tree, exactly as Running nests it.
    depth: usize,
    has_children: bool,
    identifier: Option<SharedString>,
    title: SharedString,
    byline: SharedString,
    agent: Option<coding::CodingAgent>,
}

pub(crate) struct PastSessionsSection {
    nav: Entity<Navigation>,
    /// EXP-862: the rows, derived on a data change or the clock (EXP-832).
    rows: Vec<PastRow>,
    /// EXP-827: the folded parents of this list (see
    /// [`RunningSessionsSection::collapsed`]).
    collapsed: HashSet<String>,
    /// EXP-862: the whole SECTION folds. It is history behind a composer, so
    /// it starts collapsed and the band's count says how much is under it.
    expanded: bool,
    /// EXP-862: see [`RunningSessionsSection::list_origin`].
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
            expanded: false,
            list_origin: None,
            _subscriptions: subscriptions,
            _tick: tick(cx, |this: &mut Self, cx| this.refresh(cx)),
        }
    }

    /// EXP-862: see [`RunningSessionsSection::set_list_origin`].
    pub(crate) fn set_list_origin(&mut self, origin: Option<TabOrigin>) {
        self.list_origin = origin;
    }

    /// Whether the section has anything to show.
    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
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
        let rows = queries::own_ended_runs(sessions.iter(), &me, &team_id);
        if rows.is_empty() {
            return Vec::new();
        }
        // EXP-827: a finished sub-session nests under the run that started it,
        // the same rule Running and the rail use. `own_ended_runs`' order is
        // the ROOT order (newest end first).
        let tree = domain::session_tree::nest_sessions(
            rows,
            |session| session.id.as_str(),
            |session| session.parent_session_id.as_deref(),
            |session| session.started_at.as_deref(),
        );
        let issues = collections.issues.read(cx);
        let devices = collections.devices.read(cx);
        tree.into_iter()
            .map(|tree_row| {
                let depth = tree_row.depth;
                let has_children = tree_row.has_children;
                let session = tree_row.session;
                let issue = session
                    .issue_id
                    .as_deref()
                    .and_then(|issue_id| issues.get(issue_id));
                let presentation =
                    queries::session_device_presentation(session, devices.iter(), now * 1_000);
                PastRow {
                    session_id: session.id.clone(),
                    depth,
                    has_children,
                    identifier: issue.map(|issue| SharedString::from(issue.identifier.clone())),
                    title: session_title(session, issue),
                    byline: SharedString::from(run_rows::past_run_byline(
                        session,
                        presentation.label.as_deref(),
                        now,
                    )),
                    agent: session
                        .agent
                        .as_deref()
                        .and_then(coding::CodingAgent::parse),
                }
            })
            .collect()
    }
}

impl Render for PastSessionsSection {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if self.rows.is_empty() {
            return v_flex();
        }
        let total = self.rows.len();
        let open_session = open_session_id(window, cx);
        // ×4 copy: the section is "Past" on every client, and EXP-862 made
        // the header the FOLD — collapsed by default, the count trailing.
        let band = glass_section_band_fold("past-sessions-fold", "Past", total, !self.expanded, cx)
            .on_click(cx.listener(|this, _, _window, cx| {
                this.expanded = !this.expanded;
                cx.notify();
            }));
        let section = v_flex().min_w_0().mt_6().child(band);
        if !self.expanded {
            return section;
        }
        let rows = drop_collapsed(
            self.rows.iter().collect::<Vec<_>>(),
            &self.collapsed,
            |row| row.session_id.as_str(),
            |row| row.depth,
        );
        let mut column = v_flex().min_w_0();
        let list_origin = self.list_origin.clone();
        for (index, row) in rows.iter().enumerate() {
            let open_id = row.session_id.clone();
            let list_origin = list_origin.clone();
            let fold = fold_for(row.session_id.clone(), row.has_children, &self.collapsed, cx);
            let active = open_session.as_deref() == Some(row.session_id.as_str());
            column = column.child(run_rows::render_run_row_active(
                RunRowSpec {
                    id_prefix: "past-run",
                    index,
                    lead: RunRowLead::Agent(row.agent),
                    depth: row.depth,
                    fold,
                    identifier: row.identifier.clone(),
                    title: row.title.clone(),
                    caption: Some(row.byline.clone()),
                    // A finished run's last workflow is history, not a status
                    // line (EXP-850 §8 gates the caption on liveness).
                    subcaption: None,
                    // EXP-773: a plain link. The transcript and Resume live in
                    // the fullscreen session view now.
                    on_open: Some(Box::new(move |_, window, cx| {
                        crate::session_screen::open_session_with_origin(
                            &open_id,
                            list_origin.clone(),
                            window,
                            cx,
                        );
                    })),
                    kill: None,
                },
                active,
                cx,
            ));
        }
        section.child(column)
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// EXP-862 — the 5s re-derive both sections ride (EXP-832's pattern with a
/// clock): the rows carry relative times and a liveness that expires, neither
/// of which the collections signal. The refresh short-circuits on unchanged
/// rows, so a quiet list costs one derivation and no repaint.
fn tick<V: 'static>(
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

/// EXP-827: the rows a nested list actually DRAWS — everything under a
/// collapsed parent is dropped, at any depth (the rail's `hidden_below` walk).
/// The sequence is already in tree order, so one pass over the depths is the
/// whole rule. Pure, so the folding is unit-tested without a window.
fn drop_collapsed<R>(
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
/// listener rather than a plain closure.
fn fold_for<V: Collapsible + 'static>(
    session_id: String,
    has_children: bool,
    collapsed: &HashSet<String>,
    cx: &mut gpui::Context<V>,
) -> Option<RunRowFold> {
    has_children.then(|| RunRowFold {
        collapsed: collapsed.contains(&session_id),
        on_toggle: Box::new(cx.listener(move |this: &mut V, _, _window, cx| {
            let collapsed = this.collapsed_mut();
            if !collapsed.insert(session_id.clone()) {
                collapsed.remove(&session_id);
            }
            cx.notify();
        })),
    })
}

/// A section that folds its nested rows away ([`fold_for`]).
trait Collapsible {
    fn collapsed_mut(&mut self) -> &mut HashSet<String>;
}

impl Collapsible for RunningSessionsSection {
    fn collapsed_mut(&mut self) -> &mut HashSet<String> {
        &mut self.collapsed
    }
}

impl Collapsible for PastSessionsSection {
    fn collapsed_mut(&mut self) -> &mut HashSet<String> {
        &mut self.collapsed
    }
}

/// Both sections join `coding_sessions` with the issues they name and the
/// devices they ran on, so all three deltas RE-DERIVE them (EXP-862: the rows
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
    ]
}

/// The row's subject line: the issue title, a sync placeholder while that
/// issue row is missing, the action-name snapshot (a chat run's reads "Chat",
/// EXP-615), else the batch. Mirrors the dock chip's rule — EXP-746 D6
/// deletes that copy with the chips — and is byte-identical ×4 for the Past
/// rows below: web `pastRunTitle`, iOS `PastRuns.title`, Android
/// `pastRunTitle`, all locked by `a row titles itself from whatever it has`.
fn session_title(
    session: &domain::rows::CodingSession,
    issue: Option<&domain::rows::Issue>,
) -> SharedString {
    if let Some(issue) = issue {
        let title = issue.title.trim();
        return SharedString::from(if title.is_empty() {
            "Untitled issue".to_string()
        } else {
            title.to_string()
        });
    }
    if session.issue_id.is_some() {
        return SharedString::from("Issue syncing…");
    }
    match session.action_name.as_deref() {
        Some(name) if !name.trim().is_empty() => SharedString::from(name.to_string()),
        _ => SharedString::from("Batch run"),
    }
}

/// A Running row's caption: where it runs, what it is doing, since when.
/// Parts the row cannot prove are dropped rather than guessed.
fn running_caption(
    device_label: Option<&str>,
    display: CodingSessionDisplay,
    paused: bool,
    started_at: Option<&str>,
    now_epoch: i64,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(label) = device_label.map(str::trim).filter(|label| !label.is_empty()) {
        parts.push(label.to_string());
    }
    parts.push(
        if paused {
            // EXP-550: an offline host means the run is parked, not dead.
            "Paused"
        } else {
            match display {
                CodingSessionDisplay::Running => "Running",
                CodingSessionDisplay::NeedsInput => "Needs input",
                CodingSessionDisplay::Review => "In review",
                CodingSessionDisplay::Done => "Done",
            }
        }
        .to_string(),
    );
    if let Some(at) = started_at {
        let when = crate::comments::relative_time(at, now_epoch);
        if !when.is_empty() {
            parts.push(when);
        }
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-862: the Agent page's empty Running band says exactly this, on all
    /// four clients.
    #[test]
    fn the_empty_running_band_copy_is_locked() {
        assert_eq!(NO_RUNNING_COPY, "No agents running right now.");
    }

    /// A live row says where it runs, what it is doing and since when — and
    /// says nothing it cannot prove (an unknown machine drops the name, a row
    /// without a start stamp drops the time).
    #[test]
    fn a_running_caption_names_the_machine_and_the_state() {
        let now = 1_700_000_000;
        let started = chrono::DateTime::from_timestamp(now - 120, 0)
            .expect("timestamp")
            .to_rfc3339();
        assert_eq!(
            running_caption(
                Some("Studio"),
                CodingSessionDisplay::Running,
                false,
                Some(&started),
                now
            ),
            "Studio · Running · 2 minutes ago"
        );
        // EXP-550: the offline host wins over the display word.
        assert_eq!(
            running_caption(
                Some("Studio"),
                CodingSessionDisplay::NeedsInput,
                true,
                None,
                now
            ),
            "Studio · Paused"
        );
        assert_eq!(
            running_caption(None, CodingSessionDisplay::Review, false, None, now),
            "In review"
        );
    }

    /// The subject line falls back the way the dock chip always did: the
    /// issue's title, a sync placeholder while the issue is missing, the
    /// action's name, else the batch. Byte-identical ×4 (web `pastRunTitle`,
    /// iOS `PastRuns.title`, Android `pastRunTitle`) so Running and Past name
    /// the same run the same way on every client.
    #[test]
    fn a_row_titles_itself_from_whatever_it_has() {
        let row = |value: serde_json::Value| -> domain::rows::CodingSession {
            serde_json::from_value(value).expect("row")
        };
        let issue = |title: &str| -> domain::rows::Issue {
            serde_json::from_value(serde_json::json!({
                "id": "i-1",
                "board_id": "b-1",
                "identifier": "EXP-1",
                "number": 1,
                "title": title,
                "status": "in_progress",
                "priority": "none",
            }))
            .expect("issue")
        };
        let scoped = row(serde_json::json!({ "id": "s-0", "issue_id": "i-1" }));
        assert_eq!(
            session_title(&scoped, Some(&issue("Fix the sync loop"))),
            SharedString::from("Fix the sync loop")
        );
        assert_eq!(
            session_title(&scoped, Some(&issue("  "))),
            SharedString::from("Untitled issue")
        );
        let batch = row(serde_json::json!({ "id": "s-1" }));
        assert_eq!(session_title(&batch, None), SharedString::from("Batch run"));
        let action = row(serde_json::json!({ "id": "s-2", "action_name": "Release train" }));
        assert_eq!(
            session_title(&action, None),
            SharedString::from("Release train")
        );
        // A chat run carries "Chat" as its action snapshot (EXP-615), so no
        // client sniffs the `exp/chat-` branch for a name.
        let chat = row(serde_json::json!({
            "id": "s-4", "action_name": "Chat", "branch": "exp/chat-1a2b3c4d"
        }));
        assert_eq!(session_title(&chat, None), SharedString::from("Chat"));
        let syncing = row(serde_json::json!({ "id": "s-3", "issue_id": "i-1" }));
        assert_eq!(
            session_title(&syncing, None),
            SharedString::from("Issue syncing…")
        );
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
