//! The IDE's steering VIEWER (EXP-696) — the desktop twin of the web
//! `AgentSessionView`.
//!
//! It renders ONE coding session's activity feed + composer over whichever
//! [`FeedSource`] it is handed: the in-process ACP engine, a read-only replay
//! of a finished run, or [`steer::spawn_viewer`] over the relay for a run on
//! another machine (another desktop, the headless CLI daemon, a shared
//! server). EXP-746 moved every one of those into the center pane's
//! [`crate::session_screen`]; the bottom dock keeps PTY tabs and nothing
//! else.
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
//! and the in-session **Merge** pill render for a steered session too. The
//! old rationale ("a remote run's diff is not on this machine") was wrong —
//! the host publishes its worktree diff on the activity channel and
//! [`SteerFeed::latest_diff`] holds it. The bar itself lives in
//! [`crate::changes_bar`]; the session screen's "Changes" rail draws it from
//! [`Self::latest_diff`] and resolves the merge target off the synced
//! `coding_sessions` row.
//!
//! ## Deliberate parity gaps vs the web view (EXP-696)
//!
//! * no subagent conversation TAB strip — subagent work renders inline as
//!   expandable group rows, which is the part of parity that matters;
//! * no fullscreen toggle — the screen's own tab chrome is the desktop's
//!   answer to that.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    bounce, div, ease_in_out, list, prelude::FluentBuilder as _, px, relative,
    AnimationExt as _, AnyElement, App, AppContext as _, ClickEvent, Entity, FocusHandle,
    Focusable, FollowMode, InteractiveElement as _, IntoElement, ListAlignment, ListState,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    StyledImage as _, Styled as _, Subscription, Task, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{self, InputEvent, InputState, Textarea, TextareaState},
    spinner::Spinner,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _,
};
use steer::activity::SessionAgent;
use steer::commands::parse_command;
use steer::feed::{COMPACTED_LABEL, COMPACTING_LABEL, COMPACTION_TIMEOUT};
use steer::{
    answer_key, build_steer_image_message, insert_image_marker, parse_steer_message,
    renumber_image_markers, summarize_subagent_row, AnswerStatus, FeedItem, FeedItemId, FeedKind,
    FeedRow, FeedRowSpec, QuestionOption, SteerFeed, SubagentStatus, ViewerEvent, ViewerHandle,
    ViewerPhase, ANSWER_ACK_TIMEOUT, MAX_STEER_IMAGES, REPLAY_MAX, REPLAY_QUIET,
};

use crate::controls::{glass_input, WebText as _};
use crate::icons::registry;
use crate::slash_commands;
use crate::markdown::image_paste::{
    self, max_upload_bytes_for, pasted_image_parts, read_image_file, validate_image,
};
use crate::native_dialog::{self, AlertSpec};
use crate::transcript_rows::{self, facet, plan_list_sync, ItemFacets, ListOp, RowKey};

/// How long a body may run before it folds behind "Show more" (web
/// `clampable`: >600 chars or >6 lines).
const CLAMP_CHARS: usize = 600;
const CLAMP_LINES: usize = 6;

/// Free-text answers are capped like the web input.
const FREE_TEXT_MAX: usize = 4000;

/// How often the staged-replay fallback re-checks its quiet window.
const STAGING_TICK: Duration = Duration::from_millis(100);

/// EXP-776: how far beyond the viewport the transcript list renders and
/// measures rows (`gpui::ListState` overdraw) — about a screen, so a wheel
/// flick never lands on an unmeasured row.
const FEED_OVERDRAW: gpui::Pixels = px(1024.);

/// EXP-776: the height hint every UNMEASURED row carries after a bulk
/// splice (a journal fold, a replayed backlog, a staged swap), so the
/// scrollbar thumb is sane before the rows have been on screen. Rows vary
/// wildly (a one-line tool call, a screen of prose); the list replaces the
/// hint with the real height as each row is measured.
const FEED_ROW_HINT: gpui::Pixels = px(40.);

/// EXP-776: a splice inserting at least this many rows re-applies
/// [`FEED_ROW_HINT`]. Streaming appends are one or two rows and skip it —
/// re-applying the hint turns every measured row back into a hinted one,
/// which the overdraw then re-measures.
const FEED_BULK_SPLICE: usize = 16;

/// FEED-26 — how long a LIVE run's feed may sit unchanged before the header
/// stops claiming a healthy "Live". The rule is shared byte-for-byte with the
/// web, iOS and Android session headers: only a live, unpaused run that is
/// neither awaiting an answer nor compacting can go stale.
pub(crate) const STALE_ACTIVITY_AFTER: Duration = Duration::from_secs(10 * 60);

/// How often the header re-reads that clock. The feed is clock-free, so a
/// caption counting MINUTES needs a beat of its own or it would sit on
/// whatever number the last event painted.
const STALE_TICK: Duration = Duration::from_secs(30);

/// The pending strip's thumbnail edge (web/iOS parity).
const PENDING_THUMB: f32 = 48.;

/// EXP-724: the open `/` command menu. `items` is already filtered for the
/// session's agent and the typed prefix ([`slash_commands::menu_matches`]);
/// `selected` wraps under ↑/↓.
struct SlashMenu {
    items: Vec<slash_commands::MenuCommand>,
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

/// Where a [`SteerSessionView`]'s feed comes from (EXP-746).
///
/// One renderer, four sources: today's relay viewer, an in-process ACP run on
/// this machine, a read-only replay of a finished one, and (EXP-773) a
/// finished run read straight off this device's activity JOURNAL. `Local` and
/// `Replay` additionally produce local-only rich items (per-edit diff cards,
/// command output, the pinned plan, thoughts) that never touch the wire.
pub(crate) enum FeedSource {
    /// An in-process ACP session this host is running.
    Local { session: engine::EngineSession },
    /// The relay viewer: a run on another machine (or another process here).
    /// `None` = the socket could not be dialled at all.
    Remote { handle: Option<ViewerHandle> },
    /// A finished run replayed through the engine's `session/load`.
    Replay { session: engine::EngineSession },
    /// EXP-773 — a finished run read off `{data_dir}/journal/<id>.jsonl`
    /// ([`steer::read_journal`]): the SAME events the relay fanned out, folded
    /// and replayed into the same reducer, with no engine and no socket. This
    /// is what the desktop shows for its OWN past runs; the events are drained
    /// into the feed by the constructor.
    Journal { events: Vec<steer::frames::ActivityEvent> },
}

impl FeedSource {
    /// EXP-773 — a read-only view over a past run's on-device journal.
    pub(crate) fn journal(events: Vec<steer::frames::ActivityEvent>) -> Self {
        FeedSource::Journal { events }
    }

    /// The relay handle, when this source HAS one. Steering, answers and
    /// wakeups all go through it; a local source answers `None` and drives its
    /// [`engine::EngineSession`] directly instead.
    fn handle(&self) -> Option<&ViewerHandle> {
        match self {
            FeedSource::Remote { handle } => handle.as_ref(),
            FeedSource::Local { .. } | FeedSource::Replay { .. } | FeedSource::Journal { .. } => {
                None
            }
        }
    }

    /// The in-process engine, when this source has one. `Replay` has one too
    /// — it is a real (read-only) engine session replaying `session/load`.
    fn session(&self) -> Option<&engine::EngineSession> {
        match self {
            FeedSource::Local { session } | FeedSource::Replay { session } => Some(session),
            FeedSource::Remote { .. } | FeedSource::Journal { .. } => None,
        }
    }

    /// The engine this source may be STEERED through. A replay is a
    /// transcript: it has an engine, and answering it would mean answering
    /// questions that were resolved days ago.
    fn steerable_session(&self) -> Option<&engine::EngineSession> {
        match self {
            FeedSource::Local { session } => Some(session),
            FeedSource::Replay { .. } | FeedSource::Remote { .. } | FeedSource::Journal { .. } => {
                None
            }
        }
    }

