//! EXP-746/EXP-923 — MY agent work, as two lists. EXP-1284: the lists show
//! THREADS, not bare runs (a thread is one unit of work — its issues, its
//! runs, ONE Guide), split by the contract's `partition_thread_list`
//! ([`crate::threads::team_thread_lists`]):
//!
//! **Running** (the ACTIVE TEAM's threads with a live run of mine — EXP-1075)
//! lives in the RAIL (EXP-923): a live thread is navigation, not a document,
//! so the sidebar draws it (`sidebar::RailView::render_running_section`).
//!
//! **Recent** is the ×4 section (web/iOS/Android have their own): every other
//! thread of mine in the active team, newest activity first. EXP-923 moved it
//! behind the Agent page's history button; EXP-1192 made that panel a second
//! sidebar INSIDE the content card ([`RecentRunsNav`],
//! `navigation::SecondSidebar::RecentRuns`) — the composer's page is a
//! composer and nothing else. EXP-1246: a Recent row opens its thread with
//! the Recent origin ([`crate::navigation::recent_runs_origin`]), so the list
//! stays beside it.

use gpui::{
    div, App, AppContext as _, Entity, InteractiveElement as _, IntoElement, ParentElement,
    Render, Styled, Subscription, Task, Window,
};
use gpui_component::{scroll::ScrollableElement as _, v_flex, Icon};

use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, Navigation, Screen};
use crate::surface::glass_section_header;
use crate::threads::ThreadListRow;

/// EXP-862 — how often a list re-derives itself on the CLOCK. Both lists
/// render a liveness that expires with `last_seen_at`, which produces no
/// collection delta to observe.
const TICK: std::time::Duration = std::time::Duration::from_secs(5);

/// The empty Running band's copy, fixture `running-agents` (EXP-1274). The
/// DESKTOP has no empty state for it any more (EXP-923: the rail's Running
/// section is hidden outright while nothing runs); the phones still draw it.
#[allow(dead_code)]
const NO_RUNNING_COPY: &str = crate::empty_state_copy::RUNNING_AGENTS;

/// EXP-923 — the number of Recent rows the history panel keeps. It is a
/// backstop behind a composer, not an archive.
const RECENT_CAP: usize = 20;

/// EXP-1192 — the Recent band's empty line: the panel is a column of its own
/// now, and a bare band over nothing read as still loading. Fixture
/// `recent-runs`, the shared LIST empty state (EXP-1274).
const NO_RECENT_COPY: &str = crate::empty_state_copy::RECENT_RUNS;

// ---------------------------------------------------------------------------
// Session trees (an action's Runs)
// ---------------------------------------------------------------------------

/// EXP-996 — one flattened row of a session TREE: a built run row at its
/// depth.
pub(crate) struct SessionTreeRow<T> {
    pub(crate) depth: usize,
    pub(crate) run: T,
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
    domain::session_tree::visible_session_tree_rows(&tree, &std::collections::HashSet::new())
        .into_iter()
        .map(|flat| SessionTreeRow {
            depth: flat.depth,
            run: build_run(flat.node),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Recent (the `Past*` names predate EXP-886's rename)
// ---------------------------------------------------------------------------

pub(crate) struct PastSessionsSection {
    nav: Entity<Navigation>,
    /// EXP-862/EXP-1284: the Recent THREADS, derived on a data change or the
    /// clock (EXP-832).
    rows: Vec<ThreadListRow>,
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

    /// EXP-1284: the Recent half of the active team's thread lists, capped
    /// (the list is newest first, so the cut takes the oldest).
    fn derive(nav: &Entity<Navigation>, cx: &mut App) -> Vec<ThreadListRow> {
        let Some(team_id) = active_team_id(nav, cx) else {
            return Vec::new();
        };
        let (_, mut recent) = crate::threads::team_thread_lists(&team_id, cx);
        recent.truncate(RECENT_CAP);
        recent
    }
}

impl Render for PastSessionsSection {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if self.rows.is_empty() {
            // EXP-1192: the band stays — it is the panel's only title.
            return v_flex()
                .min_w_0()
                .child(glass_section_header("Recent", None, cx))
                .child(crate::controls::empty_state_list(
                    Icon::new(registry::CODING_ENDED),
                    NO_RECENT_COPY,
                    cx,
                ));
        }
        let open_thread = open_thread_id(window, cx);
        let mut column = v_flex().min_w_0().gap_0p5();
        for (index, row) in self.rows.iter().enumerate() {
            let active = open_thread.as_deref() == Some(row.thread_id.as_str());
            // EXP-1246: list-detail like the Inbox — the thread opens to the
            // right and this list stays beside it.
            column = column.child(crate::thread_screen::thread_list_row(
                "past-thread",
                index,
                row,
                row.display(),
                active,
                false,
                crate::thread_screen::ThreadRowOpen::List(crate::navigation::recent_runs_origin()),
                cx,
            ));
        }
        // ×4 copy: the section is "Recent" on every client (EXP-886).
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
/// clock): the rows carry a liveness that expires, which the collections do
/// not signal. The refresh short-circuits on unchanged rows, so a quiet list
/// costs one derivation and no repaint.
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

/// The thread the window is SHOWING (or the thread of the run it shows), so
/// its row can wear the selected paint.
fn open_thread_id(window: &Window, cx: &mut App) -> Option<String> {
    let nav = nav_for_window(window, cx);
    match crate::navigation::resolved_screen(&nav, cx) {
        Some(Screen::Thread { thread_id }) => Some(thread_id),
        Some(Screen::Session { session_id }) => crate::threads::thread_of_session(&session_id, cx),
        _ => None,
    }
}

/// The Recent list joins `threads` ⨝ `thread_items` with the runs and issues
/// they hold, so every one of those deltas RE-DERIVES it (EXP-862: the rows
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
        // EXP-1284: the threads and their members.
        cx.observe(&collections.threads, move |this, _, cx| refresh(this, cx)),
        cx.observe(&collections.thread_items, move |this, _, cx| refresh(this, cx)),
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
        assert_eq!(NO_RUNNING_COPY, "No agents running");
        assert_eq!(NO_RECENT_COPY, "No recent sessions");
    }
}
