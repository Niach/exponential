//! The coding-session center screen (EXP-746) — one open [`Screen::Session`]
//! tab per `coding_sessions` row.
//!
//! Sessions used to live in the bottom terminal dock: a PTY tab for a run this
//! process hosts, a "remote chip" beside it for one on another machine. The
//! ACP engine ends that split. A run this process hosts is an in-process
//! [`engine::EngineSession`], not a terminal, so it renders in the CENTER pane
//! like every other detail screen; the dock keeps PTY tabs and nothing else.
//!
//! [`open_session`] is the ONE entry point (the start dialog, the issue
//! header's "coding now" pill, Devices → Running / Past): a run still hosted
//! on a PTY reveals its dock tab, everything else navigates here.
//!
//! Three feed sources, one renderer ([`SteerSessionView`]): the in-process
//! engine (`Local`), the relay viewer (`Remote` — another machine, or another
//! process on this one), and an ended ACP run replayed off disk (`Replay`).
//! [`feed_source_for`] is that decision, pure and unit-tested; materializing
//! it is [`resolve_source`].
//!
//! This screen owns the CHROME around that transcript — the identity header
//! with the session's usage and kill, and the "Changes" rail. The transcript
//! view keeps everything that is about the conversation itself (the feed, the
//! banners, the composer and its chips), which is why it renders headerless
//! here ([`SteerSessionView::set_chrome`]) and with its own header in the
//! dock.
//!
//! Lifetime rule: a view lives exactly as long as its TAB. `ScreensPanel`
//! creates it on first activation and calls [`SessionScreenView::shutdown`]
//! when the tab closes — which drops the feed, never the run: closing a tab
//! must not end an agent (kill is its own affordance). An ended run keeps its
//! tab as a read-only transcript, which is what [`SessionScreenView::mark_ended`]
//! marks when the engine reports the exit before the row syncs.

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, AppContext as _, Entity, FocusHandle,
    Focusable, InteractiveElement as _, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, popover::Popover, v_flex, ActiveTheme as _, Sizable as _,
};

use crate::coding_flow::{LocalSessionHost, LocalSessions};
use crate::icons::registry;
use crate::navigation::Screen;
use crate::steer_viewer::{FeedSource, SteerSessionView};