    /// A replay renders history and offers no composer at all — and so does a
    /// journal read, which is history off the disk.
    fn read_only(&self) -> bool {
        matches!(self, FeedSource::Replay { .. } | FeedSource::Journal { .. })
    }
}

/// The steering view for ONE coding session — remote, local or replayed.
pub(crate) struct SteerSessionView {
    session_id: String,
    /// The synced row, re-snapshotted whenever `coding_sessions` notifies.
    row: Option<domain::rows::CodingSession>,
    feed: SteerFeed,
    /// EXP-746: what drives the feed. The relay handle lives inside it.
    source: FeedSource,
    /// Whether this view paints its OWN header. The session screen paints a
    /// wider one around this view and turns it off, so the transcript never
    /// carries two identity rows (EXP-746).
    chrome: bool,
    phase: ViewerPhase,
    connected: bool,
    /// EXP-696 wakeups: the last seen edge states, so only a TRANSITION back
    /// to reachable nudges the socket (an observer fires on plenty of
    /// non-edges).
    device_offline: bool,
    sync_offline: bool,
    /// EXP-776: whether the ONE staged-replay fallback task is running. The
    /// old design spawned a fresh tick task per buffered event and let the
    /// generation counter retire the rest — a 2000-event replay meant 2000
    /// timers. Now every event only stamps [`Self::staging_last_event`] and
    /// the single loop reads it.
    staging_armed: bool,
    /// When the last replayed event landed — the quiet window
    /// ([`REPLAY_QUIET`]) counts from here.
    staging_last_event: Instant,
    /// When the CURRENT staging window opened. The quiet window restarts on
    /// every buffered event; the [`REPLAY_MAX`] cap deliberately does not —
    /// a replay that keeps trickling must still commit.
    staging_started: Option<Instant>,
    /// Bumped whenever a compaction opens; a [`COMPACTION_TIMEOUT`] backstop
    /// that finds its generation superseded exits without clearing.
    compaction_generation: u64,
    /// FEED-26: when the feed last MOVED — the clock behind the header's
    /// "No activity for N min". Stamped on any appended/replaced row and on
    /// the edge into [`ViewerPhase::Live`], deliberately NOT read off event
    /// `at` timestamps (optional on the wire, and a host's clock is not
    /// ours). A reopened tab replays its backlog, which stamps: a view built
    /// over an already-stalled run waits out the window once more before it
    /// says so.
    last_activity: std::time::Instant,
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
    /// EXP-746: the local-only cards (per-edit diffs, command output, the
    /// pinned plan, thoughts) a `Local`/`Replay` source produces. Empty for
    /// every remote session — the wire carries none of it.
    extras: crate::session_extras::LocalExtras,
    /// EXP-773: the "Latest changes" bar's parse of the published worktree
    /// diff, and the per-file list its expanded half renders into. It lives
    /// HERE rather than on the hosting screen because the bar sits between the
    /// transcript and the composer (web `agent-session` parity), and both of
    /// those are this view's.
    changes: Option<crate::changes_bar::ChangesSnapshot>,
    changes_diff: Entity<crate::diff::DiffView>,
    /// The extras cards expanded on a row (their own set: a folded tool BODY
    /// and a folded diff are different questions about the same row).
    expanded_extras: HashSet<FeedItemId>,
    /// EXP-776: the virtualised transcript. `gpui::list` renders and
    /// measures only the rows in and around the viewport, and its
    /// [`FollowMode::Tail`] IS the auto-scroll: it snaps to the end on every
    /// layout while following, stops on an upward wheel, re-arms when the
    /// reader is back within a pixel of the bottom (a wheel or a scrollbar
    /// drag), and clamps wheel deltas at event time — so a trackpad's
    /// momentum over-scroll can never read as "the reader moved up" the way
    /// it did with the hand-rolled follow this replaced (EXP-732).
    list: ListState,
    /// EXP-776: the cached row projection the list indexes into, refreshed
    /// once per frame by [`Self::sync_list`] together with `row_keys` (what
    /// the list was last told) and `active` (the answerable cards, read by
    /// the header and the rows instead of being rebuilt per call).
    rows: Vec<FeedRowSpec>,
    row_keys: Vec<RowKey>,
    active: HashSet<FeedItemId>,
    /// Whether the synthetic trailing "Working…" row is present — the list's
    /// last index when it is.
    working: bool,
    /// EXP-776: the frame's issue-chip resolver (EXP-760), built ONCE per
    /// frame over `chip_cache` instead of per prose body. `None` for a run
    /// with no resolvable team.
    chips: Option<crate::markdown::RefResolver>,
    /// The memo behind `chips`, cleared when issues or statuses change.
    chip_cache: crate::markdown::IssueChipCache,
    focus_handle: FocusHandle,
    _drain: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl SteerSessionView {
    /// EXP-746 — the view over any [`FeedSource`]. A `Remote` source with no
    /// handle yet is DIALLED here and the socket stays up for the view's
    /// whole life; `Local` and `Replay` bring their own engine session.
    pub(crate) fn with_source(
        session_id: String,
        source: FeedSource,
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
            // EXP-776: the chip memo is only as fresh as the issues and
            // statuses it was read from (glyph and tint are part of a chip).
            subscriptions.push(cx.observe(&collections.issues, |this, _, cx| {
                this.chip_cache.borrow_mut().clear();
                cx.notify();
            }));
            subscriptions.push(cx.observe(&collections.issue_statuses, |this, _, cx| {
                this.chip_cache.borrow_mut().clear();
                cx.notify();
            }));
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
            source,
            chrome: true,
            phase: ViewerPhase::Connecting,
            connected: false,
            device_offline: false,
            sync_offline: false,
            staging_armed: false,
            staging_last_event: Instant::now(),
            staging_started: None,
            compaction_generation: 0,
            last_activity: std::time::Instant::now(),
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
            extras: crate::session_extras::LocalExtras::default(),
            changes: None,
            changes_diff: cx.new(|cx| {
                let mut diff = crate::diff::DiffView::new(window, cx);
                // EXP-773: the expanded body is a per-file collapsible list,
                // like the web `FileDiffList` — not one flat wall of hunks.
                diff.set_collapsible(true);
                diff
            }),
            expanded_extras: HashSet::new(),
            list: {
                // A session tab opens on its newest rows (the question
                // waiting for an answer, the composer's context) and every
                // appended row keeps it there: the list starts FOLLOWING.
                let list = ListState::new(0, ListAlignment::Top, FEED_OVERDRAW);
                list.set_follow_mode(FollowMode::Tail);
                list
            },
            rows: Vec::new(),
            row_keys: Vec::new(),
            active: HashSet::new(),
            working: false,
            chips: None,
            chip_cache: Default::default(),
            focus_handle: cx.focus_handle(),
            _drain: drain,
            _subscriptions: subscriptions,
        };
        this.refresh_row(cx);
        this.arm_stale_tick(cx);
        // Seed the wakeup edges from the world as it is right now, so the
        // first observer call is a comparison rather than a false edge.
        this.device_offline = this.device(cx).offline;
        this.sync_offline = sync_offline(cx);
        // Only the relay source dials — and only when the caller did not hand
        // one in. A local source subscribes to its engine instead: same
        // reducer, same rendering, no socket.
        if let FeedSource::Remote { handle } = &mut this.source {
            if handle.is_none() {
                *handle = spawn_viewer(&session_id, events_tx, cx);
            }
            if handle.is_none() {
                this.phase = ViewerPhase::Unauthorized {
                    detail: Some("Live steering is unavailable on this instance.".to_string()),
                };
            }
        }
        // EXP-773: a journal source is the whole transcript, already on disk.
        // Fold it into the SAME reducer the wire feeds, then mark the run
        // over: there is nothing to connect to and nothing to steer.
        let journal = match &mut this.source {
            FeedSource::Journal { events } => Some(std::mem::take(events)),
            _ => None,
        };
        if let Some(events) = journal {
            for event in events {
                this.feed.apply(event);
            }
            this.sync_changes(cx);
            this.refresh_active();
            this.connected = false;
            this.phase = ViewerPhase::Ended { outcome: None };
            // The whole run is already here: open on its end.
            this.list.scroll_to_end();
        }
        if let Some(session) = this.source.session().cloned() {
            // EXP-746 review UI-2: seed the phase from the engine, so a view
            // built over a run that has been live for an hour is steerable on
            // its FIRST paint rather than after the replay reaches it.
            if let Some(phase) = session.phase() {
                // EXP-758: a view attached AFTER the run failed seeds the
                // reason too: `Failed` is as terminal as `Ended` here.
                this.connected = !matches!(
                    phase,
                    engine::EnginePhase::Ended | engine::EnginePhase::Failed(_)
                );
                this.phase = viewer_phase(phase.clone());
            }
            // `subscribe` replays the buffered backlog first, so a view built
            // long after the run started (a reopened tab, a rebuilt screen)
            // sees the whole session and not just the tail.
            let events = session.subscribe();
            this._drain = cx.spawn_in(window, async move |this, cx| {
                while let Ok(event) = events.recv_async().await {
                    if this
                        .update_in(cx, |this, window, cx| this.apply_local(event, window, cx))
                        .is_err()
                    {
                        return;
                    }
                }
                // The engine dropped its sender: the run is over, whether or
                // not a `Phase(Ended)` made it out first.
                let _ = this.update(cx, |this, cx| {
                    // The reason, if there was one, already arrived as
                    // `Phase(Failed)` (EXP-758); this edge only closes.
                    this.note_run_ended(None, cx);
                });
            });
        }
        this
    }

    /// EXP-746: hand the header to the hosting screen (see [`Self::chrome`]).
    pub(crate) fn set_chrome(&mut self, chrome: bool) {
        self.chrome = chrome;
    }

    /// `(identifier, subject)` for whoever paints the header.
    pub(crate) fn header_identity(&self, cx: &App) -> (Option<SharedString>, SharedString) {
        self.identity(cx)
    }

    /// The header's liveness dot tone and its caption, resolved together
    /// because both read the same four facts (paused, awaiting an answer,
    /// the phase, and FEED-26's stale-activity clock).
    pub(crate) fn header_status(&self, cx: &App) -> (gpui::Hsla, String) {
        let paused = self.paused(cx);
        let awaiting = !self.active.is_empty();
        let stale = self.stale_minutes(paused, awaiting);
        let device = self.device(cx);
        (
            self.phase_tone(cx, paused, awaiting, stale.is_some()),
            phase_label(
                &self.phase,
                device.label.as_deref(),
                awaiting,
                paused,
                stale,
            ),
        )
    }

