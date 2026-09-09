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
//! This screen owns the CHROME around that transcript — the identity header
//! with the session's usage and kill. The transcript view keeps everything
//! that is about the conversation itself (the feed, the banners, the
//! Latest-changes bar, the composer and its mode control), which is why it
//! renders headerless here ([`SteerSessionView::set_chrome`]).
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
    h_flex, popover::Popover, v_flex, ActiveTheme as _, Sizable as _,
};

use crate::coding_flow::LocalSessions;
use crate::icons::registry;
use crate::navigation::Screen;
use crate::steer_viewer::{FeedSource, SteerSessionView};

/// Open `session_id`'s surface in this window (EXP-773: a coding run has no
/// terminal tab): its issue's detail with the transcript slid in when the
/// run is issue-bound (EXP-791), else its own session screen.
pub(crate) fn open_session(session_id: &str, window: &mut Window, cx: &mut App) {
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
    if let Some(issue_id) = issue_of_session(session_id, cx) {
        crate::navigation::navigate_steering(window, cx, issue_id, session_id.to_string());
        return;
    }
    crate::navigation::navigate(
        window,
        cx,
        Screen::Session {
            session_id: session_id.to_string(),
        },
    );
}

/// EXP-791: the issue `session_id` is bound to, off its synced row — the
/// surface an issue-bound run opens on. `None` for a batch/action/chat run
/// and for a row that has not synced yet (it then opens on its own screen).
fn issue_of_session(session_id: &str, cx: &App) -> Option<String> {
    let store = sync::Store::try_global(cx)?;
    let sessions = store.collections().coding_sessions.read(cx);
    sessions.get(session_id)?.issue_id.clone()
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
    // EXP-746: a replay respawns the recorded binary, so it needs the same
    // spec the run had — except its env, which runs.json never stores. The
    // LIVE settings entry is what supplies it; an external agent the user has
    // since deleted replays env-less rather than on a rotated token.
    let configured = crate::coding_flow::CodingHub::global_ref(cx)
        .map(|hub| hub.read(cx).settings.external_agents.clone())
        .unwrap_or_default();
    let agent = match record.resolved_external_agent(&configured) {
        Some(spec) => coding::AgentKind::External(spec),
        None => coding::AgentKind::Builtin(record.agent),
    };
    // The ACP id is the handle a replay wants; the agent-native ones are the
    // fallback for a record written before the handshake answered.
    let native = record
        .agent_native_session_id
        .clone()
        .or_else(|| record.claude_session_id.clone())
        .map(engine::ResumeHandle::Native)
        .or_else(|| {
            record
                .pi_session_file
                .clone()
                .map(engine::ResumeHandle::PiSessionFile)
        })
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

/// EXP-791: the finished-run summary's height cap (scrolls inside).
const SUMMARY_MAX_H: f32 = 160.;

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
        let subscription = cx.observe(&inner, |_: &mut Self, _, cx| cx.notify());
        Self {
            session_id,
            inner,
            feed,
            ended: false,
            resumable: None,
            focus_handle: cx.focus_handle(),
            _subscriptions: vec![subscription],
        }
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

    /// EXP-773 — this machine still holds the run's workspace, so the header
    /// may offer Resume. Resolved once per screen (the registry is a file).
    fn resume_offered(&mut self, cx: &App) -> bool {
        if !self.run_over(cx) {
            return false;
        }
        match self.resumable {
            Some(known) => known,
            None => {
                let known = crate::coding_flow::run_is_resumable_ref(&self.session_id, cx);
                self.resumable = Some(known);
                known
            }
        }
    }

    /// EXP-773 — the ended run's byline, the one its list row used to carry
    /// (machine, agent, who ended it, when). `None` while the row has not
    /// synced.
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

    /// EXP-773 — the agent's own summary of a finished run, as a small muted
    /// block above the transcript. It used to unfold inside the Past list; a
    /// run is described in ONE place now, and this is it. `None` for a live
    /// run and for one that left no summary. EXP-791: clamped to
    /// [`SUMMARY_MAX_H`] and scrollable inside — a long summary used to push
    /// the whole transcript below the fold.
    fn render_summary(&mut self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        if !self.run_over(cx) {
            return None;
        }
        let summary = self
            .inner
            .read(cx)
            .session_row()?
            .summary
            .clone()
            .filter(|text| !text.trim().is_empty())?;
        Some(
            div()
                .id(SharedString::from(format!("session-summary-scroll-{}", self.session_id)))
                .w_full()
                .flex_shrink_0()
                .min_w_0()
                .max_h(px(SUMMARY_MAX_H))
                .overflow_y_scroll()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(
                    // EXP-686: the agent writes GFM — render it, never dump
                    // the source (the `comments.rs` recipe).
                    crate::markdown::MarkdownView::new(
                        SharedString::from(format!("session-summary-{}", self.session_id)),
                        summary,
                    )
                    .selectable(true),
                )
                .into_any_element(),
        )
    }

    fn render_header(&mut self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        // EXP-773: an ended run wears its list byline and, when this machine
        // still holds the workspace, the Resume the Past row used to carry.
        let resume = self.resume_offered(cx);
        let byline = self.run_over(cx).then(|| self.ended_byline(cx)).flatten();
        let inner = self.inner.read(cx);
        let (tone, caption) = inner.header_status(cx);
        let (identifier, subject) = inner.header_identity(cx);
        let device = inner.device_label(cx);
        let can_kill = inner.killable(cx);
        let usage = inner.usage();
        let usage_summary = crate::session_extras::context_summary(usage.as_ref());
        let agent = inner.builtin_agent();
        let over = inner.session_over();
        let local = inner.is_local();
        let device_id = inner
            .session_row()
            .and_then(|row| row.device_id.clone());
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

        h_flex()
            .w_full()
            .flex_shrink_0()
            .gap_2()
            .items_center()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
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
            // EXP-791: no agent pill — the agent is the byline's business
            // (an ended run names it there), and the live caption's.
            .child(div().min_w_0().max_w(px(360.)).truncate().text_sm().child(subject))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(muted)
                    // The status caption already names the device when the
                    // transcript knows it (`phase_label`); append it only
                    // when it does not. An ENDED run reads its full byline
                    // instead (machine, agent, who ended it, when).
                    .child(byline.unwrap_or_else(|| {
                        SharedString::from(match device {
                            Some(device) if !caption.contains(device.as_str()) => {
                                format!("{caption} · {device}")
                            }
                            _ => caption,
                        })
                    })),
            )
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
                                .icon(registry::UI_CLOCK)
                                .label(SharedString::from(summary))
                                .tooltip("Usage"),
                            )
                            .content(move |_, _window, cx| {
                                render_usage_sheet(agent, usage.as_ref(), windows.as_ref(), cx)
                            }),
                    )
                },
            )
            .when(resume, |this| {
                let session_id = self.session_id.clone();
                this.child(
                    Button::new("session-resume")
                        .ghost()
                        .cursor_pointer()
                        .xsmall()
                        .icon(registry::RUN_RESUME)
                        .label("Resume")
                        .on_click(move |_, window, cx| {
                            // The ONE desktop resume entry point: the
                            // transport comes from the recorded run, never
                            // from the setting.
                            crate::action_run::resume_run(
                                session_id.clone(),
                                Some(window.window_handle()),
                                false,
                                coding::LaunchOrigin::Local,
                                cx,
                            );
                        }),
                )
            })
            .when(can_kill && !self.ended, |this| {
                let inner = self.inner.clone();
                this.child(
                    Button::new("session-kill")
                        .ghost()
                        .cursor_pointer()
                        .xsmall()
                        .icon(registry::CODING_STOP)
                        .tooltip("Kill session")
                        .on_click(move |_, window, cx| {
                            inner.update(cx, |view, cx| view.prompt_kill(window, cx));
                        }),
                )
            })
            .into_any_element()
    }
}

