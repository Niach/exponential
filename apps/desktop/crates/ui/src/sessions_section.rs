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
//! empty state, and two empty headers under it read as breakage.

use std::collections::HashSet;

use gpui::{
    App, Entity, Hsla, IntoElement, ParentElement, Render, SharedString, Styled, Subscription,
    Window,
};
use gpui_component::{button::ButtonVariant, v_flex, ActiveTheme as _};

use crate::coding_flow::{LocalSessionHost, LocalSessions};
use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, nav_for_window, Navigation};
use crate::queries::{self, CodingSessionDisplay};
use crate::run_rows::{self, RunRowKill, RunRowLead, RunRowSpec};
use crate::surface::glass_section_header;

// ---------------------------------------------------------------------------
// Running
// ---------------------------------------------------------------------------

/// One live row, flattened off the synced collections so the render pass owns
/// everything it draws (the collection read borrows `cx` immutably; the row
/// callbacks need it mutably).
struct RunningRow {
    session_id: String,
    /// The synced row itself — the card takes it for its id and (never, on a
    /// live row) its summary.
    session: domain::rows::CodingSession,
    identifier: Option<SharedString>,
    title: SharedString,
    caption: Option<SharedString>,
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

pub(crate) struct RunningSessionsSection {
    _subscriptions: Vec<Subscription>,
}

impl RunningSessionsSection {
    /// Not team-scoped, deliberately: "my live sessions" is one list, the way
    /// the dock's strip always read it — a run on another team's board is
    /// still a run of yours that is going right now.
    pub(crate) fn new(cx: &mut gpui::Context<Self>) -> Self {
        Self {
            _subscriptions: watch_run_collections(cx),
        }
    }