    /// What to print on the header's agent pill: the run's own agent, which
    /// for an EXTERNAL one is the user's label (the synced row cannot name
    /// it — `coding_sessions.agent` takes contract values only).
    pub(crate) fn agent_display_label(&self) -> String {
        if let Some(session) = self.source.session() {
            return session.agent_label().to_string();
        }
        match self.agent() {
            SessionAgent::Claude => coding::CodingAgent::Claude.label().to_string(),
            SessionAgent::Codex => coding::CodingAgent::Codex.label().to_string(),
            SessionAgent::Pi => coding::CodingAgent::Pi.label().to_string(),
            SessionAgent::External => "Agent".to_string(),
        }
    }

    /// The builtin agent behind this session, for the usage sheet's device
    /// cards (an external agent reports no usage windows of its own).
    pub(crate) fn builtin_agent(&self) -> Option<coding::CodingAgent> {
        match self.agent() {
            SessionAgent::Claude => Some(coding::CodingAgent::Claude),
            SessionAgent::Codex => Some(coding::CodingAgent::Codex),
            SessionAgent::Pi => Some(coding::CodingAgent::Pi),
            SessionAgent::External => None,
        }
    }

    /// The host machine's name, for the header caption.
    pub(crate) fn device_label(&self, cx: &App) -> Option<String> {
        self.device(cx).label
    }

    /// Whether the header may offer a kill.
    pub(crate) fn killable(&self, cx: &App) -> bool {
        self.can_kill(cx)
    }

    /// The synced `coding_sessions` row behind this viewer — what the
    /// header names the run from, and what the Latest-changes bar resolves
    /// its Merge target from
    /// ([`crate::changes_bar::merge_meta_for_session`]).
    pub(crate) fn session_row(&self) -> Option<&domain::rows::CodingSession> {
        self.row.as_ref()
    }

    /// EXP-760: the team whose issues this feed's `#IDENT` / bare `EXP-1`
    /// tokens resolve against — the session row's board's team (the
    /// `IssueHeader::team_id_of` recipe). `None` for a board-less run (an
    /// action or a repo-less chat), where nothing chips.
    fn ref_team_id(&self, cx: &App) -> Option<String> {
        let row = self.row.as_ref()?;
        // The row carries the team denormalized (batch and action rows have
        // no issue at all); an old row without it falls back to its issue's
        // board — the `IssueHeader::team_id_of` recipe.
        if let Some(team_id) = row.team_id.clone().filter(|id| !id.is_empty()) {
            return Some(team_id);
        }
        let collections = sync::Store::global(cx).collections();
        let issue_id = row.issue_id.clone()?;
        let board_id = collections.issues.read(cx).get(&issue_id)?.board_id.clone();
        collections
            .boards
            .read(cx)
            .get(&board_id)
            .map(|board| board.team_id.clone())
    }

    /// EXP-760: the feed's chip resolver — BARE mode, because agents narrate
    /// identifiers without a `#` (`EXP-758`, `exp/EXP-758`). Display-only:
    /// nothing here is stored, so the `#IDENT` interchange contract and the
    /// auto-relations it drives are untouched. `None` (board-less run, team
    /// not synced) leaves every token as plain text.
    ///
    /// EXP-776: built ONCE per frame ([`Self::sync_list`]) over the view's
    /// chip memo; a transcript full of `EXP-nnn` tokens used to build a
    /// fresh resolver per prose body and re-scan the team's issues per token.
    fn ref_resolver(&self, cx: &App) -> Option<crate::markdown::RefResolver> {
        self.ref_team_id(cx).map(|team_id| {
            crate::markdown::RefResolver::from_store(team_id)
                .bare(true)
                .memoized(self.chip_cache.clone())
        })
    }

    /// EXP-760: hang the feed's bare-mode chip resolver (and the in-app
    /// open) on one prose view. A run with no resolvable team leaves the view
    /// exactly as it was — every token stays plain text.
    fn with_issue_chips(
        &self,
        view: crate::markdown::MarkdownView,
        cx: &App,
    ) -> crate::markdown::MarkdownView {
        let Some(resolver) = self.chips.clone() else {
            return view;
        };
        let Some(team_id) = self.ref_team_id(cx) else {
            return view;
        };
        view.resolver(resolver)
            .on_open_issue(move |identifier, window, cx| {
                crate::description_editor::open_issue_by_identifier(
                    &team_id, identifier, window, cx,
                );
            })
    }

    // ── EXP-776: the cached row projection + list sync ─────────────────────

    /// Re-read the answerable cards off the feed. Called wherever the feed
    /// mutates (and at the top of every frame), so the header and the rows
    /// read one cached set instead of rebuilding it per call.
    fn refresh_active(&mut self) {
        self.active = self.feed.active_question_ids();
    }

    /// Whether the synthetic "Working…" row sits under the transcript: a
    /// live, unended run with nothing waiting on the reader and no strip
    /// already saying what is happening.
    fn working_now(&self) -> bool {
        !self.feed.is_empty()
            && self.phase == ViewerPhase::Live
            && !self.row_ended()
            && self.active.is_empty()
            && !self.feed.is_staging()
            // EXP-724: the compaction strip already says what is happening —
            // a second "Working…" under it is noise (web parity).
            && self.feed.compacting().is_none()
    }

    /// The view-side bits of one item that move its rendered height — see
    /// [`transcript_rows::row_fingerprint`].
    fn item_facets(&self, item: &FeedItem) -> ItemFacets {
        let mut facets = 0;
        if self.expanded_bodies.contains(&item.id) {
            facets |= facet::BODY_EXPANDED;
        }
        if self.extras.has_extras(item.id) {
            facets |= facet::HAS_EXTRAS;
            if self.expanded_extras.contains(&item.id) {
                facets |= facet::EXTRAS_EXPANDED;
            }
        }
        if let Some(key) = answer_key(item) {
            if self.feed.is_answer_locked(&key) {
                facets |= facet::ANSWER_LOCKED;
            }
            if self.free_text.as_ref().is_some_and(|(answer, _)| *answer == key) {
                facets |= facet::FREE_TEXT_OPEN;
            }
        }
        facets
    }

    /// The focus handle a freshly spliced row registers with the list: an
    /// answerable card carries the free-text input's, so an answer being
    /// typed on a card the reader scrolled off-screen keeps receiving keys
    /// (the list renders a focused off-screen item for exactly this).
    fn row_focus_handle(&self, ix: usize, cx: &App) -> Option<FocusHandle> {
        let spec = self.rows.get(ix)?;
        let items = self.feed.items();
        spec.item_indices()
            .iter()
            .any(|&item| items.get(item).is_some_and(|item| self.active.contains(&item.id)))
            .then(|| self.free_text_input.focus_handle(cx))
    }

    /// Refresh the cached projection and tell the list what changed.
    ///
    /// Runs at the top of EVERY frame: an O(n) pass over at most `FEED_CAP`
    /// items is nothing next to the element work it replaces, and a single
    /// site cannot miss a mutation the way per-event bookkeeping could. The
    /// diff against last frame's keys ([`plan_list_sync`]) becomes splices
    /// (rows came or went) and remeasures (a row's content moved); the list
    /// re-renders every VISIBLE row each layout regardless, so a streaming
    /// row growing on screen needs nothing more than the notify that got us
    /// here.
    fn sync_list(&mut self, cx: &mut gpui::Context<Self>) {
        self.refresh_active();
        self.rows = self.feed.row_specs();
        self.working = self.working_now();
        self.chips = self.ref_resolver(cx);
        let items = self.feed.items();
        let mut keys: Vec<RowKey> = Vec::with_capacity(self.rows.len() + 1);
        for spec in &self.rows {
            let id = spec.id();
            let fingerprint = transcript_rows::row_fingerprint(
                spec,
                items,
                self.expanded_groups.contains(&id),
                |item| self.item_facets(item),
            );
            keys.push(RowKey { id, fingerprint });
        }
        if self.working {
            keys.push(RowKey::WORKING);
        }
        let ops = plan_list_sync(&self.row_keys, &keys);
        if ops.is_empty() {
            return;
        }
        let mut bulk = false;
        for op in ops {
            match op {
                ListOp::Splice { range, count } => {
                    bulk |= count >= FEED_BULK_SPLICE;
                    let start = range.start;
                    let handles: Vec<Option<FocusHandle>> = (start..start + count)
                        .map(|ix| self.row_focus_handle(ix, cx))
                        .collect();
                    self.list.splice_focusable(range, handles);
                }
                ListOp::Remeasure(range) => self.list.remeasure_items(range),
            }
        }
        self.row_keys = keys;
        if bulk {
            // `ListState` is a shared handle: hinting a clone hints the list.
            let _ = self.list.clone().with_uniform_item_height(FEED_ROW_HINT);
        }
    }

    /// Whether the run is over — a merged/ended session offers no Merge
    /// (web/iOS `canMerge` gate their pill on the same liveness).
    pub(crate) fn session_over(&self) -> bool {
        self.row_ended() || matches!(self.phase, ViewerPhase::Ended { .. })
    }

