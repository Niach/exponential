//! The coding-session center screen (EXP-746) — one open [`Screen::Session`]
//! tab per `coding_sessions` row.
//!
//! Sessions used to live in the bottom terminal dock: a PTY tab for a run this
//! process hosts, a "remote chip" beside it for one on another machine. The
//! ACP engine ends that split. A run this process hosts is an in-process
//! [`engine::EngineSession`], not a terminal, so it renders in the CENTER pane
//! like every other detail screen. EXP-769: a PTY-hosted run renders in the
//! center too (`Screen::Terminal`). EXP-870: a run is a TOP tab — the Run
//! face of its issue's tab (`Issue | Run` in the header), or a Run-only tab
//! for an issue-less run — and every live run of mine always has one.
//!
//! [`open_session`] is the ONE entry point (the start dialog, the issue's
//! coding-now card, Devices → Running / Past, a pinned run).
//!
//! Three feed sources, one renderer ([`SteerSessionView`]): the in-process
//! engine (`Local`), the relay viewer (`Remote` — another machine, or another
//! process on this one), and an ended ACP run replayed off disk (`Replay`).
//! [`feed_source_for`] is that decision, pure and unit-tested; materializing
//! it is [`resolve_source`].
//!
//! This screen owns the CHROME around that transcript — EXP-877's shared
//! `work_header::WorkHeader`, the SAME header the issue detail renders: for
//! an issue-bound run the issue's title, the `Issue | Run | +N -M` face
//! toggle with the pin and `…` menu, and the property tray trailing Merge PR
//! + the ONE coding action (Stop / Resume / Start coding, from the run
//! state); for a chat / action / batch run the run title with `[Run | diff]
//! [Merge PR] [Stop | Resume]`. The transcript view keeps everything that is
//! about the conversation itself (the feed, the banners, the full-page diff
//! the toggle's diff item opens, the composer with its usage readout) and
//! renders headerless: this screen is the only header.
//!
//! Lifetime rule: a view lives exactly as long as its TAB. `ScreensPanel`
//! creates it on first activation and calls [`SessionScreenView::shutdown`]
//! when the tab closes — which drops the feed, never the run: closing a tab
//! must not end an agent (kill is its own affordance). An ended run keeps its
//! tab as a read-only transcript, which is what [`SessionScreenView::mark_ended`]
//! marks when the engine reports the exit before the row syncs.

use std::rc::Rc;

use gpui::{
    div, AnyElement, App, AppContext as _, Entity, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::Button, h_flex, notification::Notification, v_flex, ActiveTheme as _, Icon,
    Sizable as _, WindowExt as _,
};

