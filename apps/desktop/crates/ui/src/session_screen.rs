//! The coding-session center screen (EXP-746) — one open [`Screen::Session`]
//! tab per `coding_sessions` row.
//!
//! Sessions used to live in the bottom terminal dock: a PTY tab for a run this
//! process hosts, a "remote chip" beside it for one on another machine. The
//! ACP engine ends that split. A run this process hosts is an in-process
//! [`engine::EngineSession`], not a terminal, so it renders in the CENTER pane
//! like every other detail screen. EXP-769: a PTY-hosted run renders in the
//! center too (`Screen::Terminal`), and both kinds tab in the bottom session
//! bar.
//!
//! [`open_session`] is the ONE entry point (the start dialog, the issue's
//! coding-now card, Devices → Running / Past, a Sessions row in the rail).
//! EXP-791: an ISSUE-bound run opens INSIDE its issue's detail — the
//! transcript slides in over the issue (`navigation::navigate_steering`,
//! `IssueDetailView::open_steering`) instead of leaving the page; every
//! other run (batch, action, chat) navigates here.
//!
//! Three feed sources, one renderer ([`SteerSessionView`]): the in-process
//! engine (`Local`), the relay viewer (`Remote` — another machine, or another
//! process on this one), and an ended ACP run replayed off disk (`Replay`).
//! [`feed_source_for`] is that decision, pure and unit-tested; materializing
//! it is [`resolve_source`].
//!
//! This screen owns the CHROME around that transcript — the EXP-850 §10
//! header: the identity block, CENTERED (EXP-863: no Back — the sidebar
//! carries navigation), then the read-only Plan chip, Pin, Context, Diff,
//! Merge and Stop in a right-aligned group; under it, for an issue-bound
//! run, the issue's own header (EXP-863: `IssueHeader`'s rows over a
//! read-only title, with "Open issue" in the tray's trailing slot). The
//! transcript view keeps everything that
//! is about the conversation itself (the feed, the banners, the diff pane the
//! Diff pill toggles, the composer), which is why it renders headerless here
//! ([`SteerSessionView::set_chrome`]).
//!
//! Lifetime rule: a view lives exactly as long as its TAB. `ScreensPanel`
//! creates it on first activation and calls [`SessionScreenView::shutdown`]
//! when the tab closes — which drops the feed, never the run: closing a tab
//! must not end an agent (kill is its own affordance). An ended run keeps its
//! tab as a read-only transcript, which is what [`SessionScreenView::mark_ended`]
//! marks when the engine reports the exit before the row syncs.

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, AppContext as _, Entity, FocusHandle,
    Focusable, InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, notification::Notification, popover::Popover, v_flex,
    ActiveTheme as _, Icon, Sizable as _, WindowExt as _,
};

use crate::account_switch::SwitchContext;
use crate::coding_flow::{LocalSessions, StartCodingControl};
use crate::controls::WebText as _;
use crate::icons::registry;
use crate::issue_header::IssueHeader;
use crate::navigation::Screen;
use crate::steer_viewer::{FeedSource, SteerSessionView};

/// Open `session_id`'s screen in this window (EXP-773: a coding run has no
/// terminal tab). EXP-818: ALWAYS its own [`Screen::Session`] — Start,
/// Resume, Watch and a Sessions row all land here, and the list column
/// beside it is the one the caller came from (`navigation::derive_origin`);
/// the EXP-791 slide-in over the issue is gone.
pub(crate) fn open_session(session_id: &str, window: &mut Window, cx: &mut App) {
    open_session_inner(session_id, Origin::Derive, window, cx);
}

/// EXP-851: [`open_session`] from a RAIL row (the Sessions section, a pinned
/// run) — the rail is not a list, so the run opens with no `ListNav` beside
/// it instead of inheriting whatever the main view was showing.
pub(crate) fn open_session_from_rail(session_id: &str, window: &mut Window, cx: &mut App) {
    open_session_inner(session_id, Origin::Rail, window, cx);
}

/// EXP-862: [`open_session`] from a LIST, which pins that list explicitly —
/// the run's Back and its left column then name the rows it was picked from
/// (the Agent page's sessions list, the Automations page's run log) instead
/// of whatever the breadcrumb rule can derive from the screen that was up.
/// `None` falls back to [`open_session`].
pub(crate) fn open_session_with_origin(
    session_id: &str,
    origin: Option<crate::navigation::TabOrigin>,
    window: &mut Window,
    cx: &mut App,
) {
    match origin {
        Some(origin) => open_session_inner(session_id, Origin::List(origin), window, cx),
        None => open_session(session_id, window, cx),
    }
}

/// Which list (if any) the opened run is pinned beside.
enum Origin {
    /// Let the EXP-851 breadcrumb rule work it out from the screen we leave.
    Derive,
    /// Opened from the rail: no list at all.
    Rail,
    /// Opened from a list that names itself.
    List(crate::navigation::TabOrigin),
}

fn open_session_inner(
    session_id: &str,
    origin: Origin,
    window: &mut Window,
    cx: &mut App,
) {
    // EXP-746 D5: a resume mints a NEW row id, so opening it plainly would put
    // a second tab beside the run it continues. `screens::sync_session_tabs`
    // reads that link off the synced row, but a LOCAL resume gets here first
    // — `engine::start` returns as soon as the thread is spawned while the
    // row's Electric echo is a network round trip — so the swap has to run at
    // open time too. Both call the same rule, and the loser finds the tab
    // already renamed.
    if let Some(resumed_from) = resumed_from_id(session_id, cx) {
        crate::screens::take_over_session_tab(&resumed_from, session_id, window, cx);
    }
    let screen = Screen::Session {
        session_id: session_id.to_string(),
    };
    match origin {
        Origin::Rail => crate::navigation::navigate_from_rail(window, cx, screen),
        Origin::List(origin) => crate::navigation::navigate_from(window, cx, screen, origin),
        Origin::Derive => crate::navigation::navigate(window, cx, screen),
    }
}

/// The run `session_id` continues (EXP-662 `resumed_from_id`), read off this
/// device's run registry — the local half of the link the synced row carries,
/// written by `prepare_resume_run` before the launch ever reaches this window.
/// `None` for a fresh run, and for a session hosted on another machine.
fn resumed_from_id(session_id: &str, cx: &App) -> Option<String> {
    coding::run_registry::get(&crate::coding_flow::coding_data_dir(cx), session_id)
        .and_then(|record| record.resumed_from_id)
}

