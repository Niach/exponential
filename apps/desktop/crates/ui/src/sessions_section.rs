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
use gpui_component::{v_flex, ActiveTheme as _};

use crate::coding_flow::{LocalSessionHost, LocalSessions};
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
                    lead: RunRowLead::Live(row.tone),
                    identifier: row.identifier.clone(),
                    title: row.title.clone(),
                    caption: row.caption.clone(),
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
    identifier: Option<SharedString>,
    title: SharedString,
    byline: SharedString,
    agent: Option<coding::CodingAgent>,
}

pub(crate) struct PastSessionsSection {
    nav: Entity<Navigation>,
    _subscriptions: Vec<Subscription>,
}

impl PastSessionsSection {
    pub(crate) fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let mut subscriptions = watch_run_collections(cx);
        subscriptions.push(cx.observe(&nav, |_, _, cx| cx.notify()));
        Self {
            nav,
            _subscriptions: subscriptions,
        }
    }

    fn rows(&self, cx: &mut App) -> Vec<PastRow> {
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
        rows.into_iter()
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
                }
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
            let open_id = row.session_id.clone();
            column = column.child(run_rows::render_run_row(
                RunRowSpec {
                    id_prefix: "past-run",
                    index,
                    lead: RunRowLead::Agent(row.agent),
                    identifier: row.identifier.clone(),
                    title: row.title.clone(),
                    caption: Some(row.byline.clone()),
                    // EXP-773: a plain link. The transcript, the summary and
                    // Resume all live in the fullscreen session view now.
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
pub(crate) fn session_tone(display: CodingSessionDisplay, paused: bool, muted: Hsla) -> Hsla {
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
