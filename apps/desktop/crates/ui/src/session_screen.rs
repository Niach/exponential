//! The session center screen (EXP-746; EXP-1294: one `sessions` row) — one
//! open [`Screen::Session`] tab per session.
//!
//! A session this process hosts is an in-process [`engine::EngineSession`]
//! and renders in the CENTER pane like every other detail screen.
//! Every opener navigates to [`Screen::Session`]: the session card in a
//! chat or an agent channel (`session_chat::open_session_card`), the
//! rail's Running rows and notifications.
//!
//! Three feed sources, one renderer ([`SteerSessionView`]): the in-process
//! engine (`Local`), the relay viewer (`Remote` — another machine, or another
//! process on this one), and an ended ACP run replayed off disk (`Replay`).
//! [`feed_source_for`] is that decision, pure and unit-tested; materializing
//! it is [`resolve_source`].
//!
//! EXP-1294: this screen owns the CHROME around the transcript — a
//! `← <chat title>` back link to the session's chat (its tree root), the
//! session's own title beside the agent's mark (S-3: no Stop pill; the
//! composer's stop-turn is the one stop). No face toggle, no issue tray, no
//! Resume: a message resumes a session (the composer). The transcript view
//! renders headerless.
//!
//! Lifetime rule: a view lives exactly as long as its TAB. `ScreensPanel`
//! creates it on first activation and calls [`SessionScreenView::shutdown`]
//! when the tab closes — which drops the feed, never the session: closing a
//! tab must not end an agent. An ended session keeps its tab as a transcript,
//! which is what [`SessionScreenView::mark_ended`] marks when the engine
//! reports the exit before the row syncs.