/// Mark every open screen for `session_id` ended (the engine's `on_exit` edge,
/// which can precede the synced row's flip by a round trip). The tab stays.
///
/// EXP-758: `error` is `EngineExit::error`, a handshake or transport failure.
/// It used to be logged and dropped, so a run that never got past
/// `initialize` was an empty tab whose only word was "ended".
pub(crate) fn mark_ended(session_id: &str, error: Option<String>, cx: &mut App) {
    for view in crate::screens::session_views(session_id, cx) {
        let error = error.clone();
        view.update(cx, |view, cx| view.mark_ended(error, cx));
    }
}

/// Which feed drives a session screen (D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionFeed {
    /// An ACP run this process hosts: the engine's own local feed, with the
    /// rich items (diff cards, command output, the pinned plan, thoughts) the
    /// wire never carries.
    Local,
    /// A finished ACP run replayed off its recorded transcript. Read-only.
    Replay,
    /// The relay viewer — a run on another machine, another process here, or
    /// one this machine kept no transcript of.
    Remote,
}

/// The source decision, pure so it can be tested without an engine or a store.
///
/// Order matters: a LIVE local engine wins over everything (replaying a run
/// that is still writing would show a stale transcript beside a live one), and
/// a replay is only ever offered for an ENDED run THIS machine kept a
/// transcript of — anything else belongs on the relay.
///
/// `has_transcript` is EXP-773's question: an on-device activity journal, or
/// (for a run recorded before journals existed) a run-registry record the
/// engine can re-open. It used to ask for the recorded launch TRANSPORT, back
/// when a PTY-hosted run had no ACP transcript to load at all.
pub(crate) fn feed_source_for(
    local_engine: bool,
    has_transcript: bool,
    row_ended: bool,
) -> SessionFeed {
    if local_engine {
        return SessionFeed::Local;
    }
    if row_ended && has_transcript {
        return SessionFeed::Replay;
    }
    SessionFeed::Remote
}

/// [`feed_source_for`] against the live state, then the source it names.
fn resolve_source(session_id: &str, cx: &mut App) -> (SessionFeed, FeedSource) {
    let engine = local_engine(session_id, cx);
    let record =
        coding::run_registry::get(&crate::coding_flow::coding_data_dir(cx), session_id);
    // EXP-773: the DEVICE's own activity journal is the transcript of a past
    // run — the exact events the relay fanned out, on disk, with no agent to
    // respawn. It is preferred for that reason; the engine's `session/load`
    // replay stays the fallback for a run recorded before journals existed.
    let journal = journal_events(session_id, cx);
    let feed = feed_source_for(
        engine.is_some(),
        journal.is_some() || record.is_some(),
        row_ended(session_id, cx),
    );
    match (feed, engine) {
        (SessionFeed::Local, Some(session)) => (SessionFeed::Local, FeedSource::Local { session }),
        (SessionFeed::Replay, _) => match journal {
            Some(events) => (SessionFeed::Replay, FeedSource::journal(events)),
            None => match record.and_then(|record| open_transcript(&record, cx)) {
                Some(session) => (SessionFeed::Replay, FeedSource::Replay { session }),
                // Neither a journal nor a loadable transcript (no steer
                // runtime, or the run never got as far as an id). The
                // transcript view then says so — an empty relay feed with no
                // explanation is the thing this must not become.
                None => (SessionFeed::Remote, FeedSource::Remote { handle: None }),
            },
        },
        _ => (SessionFeed::Remote, FeedSource::Remote { handle: None }),
    }
}

/// EXP-773 — this device's journal for `session_id`, folded and replay-ready
/// ([`steer::read_journal`]). `None` when the file is not there: this machine
/// never ran the session, or the 60-day prune took it, and the caller falls
/// back to the engine replay and then to the relay.
fn journal_events(session_id: &str, cx: &App) -> Option<Vec<steer::frames::ActivityEvent>> {
    let events = steer::read_journal(&crate::coding_flow::coding_data_dir(cx), session_id)?;
    (!events.is_empty()).then_some(events)
}

/// Replay `record`'s conversation through the engine (ACP `session/load`).
/// `None` when there is nothing to load it with — the caller then falls back
/// to the relay and the "no transcript" banner.
fn open_transcript(record: &coding::run_registry::RunRecord, cx: &App) -> Option<engine::EngineSession> {
    let runtime = crate::steer_wiring::runtime(cx)?;
    // EXP-862: a run an older build recorded on an external ACP agent has no
    // binary to replay here any more.
    if record.is_retired_external_agent() {
        return None;
    }
    let agent = record.agent;
    // The ACP id is the handle a replay wants; the agent-native ones are the
    // fallback for a record written before the handshake answered.
    let native = record
        .agent_native_session_id
        .clone()
        .or_else(|| record.claude_session_id.clone())
        .map(engine::ResumeHandle::Native)
        .or_else(|| {
            record
                .acp_session_id
                .clone()
                .map(engine::ResumeHandle::Acp)
        })?;
    engine::EngineSession::open_transcript(engine::OpenTranscript {
        runtime,
        data_dir: crate::coding_flow::coding_data_dir(cx),
        // REV2-17: a replay redacts exactly like a live run does.
        personal_key: crate::steer_wiring::personal_key(cx),
        handle: engine::HistoryHandle {
            agent,
            cwd: record.cwd.clone(),
            acp_session_id: record.acp_session_id.clone(),
            native,
        },
        // The screen reads the replay through `subscribe()` like every other
        // local source; the sink's presence is what asks the engine to build
        // the local events at all.
        local_sink: std::sync::Arc::new(|_| {}),
    })
    .map_err(|err| log::warn!("[ui] session replay unavailable: {err}"))
    .ok()
}

/// The in-process ACP engine hosting `session_id`, if this process is running
/// it.
fn local_engine(session_id: &str, cx: &App) -> Option<engine::EngineSession> {
    let sessions = LocalSessions::global_ref(cx)?;
    Some(sessions.read(cx).session_by_id(session_id)?.host.session.clone())
}

/// Whether the SYNCED row says the run is over.
fn row_ended(session_id: &str, cx: &App) -> bool {
    let Some(store) = sync::Store::try_global(cx) else {
        return false;
    };
    let sessions = store.collections().coding_sessions.read(cx);
    sessions
        .get(session_id)
        .and_then(|row| row.status.as_deref())
        .is_some_and(|status| status == domain::contract::CODING_SESSION_STATUS_ENDED)
}

