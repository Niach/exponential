//! The IDE's steering VIEWER (EXP-696) — the desktop twin of the web
//! `AgentSessionView`, living in the bottom dock next to the local terminal
//! tabs.
//!
//! The dock's chip strip lists this machine's terminal tabs AND the user's
//! OTHER live coding sessions (another desktop, the headless CLI daemon, a
//! shared server). Clicking one of those chips swaps the dock's content for
//! a [`SteerSessionView`]: the same activity feed + composer the web dock
//! renders, driven by [`steer::spawn_viewer`] over the relay.
//!
//! ## What lives where
//!
//! * **Transport** — `steer::viewer`. It hands us [`ViewerEvent`]s on a
//!   `flume` channel and takes messages/answers/keystrokes back through
//!   [`ViewerHandle`]. Nothing here speaks the wire.
//! * **Model** — [`steer::SteerFeed`], the pure reducer. This view only
//!   pumps events into it and renders [`SteerFeed::rows`].
//! * **Timers** — the feed is deliberately clock-free, so the two timers are
//!   ours: the [`ANSWER_ACK_TIMEOUT`] card lock, and the EXP-656 staged
//!   replay's [`REPLAY_QUIET`]/[`REPLAY_MAX`] fallback swap. Both are plain
//!   `cx.spawn` sleeps guarded by a generation counter.
//! * **Row truth** — the synced `coding_sessions` row (identity, device,
//!   `needs_input`, `status`). The view observes the collection and feeds
//!   `ended` back to the socket with
//!   [`ViewerHandle::note_session_ended`] so its redial loops stop.
//! * **Wakeups** — the three edges the web store's `kickAll` listens on
//!   (foreground, network, host device online) are wired here to
//!   [`ViewerHandle::kick`]; see the "Wakeups" section below. Without them a
//!   woken laptop waits out the transport's staleness window and backoff.
//!
//! EXP-698 closed the two biggest gaps: the pinned **Latest changes** strip
//! and the in-session **Merge** pill now render for a steered session too.
//! The old rationale ("a remote run's diff is not on this machine") was
//! wrong — the host publishes its worktree diff on the activity channel and
//! [`SteerFeed::latest_diff`] holds it; the dock parses it with the same
//! unified-diff reader the local arm uses and resolves the merge target off
//! the synced `coding_sessions` row. The bar itself lives in
//! [`crate::terminal_dock`], which owns the dock's content area.
//!
//! ## Deliberate parity gaps vs the web view (EXP-696)
//!
//! * no subagent conversation TAB strip — subagent work renders inline as
//!   expandable group rows, which is the part of parity that matters;
//! * no fullscreen toggle — the dock's own resize/undock chrome is the
//!   desktop's answer to that.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    bounce, div, ease_in_out, prelude::FluentBuilder as _, px, relative, AnimationExt as _,
    AnyElement, App, AppContext as _, ClickEvent, Entity, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, StyledImage as _, Styled as _, Subscription, Task, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{self, Input, InputEvent, InputState, Textarea, TextareaState},
    spinner::Spinner,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _,
};
use steer::activity::SessionAgent;
use steer::commands::{parse_command, SteerCommand};
use steer::feed::{COMPACTED_LABEL, COMPACTING_LABEL, COMPACTION_TIMEOUT};
use steer::{
    answer_key, build_steer_image_message, insert_image_marker, parse_steer_message,
    renumber_image_markers, summarize_subagent_row, AnswerStatus, FeedItem, FeedItemId, FeedKind, FeedRow, QuestionOption,
    SteerFeed, SubagentStatus, ViewerEvent, ViewerHandle, ViewerPhase, ANSWER_ACK_TIMEOUT,
    MAX_STEER_IMAGES, REPLAY_MAX, REPLAY_QUIET,
};

use crate::controls::WebText as _;
use crate::icons::registry;
use crate::slash_commands;
use crate::markdown::image_paste::{
    self, max_upload_bytes_for, pasted_image_parts, read_image_file, validate_image,
};
use crate::native_dialog::{self, AlertSpec};

/// How long a body may run before it folds behind "Show more" (web
/// `clampable`: >600 chars or >6 lines).
const CLAMP_CHARS: usize = 600;
const CLAMP_LINES: usize = 6;

/// Free-text answers are capped like the web input.
const FREE_TEXT_MAX: usize = 4000;

/// How often the staged-replay fallback re-checks its quiet window.
const STAGING_TICK: Duration = Duration::from_millis(100);

/// The pending strip's thumbnail edge (web/iOS parity).
const PENDING_THUMB: f32 = 48.;

/// EXP-724: the open `/` command menu. `items` is already filtered for the
/// session's agent and the typed prefix ([`slash_commands::menu_matches`]);
/// `selected` wraps under ↑/↓.
struct SlashMenu {
    items: Vec<SteerCommand>,
    selected: usize,
}

/// One image staged in the composer, uploaded on send.
struct PendingImage {
    key: u64,
    filename: String,
    content_type: String,
    /// EXP-698: the staged bytes, wrapped for `img()` — the thumbnail's
    /// source AND the upload's. `gpui::Image` owns a public `bytes: Vec<u8>`,
    /// so the ONE buffer serves both: a separate `Arc<Vec<u8>>` beside it
    /// would hold a second copy of every pasted screenshot for as long as the
    /// draft lives. Built once here, never per repaint.
    preview: Arc<gpui::Image>,
    /// Set once the attachment landed — a retry after a mid-batch failure
    /// never re-uploads what already succeeded.
    uploaded_id: Option<String>,
}