/// Open `session_id`'s surface in this window.
///
/// A run THIS process hosts on the PTY path keeps its dock terminal — there is
/// no ACP conversation to render and nothing to steer remotely about a child
/// whose grid is right there. Everything else (a local ACP run, a run on
/// another machine, an ended one) is a center screen.
// EXP-746: the remaining callers land with their lanes — D5 (Devices →
// Running / Past) and D6 (the issue header's pill).
#[allow(dead_code)]
pub(crate) fn open_session(session_id: &str, window: &mut Window, cx: &mut App) {
    if hosted_on_a_pty_tab(session_id, cx) {
        // The `Some(tab)` half of the dock's opener; lane D6 narrows it to
        // `reveal_pty_tab` when the remote chips go.
        crate::terminal_dock::open_steer_session(session_id, window, cx);
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

/// Mark every open screen for `session_id` ended (the engine's `on_exit` edge,
/// which can precede the synced row's flip by a round trip). The tab stays.
pub(crate) fn mark_ended(session_id: &str, cx: &mut App) {
    for view in crate::screens::session_views(session_id, cx) {
        view.update(cx, |view, cx| view.mark_ended(cx));
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
    /// any PTY-recorded run whose only history is the room's journal.
    Remote,
}

/// The source decision, pure so it can be tested without an engine or a store.
///
/// Order matters: a LIVE local engine wins over everything (replaying a run
/// that is still writing would show a stale transcript beside a live one), and
/// a replay is only ever offered for an ENDED run recorded on the ACP
/// transport — a PTY-recorded run has no ACP transcript to load, and a live
/// remote run belongs on the relay.
pub(crate) fn feed_source_for(
    local_engine: bool,
    recorded: Option<coding::LaunchTransport>,
    row_ended: bool,
) -> SessionFeed {
    if local_engine {
        return SessionFeed::Local;
    }
    if row_ended && recorded == Some(coding::LaunchTransport::Acp) {
        return SessionFeed::Replay;
    }
    SessionFeed::Remote
}

/// [`feed_source_for`] against the live state, then the source it names.
fn resolve_source(session_id: &str, cx: &mut App) -> (SessionFeed, FeedSource) {
    let engine = local_engine(session_id, cx);
    let record = crate::window_size::app_data_dir()
        .and_then(|data_dir| coding::run_registry::get(&data_dir, session_id));
    let feed = feed_source_for(
        engine.is_some(),
        record.as_ref().map(|record| record.transport()),
        row_ended(session_id, cx),
    );
    match (feed, engine) {
        (SessionFeed::Local, Some(session)) => (SessionFeed::Local, FeedSource::Local { session }),
        (SessionFeed::Replay, _) => match record.and_then(|record| open_transcript(&record, cx)) {
            Some(session) => (SessionFeed::Replay, FeedSource::Replay { session }),
            // The record is there but its transcript is not loadable (no
            // steer runtime, or the run never got as far as an id). The
            // transcript view then says so — an empty relay feed with no
            // explanation is the thing this must not become.
            None => (SessionFeed::Remote, FeedSource::Remote { handle: None }),
        },
        _ => (SessionFeed::Remote, FeedSource::Remote { handle: None }),
    }
}

/// Replay `record`'s conversation through the engine (ACP `session/load`).
/// `None` when there is nothing to load it with — the caller then falls back
/// to the relay and the "no transcript" banner.
fn open_transcript(record: &coding::run_registry::RunRecord, cx: &App) -> Option<engine::EngineSession> {
    let runtime = crate::steer_wiring::runtime(cx)?;
    let agent = match record.external_agent.clone() {
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
/// it (and not on a PTY).
fn local_engine(session_id: &str, cx: &App) -> Option<engine::EngineSession> {
    let sessions = LocalSessions::global_ref(cx)?;
    match &sessions.read(cx).session_by_id(session_id)?.host {
        LocalSessionHost::Acp { session } => Some(session.clone()),
        LocalSessionHost::Pty { .. } => None,
    }
}

/// Whether the run this process hosts occupies a dock terminal.
#[allow(dead_code)] // with [`open_session`]
fn hosted_on_a_pty_tab(session_id: &str, cx: &App) -> bool {
    LocalSessions::global_ref(cx).is_some_and(|sessions| {
        sessions
            .read(cx)
            .session_by_id(session_id)
            .is_some_and(|session| session.host.tab().is_some())
    })
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
    /// The "Changes" rail's parse of the published worktree diff, and the view
    /// its expanded half renders into.
    changes: Option<crate::changes_bar::ChangesSnapshot>,
    changes_diff: Entity<crate::diff::DiffView>,
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
        // the published diff, the chips and the usage move.
        let subscription = cx.observe(&inner, |this: &mut Self, _, cx| {
            this.sync_changes(cx);
            cx.notify();
        });
        Self {
            session_id,
            inner,
            feed,
            ended: false,
            changes: None,
            changes_diff: cx.new(|cx| crate::diff::DiffView::new(window, cx)),
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
    pub(crate) fn mark_ended(&mut self, cx: &mut gpui::Context<Self>) {
        if self.ended {
            return;
        }
        self.ended = true;
        cx.notify();
    }

    /// Which feed this screen renders. The screen itself keys off the field;
    /// this is for the surfaces that open one (Devices → Running / Past).
    #[allow(dead_code)] // consumed by lane D5's Devices sections
    pub(crate) fn feed(&self) -> SessionFeed {
        self.feed
    }

    // ── Changes rail ──────────────────────────────────────────────────────

    /// Install the parse of the newly published diff, when
    /// [`crate::changes_bar::sync`] says it changed. Same cache key the dock
    /// uses: the raw string, so a repaint of an unchanged feed parses nothing.
    fn sync_changes(&mut self, cx: &mut gpui::Context<Self>) {
        let raw = self.inner.read(cx).latest_diff().map(str::to_string);
        let Some(next) = crate::changes_bar::sync(self.changes.as_ref(), &self.session_id, raw.as_deref())
        else {
            return;
        };
        self.changes = next;
        if self.changes.as_ref().is_some_and(|state| state.expanded) {
            self.rebuild_changes_diff(cx);
        }
    }

    fn rebuild_changes_diff(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(state) = self.changes.as_ref() else {
            return;
        };
        let prepared = crate::diff::build_scm_diff(&state.files, &cx.theme().highlight_theme);
        self.changes_diff
            .update(cx, |diff, cx| diff.set_prepared(prepared, cx));
    }

    /// Flip the rail open/shut, building the diff rows the first time it opens
    /// (they are only worth rendering when visible).
    fn toggle_changes_expanded(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(state) = self.changes.as_mut() else {
            return;
        };
        state.expanded = !state.expanded;
        if state.expanded {
            self.rebuild_changes_diff(cx);
        }
        cx.notify();
    }

    fn render_changes_rail(&mut self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let (has_diff, merge, over) = {
            let inner = self.inner.read(cx);
            (
                inner.latest_diff().is_some(),
                inner
                    .session_row()
                    .and_then(|row| crate::changes_bar::merge_meta_for_session(row, cx)),
                inner.session_over(),
            )
        };
        let merge = crate::changes_bar::merge_when_live(merge, over);
        if !crate::changes_bar::changes_bar_visible(has_diff, merge.is_some()) {
            return None;
        }
        let state = self.changes.as_ref();
        let expanded = state.is_some_and(|state| state.expanded);
        let totals = state.map(|state| (state.additions, state.deletions));
        Some(crate::changes_bar::render(
            crate::changes_bar::ChangesSpec {
                toggle_id: "session-changes-toggle",
                placement: crate::changes_bar::ChangesPlacement::Rail,
                totals,
                expanded,
                merge,
                diff_view: self.changes_diff.clone(),
                on_toggle: Box::new(|this: &mut Self, cx| this.toggle_changes_expanded(cx)),
                on_merged: None,
            },
            cx,
        ))
    }

    // ── Header ────────────────────────────────────────────────────────────

    fn render_header(&mut self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let inner = self.inner.read(cx);
        let (tone, caption) = inner.header_status(cx);
        let (identifier, subject) = inner.header_identity(cx);
        let agent_label = inner.agent_display_label();
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
        // The HOST machine's rate-limit windows: this install's own probe for
        // a run we host, the synced `devices.agent_usage` for one we do not.
        let windows = agent.and_then(|agent| {
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
            .child(div().min_w_0().max_w(px(360.)).truncate().text_sm().child(subject))
            .child(
                crate::surface::glass_pill(
                    "session-agent",
                    crate::surface::PillSize::Sm,
                    crate::surface::PillMode::Readonly,
                    cx,
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(agent_label)),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(match device {
                        Some(device) => format!("{caption} · {device}"),
                        None => caption,
                    })),
            )
            // EXP-746: the session's own context meter. Gone once the run is
            // over (a finished run's live numbers are a snapshot of nothing)
            // and never on a replay, whose numbers are the ones the run ended
            // with, not the ones anything is spending now.
            .when_some(
                usage_summary.filter(|_| !over && self.feed != SessionFeed::Replay),
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
            .when(can_kill, |this| {
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
        let rail = self.render_changes_rail(cx);
        h_flex()
            .size_full()
            .min_h_0()
            .track_focus(&self.focus_handle)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(header)
                    .child(div().flex_1().min_h_0().child(self.inner.clone())),
            )
            .children(rail)
    }
}

#[cfg(test)]
mod tests {
    use super::{feed_source_for, SessionFeed};
    use coding::LaunchTransport;

    /// The source decision in one table: a live local engine always wins, a
    /// replay needs BOTH an ACP recording and an ended run, and everything
    /// else — including every pre-746 record, which reads as the terminal —
    /// falls back to the relay.
    #[test]
    fn feed_source_for_prefers_the_engine_then_a_replayable_record() {
        // A run this process hosts renders its own engine, ended or not: the
        // exit edge arrives on that feed too.
        assert_eq!(
            feed_source_for(true, Some(LaunchTransport::Acp), false),
            SessionFeed::Local
        );
        assert_eq!(
            feed_source_for(true, Some(LaunchTransport::Acp), true),
            SessionFeed::Local
        );
        // An ended ACP run we no longer host replays off its transcript.
        assert_eq!(
            feed_source_for(false, Some(LaunchTransport::Acp), true),
            SessionFeed::Replay
        );
        // Still running elsewhere — the relay is the only live feed.
        assert_eq!(
            feed_source_for(false, Some(LaunchTransport::Acp), false),
            SessionFeed::Remote
        );
        // A PTY-recorded run has no ACP transcript to load, ended or not.
        assert_eq!(
            feed_source_for(false, Some(LaunchTransport::Terminal), true),
            SessionFeed::Remote
        );
        // No local record at all (another machine's run).
        assert_eq!(feed_source_for(false, None, true), SessionFeed::Remote);
    }
}