    /// The user's live sessions: the ones on OTHER machines
    /// ([`queries::remote_session_rows`], the dock's projection) union the
    /// ones this process hosts. Newest start first, so the strip order and
    /// this list agree.
    fn rows(&self, cx: &mut App) -> Vec<RunningRow> {
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

        let issues = collections.issues.read(cx);
        let devices = collections.devices.read(cx);
        let theme = cx.theme();
        rows.into_iter()
            .map(|session| {
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
                RunningRow {
                    session_id: session.id.clone(),
                    session: session.clone(),
                    identifier: issue.map(|issue| SharedString::from(issue.identifier.clone())),
                    title: session_title(session, issue),
                    caption: Some(SharedString::from(running_caption(
                        presentation.label.as_deref(),
                        display,
                        paused,
                        run_rows::run_started_at(session),
                        now,
                    ))),
                    tone: session_tone(display, paused, theme.muted_foreground),
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

    /// The confirm before a live run is ended. A run this process hosts stops
    /// through its [`LocalSessionHost`] (the same path the issue header's stop
    /// takes); anything else goes out as `steer.killSession`.
    fn prompt_kill(
        row_local: Option<LocalSessionHost>,
        device_label: Option<String>,
        session_id: String,
        window: &mut Window,
        cx: &mut App,
    ) {
        let spec = match row_local {
            Some(host) => {
                // EXP-746: the copy names what actually happens — a run
                // without a terminal must not be promised one.
                let detail = if host.tab().is_some() {
                    "The agent stops immediately and the terminal tab closes. \
                     Uncommitted work in the worktree is kept."
                } else {
                    "The agent stops immediately. Uncommitted work in the worktree is kept."
                };
                AlertSpec::new("Stop this coding session?", detail, "Stop session").on_ok(
                    move |_, cx| {
                        host.stop(cx);
                        true
                    },
                )
            }
            None => AlertSpec::new(
                "Kill this coding session?",
                crate::steer_viewer::kill_description(device_label.as_deref()),
                "Kill session",
            )
            .ok_variant(ButtonVariant::Danger)
            .on_ok(move |_, cx| {
                crate::steer_viewer::kill_session(&session_id, cx);
                true
            }),
        };
        native_dialog::open_alert(window, cx, spec);
    }
}

impl Render for RunningSessionsSection {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let rows = self.rows(cx);
        if rows.is_empty() {
            return v_flex();
        }
        // NO gap on the headed section (EXP-697): the header's `pb_2` IS the
        // 8px to the list, so the rows live in their own gapped column.
        let mut column = v_flex().min_w_0().gap_2();
        for (index, row) in rows.into_iter().enumerate() {
            let open_id = row.session_id.clone();
            let kill = row.killable.then(|| {
                let session_id = row.session_id.clone();
                let local = row.local.clone();
                let device_label = row.device_label.clone();
                RunRowKill {
                    label: SharedString::from(if row.local.is_some() {
                        "Stop session"
                    } else {
                        "Kill session"
                    }),
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
            column = column.child(run_rows::render_run_row(
                RunRowSpec {
                    id_prefix: "running-run",
                    index,
                    session: &row.session,
                    lead: RunRowLead::Live(row.tone),
                    identifier: row.identifier.clone(),
                    title: row.title.clone(),
                    caption: row.caption.clone(),
                    expandable: false,
                    expanded: false,
                    resumable: false,
                    on_toggle: Box::new(|_, _, _| {}),
                    on_resume: Box::new(|_, _, _| {}),
                    on_open: Some(Box::new(move |_, window, cx| {
                        crate::session_screen::open_session(&open_id, window, cx);
                    })),
                    kill,
                },
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
struct PastRow {
    session_id: String,
    session: domain::rows::CodingSession,
    identifier: Option<SharedString>,
    title: SharedString,
    byline: SharedString,
    agent: Option<coding::CodingAgent>,
    resumable: bool,
}

/// EXP-758: the run registry's file, as a cache key. Both halves, because a
/// same-second rewrite can leave the mtime untouched (HFS+/APFS resolution and
/// the `rename` in `run_registry::save` both hide it) while the length moves.
type RegistryStamp = (std::time::SystemTime, u64);

/// EXP-758: which of this list's runs can be resumed, cached on the registry
/// file's stamp.
///
/// [`crate::coding_flow::run_is_resumable_ref`] re-reads and re-PARSES
/// `runs.json` once per row, and `rows()` runs from `render`, so a Past list
/// of 30 runs parsed the file 30 times on every repaint (scroll, hover, an
/// unrelated Electric edge). The file only changes when a run is recorded or
/// pruned, so the answers stand until its stamp moves.
///
/// Memoized PER SESSION rather than as one set, because the row list moves
/// without the file: a run whose row flips to `ended` was recorded at LAUNCH,
/// so it joins Past on an unchanged registry and still has to be answered.
///
/// Pure (the resolver is injected by `rows()`), so the invalidation matrix is
/// unit-tested without a data dir.
#[derive(Default)]
pub(crate) struct ResumableCache {
    /// `None` = nothing cached yet. A registry file that cannot be stamped
    /// (missing, or a `stat` that failed) never caches: [`Self::revalidate`]
    /// empties it on every pass, which is exactly the old behaviour.
    stamp: Option<RegistryStamp>,
    known: std::collections::HashMap<String, bool>,
}

impl ResumableCache {
    /// Point the cache at the registry file stamped `stamp`, forgetting
    /// everything it answered for a different one. `true` when the memo
    /// survived.
    pub(crate) fn revalidate(&mut self, stamp: Option<RegistryStamp>) -> bool {
        let kept = stamp.is_some() && self.stamp == stamp;
        if !kept {
            self.known.clear();
        }
        self.stamp = stamp;
        kept
    }

    /// The remembered answer for `session_id`, if this stamp has one.
    pub(crate) fn get(&self, session_id: &str) -> Option<bool> {
        self.known.get(session_id).copied()
    }

    /// Remember a freshly resolved answer. A no-op when the file could not be
    /// stamped: an answer with no key to invalidate it would go stale.
    pub(crate) fn remember(&mut self, session_id: String, resumable: bool) {
        if self.stamp.is_some() {
            self.known.insert(session_id, resumable);
        }
    }
}

/// EXP-758: the registry file's stamp, or `None` when it cannot be read.
///
/// The path is `run_registry`'s own (`<data_dir>/runs.json`); the crate keeps
/// it private and exposes no stamp accessor, so this mirrors it. A rename
/// there degrades this to "never fresh", i.e. back to a per-row resolve, never
/// to a stale answer.
fn registry_stamp(data_dir: &std::path::Path) -> Option<RegistryStamp> {
    let meta = std::fs::metadata(data_dir.join("runs.json")).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

pub(crate) struct PastSessionsSection {
    nav: Entity<Navigation>,
    /// EXP-637: rows whose summary is open (collapsed by default). Per-view
    /// and unpersisted; keyed by session row id — the Automations run log's
    /// rule, so the two lists behave the same.
    expanded: HashSet<String>,
    /// EXP-758: the per-repaint registry re-parse, removed.
    resumable: ResumableCache,
    _subscriptions: Vec<Subscription>,
}

impl PastSessionsSection {
    pub(crate) fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let mut subscriptions = watch_run_collections(cx);
        subscriptions.push(cx.observe(&nav, |_, _, cx| cx.notify()));
        Self {
            nav,
            expanded: HashSet::new(),
            resumable: ResumableCache::default(),
            _subscriptions: subscriptions,
        }
    }

    fn rows(&mut self, cx: &mut App) -> Vec<PastRow> {
        let Some(me) = queries::active_account(cx).map(|account| account.user_id) else {
            return Vec::new();
        };
        let Some(team_id) = active_team_id(&self.nav, cx) else {
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
        let issues = collections.issues.read(cx);
        let devices = collections.devices.read(cx);
        let rows: Vec<PastRow> = rows
            .into_iter()
            .map(|session| {
                let issue = session
                    .issue_id
                    .as_deref()
                    .and_then(|issue_id| issues.get(issue_id));
                let presentation =
                    queries::session_device_presentation(session, devices.iter(), now * 1_000);
                PastRow {
                    session_id: session.id.clone(),
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
                    session: session.clone(),
                    resumable: false,
                }
            })
            .collect();
        // The registry read wants `&App` again after the collection borrows
        // are done — EXP-637: only a run THIS machine recorded can be resumed
        // here (the workspace is local); one from another device shows its
        // summary and nothing else.
        //
        // EXP-758: resolved once per row per registry STAMP, not once per row
        // per repaint.
        self.resumable
            .revalidate(registry_stamp(&crate::coding_flow::coding_data_dir(cx)));
        rows.into_iter()
            .map(|mut row| {
                row.resumable = match self.resumable.get(&row.session_id) {
                    Some(known) => known,
                    None => {
                        let resolved =
                            crate::coding_flow::run_is_resumable_ref(&row.session_id, cx);
                        self.resumable.remember(row.session_id.clone(), resolved);
                        resolved
                    }
                };
                row
            })
            .collect()
    }
}

impl Render for PastSessionsSection {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let rows = self.rows(cx);
        if rows.is_empty() {
            return v_flex();
        }
        let mut column = v_flex().min_w_0().gap_2();
        for (index, row) in rows.iter().enumerate() {
            let toggle_id = row.session_id.clone();
            let resume_id = row.session_id.clone();
            let open_id = row.session_id.clone();
            column = column.child(run_rows::render_run_row(
                RunRowSpec {
                    id_prefix: "past-run",
                    index,
                    session: &row.session,
                    lead: RunRowLead::Agent(row.agent),
                    identifier: row.identifier.clone(),
                    title: row.title.clone(),
                    caption: Some(row.byline.clone()),
                    // Every row here has ended, so every row can open its
                    // summary.
                    expandable: true,
                    expanded: self.expanded.contains(&row.session_id),
                    resumable: row.resumable,
                    on_toggle: Box::new(cx.listener(move |this: &mut Self, _, _, cx| {
                        if !this.expanded.insert(toggle_id.clone()) {
                            this.expanded.remove(&toggle_id);
                        }
                        cx.notify();
                    })),
                    on_resume: Box::new(move |_, window, cx| {
                        // The ONE desktop resume entry point: the transport
                        // comes from the recorded run, never from the setting.
                        crate::action_run::resume_run(
                            resume_id.clone(),
                            Some(window.window_handle()),
                            false,
                            coding::LaunchOrigin::Local,
                            cx,
                        );
                    }),
                    // The card opens the run: an ACP record replays its
                    // transcript, anything else opens read-only.
                    on_open: Some(Box::new(move |_, window, cx| {
                        crate::session_screen::open_session(&open_id, window, cx);
                    })),
                    kill: None,
                },
                cx,
            ));
        }
        v_flex()
            .min_w_0()
            .mt_6()
            // ×4 copy: the section is "Past" on every client.
            .child(glass_section_header("Past", None, cx))
            .child(column)
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Both sections join `coding_sessions` with the issues they name and the
/// devices they ran on, so all three deltas repaint them.
fn watch_run_collections<V: 'static>(cx: &mut gpui::Context<V>) -> Vec<Subscription> {
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections().clone();
    vec![
        cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify()),
        cx.observe(&collections.issues, |_, _, cx| cx.notify()),
        cx.observe(&collections.devices, |_, _, cx| cx.notify()),
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

/// The live dot's tone, mirroring the web tab's dot rules (and the dock chip's
/// twin, which D6 removes).
fn session_tone(display: CodingSessionDisplay, paused: bool, muted: Hsla) -> Hsla {
    if paused {
        return muted.opacity(0.4);
    }
    match display {
        CodingSessionDisplay::NeedsInput => theme::tokens::YELLOW.to_hsla(),
        CodingSessionDisplay::Done => theme::tokens::BLUE.to_hsla(),
        CodingSessionDisplay::Review | CodingSessionDisplay::Running => {
            theme::tokens::GREEN.to_hsla()
        }
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

    fn stamp(secs: u64, len: u64) -> RegistryStamp {
        (
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs),
            len,
        )
    }

    /// EXP-758: Past resolved resumability by re-parsing `runs.json` once per
    /// row per repaint. The memo survives repaints, dies with the file's
    /// stamp, and answers a row that joined the list on an unchanged file.
    #[test]
    fn the_resumable_memo_lives_and_dies_with_the_registry_stamp() {
        let mut cache = ResumableCache::default();
        // First pass: nothing is known yet.
        assert!(!cache.revalidate(Some(stamp(10, 400))));
        assert_eq!(cache.get("run-a"), None);
        cache.remember("run-a".to_string(), true);
        // A repaint over the same file answers from the memo.
        assert!(cache.revalidate(Some(stamp(10, 400))));
        assert_eq!(cache.get("run-a"), Some(true));
        // A row that appeared without the file moving is still resolved (a
        // run is recorded at LAUNCH, so its Past row arrives later).
        assert_eq!(cache.get("run-b"), None);
        cache.remember("run-b".to_string(), false);
        assert_eq!(cache.get("run-b"), Some(false));
        // A rewrite that kept the mtime but changed the length invalidates…
        assert!(!cache.revalidate(Some(stamp(10, 512))));
        assert_eq!(cache.get("run-a"), None);
        // …as does a new mtime.
        cache.remember("run-a".to_string(), true);
        assert!(!cache.revalidate(Some(stamp(11, 512))));
        assert_eq!(cache.get("run-a"), None);
        // An unstampable file (no registry yet) never caches at all: every
        // pass resolves, exactly as before the memo existed.
        assert!(!cache.revalidate(None));
        cache.remember("run-a".to_string(), true);
        assert_eq!(cache.get("run-a"), None);
        assert!(!cache.revalidate(None));
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
}