/// The steering view for ONE remote coding session.
pub(crate) struct SteerSessionView {
    session_id: String,
    /// The synced row, re-snapshotted whenever `coding_sessions` notifies.
    row: Option<domain::rows::CodingSession>,
    feed: SteerFeed,
    handle: Option<ViewerHandle>,
    phase: ViewerPhase,
    connected: bool,
    /// EXP-696 wakeups: the last seen edge states, so only a TRANSITION back
    /// to reachable nudges the socket (an observer fires on plenty of
    /// non-edges).
    device_offline: bool,
    sync_offline: bool,
    /// Bumped on every reset/activity while staging; a fallback timer that
    /// finds its generation superseded exits without swapping.
    staging_generation: u64,
    /// When the CURRENT staging window opened. The quiet window restarts on
    /// every buffered event; the [`REPLAY_MAX`] cap deliberately does not —
    /// a replay that keeps trickling must still commit.
    staging_started: Option<std::time::Instant>,
    /// Bumped whenever a compaction opens; a [`COMPACTION_TIMEOUT`] backstop
    /// that finds its generation superseded exits without clearing.
    compaction_generation: u64,
    /// Composer.
    input: Entity<TextareaState>,
    /// EXP-724: the open `/` menu, refreshed on every draft change.
    slash: Option<SlashMenu>,
    /// The exact draft Escape dismissed the menu for — it stays shut until
    /// the draft changes again (and after an accept, so inserting `/clear`
    /// does not immediately re-open the menu on its own result).
    slash_dismissed_for: Option<String>,
    pending: Vec<PendingImage>,
    next_pending_key: u64,
    sending: bool,
    notice: Option<SharedString>,
    /// Question-card local state, keyed by `answer_key`.
    picked: HashMap<String, Vec<String>>,
    /// The open free-text row: `(answer key, option key)`.
    free_text: Option<(String, String)>,
    free_text_input: Entity<InputState>,
    /// Expanded tool-run / subagent group rows, and expanded long bodies.
    expanded_groups: HashSet<FeedItemId>,
    expanded_bodies: HashSet<FeedItemId>,
    scroll: ScrollHandle,
    focus_handle: FocusHandle,
    _drain: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl SteerSessionView {
    /// Build the view and dial the relay. The socket stays up for the view's
    /// whole life — the dock creates one lazily on the first chip click and
    /// keeps it until the session's row leaves the live set.
    pub(crate) fn new(
        session_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            crate::controls::web_textarea(1, 6, window, cx)
                // Enter sends, Shift+Enter inserts a newline (web parity).
                .submit_on_enter(true)
                // EXP-724: the `/` menu is invisible until it is typed, so
                // the placeholder is the only hint it exists. Every agent's
                // catalog is non-empty, which is why this is a constant here
                // and a conditional on web/Android.
                .placeholder("Message the agent… (/ for commands)")
        });
        let free_text_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Type your answer…"));

        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe_in(
            &input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => this.send(window, cx),
                // EXP-724: the `/` menu is a pure function of the draft, so
                // the change event is the only thing that opens or closes it.
                InputEvent::Change => {
                    this.refresh_slash(cx);
                    cx.notify();
                }
                _ => {}
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &free_text_input,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.submit_free_text(window, cx);
                }
            },
        ));
        if let Some(store) = sync::Store::try_global(cx) {
            let collections = store.collections().clone();
            let shared_state = store.state();
            subscriptions.push(cx.observe(&collections.coding_sessions, |this, _, cx| {
                this.refresh_row(cx);
            }));
            subscriptions.push(cx.observe(&collections.issues, |_, _, cx| cx.notify()));
            // EXP-696 wakeup #1: the host machine came back (the devices
            // shape's `last_seen_at` moved back inside the online window).
            subscriptions.push(cx.observe(&collections.devices, |this, _, cx| {
                this.note_device_edge(cx);
                cx.notify();
            }));
            // …#2: our own connectivity came back. `SharedState` notifies on
            // the Ok ⇄ Offline transition, which is this app's `online`
            // event — the control channel keeps no connect callback of its
            // own, and the same outage that killed its socket killed the
            // viewer's.
            subscriptions.push(cx.observe(&shared_state, |this, _, cx| {
                this.note_network_edge(cx);
            }));
        }
        // …#3: the window came forward — the desktop's `visibilitychange`.
        // A lid closed for hours leaves a half-open socket behind that no
        // close frame is ever coming for; this is the same edge
        // `Store::kick_if_stale` hangs off in the shell.
        subscriptions.push(cx.observe_window_activation(window, |this, window, _cx| {
            if window.is_window_active() {
                this.wake("window activated");
            }
        }));

        // The socket task publishes into this channel; the drain applies each
        // event on the gpui foreground (the `steer_wiring` recipe).
        let (events_tx, events_rx) = flume::unbounded::<ViewerEvent>();
        let drain = cx.spawn(async move |this, cx| {
            while let Ok(event) = events_rx.recv_async().await {
                if this
                    .update(cx, |this, cx| this.apply_event(event, cx))
                    .is_err()
                {
                    return;
                }
            }
        });

        let mut this = Self {
            session_id: session_id.clone(),
            row: None,
            feed: SteerFeed::new(),
            handle: None,
            phase: ViewerPhase::Connecting,
            connected: false,
            device_offline: false,
            sync_offline: false,
            staging_generation: 0,
            staging_started: None,
            compaction_generation: 0,
            input,
            slash: None,
            slash_dismissed_for: None,
            pending: Vec::new(),
            next_pending_key: 0,
            sending: false,
            notice: None,
            picked: HashMap::new(),
            free_text: None,
            free_text_input,
            expanded_groups: HashSet::new(),
            expanded_bodies: HashSet::new(),
            scroll: ScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            _drain: drain,
            _subscriptions: subscriptions,
        };
        this.refresh_row(cx);
        // Seed the wakeup edges from the world as it is right now, so the
        // first observer call is a comparison rather than a false edge.
        this.device_offline = this.device(cx).offline;
        this.sync_offline = sync_offline(cx);
        this.handle = spawn_viewer(&session_id, events_tx, cx);
        if this.handle.is_none() {
            this.phase = ViewerPhase::Unauthorized {
                detail: Some("Live steering is unavailable on this instance.".to_string()),
            };
        }
        this
    }

    /// Take focus when the dock swaps this view in — otherwise keystrokes
    /// keep going to the terminal grid the steer view just covered.
    pub(crate) fn focus_composer(&self, window: &mut Window, cx: &mut App) {
        let handle = if self.composer_visible() {
            self.input.focus_handle(cx)
        } else {
            self.focus_handle.clone()
        };
        window.focus(&handle, cx);
    }

    /// EXP-698 — the relay-delivered worktree diff behind the dock's
    /// "Latest changes" bar. The host publishes it with every activity
    /// frame, so a REMOTE run's diff is on this machine after all: the bar
    /// used to be local-tabs-only on the stale rationale that it was not.
    pub(crate) fn latest_diff(&self) -> Option<&str> {
        self.feed.latest_diff()
    }

    /// The synced `coding_sessions` row behind this viewer — what the dock
    /// resolves the Merge target from ([`crate::terminal_dock`]'s
    /// `merge_meta_for_session`).
    pub(crate) fn session_row(&self) -> Option<&domain::rows::CodingSession> {
        self.row.as_ref()
    }

    /// Whether the run is over — a merged/ended session offers no Merge
    /// (web/iOS `canMerge` gate their pill on the same liveness).
    pub(crate) fn session_over(&self) -> bool {
        self.row_ended() || matches!(self.phase, ViewerPhase::Ended { .. })
    }

    /// The dock calls this when the chip goes away (the row ended, the user
    /// signed out, the window closed).
    pub(crate) fn shutdown(&mut self) {
        // EXP-724: nothing is coming to close an open compaction strip once
        // the socket is gone.
        self.feed.clear_compaction();
        if let Some(handle) = self.handle.as_ref() {
            handle.note_session_ended();
            handle.shutdown();
        }
    }

    // ── Synced row ─────────────────────────────────────────────────────────

    fn refresh_row(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(store) = sync::Store::try_global(cx) else {
            return;
        };
        let row = store
            .collections()
            .coding_sessions
            .read(cx)
            .get(&self.session_id)
            .cloned();
        let ended = row
            .as_ref()
            .and_then(|row| row.status.clone())
            .as_deref()
            .map(|status| status == domain::contract::CODING_SESSION_STATUS_ENDED)
            .unwrap_or(false);
        if ended {
            // EXP-639: the redial loops treat the synced row as the truth —
            // a `no_such_session` for an ended run would otherwise park the
            // viewer in `Starting` forever.
            if let Some(handle) = self.handle.as_ref() {
                handle.note_session_ended();
            }
            // EXP-724: a run that ended mid-compaction never sends `ended`.
            self.feed.clear_compaction();
        }
        if self.row != row {
            self.row = row;
            cx.notify();
        }
    }

    fn row_ended(&self) -> bool {
        self.row
            .as_ref()
            .and_then(|row| row.status.as_deref())
            .map(|status| status == domain::contract::CODING_SESSION_STATUS_ENDED)
            .unwrap_or(false)
    }

    fn device(&self, cx: &App) -> crate::queries::SessionDevicePresentation {
        let empty = crate::queries::SessionDevicePresentation {
            label: None,
            offline: false,
        };
        let (Some(row), Some(store)) = (self.row.as_ref(), sync::Store::try_global(cx)) else {
            return empty;
        };
        crate::queries::session_device_presentation(
            row,
            store.collections().devices.read(cx).iter(),
            chrono::Utc::now().timestamp_millis(),
        )
    }

    /// EXP-550: the host machine went offline — the run is paused, not dead,
    /// and no affordance may claim otherwise.
    fn paused(&self, cx: &App) -> bool {
        !self.row_ended() && self.device(cx).offline
    }

    // ── Wakeups (the web store's `kickAll`, EXP-696) ───────────────────────
    //
    // Without these the viewer's only way back from a dead socket is the 45s
    // staleness window plus a 3→30s redial backoff, so a chip could sit in
    // Starting/Reconnecting for half a minute after the laptop woke or the
    // host machine came back. The transport's own `kick` is deliberately
    // conservative (a healthy socket is left alone), so firing these freely
    // costs nothing.

    /// Nudge the socket to redial NOW if it is stuck. A no-op once the loop
    /// has stopped for good (ended, unauthorized, shut down).
    fn wake(&self, reason: &str) {
        let Some(handle) = self.handle.as_ref() else {
            return;
        };
        if !handle.is_active() {
            return;
        }
        log::debug!("[ui] steer viewer {}: kick ({reason})", self.session_id);
        handle.kick();
    }

    /// The host machine flipped back online: its publisher is reachable
    /// again, so stop waiting out the backoff that was drawn while it slept.
    fn note_device_edge(&mut self, cx: &App) {
        let offline = self.device(cx).offline;
        if std::mem::replace(&mut self.device_offline, offline) && !offline {
            self.wake("device online");
        }
    }

    /// OUR connectivity came back (the sync pipeline's Ok ⇄ Offline edge —
    /// this app's `online` event).
    fn note_network_edge(&mut self, cx: &App) {
        let offline = sync_offline(cx);
        if std::mem::replace(&mut self.sync_offline, offline) && !offline {
            self.wake("network back");
        }
    }

    /// EXP-312 mirrors the web `ownsLiveRow` + `live` gate.
    fn can_kill(&self, cx: &App) -> bool {
        let Some(row) = self.row.as_ref() else {
            return false;
        };
        let me = crate::queries::active_account(cx).map(|account| account.user_id);
        self.phase == ViewerPhase::Live
            && !self.paused(cx)
            && row.user_id.is_some()
            && row.user_id == me
            && matches!(
                row.status.as_deref(),
                Some(domain::contract::CODING_SESSION_STATUS_RUNNING)
                    | Some(domain::contract::CODING_SESSION_STATUS_IN_REVIEW)
            )
    }

    // ── Viewer events ──────────────────────────────────────────────────────

    fn apply_event(&mut self, event: ViewerEvent, cx: &mut gpui::Context<Self>) {
        let was_compacting = self.feed.compacting().is_some();
        match event {
            ViewerEvent::Phase(phase) => self.phase = phase,
            ViewerEvent::Connected(connected) => {
                self.connected = connected;
                if !connected && self.feed.is_staging() {
                    // A partial replay of a room we are no longer joined to.
                    self.feed.discard_staging();
                }
            }
            ViewerEvent::Activity(activity) => {
                self.feed.apply(activity);
                if self.feed.is_staging() {
                    self.arm_staging_swap(cx);
                }
            }
            ViewerEvent::Reset => {
                self.feed.apply_reset();
                self.staging_started = Some(std::time::Instant::now());
                self.arm_staging_swap(cx);
            }
            ViewerEvent::Synced => self.feed.apply_synced(),
            ViewerEvent::Keepalive => {
                // EXP-656: a keepalive is also an end-of-replay signal for a
                // markerless republish — the beat means the burst is over.
                if self.feed.is_staging() {
                    self.feed.force_swap();
                }
            }
            ViewerEvent::LocalMessage(text) => {
                self.feed.push_local_message(&text);
            }
        }
        self.note_compaction(was_compacting, cx);
        cx.notify();
    }

    /// EXP-724: the feed reads no clock, so the [`COMPACTION_TIMEOUT`]
    /// backstop that keeps the strip from sticking is ours — armed on the
    /// edge into compacting, wherever that edge lands (a live frame, or the
    /// staged replay's swap).
    fn note_compaction(&mut self, was_compacting: bool, cx: &mut gpui::Context<Self>) {
        if was_compacting || self.feed.compacting().is_none() {
            return;
        }
        self.compaction_generation += 1;
        let generation = self.compaction_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COMPACTION_TIMEOUT).await;
            let _ = this.update(cx, |this, cx| {
                if this.compaction_generation != generation {
                    return;
                }
                this.feed.clear_compaction();
                cx.notify();
            });
        })
        .detach();
    }

    /// EXP-656: a replay that never sends `activity_synced` commits on the
    /// caller's quiet window ([`REPLAY_QUIET`]), and unconditionally at
    /// [`REPLAY_MAX`]. One task per staging window, superseded by generation.
    fn arm_staging_swap(&mut self, cx: &mut gpui::Context<Self>) {
        self.staging_generation += 1;
        let generation = self.staging_generation;
        cx.spawn(async move |this, cx| {
            let mut quiet = Duration::ZERO;
            loop {
                cx.background_executor().timer(STAGING_TICK).await;
                quiet += STAGING_TICK;
                let Ok(done) = this.update(cx, |this, cx| {
                    if this.staging_generation != generation || !this.feed.is_staging() {
                        this.staging_started = None;
                        return true;
                    }
                    let capped = this
                        .staging_started
                        .is_some_and(|started| started.elapsed() >= REPLAY_MAX);
                    if quiet >= REPLAY_QUIET || capped {
                        let was_compacting = this.feed.compacting().is_some();
                        this.feed.force_swap();
                        this.note_compaction(was_compacting, cx);
                        this.staging_started = None;
                        cx.notify();
                        return true;
                    }
                    false
                }) else {
                    return;
                };
                if done {
                    return;
                }
            }
        })
        .detach();
    }

    // ── Answering ──────────────────────────────────────────────────────────

    /// Send one answer and lock the card. Semantic when the card carries a
    /// wire id, raw keystrokes otherwise (EXP-249's legacy path).
    fn answer(
        &mut self,
        item_id: FeedItemId,
        keys: Vec<String>,
        labels: Vec<String>,
        text: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(item) = self.feed.items().iter().find(|item| item.id == item_id) else {
            return;
        };
        let key = answer_key(item);
        if self.feed.is_answer_locked(&key) {
            return;
        }
        let Some(card) = item.question().cloned() else {
            return;
        };
        let Some(handle) = self.handle.as_ref() else {
            return;
        };
        let sent = match card.question_id.as_deref() {
            Some(question_id) => {
                handle.send_answer(question_id, card.ask_id.as_deref(), &keys, text.as_deref())
            }
            None => handle.send_keystrokes(&keys),
        };
        if !sent {
            self.notice = Some(SharedString::from("The session is no longer connected"));
            cx.notify();
            return;
        }
        self.feed.note_answer_sent(&key, keys, labels);
        self.picked.remove(&key);
        self.free_text = None;
        // The ack deadline is the CALLER's (the feed reads no clock).
        let deadline_key = key.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ANSWER_ACK_TIMEOUT).await;
            let _ = this.update(cx, |this, cx| {
                this.feed.fail_answer(&deadline_key);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn submit_free_text(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some((answer, option_key)) = self.free_text.clone() else {
            return;
        };
        let value = self.free_text_input.read(cx).value().to_string();
        // The web input caps at 4000 chars; gpui-component's has no maxLength,
        // so the cap lands here instead (the relay would reject a longer one).
        let trimmed: String = value.trim().chars().take(FREE_TEXT_MAX).collect();
        if trimmed.is_empty() {
            return;
        }
        let Some(item_id) = self
            .feed
            .items()
            .iter()
            .find(|item| answer_key(item) == answer)
            .map(|item| item.id)
        else {
            return;
        };
        self.answer(
            item_id,
            vec![option_key],
            vec![trimmed.clone()],
            Some(trimmed),
            cx,
        );
    }

    // ── Composer ───────────────────────────────────────────────────────────

    /// The web gate, verbatim: a draft goes out only on a LIVE phase with an
    /// open socket (a slow-consumer redial keeps the phase and must still dim
    /// the button) over a row that has not ended.
    fn can_send(&self, cx: &App) -> bool {
        let has_content =
            !self.input.read(cx).value().trim().is_empty() || !self.pending.is_empty();
        !self.sending
            && self.phase == ViewerPhase::Live
            && self.connected
            && !self.row_ended()
            && has_content
    }

    fn composer_visible(&self) -> bool {
        !self.row_ended() && !matches!(self.phase, ViewerPhase::Ended { .. })
    }

    // ── Slash commands (EXP-724) ───────────────────────────────────────────

    /// Which catalog this session's composer offers. A row that names no
    /// agent is a claude run (contract order).
    fn agent(&self) -> SessionAgent {
        self.row
            .as_ref()
            .map_or(SessionAgent::Claude, slash_commands::agent_of)
    }

    /// Re-derive the `/` menu from the draft. Pure in, pure out — the only
    /// state it carries is the Escape dismissal, which lasts exactly as long
    /// as the draft it was pressed on.
    fn refresh_slash(&mut self, cx: &mut gpui::Context<Self>) {
        let draft = self.input.read(cx).value().to_string();
        if self.slash_dismissed_for.as_deref() == Some(draft.as_str()) {
            self.slash = None;
            return;
        }
        self.slash_dismissed_for = None;
        let items = slash_commands::menu_matches(&draft, self.agent());
        self.slash = if items.is_empty() {
            None
        } else {
            let selected = self
                .slash
                .as_ref()
                .map_or(0, |menu| menu.selected)
                .min(items.len() - 1);
            Some(SlashMenu { items, selected })
        };
    }

    fn move_slash(&mut self, delta: isize, cx: &mut gpui::Context<Self>) {
        if let Some(menu) = self.slash.as_mut() {
            let len = menu.items.len() as isize;
            if len > 0 {
                menu.selected = (menu.selected as isize + delta).rem_euclid(len) as usize;
                cx.notify();
            }
        }
    }

    /// Put the highlighted command in the composer. NEVER sends — the user
    /// still presses Enter on the finished draft (web/iOS/Android parity).
    fn accept_slash(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(menu) = self.slash.take() else {
            return;
        };
        let Some(command) = menu.items.get(menu.selected).copied() else {
            return;
        };
        let draft = slash_commands::insertion(&command);
        let position = crate::markdown::byte_offset_to_position(&draft, draft.len());
        // Set BEFORE the write: the value change re-enters `refresh_slash`,
        // and a bare `/clear` would otherwise re-open the menu on its own
        // insertion.
        self.slash_dismissed_for = Some(draft.clone());
        self.input.update(cx, |state, cx| {
            state.set_value(draft, window, cx);
            state.set_cursor_position(position, window, cx);
        });
        cx.notify();
    }

    // -- keyboard capture (runs BEFORE the textarea's own handlers) ---------

    fn on_slash_up(&mut self, _: &input::MoveUp, _: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.slash.is_some() {
            self.move_slash(-1, cx);
            cx.stop_propagation();
        }
    }

    fn on_slash_down(&mut self, _: &input::MoveDown, _: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.slash.is_some() {
            self.move_slash(1, cx);
            cx.stop_propagation();
        }
    }

    fn on_slash_escape(&mut self, _: &input::Escape, _: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.slash.take().is_some() {
            self.slash_dismissed_for = Some(self.input.read(cx).value().to_string());
            cx.stop_propagation();
            cx.notify();
        }
    }

    /// The whole point of the capture: with a menu open Enter ACCEPTS, and
    /// the textarea never emits the `PressEnter` that sends.
    fn on_slash_enter(
        &mut self,
        action: &input::Enter,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.slash.is_some() && !action.shift {
            self.accept_slash(window, cx);
            cx.stop_propagation();
        }
    }

    fn on_slash_tab(
        &mut self,
        _: &input::IndentInline,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.slash.is_some() {
            self.accept_slash(window, cx);
            cx.stop_propagation();
        }
    }

    /// EXP-724: a context-discarding command asks first. The publisher runs
    /// whatever it receives, so the confirm is entirely the client's — same
    /// words on all four viewers.
    fn prompt_command(&mut self, name: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let view = cx.entity().downgrade();
        let opener = window.window_handle();
        let spec = AlertSpec::new(
            slash_commands::confirm_title(name),
            slash_commands::CONFIRM_BODY,
            slash_commands::confirm_button(name),
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            let view = view.clone();
            // The alert's own window is not the one holding the composer.
            let _ = opener.update(cx, move |_, window, cx| {
                if let Some(view) = view.upgrade() {
                    view.update(cx, |this, cx| this.send_confirmed(window, cx));
                }
            });
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    fn send(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if !self.can_send(cx) {
            return;
        }
        let text = self.input.read(cx).value().to_string();
        if let Some(parsed) = parse_command(&text, self.agent()) {
            if parsed.command.confirm {
                self.prompt_command(parsed.command.name, window, cx);
                return;
            }
        }
        self.send_confirmed(window, cx);
    }

    fn send_confirmed(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if !self.can_send(cx) {
            return;
        }
        let text = self.input.read(cx).value().to_string();
        self.notice = None;
        if self.pending.is_empty() {
            if self.deliver(&text) {
                self.clear_draft(window, cx);
            } else {
                self.notice = Some(SharedString::from("The session is no longer connected"));
            }
            cx.notify();
            return;
        }
        // EXP-702: steer images upload to the SESSION route, not the issue's
        // — a screenshot pasted into a steering conversation is not part of
        // the issue's Files, and a batch/action run has no issue at all. That
        // is what retires the old "no issue = no attachment" gate.
        let session_id = self.session_id.clone();
        let Some(transport) = crate::queries::attachment_transport(cx) else {
            self.notice = Some(SharedString::from("Couldn't upload image"));
            cx.notify();
            return;
        };
        // Sequential + idempotent per image: a mid-batch failure keeps the
        // composer intact and a retry only uploads the rest (web parity).
        let jobs: Vec<(u64, Option<String>, String, String, Arc<gpui::Image>)> = self
            .pending
            .iter()
            .map(|image| {
                (
                    image.key,
                    image.uploaded_id.clone(),
                    image.filename.clone(),
                    image.content_type.clone(),
                    // An Arc clone — the bytes themselves are never copied.
                    image.preview.clone(),
                )
            })
            .collect();
        self.sending = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move {
                    let mut resolved: Vec<(u64, String)> = Vec::with_capacity(jobs.len());
                    for (key, uploaded, filename, content_type, staged) in jobs {
                        match uploaded {
                            Some(id) => resolved.push((key, id)),
                            None => {
                                let image = transport
                                    .upload_session(
                                        &session_id,
                                        &filename,
                                        &content_type,
                                        &staged.bytes,
                                    )
                                    .map_err(|err| (resolved.clone(), err.to_string()))?;
                                resolved.push((key, image.id));
                            }
                        }
                    }
                    Ok(resolved)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.sending = false;
                match outcome {
                    Ok(resolved) => {
                        this.note_uploaded(&resolved);
                        let ids: Vec<String> =
                            resolved.into_iter().map(|(_, id)| id).collect();
                        let message = build_steer_image_message(&text, &ids);
                        if this.deliver(&message) {
                            this.clear_draft(window, cx);
                        } else {
                            this.notice =
                                Some(SharedString::from("The session is no longer connected"));
                        }
                    }
                    Err((resolved, error)) => {
                        // Keep what landed so a retry uploads only the rest.
                        this.note_uploaded(&resolved);
                        log::warn!("[ui] steer composer upload failed: {error}");
                        this.notice = Some(SharedString::from("Couldn't upload image"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn note_uploaded(&mut self, resolved: &[(u64, String)]) {
        for (key, id) in resolved {
            if let Some(image) = self.pending.iter_mut().find(|image| image.key == *key) {
                image.uploaded_id = Some(id.clone());
            }
        }
    }

    /// Push a composed message onto the wire. `false` = the socket is down
    /// and the caller keeps its draft.
    fn deliver(&self, message: &str) -> bool {
        self.handle
            .as_ref()
            .is_some_and(|handle| handle.send_message(message))
    }

    fn clear_draft(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.pending.clear();
        self.notice = None;
        self.slash = None;
        self.slash_dismissed_for = None;
        self.input
            .update(cx, |state, cx| state.set_value("", window, cx));
    }

    /// Add clipboard / picked images to the draft, applying the same caps as
    /// the web composer (type + 10 MB + at most [`MAX_STEER_IMAGES`]).
    ///
    /// EXP-698: staging the k-th image also drops `[Image #k]` at the caret,
    /// so a sentence can NAME the picture it means ("crop [Image #2]") and
    /// the agent's numbered manifest lines up with it. The insertion is the
    /// contract's ([`insert_image_marker`]) — only the caret handling is the
    /// component's, so a marker never splits a word.
    fn stage_images(
        &mut self,
        images: Vec<(String, String, Vec<u8>)>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut rejected = false;
        let mut overflow = false;
        for (filename, content_type, bytes) in images {
            if validate_image(&content_type, bytes.len()).is_err()
                || bytes.len() > max_upload_bytes_for(&content_type)
            {
                rejected = true;
                continue;
            }
            if self.pending.len() >= MAX_STEER_IMAGES {
                overflow = true;
                continue;
            }
            let preview = pending_preview(&content_type, bytes);
            self.pending.push(PendingImage {
                key: self.next_pending_key,
                filename,
                content_type,
                preview,
                uploaded_id: None,
            });
            self.next_pending_key += 1;
            self.insert_marker(self.pending.len() as u32, window, cx);
        }
        self.notice = if overflow {
            Some(SharedString::from(format!(
                "Up to {MAX_STEER_IMAGES} images per message"
            )))
        } else if rejected {
            Some(SharedString::from(
                "Only images up to 10 MB can be attached",
            ))
        } else {
            None
        };
        cx.notify();
    }

    /// Insert `[Image #index]` at the composer's caret, padded exactly as the
    /// shared contract pads it. The component does the actual insert so it
    /// owns the caret and the undo entry; the SLICE it inserts is the one
    /// [`insert_image_marker`] would have produced.
    fn insert_marker(&mut self, index: u32, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let (text, caret) = {
            let state = self.input.read(cx);
            (state.value().to_string(), state.cursor())
        };
        let (next, after) = insert_image_marker(&text, caret, index);
        let Some(inserted) = next.get(caret..after) else {
            return;
        };
        let inserted = inserted.to_string();
        self.input
            .update(cx, |state, cx| state.insert(inserted, window, cx));
    }

    /// Drop a staged image and renumber the draft's markers behind it: the
    /// removed image's own `[Image #k]` goes and every higher one slides
    /// down, so the markers keep naming the right pictures.
    fn remove_pending(&mut self, key: u64, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(position) = self.pending.iter().position(|image| image.key == key) else {
            return;
        };
        self.pending.remove(position);
        self.input.update(cx, |state, cx| {
            let text = state.value().to_string();
            let next = renumber_image_markers(&text, position as u32 + 1);
            if next == text {
                return;
            }
            // `set_value` parks the caret at the start of a multi-line field
            // (upstream `InputState::set_value`), which would throw the
            // writer back to the top of their draft for removing a
            // thumbnail. Carry the caret over, clamped into the shortened
            // text and snapped to a char boundary — the same restore the
            // markdown toolbar's transforms do. It focuses the field, which
            // is where the writer was anyway: they are mid-draft.
            let caret = clamp_to_char_boundary(&next, state.cursor());
            let caret = crate::markdown::byte_offset_to_position(&next, caret);
            state.set_value(next, window, cx);
            state.set_cursor_position(caret, window, cx);
        });
        cx.notify();
    }

    fn on_paste(&mut self, _: &input::Paste, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let mut images = Vec::new();
        for entry in item.entries() {
            match entry {
                gpui::ClipboardEntry::Image(image) => {
                    let (mime, filename) = pasted_image_parts(image.format());
                    images.push((filename, mime.to_string(), image.bytes().to_vec()));
                }
                gpui::ClipboardEntry::ExternalPaths(paths) => {
                    for path in paths.paths() {
                        if let Ok((filename, mime, bytes)) = read_image_file(path) {
                            images.push((filename, mime, bytes));
                        }
                    }
                }
                gpui::ClipboardEntry::String(_) => {}
            }
        }
        if images.is_empty() {
            return;
        }
        cx.stop_propagation();
        self.stage_images(images, window, cx);
    }

    fn pick_images(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let read: Vec<(String, String, Vec<u8>)> = paths
                .into_iter()
                .filter(|path| image_paste::is_inline_image_path(path))
                .filter_map(|path| read_image_file(&path).ok())
                .collect();
            if read.is_empty() {
                return;
            }
            let _ = this.update_in(cx, |this, window, cx| this.stage_images(read, window, cx));
        })
        .detach();
    }

    // ── Kill ───────────────────────────────────────────────────────────────

    pub(crate) fn prompt_kill(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let label = self.device(cx).label;
        let view = cx.entity().downgrade();
        let spec = AlertSpec::new(
            "Kill this coding session?",
            kill_description(label.as_deref()),
            "Kill session",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            if let Some(view) = view.upgrade() {
                view.update(cx, |this, cx| this.kill(cx));
            }
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    fn kill(&mut self, cx: &mut gpui::Context<Self>) {
        kill_session(&self.session_id, cx);
    }
}

/// The confirm copy, byte-identical to the web `useKillSession` dialog.
pub(crate) fn kill_description(device_label: Option<&str>) -> String {
    let on_device = match device_label {
        Some(label) if !label.is_empty() => format!(" on {label}"),
        _ => String::new(),
    };
    format!(
        "This force-terminates the terminal{on_device} and ends the session. \
         Uncommitted work in the worktree is kept, but the agent stops immediately."
    )
}

/// `steer.killSession` off the gpui foreground. Shared with the dock chip's X.
pub(crate) fn kill_session(session_id: &str, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        return;
    };
    let session_id = session_id.to_string();
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::steer::kill_session(&trpc, &session_id) {
                log::warn!("[ui] steer kill failed for {session_id}: {err}");
            }
        })
        .detach();
}

/// Whether the active account's sync pipeline is currently calling itself
/// offline — the signal behind the shell's offline strip, reused here as the
/// "the network is back" edge.
fn sync_offline(cx: &App) -> bool {
    sync::Store::try_global(cx)
        .map(|store| store.sync_status(cx).health == sync::SyncHealth::Offline)
        .unwrap_or(false)
}

/// Dial the relay for `session_id`. `None` when the steer runtime never came
/// up or nobody is signed in — the caller then renders the unauthorized
/// banner instead of a dead feed.
fn spawn_viewer(
    session_id: &str,
    events_tx: flume::Sender<ViewerEvent>,
    cx: &mut App,
) -> Option<ViewerHandle> {
    let runtime = crate::steer_wiring::runtime(cx)?;
    let trpc = crate::queries::trpc_client(cx)?;
    let tickets = Arc::new(steer::TrpcViewerTickets {
        trpc: Arc::new(trpc),
        coding_session_id: session_id.to_string(),
    });
    Some(steer::spawn_viewer(
        &runtime,
        tickets,
        session_id.to_string(),
        events_tx,
    ))
}

// ---------------------------------------------------------------------------
// Pure presentation helpers (unit-tested)
// ---------------------------------------------------------------------------

/// The header/tooltip caption for a phase, mirroring the web `phaseLabel`.
pub(crate) fn phase_label(
    phase: &ViewerPhase,
    device: Option<&str>,
    awaiting_input: bool,
    paused: bool,
) -> String {
    if paused {
        return format!("Paused · {} is offline", device.unwrap_or("device"));
    }
    match phase {
        ViewerPhase::Live => {
            let head = if awaiting_input {
                "Needs your input"
            } else {
                "Live"
            };
            match device {
                Some(device) => format!("{head} · {device}"),
                None => head.to_string(),
            }
        }
        ViewerPhase::Starting => "Agent starting…".to_string(),
        ViewerPhase::Connecting => "Connecting…".to_string(),
        ViewerPhase::Reconnecting => "Reconnecting…".to_string(),
        ViewerPhase::Ended { .. } => "Session ended".to_string(),
        ViewerPhase::Unauthorized { .. } => "Disconnected".to_string(),
    }
}

/// "2 of 3" / "3 questions" — the ask stepper's counter (web `askStepperView`).
/// `None` when the ask has a single step (no counter renders there).
pub(crate) fn ask_counter(position: Option<u32>, total: u32) -> Option<String> {
    if total <= 1 {
        return None;
    }
    Some(match position {
        Some(position) => format!("{position} of {total}"),
        None => format!("{total} questions"),
    })
}

/// The subagent group row's status caption: `running · 1 tool call`.
/// Note the singular IS handled here (unlike the tool-run group, which only
/// ever forms at ≥2).
pub(crate) fn subagent_caption(done: bool, tool_count: usize) -> String {
    let state = if done { "done" } else { "running" };
    match tool_count {
        0 => state.to_string(),
        1 => format!("{state} · 1 tool call"),
        n => format!("{state} · {n} tool calls"),
    }
}

/// A body long enough to fold behind "Show more" (web `clampable`).
pub(crate) fn clampable(text: &str) -> bool {
    text.len() > CLAMP_CHARS || text.lines().count() > CLAMP_LINES
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

impl SteerSessionView {
    fn render_header(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let paused = self.paused(cx);
        let device = self.device(cx);
        let awaiting = !self.feed.active_question_ids().is_empty();
        let caption = phase_label(&self.phase, device.label.as_deref(), awaiting, paused);
        let identity = self.identity(cx);
        let can_kill = self.can_kill(cx);

        h_flex()
            .w_full()
            .flex_shrink_0()
            .gap_2()
            .items_center()
            .px_3()
            .py_1p5()
            .border_b_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            .child(status_dot(self.phase_tone(cx, paused, awaiting)))
            .when_some(identity.0, |this, identifier| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(muted)
                        .font_family(theme::terminal::FONT_FAMILY)
                        .child(identifier),
                )
            })
            .child(
                div()
                    .min_w_0()
                    .max_w(px(280.))
                    .truncate()
                    .text_sm()
                    .child(identity.1),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(caption)),
            )
            .when(can_kill, |this| {
                this.child(
                    Button::new("steer-kill")
                        .ghost()
                        .cursor_pointer()
                        .xsmall()
                        .icon(registry::CODING_STOP)
                        .tooltip("Kill session")
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.prompt_kill(window, cx);
                        })),
                )
            })
            .into_any_element()
    }

    /// `(identifier, subject)` — the web `sessionIdentity`.
    fn identity(&self, cx: &App) -> (Option<SharedString>, SharedString) {
        let Some(row) = self.row.as_ref() else {
            return (None, SharedString::from("Coding session"));
        };
        if let Some(issue_id) = row.issue_id.as_deref() {
            if let Some(issue) = sync::Store::try_global(cx)
                .and_then(|store| store.collections().issues.read(cx).get(issue_id).cloned())
            {
                let title = issue.title.trim();
                let subject = if title.is_empty() {
                    "Untitled issue".to_string()
                } else {
                    title.to_string()
                };
                return (
                    Some(SharedString::from(issue.identifier.clone())),
                    SharedString::from(subject),
                );
            }
            return (None, SharedString::from("Issue syncing…"));
        }
        (
            None,
            SharedString::from(
                row.action_name
                    .clone()
                    .unwrap_or_else(|| "Batch run".to_string()),
            ),
        )
    }

    fn phase_tone(&self, cx: &App, paused: bool, awaiting: bool) -> gpui::Hsla {
        if paused || self.row_ended() {
            return cx.theme().muted_foreground.opacity(0.5);
        }
        if awaiting {
            return theme::tokens::YELLOW.to_hsla();
        }
        match self.phase {
            ViewerPhase::Live => theme::tokens::GREEN.to_hsla(),
            ViewerPhase::Ended { .. } | ViewerPhase::Unauthorized { .. } => {
                cx.theme().muted_foreground.opacity(0.5)
            }
            _ => theme::tokens::NEUTRAL.to_hsla(),
        }
    }

    fn render_feed(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        if self.feed.is_empty() {
            let paused = self.paused(cx);
            let body: AnyElement = if paused {
                let device = self.device(cx);
                centered(vec![
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(SharedString::from(paused_title(
                            device.label.as_deref(),
                        )))
                        .into_any_element(),
                    div()
                        .text_xs()
                        .text_color(muted.opacity(0.7))
                        .child(PAUSED_BODY)
                        .into_any_element(),
                ])
            } else if matches!(
                self.phase,
                ViewerPhase::Connecting | ViewerPhase::Starting | ViewerPhase::Reconnecting
            ) {
                centered(vec![
                    Spinner::new().small().into_any_element(),
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(if self.phase == ViewerPhase::Starting {
                            "The agent is starting. Waiting for the live stream…"
                        } else {
                            "Connecting…"
                        })
                        .into_any_element(),
                ])
            } else {
                centered(vec![
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("Waiting for activity…")
                        .into_any_element(),
                    div()
                        .text_xs()
                        .text_color(muted.opacity(0.7))
                        .child(
                            "This session isn't publishing an activity feed. It may be marked \
                             private on the desktop, or the desktop app needs an update.",
                        )
                        .into_any_element(),
                ])
            };
            return div().flex_1().min_h_0().child(body).into_any_element();
        }

        let rows = self.feed.rows();
        let last_row = rows.len().saturating_sub(1);
        let active = self.feed.active_question_ids();
        let live = self.phase == ViewerPhase::Live;
        let working = live
            && !self.row_ended()
            && active.is_empty()
            && !self.feed.is_staging()
            // EXP-724: the compaction strip already says what is happening —
            // a second "Working…" under it is noise (web parity).
            && self.feed.compacting().is_none();
        let mut column = v_flex().w_full().min_w_0().gap_0p5().px_3().py_2();
        for (index, row) in rows.iter().enumerate() {
            column = column.child(self.render_row(row, index == last_row && live, &active, cx));
        }
        if working {
            column = column.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .py_1p5()
                    .child(Icon::new(registry::CODING_ASSISTANT).xsmall().text_color(muted.opacity(0.6)))
                    .child(div().text_xs().text_color(muted).child("Working…")),
            );
        }
        crate::scroll_pane::v_scroll_pane("steer-feed", &self.scroll, column).into_any_element()
    }

    fn render_row(
        &self,
        row: &FeedRow<'_>,
        live_tail: bool,
        active: &HashSet<FeedItemId>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        match row {
            FeedRow::Single(item) => self.render_item(item, active, cx),
            FeedRow::ToolRun { id, items } => self.render_tool_run(*id, items, live_tail, cx),
            FeedRow::Ask { id, items, .. } => self.render_ask(*id, items, active, cx),
            FeedRow::Subagent { id, items, .. } => self.render_subagent(*id, items, cx),
        }
    }

    fn render_item(
        &self,
        item: &FeedItem,
        active: &HashSet<FeedItemId>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        match &item.kind {
            FeedKind::Narration { text } => h_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .items_start()
                .py_1()
                .child(
                    div().mt(px(3.)).flex_shrink_0().child(
                        Icon::new(registry::CODING_ASSISTANT)
                            .xsmall()
                            .text_color(muted.opacity(0.6)),
                    ),
                )
                .child(
                    div().flex_1().min_w_0().text_sm().child(
                        crate::markdown::MarkdownView::new(
                            SharedString::from(format!("steer-narration-{}", item.id)),
                            text.clone(),
                        )
                        // EXP-698: the feed reads at the chat rhythm, and its
                        // inline code takes the semantic tint.
                        .chat(true)
                        .selectable(true),
                    ),
                )
                .into_any_element(),
            // EXP-724: a message that IS a catalog command reads as a
            // compact pill, not a chat bubble — the agent was steered, not
            // spoken to.
            FeedKind::UserMessage { text }
                if parse_command(text, self.agent()).is_some() =>
            {
                let parsed = parse_command(text, self.agent()).expect("matched above");
                h_flex()
                    .w_full()
                    .min_w_0()
                    .justify_end()
                    .pl_8()
                    .py_1()
                    .child(
                        crate::surface::glass_chip()
                            .max_w_full()
                            .child(
                                Icon::new(registry::CODING_COMMAND)
                                    .xsmall()
                                    .text_color(muted.opacity(0.7)),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .font_family(theme::terminal::FONT_FAMILY)
                                    .child(SharedString::from(format!(
                                        "/{}",
                                        parsed.command.name
                                    ))),
                            )
                            .when(!parsed.args.is_empty(), |this| {
                                this.child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_color(muted)
                                        .child(SharedString::from(parsed.args.clone())),
                                )
                            }),
                    )
                    .into_any_element()
            }
            FeedKind::UserMessage { text } => h_flex()
                .w_full()
                .min_w_0()
                .justify_end()
                .pl_8()
                .py_1()
                .child(
                    div()
                        .min_w_0()
                        .rounded(px(12.))
                        .border_1()
                        .border_color(theme::tokens::glass::STROKE_STRONG.to_hsla())
                        .bg(theme::tokens::glass::FILL_ACTIVE.to_hsla())
                        .px_3()
                        .py_2()
                        .text_sm()
                        .child(self.render_user_message(item.id, text, cx)),
                )
                .into_any_element(),
            FeedKind::Tool {
                name,
                detail,
                subagent_id: _,
            } => tool_row(name, detail.as_deref(), cx).into_any_element(),
            FeedKind::Permission { tool, detail } => {
                let amber = theme::tokens::YELLOW.to_hsla();
                v_flex()
                    .w_full()
                    .min_w_0()
                    .py_0p5()
                    .child(
                        h_flex()
                            .min_w_0()
                            .gap_2()
                            .items_center()
                            .child(
                                Icon::new(registry::UI_PERMISSION)
                                    .xsmall()
                                    .text_color(amber.opacity(0.7)),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_xs()
                                    .text_color(amber.opacity(0.9))
                                    .child(SharedString::from(format!("Permission · {tool}"))),
                            )
                            .when_some(detail.clone(), |this, detail| {
                                this.child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        // EXP-698: the web's 11px caption
                                        // rung — a secondary detail must read
                                        // BELOW the label it hangs off.
                                        .text_2xs()
                                        .text_color(muted)
                                        .font_family(theme::terminal::FONT_FAMILY)
                                        .child(SharedString::from(detail)),
                                )
                            }),
                    )
                    .when(active.is_empty() && self.phase == ViewerPhase::Live, |this| {
                        this.child(
                            div()
                                .pl_5()
                                .text_2xs()
                                .text_color(muted)
                                .child("Approve on the desktop, or reply below to continue."),
                        )
                    })
                    .into_any_element()
            }
            FeedKind::Subagent { .. } => self.render_subagent(item.id, &[item], cx),
            FeedKind::Question(_) => self.render_question(item, active, cx),
            // EXP-724: the quiet divider a finished compaction leaves behind
            // — everything above it is context the agent no longer holds.
            FeedKind::Compaction => h_flex()
                .w_full()
                .gap_1p5()
                .items_center()
                .justify_center()
                .py_1p5()
                .child(
                    Icon::new(registry::CODING_COMPACT)
                        .xsmall()
                        .text_color(muted.opacity(0.6)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(COMPACTED_LABEL),
                )
                .into_any_element(),
        }
    }

    /// EXP-698 — a sent message, with its `[Image #N]` markers rendered as
    /// PILLS instead of literal bracket text. The prose is split on the
    /// markers and flowed with them, the embed block still renders its images
    /// underneath, and clicking a pill opens THAT image (the N-th in embed
    /// order) in the lightbox.
    ///
    /// The web rule, exactly: a marker only means anything when the message
    /// actually CARRIES embeds, and only when its number names one of them.
    /// A message with no embeds, or whose every marker is out of range, takes
    /// the plain [`Self::render_body`] path — which is every message the
    /// older clients send and every one that carries no images at all — and
    /// an out-of-range marker inside a marker message stays literal prose.
    ///
    /// The runs are rendered as PLAIN TEXT, one flex-wrap row per source
    /// line, not as markdown: a marker can land inside a fenced block or a
    /// list item, and splitting markdown at that seam would render two broken
    /// halves. The web renders these messages as pre-wrap text for the same
    /// reason. (It also autolinks bare URLs; this port does not, because
    /// nothing in `crate::markdown` autolinks — `markdown/parse.rs` documents
    /// that bare URLs stay bare for tiptap parity — and inventing an autolink
    /// here would make the marker path diverge from every other body.)
    fn render_user_message(
        &self,
        id: FeedItemId,
        text: &str,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let parsed = parse_steer_message(text);
        let count = parsed.attachment_ids.len();
        let chips = count > 0
            && parsed
                .markers
                .iter()
                .any(|number| image_marker_in_range(*number, count));
        if !chips {
            return self.render_body(id, text, cx);
        }
        // One row per SOURCE LINE, so a message's own line breaks survive;
        // the row wraps, so a pill sits with the words around it until the
        // line runs out.
        let mut column = v_flex()
            .w_full()
            .min_w_0()
            .line_height(gpui::relative(1.625));
        for (line_number, line) in parsed.text.split('\n').enumerate() {
            let runs = split_image_markers(line, count);
            if runs.is_empty() {
                // A blank line is a paragraph break — keep it as the chat
                // rhythm's 8px gap rather than collapsing it away.
                column = column.child(div().h_2());
                continue;
            }
            let mut row = h_flex().w_full().min_w_0().flex_wrap().items_center().gap_1();
            for (index, run) in runs.into_iter().enumerate() {
                row = match run {
                    MarkerSegment::Text(body) => {
                        row.child(div().min_w_0().child(SharedString::from(body)))
                    }
                    MarkerSegment::Marker(number) => {
                        // In range by construction — `split_image_markers`
                        // folded every other number back into the prose.
                        let attachment = parsed.attachment_ids[number as usize - 1].clone();
                        let label = SharedString::from(format!("Image {number}"));
                        row.child(
                            crate::surface::glass_pill(
                                // A per-(row, run) id: an arithmetic id
                                // (`id * 16 + index`) collides as soon as one
                                // message has 16 runs.
                                SharedString::from(format!(
                                    "steer-msg-image-{id}-{line_number}-{index}"
                                )),
                                crate::surface::PillSize::Sm,
                                crate::surface::PillMode::Action,
                                cx,
                            )
                            .child(
                                Icon::new(registry::EDITOR_IMAGE)
                                    .with_size(px(crate::surface::PillSize::Sm.glyph())),
                            )
                            .child(label.clone())
                            .on_click(move |_: &ClickEvent, window, cx| {
                                crate::image_preview::open_image_preview(
                                    format!("/api/attachments/{attachment}"),
                                    label.to_string(),
                                    None,
                                    None,
                                    window,
                                    cx,
                                );
                            }),
                        )
                    }
                };
            }
            column = column.child(row);
        }
        // The embeds still render underneath — the pills REFERENCE the
        // images, they do not replace them.
        let embeds = build_steer_image_message("", &parsed.attachment_ids);
        v_flex()
            .w_full()
            .min_w_0()
            .gap_1()
            .child(column)
            .when(!embeds.is_empty(), |this| {
                this.child(
                    crate::markdown::MarkdownView::new(
                        SharedString::from(format!("steer-msg-images-{id}")),
                        embeds,
                    )
                    .selectable(true),
                )
            })
            .into_any_element()
    }

    /// A body that folds behind "Show more" once it runs long.
    fn render_body(&self, id: FeedItemId, text: &str, cx: &mut gpui::Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        // EXP-698: every body this renders is a CHAT body — the user bubble,
        // the plan card, the ask card, a stepper step — so the rhythm and the
        // code tint are set once, here.
        let view = crate::markdown::MarkdownView::new(
            SharedString::from(format!("steer-body-{id}")),
            text.to_string(),
        )
        .chat(true)
        .selectable(true);
        if !clampable(text) {
            return div().w_full().min_w_0().child(view).into_any_element();
        }
        let expanded = self.expanded_bodies.contains(&id);
        v_flex()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .when(!expanded, |this| this.max_h(px(160.)).overflow_hidden())
                    .child(view),
            )
            .child(
                div()
                    .id(("steer-body-toggle", id as usize))
                    .mt_1()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(muted)
                    .child(if expanded { "Show less" } else { "Show more" })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        if !this.expanded_bodies.insert(id) {
                            this.expanded_bodies.remove(&id);
                        }
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn render_tool_run(
        &self,
        id: FeedItemId,
        items: &[&FeedItem],
        live_tail: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let expanded = self.expanded_groups.contains(&id);
        let mut column = v_flex().w_full().min_w_0().child(
            h_flex()
                .id(("steer-tool-run", id as usize))
                .w_full()
                .min_w_0()
                .gap_2()
                .items_center()
                .py_0p5()
                .cursor_pointer()
                .text_color(muted)
                .child(
                    Icon::new(if expanded {
                        registry::UI_CHEVRON_DOWN
                    } else {
                        registry::UI_CHEVRON_RIGHT
                    })
                    .xsmall(),
                )
                .child(Icon::new(registry::CODING_TOOL).xsmall())
                .child(
                    div()
                        .text_xs()
                        .child(SharedString::from(format!("{} tool calls", items.len()))),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    if !this.expanded_groups.insert(id) {
                        this.expanded_groups.remove(&id);
                    }
                    cx.notify();
                })),
        );
        if expanded {
            for item in items {
                if let FeedKind::Tool { name, detail, .. } = &item.kind {
                    column = column.child(div().pl_5().child(tool_row(name, detail.as_deref(), cx)));
                }
            }
        } else if live_tail {
            // Collapsed but still running — keep the newest call visible.
            if let Some(FeedKind::Tool { name, detail, .. }) = items.last().map(|item| &item.kind) {
                column = column.child(div().pl_5().child(tool_row(name, detail.as_deref(), cx)));
            }
        }
        column.into_any_element()
    }

    fn render_subagent(
        &self,
        id: FeedItemId,
        items: &[&FeedItem],
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let summary = summarize_subagent_row(items);
        let tools: Vec<&&FeedItem> = items.iter().filter(|item| item.is_tool()).collect();
        let expandable = !tools.is_empty();
        let expanded = expandable && self.expanded_groups.contains(&id);
        let running = matches!(
            items.iter().find_map(|item| match &item.kind {
                FeedKind::Subagent { status, .. } => Some(*status),
                _ => None,
            }),
            Some(SubagentStatus::Started)
        ) && !summary.done;

        let header = h_flex()
            .id(("steer-subagent", id as usize))
            .w_full()
            .min_w_0()
            .gap_2()
            .items_center()
            .py_0p5()
            .text_color(muted)
            .when(expandable, |this| {
                this.child(
                    Icon::new(if expanded {
                        registry::UI_CHEVRON_DOWN
                    } else {
                        registry::UI_CHEVRON_RIGHT
                    })
                    .xsmall(),
                )
            })
            .child(Icon::new(registry::CODING_SUBAGENT).xsmall())
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().foreground)
                    .child(SharedString::from(summary.agent_type.clone())),
            )
            .when(running, |this| this.child(Spinner::new().xsmall()))
            .child(
                // EXP-698: 11px — the agent TYPE is the row's 12px line, its
                // status and detail the captions beside it.
                div()
                    .flex_shrink_0()
                    .text_2xs()
                    .child(SharedString::from(subagent_caption(
                        summary.done,
                        summary.tool_count,
                    ))),
            )
            .when_some(summary.detail.clone(), |this, detail| {
                this.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_2xs()
                        .child(SharedString::from(detail)),
                )
            });
        let header = header.when(expandable, |header| {
            header
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    if !this.expanded_groups.insert(id) {
                        this.expanded_groups.remove(&id);
                    }
                    cx.notify();
                }))
        });
        let mut column = v_flex().w_full().min_w_0().child(header);
        if expanded {
            for item in tools {
                if let FeedKind::Tool { name, detail, .. } = &item.kind {
                    column = column.child(div().pl_5().child(tool_row(name, detail.as_deref(), cx)));
                }
            }
        }
        column.into_any_element()
    }

    /// One `askId` group as a stepper card: the answered steps, then the
    /// current one (or the waiting line).
    fn render_ask(
        &self,
        _id: FeedItemId,
        items: &[&FeedItem],
        active: &HashSet<FeedItemId>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let amber = theme::tokens::YELLOW.to_hsla();
        let answered: Vec<&&FeedItem> = items
            .iter()
            .filter(|item| {
                item.question().is_some_and(|card| card.resolved)
                    || self.feed.is_answer_locked(&answer_key(item))
            })
            .collect();
        let current = items.iter().find(|item| {
            !item.question().is_some_and(|card| card.resolved)
                && !self.feed.is_answer_locked(&answer_key(item))
        });
        let numbered: Vec<&&FeedItem> = items
            .iter()
            .filter(|item| item.question().and_then(|card| card.index).is_some())
            .collect();
        let total = numbered
            .first()
            .and_then(|item| item.question().and_then(|card| card.total))
            .unwrap_or(numbered.len() as u32);
        let submit_step = current
            .map(|item| item.question().and_then(|card| card.index).is_none())
            .unwrap_or(false);
        let header = current
            .and_then(|item| item.question().and_then(|card| card.header.clone()))
            .or_else(|| items.first().and_then(|item| item.question().and_then(|card| card.header.clone())))
            .unwrap_or_else(|| {
                if submit_step {
                    "Review answers".to_string()
                } else {
                    "Question".to_string()
                }
            });
        let counter = ask_counter(
            current
                .filter(|_| !submit_step)
                .and_then(|item| item.question().and_then(|card| card.index)),
            total,
        );

        // EXP-698: the stepper wears the SAME neutral glass card chrome as
        // `render_question` — a tinted border plus a tinted fill made the two
        // question surfaces read as two different materials in one feed. Only
        // the glyph and the heading carry the amber accent.
        let mut card = crate::surface::glass_card()
            .w_full()
            .min_w_0()
            .my_1()
            .gap_1()
            .p_3()
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_1p5()
                    .items_center()
                    .child(Icon::new(registry::UI_HELP).xsmall().text_color(amber))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(amber)
                            .child(SharedString::from(header)),
                    )
                    .when_some(counter, |this, counter| {
                        this.child(
                            // EXP-698: 11px — "2 of 3" is a caption beside the
                            // heading, never a peer of it.
                            div()
                                .flex_shrink_0()
                                .text_2xs()
                                .text_color(muted)
                                .child(SharedString::from(counter)),
                        )
                    }),
            );
        for item in answered {
            card = card.child(self.render_answered_step(item, cx));
        }
        match current {
            Some(item) => {
                let text = item.question().map(|card| card.text.clone()).unwrap_or_default();
                card = card
                    .child(div().w_full().min_w_0().text_sm().child(self.render_body(
                        item.id,
                        &text,
                        cx,
                    )))
                    .child(self.render_prompt(item, active, submit_step, cx));
            }
            None => {
                card = card.child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .child(Spinner::new().xsmall())
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child("Waiting for the next question…"),
                        ),
                );
            }
        }
        card.into_any_element()
    }

    fn render_answered_step(&self, item: &FeedItem, cx: &mut gpui::Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let card = item.question();
        let dismissed = card.is_some_and(|card| card.dismissed);
        let answer = card
            .and_then(|card| card.answer.clone())
            .or_else(|| {
                self.feed
                    .answer_state(&answer_key(item))
                    .map(|state| state.labels.join(", "))
                    .filter(|labels| !labels.is_empty())
            })
            .unwrap_or_else(|| "Answered".to_string());
        h_flex()
            .w_full()
            .min_w_0()
            .gap_1p5()
            .items_center()
            .py_1()
            .text_xs()
            .child(
                Icon::new(if dismissed {
                    registry::UI_CLOSE
                } else {
                    registry::UI_CHECK
                })
                .xsmall()
                .text_color(if dismissed {
                    muted
                } else {
                    theme::tokens::GREEN.to_hsla()
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(muted)
                    .child(SharedString::from(
                        card.map(|card| card.text.clone()).unwrap_or_default(),
                    )),
            )
            .child(
                div()
                    .max_w(px(200.))
                    .truncate()
                    .child(SharedString::from(if dismissed {
                        "Dismissed".to_string()
                    } else {
                        answer
                    })),
            )
            .into_any_element()
    }

    /// A standalone question / plan-approval card.
    fn render_question(
        &self,
        item: &FeedItem,
        active: &HashSet<FeedItemId>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let Some(card) = item.question() else {
            return div().into_any_element();
        };
        let accent = if card.plan_mode {
            cx.theme().primary
        } else {
            theme::tokens::YELLOW.to_hsla()
        };
        let heading = if card.plan_mode {
            Some(SharedString::from("Plan ready"))
        } else {
            card.header.clone().map(SharedString::from)
        };
        // EXP-698: NEUTRAL card chrome — the shared glass card, radius XL.
        // Only the glyph and the heading carry the accent (primary for a
        // ready plan, yellow for a question); a tinted border + tinted fill
        // made these read as two more materials in the feed.
        crate::surface::glass_card()
            .w_full()
            .min_w_0()
            .my_1()
            .gap_1()
            .p_3()
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(
                        Icon::new(if card.plan_mode {
                            registry::CODING_PLAN
                        } else {
                            registry::UI_HELP
                        })
                        .xsmall()
                        .text_color(accent),
                    )
                    .when_some(heading, |this, heading| {
                        this.child(div().text_xs().text_color(accent).child(heading))
                    }),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .text_sm()
                    .child(self.render_body(item.id, &card.text, cx)),
            )
            .child(self.render_prompt(item, active, false, cx))
            .into_any_element()
    }

    /// The interactive half of a card: options, multi-select submit, the
    /// free-text row, the lock and the resolution line.
    fn render_prompt(
        &self,
        item: &FeedItem,
        active: &HashSet<FeedItemId>,
        submit_step: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let Some(card) = item.question() else {
            return div().into_any_element();
        };
        let key = answer_key(item);
        let state = self.feed.answer_state(&key);
        let locked = state.is_some_and(|state| state.is_locked());
        let errored = state.is_some_and(|state| state.status == AnswerStatus::Error);
        let semantic = card.question_id.is_some();

        if card.resolved {
            let answer = card
                .answer
                .clone()
                .unwrap_or_else(|| "Answered".to_string());
            return h_flex()
                .gap_1p5()
                .items_center()
                .mt_1()
                .child(
                    Icon::new(if card.dismissed {
                        registry::UI_CLOSE
                    } else {
                        registry::UI_CHECK
                    })
                    .xsmall()
                    .text_color(if card.dismissed {
                        muted
                    } else {
                        theme::tokens::GREEN.to_hsla()
                    }),
                )
                .child(div().text_xs().child(SharedString::from(if card.dismissed {
                    "Dismissed".to_string()
                } else {
                    answer
                })))
                .into_any_element();
        }

        if locked {
            let labels = state.map(|state| state.labels.join(", ")).unwrap_or_default();
            return h_flex()
                .gap_1p5()
                .items_center()
                .mt_2()
                .text_xs()
                .text_color(muted)
                .child(Spinner::new().xsmall())
                .child("Answering…")
                .when(!labels.is_empty(), |this| {
                    this.child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(cx.theme().foreground)
                            .child(SharedString::from(labels)),
                    )
                })
                .into_any_element();
        }

        let answerable = active.contains(&item.id)
            && self.phase == ViewerPhase::Live
            && self.connected
            && !self.row_ended();
        if !answerable {
            let note = if card.plan_mode {
                "Waiting for approval. You're viewing read-only."
            } else {
                "Waiting for an answer. You're viewing read-only."
            };
            return v_flex()
                .mt_2()
                .gap_0p5()
                .children(card.options.iter().map(|option| {
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(format!(
                            "{} · {}",
                            option.key, option.label
                        )))
                }))
                .child(div().text_xs().text_color(muted).child(note))
                .into_any_element();
        }

        let picked = self.picked.get(&key).cloned().unwrap_or_default();
        let promote_first = card.plan_mode || submit_step;
        let item_id = item.id;
        let mut options = v_flex().mt_2().w_full().min_w_0().gap_1();
        if promote_first {
            options = options.flex_row().flex_wrap().items_center();
        }
        for (index, option) in card.options.iter().enumerate() {
            options = options.child(self.render_option(
                item_id,
                &key,
                card.multi_select,
                semantic,
                promote_first && index == 0,
                submit_step && index == 0,
                option,
                picked.contains(&option.key),
                index,
                cx,
            ));
        }

        let mut column = v_flex().w_full().min_w_0().child(options);
        if card.multi_select {
            let labels: Vec<String> = card
                .options
                .iter()
                .filter(|option| picked.contains(&option.key))
                .map(|option| option.label.clone())
                .collect();
            let keys = picked.clone();
            column = column.child(
                Button::new(("steer-answer-submit", item_id as usize))
                    .with_variant(ButtonVariant::Secondary)
                    .cursor_pointer()
                    .xsmall()
                    .label(if semantic { "Answer" } else { "Continue" })
                    .disabled(semantic && keys.is_empty())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        let sent_keys = if semantic {
                            keys.clone()
                        } else {
                            vec!["\t".to_string()]
                        };
                        this.answer(item_id, sent_keys, labels.clone(), None, cx);
                    })),
            );
        }
        if self.free_text.as_ref().is_some_and(|(answer, _)| *answer == key) {
            column = column.child(
                h_flex()
                    .mt_2()
                    .w_full()
                    .gap_1p5()
                    .items_center()
                    .child(div().flex_1().min_w_0().child(Input::new(&self.free_text_input)))
                    .child(
                        Button::new(("steer-free-text", item_id as usize))
                            .with_variant(ButtonVariant::Secondary)
                            .cursor_pointer()
                            .xsmall()
                            .label("Answer")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                this.submit_free_text(window, cx);
                            })),
                    ),
            );
        }
        if errored && semantic {
            column = column.child(
                div()
                    .mt_1p5()
                    .text_xs()
                    .text_color(theme::tokens::YELLOW.to_hsla())
                    .child("No confirmation from the desktop. Pick again to retry."),
            );
        }
        column.into_any_element()
    }

    #[allow(clippy::too_many_arguments)] // one call site; every flag is a render decision
    fn render_option(
        &self,
        item_id: FeedItemId,
        key: &str,
        multi_select: bool,
        semantic: bool,
        primary: bool,
        submit_label: bool,
        option: &QuestionOption,
        picked: bool,
        index: usize,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let label = if submit_label {
            "Submit answers".to_string()
        } else {
            option.label.clone()
        };
        let mut button = Button::new(("steer-option", item_id as usize * 64 + index))
            .cursor_pointer()
            .xsmall()
            .label(SharedString::from(label.clone()));
        button = if primary {
            button.primary()
        } else {
            button.outline()
        };
        if picked {
            button = button.selected(true);
        }
        let answer_key = key.to_string();
        let option_key = option.key.clone();
        let option_label = option.label.clone();
        let free_text = option.free_text;
        button
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                if multi_select {
                    if !semantic {
                        // Legacy multi-select toggles the TUI immediately.
                        if let Some(handle) = this.handle.as_ref() {
                            handle.send_keystrokes(&[option_key.clone()]);
                        }
                    }
                    let picks = this.picked.entry(answer_key.clone()).or_default();
                    match picks.iter().position(|pick| *pick == option_key) {
                        Some(at) => {
                            picks.remove(at);
                        }
                        None => picks.push(option_key.clone()),
                    }
                    cx.notify();
                    return;
                }
                if free_text && semantic {
                    let open = this
                        .free_text
                        .as_ref()
                        .is_some_and(|(answer, opt)| *answer == answer_key && *opt == option_key);
                    this.free_text = if open {
                        None
                    } else {
                        this.free_text_input
                            .update(cx, |state, cx| state.set_value("", window, cx));
                        Some((answer_key.clone(), option_key.clone()))
                    };
                    cx.notify();
                    return;
                }
                this.answer(
                    item_id,
                    vec![option_key.clone()],
                    vec![option_label.clone()],
                    None,
                    cx,
                );
            }))
            .into_any_element()
    }

    fn render_banners(&self, cx: &mut gpui::Context<Self>) -> Vec<AnyElement> {
        let muted = cx.theme().muted_foreground;
        let mut banners = Vec::new();
        let paused = self.paused(cx);
        let banner = |text: String| -> AnyElement {
            div()
                .w_full()
                .flex_shrink_0()
                .px_3()
                .py_2()
                .border_t_1()
                .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(text))
                .into_any_element()
        };
        match &self.phase {
            ViewerPhase::Ended { outcome } => banners.push(banner(
                outcome
                    .clone()
                    .unwrap_or_else(|| "The session has ended.".to_string()),
            )),
            ViewerPhase::Unauthorized { detail } => banners.push(banner(
                detail
                    .clone()
                    .unwrap_or_else(|| "Live steering is unavailable on this instance.".to_string()),
            )),
            ViewerPhase::Reconnecting => banners.push(banner("Connection lost.".to_string())),
            ViewerPhase::Starting if !self.feed.is_empty() => banners.push(banner(
                "The agent is starting. Waiting for the live stream…".to_string(),
            )),
            _ => {}
        }
        if paused && !self.feed.is_empty() {
            let device = self.device(cx);
            banners.push(banner(format!(
                "Paused — {}. {PAUSED_BODY}",
                paused_title(device.label.as_deref())
            )));
        }
        if let Some(notice) = self.notice.clone() {
            banners.push(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
                    .text_xs()
                    .text_color(theme::tokens::YELLOW.to_hsla())
                    .child(notice)
                    .into_any_element(),
            );
        }
        banners
    }

    fn render_composer(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let can_send = self.can_send(cx);
        let composer = crate::composer::GlassComposer::new(
            v_flex()
                .w_full()
                .min_w_0()
                // EXP-724: the `/` menu sits INSIDE the composer card,
                // above the textarea — no popover, no caret anchoring
                // (the token is always the whole draft).
                .when_some(self.render_slash_menu(cx), |this, menu| this.child(menu))
                .child(
                    div()
                        // The five captures run before the textarea's own
                        // handlers, so with a menu open Enter/Tab accept
                        // and `PressEnter` — the thing that SENDS — never
                        // fires (the `mention_input` recipe).
                        .key_context("SteerComposer")
                        .w_full()
                        .min_w_0()
                        .capture_action(cx.listener(Self::on_slash_up))
                        .capture_action(cx.listener(Self::on_slash_down))
                        .capture_action(cx.listener(Self::on_slash_escape))
                        .capture_action(cx.listener(Self::on_slash_enter))
                        .capture_action(cx.listener(Self::on_slash_tab))
                        .child(Textarea::new(&self.input).w_full().appearance(false)),
                )
                .into_any_element(),
        )
        .strip((!self.pending.is_empty()).then(|| self.render_pending_strip(cx)))
        // EXP-698: the attach tool is ALWAYS offered — steer images upload to
        // the session route, so a batch/action run (no issue at all) attaches
        // exactly like an issue run. Its glyph is `ui-add`, the `+` web, iOS
        // and Android all wear on this control.
        .tool(
            crate::composer::composer_tool("steer-attach", registry::UI_ADD, cx)
                .tooltip("Attach image")
                .disabled(self.sending)
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.pick_images(window, cx);
                })),
        );
        // The steer composer's send is `ui-send` on web/iOS/Android
        // (`ui-submit` is the COMMENT composer's) — same surface, same
        // concept. EXP-698: no tooltip — a floating "Send" label beside a
        // circled arrow reads as a second button.
        let composer = composer.submit(
            crate::composer::composer_submit("steer-send", registry::UI_SEND, !can_send, cx)
                .loading(self.sending)
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.send(window, cx);
                })),
        );
        div()
            .w_full()
            .flex_shrink_0()
            .p_2()
            .border_t_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            .child(
                crate::composer::glass_composer(composer)
                    .capture_action(cx.listener(Self::on_paste)),
            )
            .into_any_element()
    }

    /// EXP-724: the `/` command rows — mono name, muted argument hint, muted
    /// description. `None` when no menu is open.
    fn render_slash_menu(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let menu = self.slash.as_ref()?;
        let muted = cx.theme().muted_foreground;
        let accent = cx.theme().accent;
        let mut column = v_flex().w_full().min_w_0().gap_0p5();
        for (index, command) in menu.items.iter().enumerate() {
            let selected = index == menu.selected;
            column = column.child(
                h_flex()
                    .id(("steer-slash-row", index))
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .items_center()
                    .px_2()
                    .py_1()
                    .rounded(px(theme::tokens::radius::SM))
                    .when(selected, |this| this.bg(accent))
                    .hover(|this| this.bg(accent))
                    .cursor_pointer()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            if let Some(menu) = this.slash.as_mut() {
                                menu.selected = index;
                            }
                            this.accept_slash(window, cx);
                        }),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .font_family(theme::terminal::FONT_FAMILY)
                            .child(SharedString::from(format!("/{}", command.name))),
                    )
                    .when(!command.arg_hint.is_empty(), |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(muted)
                                .font_family(theme::terminal::FONT_FAMILY)
                                .child(SharedString::from(command.arg_hint.to_string())),
                        )
                    })
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from(command.description.to_string())),
                    ),
            );
        }
        Some(column.into_any_element())
    }

    /// EXP-724: the pinned strip while the agent folds its context. The bar
    /// is INDETERMINATE on purpose — no agent reports compaction progress,
    /// and the only honest signal is "still going".
    fn render_compaction_strip(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let primary = cx.theme().primary;
        h_flex()
            .w_full()
            .flex_shrink_0()
            .gap_2()
            .items_center()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            .child(
                Icon::new(registry::CODING_COMPACT)
                    .xsmall()
                    .text_color(muted.opacity(0.7)),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child(COMPACTING_LABEL),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(px(3.))
                    .rounded_full()
                    .bg(muted.opacity(0.15))
                    .overflow_hidden()
                    .child(
                        div()
                            .h_full()
                            .w(relative(0.3))
                            .rounded_full()
                            .bg(primary.opacity(0.7))
                            .with_animation(
                                "steer-compacting",
                                gpui::Animation::new(Duration::from_millis(1400))
                                    .repeat()
                                    .with_easing(bounce(ease_in_out)),
                                |bar, delta| bar.ml(relative(delta * 0.7)),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// EXP-698: pending images render as 48px THUMBNAILS with a corner ✕ —
    /// the web/iOS/Android strip. The bytes are already in hand (they are
    /// what the send uploads), so the old filename chip was showing the
    /// least useful thing about a picture; a chip survives only as the
    /// fallback for bytes nothing can decode.
    fn render_pending_strip(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut strip = h_flex().w_full().flex_wrap().gap_1p5();
        for image in &self.pending {
            let key = image.key;
            // EXP-698: a 24px hit target (`size::CONTROL_SM`) overlaid on the
            // 48px tile — `xsmall()` alone sized the glyph, not the box, and
            // left a corner ✕ that was hard to actually hit.
            let remove = Button::new(("steer-pending-remove", key as usize))
                .ghost()
                .cursor_pointer()
                .with_size(px(theme::tokens::size::CONTROL_SM))
                .rounded_full()
                .icon(registry::UI_CLOSE)
                .tooltip("Remove image")
                .disabled(self.sending)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.remove_pending(key, window, cx);
                }));
            let preview = image.preview.clone();
            let filename = SharedString::from(image.filename.clone());
            strip = strip.child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .size(px(PENDING_THUMB))
                    .rounded(px(theme::tokens::radius::SM))
                    .border_1()
                    .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                    .bg(theme::tokens::glass::FILL_CARD.to_hsla())
                    .overflow_hidden()
                    .child(
                        gpui::img(preview)
                            .size_full()
                            .object_fit(gpui::ObjectFit::Cover)
                            // Bytes gpui cannot decode fall back to the
                            // filename, so a tile is never a silent blank
                            // square — that is the old chip's whole job.
                            .with_fallback(move || {
                                div()
                                    .size_full()
                                    .p_1()
                                    .text_xs()
                                    .truncate()
                                    .text_color(muted)
                                    .child(filename.clone())
                                    .into_any_element()
                            }),
                    )
                    // The ✕ rides the tile's top-right corner (web/iOS).
                    .child(div().absolute().top_0().right_0().child(remove)),
            );
        }
        strip.into_any_element()
    }
}