/// The usage sheet: this SESSION's context block first (live, on the wire),
/// then the host machine's per-agent rate-limit windows (per-machine, up to a
/// heartbeat stale). Two different quantities, which is why they are two
/// blocks and not one merged list.
fn render_usage_sheet(
    agent: Option<coding::CodingAgent>,
    usage: Option<&steer::SessionUsage>,
    windows: Option<&coding::agent_usage::AgentUsage>,
    cx: &App,
) -> AnyElement {
    let mut sheet = v_flex().w(px(260.)).gap_3();
    let context = crate::usage_bar::render_context_block(usage, cx);
    let has_windows = windows.is_some_and(|windows| !windows.windows.is_empty());
    let has_context = context.is_some();
    sheet = sheet.children(context);
    if let (Some(agent), Some(windows)) = (agent, windows) {
        sheet = sheet.child(crate::usage_bar::render_usage_cards(
            agent,
            windows,
            chrono::Utc::now().timestamp(),
            true,
            cx,
        ));
    }
    if !has_context && !has_windows {
        sheet = sheet.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("No usage reported yet."),
        );
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
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let header = self.render_header(cx);
        let summary = self.render_summary(cx);
        // EXP-773: one column — the identity header, a finished run's summary,
        // then the transcript, whose own footer carries the Latest-changes bar
        // and the composer. The 280px right rail the changes used to live in
        // is gone.
        v_flex()
            .size_full()
            .min_w_0()
            .min_h_0()
            .track_focus(&self.focus_handle)
            .child(header)
            .children(summary)
            .child(div().flex_1().min_h_0().child(self.inner.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::{feed_source_for, SessionFeed, SUMMARY_MAX_H};

    /// EXP-791: the summary block is clamped (and scrolls inside) — it used to
    /// take whatever height the agent's prose needed.
    #[test]
    fn summary_is_clamped() {
        assert_eq!(SUMMARY_MAX_H, 160.);
    }

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
}