use gpui::{
    div, px, AnyElement, App, AppContext as _, Entity, FocusHandle,
    Focusable, InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use crate::coding_flow::LocalSessions;
use crate::icons::registry;
use crate::navigation::Screen;
use crate::steer_viewer::{FeedSource, SteerSessionView};

/// Mark every open screen for `session_id` ended (the engine's `on_exit` edge,
/// which can precede the synced row's flip by a round trip). The tab stays.
///
/// EXP-758: `error` is `EngineExit::error`, a handshake or transport failure.
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
pub(crate) fn resolve_source(session_id: &str, cx: &mut App) -> (SessionFeed, FeedSource) {
    let engine = local_engine(session_id, cx);
    let ended = crate::session_data::session(session_id, cx).is_some_and(|row| row.is_ended());
    // EXP-773: the DEVICE's own activity journal is the transcript of a past
    // run — the exact events the relay fanned out, on disk, with no agent to
    // respawn. It is preferred for that reason; the engine's `session/load`
    // replay stays the fallback for a run recorded before journals existed.
    // #LOGS-7: the disk is read only when a replay can be the answer (an
    // ended run with no live engine): a live chat opened without parsing
    // the whole run registry.
    let (journal, record) = if engine.is_none() && ended {
        let journal = has_journal(session_id, cx);
        let record = (!journal)
            .then(|| coding::run_registry::get(&crate::coding_flow::coding_data_dir(cx), session_id))
            .flatten();
        (journal, record)
    } else {
        (false, None)
    };
    let feed = feed_source_for(engine.is_some(), journal || record.is_some(), ended);
    match (feed, engine) {
        (SessionFeed::Local, Some(session)) => (SessionFeed::Local, FeedSource::Local { session }),
        (SessionFeed::Replay, _) => match journal {
            true => (SessionFeed::Replay, FeedSource::journal()),
            false => match record.and_then(|record| open_transcript(&record, cx)) {
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

/// EXP-773 — whether this device kept a journal for `session_id`. `false`
/// when the file is not there (or empty): this machine never ran the
/// session, or the retention prune (EXP-886, Settings → Sessions) took it,
/// and the caller falls back to the engine replay and then to the relay.
/// Large chats: only the file's size is asked here; the view reads it off
/// the main thread, and folds only its REPLAY TAIL
/// ([`crate::steer_viewer::read_journal_tail`]).
fn has_journal(session_id: &str, cx: &App) -> bool {
    steer::journal_path(&crate::coding_flow::coding_data_dir(cx), session_id)
        .and_then(|path| std::fs::metadata(path).ok())
        .is_some_and(|meta| meta.len() > 0)
}

/// Replay `record`'s conversation through the engine (ACP `session/load`).
/// `None` when there is nothing to load it with — the caller then falls back
/// to the relay and the "no transcript" banner.
fn open_transcript(record: &coding::run_registry::RunRecord, cx: &App) -> Option<engine::EngineSession> {
    let runtime = crate::steer_wiring::runtime(cx)?;
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
pub(crate) fn local_engine(session_id: &str, cx: &App) -> Option<engine::EngineSession> {
    let sessions = LocalSessions::global_ref(cx)?;
    Some(sessions.read(cx).session_by_id(session_id)?.host.session.clone())
}

/// Where the back link goes and what it says: the session's tree root — a
/// chat's screen under its title, or an agent's channel under its name.
fn back_target(session_id: &str, cx: &App) -> Option<(Screen, String)> {
    let root = crate::session_data::root_of(session_id, cx)?;
    if root.is_agent() {
        // NEW-6: the agent's page, under the `agents` row's name.
        let agent_id = root.agent_id.clone().unwrap_or_else(|| root.id.clone());
        let name = crate::session_data::agent_def(&agent_id, cx)
            .map(|agent| crate::agent_screen::agent_def_name(&agent))
            .unwrap_or_default();
        return Some((Screen::Agent { agent_id }, name));
    }
    let title = crate::session_data::session_title_of(&root.id, cx)
        .unwrap_or_else(|| domain::session_tree::SESSION_CHAT.copy.untitled.clone());
    Some((
        Screen::Chat {
            session_id: root.id.clone(),
        },
        title,
    ))
}

/// The back link row: `← <title>`, muted, foreground on hover.
fn back_link(screen: Screen, title: String, cx: &App) -> AnyElement {
    // Back to the root when it is the previous entry (no ping-pong).
    back_link_row("session-back-link", title, move |window, cx| {
        crate::navigation::go_back_to(window, cx, screen.clone());
    }, cx)
}

/// #FEED-191 (chats.json THE WAY BACK): the Back of a chat a session
/// link opened: THE back link row naming the screen it left, popping the
/// history (web `‹`).
pub(crate) fn session_link_back(window: &Window, cx: &App) -> Option<AnyElement> {
    let previous = crate::navigation::session_back_screen(window, cx)?;
    let title = crate::navigation::screen_title(&previous, cx).to_string();
    Some(back_link_row("chat-back-link", title, |window, cx| crate::navigation::go_back(window, cx), cx))
}

/// THE back link row's look (#EXP-1388: the Archived page's `← Chats` too); a
/// name not synced yet reads `Back`, never a bare arrow.
pub(crate) fn back_link_row(
    id: &'static str,
    title: String,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let hover = cx.theme().foreground;
    let title = if title.is_empty() { "Back".to_string() } else { title };
    h_flex()
        .id(id)
        .role(gpui::Role::Link)
        .aria_label(SharedString::from(title.clone()))
        .debug_selector(move || id.to_string())
        .min_w_0()
        .gap_1p5()
        .items_center()
        .cursor_pointer()
        .text_sm()
        .text_color(muted)
        .hover(move |style| style.text_color(hover))
        .child(Icon::new(registry::UI_BACK).small())
        .child(div().min_w_0().truncate().child(SharedString::from(title)))
        .on_click(move |_, window, cx| on_click(window, cx))
        .into_any_element()
}

/// #VAPP-229 (via-app.json SESSION VIEW): one muted text-xs `via <app>`
/// line under the header's title / origin row when an app's tool started
/// the session and that app still syncs; `None` otherwise (the chat header
/// itself stays caption-free).
pub(crate) fn via_app_line(row: Option<&domain::rows::SessionRow>, cx: &App) -> Option<AnyElement> {
    let app = crate::comments::synced_app_name(row?.via_app_id.as_deref(), cx);
    let caption = domain::via_app::via_app_caption(app.as_deref())?;
    Some(
        div()
            .id("session-via-app")
            .w_full()
            .min_w_0()
            .truncate()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(SharedString::from(caption))
            .into_any_element(),
    )
}

/// One session's center screen.
pub(crate) struct SessionScreenView {
    session_id: String,
    /// The ONE transcript renderer, over whichever source [`resolve_source`]
    /// picked. It renders headerless: this screen paints the header.
    inner: Entity<SteerSessionView>,
    /// The engine reported its exit (D2). The synced row carries the same
    /// truth a round trip later — the inner view reads that one itself.
    ended: bool,
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
    /// handle on a live local engine.
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
        // the header's Stop moves; the tree (titles, the root) rides the
        // session collections.
        let mut subscriptions = vec![cx.observe(&inner, |_: &mut Self, _, cx| cx.notify())];
        subscriptions.extend(crate::session_data::observe_session_collections(cx));
        Self {
            session_id,
            inner,
            ended: false,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// EXP-1316: the transcript a chat hosts IN PLACE (its own header
    /// stays unused there).
    pub(crate) fn inner(&self) -> &Entity<SteerSessionView> {
        &self.inner
    }

    /// The tab closed. Drops the feed (the relay socket, the engine drain) —
    /// deliberately NOT the session: an agent keeps working when its tab goes
    /// away.
    pub(crate) fn shutdown(&mut self, cx: &mut App) {
        self.inner.update(cx, |view, _| view.shutdown());
    }

    /// The session is over — the HOST's edge (the engine's `on_exit`), which
    /// can beat both the synced row and the engine's own feed closing.
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

    /// EXP-1294 — the header: the back link to the chat over the
    /// session's title (the agent's mark beside it). S-3: no Stop pill (the
    /// composer's stop-turn is the one stop).
    fn render_header(&self, window: &Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let can_go_back = crate::navigation::window_can_go_back(window, cx);
        let theme = cx.theme();
        let now_ms = crate::session_data::now_ms();
        let card = crate::session_data::session_card_of(&self.session_id, now_ms, cx);
        let row = self.inner.read(cx).session_row().cloned();
        // chats.json THE WAY BACK: a root not synced yet has no target to name:
        // the history's Back stands in.
        let back = back_target(&self.session_id, cx)
            .map(|(screen, title)| back_link(screen, title, cx))
            .or_else(|| {
                can_go_back.then(|| {
                    back_link_row("session-back-link", String::new(), |window, cx| crate::navigation::go_back(window, cx), cx)
                })
            });
        let mark = row.as_ref().map(|row| {
            let state = domain::session_tree::session_state(row);
            crate::run_rows::run_mark(crate::run_rows::run_agent(row.cli.as_deref()), state, cx)
        });
        let title: AnyElement = match card {
            Some(card) => div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(crate::work_header::TITLE_SIZE))
                .line_height(px(crate::work_header::TITLE_LINE))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.foreground)
                .child(SharedString::from(card.name))
                .into_any_element(),
            None => div()
                .flex_1()
                .child(crate::work_header::collapsed_title_placeholder(
                    "session-title-placeholder",
                    cx,
                ))
                .into_any_element(),
        };
        let column = v_flex()
            .w_full()
            .min_w_0()
            .gap_1()
            .pt(px(crate::work_header::TITLE_PT))
            .pb(px(crate::work_header::TITLE_PB))
            .children(back)
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .items_center()
                    .min_h(px(crate::work_header::BAR_H))
                    .children(mark)
                    .child(title),
            )
            .children(via_app_line(row.as_ref(), cx));
        v_flex()
            .w_full()
            .flex_shrink_0()
            .child(crate::steer_viewer::work_column_row(column.into_any_element()))
            .into_any_element()
    }
}

impl Focusable for SessionScreenView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SessionScreenView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "SessionScreenView").entered();
        // EXP-1323: the session on screen is read (its own agent replies).
        crate::session_data::mark_read_if_unread(&self.session_id, false, cx);
        let header = self.render_header(window, cx);
        // One column — the header, then the transcript, whose own footer
        // carries the composer (and the usage readout).
        v_flex()
            .size_full()
            .min_w_0()
            .min_h_0()
            .track_focus(&self.focus_handle)
            .child(header)
            // EXP-1162: the top edge strip under the header; the transcript's
            // own bottom strip sits above the composer (`steer_viewer`).
            .child(crate::work_header::work_body(
                div().flex_1().min_h_0().child(self.inner.clone()).into_any_element(),
                false,
                self.inner.read(cx).body_scrolled(cx),
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::{feed_source_for, SessionFeed, SessionScreenView};
    use gpui::AppContext as _;
    use sync::{ShapeRow as _, Store};

    /// chats.json THE WAY BACK: a child of an agent's chat opens on this screen,
    /// and its header always holds the way back: the chat by name, `Back` with
    /// the agent not synced; with no root at all, the history's.
    #[gpui::test]
    #[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
    fn a_child_of_an_agent_chat_always_has_a_back_link(cx: &mut gpui::TestAppContext) {
        const BACK: &str = "session-back-link";
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
            let store = Store::open(cx, None, None);
            cx.set_global(store);
            crate::queries::set_own_device_id_for_test("d1", cx);
            let collections = Store::global(cx).collections().clone();
            let session = |id: &str, parent: Option<&str>, agent: Option<&str>| -> domain::rows::SessionRow {
                serde_json::from_value(serde_json::json!({
                    "id": id,
                    "team_id": "t1",
                    "root_id": "chat",
                    "parent_id": parent,
                    "agent_id": agent,
                    "status": "idle",
                    "shared": true,
                    "created_at": "2026-11-04T09:00:00.000Z",
                    "updated_at": "2026-11-04T09:00:00.000Z",
                }))
                .unwrap()
            };
            collections.sessions.update(cx, |collection, cx| {
                for row in [session("chat", None, Some("ag1")), session("chat-child", Some("chat"), None)] {
                    collection.seed(row.key(), row);
                }
                cx.notify();
            });
        });
        let draw = |cx: &mut gpui::VisualTestContext| {
            for _ in 0..3 {
                cx.update(|window, cx| window.draw(cx).clear(cx));
                cx.run_until_parked();
            }
        };
        let (_child, vcx) = cx.add_window_view(|window, cx| SessionScreenView::new("chat-child".to_string(), window, cx));
        draw(vcx);
        assert!(vcx.debug_bounds(BACK).is_some(), "the way back to the chat");

        let (_missing, vcx) = cx.add_window_view(|window, cx| SessionScreenView::new("unsynced".to_string(), window, cx));
        draw(vcx);
        assert!(vcx.debug_bounds(BACK).is_none(), "nowhere to go yet");
        vcx.update(|window, cx| {
            crate::navigation::nav_for_window(window, cx);
            crate::navigation::navigate(window, cx, crate::navigation::Screen::NeedsYou);
            crate::navigation::navigate(window, cx, crate::navigation::Screen::Session { session_id: "unsynced".into() });
        });
        draw(vcx);
        assert!(vcx.debug_bounds(BACK).is_some(), "the history's Back");
    }

    /// The source decision in one table: a live local engine always wins, a
    /// replay needs BOTH a transcript on this machine and an ended run, and
    /// everything else is the remote viewer (eNet, VAPP-43).
    #[test]
    fn feed_source_for_prefers_the_engine_then_a_local_transcript() {
        // A run this process hosts renders its own engine, ended or not: the
        // exit edge arrives on that feed too.
        assert_eq!(feed_source_for(true, true, false), SessionFeed::Local);
        assert_eq!(feed_source_for(true, true, true), SessionFeed::Local);
        // An ended run we no longer host replays off its on-device transcript.
        assert_eq!(feed_source_for(false, true, true), SessionFeed::Replay);
        // Still running elsewhere — the eNet viewer is the only live feed.
        assert_eq!(feed_source_for(false, true, false), SessionFeed::Remote);
        // Nothing on this machine (another machine's run, or a pruned
        // journal): the eNet viewer, which that device answers off its journal.
        assert_eq!(feed_source_for(false, false, true), SessionFeed::Remote);
        assert_eq!(feed_source_for(false, false, false), SessionFeed::Remote);
    }
}