const PAUSED_BODY: &str =
    "The agent is paused on that machine and continues when it comes back online.";

fn paused_title(device: Option<&str>) -> String {
    match device {
        Some(label) if !label.is_empty() => format!("{label} is offline"),
        _ => "The device is offline".to_string(),
    }
}

fn centered(children: Vec<AnyElement>) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_2()
        .p_6()
        .text_center()
        .children(children)
        .into_any_element()
}

/// One run of a marker-carrying message: prose, or one `[Image #N]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MarkerSegment {
    Text(String),
    Marker(u32),
}

/// The largest char boundary at or before `at` (and never past the end) —
/// gpui carries the caret as a BYTE offset, and a renumbered draft is shorter
/// than the one the offset was taken from.
fn clamp_to_char_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Whether `number` names one of a message's `count` embeds — the web rule:
/// markers are 1-based and a number outside the range references nothing, so
/// it is not a chip, it is the text the sender typed.
pub(crate) fn image_marker_in_range(number: u32, count: usize) -> bool {
    number >= 1 && (number as usize) <= count
}

/// Split ONE LINE of a message's prose on its `[Image #N]` markers, keeping
/// order. `count` is how many embeds the message carries: a marker outside
/// `1..=count` is not a marker at all and folds back into the surrounding
/// prose run verbatim.
///
/// Seam trimming takes SPACES AND TABS only, never newlines — the flow that
/// renders these puts a 4px gap between the runs, so a preserved seam space
/// would double it, but a swallowed line break would silently reflow the
/// message. (Callers split on `\n` first, so a newline should not reach here
/// at all; the rule is explicit so it stays true if one ever does.)
///
/// The token is the shared contract's ([`steer::image_marker`]); anything
/// that is not exactly `[Image #<digits>]` stays prose.
pub(crate) fn split_image_markers(text: &str, count: usize) -> Vec<MarkerSegment> {
    const OPEN: &str = "[Image #";
    let mut runs: Vec<MarkerSegment> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        let after = &rest[start + OPEN.len()..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        let closed = !digits.is_empty() && after[digits.len()..].starts_with(']');
        let number = closed
            .then(|| digits.parse::<u32>().ok())
            .flatten()
            .filter(|number| image_marker_in_range(*number, count));
        let Some(number) = number else {
            // `[Image #]`, `[Image #x]`, an unterminated token, or a number
            // with no embed behind it: prose. Keep the opener with the run
            // and carry on past it — the digits and `]` fuse back on through
            // the next `push_text`.
            let (head, tail) = rest.split_at(start + OPEN.len());
            push_text(&mut runs, head);
            rest = tail;
            continue;
        };
        push_text(&mut runs, &rest[..start]);
        runs.push(MarkerSegment::Marker(number));
        rest = &after[digits.len() + 1..];
    }
    push_text(&mut runs, rest);
    runs.into_iter()
        .filter_map(|run| match run {
            MarkerSegment::Text(body) => {
                let trimmed = body.trim_matches([' ', '\t']);
                (!trimmed.is_empty()).then(|| MarkerSegment::Text(trimmed.to_string()))
            }
            marker => Some(marker),
        })
        .collect()
}