    /// The tab closed (the row ended, the user signed out, the window closed).
    ///
    /// EXP-746: this drops the FEED, never the run. A local engine keeps
    /// working when its tab goes away — killing an agent because a tab closed
    /// is exactly the surprise the header's kill affordance exists to avoid.
    pub(crate) fn shutdown(&mut self) {
        // EXP-724: nothing is coming to close an open compaction strip once
        // the socket is gone.
        self.feed.clear_compaction();
        if let Some(handle) = self.source.handle() {
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
            if let Some(handle) = self.source.handle() {
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
        let Some(handle) = self.source.handle() else {
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
        // EXP-746: a run this process hosts is ours by construction — the
        // ownership question the remote gate asks is about someone ELSE's
        // machine, and a live engine here is always killable (a replay never
        // is: there is nothing running).
        if self.source.steerable_session().is_some() {
            return !matches!(self.phase, ViewerPhase::Ended { .. }) && !self.row_ended();
        }
        if self.source.read_only() {
            return false;
        }
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
        let pulse = feed_pulse(&self.feed);
        match event {
            ViewerEvent::Phase(phase) => self.note_phase(phase),
            ViewerEvent::Connected(connected) => {
                self.connected = connected;
                if !connected && self.feed.is_staging() {
                    // A partial replay of a room we are no longer joined to.
                    self.feed.discard_staging();
                }
            }
            ViewerEvent::Activity(activity) => {
                self.feed.apply(activity);
                self.sync_changes(cx);
                if self.feed.is_staging() {
                    self.arm_staging_swap(cx);
                }
            }
            ViewerEvent::Reset => {
                self.feed.apply_reset();
                self.staging_started = Some(Instant::now());
                self.arm_staging_swap(cx);
            }
            ViewerEvent::Synced => {
                self.feed.apply_synced();
                self.sync_changes(cx);
            }
            ViewerEvent::Keepalive => {
                // EXP-656: a keepalive is also an end-of-replay signal for a
                // markerless republish — the beat means the burst is over.
                if self.feed.is_staging() {
                    self.feed.force_swap();
                    self.sync_changes(cx);
                }
            }
            ViewerEvent::LocalMessage(text) => {
                self.feed.push_local_message(&text);
            }
        }
        self.refresh_active();
        self.note_feed_moved(pulse);
        self.note_compaction(was_compacting, cx);
        cx.notify();
    }

    /// FEED-26 — every phase assignment that is not an END goes through here.
    /// The edge INTO `Live` starts the stale-activity clock: a run that has
    /// only just connected has not been quiet for anything yet, and its feed
    /// may not move for a while after the handshake.
    fn note_phase(&mut self, phase: ViewerPhase) {
        if phase == ViewerPhase::Live && self.phase != ViewerPhase::Live {
            self.last_activity = std::time::Instant::now();
        }
        self.phase = phase;
    }

    /// Stamp the stale-activity clock when the feed actually moved between
    /// `before` and now (a latest-wins state kind — config, usage, the
    /// published diff — appends nothing and is not activity the reader can
    /// see).
    fn note_feed_moved(&mut self, before: FeedPulse) {
        if feed_pulse(&self.feed) != before {
            self.last_activity = std::time::Instant::now();
        }
    }

    /// FEED-26: whole minutes of quiet, or `None` while the header must keep
    /// saying "Live" — see [`STALE_ACTIVITY_AFTER`]. Paused, awaiting an
    /// answer and compacting are all deliberate quiet: the header already
    /// says what is happening, and none of them is a stalled run.
    fn stale_minutes(&self, paused: bool, awaiting: bool) -> Option<u64> {
        if paused || awaiting || self.phase != ViewerPhase::Live {
            return None;
        }
        if self.feed.compacting().is_some() {
            return None;
        }
        let quiet = self.last_activity.elapsed();
        (quiet >= STALE_ACTIVITY_AFTER).then(|| quiet.as_secs() / 60)
    }

    /// FEED-26 — the header's own beat. The feed is clock-free and a stalled
    /// run produces no events by definition, so the minute count needs a
    /// wakeup of its own; it only repaints once a live run is ACTUALLY quiet
    /// past the window, so an ordinary session pays a timer and nothing else.
    fn arm_stale_tick(&self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(STALE_TICK).await;
            if this
                .update(cx, |this, cx| {
                    let paused = this.paused(cx);
                    let awaiting = !this.active.is_empty();
                    if this.stale_minutes(paused, awaiting).is_some() {
                        cx.notify();
                    }
                })
                .is_err()
            {
                // The view is gone with its tab.
                return;
            }
        })
        .detach();
    }

    /// EXP-746 — one event off the in-process engine.
    ///
    /// `Activity` is the exact event the relay got, so it goes through the
    /// SAME `SteerFeed::apply` a remote session uses (that is what "one
    /// renderer, three sources" means); everything else is a local card and
    /// lands in [`crate::session_extras`]. The `tool_call_id` riding the
    /// activity is the join: the row it just appended is the row those cards
    /// hang off.
    fn apply_local(
        &mut self,
        event: engine::LocalFeedEvent,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match event {
            engine::LocalFeedEvent::Activity {
                event,
                tool_call_id,
            } => {
                let was_compacting = self.feed.compacting().is_some();
                let pulse = feed_pulse(&self.feed);
                let before = self.feed.items().last().map(|item| item.id);
                self.feed.apply(event);
                if let Some(tool_call_id) = tool_call_id {
                    let after = self.feed.items().last().map(|item| item.id);
                    // Only a row that was actually APPENDED gets bound — a
                    // latest-wins kind (config/usage/diff) appends nothing.
                    if after != before {
                        if let Some(item) = after {
                            self.extras.bind(item, tool_call_id);
                        }
                    }
                }
                self.refresh_active();
                self.note_feed_moved(pulse);
                self.sync_changes(cx);
                self.note_compaction(was_compacting, cx);
            }
            engine::LocalFeedEvent::Phase(phase) => {
                // EXP-758: `Failed` is terminal too, and it arrives BEFORE
                // the `Ended` that used to be the only end edge here, so the
                // merge keeps its reason when that one lands.
                let over = matches!(
                    phase,
                    engine::EnginePhase::Ended | engine::EnginePhase::Failed(_)
                );
                self.note_phase(merge_ended_phase(&self.phase, viewer_phase(phase.clone())));
                // There is no socket on this path — "connected" is simply
                // whether the engine is still talking to us.
                self.connected = !over;
                if over {
                    // EXP-724: nothing is coming to close an open strip.
                    self.feed.clear_compaction();
                    // …and nothing is coming to close a terminal card that
                    // never got an exit code either.
                    self.extras.apply(engine::LocalFeedEvent::Phase(phase));
                }
            }
            other => self.extras.apply(other),
        }
        cx.notify();
    }

    /// The run is over. Two callers, one edge: the engine's feed closing
    /// (with or without a final `Phase`), and the HOST's own exit callback,
    /// which can land a round trip before the synced row flips. A composer
    /// over a dead engine would silently swallow every message, so this is
    /// deliberately idempotent and never waits for the row.
    ///
    /// EXP-758: `failure` is the host's `EngineExit::error`. It is the one
    /// thing that may overwrite an end already recorded: the feed closing
    /// can beat the exit callback, and an empty transcript that only says
    /// "ended" is exactly the report this fixes.
    pub(crate) fn note_run_ended(
        &mut self,
        failure: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.connected = false;
        if !matches!(self.phase, ViewerPhase::Ended { .. }) || failure.is_some() {
            let next = ViewerPhase::Ended {
                outcome: failure.as_deref().map(failure_banner),
            };
            self.phase = merge_ended_phase(&self.phase, next);
            self.feed.clear_compaction();
        }
        // The same edge as the engine's `Phase(Ended)`, on the path where no
        // phase arrives at all: a card still marked live has nothing left to
        // end it (review C4).
        self.extras.end_live();
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
    /// [`REPLAY_MAX`].
    ///
    /// EXP-776: ONE task per staging window. Every buffered event lands here,
    /// stamps [`Self::staging_last_event`] and returns; the single loop reads
    /// that stamp on its tick and disarms itself when it swaps, when the
    /// staging was discarded, or when the view is gone.
    fn arm_staging_swap(&mut self, cx: &mut gpui::Context<Self>) {
        self.staging_last_event = Instant::now();
        if self.staging_armed {
            return;
        }
        self.staging_armed = true;
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(STAGING_TICK).await;
            let Ok(done) = this.update(cx, |this, cx| {
                if !this.feed.is_staging() {
                    this.staging_started = None;
                    this.staging_armed = false;
                    return true;
                }
                let capped = this
                    .staging_started
                    .is_some_and(|started| started.elapsed() >= REPLAY_MAX);
                if this.staging_last_event.elapsed() >= REPLAY_QUIET || capped {
                    let was_compacting = this.feed.compacting().is_some();
                    let pulse = feed_pulse(&this.feed);
                    this.feed.force_swap();
                    this.refresh_active();
                    this.note_feed_moved(pulse);
                    this.sync_changes(cx);
                    this.note_compaction(was_compacting, cx);
                    this.staging_started = None;
                    this.staging_armed = false;
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
        })
        .detach();
    }

    // ── Answering ──────────────────────────────────────────────────────────

    /// Whether this card's answer is in flight (an id-less card never is —
    /// it has no answer key).
    fn is_answer_locked(&self, item: &FeedItem) -> bool {
        answer_key(item).is_some_and(|key| self.feed.is_answer_locked(&key))
    }

    /// Send one answer and lock the card. Always semantic: EXP-730 retired
    /// the raw-keystroke path, so a card with no wire id is not answerable at
    /// all (it renders read-only, as on web and Android).
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
        // An id-less card carries no key: nothing to send, nothing to lock.
        let Some(key) = answer_key(item) else {
            return;
        };
        if self.feed.is_answer_locked(&key) {
            return;
        }
        let Some(card) = item.question().cloned() else {
            return;
        };
        let Some(question_id) = card.question_id.as_deref() else {
            return;
        };
        // EXP-746: the OPTION KEYS are the same either way — an ACP option id
        // travels the wire verbatim (D3 retired the keystroke path), so the
        // only difference is who receives them.
        let sent = match &self.source {
            FeedSource::Local { session } => {
                session.answer(steer::RemoteAnswer {
                    question_id: question_id.to_string(),
                    ask_id: card.ask_id.clone(),
                    keys: keys.clone(),
                    text: text.clone(),
                });
                true
            }
            // A replay (and a journal read) is history: its questions were
            // answered — or dropped — when the run happened.
            FeedSource::Replay { .. } | FeedSource::Journal { .. } => false,
            FeedSource::Remote { handle } => handle.as_ref().is_some_and(|handle| {
                handle.send_answer(question_id, card.ask_id.as_deref(), &keys, text.as_deref())
            }),
        };
        if !sent {
            self.notice = Some(SharedString::from("The session is no longer connected"));
            cx.notify();
            return;
        }
        self.feed.note_answer_sent(&key, keys, labels);
        // FEED-26: answering is the freshest activity there is — the quiet
        // that led up to the card was the reader's, not the agent's.
        self.last_activity = std::time::Instant::now();
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
            .find(|item| answer_key(item).is_some_and(|key| key == answer))
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
            && !self.source.read_only()
            && self.phase == ViewerPhase::Live
            && self.connected
            && !self.row_ended()
            && has_content
    }

    fn composer_visible(&self) -> bool {
        // EXP-746: a replay is a transcript — there is nothing to type at.
        !self.source.read_only()
            && !self.row_ended()
            && !matches!(self.phase, ViewerPhase::Ended { .. })
    }

    // ── Slash commands (EXP-724) ───────────────────────────────────────────

    /// Which catalog this session's composer offers. A row that names no
    /// agent is a claude run (contract order).
    fn agent(&self) -> SessionAgent {
        // EXP-746: a local run KNOWS its agent — including an external one,
        // which the synced row cannot name at all (`coding_sessions.agent`
        // takes contract values only, so an external run syncs `NULL`).
        if let Some(session) = self.source.session() {
            return match session.agent() {
                coding::AgentKind::Builtin(coding::CodingAgent::Claude) => SessionAgent::Claude,
                coding::AgentKind::Builtin(coding::CodingAgent::Codex) => SessionAgent::Codex,
                coding::AgentKind::Builtin(coding::CodingAgent::Pi) => SessionAgent::Pi,
                coding::AgentKind::External(_) => SessionAgent::External,
            };
        }
        // A REMOTE run has only its row to go on, and an EXTERNAL agent's row
        // names no agent at all — the `config_state` it published (nothing on
        // the PTY path does) is what tells the two apart.
        let acp = self.feed.config().is_some();
        self.row
            .as_ref()
            .map_or(SessionAgent::Claude, |row| slash_commands::agent_of(row, acp))
    }

    /// The `/` menu's agent half — what the AGENT advertised for this run
    /// (`config_state.commands`), empty until it says.
    fn agent_commands(&self) -> Vec<steer::frames::ConfigCommand> {
        self.feed
            .config()
            .map(|config| config.commands.clone())
            .unwrap_or_default()
    }

    /// EXP-772: the composer's ONE chip — the session mode, or `None` when
    /// the run advertises no modes.
    pub(crate) fn mode_chip(&self) -> Option<crate::session_extras::ConfigChip> {
        crate::session_extras::mode_chip(self.feed.config())
    }

    /// EXP-772: the plan/build pair behind the compact "Plan" switch.
    pub(crate) fn plan_toggle(&self) -> Option<crate::session_extras::PlanModeToggle> {
        crate::session_extras::plan_mode_toggle(self.feed.config())
    }

    /// EXP-746: the run's context/spend meter (the usage sheet's own block).
    pub(crate) fn usage(&self) -> Option<steer::SessionUsage> {
        self.feed.usage()
    }

    // ── EXP-773: the "Latest changes" bar ─────────────────────────────────

    /// Install the parse of the newly published diff, when
    /// [`crate::changes_bar::sync`] says it changed. The cache key is the raw
    /// string, so a feed event that carried no new diff parses nothing.
    fn sync_changes(&mut self, cx: &mut gpui::Context<Self>) {
        let next = {
            // Borrowed, never cloned: the published diff runs to 512 KiB and
            // this fires on every feed event, so an unchanged one must cost
            // a pointer comparison.
            let raw = self.feed.latest_diff();
            crate::changes_bar::sync(self.changes.as_ref(), &self.session_id, raw)
        };
        let Some(next) = next else {
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

    /// Flip the bar open/shut, building the diff rows the first time it opens
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

    /// EXP-773 — the collapsible "Latest changes +N −M [Merge]" row, painted
    /// UNDER the transcript and above the composer (web `agent-session`). It
    /// draws for a diff OR an open PR, so the Merge pill never stands alone.
    fn render_changes_bar(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let merge = self
            .session_row()
            .and_then(|row| crate::changes_bar::merge_meta_for_session(row, cx));
        let merge = crate::changes_bar::merge_when_live(merge, self.session_over());
        let has_diff = self.feed.latest_diff().is_some();
        if !crate::changes_bar::changes_bar_visible(has_diff, merge.is_some()) {
            return None;
        }
        let state = self.changes.as_ref();
        let expanded = state.is_some_and(|state| state.expanded);
        let totals = state.map(|state| (state.additions, state.deletions));
        Some(crate::changes_bar::render(
            crate::changes_bar::ChangesSpec {
                toggle_id: "session-changes-toggle",
                totals,
                expanded,
                merge,
                diff_view: self.changes_diff.clone(),
                on_toggle: Box::new(|this: &mut Self, cx| this.toggle_changes_expanded(cx)),
                // No tab to close here, and the server ends the session on
                // merge anyway (EXP-498).
                on_merged: None,
            },
            cx,
        ))
    }

    /// EXP-746 — switch the session mode (`plan` ⇄ the agent's default).
    pub(crate) fn set_mode(&self, id: &str) {
        match &self.source {
            FeedSource::Local { session } => session.set_mode(id),
            FeedSource::Replay { .. } | FeedSource::Journal { .. } => {}
            FeedSource::Remote { handle } => {
                if let Some(handle) = handle.as_ref() {
                    handle.send_mode(id);
                }
            }
        }
    }

    /// Whether this view drives an in-process engine (the header's kill copy
    /// and the screen's banners fork on it).
    pub(crate) fn is_local(&self) -> bool {
        self.source.steerable_session().is_some()
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
        // EXP-746: the contract catalog UNION whatever this agent advertised.
        let items = slash_commands::menu_matches_with(&draft, self.agent(), &self.agent_commands());
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
        let Some(command) = menu.items.get(menu.selected).cloned() else {
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

    /// Push a composed message at the agent. `false` = it did not go out and
    /// the caller keeps its draft.
    ///
    /// EXP-746: a LOCAL session has no publisher in front of its composer, so
    /// a `/` command has to be recognised here and handed to the engine as a
    /// command — a remote one is recognised by the publisher instead, which is
    /// why the wire keeps carrying commands as ordinary `input` text.
    fn deliver(&self, message: &str) -> bool {
        let Some(session) = self.source.steerable_session() else {
            return self
                .source
                .handle()
                .is_some_and(|handle| handle.send_message(message));
        };
        match parse_command(message, self.agent()) {
            Some(parsed) => session.run_command(parsed.command.name, &parsed.args),
            None => session.send_prompt(message.to_string()),
        }
        true
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
        let description = if self.is_local() {
            LOCAL_KILL_DESCRIPTION.to_string()
        } else {
            kill_description(label.as_deref())
        };
        let spec = AlertSpec::new("Kill this coding session?", description, "Kill session")
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
        // EXP-746: a run THIS process hosts is killed in process — going out
        // to `steer.killSession` and waiting for the row to sync back would
        // take the long way round to our own engine.
        if let Some(session) = self.source.steerable_session() {
            session.kill("killed");
            return;
        }
        kill_session(&self.session_id, cx);
    }
}

/// EXP-746: what an ended session's transcript says about itself. The
/// remote wording ("The session has ended.") is the publisher's `bye`
/// outcome when it sent one; this is the fallback every source shares.
pub(crate) const ENDED_BANNER: &str = "Read-only — this session has ended";

/// A Past row opened off its recorded transcript.
pub(crate) const REPLAY_BANNER: &str = "Replaying transcript…";

/// …and the same row when the run left nothing to replay (it ran on the
/// terminal transport, or its workspace is gone).
pub(crate) const REPLAY_EMPTY_BANNER: &str = "No transcript for this run";

/// EXP-746: the confirm body for a run hosted IN this app. The remote copy
/// talks about "the terminal", which an ACP run does not have — and the
/// worktree promise is the part that matters either way.
pub(crate) const LOCAL_KILL_DESCRIPTION: &str =
    "The agent stops immediately and the session ends. Uncommitted work in the worktree is kept.";


/// The confirm copy, byte-identical to the web `useKillSession` dialog.
pub(crate) fn kill_description(device_label: Option<&str>) -> String {
    let on_device = match device_label {
        Some(label) if !label.is_empty() => format!(" on {label}"),
        _ => String::new(),
    };
    format!(
        "This stops the agent{on_device} and ends the session. \
         Uncommitted work in the worktree is kept, but the agent stops immediately."
    )
}

/// `steer.killSession` off the gpui foreground. Shared with the Devices
/// screen's per-row kill.
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

/// What an engine phase means to this view. EXP-746 review UI-2: BOTH the
/// replayed edge and the seed taken on attach go through it, so a reopened tab
/// over a long-running session cannot end up in a different state than one
/// that watched the run from the start.
pub(crate) fn viewer_phase(phase: engine::EnginePhase) -> ViewerPhase {
    match phase {
        engine::EnginePhase::Connecting => ViewerPhase::Connecting,
        engine::EnginePhase::Live => ViewerPhase::Live,
        // EXP-758: a failed run is ended, and says WHY. The same text the
        // host's `EngineExit::error` produces, so a view that only ever saw
        // the feed reads exactly like one the host told directly.
        engine::EnginePhase::Failed(error) => ViewerPhase::Ended {
            outcome: Some(failure_banner(&error)),
        },
        engine::EnginePhase::Ended => ViewerPhase::Ended { outcome: None },
    }
}

/// The longest a failure reason may run in the banner. One line: a stderr
/// dump behind a `Failed` phase would push the composer off the screen.
const FAILURE_BANNER_MAX: usize = 200;

/// EXP-758: the banner an ACP run that DIED renders in place of the plain
/// [`ENDED_BANNER`]. Pure so the shaping (one line, capped) is unit-tested;
/// an empty/blank reason degrades to the plain ended wording rather than to
/// "Session failed: ".
pub(crate) fn failure_banner(error: &str) -> String {
    let flattened = error.split_whitespace().collect::<Vec<_>>().join(" ");
    if flattened.is_empty() {
        return ENDED_BANNER.to_string();
    }
    format!(
        "Session failed: {}",
        steer::truncate(&flattened, FAILURE_BANNER_MAX)
    )
}

/// EXP-758: the engine emits `Failed(reason)` and THEN `Ended`, and the feed
/// closing can add a third plain end after both. A later, less specific end
/// must never erase the reason the first one carried.
pub(crate) fn merge_ended_phase(current: &ViewerPhase, next: ViewerPhase) -> ViewerPhase {
    match (current, &next) {
        (ViewerPhase::Ended { outcome: Some(_) }, ViewerPhase::Ended { outcome: None }) => {
            current.clone()
        }
        _ => next,
    }
}

/// FEED-26 — the cheap "did the feed move?" probe behind the stale-activity
/// clock. Ids are monotonic, so an APPEND always changes the tail's id and a
/// splice its length; a REPLACED last row (a question resolving, a subagent
/// marker updating) changes the item itself. One tail clone per event costs
/// less than the event that produced it, and a mid-feed edit is deliberately
/// out of scope: it can only make the caption appear a beat early, never
/// hide a genuinely stalled run.
pub(crate) type FeedPulse = (usize, Option<FeedItem>);

pub(crate) fn feed_pulse(feed: &SteerFeed) -> FeedPulse {
    (feed.len(), feed.items().last().cloned())
}

/// The header/tooltip caption for a phase, mirroring the web `phaseLabel`.
///
/// FEED-26: `stale_minutes` is the whole minutes a LIVE run's feed has been
/// quiet past [`STALE_ACTIVITY_AFTER`] — `None` for every other state, so
/// the caller owns the "is this quiet meaningful?" question (not live,
/// paused, awaiting an answer or compacting all answer no) and this stays a
/// pure formatter.
pub(crate) fn phase_label(
    phase: &ViewerPhase,
    device: Option<&str>,
    awaiting_input: bool,
    paused: bool,
    stale_minutes: Option<u64>,
) -> String {
    if paused {
        return format!("Paused · {} is offline", device.unwrap_or("device"));
    }
    match phase {
        ViewerPhase::Live => {
            let head = if awaiting_input {
                "Needs your input".to_string()
            } else if let Some(minutes) = stale_minutes {
                format!("No activity for {minutes} min")
            } else {
                "Live".to_string()
            };
            match device {
                Some(device) => format!("{head} · {device}"),
                None => head,
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
        let awaiting = !self.active.is_empty();
        let stale = self.stale_minutes(paused, awaiting);
        let caption = phase_label(
            &self.phase,
            device.label.as_deref(),
            awaiting,
            paused,
            stale,
        );
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
            .child(status_dot(self.phase_tone(cx, paused, awaiting, stale.is_some())))
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

    fn phase_tone(&self, cx: &App, paused: bool, awaiting: bool, stale: bool) -> gpui::Hsla {
        if paused || self.row_ended() {
            return cx.theme().muted_foreground.opacity(0.5);
        }
        // FEED-26: a live run whose feed has gone quiet wears the SAME steady
        // amber as "Needs your input" — both mean "this is not progressing on
        // its own", and a green dot over a stalled agent is the lie the issue
        // is about.
        if awaiting || stale {
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

        // EXP-776: the virtualised transcript. The list holds `row_keys.len()`
        // items ([`Self::sync_list`] ran at the top of this frame) and asks
        // for each one it paints by index; the padding is the old column's,
        // honoured by the list as its own (`last_padding`).
        let list = list(
            self.list.clone(),
            cx.processor(|this, ix: usize, window, cx| this.render_list_row(ix, window, cx)),
        )
        .px_3()
        .py_2();
        crate::scroll_pane::v_list_pane(list, &self.list).into_any_element()
    }

    /// One transcript row by list index: the cached spec resolved against
    /// the feed and rendered exactly as before, or — past the last spec —
    /// the synthetic "Working…" line. Each row wears the 2px bottom gap the
    /// old column's `gap_0p5` gave it, so the row rhythm is unchanged.
    fn render_list_row(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let Some(spec) = self.rows.get(ix) else {
            return div()
                .w_full()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .py_1p5()
                        .child(
                            Icon::new(registry::CODING_ASSISTANT)
                                .xsmall()
                                .text_color(muted.opacity(0.6)),
                        )
                        .child(div().text_xs().text_color(muted).child("Working…")),
                )
                .into_any_element();
        };
        let live = self.phase == ViewerPhase::Live;
        let last_row = self.rows.len().saturating_sub(1);
        let row = spec.resolve(self.feed.items());
        let element = self.render_row(&row, ix == last_row && live, &self.active, window, cx);
        div()
            .w_full()
            .min_w_0()
            .pb_0p5()
            .child(element)
            .into_any_element()
    }

    fn render_row(
        &self,
        row: &FeedRow<'_>,
        live_tail: bool,
        active: &HashSet<FeedItemId>,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        match row {
            FeedRow::Single(item) => self.render_item(item, active, window, cx),
            FeedRow::ToolRun { id, items } => self.render_tool_run(*id, items, live_tail, cx),
            FeedRow::Ask { id, items, .. } => self.render_ask(*id, items, active, window, cx),
            FeedRow::Subagent { id, items, .. } => {
                self.render_subagent(*id, items, window, cx)
            }
        }
    }

    fn render_item(
        &self,
        item: &FeedItem,
        active: &HashSet<FeedItemId>,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        match &item.kind {
            FeedKind::Narration { text, .. } => h_flex()
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
                        self.with_issue_chips(
                            crate::markdown::MarkdownView::new(
                                SharedString::from(format!("steer-narration-{}", item.id)),
                                text.clone(),
                            )
                            // EXP-698: the feed reads at the chat rhythm, and
                            // its inline code takes the semantic tint.
                            .chat(true)
                            .selectable(true),
                            cx,
                        ),
                    ),
                )
                .into_any_element(),
            // EXP-724: a message that IS a catalog command reads as a
            // compact pill, not a chat bubble — the agent was steered, not
            // spoken to.
            FeedKind::UserMessage { text, .. }
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
                        crate::surface::glass_pill(
                            SharedString::from(format!("steer-command-{}", item.id)),
                            crate::surface::PillSize::Sm,
                            crate::surface::PillMode::Readonly,
                            cx,
                        )
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
            FeedKind::UserMessage { text, .. } => h_flex()
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
            } => {
                let row = tool_row(name, detail.as_deref(), cx);
                // EXP-746: a LOCAL run's per-edit diff and command output hang
                // off this row (a remote one has none — the wire carries
                // neither).
                match self.render_extras(item.id, cx) {
                    Some(extras) => v_flex()
                        .w_full()
                        .min_w_0()
                        .child(row)
                        .child(extras)
                        .into_any_element(),
                    None => row.into_any_element(),
                }
            }
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
            FeedKind::Subagent { .. } => self.render_subagent(item.id, &[item], window, cx),
            FeedKind::Question(_) => self.render_question(item, active, window, cx),
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
                    self.with_issue_chips(
                        crate::markdown::MarkdownView::new(
                            SharedString::from(format!("steer-msg-images-{id}")),
                            embeds,
                        )
                        .selectable(true),
                        cx,
                    ),
                )
            })
            .into_any_element()
    }

    /// A body that folds behind "Show more" once it runs long.
    fn render_body(&self, id: FeedItemId, text: &str, cx: &mut gpui::Context<Self>) -> AnyElement {
        self.render_body_folding(id, text, true, cx)
    }

    /// EXP-746 — the local cards hanging off feed row `item` (per-edit
    /// diffs, command output). `None` for every remote row: they exist only
    /// where the engine runs.
    ///
    /// The fold flag is this view's, the rendering is
    /// [`crate::session_extras`]' — the same split every other card here
    /// uses, so the transcript owns interaction and the extras own shape.
    fn render_extras(&self, item: FeedItemId, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        if !self.extras.has_extras(item) {
            return None;
        }
        // EXP-750: the Stop on a live terminal card goes straight to the
        // in-process engine — a remote viewer and a replay have no terminal
        // to stop, so they get no Running/Stop strip at all rather than a
        // button whose click goes nowhere.
        let session = self.source.steerable_session().cloned();
        crate::session_extras::render_extras(
            &self.extras,
            item,
            self.expanded_extras.contains(&item),
            Box::new(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                if !this.expanded_extras.insert(item) {
                    this.expanded_extras.remove(&item);
                }
                cx.notify();
            })),
            session.map(|session| -> Box<dyn Fn(&str, &mut Window, &mut App) + 'static> {
                Box::new(move |terminal_id: &str, _window: &mut Window, _cx: &mut App| {
                    session.kill_terminal(terminal_id);
                })
            }),
            cx,
        )
    }

    /// A body that never folds: the plan a reader must approve is always
    /// shown in full (EXP-738, parity with iOS/Android).
    fn render_unfolded_body(
        &self,
        id: FeedItemId,
        text: &str,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        self.render_body_folding(id, text, false, cx)
    }

    fn render_body_folding(
        &self,
        id: FeedItemId,
        text: &str,
        fold: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        // EXP-698: every body this renders is a CHAT body — the user bubble,
        // the plan card, the ask card, a stepper step — so the rhythm and the
        // code tint are set once, here.
        let view = self.with_issue_chips(
            crate::markdown::MarkdownView::new(
                SharedString::from(format!("steer-body-{id}")),
                text.to_string(),
            )
            .chat(true)
            .selectable(true),
            cx,
        );
        if !fold || !clampable(text) {
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
                    // EXP-746: an expanded group shows each call's local
                    // cards too — that is what expanding it is for.
                    if let Some(extras) = self.render_extras(item.id, cx) {
                        column = column.child(div().pl_5().child(extras));
                    }
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
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let summary = summarize_subagent_row(items);
        // EXP-773: the group's BODY is everything the subagent produced —
        // its prose and the turns addressed to it as well as its tool calls,
        // in feed order. Only the lifecycle markers stay in the header.
        let body: Vec<&&FeedItem> = items
            .iter()
            .filter(|item| !matches!(item.kind, FeedKind::Subagent { .. }))
            .collect();
        let expandable = !body.is_empty();
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
            for item in body {
                match &item.kind {
                    FeedKind::Tool { name, detail, .. } => {
                        column = column
                            .child(div().pl_5().child(tool_row(name, detail.as_deref(), cx)));
                        // EXP-746: an expanded group shows each call's local
                        // cards too — that is what expanding it is for.
                        if let Some(extras) = self.render_extras(item.id, cx) {
                            column = column.child(div().pl_5().child(extras));
                        }
                    }
                    // EXP-773: the subagent's own prose and the turns sent to
                    // it read exactly as they do on the main line, indented
                    // under the group instead of splitting it.
                    FeedKind::Narration { .. } | FeedKind::UserMessage { .. } => {
                        column = column.child(
                            div()
                                .pl_5()
                                .child(self.render_item(item, &HashSet::new(), window, cx)),
                        );
                    }
                    _ => {}
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
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let amber = theme::tokens::YELLOW.to_hsla();
        let answered: Vec<&&FeedItem> = items
            .iter()
            .filter(|item| {
                item.question().is_some_and(|card| card.resolved) || self.is_answer_locked(item)
            })
            .collect();
        let current = items.iter().find(|item| {
            !item.question().is_some_and(|card| card.resolved) && !self.is_answer_locked(item)
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
                    .child(self.render_prompt(item, active, submit_step, window, cx));
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
                answer_key(item)
                    .and_then(|key| self.feed.answer_state(&key))
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
        window: &Window,
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
                    .child(if card.plan_mode {
                        self.render_unfolded_body(item.id, &card.text, cx)
                    } else {
                        self.render_body(item.id, &card.text, cx)
                    }),
            )
            .child(self.render_prompt(item, active, false, window, cx))
            .into_any_element()
    }

    /// The interactive half of a card: options, multi-select submit, the
    /// free-text row, the lock and the resolution line.
    fn render_prompt(
        &self,
        item: &FeedItem,
        active: &HashSet<FeedItemId>,
        submit_step: bool,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let Some(card) = item.question() else {
            return div().into_any_element();
        };
        let key = answer_key(item);
        let state = key.as_deref().and_then(|key| self.feed.answer_state(key));
        let locked = state.is_some_and(|state| state.is_locked());
        let errored = state.is_some_and(|state| state.status == AnswerStatus::Error);

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

        // Past the read-only gate a wire id is guaranteed: an id-less card is
        // never active (EXP-730).
        let Some(key) = key else {
            return div().into_any_element();
        };
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
                    .label("Answer")
                    .disabled(keys.is_empty())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        this.answer(item_id, keys.clone(), labels.clone(), None, cx);
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
                    .child(
                        div().flex_1().min_w_0().child(glass_input(&self.free_text_input, window, cx)),
                    )
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
        if errored {
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
                if free_text {
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
        // EXP-746: a replay says what it is BEFORE anything else — its feed
        // is history, and every other banner would read as live state. A run
        // that ended with NOTHING to show says why: its transcript is not on
        // this machine (it ran on the terminal transport, or elsewhere), and
        // an ended session's relay room is gone.
        // Once the replay has ended, the ended banner below says it all.
        if self.source.read_only() && !matches!(self.phase, ViewerPhase::Ended { .. }) {
            banners.push(banner(REPLAY_BANNER.to_string()));
        } else if self.feed.is_empty() && !self.is_local() && self.session_over() {
            banners.push(banner(REPLAY_EMPTY_BANNER.to_string()));
        }
        match &self.phase {
            ViewerPhase::Ended { outcome } => banners.push(banner(
                outcome
                    .clone()
                    .unwrap_or_else(|| ENDED_BANNER.to_string()),
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
        let mode = self.render_mode_control(cx);
        let composer = crate::composer::GlassComposer::new(
            v_flex()
                .w_full()
                .min_w_0()
                // EXP-772: the ONE live control — the session mode — sits
                // above the draft, where the agent's posture is visible while
                // typing at it.
                .when_some(mode, |this, mode| this.child(mode))
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

    /// EXP-772 — the composer's ONE live control: the session MODE.
    ///
    /// Model, effort and every other option picker left the mid-session UI (an
    /// agent is configured when it starts, and the only thing worth flipping
    /// mid-run is plan on/off). The claude/pi shape — exactly two modes, one
    /// of them `plan` — draws a compact "Plan" toggle pill; any other mode
    /// list falls back to a two-value chip, and a run with no modes (codex)
    /// draws nothing.
    ///
    /// Fire-and-forget (D4): the pill repaints when the publisher's next
    /// `config_state` lands, so there is no optimistic value and no spinner.
    fn render_mode_control(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let muted = cx.theme().muted_foreground;
        if let Some(toggle) = self.plan_toggle() {
            let target = if toggle.active {
                toggle.build_id.clone()
            } else {
                toggle.plan_id.clone()
            };
            return Some(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_1()
                    .pb_1()
                    .child(
                        crate::surface::glass_pill(
                            "steer-plan-toggle",
                            crate::surface::PillSize::Sm,
                            crate::surface::PillMode::Select {
                                selected: toggle.active,
                            },
                            cx,
                        )
                        .tooltip(|window, cx| {
                            gpui_component::tooltip::Tooltip::new("Plan mode").build(window, cx)
                        })
                        .child(div().text_xs().child("Plan"))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            this.set_mode(&target);
                            cx.notify();
                        })),
                    )
                    .into_any_element(),
            );
        }
        let chip = self.mode_chip()?;
        let label = SharedString::from(format!("{}: {}", chip.label, chip.value_label));
        if chip.values.len() < 2 {
            return Some(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_1()
                    .pb_1()
                    .child(
                        crate::surface::glass_pill(
                            "steer-mode-chip",
                            crate::surface::PillSize::Sm,
                            crate::surface::PillMode::Readonly,
                            cx,
                        )
                        .child(div().text_xs().text_color(muted).child(label)),
                    )
                    .into_any_element(),
            );
        }
        // The popover and its trigger are two elements in one subtree — gpui
        // ids must not collide.
        let trigger = crate::surface::glass_pill_button(
            "steer-mode-chip-trigger",
            crate::surface::PillSize::Sm,
            cx,
        )
        .label(label);
        let values = chip.values.clone();
        let current = chip.value.clone();
        let view = cx.entity().downgrade();
        Some(
            h_flex()
                .w_full()
                .min_w_0()
                .gap_1()
                .pb_1()
                .child(
                    gpui_component::popover::Popover::new("steer-mode-chip")
                        .p_1()
                        .trigger(trigger)
                        .content(move |_, _window, cx| {
                            let mut menu = v_flex().min_w(px(160.)).gap_0p5();
                            for value in &values {
                                let view = view.clone();
                                let value_id = value.id.clone();
                                let picked = value_id == current;
                                menu = menu.child(
                                    h_flex()
                                        .id(SharedString::from(format!(
                                            "steer-mode-value-{value_id}"
                                        )))
                                        .w_full()
                                        .gap_2()
                                        .items_center()
                                        .px_2()
                                        .py_1()
                                        .rounded(px(theme::tokens::radius::SM))
                                        .cursor_pointer()
                                        .text_xs()
                                        .hover(|this| this.bg(cx.theme().accent))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .child(SharedString::from(value.label.clone())),
                                        )
                                        // The mode in force, marked the way
                                        // the web menu marks it.
                                        .when(picked, |this| {
                                            this.child(
                                                Icon::new(registry::UI_CHECK)
                                                    .xsmall()
                                                    .text_color(theme::tokens::GREEN.to_hsla()),
                                            )
                                        })
                                        .on_click(move |_: &ClickEvent, _window, cx| {
                                            let Some(view) = view.upgrade() else {
                                                return;
                                            };
                                            view.update(cx, |this, cx| {
                                                this.set_mode(&value_id);
                                                cx.notify();
                                            });
                                        }),
                                );
                            }
                            menu
                        }),
                )
                .into_any_element(),
        )
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
        // EXP-776: the transcript list learns what changed since the last
        // frame here, before anything reads the cached projection.
        self.sync_list(cx);
        let header = self.chrome.then(|| self.render_header(cx));
        let feed = self.render_feed(cx);
        let banners = self.render_banners(cx);
        let composer_visible = self.composer_visible();
        // EXP-724: between the banners and the composer, exactly where the
        // web view puts it — and gone with the composer once the run ends.
        let compacting = (composer_visible && self.feed.compacting().is_some())
            .then(|| self.render_compaction_strip(cx));
        // EXP-746: the local pinned state — the plan the agent is working
        // through, and its latest thought. Both are STATE (replaced, not
        // appended), which is why they sit above the composer instead of
        // scrolling away in the feed.
        let plan = (!self.extras.plan().is_empty())
            .then(|| crate::session_extras::render_plan_card(self.extras.plan(), cx));
        let thought = self
            .extras
            .thought()
            .filter(|_| composer_visible)
            .map(|text| crate::session_extras::render_thought(text, cx));
        let composer = composer_visible.then(|| self.render_composer(cx));
        // EXP-773: the Latest-changes bar sits between the transcript and the
        // composer, exactly where the web view puts it — an ended run keeps
        // it (the work is what the reader came for), it only loses the Merge.
        let changes = self.render_changes_bar(cx);
        v_flex()
            .key_context("SteerSession")
            .track_focus(&self.focus_handle)
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .children(header)
            .child(feed)
            .children(banners)
            .children(plan)
            .children(thought)
            .children(compacting)
            .children(changes)
            .children(composer)
    }
}

impl Drop for SteerSessionView {
    fn drop(&mut self) {
        if let Some(handle) = self.source.handle() {
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
            phase_label(&ViewerPhase::Live, Some("macbook"), false, false, None),
            "Live · macbook"
        );
        assert_eq!(
            phase_label(&ViewerPhase::Live, None, false, false, None),
            "Live"
        );
        assert_eq!(
            phase_label(&ViewerPhase::Live, Some("macbook"), true, false, None),
            "Needs your input · macbook"
        );
        assert_eq!(
            phase_label(&ViewerPhase::Starting, Some("macbook"), false, false, None),
            "Agent starting…"
        );
        assert_eq!(
            phase_label(&ViewerPhase::Ended { outcome: None }, None, false, false, None),
            "Session ended"
        );
        // FEED-26: a live run whose feed has gone quiet says so, with the
        // same ` · {device}` suffix every other live caption carries.
        assert_eq!(
            phase_label(&ViewerPhase::Live, Some("macbook"), false, false, Some(27)),
            "No activity for 27 min · macbook"
        );
        assert_eq!(
            phase_label(&ViewerPhase::Live, None, false, false, Some(10)),
            "No activity for 10 min"
        );
        // …and a card waiting for an answer still wins: the run is not stuck,
        // the reader is.
        assert_eq!(
            phase_label(&ViewerPhase::Live, Some("macbook"), true, false, Some(27)),
            "Needs your input · macbook"
        );
        // Quiet only means anything while LIVE.
        assert_eq!(
            phase_label(&ViewerPhase::Starting, None, false, false, Some(27)),
            "Agent starting…"
        );
        assert_eq!(
            phase_label(&ViewerPhase::Live, Some("macbook"), false, true, Some(27)),
            "Paused · macbook is offline"
        );
    }

    /// FEED-26: the threshold is shared byte-for-byte with the other three
    /// clients — 10 minutes, and the caption counts WHOLE minutes.
    #[test]
    fn the_stale_activity_window_is_ten_minutes() {
        assert_eq!(STALE_ACTIVITY_AFTER, Duration::from_secs(600));
        assert!(STALE_TICK < STALE_ACTIVITY_AFTER);
    }

    /// A paused host wins over every other phase — the run is not gone, the
    /// machine is (EXP-550).
    #[test]
    fn a_paused_host_beats_the_phase() {
        assert_eq!(
            phase_label(&ViewerPhase::Live, Some("macbook"), true, true, None),
            "Paused · macbook is offline"
        );
        assert_eq!(
            phase_label(&ViewerPhase::Live, None, false, true, None),
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
            "This stops the agent on macbook and ends the session."
        ));
        assert!(kill_description(None)
            .starts_with("This stops the agent and ends the session."));
        assert!(kill_description(Some("")).starts_with(
            "This stops the agent and ends the session."
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

    /// EXP-746 review UI-2: attaching to a live engine seeds the phase through
    /// this, and `can_send` compares against `ViewerPhase::Live` — so a tab
    /// reopened over a long-running session has a live composer on its first
    /// paint, without waiting for a replayed edge that a full backlog may have
    /// evicted (`engine::LocalFeed` replays the latest phase for that too).
    #[test]
    fn an_engine_phase_becomes_the_matching_viewer_phase() {
        assert_eq!(
            viewer_phase(engine::EnginePhase::Connecting),
            ViewerPhase::Connecting
        );
        assert_eq!(viewer_phase(engine::EnginePhase::Live), ViewerPhase::Live);
        assert!(matches!(
            viewer_phase(engine::EnginePhase::Ended),
            ViewerPhase::Ended { outcome: None }
        ));
    }

    /// EXP-758: a run that died on its handshake says WHY, on one capped
    /// line. The old behaviour was an empty tab whose only word was
    /// "ended".
    #[test]
    fn a_failure_reason_becomes_a_one_line_banner() {
        assert_eq!(
            failure_banner("codex app-server exited: status 127"),
            "Session failed: codex app-server exited: status 127"
        );
        // Multi-line stderr collapses to one line.
        assert_eq!(
            failure_banner("initialize failed\n  caused by: broken pipe\n"),
            "Session failed: initialize failed caused by: broken pipe"
        );
        // …and a long one is capped.
        let long = "x".repeat(FAILURE_BANNER_MAX * 2);
        assert_eq!(
            failure_banner(&long).len(),
            "Session failed: ".len() + FAILURE_BANNER_MAX
        );
        // A blank reason is no reason: the plain ended wording, never
        // "Session failed: ".
        assert_eq!(failure_banner("   \n "), ENDED_BANNER);
        // The phase mapping carries it.
        assert_eq!(
            viewer_phase(engine::EnginePhase::Failed("boom".to_string())),
            ViewerPhase::Ended {
                outcome: Some("Session failed: boom".to_string())
            }
        );
    }

    /// EXP-758: `Failed` precedes `Ended`, and the feed closing can add a
    /// third plain end after both. Only the FIRST reason survives.
    #[test]
    fn a_later_plain_end_never_erases_the_failure_reason() {
        let failed = ViewerPhase::Ended {
            outcome: Some("Session failed: boom".to_string()),
        };
        let plain = ViewerPhase::Ended { outcome: None };
        assert_eq!(merge_ended_phase(&failed, plain.clone()), failed);
        // A plain end still records itself when nothing better is there…
        assert_eq!(
            merge_ended_phase(&ViewerPhase::Live, plain.clone()),
            plain.clone()
        );
        // …and a reason still lands on an end that had none.
        assert_eq!(merge_ended_phase(&plain, failed.clone()), failed);
        // Every other phase is a plain overwrite.
        assert_eq!(
            merge_ended_phase(&ViewerPhase::Connecting, ViewerPhase::Live),
            ViewerPhase::Live
        );
    }
}
