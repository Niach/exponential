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
//! Lifetime rule: a view lives exactly as long as its TAB. `ScreensPanel`
//! creates it on first activation and calls [`SessionScreenView::shutdown`]
//! when the tab closes — which drops the feed, never the run: closing a tab
//! must not end an agent (kill is its own affordance). An ended run keeps its
//! tab as a read-only transcript, which is what [`SessionScreenView::mark_ended`]
//! marks when the engine reports the exit before the row syncs.
//!
//! P0-g landed the skeleton: the screen wiring, the source resolution and the
//! host lifecycle. Lane D3 replaces the render tree (header, extras, chips,
//! banners, the Changes rail and the usage sheet).

use gpui::{
    div, prelude::FluentBuilder as _, App, AppContext as _, Entity, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString, Styled, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use crate::coding_flow::{LocalSessionHost, LocalSessions};
use crate::navigation::{screen_title, Screen};
use crate::steer_viewer::{FeedSource, SteerSessionView};

/// Open `session_id`'s surface in this window.
///
/// A run THIS process hosts on the PTY path keeps its dock terminal — there is
/// no ACP conversation to render and nothing to steer remotely about a child
/// whose grid is right there. Everything else (a local ACP run, a run on
/// another machine, an ended one) is a center screen.
// EXP-746: the callers land with their lanes — D2 (a local ACP start), D5
// (Devices → Running / Past) and D6 (the issue header's pill).
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
// EXP-746 D2: `DesktopEngineHost::on_exit` calls this.
#[allow(dead_code)]
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
    let recorded = crate::window_size::app_data_dir()
        .and_then(|data_dir| coding::run_registry::get(&data_dir, session_id))
        .map(|record| record.transport());
    let feed = feed_source_for(engine.is_some(), recorded, row_ended(session_id, cx));
    let source = match (feed, engine) {
        (SessionFeed::Local, Some(session)) => FeedSource::Local { session },
        // EXP-746 D3: fill — `EngineSession::open_transcript` is an E1 stub,
        // and the replay also wants the personal key D2 extracts. Until both
        // land an ended ACP run renders the relay's journal, exactly like
        // every PTY-recorded run does; nothing else about the screen changes.
        _ => FeedSource::Remote { handle: None },
    };
    (feed, source)
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
    /// picked. Lane D3 wraps it in the real header / chips / rail.
    inner: Entity<SteerSessionView>,
    feed: SessionFeed,
    /// The engine reported its exit (D2). The synced row carries the same
    /// truth a round trip later — the inner view reads that one itself.
    ended: bool,
    focus_handle: FocusHandle,
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
        let inner =
            cx.new(|cx| SteerSessionView::with_source(session_id.clone(), source, window, cx));
        Self {
            session_id,
            inner,
            feed,
            ended: false,
            focus_handle: cx.focus_handle(),
        }
    }

    /// The tab closed. Drops the feed (the relay socket, later the engine
    /// drain) — deliberately NOT the run: an agent keeps working when its tab
    /// goes away, and Past reopens the transcript.
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

    /// Which feed this screen renders — the banner and the composer gate read
    /// it (lane D3).
    #[allow(dead_code)] // consumed by D3's render tree
    pub(crate) fn feed(&self) -> SessionFeed {
        self.feed
    }
}

impl Focusable for SessionScreenView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SessionScreenView {
    // EXP-746 D3: fill — this is the placeholder header over the shared
    // transcript view, so every lane after P0-g can already open a session.
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let title: SharedString = screen_title(
            &Screen::Session {
                session_id: self.session_id.clone(),
            },
            cx,
        );
        let header = h_flex()
            .w_full()
            .px_3()
            .py_2()
            .gap_2()
            .border_b_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            .child(div().text_sm().child(title))
            .when(self.ended, |this| {
                this.child(div().text_xs().text_color(muted).child("Ended"))
            });
        v_flex()
            .size_full()
            .track_focus(&self.focus_handle)
            .child(header)
            .child(div().flex_1().min_h_0().child(self.inner.clone()))
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