/// Append a prose run, FUSING it onto the previous one — a token that turned
/// out not to be a marker leaves two halves that are really one run.
fn push_text(runs: &mut Vec<MarkerSegment>, body: &str) {
    if body.is_empty() {
        return;
    }
    if let Some(MarkerSegment::Text(previous)) = runs.last_mut() {
        previous.push_str(body);
        return;
    }
    runs.push(MarkerSegment::Text(body.to_string()));
}

/// EXP-698: wrap staged bytes for `img()`, using the same magic-byte sniff
/// the editor's image slots use. Bytes gpui cannot decode simply paint the
/// element's `with_fallback` (the filename), so this never has to guess right.
fn pending_preview(content_type: &str, bytes: Vec<u8>) -> Arc<gpui::Image> {
    let format = crate::markdown::sniff_format(content_type, &bytes);
    Arc::new(gpui::Image::from_bytes(format, bytes))
}

fn status_dot(color: gpui::Hsla) -> impl IntoElement {
    div()
        .flex_shrink_0()
        .size_1p5()
        .rounded_full()
        .bg(color)
}

fn tool_row(name: &str, detail: Option<&str>, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .py_0p5()
        .child(
            Icon::new(registry::CODING_TOOL)
                .xsmall()
                .text_color(muted.opacity(0.6)),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .child(SharedString::from(name.to_string())),
        )
        .when_some(detail, |this, detail| {
            this.child(
                div()
                    .min_w_0()
                    .truncate()
                    // EXP-698: 11px, the web's caption rung — the tool NAME
                    // is the 12px line, its argument the quieter one under it.
                    .text_2xs()
                    .text_color(muted)
                    .font_family(theme::terminal::FONT_FAMILY)
                    .child(SharedString::from(detail.to_string())),
            )
        })
}