/// EXP-800: where an ended run's Resume goes. `Local` re-enters the run
/// recorded in this machine's registry; `Remote` asks the machine that
/// hosted it over the steer rails (`steer.startSession({resumeSessionId})`),
/// the path the web and mobile clients already take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ResumePath {
    Local,
    Remote { device_id: String },
}

/// The ONE resume decision, pure so the table test can pin it.
///
/// A local record always wins. Without one, a run this install hosted is
/// NOT relaunched through the relay: the answer could only be "no local
/// record" again (a purged repo-less run, EXP-764). Any other machine takes
/// the resume when its synced row is online and advertises the
/// `resume-run` cap — the same two gates the web's `useCanResumeOn` reads,
/// and the ones the server enforces on `steer.startSession`.
pub(crate) fn resume_path_for<'a>(
    local_resumable: bool,
    own_device: bool,
    session: &domain::rows::CodingSession,
    devices: impl Iterator<Item = &'a domain::rows::DeviceRow>,
    now_ms: i64,
) -> Option<ResumePath> {
    if local_resumable {
        return Some(ResumePath::Local);
    }
    if own_device {
        return None;
    }
    let row = crate::queries::session_device_row(session, devices)?;
    let online = crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms);
    let can_resume = row
        .cap_ids()
        .iter()
        .any(|cap| cap == coding::doctor::RESUME_RUN_CAP);
    if !(online && can_resume) {
        return None;
    }
    Some(ResumePath::Remote {
        device_id: session.device_id.clone()?,
    })
}

