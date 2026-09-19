//! EXP-746/EXP-923 — MY agent runs, as two lists.
//!
//! **Running** (the user's live sessions, here and on every other machine)
//! moved OUT of a rendered section and into the RAIL (EXP-923): a live run is
//! navigation now, not a document, so it lives in the sidebar beside the
//! boards and never opens a top tab. What is left here is its data half,
//! [`rail_running_rows`] — the one projection (remote ∪ local, nested) the
//! rail draws.
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
//! EXP-827: both NEST — a run started by another run through
//! `exponential_sessions_start` sits under it
//! ([`domain::session_tree::nest_sessions`], the rail's rule) behind a fold
//! chevron, so a parent's subtree can be collapsed out of the way. EXP-965:
//! the nesting draws a tree connector ([`domain::tree_guides`]).

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
    /// EXP-827: where the row sits in the session TREE.
    pub(crate) depth: usize,
    pub(crate) has_children: bool,
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

/// The user's live sessions: the ones on OTHER machines
/// ([`queries::remote_session_rows`], the retired dock's projection) union the
/// ones this process hosts, newest start first and NESTED by
/// `parent_session_id`.
///
/// Not team-scoped, deliberately: "my live sessions" is one list, the way the
/// dock's strip always read it — a run on another team's board is still a run
/// of yours that is going right now.
fn live_run_tree<T>(
    cx: &mut App,
    build: impl Fn(&domain::rows::CodingSession, usize, bool, Option<&LocalSessionHost>, i64, &App) -> T,
) -> Vec<T> {
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
    tree.into_iter()
        .map(|tree_row| {
            let session = tree_row.session;
            let host = hosts.iter().find(|(id, _)| id == &session.id).map(|(_, host)| host);
            build(session, tree_row.depth, tree_row.has_children, host, now, cx)
        })
        .collect()
}

/// EXP-923 — the rail's Running rows. Derived per paint like the rail's other
/// live reads (its observers already cover sessions, devices and the local
/// host set; [`tick`] covers the clock).
pub(crate) fn rail_running_rows(cx: &mut App) -> Vec<RailRunRow> {
    live_run_tree(cx, |session, depth, has_children, host, now, cx| {
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
            depth,
            has_children,
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

/// One finished row, flattened off the collections like the rail's.
#[derive(Clone, PartialEq)]
struct PastRow {
    /// EXP-827: the session tree, exactly as Running nests it.
    depth: usize,
    has_children: bool,
    facts: PastRunFacts,
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
        // EXP-827: a finished sub-session nests under the run that started it,
        // the same rule Running and the rail use. `own_ended_runs`' order is
        // the ROOT order (newest end first).
        let tree = domain::session_tree::nest_sessions(
            rows,
            |session| session.id.as_str(),
            |session| session.parent_session_id.as_deref(),
            |session| session.started_at.as_deref(),
        );
        tree.into_iter()
            .map(|tree_row| PastRow {
                depth: tree_row.depth,
                has_children: tree_row.has_children,
                facts: run_rows::past_run_facts(tree_row.session, now, cx),
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
            |row| row.facts.session_id.as_str(),
            |row| row.depth,
        );
        // EXP-965: the connector, off the VISIBLE depth sequence.
        let guides =
            domain::tree_guides::guides_for(&rows.iter().map(|row| row.depth).collect::<Vec<_>>());
        let mut column = v_flex().min_w_0();
        let list_origin = self.list_origin.clone();
        for (index, row) in rows.iter().enumerate() {
            let open_id = row.facts.session_id.clone();
            let list_origin = list_origin.clone();
            let fold = fold_for(
                row.facts.session_id.clone(),
                row.has_children,
                &self.collapsed,
                cx,
            );
            let active = open_session.as_deref() == Some(row.facts.session_id.as_str());
            column = column.child(run_rows::render_past_run_row(
                PastRunSpec {
                    id_prefix: "past-run",
                    index,
                    guides: guides.get(index).cloned().unwrap_or_default(),
                    fold,
                    facts: row.facts.clone(),
                    // EXP-773: a plain link. The transcript and Resume live in
                    // the fullscreen session view now.
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
/// listener rather than a plain closure.
pub(crate) fn fold_for<V: Collapsible + 'static>(
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