impl Focusable for SteerSessionView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SteerSessionView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let header = self.render_header(cx);
        let feed = self.render_feed(cx);
        let banners = self.render_banners(cx);
        let composer_visible = self.composer_visible();
        // EXP-724: between the banners and the composer, exactly where the
        // web view puts it — and gone with the composer once the run ends.
        let compacting = (composer_visible && self.feed.compacting().is_some())
            .then(|| self.render_compaction_strip(cx));
        let composer = composer_visible.then(|| self.render_composer(cx));
        v_flex()
            .key_context("SteerSession")
            .track_focus(&self.focus_handle)
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .child(header)
            .child(feed)
            .children(banners)
            .children(compacting)
            .children(composer)
    }
}

impl Drop for SteerSessionView {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.as_ref() {
            handle.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── EXP-698: `[Image #N]` pills in a sent message ──────────────────────

    fn text(body: &str) -> MarkerSegment {
        MarkerSegment::Text(body.to_string())
    }

    #[test]
    fn a_message_splits_into_prose_and_marker_runs() {
        assert_eq!(
            split_image_markers("crop [Image #2] please", 2),
            vec![text("crop"), MarkerSegment::Marker(2), text("please")]
        );
    }

    #[test]
    fn a_marker_may_stand_alone_or_bookend_the_prose() {
        assert_eq!(
            split_image_markers("[Image #1]", 1),
            vec![MarkerSegment::Marker(1)]
        );
        assert_eq!(
            split_image_markers("[Image #1] fix this", 1),
            vec![MarkerSegment::Marker(1), text("fix this")]
        );
        assert_eq!(
            split_image_markers("fix this [Image #1]", 1),
            vec![text("fix this"), MarkerSegment::Marker(1)]
        );
    }