/// One coding session's center screen.
pub(crate) struct SessionScreenView {
    session_id: String,
    /// The ONE transcript renderer, over whichever source [`resolve_source`]
    /// picked. It renders headerless: this screen paints the header.
    inner: Entity<SteerSessionView>,
    feed: SessionFeed,
    /// The engine reported its exit (D2). The synced row carries the same
    /// truth a round trip later — the inner view reads that one itself.
    ended: bool,
    /// EXP-773: whether this machine still holds the run's workspace, resolved
    /// ONCE (the registry parse is a file read; a session screen repaints on
    /// every feed event). `None` until the run is over — a live run offers no
    /// Resume.
    resumable: Option<bool>,
    /// EXP-800: this install's persistent device id, read off settings.json
    /// ONCE — the resume decision compares it against the row's `device_id`
    /// on every repaint.
    own_device_id: String,
    /// EXP-863: the issue band IS the issue's header (pin + `…` menu, the
    /// title, the property tray). Built lazily, once the synced row names an
    /// `issue_id`; `None` for a batch / action / chat run. The header owns its
    /// own `StartCodingControl` (its constructor wants one; the session
    /// screen never renders it — the tray's trailing slot is "Open issue").
    header: Option<Entity<IssueHeader>>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl SessionScreenView {
    /// The screen for `session_id`, over the source its live state names.
    pub(crate) fn new(
        session_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let (feed, source) = resolve_source(&session_id, cx);
        Self::build(session_id, feed, source, window, cx)
    }

    /// The REMOTE screen, unconditionally — what `build_screen_content` needs
    /// for a fresh instance in another window: it must never take a second
    /// handle on a live local engine (which is also why sessions are not
    /// undockable today).
    pub(crate) fn remote(
        session_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        Self::build(
            session_id,
            SessionFeed::Remote,
            FeedSource::Remote { handle: None },
            window,
            cx,
        )
    }

    fn build(
        session_id: String,
        feed: SessionFeed,
        source: FeedSource,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let inner = cx.new(|cx| {
            let mut view = SteerSessionView::with_source(session_id.clone(), source, window, cx);
            view.set_chrome(false);
            view
        });
        // The transcript notifies on every feed change — which is also when
        // the header's usage and status move.
        let mut subscriptions = vec![cx.observe(&inner, |_: &mut Self, _, cx| cx.notify())];
        // EXP-778: the header's pin toggle reads the per-user pins rows, which
        // move on no other feed this screen watches.
        if let Some(store) = sync::Store::try_global(cx) {
            let pins = store.collections().pins.clone();
            subscriptions.push(cx.observe(&pins, |_: &mut Self, _, cx| cx.notify()));
        }
        let own_device_id = crate::queries::own_device_id(cx);
        Self {
            session_id,
            inner,
            feed,
            ended: false,
            resumable: None,
            own_device_id,
            header: None,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// EXP-863: point the issue header at `issue_id`, creating it on first
    /// use. The header is an entity with its own collection subscriptions
    /// (labels, members, statuses, pins …); this screen observes it so a
    /// picker's `notify` repaints the band.
    fn ensure_header(
        &mut self,
        issue_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<IssueHeader> {
        let header = match self.header.clone() {
            Some(header) => header,
            None => {
                let start_coding = cx.new(StartCodingControl::new);
                let header = cx.new(|cx| IssueHeader::new(start_coding, window, cx));
                self._subscriptions
                    .push(cx.observe(&header, |_: &mut Self, _, cx| cx.notify()));
                self.header = Some(header.clone());
                header
            }
        };
        header.update(cx, |header, cx| {
            header.set_issue(Some(issue_id.to_string()), window, cx);
        });
        header
    }

    /// The tab closed. Drops the feed (the relay socket, the engine drain) —
    /// deliberately NOT the run: an agent keeps working when its tab goes
    /// away, and Past reopens the transcript.
    pub(crate) fn shutdown(&mut self, cx: &mut App) {
        self.inner.update(cx, |view, _| view.shutdown());
    }

    /// The run is over — the screen turns into a read-only transcript.
    ///
    /// This is the HOST's edge (the engine's `on_exit`), which can beat both
    /// the synced row and the engine's own feed closing; the transcript is
    /// told directly so its composer goes at the same moment the header's
    /// affordances do.
    ///
    /// EXP-758: a second call carrying an `error` is NOT a no-op. The feed
    /// closing can beat the host's exit, so the plain end can land first and
    /// the reason second.
    pub(crate) fn mark_ended(&mut self, error: Option<String>, cx: &mut gpui::Context<Self>) {
        if self.ended && error.is_none() {
            return;
        }
        self.ended = true;
        self.inner
            .update(cx, |view, cx| view.note_run_ended(error, cx));
        cx.notify();
    }

    // ── Header ────────────────────────────────────────────────────────────

    /// Whether the run is over — the engine's own exit edge, the transcript's
    /// phase, or the synced row.
    fn run_over(&self, cx: &App) -> bool {
        self.ended || self.inner.read(cx).session_over()
    }

    /// EXP-773 — this machine holds the run's record (a reclaimed worktree
    /// is re-created on resume), so the header may offer Resume. Resolved
    /// once per screen (the registry is a file).
    fn local_resumable(&mut self, cx: &App) -> bool {
        match self.resumable {
            Some(known) => known,
            None => {
                let known = crate::coding_flow::run_is_resumable_ref(&self.session_id, cx);
                self.resumable = Some(known);
                known
            }
        }
    }

    /// Where the header's Resume goes, or `None` while the run is live or no
    /// machine can take it. EXP-800: the local half is the once-per-screen
    /// cache above; the remote half is re-read per repaint (one pass over the
    /// synced `devices` rows — the inner viewer observes that collection, so
    /// a heartbeat edge repaints this header like the offline caption).
    fn resume_path(&mut self, cx: &App) -> Option<ResumePath> {
        if !self.run_over(cx) {
            return None;
        }
        let local = self.local_resumable(cx);
        let inner = self.inner.read(cx);
        let (Some(row), Some(store)) = (inner.session_row(), sync::Store::try_global(cx)) else {
            return local.then_some(ResumePath::Local);
        };
        let own_device = row.device_id.as_deref() == Some(self.own_device_id.as_str());
        resume_path_for(
            local,
            own_device,
            row,
            store.collections().devices.read(cx).iter(),
            chrono::Utc::now().timestamp_millis(),
        )
    }

    /// EXP-773 — the ended run's byline, the one its list row used to carry
    /// (machine and when, EXP-833). `None` while the row has not synced.
    fn ended_byline(&self, cx: &App) -> Option<SharedString> {
        let inner = self.inner.read(cx);
        let row = inner.session_row()?;
        let byline = crate::run_rows::past_run_byline(
            row,
            inner.device_label(cx).as_deref(),
            chrono::Utc::now().timestamp(),
        );
        (!byline.is_empty()).then(|| SharedString::from(byline))
    }

    /// EXP-849 — this run IS the continuation of an earlier one (an ordinary
    /// Resume, or a switch to another account): say so ONCE, with the
    /// transcript's one-time cost, so a second context-window charge on a new
    /// account is never a surprise. The ×4 sentences
    /// (`crate::account_switch`); the run it continues is reachable from the
    /// lists, which nest the chain.
    fn render_continuation(&mut self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        // The synced row is the authority (it covers a run this machine does
        // not host); the local registry answers a beat earlier, before the
        // resumed row's Electric echo lands.
        let continues = self
            .inner
            .read(cx)
            .session_row()
            .and_then(|row| row.resumed_from_id.clone())
            .or_else(|| resumed_from_id(&self.session_id, cx))
            .is_some_and(|id| !id.trim().is_empty());
        if !continues {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        Some(
            h_flex()
                .w_full()
                .flex_shrink_0()
                .min_w_0()
                .items_start()
                .gap_1p5()
                .px_3()
                .py_1p5()
                .border_b_1()
                .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
                .child(
                    Icon::new(registry::RUN_RESUME)
                        .xsmall()
                        .flex_shrink_0()
                        .text_color(muted),
                )
                .child(
                    v_flex()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(crate::account_switch::CONTINUATION_NOTE),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted.opacity(0.7))
                                .child(crate::account_switch::CONTINUATION_COST_NOTE),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_header(&mut self, cx: &mut gpui::Context<Self>) -> AnyElement {
        // EXP-778: the pin toggle needs the run's team; a local start ahead
        // of its synced echo has no row yet and simply shows no toggle.
        let pin_team_id = self
            .inner
            .read(cx)
            .session_row()
            .and_then(|row| row.team_id.clone());
        let muted = cx.theme().muted_foreground;
        // EXP-773: an ended run wears its list byline and, when this machine
        // still holds the workspace, the Resume the Past row used to carry.
        let resume = self.resume_path(cx);
        // EXP-800: the remote button names the host; the byline below moves
        // the label, so its copy is taken first.
        let resume_host = self
            .inner
            .read(cx)
            .device_label(cx)
            .unwrap_or_else(|| "its machine".to_string());
        let byline = self.run_over(cx).then(|| self.ended_byline(cx)).flatten();
        let inner = self.inner.read(cx);
        let (tone, caption) = inner.header_status(cx);
        let (identifier, subject) = inner.header_identity(cx);
        let device = inner.device_label(cx);
        let can_kill = inner.killable(cx);
        let usage = inner.usage();
        let usage_summary = crate::session_extras::context_summary(usage.as_ref());
        let agent = inner.builtin_agent();
        // EXP-847: the run is in PLAN mode right now — a read-only chip, never
        // a control (EXP-790: the mode is a launch-time choice). It clears
        // itself the moment an approved `ExitPlanMode` moves the mode on.
        let plan_mode = inner.plan_mode_label();
        let over = inner.session_over();
        let local = inner.is_local();
        let device_id = inner
            .session_row()
            .and_then(|row| row.device_id.clone());
        // EXP-849: read with the other `inner` facts — an account switch is
        // refused mid-turn, and taking it later would hold this borrow across
        // the settings read below.
        let working = inner.working_now();
        // EXP-850 §10: the Diff pill's totals (hidden without a diff) and
        // whether the pane is up; §11's pane lives inside the transcript view.
        let diff_totals = inner.diff_totals();
        let diff_open = inner.diff_open();
        // EXP-850 §10: the Merge control the Changes band used to carry — the
        // ONE merge-target rule, offered only while the run is live.
        let merge_target = inner.merge_target(cx);
        // The pill only exists for a LIVE run's own meter (see the header
        // below) — resolving the machine's windows for a header that will not
        // show them is a settings read and a jsonb parse per repaint.
        let shows_usage = usage_summary.is_some()
            && !over
            && !self.ended
            && self.feed != SessionFeed::Replay;
        // The HOST machine's rate-limit windows: this install's own probe for
        // a run we host, the synced `devices.agent_usage` for one we do not.
        let windows = agent.filter(|_| shows_usage).and_then(|agent| {
            if local {
                crate::device_settings::own_agent_status(cx)
                    .1
                    .get(agent.id())
                    .cloned()
            } else {
                device_usage(device_id.as_deref(), agent, cx)
            }
        });

        // EXP-849: the usage readout is also the ACCOUNT control — which login
        // this run is spending and, for claude between turns, which other one
        // it could continue on. Built here (it needs the store and the run
        // registry) and handed to the sheet as plain data.
        let switch = shows_usage
            .then(|| {
                agent.map(|agent| SwitchContext {
                    session_id: self.session_id.clone(),
                    device_id: device_id.clone(),
                    local,
                    agent,
                    working,
                })
            })
            .flatten();
        // The status caption already names the device when the transcript
        // knows it (`phase_label`); append it only when it does not. An ENDED
        // run reads its full byline instead (machine, agent, who ended it,
        // when).
        let caption = byline.unwrap_or_else(|| {
            SharedString::from(match device {
                Some(device) if !caption.contains(device.as_str()) => {
                    format!("{caption} · {device}")
                }
                _ => caption,
            })
        });
        // EXP-863: the identity is CENTERED like the web header — a spacer on
        // the left, the two-line identity column in the middle, the controls
        // right-aligned. The outer two share the slack equally; the right one
        // keeps its content width (no `min_w_0`), so on a narrow pane the
        // identity gives way before a control does.
        let identity = v_flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap_0p5()
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .justify_center()
                    .gap_1p5()
                    .child(
                        div()
                            .flex_shrink_0()
                            .size_1p5()
                            .rounded_full()
                            .bg(tone),
                    )
                    .when_some(identifier, |this, identifier| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(muted)
                                .font_family(theme::terminal::FONT_FAMILY)
                                .child(identifier),
                        )
                    })
                    // EXP-791: no agent pill — the agent is the byline's
                    // business (an ended run names it there), and the live
                    // caption's.
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(subject),
                    ),
            )
            .child(
                div()
                    .max_w_full()
                    .min_w_0()
                    .truncate()
                    .text_2xs()
                    .text_color(muted)
                    .child(caption),
            );
        h_flex()
            .w_full()
            .flex_shrink_0()
            .gap_2()
            .items_center()
            .px_2()
            .py_1p5()
            .border_b_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            .child(div().flex_1().min_w_0())
            .child(identity)
            .child(
                h_flex()
                    .flex_1()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    // EXP-847: plan mode, VISIBLE and read-only (EXP-790: the mode
                    // is a launch-time choice). EXP-863: the web's chip — a bare
                    // hairline box, no glyph, no fill, 11px muted text.
                    .when_some(plan_mode.filter(|_| !over), |this, label| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .rounded(px(theme::tokens::radius::SM))
                                .border_1()
                                .border_color(cx.theme().border.opacity(0.6))
                                .px_1p5()
                                .py_0p5()
                                .text_2xs()
                                .text_color(muted)
                                .whitespace_nowrap()
                                .child(label),
                        )
                    })
                    // EXP-778/EXP-850 §10: the personal pin toggle — a pinned run
                    // lands in the rail's Pinned section, live or ended. It sits
                    // between the Plan chip and the Context pill, and it is a GHOST
                    // button everywhere a pin renders.
                    .when_some(pin_team_id, |this, team_id| {
                        this.child(crate::pins::pin_toggle_button(
                            "session-pin",
                            team_id,
                            domain::contract::PIN_KIND_SESSION,
                            self.session_id.clone(),
                            cx,
                        ))
                    })
                    // EXP-746: the session's own context meter. Gone once the run is
                    // over (a finished run's live numbers are a snapshot of nothing)
                    // and never on a replay, whose numbers are the ones the run ended
                    // with, not the ones anything is spending now.
                    .when_some(
                        usage_summary.filter(|_| shows_usage),
                        |this, summary| {
                            let usage = usage;
                            this.child(
                                Popover::new("session-usage")
                                    .p_2()
                                    .trigger(
                                        crate::surface::glass_pill_button(
                                            "session-usage-pill",
                                            crate::surface::PillSize::Sm,
                                            cx,
                                        )
                                        .icon(registry::UI_USAGE)
                                        .label(SharedString::from(summary))
                                        .tooltip("Usage"),
                                    )
                                    .content(move |_, _window, cx| {
                                        render_usage_sheet(
                                            agent,
                                            usage.as_ref(),
                                            windows.as_ref(),
                                            switch.clone(),
                                            cx,
                                        )
                                    }),
                            )
                        },
                    )
                    // EXP-850 §10: the Diff pill — the `coding-diff` glyph with the
                    // run's `+N −M`, toggling the pane beside the transcript. Hidden
                    // when the run has published no diff at all.
                    .when_some(diff_totals, |this, (additions, deletions)| {
                        let inner = self.inner.clone();
                        this.child(
                            crate::surface::glass_pill(
                                "session-diff",
                                crate::surface::PillSize::Sm,
                                if diff_open {
                                    crate::surface::PillMode::Select { selected: true }
                                } else {
                                    crate::surface::PillMode::Action
                                },
                                cx,
                            )
                            .tooltip(move |window, cx| {
                                gpui_component::tooltip::Tooltip::new(if diff_open {
                                    "Hide changes"
                                } else {
                                    "Show changes"
                                })
                                .build(window, cx)
                            })
                            .child(
                                Icon::new(registry::CODING_DIFF)
                                    .with_size(px(crate::surface::PillSize::Sm.glyph())),
                            )
                            .child(
                                div()
                                    .font_family(theme::terminal::FONT_FAMILY)
                                    .text_color(theme::tokens::GREEN.to_hsla())
                                    .child(SharedString::from(format!("+{additions}"))),
                            )
                            .child(
                                div()
                                    .font_family(theme::terminal::FONT_FAMILY)
                                    .text_color(cx.theme().danger)
                                    .child(SharedString::from(format!("-{deletions}"))),
                            )
                            .on_click(cx.listener(move |_, _, _window, cx| {
                                inner.update(cx, |view, cx| view.toggle_diff(cx));
                            })),
                        )
                    })
                    // EXP-850 §10: Merge, where the Changes band used to carry it —
                    // the same two-click arm/confirm control, offered only while the
                    // run is live and its PR open.
                    .when_some(merge_target, |this, target| {
                        let merge_state = crate::pr_merge::MergeState::global(cx);
                        this.child(
                            crate::surface::glass_pill(
                                "session-merge",
                                crate::surface::PillSize::Sm,
                                crate::surface::PillMode::Readonly,
                                cx,
                            )
                            .px_0()
                            .child(crate::changes_bar::merge_button(&target, &merge_state, cx)),
                        )
                    })
                    .when_some(resume, |this, path| {
                        let session_id = self.session_id.clone();
                        let button = Button::new("session-resume")
                            .ghost()
                            .cursor_pointer()
                            .xsmall()
                            .icon(registry::RUN_RESUME)
                            .label("Resume");
                        match path {
                            ResumePath::Local => this.child(button.on_click(move |_, window, cx| {
                                // The ONE desktop resume entry point: the transport
                                // comes from the recorded run, never from the
                                // setting.
                                crate::action_run::resume_run(
                                    session_id.clone(),
                                    Some(window.window_handle()),
                                    false,
                                    coding::LaunchOrigin::Local,
                                    cx,
                                );
                            })),
                            ResumePath::Remote { device_id } => {
                                // EXP-800: the run relaunches on the machine that
                                // hosted it; the tooltip says so before the click.
                                let label = resume_host.clone();
                                this.child(
                                    button
                                        .tooltip(SharedString::from(format!("Resume on {label}")))
                                        .on_click(move |_, window, cx| {
                                            resume_remote(
                                                session_id.clone(),
                                                device_id.clone(),
                                                label.clone(),
                                                window,
                                                cx,
                                            );
                                        }),
                                )
                            }
                        }
                    })
                    .when(can_kill && !self.ended, |this| {
                        // EXP-818: ONE Stop, the same pill whether this machine hosts
                        // the run or another one does — the confirm is the run's own
                        // (`prompt_kill`: in-process kill or `steer.killSession`).
                        let inner = self.inner.clone();
                        this.child(stop_session_pill("session-stop", cx).on_click(
                            move |_, window, cx| {
                                inner.update(cx, |view, cx| view.prompt_kill(window, cx));
                            },
                        ))
                    }),
            )
            .into_any_element()
    }

    /// The issue this run is bound to, off the synced row. `None` for a batch
    /// / action / chat run, and before the row has synced.
    fn issue_id(&self, cx: &App) -> Option<String> {
        self.inner
            .read(cx)
            .session_row()
            .and_then(|row| row.issue_id.clone())
    }

    /// EXP-827/EXP-863 — the ISSUE BAND under the header: the issue's OWN
    /// header (`IssueHeader`: the pin + `…` top row, a read-only title, the
    /// property tray) with "Open issue" in the tray's trailing slot where the
    /// detail view puts Start coding / Watch — a run's own screen has nothing
    /// to start or watch. The agent row (merge-error caption) is skipped: the
    /// session header carries Merge itself. `None` for a batch / action /
    /// chat run, and while the issue row has not synced.
    fn render_issue_band(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let issue_id = self.issue_id(cx)?;
        let issue = sync::Store::try_global(cx)?
            .collections()
            .issues
            .read(cx)
            .get(&issue_id)
            .cloned()?;
        let header = self.ensure_header(&issue_id, window, cx);
        let open_id = issue_id.clone();
        let open_issue_pill = crate::surface::glass_pill_button(
            "session-open-issue",
            crate::surface::PillSize::Sm,
            cx,
        )
        .icon(
            gpui_component::Icon::new(registry::NAV_ISSUES)
                .with_size(px(crate::surface::PillSize::Sm.glyph())),
        )
        .label("Open issue")
        .on_click(move |_, window, cx| {
            open_issue(&open_id, window, cx);
        })
        .into_any_element();
        // The header entity's rows are built through `entity.update` from
        // this render (the detail view's `render_header` precedent) — they
        // never call back into this view synchronously.
        let (top_row, chip_row) = header.update(cx, |header, cx| {
            (
                header.top_row(&issue, cx),
                header.chip_row(&issue, Some(open_issue_pill), cx),
            )
        });
        Some(
            v_flex()
                .id(SharedString::from(format!("session-issue-band-{issue_id}")))
                .w_full()
                .flex_shrink_0()
                .min_w_0()
                .border_b_1()
                .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
                .child(crate::issue_detail::centered_column(
                    v_flex()
                        .child(top_row)
                        .child(crate::issue_header::title_row(&issue))
                        .child(chip_row),
                ))
                .into_any_element(),
        )
    }
}

/// Open `issue_id`'s detail — the issue band's click and its pill.
fn open_issue(issue_id: &str, window: &mut Window, cx: &mut App) {
    crate::navigation::navigate(
        window,
        cx,
        Screen::IssueDetail {
            issue_id: issue_id.to_string(),
        },
    );
}

/// EXP-818: the session header's Stop — a small glass pill with the stop
/// glyph in the danger tint and the word "Stop". The SAME element on every
/// session surface (the screen's header, the viewer's own chrome, whichever
/// machine hosts the run); the caller wires the click to the run's confirm.
pub(crate) fn stop_session_pill(id: impl Into<gpui::ElementId>, cx: &App) -> Button {
    crate::surface::glass_pill_button(id, crate::surface::PillSize::Sm, cx)
        .icon(
            gpui_component::Icon::new(registry::CODING_STOP)
                .with_size(gpui::px(crate::surface::PillSize::Sm.glyph()))
                .text_color(cx.theme().danger),
        )
        .label("Stop")
        .tooltip("Stop the agent and end the session")
}

/// EXP-800: send a resume to the machine that hosted the run — the
/// composer's `launch_remote` recipe (`chat_screen`). The server checks owner,
/// `ended`, the device and its `resume-run` cap.
///
/// EXP-818: it then FOLLOWS the resumed run in, like every other start
/// (`coding_flow::follow_remote_start` waits for the row the other machine
/// writes, matched by `resumed_from_id`). The tab swap
/// (`screens::sync_session_tabs`, also keyed on `resumed_from_id`) still
/// happens on its own; opening the new row is what makes the click land
/// somewhere instead of only toasting.
fn resume_remote(
    session_id: String,
    device_id: String,
    device_label: String,
    window: &mut Window,
    cx: &mut App,
) {
    resume_remote_inner(session_id, device_id, device_label, None, window, cx)
}

/// EXP-849 — the same relay resume, naming an ACCOUNT on the target machine:
/// a remote "switch account" is a resume with an account, which is exactly
/// what the frame already carries. The machine applies the same rules a local
/// switch does (claude only, the profile must exist there) and refuses with
/// its own message.
pub(crate) fn resume_remote_on_account(
    session_id: String,
    device_id: String,
    account: Option<String>,
    window: &mut Window,
    cx: &mut App,
) {
    let label = crate::queries::device_label_for_id(cx, &device_id)
        .unwrap_or_else(|| "that machine".to_string());
    resume_remote_inner(session_id, device_id, label, account, window, cx)
}

fn resume_remote_inner(
    session_id: String,
    device_id: String,
    device_label: String,
    account: Option<String>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        window.push_notification(
            Notification::error("Sign in and wait for sync before resuming."),
            cx,
        );
        return;
    };
    let input = api::steer::StartSessionInput {
        resume_session_id: Some(session_id),
        device_id: device_id.clone(),
        account,
        ..Default::default()
    };
    let subject = crate::coding_flow::RemoteRunSubject::of(&input);
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::steer::start_session(&trpc, &input) })
            .await;
        let sent = result.is_ok();
        let note = match result {
            Ok(()) => Notification::success(SharedString::from(format!(
                "Resume sent to {device_label}."
            ))),
            Err(err) => Notification::error(SharedString::from(err.user_message())),
        };
        let _ = handle.update(cx, |_, window, cx| {
            window.push_notification(note, cx);
            if sent {
                crate::coding_flow::follow_remote_start(device_id, subject, window, cx);
            }
        });
    })
    .detach();
}