use crate::coding_flow::{LocalSessions, StartCodingControl};
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
    /// The engine reported its exit (D2). The synced row carries the same
    /// truth a round trip later — the inner view reads that one itself.
    ended: bool,
    /// EXP-773/EXP-877: whether this machine still holds a run's workspace
    /// (a reclaimed worktree is re-created on resume), resolved ONCE per run
    /// id (the registry parse is a file read; a session screen repaints on
    /// every feed event) — `work_header::resume_path_cached`'s cache.
    resumable: Option<(String, bool)>,
    /// EXP-863/EXP-877: the issue's header entity (the property tray, the
    /// pin + `…` cluster), built lazily once the synced row names an
    /// `issue_id`; `None` for a batch / action / chat run. It shares the
    /// detail's `WorkHeader` frame, launcher included.
    header: Option<Entity<IssueHeader>>,
    /// The launcher the header above owns — pointed at the run's issue so
    /// "Start coding" works from the run face too.
    start_coding: Option<Entity<StartCodingControl>>,
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
        let (_, source) = resolve_source(&session_id, cx);
        Self::build(session_id, source, window, cx)
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
        Self::build(session_id, FeedSource::Remote { handle: None }, window, cx)
    }

    fn build(
        session_id: String,
        source: FeedSource,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let inner =
            cx.new(|cx| SteerSessionView::with_source(session_id.clone(), source, window, cx));
        // The transcript notifies on every feed change — which is also when
        // the header's status moves.
        let mut subscriptions = vec![cx.observe(&inner, |_: &mut Self, _, cx| cx.notify())];
        if let Some(store) = sync::Store::try_global(cx) {
            // EXP-877: the header names the run's issue (title, PR state)
            // straight off the synced row — its arrival, ahead of the issue
            // header entity that will observe it, must repaint this screen.
            let issues = store.collections().issues.clone();
            subscriptions.push(cx.observe(&issues, |_: &mut Self, _, cx| cx.notify()));
        }
        // The header's Merge pill arms/spins off the shared merge state.
        let merge_state = crate::pr_merge::MergeState::global(cx);
        subscriptions.push(cx.observe(&merge_state, |_: &mut Self, _, cx| cx.notify()));
        Self {
            session_id,
            inner,
            ended: false,
            resumable: None,
            header: None,
            start_coding: None,
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
                let header =
                    cx.new(|cx| IssueHeader::new(start_coding.clone(), window, cx));
                self._subscriptions
                    .push(cx.observe(&header, |_: &mut Self, _, cx| cx.notify()));
                self.header = Some(header.clone());
                self.start_coding = Some(start_coding);
                header
            }
        };
        if let Some(start_coding) = &self.start_coding {
            start_coding.update(cx, |control, cx| {
                control.set_issue(Some(issue_id.to_string()), cx)
            });
        }
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

    // ── Diff face (EXP-877) ───────────────────────────────────────────────

    /// `+N -M` over the run's published diff — the header toggle's diff item.
    /// `None` when the run published no diff at all (the item is hidden).
    pub(crate) fn diff_totals(&self, cx: &App) -> Option<(u32, u32)> {
        self.inner.read(cx).diff_totals()
    }

    /// Whether the viewer's full-page diff is up (= the Diff face).
    pub(crate) fn diff_open(&self, cx: &App) -> bool {
        self.inner.read(cx).diff_open()
    }

    /// Show or hide the full-page diff — the toggle's Diff / Run picks, and
    /// the detail's Diff pick after it flips the tab to this face.
    pub(crate) fn set_diff_open(&mut self, open: bool, cx: &mut gpui::Context<Self>) {
        self.inner.update(cx, |view, cx| view.set_diff_open(open, cx));
        cx.notify();
    }

    // ── Header ────────────────────────────────────────────────────────────

    /// Whether the run is over — the engine's own exit edge, the transcript's
    /// phase, or the synced row.
    fn run_over(&self, cx: &App) -> bool {
        self.ended || self.inner.read(cx).session_over()
    }

    /// Where an issue-less run's Resume goes, or `None` while the run is live
    /// or no machine can take it (EXP-800: the local half is the once-per-run
    /// cache, the remote half is re-read per repaint — the inner viewer
    /// observes the devices rows, so a heartbeat edge repaints this header).
    fn resume_path(&mut self, cx: &mut gpui::Context<Self>) -> Option<ResumePath> {
        if !self.run_over(cx) {
            return None;
        }
        let row = self.inner.read(cx).session_row().cloned();
        let Some(row) = row else {
            // No synced row: only this machine's registry can answer.
            let local = match &self.resumable {
                Some((id, known)) if *id == self.session_id => *known,
                _ => {
                    let known = crate::coding_flow::run_is_resumable_ref(&self.session_id, cx);
                    self.resumable = Some((self.session_id.clone(), known));
                    known
                }
            };
            return local.then_some(ResumePath::Local);
        };
        crate::work_header::resume_path_cached(&row, &mut self.resumable, cx)
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

    /// The face toggle for this screen: `Issue` when the run is issue-bound,
    /// `Run`, and the diff item when the run has changes; active = Diff while
    /// the full-page diff is up, else Run. Issue → the tab's issue face;
    /// Run / Diff → hide / show the diff in place.
    fn face_toggle(&self, issue_id: Option<String>, cx: &App) -> Option<AnyElement> {
        use crate::work_header::{Face, FaceToggle};
        let inner = self.inner.clone();
        let spec = FaceToggle {
            issue: issue_id.is_some(),
            run: Some(self.session_id.clone()),
            diff: self.diff_totals(cx),
            active: if self.diff_open(cx) {
                Face::Diff
            } else {
                Face::Run
            },
        };
        crate::work_header::face_toggle(
            spec,
            Rc::new(move |face, window, cx| match face {
                Face::Issue => {
                    if let Some(issue_id) = &issue_id {
                        crate::screens::set_tab_face(
                            issue_id,
                            crate::screens::TabFace::Issue,
                            None,
                            window,
                            cx,
                        );
                    }
                }
                Face::Run => inner.update(cx, |view, cx| view.set_diff_open(false, cx)),
                Face::Diff => inner.update(cx, |view, cx| view.set_diff_open(true, cx)),
            }),
            cx,
        )
    }

    /// EXP-877: the shared `WorkHeader`. Issue-bound → the SAME header the
    /// issue detail renders (a static title, the toggle · pin · `…` cluster,
    /// the property tray trailing Merge PR + the ONE coding action). Issue-
    /// less (chat / action / batch) → the run title (`run_rows::run_title`)
    /// with `[Run | +N -M] [Merge PR] [Stop | Resume]` and no tray.
    fn render_header(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        use crate::work_header::{CodingAction, WorkHeader};
        let row = self.inner.read(cx).session_row().cloned();
        let issue = row
            .as_ref()
            .and_then(|row| row.issue_id.as_deref())
            .and_then(|issue_id| {
                sync::Store::try_global(cx)?
                    .collections()
                    .issues
                    .read(cx)
                    .get(issue_id)
                    .cloned()
            });

        if let (Some(row), Some(issue)) = (row.as_ref(), issue) {
            let title = crate::work_header::title_row(crate::run_rows::run_title(row, Some(&issue)));
            let toggle = self.face_toggle(Some(issue.id.clone()), cx);
            let action = crate::work_header::issue_coding_action(
                &issue.id,
                Some(self.session_id.as_str()),
                &mut self.resumable,
                cx,
            );
            let header = self.ensure_header(&issue.id, window, cx);
            // The header entity's rows are built through `entity.update` from
            // this render (the detail view's precedent) — they never call
            // back into this view synchronously.
            let (right, tray, extra) = header.update(cx, |header, cx| {
                let right = header.right_cluster(&issue, toggle, cx);
                let actions = header.issue_actions(&issue, action, cx);
                (
                    right,
                    Some(header.chip_row(&issue, actions, cx)),
                    header.agent_row(&issue, cx),
                )
            });
            return crate::work_header::render_work_header(
                WorkHeader {
                    title,
                    right,
                    tray,
                    extra,
                },
                cx,
            );
        }

        // Issue-less (or the issue row not synced yet): the run's own title.
        let title = crate::work_header::title_row(match row.as_ref() {
            Some(row) => crate::run_rows::run_title(row, None),
            None => SharedString::from("Loading…"),
        });
        let mut right: Vec<AnyElement> = Vec::with_capacity(4);
        right.extend(self.face_toggle(None, cx));
        let (merge_target, killable, local, device_label) = {
            let inner = self.inner.read(cx);
            (
                inner.merge_target(cx),
                inner.killable(cx),
                inner.is_local(),
                inner.device_label(cx),
            )
        };
        if let Some(target) = merge_target {
            right.push(crate::work_header::merge_pill("session-merge", &target, true, cx));
        }
        let over = self.run_over(cx);
        if killable && !self.ended && !over {
            right.extend(crate::work_header::coding_action_button(
                CodingAction::Stop {
                    session_id: self.session_id.clone(),
                    local,
                    device_label,
                },
                None,
                cx,
            ));
        } else if let Some(path) = self.resume_path(cx) {
            right.extend(crate::work_header::coding_action_button(
                CodingAction::Resume {
                    session_id: self.session_id.clone(),
                    path,
                    host_label: device_label,
                },
                None,
                cx,
            ));
        }
        crate::work_header::render_work_header(
            WorkHeader {
                title,
                right,
                tray: None,
                extra: None,
            },
            cx,
        )
    }
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
pub(crate) fn resume_remote(
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

impl Focusable for SessionScreenView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SessionScreenView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let header = self.render_header(window, cx);
        // EXP-849: the continuation byline sits directly under the header —
        // it is about THIS run's history, not about its result.
        let continuation = self.render_continuation(cx);
        // EXP-773/EXP-877: one column — the shared work header, then the
        // transcript, whose own footer carries the composer (and the usage
        // readout). EXP-862 stopped storing `coding_sessions.summary`, so no
        // finished-run summary block renders anywhere.
        v_flex()
            .size_full()
            .min_w_0()
            .min_h_0()
            .track_focus(&self.focus_handle)
            .child(header)
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