    #[test]
    fn adjacent_markers_keep_their_order_and_numbers() {
        assert_eq!(
            split_image_markers("[Image #2][Image #1]", 2),
            vec![MarkerSegment::Marker(2), MarkerSegment::Marker(1)]
        );
    }

    #[test]
    fn a_message_with_no_markers_is_one_run() {
        assert_eq!(
            split_image_markers("just words", 1),
            vec![text("just words")]
        );
        assert!(split_image_markers("   ", 1).is_empty());
    }

    #[test]
    fn a_near_miss_token_stays_prose() {
        // Only the exact contract token is a marker — anything else must
        // survive as the words the sender typed.
        for near_miss in ["[Image #]", "[Image #x]", "[Image #1", "[image #1]"] {
            assert_eq!(
                split_image_markers(near_miss, 4),
                vec![text(near_miss)],
                "{near_miss} is not a marker"
            );
        }
    }

    /// The web rule: a marker references one of the message's embeds or it is
    /// not a marker. Out of range it folds back into the prose VERBATIM —
    /// the sender's words are never silently eaten.
    #[test]
    fn an_out_of_range_marker_folds_back_into_the_prose() {
        assert_eq!(
            split_image_markers("crop [Image #3] please", 2),
            vec![text("crop [Image #3] please")]
        );
        // Zero is out of range too — the numbering is 1-based.
        assert_eq!(
            split_image_markers("[Image #0]", 2),
            vec![text("[Image #0]")]
        );
        // With NO embeds nothing is ever a chip.
        assert_eq!(
            split_image_markers("crop [Image #1]", 0),
            vec![text("crop [Image #1]")]
        );
    }