/// EXP-863 — the usage sheet, the SAME structure the web popover builds:
///
/// 1. the ACTIVE account's caption once (the only place it appears);
/// 2. "Context" — the run's live window, cost right-aligned, then the meter;
/// 3. the active account's rate-limit windows (the three cards as today);
/// 4. "Accounts" — ONLY the other accounts on the host, each with Switch,
///    dense cards and an account-level refusal; omitted with no other one;
/// 5. ONE footer note: the global blocker, else the one-time cost.
///
/// Sections are separated by hairlines; no sentence appears twice. The
/// context block and the windows are two different quantities (this run's
/// tokens on the wire vs. the machine's rate limits, up to a heartbeat
/// stale), which is why they stay two sections and not one merged list.
fn render_usage_sheet(
    agent: Option<coding::CodingAgent>,
    usage: Option<&steer::SessionUsage>,
    windows: Option<&coding::agent_usage::AgentUsage>,
    switch: Option<crate::account_switch::SwitchContext>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let resolved = switch
        .as_ref()
        .and_then(|switch| switch.resolve(cx));
    let (targets, blocker) = match resolved.as_ref() {
        Some((targets, blocker)) => (targets.as_slice(), blocker.as_ref()),
        None => (&[][..], None),
    };
    let mut sections: Vec<AnyElement> = Vec::new();

    // 1. The active account, once.
    if let Some(current) = targets.iter().find(|target| target.current) {
        sections.push(
            h_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(SharedString::from(current.caption.clone())),
                )
                .children(crate::usage_bar::health_badge(current.health, cx))
                .into_any_element(),
        );
    }

    // 2. Context.
    let context = crate::usage_bar::render_context_block(usage, cx);
    let has_context = context.is_some();
    sections.extend(context);

    // 3. The active account's windows.
    let has_windows = windows.is_some_and(|windows| !windows.windows.is_empty());
    if let (Some(agent), Some(windows)) = (agent, windows.filter(|_| has_windows)) {
        sections.push(crate::usage_bar::render_usage_cards(
            agent,
            windows,
            chrono::Utc::now().timestamp(),
            true,
            cx,
        ));
    }
    if !has_context && !has_windows {
        // EXP-862: a live run's login IS signed in, so windows that have not
        // been read yet are "Checking…" (the ×4 `usage_caption` rule), never
        // a machine that looks broken.
        let caption = crate::usage_bar::usage_caption(crate::usage_bar::UsageState::Checking, None)
            .unwrap_or_default();
        sections.push(div().text_xs().text_color(muted).child(caption).into_any_element());
    }

    // 4. The OTHER accounts.
    if let Some(rows) = switch
        .as_ref()
        .and_then(|switch| switch.render_account_rows(targets, cx))
    {
        sections.push(
            v_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .child(crate::usage_bar::sheet_section_title(
                    crate::account_switch::SECTION_TITLE,
                    cx,
                ))
                .child(rows)
                .into_any_element(),
        );
    }

    // 5. One footer note.
    if let Some(note) = crate::account_switch::footer_note(targets, blocker) {
        sections.push(
            div()
                .text_2xs()
                .text_color(muted.opacity(0.8))
                .child(note)
                .into_any_element(),
        );
    }

    let mut sheet = v_flex().w(px(320.)).min_w_0();
    for (index, section) in sections.into_iter().enumerate() {
        let mut slot = div().w_full().min_w_0().py_2();
        if index > 0 {
            slot = slot
                .border_t_1()
                .border_color(theme::tokens::glass::STROKE_ROW.to_hsla());
        }
        sheet = sheet.child(slot.child(section));
    }
    sheet.into_any_element()
}