    #[test]
    fn an_out_of_range_marker_beside_a_valid_one_keeps_both_intact() {
        assert_eq!(
            split_image_markers("[Image #1] and [Image #9] too", 1),
            vec![
                MarkerSegment::Marker(1),
                text("and [Image #9] too"),
            ]
        );
    }

    /// Seams lose spaces and tabs (the flow's gap replaces them) but NEVER a
    /// line break — callers split per line, and a swallowed `\n` would
    /// silently reflow the sender's message.
    #[test]
    fn seam_trimming_takes_spaces_but_not_newlines() {
        assert_eq!(
            split_image_markers("crop \t[Image #1]\t please", 1),
            vec![text("crop"), MarkerSegment::Marker(1), text("please")]
        );
        assert_eq!(
            split_image_markers("a\n[Image #1]\nb", 1),
            vec![text("a\n"), MarkerSegment::Marker(1), text("\nb")]
        );
    }

    #[test]
    fn the_split_agrees_with_the_shared_parser() {
        // The pill numbers the view renders and the markers the contract
        // reports are the same list, in the same order.
        let message = "crop [Image #2] then [Image #1]";
        let from_split: Vec<u32> = split_image_markers(message, 2)
            .into_iter()
            .filter_map(|run| match run {
                MarkerSegment::Marker(number) => Some(number),
                MarkerSegment::Text(_) => None,
            })
            .collect();
        assert_eq!(from_split, parse_steer_message(message).markers);
    }

    #[test]
    fn the_marker_range_is_one_based_and_inclusive() {
        assert!(!image_marker_in_range(0, 2));
        assert!(image_marker_in_range(1, 2));
        assert!(image_marker_in_range(2, 2));
        assert!(!image_marker_in_range(3, 2));
        assert!(!image_marker_in_range(1, 0));
    }

    #[test]
    fn the_phase_caption_mirrors_the_web_labels() {
        assert_eq!(
            phase_label(&ViewerPhase::Live, Some("macbook"), false, false),
            "Live · macbook"
        );
        assert_eq!(phase_label(&ViewerPhase::Live, None, false, false), "Live");
        assert_eq!(
            phase_label(&ViewerPhase::Live, Some("macbook"), true, false),
            "Needs your input · macbook"
        );
        assert_eq!(
            phase_label(&ViewerPhase::Starting, Some("macbook"), false, false),
            "Agent starting…"
        );
        assert_eq!(
            phase_label(&ViewerPhase::Ended { outcome: None }, None, false, false),
            "Session ended"
        );
    }

    /// A paused host wins over every other phase — the run is not gone, the
    /// machine is (EXP-550).
    #[test]
    fn a_paused_host_beats_the_phase() {
        assert_eq!(
            phase_label(&ViewerPhase::Live, Some("macbook"), true, true),
            "Paused · macbook is offline"
        );
        assert_eq!(
            phase_label(&ViewerPhase::Live, None, false, true),
            "Paused · device is offline"
        );
    }

    #[test]
    fn the_ask_counter_hides_itself_for_a_single_step() {
        assert_eq!(ask_counter(Some(2), 3).as_deref(), Some("2 of 3"));
        assert_eq!(ask_counter(None, 3).as_deref(), Some("3 questions"));
        assert_eq!(ask_counter(Some(1), 1), None);
        assert_eq!(ask_counter(None, 0), None);
    }

    /// The subagent row pluralizes; the tool-RUN group never does (it only
    /// forms at two or more).
    #[test]
    fn the_subagent_caption_pluralizes_its_tool_count() {
        assert_eq!(subagent_caption(false, 0), "running");
        assert_eq!(subagent_caption(false, 1), "running · 1 tool call");
        assert_eq!(subagent_caption(true, 7), "done · 7 tool calls");
    }

    #[test]
    fn long_bodies_fold_behind_show_more() {
        assert!(!clampable("short"));
        assert!(clampable(&"x".repeat(CLAMP_CHARS + 1)));
        assert!(clampable(&"line\n".repeat(CLAMP_LINES + 1)));
        assert!(!clampable(&"line\n".repeat(CLAMP_LINES - 1)));
    }

    #[test]
    fn the_kill_copy_names_the_device_only_when_there_is_one() {
        assert!(kill_description(Some("macbook")).starts_with(
            "This force-terminates the terminal on macbook and ends the session."
        ));
        assert!(kill_description(None)
            .starts_with("This force-terminates the terminal and ends the session."));
        assert!(kill_description(Some("")).starts_with(
            "This force-terminates the terminal and ends the session."
        ));
    }

    /// EXP-724: the `/clear` confirm is the same four strings on
    /// web, iOS, Android and here. `Cancel` is [`AlertSpec`]'s own footer
    /// label, which is why it is asserted against the alert, not a constant.
    #[test]
    fn the_clear_confirm_copy_mirrors_the_web_dialog() {
        assert_eq!(slash_commands::confirm_title("clear"), "Run /clear?");
        assert_eq!(
            slash_commands::CONFIRM_BODY,
            "The agent forgets everything in this session so far. Files in the worktree are kept."
        );
        assert_eq!(slash_commands::confirm_button("clear"), "Run /clear");
    }

    /// The two compaction strings the strip and its marker row render come
    /// straight off the feed, byte-identical with the other three clients.
    #[test]
    fn the_compaction_copy_mirrors_the_web_labels() {
        assert_eq!(COMPACTING_LABEL, "Compacting context…");
        assert_eq!(COMPACTED_LABEL, "Context compacted");
    }

    #[test]
    fn the_paused_title_falls_back_to_a_nameless_device() {
        assert_eq!(paused_title(Some("macbook")), "macbook is offline");
        assert_eq!(paused_title(None), "The device is offline");
    }
}