/// Another machine's per-agent windows, off the synced `devices` row — the
/// same jsonb every client reads (the device reports it on heartbeat).
fn device_usage(
    device_id: Option<&str>,
    agent: coding::CodingAgent,
    cx: &App,
) -> Option<coding::agent_usage::AgentUsage> {
    let device_id = device_id?;
    let store = sync::Store::try_global(cx)?;
    let devices = store.collections().devices.read(cx);
    let row = devices
        .iter()
        .find(|row| row.device_id.as_deref() == Some(device_id))?;
    crate::device_settings::parse_agent_map::<coding::agent_usage::AgentUsage>(
        row.agent_usage.as_ref(),
    )
    .remove(agent.id())
}

impl Focusable for SessionScreenView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SessionScreenView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let header = self.render_header(cx);
        let issue_band = self.render_issue_band(window, cx);
        // EXP-849: the continuation byline sits directly under the subject —
        // it is about THIS run's history, not about its result.
        let continuation = self.render_continuation(cx);
        // EXP-773: one column — the identity header, EXP-827's issue band,
        // then the transcript, whose own footer carries the Changes bar and
        // the composer. The 280px right rail the changes used to live in is
        // gone, and so is the finished run's summary block: EXP-862 stopped
        // storing `coding_sessions.summary` at all, so no client renders it.
        v_flex()
            .size_full()
            .min_w_0()
            .min_h_0()
            .track_focus(&self.focus_handle)
            .child(header)
            .children(issue_band)
            .children(continuation)
            .child(div().flex_1().min_h_0().child(self.inner.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::{feed_source_for, resume_path_for, ResumePath, SessionFeed};
    use serde_json::json;

    /// The source decision in one table: a live local engine always wins, a
    /// replay needs BOTH a transcript on this machine and an ended run, and
    /// everything else falls back to the relay.
    #[test]
    fn feed_source_for_prefers_the_engine_then_a_local_transcript() {
        // A run this process hosts renders its own engine, ended or not: the
        // exit edge arrives on that feed too.
        assert_eq!(feed_source_for(true, true, false), SessionFeed::Local);
        assert_eq!(feed_source_for(true, true, true), SessionFeed::Local);
        // An ended run we no longer host replays off its on-device transcript.
        assert_eq!(feed_source_for(false, true, true), SessionFeed::Replay);
        // Still running elsewhere — the relay is the only live feed.
        assert_eq!(feed_source_for(false, true, false), SessionFeed::Remote);
        // Nothing on this machine (another machine's run, or a pruned
        // journal): the relay, which asks that device for the history.
        assert_eq!(feed_source_for(false, false, true), SessionFeed::Remote);
        assert_eq!(feed_source_for(false, false, false), SessionFeed::Remote);
    }

    // ---- EXP-800: local-or-remote resume ----------------------------------

    const NOW_MS: i64 = 1784289600_000;
    const FRESH: &str = "2026-07-17T11:59:30Z";

    fn ended_session(device_id: Option<&str>, user_id: Option<&str>) -> domain::rows::CodingSession {
        serde_json::from_value(json!({
            "id": "sess-1",
            "issue_id": "issue-1",
            "status": "ended",
            "updated_at": "2026-07-17T11:59:00Z",
            "device_id": device_id,
            "device_label": "old-host",
            "user_id": user_id,
        }))
        .unwrap()
    }

    fn device_row(
        id: &str,
        device_id: &str,
        user_id: &str,
        last_seen_at: &str,
        caps: &[&str],
    ) -> domain::rows::DeviceRow {
        serde_json::from_value(json!({
            "id": id,
            "device_id": device_id,
            "user_id": user_id,
            "label": id,
            "last_seen_at": last_seen_at,
            "caps": caps,
        }))
        .unwrap()
    }

    fn stale() -> String {
        let window_secs = domain::contract::DEVICE_ONLINE_WINDOW_MS / 1_000;
        let seen = chrono::DateTime::from_timestamp(NOW_MS / 1_000 - window_secs - 5, 0).unwrap();
        seen.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    /// A run this machine still holds resumes locally no matter what the
    /// synced rows say — even when its host row is offline or capless.
    #[test]
    fn resume_path_prefers_a_local_record() {
        let session = ended_session(Some("dev-other"), Some("user-1"));
        let devices = vec![device_row("d-1", "dev-other", "user-1", &stale(), &[])];
        assert_eq!(
            resume_path_for(true, false, &session, devices.iter(), NOW_MS),
            Some(ResumePath::Local)
        );
        assert_eq!(
            resume_path_for(true, true, &session, [].iter(), NOW_MS),
            Some(ResumePath::Local)
        );
    }

    /// The host machine is online and advertises `resume-run`: the relay
    /// path, addressed by the session's own `device_id`.
    #[test]
    fn resume_path_goes_remote_to_an_online_capable_host() {
        let session = ended_session(Some("dev-other"), Some("user-1"));
        let devices = vec![device_row(
            "d-1",
            "dev-other",
            "user-1",
            FRESH,
            &["actions", coding::doctor::RESUME_RUN_CAP],
        )];
        assert_eq!(
            resume_path_for(false, false, &session, devices.iter(), NOW_MS),
            Some(ResumePath::Remote { device_id: "dev-other".to_string() })
        );
    }

    /// Offline, or online without the cap: no button — the server would
    /// refuse the start anyway.
    #[test]
    fn resume_path_needs_the_host_online_and_capable() {
        let session = ended_session(Some("dev-other"), Some("user-1"));
        let offline = vec![device_row(
            "d-1",
            "dev-other",
            "user-1",
            &stale(),
            &[coding::doctor::RESUME_RUN_CAP],
        )];
        assert_eq!(
            resume_path_for(false, false, &session, offline.iter(), NOW_MS),
            None
        );
        let capless = vec![device_row("d-1", "dev-other", "user-1", FRESH, &["actions"])];
        assert_eq!(
            resume_path_for(false, false, &session, capless.iter(), NOW_MS),
            None
        );
    }

    /// No `device_id` stamp, or no synced row for it: nowhere to send it.
    #[test]
    fn resume_path_needs_a_resolved_host() {
        let devices = vec![device_row(
            "d-1",
            "dev-other",
            "user-1",
            FRESH,
            &[coding::doctor::RESUME_RUN_CAP],
        )];
        let unstamped = ended_session(None, Some("user-1"));
        assert_eq!(
            resume_path_for(false, false, &unstamped, devices.iter(), NOW_MS),
            None
        );
        let unknown = ended_session(Some("dev-unknown"), Some("user-1"));
        assert_eq!(
            resume_path_for(false, false, &unknown, devices.iter(), NOW_MS),
            None
        );
    }

    /// EXP-764: this install hosted the run but no longer holds a record (a
    /// purged repo-less run) — a relay round trip to ourselves could only
    /// answer the same "no", so nothing is offered even with a live row.
    #[test]
    fn resume_path_never_relays_to_itself() {
        let session = ended_session(Some("dev-me"), Some("user-1"));
        let devices = vec![device_row(
            "d-1",
            "dev-me",
            "user-1",
            FRESH,
            &[coding::doctor::RESUME_RUN_CAP],
        )];
        assert_eq!(
            resume_path_for(false, true, &session, devices.iter(), NOW_MS),
            None
        );
    }

    /// A shared device syncs one row per member; the session owner's row is
    /// the one whose caps and heartbeat count, first in the list or not.
    #[test]
    fn resume_path_reads_the_owner_row_on_a_shared_device() {
        let session = ended_session(Some("dev-shared"), Some("user-1"));
        let teammate_capable = vec![
            device_row(
                "d-teammate",
                "dev-shared",
                "user-2",
                FRESH,
                &[coding::doctor::RESUME_RUN_CAP],
            ),
            device_row("d-mine", "dev-shared", "user-1", FRESH, &[]),
        ];
        assert_eq!(
            resume_path_for(false, false, &session, teammate_capable.iter(), NOW_MS),
            None,
            "the teammate's caps never speak for the owner's row"
        );
        let mine_capable = vec![
            device_row("d-teammate", "dev-shared", "user-2", FRESH, &[]),
            device_row(
                "d-mine",
                "dev-shared",
                "user-1",
                FRESH,
                &[coding::doctor::RESUME_RUN_CAP],
            ),
        ];
        assert_eq!(
            resume_path_for(false, false, &session, mine_capable.iter(), NOW_MS),
            Some(ResumePath::Remote { device_id: "dev-shared".to_string() })
        );
    }
}
