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
//! EXP-698 closed the two biggest gaps: the pinned **Changes** strip
//! and the in-session **Merge** pill render for a steered session too. The
//! old rationale ("a remote run's diff is not on this machine") was wrong —
//! the host publishes its worktree diff on the activity channel and
//! [`SteerFeed::latest_diff`] holds it. The bar itself lives in
//! [`crate::changes_bar`]; the session screen's "Changes" rail draws it from
//! [`Self::latest_diff`] and resolves the merge target off the synced
//! `coding_sessions` row.
//!
//! EXP-789 closed the last parity gap: the subagent TAB strip ("Main" plus
//! one tab per running subagent, the focused one lingering after it ends)
//! sits between the header and the feed, and a focused tab projects only
//! that subagent's rows ([`steer::feed::group_subagent_row_specs_into`]).
//! EXP-788 made the options a numbered list the keyboard drives (digits,
//! ↑/↓, Enter); EXP-790 made the field mention-capable and the send button a
//! Send/Stop toggle. EXP-820 settled the answering model: the composer HIDES
//! while a card is pending, free text ("Type something." / a plan's "keep
//! planning") is typed INLINE in that option row, and an answered step of a
//! multi-question ask is re-openable until the ask completes
//! ([`ask_complete`]). The only deliberate gap left: no fullscreen toggle —
//! the screen's own chrome is the desktop's answer to that.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    bounce, div, ease_in_out, list, prelude::FluentBuilder as _, px, relative,
    AnimationExt as _, AnyElement, App, AppContext as _, ClickEvent, Entity, FocusHandle,
    Focusable, FollowMode, InteractiveElement as _, IntoElement, ListAlignment, ListState,
    ParentElement as _, Pixels, Render, SharedString, StatefulInteractiveElement as _,
    Styled, Subscription, Task, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{self, InputEvent, InputState, TextareaState},
    spinner::Spinner,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use steer::activity::SessionAgent;
use steer::commands::parse_command;
use steer::feed::{COMPACTED_LABEL, COMPACTING_LABEL, COMPACTION_TIMEOUT};
use steer::{
    answer_key, build_steer_image_message, parse_steer_message,
    summarize_subagent_row, transcript_gap, AnswerStatus, FeedItem,
    FeedItemId, FeedKind, FeedRow, FeedRowSpec, Gap, QuestionOption, RowClass, SteerFeed,
    SubagentStatus, ViewerEvent, ViewerHandle, ViewerPhase, ANSWER_ACK_TIMEOUT,
    REPLAY_MAX, REPLAY_QUIET,
};
use theme::tokens::transcript;

use crate::controls::WebText as _;
use crate::icons::registry;
use crate::slash_commands;
use crate::composer_images::{self, PendingImages};
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

/// EXP-783 — how many of the feed's newest items the transcript projects into
/// rows. The feed itself keeps the WHOLE run; this is what makes keeping it
/// free, because every per-frame pass ([`SteerSessionView::sync_list`], the
/// list's own bookkeeping) is over the window and not over the run. The
/// contract's `steerFeed.window` — web, iOS and Android render the same.
const FEED_WINDOW: usize = domain::contract::STEER_FEED_WINDOW;

/// How much older transcript one "scrolled to the top" extension pulls in.
const FEED_WINDOW_STEP: usize = domain::contract::STEER_FEED_WINDOW_STEP;

/// EXP-783 — events per `history_page` ask: the contract's
/// `steerFeed.historyPageMax`, which the relay's schema enforces.
const HISTORY_PAGE_LIMIT: u32 = domain::contract::STEER_FEED_HISTORY_PAGE_MAX;

/// EXP-795 — the window anchor that means "the feed's first row, whatever it
/// is". Set when the reader asks for a page the feed does not hold yet, so a
/// prepended page is inside the window the moment it lands; below every real
/// id because ids start at [`steer::feed::FEED_ID_BASE`] and only count down
/// at the front by a finite amount.
const WINDOW_FROM_START: FeedItemId = 0;

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
/// EXP-724: the `/` command menu over the composer — the catalog entries
/// matching the typed prefix, and the highlighted one.
struct SlashMenu {
    items: Vec<slash_commands::MenuCommand>,
    selected: usize,
}

/// EXP-820: the inline free-text field open on ONE option row — a
/// question's "Type something." row or a plan card's "keep planning" row.
/// Created when the row is activated, dropped on send or Escape; the
/// composer is hidden while its card is pending, so this IS the typing
/// surface for the card.
struct InlineAnswer {
    item: FeedItemId,
    option_key: String,
    state: Entity<InputState>,
    _subscription: Subscription,
}

/// The inline field's hint on a question's free-text row.
const FREE_TEXT_PLACEHOLDER: &str = "Type your answer…";
/// …and on a plan card's reject row, where the text is the change request.
const PLAN_REJECT_PLACEHOLDER: &str = "Tell the agent what to change…";

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
    /// EXP-790: the `@`-member / `#`-issue / `:`-emoji completion layered on
    /// `input` — the same widget the comment composer types into. Its source
    /// follows the run's team ([`Self::mention_team`]); a team-less run keeps
    /// the widget and gets plain-input behaviour.
    mention: Entity<crate::mention_input::MentionInput>,
    /// The team the completion source was last pointed at.
    mention_team: Option<String>,
    /// EXP-790: the composer card's measured width, for the tool row's
    /// wrap decision ([`tool_row_wraps`]).
    composer_width: std::rc::Rc<std::cell::Cell<Pixels>>,
    /// …and whether the tool row is on its own line right now (the
    /// hysteresis state).
    tools_wrapped: std::cell::Cell<bool>,
    /// EXP-820: the inline free-text field, open on at most one option row.
    inline: Option<InlineAnswer>,
    /// EXP-820: the answered step of a multi-question ask the reader
    /// re-opened. Honoured only while that ask is still open
    /// ([`Self::editing_card_id`]).
    editing_step: Option<FeedItemId>,
    /// EXP-820: whether the composer rendered LAST frame. Its hide edge (a
    /// card became pending) moves focus onto this view so the card keyboard
    /// keeps working; its show edge hands focus back to the field.
    composer_shown: bool,
    /// EXP-789: the subagent tab in focus (`None` = Main). Only honoured
    /// while that subagent's tab is visible ([`Self::active_subagent`]).
    focused_subagent: Option<String>,
    /// EXP-788: the option the keyboard has highlighted on the pending card
    /// (↑/↓), and which card it was highlighted on — a new card starts with
    /// nothing highlighted.
    answer_cursor: Option<usize>,
    answer_cursor_for: Option<FeedItemId>,
    /// EXP-724: the open `/` menu, refreshed on every draft change.
    slash: Option<SlashMenu>,
    /// The exact draft Escape dismissed the menu for — it stays shut until
    /// the draft changes again (and after an accept, so inserting `/clear`
    /// does not immediately re-open the menu on its own result).
    slash_dismissed_for: Option<String>,
    /// EXP-698/EXP-825: the staged images (the shared composer strip).
    pending_images: PendingImages,
    sending: bool,
    notice: Option<SharedString>,
    /// Question-card local state, keyed by `answer_key`.
    picked: HashMap<String, Vec<String>>,
    /// Expanded tool-run / subagent group rows, and expanded long bodies.
    expanded_groups: HashSet<FeedItemId>,
    expanded_bodies: HashSet<FeedItemId>,
    /// EXP-746: the local-only cards (per-edit diffs, command output, the
    /// pinned plan, thoughts) a `Local`/`Replay` source produces. Empty for
    /// every remote session — the wire carries none of it.
    extras: crate::session_extras::LocalExtras,
    /// EXP-773: the "Changes" bar's parse of the published worktree
    /// diff, and the per-file list its expanded half renders into. It lives
    /// HERE rather than on the hosting screen because the bar sits between the
    /// transcript and the composer (web `agent-session` parity), and both of
    /// those are this view's.
    changes: Option<crate::changes_bar::ChangesSnapshot>,
    changes_diff: Entity<crate::diff::DiffView>,
    /// The extras cards expanded on a row (their own set: a folded tool BODY
    /// and a folded diff are different questions about the same row).
    expanded_extras: HashSet<FeedItemId>,
    /// The oldest feed item id this view has already pruned its per-row state
    /// against. `steer::feed::trim` evicts past `FEED_BYTE_CAP`, and the maps
    /// keyed off a feed row (`expanded_*`, `picked`, `extras`) have no other
    /// signal that a row is gone; without this a long run accumulates them for
    /// its whole life. Seeded at 0, which is below every real id.
    pruned_before: FeedItemId,
    /// EXP-783: the oldest feed item the transcript currently RENDERS —
    /// `None` = the newest [`FEED_WINDOW`] items, [`WINDOW_FROM_START`] = the
    /// feed's own first row. An item id rather than an index, so an eviction
    /// or a replay swap cannot silently move the reader's window somewhere
    /// else.
    window_from: Option<FeedItemId>,
    /// Set by the row renderer when it paints row 0 and there is older
    /// transcript above it: the next [`Self::sync_list`] extends the window by
    /// [`FEED_WINDOW_STEP`]. The extension is a `0..0` front splice, which
    /// gpui rebases the scroll anchor across, so the reader stays put and row
    /// 0 leaves the viewport — endless scroll that terminates on its own.
    extend_window: bool,
    /// EXP-783: the `history_page` request in flight, if any — a chunk for
    /// any other id is not ours.
    history_request: Option<String>,
    /// The relay said its replay log is a TAIL, so there IS older transcript
    /// to ask the device for.
    history_truncated: bool,
    /// A page came back with nothing new: this run has no more history, so
    /// the window extension stops asking.
    history_exhausted: bool,
    /// Numbers the request ids, so a late chunk from a superseded ask is
    /// dropped rather than prepended twice.
    history_requests: u64,
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
    /// EXP-780: the bearer-fetch image cache every prose body in the
    /// transcript renders `/api/attachments/{id}` through. Without one the
    /// slot never leaves [`crate::markdown::ImageSlot::Failed`] and a steered
    /// screenshot painted "Image unavailable" — the agent saw it (that path
    /// is MCP), the reader did not.
    images: Entity<crate::markdown::ImageCache>,
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
                .placeholder(COMPOSER_PLACEHOLDER)
        });
        // EXP-790: the completion overlay rides the same state; the composer
        // card draws the chrome, so the widget draws none of its own.
        let mention = cx.new(|cx| {
            let mut mention = crate::mention_input::MentionInput::new(input.clone(), cx);
            mention.set_appearance(false);
            mention
        });

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

        let transport = crate::queries::attachment_transport(cx);
        let image_cache = cx.new(|_| crate::markdown::ImageCache::new(transport));
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
            mention,
            mention_team: None,
            composer_width: std::rc::Rc::new(std::cell::Cell::new(px(0.))),
            tools_wrapped: std::cell::Cell::new(false),
            inline: None,
            editing_step: None,
            // Seeded TRUE: a view built over an already-pending card takes
            // the keyboard on its first frame (the hide edge fires once).
            composer_shown: true,
            focused_subagent: None,
            answer_cursor: None,
            answer_cursor_for: None,
            slash: None,
            slash_dismissed_for: None,
            pending_images: PendingImages::default(),
            sending: false,
            notice: None,
            picked: HashMap::new(),
            images: image_cache,
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
            pruned_before: 0,
            window_from: None,
            extend_window: false,
            history_request: None,
            history_truncated: false,
            history_exhausted: false,
            history_requests: 0,
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

    /// The builtin agent behind this session, for the usage sheet's device
    /// cards (an external agent reports no usage windows of its own).
    pub(crate) fn builtin_agent(&self) -> Option<coding::CodingAgent> {
        match self.agent() {
            SessionAgent::Claude => Some(coding::CodingAgent::Claude),
            SessionAgent::Codex => Some(coding::CodingAgent::Codex),
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
    /// header names the run from, and what the Changes bar resolves
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

    /// EXP-848 — the ONE working predicate (identical on all four clients,
    /// ONE copy per client): the synthetic "Working…" row and the composer's
    /// Stop button both read it.
    ///
    /// `turn_state` is the authority — a LIVE run is working only while the
    /// engine says a turn is in flight, and the slot defaults to `ended`, so
    /// nothing pulses at a run that is merely connected. Everything else is a
    /// veto: an ended row, a question waiting on the reader (the open cards
    /// AND the row's own `needs_input`, FEED-35), a rate-limit wall (a walled
    /// run is stalled, not working) and a compaction (whose strip already says
    /// what is happening).
    ///
    /// Authoritative through the FEED SLOT, never the local engine's
    /// `TurnSignal`: a remote viewer has no engine, and one rule that works
    /// for every `FeedSource` beats two that disagree.
    fn working_now(&self) -> bool {
        is_working(&WorkingFacts {
            empty_feed: self.feed.is_empty(),
            live: self.phase == ViewerPhase::Live,
            row_ended: self.row_ended(),
            turn_working: self.feed.turn_state().is_working(),
            awaiting_input: !self.active.is_empty(),
            needs_input: self.row_needs_input(),
            staging: self.feed.is_staging(),
            blocked: self.rate_limit_wall_showing(chrono::Utc::now().timestamp_millis()),
            compacting: self.feed.compacting().is_some(),
        })
    }

    /// FEED-35: the synced attention flag the HOST wrote — a run parked on a
    /// picker this viewer never received a card for is still waiting.
    fn row_needs_input(&self) -> bool {
        self.row
            .as_ref()
            .and_then(|row| row.needs_input)
            .unwrap_or(false)
    }

    /// The view-side bits of one item that move its rendered height — see
    /// [`transcript_rows::row_fingerprint`].
    fn item_facets(&self, item: &FeedItem) -> ItemFacets {
        let mut facets = 0;
        if self.expanded_bodies.contains(&item.id) {
            facets |= facet::BODY_EXPANDED;
        }
        // EXP-786: a remote row's wire diff is the same fold as a local
        // row's extras — same card, same "Show more".
        let has_extras = self.extras.has_extras(item.id)
            || (self.source.session().is_none()
                && matches!(&item.kind, FeedKind::Tool { diff: Some(_), .. }));
        if has_extras {
            facets |= facet::HAS_EXTRAS;
            if self.expanded_extras.contains(&item.id) {
                facets |= facet::EXTRAS_EXPANDED;
            }
        }
        if let Some(key) = answer_key(item) {
            if self.feed.is_answer_locked(&key) {
                facets |= facet::ANSWER_LOCKED;
            }
            // EXP-788: the keyboard highlight moves the promoted option's
            // paint, not its height — but the picked set does move a
            // multi-select's Submit, so both fold in for a clean re-measure.
            if self.pending_card_id() == Some(item.id) {
                facets |= facet::CARD_PENDING;
            }
            if self.picked.get(&key).is_some_and(|picks| !picks.is_empty()) {
                facets |= facet::PICKS_MADE;
            }
            // EXP-820: the inline field adds a row under one option, and a
            // re-opened step unfolds from one line to its whole prompt.
            if self.inline.as_ref().is_some_and(|inline| inline.item == item.id) {
                facets |= facet::INLINE_OPEN;
            }
            if self.editing_card_id() == Some(item.id) {
                facets |= facet::STEP_EDITING;
            }
        }
        facets
    }

    /// Drop the per-row state of feed items the feed has trimmed away.
    ///
    /// The feed drains its oldest items past `FEED_BYTE_CAP`, but every map
    /// keyed off a feed row lives here and hears nothing about it. Cheap in
    /// the common case: the oldest surviving id only moves once the cap is
    /// full (or when a page is prepended — EXP-795: it moves DOWN then, and a
    /// `<=` guard here would have skipped every eviction until the first id
    /// climbed back past the old high-water mark), so this is a single
    /// integer compare per frame until then.
    fn prune_dropped_rows(&mut self) {
        let Some(first) = self.feed.items().first().map(|item| item.id) else {
            return;
        };
        if first == self.pruned_before {
            return;
        }
        self.pruned_before = first;
        self.expanded_groups.retain(|id| *id >= first);
        self.expanded_bodies.retain(|id| *id >= first);
        self.expanded_extras.retain(|id| *id >= first);
        // `picked` is keyed by `answer_key`, not by row id, so it has to be
        // retained against the question ids still in the feed. Read them
        // borrow-free (`items()` and `picked` are disjoint fields) and without
        // allocating a set: a transcript holds a handful of question cards.
        let items = self.feed.items();
        self.picked.retain(|key, _| {
            items.iter().any(|item| {
                item.question()
                    .is_some_and(|card| card.question_id == *key)
            })
        });
        self.extras.prune_before(first);
    }

    /// EXP-783 — the index of the oldest item the transcript renders.
    ///
    /// The feed keeps the whole run; the window is the newest
    /// [`FEED_WINDOW`] items, moved upward (never downward) by the reader
    /// reaching the top. Anchored on an item ID rather than an index so an
    /// eviction or a replay swap cannot slide it.
    fn window_start(&self) -> usize {
        let items = self.feed.items();
        let tail = items.len().saturating_sub(FEED_WINDOW);
        match self.window_from {
            Some(from) => self.feed.position_of(from).min(tail),
            None => tail,
        }
    }

    /// Whether there is anything above the window to pull in: more of the
    /// feed, or (EXP-783) a page of it the device still holds.
    fn can_grow_window(&self) -> bool {
        self.window_start() > 0 || self.can_load_earlier()
    }

    /// EXP-783/796 — whether a `history_page` ask could be answered: the
    /// relay said its replay is a tail, no page came back empty, none is in
    /// flight, AND the viewer socket is OPEN right now. A lingering history
    /// room keeps the socket up after the run ended, so the ask still works
    /// there; once the socket closed, nobody is listening and the affordance
    /// hides rather than asking into the void.
    fn can_load_earlier(&self) -> bool {
        self.history_truncated
            && !self.history_exhausted
            && self.history_request.is_none()
            && self
                .source
                .handle()
                .is_some_and(|handle| handle.is_connected())
    }

    /// Pull [`FEED_WINDOW_STEP`] more of the run into the window, or — when
    /// the window already reaches the feed's own first row — pin it to the
    /// FRONT and ask the device for the page BELOW it (EXP-783). The pin is
    /// what puts the page inside the window when it lands, without the view
    /// having to notice the arrival (EXP-795).
    fn grow_window(&mut self) {
        let start = self.window_start();
        if start > 0 {
            let next = start.saturating_sub(FEED_WINDOW_STEP);
            self.window_from = Some(self.feed.items()[next].id);
            return;
        }
        self.window_from = Some(WINDOW_FROM_START);
        self.request_older_page();
    }

    /// EXP-783 — one `history_page` ask, at most one in flight.
    ///
    /// A viewer that joined a long-running session holds only the relay's
    /// replay TAIL (`truncated` on `activity_synced` is how it knows), and the
    /// pages below it exist only in the device's journal. A run whose feed
    /// carries no wire sequences at all is a publisher older than EXP-783 and
    /// cannot be paged.
    fn request_older_page(&mut self) {
        if !self.can_load_earlier() {
            return;
        }
        let Some(before) = self.feed.oldest_seq() else {
            self.history_exhausted = true;
            return;
        };
        if before == 0 {
            self.history_exhausted = true;
            return;
        }
        let Some(handle) = self.source.handle() else { return };
        self.history_requests += 1;
        let request_id = format!("p{}", self.history_requests);
        if handle.request_history_page(&request_id, before, HISTORY_PAGE_LIMIT) {
            self.history_request = Some(request_id);
        }
    }

    /// EXP-783 — decide whether the window's top may SLIDE with the stream.
    ///
    /// While the list follows the tail the reader is at the newest rows, so
    /// dropping the oldest window rows as new ones arrive is invisible and
    /// keeps the projection bounded. The moment they scroll up the top is
    /// PINNED: rows vanishing above a reader is exactly the jump this whole
    /// change exists to avoid. Returning to the bottom releases the pin, and
    /// the window falls back to the newest [`FEED_WINDOW`] items.
    fn reanchor_window(&mut self) {
        if self.list.is_following_tail() {
            self.window_from = None;
            return;
        }
        if self.window_from.is_some() {
            return;
        }
        let start = self.window_start();
        if start > 0 {
            self.window_from = Some(self.feed.items()[start].id);
        }
    }

    /// Refresh the cached projection and tell the list what changed.
    ///
    /// Runs at the top of EVERY frame over the WINDOW (EXP-783), never over
    /// the run: a single site cannot miss a mutation the way per-event
    /// bookkeeping could, and the window is what keeps that affordable on a
    /// 50k-event transcript. The diff against last frame's keys
    /// ([`plan_list_sync`]) becomes splices (rows came or went) and remeasures
    /// (a row's content moved); the list re-renders every VISIBLE row each
    /// layout regardless, so a streaming row growing on screen needs nothing
    /// more than the notify that got us here.
    ///
    /// Every row in the window is re-fingerprinted every frame, on purpose
    /// (EXP-795). The first cut memoised fingerprints by row id behind an
    /// epoch hash of the view's facet state plus a dirty list from the feed —
    /// and missed one input (the answer lock, which is feed state the epoch
    /// never covered), so a card answered below the recomputed tail kept its
    /// unlocked height. A window of [`FEED_WINDOW`] items hashes in ~60µs a
    /// frame (release, 1000 rows of narration and tool calls), about what
    /// maintaining the memo's own maps cost, and a missed invalidation here
    /// is a wrong row height rather than a crash: exactly the class of bug
    /// not worth a cache to earn.
    fn sync_list(&mut self, cx: &mut gpui::Context<Self>) {
        self.refresh_active();
        self.reanchor_window();
        if std::mem::take(&mut self.extend_window) {
            self.grow_window();
        }
        let start = self.window_start();
        // EXP-783: reuse the buffer — a 1500-row `Vec<FeedRowSpec>` per frame
        // is an allocation the projection does not need.
        // EXP-789: a focused subagent tab projects ONLY that subagent's rows
        // (web `AgentConversation`); Main is the grouped transcript.
        let focus = self.active_subagent();
        match focus.as_deref() {
            Some(subagent_id) => steer::feed::group_subagent_row_specs_into(
                self.feed.items(),
                start,
                subagent_id,
                &mut self.rows,
            ),
            None => {
                self.rows.clear();
                self.feed.row_specs_from_into(start, &mut self.rows);
            }
        }
        self.prune_dropped_rows();
        // The "Working…" line belongs to the main transcript — a subagent's
        // tab has its own running spinner in the strip.
        self.working = focus.is_none() && self.working_now();
        self.chips = self.ref_resolver(cx);
        let items = self.feed.items();
        let mut keys: Vec<RowKey> = Vec::with_capacity(self.rows.len() + 1);
        for (ix, spec) in self.rows.iter().enumerate() {
            let id = spec.id();
            let fingerprint = transcript_rows::row_fingerprint(
                spec,
                items,
                self.expanded_groups.contains(&id),
                |item| self.item_facets(item),
            );
            // EXP-787: the ladder gap is chosen from the PREVIOUS row, so it
            // moves this row's height without its own content changing.
            let fingerprint =
                transcript_rows::fold_gap(fingerprint, f32::from(self.row_gap(ix)));
            keys.push(RowKey { id, fingerprint });
        }
        if self.working {
            let gap = f32::from(self.row_gap(self.rows.len()));
            keys.push(RowKey {
                fingerprint: transcript_rows::fold_gap(RowKey::WORKING.fingerprint, gap),
                ..RowKey::WORKING
            });
        }
        let ops = plan_list_sync(&self.row_keys, &keys);
        if ops.is_empty() {
            return;
        }
        let mut bulk = false;
        for op in ops {
            match op {
                ListOp::Splice { range, count } => {
                    // EXP-783: a window extension is a FRONT splice, and the
                    // uniform-height hint rewrites every item to `Unmeasured`
                    // — which would teleport a reader who is sitting at the
                    // top of the transcript. Only a splice further down may
                    // re-hint.
                    bulk |= range.start > 0 && count >= FEED_BULK_SPLICE;
                    // EXP-788: no row carries a focus handle any more — the
                    // answer is typed into the composer, which is not a list
                    // row, so an off-screen card needs no keyboard.
                    self.list.splice(range, count);
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
        self.sync_mention_source(cx);
    }

    /// EXP-790: point the composer's completion at the run's team — the same
    /// `#`-issue / `@`-member source the comment composer uses. A run with no
    /// resolvable team (an action, a repo-less chat) gets `None`, which is
    /// the plain-input behaviour on the same widget.
    fn sync_mention_source(&mut self, cx: &mut gpui::Context<Self>) {
        let team_id = self.ref_team_id(cx);
        if team_id == self.mention_team {
            return;
        }
        self.mention_team = team_id.clone();
        self.mention.update(cx, |mention, _| {
            mention.set_source(team_id.map(crate::markdown::store_completion_source));
        });
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
                if !connected {
                    if self.feed.is_staging() {
                        // A partial replay of a room we are no longer joined to.
                        self.feed.discard_staging();
                    }
                    // A page asked for on that socket is not coming either
                    // (EXP-795): the relay drops the ask with the viewer, so
                    // the next scroll to the top may ask again.
                    self.history_request = None;
                }
            }
            ViewerEvent::Activity(seq, activity) => {
                self.feed.apply_seq(seq, activity);
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
            ViewerEvent::Synced {
                first_seq,
                truncated,
            } => {
                // EXP-783: the replay names the span it covers, so pages the
                // reader had already scrolled back to load are kept rather
                // than swapped away.
                self.feed.apply_synced_from(first_seq);
                self.history_truncated = truncated;
                self.sync_changes(cx);
            }
            // EXP-783: an older page, requested by scrolling past the top of
            // what this client holds. It is PREPENDED, so every visible row
            // stays exactly where it is — and the window, pinned to the front
            // by the ask ([`Self::grow_window`]), already covers it.
            ViewerEvent::HistoryPage {
                request_id,
                events,
                done,
            } => {
                if self.history_request.as_deref() == Some(request_id.as_str()) {
                    if self.feed.prepend_page(events) == 0 {
                        self.history_exhausted = true;
                    }
                    if done {
                        self.history_request = None;
                    }
                }
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
        // EXP-848: "No activity for N min" under a pulsing "Working…" was a
        // contradiction — the header and the spinner disagreed because one
        // read the clock and the other nothing at all. With the turn slot the
        // answer is unambiguous: a run whose agent is mid-turn is thinking or
        // inside a long tool, which is not a stall. Quiet + NO turn in flight
        // is the real FEED-26 case, and that is what the caption now names.
        if self.working_now() {
            return None;
        }
        let quiet = self.last_activity.elapsed();
        (quiet >= STALE_ACTIVITY_AFTER).then(|| quiet.as_secs() / 60)
    }

    /// FEED-26 — the header's own beat. The feed is clock-free and a stalled
    /// run produces no events by definition, so the minute count needs a
    /// wakeup of its own; it only repaints once a live run is ACTUALLY quiet
    /// past the window (or, EXP-831, while a rate-limit wall counts down),
    /// so an ordinary session pays a timer and nothing else.
    fn arm_stale_tick(&self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(STALE_TICK).await;
            if this
                .update(cx, |this, cx| {
                    let paused = this.paused(cx);
                    let awaiting = !this.active.is_empty();
                    // The wall check runs one tick PAST the expiry too: the
                    // last repaint is the one that takes the banner down.
                    let now_ms = chrono::Utc::now().timestamp_millis();
                    let wall = this.rate_limit_wall_showing(
                        now_ms - STALE_TICK.as_millis() as i64,
                    );
                    if wall || this.stale_minutes(paused, awaiting).is_some() {
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

    /// Whether this card's answer is in flight (a non-question row never is —
    /// it has no answer key).
    fn is_answer_locked(&self, item: &FeedItem) -> bool {
        answer_key(item).is_some_and(|key| self.feed.is_answer_locked(&key))
    }

    /// Send one answer and lock the card. Always semantic: EXP-730 retired
    /// the raw-keystroke path, so every card is answered by its wire id.
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
        // Not a question card: nothing to send, nothing to lock.
        let Some(key) = answer_key(item) else {
            return;
        };
        let Some(card) = item.question().cloned() else {
            return;
        };
        // The lock guards against a double send. A resolved step being
        // re-answered (EXP-820) still carries its first answer's `Acked`
        // state — that lock is history, not a guard — but a re-answer of its
        // own that is still `Sending` is.
        let sending = self
            .feed
            .answer_state(&key)
            .is_some_and(|state| state.status == AnswerStatus::Sending);
        if self.feed.is_answer_locked(&key) && (!card.resolved || sending) {
            return;
        }
        let question_id = card.question_id.as_str();
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
        // EXP-788: the highlight belongs to the card that was just answered;
        // the next card (an `<ask>` stepper's next step) starts clean.
        self.answer_cursor = None;
        self.answer_cursor_for = None;
        // EXP-820: an answer closes the card's inline field and, for a
        // re-opened step, folds it back into its answered row.
        if self.inline.as_ref().is_some_and(|inline| inline.item == item_id) {
            self.inline = None;
        }
        if self.editing_step == Some(item_id) {
            self.editing_step = None;
        }
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

    // ── EXP-788/EXP-820: the answer panel's keyboard ───────────────────────
    //
    // A pending card is answered IN PLACE: its options are a numbered list,
    // and while no text field holds the keyboard the digits 1-9, ↑/↓ and Enter
    // drive it. The composer is hidden while a card is pending (EXP-820), so
    // the captures live on the view's root and the view itself takes focus on
    // that edge. Free text is typed into the option row's own inline field
    // ([`InlineAnswer`]); an `<ask>` stepper advances on its own — the next
    // step is simply the next pending card — and an answered step can be
    // re-opened while the ask is still open ([`Self::editing_card_id`]).

    /// Whether this viewer may answer at all: a live, connected, unended run
    /// it is not merely replaying.
    fn answerable_run(&self) -> bool {
        self.phase == ViewerPhase::Live
            && self.connected
            && !self.row_ended()
            && !self.source.read_only()
    }

    /// The card the keyboard targets: the re-opened step while one is open,
    /// else the newest answerable, unlocked card on a live, connected run.
    /// `None` = nothing is pending.
    fn pending_card_id(&self) -> Option<FeedItemId> {
        if !self.answerable_run() {
            return None;
        }
        if let Some(editing) = self.editing_card_id() {
            return Some(editing);
        }
        self.feed
            .items()
            .iter()
            .rev()
            .find(|item| self.active.contains(&item.id) && !self.is_answer_locked(item))
            .map(|item| item.id)
    }

    fn pending_card(&self) -> Option<&FeedItem> {
        let id = self.pending_card_id()?;
        self.feed.items().iter().find(|item| item.id == id)
    }

    /// EXP-820: the answered step being edited, if it still can be — the
    /// row exists, it is a resolved (not dismissed) step of an ask that is
    /// still OPEN ([`ask_complete`]), and this run is answerable.
    fn editing_card_id(&self) -> Option<FeedItemId> {
        let id = self.editing_step?;
        if !self.answerable_run() {
            return None;
        }
        let item = self.feed.items().iter().find(|item| item.id == id)?;
        let card = item.question()?;
        let ask_id = card.ask_id.as_deref()?;
        (card.resolved && !card.dismissed && !self.ask_complete_for(ask_id)).then_some(id)
    }

    /// [`ask_complete`] over every card of one ask in the feed.
    fn ask_complete_for(&self, ask_id: &str) -> bool {
        let items: Vec<&FeedItem> = self
            .feed
            .items()
            .iter()
            .filter(|item| {
                item.question()
                    .is_some_and(|card| card.ask_id.as_deref() == Some(ask_id))
            })
            .collect();
        ask_complete(&items)
    }

    /// Whether an answered step may be re-opened: the ask is open, the run
    /// answerable, and the step's own answer has landed (a step whose first
    /// answer is still in flight is not a step yet).
    fn step_editable(&self, item: &FeedItem) -> bool {
        let Some(card) = item.question() else {
            return false;
        };
        let Some(ask_id) = card.ask_id.as_deref() else {
            return false;
        };
        card.resolved
            && !card.dismissed
            && self.answerable_run()
            && !self.ask_complete_for(ask_id)
    }

    /// The keyboard drives the card only while no text field wants the keys
    /// ([`keyboard_drives_card`]).
    fn keyboard_answers(&self, cx: &App) -> bool {
        keyboard_drives_card(
            self.pending_card_id().is_some(),
            self.inline.is_some(),
            self.composer_visible(),
            self.input.read(cx).value().trim().is_empty(),
            self.slash.is_some(),
        )
    }

    /// The highlighted option on the pending card, or `None` when the
    /// highlight was set on a card that is no longer pending.
    fn answer_cursor_on(&self, card: FeedItemId) -> Option<usize> {
        (self.answer_cursor_for == Some(card))
            .then_some(self.answer_cursor)
            .flatten()
    }

    fn move_answer_cursor(&mut self, delta: isize, cx: &mut gpui::Context<Self>) {
        let Some(item) = self.pending_card() else {
            return;
        };
        let (id, len) = (item.id, item.question().map_or(0, |card| card.options.len()));
        if len == 0 {
            return;
        }
        let next = match self.answer_cursor_on(id) {
            Some(at) => (at as isize + delta).rem_euclid(len as isize) as usize,
            None if delta < 0 => len - 1,
            None => 0,
        };
        self.answer_cursor = Some(next);
        self.answer_cursor_for = Some(id);
        cx.notify();
    }

    /// Activate option `index` of the pending card from the keyboard: a
    /// multi-select toggles it into the picks (and parks the highlight on
    /// it), anything else goes through [`Self::pick_option`].
    fn activate_option(&mut self, index: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(item) = self.pending_card() else {
            return;
        };
        let item_id = item.id;
        let multi = item.question().is_some_and(|card| card.multi_select);
        self.pick_option(item_id, index, window, cx);
        if multi {
            self.answer_cursor = Some(index);
            self.answer_cursor_for = Some(item_id);
            cx.notify();
        }
    }

    /// The ONE option activation (click, digit, Enter on the highlight): a
    /// multi-select toggles the pick; a free-text row and a plan card's
    /// reject row OPEN their inline field (EXP-820) instead of sending; any
    /// other option is the answer.
    fn pick_option(
        &mut self,
        item_id: FeedItemId,
        index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(item) = self.feed.items().iter().find(|item| item.id == item_id) else {
            return;
        };
        let (Some(card), Some(key)) = (item.question(), answer_key(item)) else {
            return;
        };
        let Some(option) = card.options.get(index) else {
            return;
        };
        let (option_key, option_label) = (option.key.clone(), option.label.clone());
        if card.multi_select {
            let picks = self.picked.entry(key).or_default();
            match picks.iter().position(|pick| *pick == option_key) {
                Some(at) => {
                    picks.remove(at);
                }
                None => picks.push(option_key),
            }
            cx.notify();
            return;
        }
        if let Some(placeholder) = inline_placeholder(card, index) {
            self.open_inline(item_id, option_key, placeholder, window, cx);
            return;
        }
        self.answer(item_id, vec![option_key], vec![option_label], None, cx);
    }

    /// Submit a multi-select card's picks (the "Submit" button, and Enter
    /// with nothing highlighted).
    fn submit_picks(&mut self, item_id: FeedItemId, cx: &mut gpui::Context<Self>) {
        let Some(item) = self.feed.items().iter().find(|item| item.id == item_id) else {
            return;
        };
        let (Some(card), Some(key)) = (item.question(), answer_key(item)) else {
            return;
        };
        let picked = self.picked.get(&key).cloned().unwrap_or_default();
        if picked.is_empty() {
            return;
        }
        let labels: Vec<String> = card
            .options
            .iter()
            .filter(|option| picked.contains(&option.key))
            .map(|option| option.label.clone())
            .collect();
        self.answer(item_id, picked, labels, None, cx);
    }

    fn on_answer_up(&mut self, _: &input::MoveUp, _: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.keyboard_answers(cx) {
            self.move_answer_cursor(-1, cx);
            cx.stop_propagation();
        }
    }

    fn on_answer_down(
        &mut self,
        _: &input::MoveDown,
        _: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.keyboard_answers(cx) {
            self.move_answer_cursor(1, cx);
            cx.stop_propagation();
        }
    }

    /// Enter with a card pending and no field holding the keys: the
    /// highlighted option, or a multi-select's picks. With nothing
    /// highlighted and nothing picked it does nothing.
    fn on_answer_enter(
        &mut self,
        action: &input::Enter,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if action.shift || !self.keyboard_answers(cx) {
            return;
        }
        cx.stop_propagation();
        self.enter_pending(window, cx);
    }

    /// The Enter half of the card keyboard, shared by the `input::Enter`
    /// action (a field is focused) and the raw key (the view is).
    fn enter_pending(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(item) = self.pending_card() else {
            return;
        };
        let id = item.id;
        let multi = item.question().is_some_and(|card| card.multi_select);
        match self.answer_cursor_on(id) {
            Some(index) => self.activate_option(index, window, cx),
            None if multi => self.submit_picks(id, cx),
            None => {}
        }
    }

    /// The digits: `1`-`9` pick the option at that position (unmodified —
    /// ⌘1 is the window's, and a shifted digit is punctuation). EXP-820:
    /// with the composer hidden no `input::*` action fires, so ↑/↓/Enter
    /// arrive here as raw keys too.
    fn on_answer_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.keyboard_answers(cx) {
            return;
        }
        if let Some(index) = answer_digit(&event.keystroke) {
            let options = self
                .pending_card()
                .and_then(FeedItem::question)
                .map_or(0, |card| card.options.len());
            if index >= options {
                return;
            }
            cx.stop_propagation();
            self.activate_option(index, window, cx);
            return;
        }
        // A focused field turns these into `input::*` actions before any key
        // listener runs, so reaching here means the view holds the keys.
        if self.composer_visible() {
            return;
        }
        let modifiers = &event.keystroke.modifiers;
        if modifiers.control || modifiers.alt || modifiers.platform || modifiers.function {
            return;
        }
        match event.keystroke.key.as_str() {
            "up" => {
                cx.stop_propagation();
                self.move_answer_cursor(-1, cx);
            }
            "down" => {
                cx.stop_propagation();
                self.move_answer_cursor(1, cx);
            }
            "enter" if !modifiers.shift => {
                cx.stop_propagation();
                self.enter_pending(window, cx);
            }
            _ => {}
        }
    }

    // ── EXP-820: the inline free-text field ────────────────────────────────

    /// Open the inline field under option `option_key` of `item_id` (closing
    /// any other) and give it the keyboard.
    fn open_inline(
        &mut self,
        item_id: FeedItemId,
        option_key: String,
        placeholder: &'static str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(state) = self
            .inline
            .as_ref()
            .filter(|inline| inline.item == item_id && inline.option_key == option_key)
            .map(|inline| inline.state.clone())
        {
            state.update(cx, |state, cx| state.focus(window, cx));
            return;
        }
        let state = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let subscription = cx.subscribe_in(
            &state,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => this.send_inline(window, cx),
                // The Send button's enabled state follows the draft.
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        );
        state.update(cx, |state, cx| state.focus(window, cx));
        self.inline = Some(InlineAnswer {
            item: item_id,
            option_key,
            state,
            _subscription: subscription,
        });
        cx.notify();
    }

    /// Collapse the inline field (Escape) and hand the keys back to the view.
    fn close_inline(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.inline.take().is_some() {
            window.focus(&self.focus_handle, cx);
            cx.notify();
        }
    }

    fn on_inline_escape(
        &mut self,
        _: &input::Escape,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.inline.is_some() {
            cx.stop_propagation();
            self.close_inline(window, cx);
        }
    }

    /// Send the inline field. A question's free-text row answers with the
    /// typed text as the option's label (empty never sends). A plan card's
    /// reject row answers with the reject option and THEN sends the text as
    /// an ordinary steer message — with nothing typed it is a plain reject.
    fn send_inline(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(inline) = self.inline.as_ref() else {
            return;
        };
        let (item_id, option_key) = (inline.item, inline.option_key.clone());
        // The web input caps at 4000 chars; gpui-component's has no
        // maxLength, so the cap lands here (the relay rejects longer).
        let text: String = inline
            .state
            .read(cx)
            .value()
            .trim()
            .chars()
            .take(FREE_TEXT_MAX)
            .collect();
        let Some(item) = self.feed.items().iter().find(|item| item.id == item_id) else {
            self.close_inline(window, cx);
            return;
        };
        let Some(card) = item.question() else {
            return;
        };
        let Some((index, option)) = card
            .options
            .iter()
            .enumerate()
            .find(|(_, option)| option.key == option_key)
        else {
            self.close_inline(window, cx);
            return;
        };
        let plan_reject = is_plan_reject(card, index);
        let option_label = option.label.clone();
        if plan_reject {
            self.answer(item_id, vec![option_key], vec![option_label], None, cx);
        } else {
            if text.is_empty() {
                return;
            }
            self.answer(item_id, vec![option_key], vec![text.clone()], Some(text.clone()), cx);
        }
        // `answer` dropped the field once the answer went out; a refused send
        // (not connected) keeps it — and its text — for a retry.
        if self.inline.is_some() {
            return;
        }
        if plan_reject && !text.is_empty() && !self.deliver(&text) {
            self.notice = Some(SharedString::from("The session is no longer connected"));
        }
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    // ── Composer ───────────────────────────────────────────────────────────

    /// The web gate, verbatim: a draft goes out only on a LIVE phase with an
    /// open socket (a slow-consumer redial keeps the phase and must still dim
    /// the button) over a row that has not ended.
    fn can_send(&self, cx: &App) -> bool {
        let has_content =
            !self.input.read(cx).value().trim().is_empty() || !self.pending_images.is_empty();
        !self.sending
            && !self.source.read_only()
            && self.phase == ViewerPhase::Live
            && self.connected
            && !self.row_ended()
            && has_content
    }

    fn composer_visible(&self) -> bool {
        // EXP-746: a replay is a transcript — there is nothing to type at.
        if self.source.read_only()
            || self.row_ended()
            || matches!(self.phase, ViewerPhase::Ended { .. })
        {
            return false;
        }
        // EXP-820: a pending card takes the composer's place — free text is
        // typed in the card, and a message would only race the answer.
        !(self.phase == ViewerPhase::Live && self.connected && !self.active.is_empty())
    }

    /// EXP-820: focus follows the composer's visibility EDGES, once per
    /// flip. A card becoming pending pulls the composer out from under the
    /// caret, so the view takes the keys (the root captures drive the card);
    /// the last card resolving brings the composer back, and a view still
    /// holding focus hands it to the field. Never steals from anything
    /// outside this view.
    fn sync_card_focus(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let visible = self.composer_visible();
        if visible == self.composer_shown {
            return;
        }
        self.composer_shown = visible;
        let field = self.input.read(cx).focus_handle(cx);
        if !visible {
            if self.inline.is_some() || self.pending_card_id().is_none() {
                return;
            }
            if field.is_focused(window) || window.focused(cx).is_none() {
                window.focus(&self.focus_handle, cx);
            }
        } else if self.focus_handle.is_focused(window) {
            window.focus(&field, cx);
        }
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

    /// EXP-746: the run's context/spend meter (the usage sheet's own block).
    pub(crate) fn usage(&self) -> Option<steer::SessionUsage> {
        self.feed.usage()
    }

    /// EXP-847: the run's MODE, for the header's read-only Plan chip — the
    /// latest `config_state` through the shared
    /// [`crate::session_extras::mode_chip`] (the ×4 `modeChip`). `None` for a
    /// run that advertises no modes (codex) and for one that is not in plan
    /// mode, which is why the header shows nothing the moment an approved
    /// `ExitPlanMode` flips the mode back.
    ///
    /// Never a control: EXP-790 keeps plan a LAUNCH-time choice.
    pub(crate) fn plan_mode_label(&self) -> Option<SharedString> {
        let chip = crate::session_extras::mode_chip(self.feed.config())?;
        (chip.value == crate::session_extras::PLAN_MODE_ID)
            .then(|| SharedString::from(chip.value_label))
    }

    // ── EXP-773: the "Changes" bar ────────────────────────────────────────

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

    /// EXP-773 — the collapsible "Changes +N −M [Merge]" row, painted
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
            },
            cx,
        ))
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
        // EXP-820: the draft is always a MESSAGE — a card's free text is
        // typed in the card itself, and the composer is hidden while one is
        // pending.
        if let Some(parsed) = parse_command(&text, self.agent()) {
            if parsed.command.confirm {
                self.prompt_command(parsed.command.name, window, cx);
                return;
            }
        }
        self.send_confirmed(window, cx);
    }

    /// EXP-790: the Stop half of the composer's one button — interrupt the
    /// running turn without ending the run. Local: the engine's cancel;
    /// remote: the interrupt frame down the viewer socket.
    fn stop_turn(&mut self, cx: &mut gpui::Context<Self>) {
        match &self.source {
            FeedSource::Local { session } => session.cancel_turn(),
            FeedSource::Replay { .. } | FeedSource::Journal { .. } => {}
            FeedSource::Remote { handle } => {
                if !handle
                    .as_ref()
                    .is_some_and(|handle| handle.send_interrupt())
                {
                    self.notice = Some(SharedString::from("The session is no longer connected"));
                }
            }
        }
        cx.notify();
    }

    /// EXP-790: Stop shows while the agent is WORKING — a live turn with
    /// nothing waiting on the reader — and only where a stop can reach it;
    /// Send otherwise, and always once there is a draft to send.
    fn shows_stop(&self, cx: &App) -> bool {
        let has_draft =
            !self.input.read(cx).value().trim().is_empty() || !self.pending_images.is_empty();
        self.working && !has_draft && !self.source.read_only()
    }

    fn send_confirmed(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if !self.can_send(cx) {
            return;
        }
        let text = self.input.read(cx).value().to_string();
        self.notice = None;
        if self.pending_images.is_empty() {
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
        let jobs = self.pending_images.jobs();
        self.sending = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move {
                    composer_images::upload_all(jobs, |filename, content_type, bytes| {
                        transport.upload_session(&session_id, filename, content_type, bytes)
                    })
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.sending = false;
                match outcome {
                    Ok(resolved) => {
                        this.pending_images.note_uploaded(&resolved);
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
                        this.pending_images.note_uploaded(&resolved);
                        log::warn!("[ui] steer composer upload failed: {error}");
                        this.notice = Some(SharedString::from("Couldn't upload image"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
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
        self.pending_images.clear();
        self.notice = None;
        self.slash = None;
        self.slash_dismissed_for = None;
        self.input
            .update(cx, |state, cx| state.set_value("", window, cx));
    }

    /// Add clipboard / picked images to the draft — the shared composer
    /// strip's rules (type + 10 MB + at most `MAX_STEER_IMAGES`, an
    /// `[Image #k]` marker at the caret per image; EXP-698/EXP-825).
    fn stage_images(
        &mut self,
        images: Vec<composer_images::StagedFile>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.notice = self.pending_images.stage(images, &self.input, window, cx);
        cx.notify();
    }

    /// Drop a staged image and renumber the draft's markers behind it.
    fn remove_pending(&mut self, key: u64, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.pending_images.remove(key, &self.input, window, cx);
        cx.notify();
    }

    fn on_paste(&mut self, _: &input::Paste, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let images = composer_images::clipboard_images(cx);
        if images.is_empty() {
            return;
        }
        cx.stop_propagation();
        self.stage_images(images, window, cx);
    }

    fn pick_images(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        composer_images::pick_image_files(window, cx, |this, read, window, cx| {
            this.stage_images(read, window, cx)
        });
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
        // EXP-849 fix-up: ONE verb for ending a run — "Stop", the same word
        // Android and iOS already use. "Kill" described the mechanism, not
        // what the person is doing.
        let spec = AlertSpec::new("Stop this coding session?", description, "Stop session")
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

/// …and the same row when the run left nothing to replay (its workspace,
/// and with it the device journal, is gone).
pub(crate) const REPLAY_EMPTY_BANNER: &str = "No transcript for this run";

/// EXP-746: the confirm body for a run hosted IN this app — the remote
/// copy's "on <machine>" has nothing to name here, and the worktree promise
/// is the part that matters either way.
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

/// EXP-848 — everything the working predicate reads, gathered so the rule
/// itself is a pure function one test can pin (the spinner, the Stop button
/// and the FEED-26 caption all go through [`is_working`], and a second copy
/// is how they drifted apart in the first place).
pub(crate) struct WorkingFacts {
    /// Nothing on screen yet — the synthetic row has nothing to sit under.
    pub empty_feed: bool,
    pub live: bool,
    pub row_ended: bool,
    /// The feed's latest-wins `turn` slot (default `ended`).
    pub turn_working: bool,
    /// A question card is waiting for an answer in THIS viewer.
    pub awaiting_input: bool,
    /// The synced row says the host parked the run (FEED-35).
    pub needs_input: bool,
    pub staging: bool,
    /// A rate-limit WALL is up (EXP-804/FEED-35 — the same rule the row's
    /// `blocked` column records).
    pub blocked: bool,
    pub compacting: bool,
}

/// EXP-848 — the ONE working predicate, identical on all four clients: the
/// agent is executing a turn and nothing is waiting, walled or compacting.
/// `turn_working` is the authority; every other field is a veto.
pub(crate) fn is_working(facts: &WorkingFacts) -> bool {
    !facts.empty_feed
        && facts.live
        && !facts.row_ended
        && facts.turn_working
        && !facts.awaiting_input
        && !facts.needs_input
        && !facts.staging
        && !facts.blocked
        && !facts.compacting
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

/// EXP-785 — the collapsed tool-group caption over the group's tool rows:
/// the shared [`steer::tool_group_summary`] fed each call's contract kind
/// (`None` = `other`), its detail and whether it failed. Rows that are not
/// tool calls (none reach a tool run today) are skipped rather than counted.
pub(crate) fn tool_group_caption(items: &[&FeedItem]) -> String {
    let calls: Vec<steer::ToolCallSummary<'_>> = items
        .iter()
        .filter_map(|item| match &item.kind {
            FeedKind::Tool {
                detail,
                tool_kind,
                failed,
                ..
            } => Some(steer::ToolCallSummary {
                kind: tool_kind.map_or("other", steer::ToolKind::as_str),
                detail: detail.as_deref(),
                failed: *failed,
            }),
            _ => None,
        })
        .collect();
    steer::tool_group_summary(&calls)
}

/// EXP-847 — what a subagent chip is NAMED: the spawning `Agent` call's own
/// `title` (what the model said this subagent is FOR) when it carried one,
/// else the agent TYPE, which is all codex, an external agent and every
/// pre-847 publisher have. Mirrored ×4.
pub(crate) fn subagent_label(title: Option<&str>, agent_type: &str) -> String {
    title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| agent_type.trim())
        .to_string()
}

/// EXP-847 — the agent TYPE as a SECONDARY caption: shown only when the
/// title took the primary line, so a type-only chip never says its type twice.
pub(crate) fn subagent_type_caption(title: Option<&str>, agent_type: &str) -> Option<String> {
    let title = title.map(str::trim).filter(|title| !title.is_empty())?;
    let agent_type = agent_type.trim();
    (!agent_type.is_empty() && agent_type != title).then(|| agent_type.to_string())
}

/// The subagent group row's status caption — EXP-847: the state word plus the
/// SAME contract `toolGroupSummary` a collapsed tool group wears
/// (`running · read 3 files · 1 failed`), not the bare count of calls it used to
/// report. What the subagent DID is the interesting half, and the number alone
/// never said it. Mirrored ×4.
///
/// `tool_summary` is [`tool_group_caption`] over the subagent's own tool rows,
/// `None` when this feed holds none — a replay whose oldest subagent calls the
/// journal evicted (EXP-748) still knows the publisher's COUNT, so that is the
/// fallback rather than a silent "done".
pub(crate) fn subagent_caption(done: bool, tool_count: usize, tool_summary: Option<&str>) -> String {
    let state = if done { "done" } else { "running" };
    if let Some(summary) = tool_summary.map(str::trim).filter(|text| !text.is_empty()) {
        // The summary capitalises its first letter for a row of its own; here
        // it continues a sentence that starts with the state word.
        return format!("{state} · {}", lower_first(summary));
    }
    match tool_count {
        0 => state.to_string(),
        1 => format!("{state} · 1 tool call"),
        n => format!("{state} · {n} tool calls"),
    }
}

/// `Read 3 files` → `read 3 files` (see [`subagent_caption`]). ASCII-only by
/// design: every `toolGroupSummary` segment starts with an English verb.
fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// A body long enough to fold behind "Show more" (web `clampable`).
pub(crate) fn clampable(text: &str) -> bool {
    text.len() > CLAMP_CHARS || text.lines().count() > CLAMP_LINES
}

/// The composer's hint. EXP-724: the `/` menu is invisible until it is
/// typed, so the placeholder is the only hint it exists (every agent's catalog
/// is non-empty, which is why this is a constant here and a conditional on
/// web/Android). EXP-820 retired the per-card forks: a pending card hides the
/// composer and takes its free text inline.
pub(crate) const COMPOSER_PLACEHOLDER: &str = "Message the agent… (/ for commands)";

/// EXP-820 — whether the card keyboard (digits, ↑/↓, Enter) is live. A text
/// field that wants the keys wins: the inline answer field whenever it is
/// open, and the composer once it holds a draft or a `/` menu. With the
/// composer hidden (a card is pending) the view itself holds the keys.
pub(crate) fn keyboard_drives_card(
    card_pending: bool,
    inline_open: bool,
    composer_visible: bool,
    draft_empty: bool,
    slash_open: bool,
) -> bool {
    card_pending && !inline_open && (!composer_visible || (draft_empty && !slash_open))
}

/// EXP-820 — whether a multi-question ask is OVER, so its stepper stops
/// waiting for a next step and its answered rows stop being editable. Web,
/// iOS and Android apply the same rule: the submit step (an `ask_id` card with
/// no `index`) resolved, or a one-step ask (`total` ≤ 1) with every numbered
/// step resolved, or any step dismissed.
pub(crate) fn ask_complete(items: &[&FeedItem]) -> bool {
    let cards = || items.iter().filter_map(|item| item.question());
    if cards().any(|card| card.dismissed) {
        return true;
    }
    if cards().any(|card| card.ask_id.is_some() && card.index.is_none() && card.resolved) {
        return true;
    }
    let numbered: Vec<&steer::feed::QuestionCard> =
        cards().filter(|card| card.index.is_some()).collect();
    let total = numbered
        .first()
        .and_then(|card| card.total)
        .unwrap_or(numbered.len() as u32);
    total <= 1 && !numbered.is_empty() && numbered.iter().all(|card| card.resolved)
}

/// EXP-820 — a plan card's LAST option is its reject ("No, keep planning"),
/// the one whose inline text is a change request.
fn is_plan_reject(card: &steer::feed::QuestionCard, index: usize) -> bool {
    card.plan_mode && !card.options.is_empty() && index + 1 == card.options.len()
}

/// EXP-820 — the inline field an option opens instead of answering, with its
/// placeholder: a question's free-text row ("Type something.") or a plan
/// card's reject row. `None` = the option answers directly. A multi-select's
/// rows only ever toggle (out of scope).
fn inline_placeholder(card: &steer::feed::QuestionCard, index: usize) -> Option<&'static str> {
    if card.multi_select {
        return None;
    }
    if is_plan_reject(card, index) {
        return Some(PLAN_REJECT_PLACEHOLDER);
    }
    card.options
        .get(index)
        .filter(|option| option.free_text)
        .map(|_| FREE_TEXT_PLACEHOLDER)
}

/// EXP-788 — the option a plain digit keystroke names: `1`-`9` → `0..9`,
/// unmodified only (a ⌘-digit is the window's, a shifted one is punctuation).
pub(crate) fn answer_digit(keystroke: &gpui::Keystroke) -> Option<usize> {
    let modifiers = keystroke.modifiers;
    if modifiers.control || modifiers.alt || modifiers.platform || modifiers.function || modifiers.shift
    {
        return None;
    }
    let mut chars = keystroke.key.chars();
    let (Some(digit), None) = (chars.next(), chars.next()) else {
        return None;
    };
    match digit.to_digit(10) {
        Some(n) if (1..=9).contains(&n) => Some(n as usize - 1),
        _ => None,
    }
}

/// The subagent a group row belongs to — its first scoped item's id.
fn subagent_id_of(items: &[&FeedItem]) -> Option<String> {
    items
        .iter()
        .find_map(|item| item.subagent_id())
        .map(str::to_string)
}

/// EXP-790 — the composer width under which the tool row leaves the field's
/// line and drops under it: a 240px field beside the 24px attach glyph, the
/// 32px round button and the card's own padding and gaps.
const COMPOSER_INLINE_MIN_WIDTH: f32 = 340.;

/// How much wider than the threshold the card must get again before the row
/// comes back up. Without it a width sitting on the threshold — a pane being
/// dragged, a textarea growing a line — would flap between the two layouts
/// on every frame.
const TOOL_ROW_WRAP_HYSTERESIS: f32 = 24.;

/// EXP-818: how much of the transcript column a user bubble may take (web
/// `max-w-[85%]`), and the narrowest bubble worth drawing.
const USER_BUBBLE_MAX_FRACTION: f32 = 0.85;
const USER_BUBBLE_MIN_W: f32 = 48.;

/// EXP-790 — whether the composer's tool row wraps under the field at `width`
/// when it takes `needed` to sit beside it, given whether it is `wrapped` right
/// now. Wraps as soon as the width falls short; un-wraps only once the width
/// clears the threshold by [`TOOL_ROW_WRAP_HYSTERESIS`].
pub(crate) fn tool_row_wraps(width: f32, needed: f32, wrapped: bool) -> bool {
    if wrapped {
        width < needed + TOOL_ROW_WRAP_HYSTERESIS
    } else {
        width < needed
    }
}

/// EXP-818: whether a rate-limit report is a WALL worth a banner. Claude
/// files `allowed_warning` on every turn past ~75% of a window while it
/// keeps working — the Usage pill already shows that percentage, and a
/// "rate limited" banner over a run that is visibly working was wrong. Only
/// `rejected`, or a notice the agent itself wrote, is a banner. Web
/// `rateLimitBanner` twin.
pub(crate) fn rate_limit_is_wall(status: &str, message: Option<&str>) -> bool {
    // FEED-35: ONE rule with the row's `blocked` (the mapper) and the web.
    steer::rate_limit_is_wall(status, message)
}

/// EXP-784 — the rate-limit banner's line: the agent's message (or a generic
/// one) and, when the report named a reset, ` · resets in 2h 10m` (EXP-818:
/// relative, the usage cards' countdown — a clock reading `00:00` looked
/// like a zero).
pub(crate) fn rate_limit_caption(message: Option<&str>, countdown: Option<String>) -> String {
    let message = message
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or("Rate limit reached");
    match countdown {
        Some(countdown) => format!("{message} · {countdown}"),
        None => message.to_string(),
    }
}

/// EXP-788 — the numbered chip on an option row: the digit that picks it
/// (1-9, by position). `live` = the keyboard is on this card, so the chip
/// reads as a key; otherwise it is a quiet ordinal. Options past the ninth
/// carry no chip (there is no key for them).
fn key_chip(index: usize, live: bool, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let Some(digit) = (index < 9).then(|| (index + 1).to_string()) else {
        return div().w(px(18.)).into_any_element();
    };
    div()
        .flex_shrink_0()
        .w(px(18.))
        .h(px(18.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(theme::tokens::radius::SM))
        .border_1()
        .border_color(if live {
            theme::tokens::glass::STROKE_ACTIVE.to_hsla()
        } else {
            theme::tokens::glass::STROKE_CARD.to_hsla()
        })
        .bg(theme::tokens::glass::FILL_CARD.to_hsla())
        .text_2xs()
        .font_family(theme::terminal::FONT_FAMILY)
        .text_color(if live { cx.theme().foreground } else { muted })
        .child(SharedString::from(digit))
        .into_any_element()
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
                // EXP-818: the ONE Stop — identical on the hosting machine
                // and on a watching one.
                this.child(
                    crate::session_screen::stop_session_pill("steer-stop", cx).on_click(
                        cx.listener(|this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.prompt_kill(window, cx);
                        }),
                    ),
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
        //
        // EXP-787: the list keeps the pane's 12px inset (the header, banners
        // and composer sit at the same `px_3`); the token GUTTER and the
        // reading column are laid out per ROW ([`Self::transcript_row`])
        // rather than by wrapping the list in a narrower box, so the list
        // keeps measuring and scrolling over the full pane width exactly as
        // before.
        let list = list(
            self.list.clone(),
            cx.processor(|this, ix: usize, window, cx| this.render_list_row(ix, window, cx)),
        )
        .px_3()
        .py_2();
        crate::scroll_pane::v_list_pane(list, &self.list).into_any_element()
    }

    /// EXP-787 — the rhythm class of list row `ix`: the feed row's own class,
    /// or [`RowClass::Tool`] for the synthetic "Working…" line past the last
    /// spec (it is machine chatter like the calls it trails).
    fn row_class(&self, ix: usize) -> RowClass {
        match self.rows.get(ix) {
            Some(spec) => spec.class(self.feed.items()),
            None => RowClass::Tool,
        }
    }

    /// EXP-787 — the space ABOVE list row `ix`: the shared ladder
    /// ([`transcript_gap`]) over its predecessor's class, mapped onto the
    /// design tokens. The FIRST painted row takes none.
    fn row_gap(&self, ix: usize) -> Pixels {
        let Some(prev) = ix.checked_sub(1) else {
            return px(0.);
        };
        px(match transcript_gap(self.row_class(prev), self.row_class(ix)) {
            Gap::Turn => transcript::GAP_TURN,
            Gap::Block => transcript::GAP_BLOCK,
            Gap::Tool => transcript::GAP_TOOL,
            Gap::Default => transcript::GAP_DEFAULT,
        })
    }

    /// EXP-787 — one row's outer box: the ladder gap above it, and the
    /// content centred inside the token measure. The wrapper still spans the
    /// list's full width (the list measures it that way), so only the reading
    /// column is clamped.
    ///
    /// The gutters are flex SPACERS, not padding: on a pane wide enough for
    /// the whole measure they hold the token GUTTER (minus the list's own
    /// 12px inset) either side of a 736px column, and on a narrow IDE split
    /// they give way in proportion with the column instead of eating 96px of
    /// a 400px pane — the web's `sm:` fallback, done the way gpui can.
    fn transcript_row(&self, ix: usize, element: AnyElement) -> AnyElement {
        let gutter = || {
            div()
                .flex_basis(px(transcript::GUTTER - 12.))
                .flex_shrink(1.)
                .min_w_0()
        };
        h_flex()
            .w_full()
            .justify_center()
            .pt(self.row_gap(ix))
            .child(gutter())
            .child(
                div()
                    .flex_basis(px(transcript::MAX_WIDTH))
                    .flex_shrink(1.)
                    .min_w_0()
                    .child(element),
            )
            .child(gutter())
            .into_any_element()
    }

    /// One transcript row by list index: the cached spec resolved against
    /// the feed and rendered exactly as before, or — past the last spec —
    /// the synthetic "Working…" line. EXP-787: the row's own vertical padding
    /// is gone; [`Self::transcript_row`] gives it the gap the ladder chose
    /// from the row before it.
    fn render_list_row(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        // EXP-783: painting the first row means the reader reached the top of
        // the window; pull the next page of the run in behind it. The
        // extension front-splices, gpui rebases the scroll anchor, and row 0
        // leaves the viewport — so this fires again only if the reader keeps
        // scrolling up, and not at all once the whole run is in.
        if ix == 0 && !self.extend_window && self.can_grow_window() {
            self.extend_window = true;
            cx.notify();
        }
        let Some(spec) = self.rows.get(ix) else {
            let working = tool_text(h_flex())
                .w_full()
                .gap_2()
                .items_center()
                .child(
                    Icon::new(registry::CODING_ASSISTANT)
                        .xsmall()
                        .text_color(muted.opacity(0.6)),
                )
                .child(div().text_color(muted).child("Working…"))
                .into_any_element();
            return self.transcript_row(ix, working);
        };
        let live = self.phase == ViewerPhase::Live;
        let last_row = self.rows.len().saturating_sub(1);
        let row = spec.resolve(self.feed.items());
        let element = self.render_row(&row, ix == last_row && live, &self.active, window, cx);
        self.transcript_row(ix, element)
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
                .child(
                    div().mt(px(3.)).flex_shrink_0().child(
                        Icon::new(registry::CODING_ASSISTANT)
                            .xsmall()
                            .text_color(muted.opacity(0.6)),
                    ),
                )
                .child(
                    body_text(div()).flex_1().min_w_0().child(
                        self.with_issue_chips(
                            crate::markdown::MarkdownView::new(
                                SharedString::from(format!("steer-narration-{}", item.id)),
                                text.clone(),
                            )
                            // EXP-698: the feed reads at the chat rhythm, and
                            // its inline code takes the semantic tint.
                            .chat(true)
                            .selectable(true)
                            .images(self.images.clone()),
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
                .child(
                    // The bubble keeps its INNER padding — only the row's
                    // outer rhythm moved to the ladder (EXP-787). EXP-818:
                    // and a DEFINITE width — inside the EXP-787 column a
                    // content-sized bubble measured at min-content and
                    // wrapped every word.
                    body_text(div())
                        .min_w_0()
                        .w(self.user_bubble_width(text, window))
                        .rounded(px(12.))
                        .border_1()
                        .border_color(theme::tokens::glass::STROKE_STRONG.to_hsla())
                        .bg(theme::tokens::glass::FILL_ACTIVE.to_hsla())
                        .px_3()
                        .py_2()
                        .child(self.render_user_message(item.id, text, cx)),
                )
                .into_any_element(),
            FeedKind::Tool { .. } => self.render_tool_item(item, true, cx),
            FeedKind::Permission { tool, detail } => {
                let amber = theme::tokens::YELLOW.to_hsla();
                tool_text(v_flex())
                    .w_full()
                    .min_w_0()
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
            FeedKind::Compaction => tool_text(h_flex())
                .w_full()
                .gap_1p5()
                .items_center()
                .justify_center()
                .child(
                    Icon::new(registry::CODING_COMPACT)
                        .xsmall()
                        .text_color(muted.opacity(0.6)),
                )
                .child(div().text_color(muted).child(COMPACTED_LABEL))
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
    /// EXP-818: a user bubble's width — its longest source line shaped at
    /// the body size plus the bubble's padding, capped at
    /// [`USER_BUBBLE_MAX_FRACTION`] of the transcript column. A bubble
    /// sized to its content wrapped every word once the EXP-787 column gave
    /// it a min-content measure; a definite width is the recorded-px
    /// pattern every wrapped text in this crate ends up on.
    fn user_bubble_width(&self, text: &str, window: &Window) -> Pixels {
        let parsed = steer::image_message::parse_steer_message(text);
        let font = window.text_style().font();
        let mut longest = px(0.);
        for line in parsed.text.lines() {
            let line = line.trim_end();
            if line.is_empty() {
                continue;
            }
            let run = gpui::TextRun {
                len: line.len(),
                font: font.clone(),
                color: gpui::black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let width = window
                .text_system()
                .shape_line(
                    SharedString::from(line.to_string()),
                    px(transcript::BODY_SIZE),
                    &[run],
                    None,
                )
                .width;
            if width > longest {
                longest = width;
            }
        }
        let column = f32::from(self.composer_width.get());
        let column = if column > 0. { column } else { transcript::MAX_WIDTH };
        let cap = px((column * USER_BUBBLE_MAX_FRACTION).max(USER_BUBBLE_MIN_W));
        // px_3 both sides + the 1px stroke each side.
        let padded = longest + px(2. * 12. + 2.);
        if padded > cap {
            cap
        } else if padded < px(USER_BUBBLE_MIN_W) {
            px(USER_BUBBLE_MIN_W)
        } else {
            padded
        }
    }

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
        // EXP-787: the token prose rhythm, the same one `.chat(true)` gives
        // the markdown path — a bubble with pills must not read at a
        // different line height from one without.
        let mut column = body_text(v_flex()).w_full().min_w_0();
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
                        .selectable(true)
                        .images(self.images.clone()),
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

    /// One tool call's row plus whatever hangs off it: a LOCAL run's per-edit
    /// diff and command output ([`Self::render_extras`]), or — for a source
    /// with no engine (EXP-786) — the per-call diff the publisher put on the
    /// wire. The two never stack: where the engine runs, its ACP content is
    /// richer than the wire's cut, so the wire diff is ignored there.
    fn render_tool_item(
        &self,
        item: &FeedItem,
        with_extras: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let FeedKind::Tool {
            name,
            detail,
            failed,
            diff,
            settled,
            preview,
            ..
        } = &item.kind
        else {
            return div().into_any_element();
        };
        // EXP-846: one of OUR MCP calls reads as product work (the mark, the
        // contract's caption, the answer's preview); everything else is the
        // plain tool row.
        let row = match steer::exp_tool_display(name, *settled) {
            Some(display) => exp_tool_call_row(
                item.id,
                display,
                detail.as_deref(),
                *failed,
                preview.as_ref(),
                cx,
            ),
            None => tool_row(name, detail.as_deref(), *failed, cx).into_any_element(),
        };
        if !with_extras {
            return row;
        }
        let extras = if self.source.session().is_some() {
            self.render_extras(item.id, cx)
        } else {
            let id = item.id;
            diff.as_deref().map(|diff| {
                crate::session_extras::render_wire_diff(
                    diff,
                    id,
                    self.expanded_extras.contains(&id),
                    Box::new(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        if !this.expanded_extras.insert(id) {
                            this.expanded_extras.remove(&id);
                        }
                        cx.notify();
                    })),
                    cx,
                )
            })
        };
        match extras {
            Some(extras) => v_flex()
                .w_full()
                .min_w_0()
                .child(row)
                .child(extras)
                .into_any_element(),
            None => row,
        }
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
            .selectable(true)
            .images(self.images.clone()),
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
            tool_text(h_flex())
                .id(("steer-tool-run", id as usize))
                .w_full()
                .min_w_0()
                .gap_2()
                .items_center()
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
                // EXP-785: the ONE caption every client derives from the
                // group's calls ("Ran 3 commands · edited 2 files · 1 failed").
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(SharedString::from(tool_group_caption(items))),
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
                if item.is_tool() {
                    // EXP-787: the group's INNER rhythm is unchanged — the
                    // 2px that used to live on `tool_row` itself (and gave
                    // the transcript its old row spacing) sits here now.
                    // EXP-746: an expanded group shows each call's cards
                    // too — that is what expanding it is for.
                    column = column.child(
                        div()
                            .pl_5()
                            .py_0p5()
                            .child(self.render_tool_item(item, true, cx)),
                    );
                }
            }
        } else if live_tail {
            // Collapsed but still running — keep the newest call visible.
            if let Some(item) = items.last().filter(|item| item.is_tool()) {
                column = column.child(
                    div()
                        .pl_5()
                        .py_0p5()
                        .child(self.render_tool_item(item, false, cx)),
                );
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
        // EXP-847: what the subagent DID, in the collapsed group's own words.
        // Only over rows this feed still holds — with none, the caption falls
        // back to the publisher's count.
        let tool_summary = items
            .iter()
            .any(|item| item.is_tool())
            .then(|| tool_group_caption(items));
        let running = matches!(
            items.iter().find_map(|item| match &item.kind {
                FeedKind::Subagent { status, .. } => Some(*status),
                _ => None,
            }),
            Some(SubagentStatus::Started)
        ) && !summary.done;

        let header = tool_text(h_flex())
            .id(("steer-subagent", id as usize))
            .w_full()
            .min_w_0()
            .gap_2()
            .items_center()
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
                    .text_color(cx.theme().foreground)
                    .child(SharedString::from(subagent_label(
                        summary.title.as_deref(),
                        &summary.agent_type,
                    ))),
            )
            .when_some(
                subagent_type_caption(summary.title.as_deref(), &summary.agent_type),
                |this, agent_type| {
                    this.child(
                        div()
                            .flex_shrink_0()
                            .text_2xs()
                            .child(SharedString::from(agent_type)),
                    )
                },
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
                        tool_summary.as_deref(),
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
            })
            // EXP-818: no per-row "Open" pill — the subagent strip over the
            // feed is the one way into a subagent's own conversation.
            .child(div().flex_1());
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
                    FeedKind::Tool { .. } => {
                        // EXP-787: the subagent's inner rhythm is unchanged.
                        // EXP-746: an expanded group shows each call's cards
                        // too — that is what expanding it is for.
                        column = column.child(
                            div()
                                .pl_5()
                                .py_0p5()
                                .child(self.render_tool_item(item, true, cx)),
                        );
                    }
                    // EXP-773: the subagent's own prose and the turns sent to
                    // it read exactly as they do on the main line, indented
                    // under the group instead of splitting it.
                    FeedKind::Narration { .. } | FeedKind::UserMessage { .. } => {
                        // EXP-787: the 4px the rows themselves used to carry,
                        // kept here so the nested conversation reads exactly
                        // as it did while the main line moved to the ladder.
                        column = column.child(
                            div()
                                .pl_5()
                                .py_1()
                                .child(self.render_item(item, &HashSet::new(), window, cx)),
                        );
                    }
                    _ => {}
                }
            }
        }
        column.into_any_element()
    }

    /// One `askId` group as a stepper card: the answered steps (re-openable
    /// while the ask is open, EXP-820), then the current one — or, while a
    /// next step is still owed, the waiting line.
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
        let complete = ask_complete(items);
        // The re-opened step, if it is one of THIS ask's rows.
        let editing = self
            .editing_card_id()
            .filter(|id| items.iter().any(|item| item.id == *id));
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
            if editing == Some(item.id) {
                card = card.child(self.render_editing_step(item, active, window, cx));
                continue;
            }
            card = card.child(self.render_answered_step(item, !complete, cx));
        }
        match current {
            // EXP-820: while an earlier step is re-opened, the current one
            // folds into a muted row; clicking it comes back.
            Some(item) if editing.is_some() => {
                let text = item.question().map(|card| card.text.clone()).unwrap_or_default();
                card = card.child(
                    h_flex()
                        .id(("steer-step-current", item.id as usize))
                        .w_full()
                        .min_w_0()
                        .gap_1p5()
                        .items_center()
                        .py_1()
                        .text_xs()
                        .text_color(muted)
                        .cursor_pointer()
                        .rounded(px(theme::tokens::radius::SM))
                        .hover(|this| this.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
                        .child(Icon::new(registry::UI_CHEVRON_RIGHT).xsmall().text_color(muted))
                        .child(div().flex_1().min_w_0().truncate().child(SharedString::from(text)))
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.stop_editing(window, cx);
                        })),
                );
            }
            Some(item) => {
                let text = item.question().map(|card| card.text.clone()).unwrap_or_default();
                card = card
                    .child(body_text(div()).w_full().min_w_0().child(self.render_body(
                        item.id,
                        &text,
                        cx,
                    )))
                    .child(self.render_prompt(item, active, submit_step, false, window, cx));
            }
            // EXP-820: the spinner only while a next step is actually owed —
            // a finished ask is just its answered rows.
            None if !complete => {
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
            None => {}
        }
        card.into_any_element()
    }

    /// EXP-820: the re-opened step — its prompt and options, answerable
    /// again, with the recorded answer drawn as the pick and a way back.
    fn render_editing_step(
        &self,
        item: &FeedItem,
        active: &HashSet<FeedItemId>,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let text = item.question().map(|card| card.text.clone()).unwrap_or_default();
        v_flex()
            .w_full()
            .min_w_0()
            .child(body_text(div()).w_full().min_w_0().child(self.render_body(
                item.id,
                &text,
                cx,
            )))
            .child(self.render_prompt(item, active, false, true, window, cx))
            .child(
                div().mt_1p5().child(
                    Button::new(("steer-step-back", item.id as usize))
                        .ghost()
                        .cursor_pointer()
                        .xsmall()
                        .text_color(cx.theme().muted_foreground)
                        .label("Back to current step")
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.stop_editing(window, cx);
                        })),
                ),
            )
            .into_any_element()
    }

    /// EXP-820: re-open an answered step (its ask must still be open).
    fn start_editing(
        &mut self,
        item_id: FeedItemId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.editing_step = Some(item_id);
        self.answer_cursor = None;
        self.answer_cursor_for = None;
        self.close_inline(window, cx);
        // The keyboard follows the re-opened step.
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn stop_editing(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.editing_step = None;
        self.answer_cursor = None;
        self.answer_cursor_for = None;
        self.close_inline(window, cx);
        cx.notify();
    }

    /// One answered (or in-flight) step as a line: check, question, answer.
    /// `open` = the ask is still open, so a resolved step is clickable to
    /// change its answer (EXP-820) and wears the edit glyph to say so.
    fn render_answered_step(
        &self,
        item: &FeedItem,
        open: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let card = item.question();
        let dismissed = card.is_some_and(|card| card.dismissed);
        // The freshest answer wins: a re-sent one is in the lock state until
        // its `question_resolved` rewrites the card.
        let in_flight = answer_key(item)
            .and_then(|key| self.feed.answer_state(&key))
            .filter(|state| state.is_locked())
            .map(|state| state.labels.join(", "))
            .filter(|labels| !labels.is_empty());
        let answer = in_flight
            .or_else(|| card.and_then(|card| card.answer.clone()))
            .unwrap_or_else(|| "Answered".to_string());
        let editable = open && self.step_editable(item);
        let item_id = item.id;
        h_flex()
            .id(("steer-step", item_id as usize))
            .w_full()
            .min_w_0()
            .gap_1p5()
            .items_center()
            .py_1()
            .text_xs()
            .when(editable, |this| {
                this.cursor_pointer()
                    .rounded(px(theme::tokens::radius::SM))
                    .hover(|this| this.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.start_editing(item_id, window, cx);
                    }))
            })
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
            .when(editable, |this| {
                this.child(Icon::new(registry::UI_EDIT).xsmall().text_color(muted))
            })
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
                body_text(div())
                    .w_full()
                    .min_w_0()
                    .child(if card.plan_mode {
                        self.render_unfolded_body(item.id, &card.text, cx)
                    } else {
                        self.render_body(item.id, &card.text, cx)
                    }),
            )
            .child(self.render_prompt(item, active, false, false, window, cx))
            .into_any_element()
    }

    /// The interactive half of a card (EXP-788): the options as a numbered
    /// list of full-width buttons, a multi-select's Submit, the lock and the
    /// resolution line. EXP-820: a free-text row (and a plan's reject row)
    /// unfolds its own inline field ([`Self::render_inline_answer`]) — the
    /// composer is hidden while the card is pending. `editing` renders a
    /// RESOLVED step as answerable again (the re-opened stepper step).
    fn render_prompt(
        &self,
        item: &FeedItem,
        active: &HashSet<FeedItemId>,
        submit_step: bool,
        editing: bool,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let Some(card) = item.question() else {
            return div().into_any_element();
        };
        let key = card.question_id.clone();
        let state = self.feed.answer_state(&key);
        let locked = state.is_some_and(|state| state.is_locked());
        let errored = state.is_some_and(|state| state.status == AnswerStatus::Error);

        if card.resolved && !editing {
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

        // A re-opened step still carries its first answer's lock state; that
        // lock is history (see `answer`), not a reason to show "Answering…".
        if locked && !editing {
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

        let answerable = (editing || active.contains(&item.id)) && self.answerable_run();
        if !answerable {
            let note = if card.plan_mode {
                "Waiting for approval. You're viewing read-only."
            } else {
                "Waiting for an answer. You're viewing read-only."
            };
            return v_flex()
                .mt_2()
                .gap_0p5()
                .children(card.options.iter().enumerate().map(|(index, option)| {
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .text_xs()
                        .text_color(muted)
                        .child(key_chip(index, false, cx))
                        .child(div().min_w_0().truncate().child(SharedString::from(option.label.clone())))
                }))
                .child(div().text_xs().text_color(muted).child(note))
                .into_any_element();
        }

        let picked = self.picked.get(&key).cloned().unwrap_or_default();
        // EXP-820: the re-opened step shows its recorded answer as the pick.
        let recorded = editing.then(|| card.answer.clone()).flatten();
        let promote_first = card.plan_mode || submit_step;
        let item_id = item.id;
        // Only THE pending card takes the keyboard; an older active card (a
        // second question the agent asked before the first was answered)
        // renders its chips dimmed and answers by click.
        let keyboard = self.pending_card_id() == Some(item_id);
        let cursor = self.answer_cursor_on(item_id);
        let inline = self
            .inline
            .as_ref()
            .filter(|inline| inline.item == item_id);
        let mut options = v_flex().mt_2().w_full().min_w_0().gap_1();
        for (index, option) in card.options.iter().enumerate() {
            let inline_here = inline.filter(|inline| inline.option_key == option.key);
            options = options.child(self.render_option(
                item_id,
                promote_first && index == 0,
                submit_step && index == 0,
                option,
                picked.contains(&option.key)
                    || recorded.as_deref() == Some(option.label.as_str())
                    || inline_here.is_some(),
                index,
                keyboard,
                cursor == Some(index),
                cx,
            ));
            if let Some(inline) = inline_here {
                options = options.child(self.render_inline_answer(
                    inline,
                    is_plan_reject(card, index),
                    window,
                    cx,
                ));
            }
        }

        let mut column = v_flex().w_full().min_w_0().child(options);
        if card.multi_select {
            column = column.child(
                div().mt_1p5().child(
                    Button::new(("steer-answer-submit", item_id as usize))
                        .with_variant(ButtonVariant::Secondary)
                        .cursor_pointer()
                        .xsmall()
                        .label("Submit")
                        .disabled(picked.is_empty())
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            this.submit_picks(item_id, cx);
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

    /// EXP-820: the inline free-text field under its option row — the glass
    /// text field plus the composer's round Send. `always_sendable` is the
    /// plan reject row, where an empty send is a plain reject; a question's
    /// free-text row needs text.
    fn render_inline_answer(
        &self,
        inline: &InlineAnswer,
        always_sendable: bool,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let has_text = !inline.state.read(cx).value().trim().is_empty();
        h_flex()
            .key_context("SteerInlineAnswer")
            .w_full()
            .min_w_0()
            .gap_1()
            .items_center()
            // Indented under the row's label (chip 18px + gap 8px).
            .pl(px(26.))
            .capture_action(cx.listener(Self::on_inline_escape))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(crate::controls::glass_input(&inline.state, window, cx).small()),
            )
            .child(
                crate::composer::composer_submit_kind(
                    ("steer-inline-send", inline.item as usize),
                    crate::composer::SubmitKind::Send,
                    !always_sendable && !has_text,
                    cx,
                )
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.send_inline(window, cx);
                })),
            )
            .into_any_element()
    }

    /// One option row (EXP-788): a full-width button carrying its numbered
    /// key chip, its label and — under the label — its description. EXP-820
    /// styleguide: NO blue. The promoted option (a plan card's "Yes", the
    /// stepper's submit) is the app's PRIMARY button; a picked row (a
    /// multi-select pick, the recorded answer of a re-opened step, the row
    /// whose inline field is open) is the glass active fill under the active
    /// stroke; the keyboard highlight is the strong stroke on the hairline,
    /// so ↑/↓ read as a cursor and not as a second selection.
    #[allow(clippy::too_many_arguments)] // one call site; every flag is a render decision
    fn render_option(
        &self,
        item_id: FeedItemId,
        primary: bool,
        submit_label: bool,
        option: &QuestionOption,
        picked: bool,
        index: usize,
        keyboard: bool,
        highlighted: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let label = if submit_label {
            "Submit answers".to_string()
        } else {
            option.label.clone()
        };
        let label_color = if primary {
            cx.theme().primary_foreground
        } else {
            cx.theme().foreground
        };
        let mut button = Button::new(("steer-option", item_id as usize * 64 + index))
            .cursor_pointer()
            .w_full()
            .h_auto()
            .px_2()
            .py_1p5()
            .rounded(px(theme::tokens::radius::MD));
        button = if primary {
            button.with_variant(ButtonVariant::Primary)
        } else if picked {
            let fill = theme::tokens::glass::FILL_ACTIVE.to_hsla();
            // Hover/pressed step one rung up the same white ladder.
            let raised = theme::tokens::glass::STROKE_ACTIVE.to_hsla();
            button
                .custom(
                    gpui_component::button::ButtonCustomVariant::new(cx)
                        .color(fill)
                        .hover(raised)
                        .active(raised)
                        .foreground(cx.theme().foreground),
                )
                .border_color(theme::tokens::glass::STROKE_ACTIVE.to_hsla())
        } else {
            button.outline()
        };
        if highlighted {
            button = button.border_color(theme::tokens::glass::STROKE_STRONG.to_hsla());
        }
        let description = option.description.clone().filter(|text| !text.trim().is_empty());
        button
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .items_start()
                    .child(div().mt_px().child(key_chip(index, keyboard && !primary, cx)))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .items_start()
                            .gap_0p5()
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .text_left()
                                    .text_xs()
                                    .text_color(label_color)
                                    .child(SharedString::from(label)),
                            )
                            .when_some(description, |this, description| {
                                this.child(
                                    div()
                                        .w_full()
                                        .min_w_0()
                                        .text_left()
                                        .text_2xs()
                                        .text_color(if primary {
                                            label_color.opacity(0.8)
                                        } else {
                                            muted
                                        })
                                        .child(SharedString::from(description)),
                                )
                            }),
                    ),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                this.pick_option(item_id, index, window, cx);
            }))
            .into_any_element()
    }

    // ── EXP-789: the subagent strip ────────────────────────────────────────

    /// The subagent whose tab is in focus, honoured only while its tab is
    /// still visible (web `activeAgent`): a done subagent's tab lingers
    /// exactly as long as it stays focused, then Main takes over.
    fn active_subagent(&self) -> Option<String> {
        let focused = self.focused_subagent.as_deref()?;
        let agents = self.feed.subagents();
        steer::feed::visible_subagent_tabs(&agents, Some(focused))
            .iter()
            .any(|agent| agent.subagent_id == focused)
            .then(|| focused.to_string())
    }

    /// Focus a subagent's tab (`None` = Main). The list re-projects on the
    /// next frame and opens on the newest rows of the conversation.
    fn focus_subagent(&mut self, subagent_id: Option<String>, cx: &mut gpui::Context<Self>) {
        if self.focused_subagent == subagent_id {
            return;
        }
        self.focused_subagent = subagent_id;
        self.window_from = None;
        self.list.set_follow_mode(FollowMode::Tail);
        self.list.scroll_to_end();
        cx.notify();
    }

    /// "Main" plus one tab per RUNNING subagent (and the focused one, done or
    /// not); nothing at all while no subagent is running. Under the tabs the
    /// focused conversation's summary line — its type, liveness and detail —
    /// the header the web `AgentConversation` paints over the stream.
    fn render_subagent_strip(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let muted = cx.theme().muted_foreground;
        let active = self.active_subagent();
        let agents = self.feed.subagents();
        let tabs = steer::feed::visible_subagent_tabs(&agents, active.as_deref());
        if tabs.is_empty() {
            return None;
        }
        let mut strip = h_flex()
            .w_full()
            .flex_shrink_0()
            .flex_wrap()
            .gap_1()
            .items_center()
            .px_2()
            .py_1()
            .child(
                crate::surface::glass_pill(
                    "steer-subagent-tab-main",
                    crate::surface::PillSize::Sm,
                    crate::surface::PillMode::Select {
                        selected: active.is_none(),
                    },
                    cx,
                )
                .child("Main")
                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.focus_subagent(None, cx);
                })),
            );
        for agent in &tabs {
            let selected = active.as_deref() == Some(agent.subagent_id.as_str());
            let subagent_id = agent.subagent_id.clone();
            strip = strip.child(
                crate::surface::glass_pill(
                    SharedString::from(format!("steer-subagent-tab-{}", agent.subagent_id)),
                    crate::surface::PillSize::Sm,
                    crate::surface::PillMode::Select { selected },
                    cx,
                )
                .when(!agent.done, |this| this.child(Spinner::new().xsmall()))
                .child(SharedString::from(subagent_label(
                    agent.title.as_deref(),
                    &agent.agent_type,
                )))
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    this.focus_subagent(Some(subagent_id.clone()), cx);
                })),
            );
        }
        let summary = active
            .as_deref()
            .and_then(|id| agents.iter().find(|agent| agent.subagent_id == id));
        let column = v_flex()
            .w_full()
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            .child(strip)
            .when_some(summary, |this, summary| {
                this.child(
                    tool_text(h_flex())
                        .w_full()
                        .min_w_0()
                        .gap_2()
                        .items_center()
                        .px_3()
                        .pb_1p5()
                        .text_color(muted)
                        .child(Icon::new(registry::CODING_SUBAGENT).xsmall())
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(cx.theme().foreground)
                                .child(SharedString::from(subagent_label(
                                    summary.title.as_deref(),
                                    &summary.agent_type,
                                ))),
                        )
                        .when_some(
                            subagent_type_caption(summary.title.as_deref(), &summary.agent_type),
                            |this, agent_type| {
                                this.child(
                                    div()
                                        .flex_shrink_0()
                                        .text_2xs()
                                        .child(SharedString::from(agent_type)),
                                )
                            },
                        )
                        .when(!summary.done, |this| this.child(Spinner::new().xsmall()))
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_2xs()
                                .child(SharedString::from(subagent_caption(
                                    summary.done,
                                    summary.tool_count,
                                    // EXP-849: the projection carries the
                                    // `toolGroupSummary` now, so the strip says
                                    // what the subagent DID instead of how many
                                    // calls it made — the same caption its
                                    // inline group row wears.
                                    summary.tool_summary.as_deref(),
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
                        }),
                )
            });
        Some(column.into_any_element())
    }

    /// EXP-831: whether the status stack currently shows a rate-limit wall
    /// — the stale tick repaints while one is up, so its countdown moves and
    /// it drops itself once the reset is behind us.
    fn rate_limit_wall_showing(&self, now_ms: i64) -> bool {
        self.feed.rate_limit().is_some_and(|limit| {
            rate_limit_is_wall(&limit.status, limit.message.as_deref())
                && !steer::rate_limit_expired(limit.resets_at, now_ms)
        })
    }

    /// EXP-784: the agent's rate-limit report as a banner in the status
    /// stack — its message (or a generic one) plus the local reset time when
    /// it named one. `None` once the slot cleared, and (EXP-831) once the
    /// reset it named is behind us: the engine lifts the slot on the run's
    /// next activity, but a banner must not outlive its own reset over a
    /// run that is visibly working.
    fn render_rate_limit_banner(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let limit = self.feed.rate_limit()?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        if !rate_limit_is_wall(&limit.status, limit.message.as_deref())
            || steer::rate_limit_expired(limit.resets_at, now_ms)
        {
            return None;
        }
        let amber = theme::tokens::YELLOW.to_hsla();
        let caption = rate_limit_caption(
            limit.message.as_deref(),
            limit
                .resets_at
                .map(|at| crate::usage_bar::countdown_from_secs((at - now_ms) / 1000)),
        );
        Some(
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
                    Icon::new(registry::UI_WARNING)
                        .xsmall()
                        .text_color(amber.opacity(0.8)),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(caption)),
                )
                .into_any_element(),
        )
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
        // this machine (it ran elsewhere, or its journal is gone), and an
        // ended session's relay room is gone.
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
        let stop = self.shows_stop(cx);
        // EXP-790: the tool row's wrap decision, with hysteresis so a width
        // hovering around the threshold does not flap between layouts.
        // The first frame has no measurement yet (0px): keep the tools
        // inline rather than flashing a wrapped row before the probe lands.
        let width = f32::from(self.composer_width.get());
        let wrapped = width > 0.
            && tool_row_wraps(width, COMPOSER_INLINE_MIN_WIDTH, self.tools_wrapped.get());
        self.tools_wrapped.set(wrapped);
        let width_probe = self.composer_width.clone();
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
                        // The captures run before the field's own handlers,
                        // so with a `/` menu open Enter/Tab accept and never
                        // reach the textarea (so `PressEnter`, the thing that
                        // SENDS, never fires for them). EXP-820: the card
                        // keyboard's captures sit on the view's ROOT, since
                        // the composer is hidden while a card is pending.
                        .key_context("SteerComposer")
                        .w_full()
                        .min_w_0()
                        .capture_action(cx.listener(Self::on_slash_up))
                        .capture_action(cx.listener(Self::on_slash_down))
                        .capture_action(cx.listener(Self::on_slash_escape))
                        .capture_action(cx.listener(Self::on_slash_enter))
                        .capture_action(cx.listener(Self::on_slash_tab))
                        // EXP-790: the mention-capable field — `@`, `#` and
                        // `:` complete exactly as they do in a comment.
                        .child(self.mention.clone()),
                )
                .into_any_element(),
        )
        .inline_tools(!wrapped)
        .strip((!self.pending_images.is_empty()).then(|| self.render_pending_strip(cx)))
        // EXP-698: the attach tool is ALWAYS offered — steer images upload to
        // the session route, so a batch/action run (no issue at all) attaches
        // exactly like an issue run. EXP-818: its glyph is `editor-image`,
        // the image glyph every other composer (comments, the description
        // editor) wears — web, iOS and Android swapped with it.
        .tool(
            crate::composer::composer_tool("steer-attach", registry::EDITOR_IMAGE, cx)
                .tooltip("Attach image")
                .disabled(self.sending)
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.pick_images(window, cx);
                })),
        );
        // EXP-790: ONE round button — Stop while the agent works, Send
        // otherwise (and always once there is a draft). Same ring, the glyph
        // swaps. No tooltip — a floating label beside a circled glyph reads
        // as a second button (EXP-698).
        let kind = if stop {
            crate::composer::SubmitKind::Stop
        } else {
            crate::composer::SubmitKind::Send
        };
        let composer = composer.submit(
            crate::composer::composer_submit_kind("steer-send", kind, !stop && !can_send, cx)
                .loading(self.sending)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    if stop {
                        this.stop_turn(cx);
                    } else {
                        this.send(window, cx);
                    }
                })),
        );
        div()
            .w_full()
            .flex_shrink_0()
            .p_2()
            .border_t_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            .child(
                div()
                    .relative()
                    .w_full()
                    .min_w_0()
                    .child(
                        crate::composer::glass_composer(composer)
                            .capture_action(cx.listener(Self::on_paste)),
                    )
                    // The card's width, read back for next frame's wrap
                    // decision (the canvas paints nothing).
                    .child(
                        gpui::canvas(
                            move |bounds, _, _| width_probe.set(bounds.size.width),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    ),
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
        self.pending_images
            .render_strip("steer-pending-remove", self.sending, Self::remove_pending, cx)
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

fn status_dot(color: gpui::Hsla) -> impl IntoElement {
    div()
        .flex_shrink_0()
        .size_1p5()
        .rounded_full()
        .bg(color)
}

/// EXP-787 — the transcript's PROSE rung: narration, a sent message, a card's
/// body. 14/22 from the shared tokens, on every client; the rem-relative
/// `text_sm` these used to take resolved to 12.25px here and to something else
/// on web, so a run read differently per platform.
fn body_text<E: Styled>(element: E) -> E {
    element
        .text_size(px(transcript::BODY_SIZE))
        .line_height(px(transcript::BODY_LINE_HEIGHT))
}

/// EXP-787 — the transcript's TOOL rung: tool rows, group captions, permission
/// rows, the compaction divider, "Working…". 12/18. The 11px `text_2xs`
/// captions that hang UNDER one of these (a tool's argument, a subagent's
/// status) stay where EXP-698 put them.
fn tool_text<E: Styled>(element: E) -> E {
    element
        .text_size(px(transcript::TOOL_SIZE))
        .line_height(px(transcript::TOOL_LINE_HEIGHT))
}

/// One tool call's line. EXP-789: a call whose `tool_update` said `failed`
/// tints its name and glyph rose (the web `text-rose-400` row) — the detail
/// stays muted, so the failure reads at a glance without shouting.
fn tool_row(name: &str, detail: Option<&str>, failed: bool, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let rose = cx.theme().danger;
    tool_text(h_flex())
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .child(
            Icon::new(registry::CODING_TOOL)
                .xsmall()
                .text_color(if failed { rose.opacity(0.8) } else { muted.opacity(0.6) }),
        )
        .child(
            div()
                .flex_shrink_0()
                .when(failed, |this| this.text_color(rose))
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

/// EXP-846 — one of OUR MCP calls, as a row.
///
/// A tool whose name resolves to a contract `expToolNames` row
/// (`steer::exp_tool_display`) is the agent working on the PRODUCT, and it reads
/// as such: the Exponential mark instead of the generic tool glyph, the
/// contract's own caption (progressive while it runs, past tense once it
/// settles), the call's subject, and — once settled — a small preview of what it
/// answered with ([`steer::frames::ToolPreview`], published by whoever hosts the
/// run). A FAILED call keeps the generic failed styling and previews nothing:
/// there is no subject to show.
///
/// Every part is optional: the mark plus the caption is a complete row, and a
/// tool that answers `none` never gets more. Mirrored ×4.
fn exp_tool_call_row(
    id: FeedItemId,
    display: steer::ExpToolDisplay,
    detail: Option<&str>,
    failed: bool,
    preview: Option<&steer::frames::ToolPreview>,
    cx: &mut App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let rose = cx.theme().danger;
    let header = tool_text(h_flex())
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .child(
            // The app's own mark (`assets/icons/logo.svg`), at glyph size.
            Icon::from(crate::icons::ExpIcon::Logo)
                .xsmall()
                .text_color(if failed { rose.opacity(0.8) } else { muted }),
        )
        .child(
            div()
                .flex_shrink_0()
                .when(failed, |this| this.text_color(rose))
                .child(SharedString::from(display.caption)),
        )
        .when_some(detail, |this, detail| {
            this.child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_2xs()
                    .text_color(muted)
                    .child(SharedString::from(detail.to_string())),
            )
        });
    let body = (!failed)
        .then(|| preview.and_then(|preview| exp_tool_preview_row(id, display.result, preview, cx)))
        .flatten();
    match body {
        Some(body) => v_flex()
            .w_full()
            .min_w_0()
            .gap_1()
            .child(header)
            .child(div().pl_5().child(body))
            .into_any_element(),
        None => header.into_any_element(),
    }
}

/// EXP-846 — the settled preview under an Exponential tool row, by contract
/// result kind:
///
/// * `issue` — the SAME hover card an issue-ref pill shows
///   ([`crate::issue_preview::card`]), clickable straight into the issue; an
///   issue that has not synced here (another team's board, a trashed one)
///   degrades to the identifier and title the tool itself reported;
/// * `pr` — a link row opening the pull request in the browser;
/// * `list` — "N results";
/// * `session` / `board` / `action` / `automation` / `comment` — a name chip;
/// * `none` — nothing: the caption said it all.
fn exp_tool_preview_row(
    id: FeedItemId,
    result: &str,
    preview: &steer::frames::ToolPreview,
    cx: &mut App,
) -> Option<AnyElement> {
    use steer::exp_tool::result as kind;
    let muted = cx.theme().muted_foreground;
    match result {
        kind::ISSUE => {
            let issue_id = preview.id.clone()?;
            if let Some(card) = crate::issue_preview::card(&issue_id, cx) {
                return Some(
                    div()
                        .id(("steer-exp-issue", id as usize))
                        .cursor_pointer()
                        .on_click(move |_, window, cx| {
                            crate::navigation::navigate(
                                window,
                                cx,
                                crate::navigation::Screen::IssueDetail {
                                    issue_id: issue_id.clone(),
                                },
                            );
                        })
                        .child(card)
                        .into_any_element(),
                );
            }
            // Not synced here: what the answer itself named.
            let label = exp_preview_label(preview)?;
            Some(exp_preview_chip(registry::NAV_ISSUES, label, cx))
        }
        kind::PR => {
            let url = preview.url.clone().filter(|url| !url.is_empty())?;
            let label = preview
                .identifier
                .clone()
                .filter(|identifier| !identifier.is_empty())
                .unwrap_or_else(|| url.clone());
            Some(
                tool_text(h_flex())
                    .id(("steer-exp-pr", id as usize))
                    .min_w_0()
                    .gap_1p5()
                    .items_center()
                    .cursor_pointer()
                    .text_color(muted)
                    .child(Icon::new(registry::PR_OPEN).xsmall())
                    .child(div().min_w_0().truncate().text_2xs().child(SharedString::from(label)))
                    .child(Icon::new(registry::UI_EXTERNAL_LINK).xsmall())
                    .on_click(move |_, _, cx| crate::settings::open_url(cx, url.clone()))
                    .into_any_element(),
            )
        }
        kind::LIST => preview
            .count
            .map(|count| {
                div()
                    .text_2xs()
                    .text_color(muted)
                    .child(SharedString::from(exp_tool_result_count(count)))
                    .into_any_element()
            }),
        kind::NONE => None,
        // session / board / action / automation / comment: a name chip.
        _ => exp_preview_label(preview).map(|label| exp_preview_chip(registry::CODING_TOOL, label, cx)),
    }
}

/// What a non-issue preview is CALLED: its human identifier, else its title.
fn exp_preview_label(preview: &steer::frames::ToolPreview) -> Option<String> {
    preview
        .identifier
        .clone()
        .or_else(|| preview.title.clone())
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty())
}

/// "17 results" — a LIST answer's preview (`1 result` in the singular).
fn exp_tool_result_count(count: u32) -> String {
    match count {
        1 => "1 result".to_string(),
        n => format!("{n} results"),
    }
}

/// The quiet chip a non-issue preview renders as.
fn exp_preview_chip(icon: crate::icons::ExpIcon, label: String, cx: &App) -> AnyElement {
    tool_text(h_flex())
        .min_w_0()
        .gap_1p5()
        .items_center()
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(icon).xsmall())
        .child(div().min_w_0().truncate().text_2xs().child(SharedString::from(label)))
        .into_any_element()
}

impl Focusable for SteerSessionView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SteerSessionView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // EXP-776: the transcript list learns what changed since the last
        // frame here, before anything reads the cached projection.
        self.sync_list(cx);
        self.sync_card_focus(window, cx);
        let header = self.chrome.then(|| self.render_header(cx));
        // EXP-789: the subagent strip sits between the header and the feed.
        let strip = self.render_subagent_strip(cx);
        let feed = self.render_feed(cx);
        let banners = self.render_banners(cx);
        let composer_visible = self.composer_visible();
        // EXP-724: between the banners and the composer, exactly where the
        // web view puts it — and gone with the composer once the run ends.
        let compacting = (composer_visible && self.feed.compacting().is_some())
            .then(|| self.render_compaction_strip(cx));
        // EXP-784: the agent's rate-limit report, beside the compaction
        // strip in the same status stack; gone the moment it clears.
        let rate_limit = composer_visible
            .then(|| self.render_rate_limit_banner(cx))
            .flatten();
        let composer = composer_visible.then(|| self.render_composer(cx));
        // EXP-773: the Changes bar sits between the transcript and the
        // composer, exactly where the web view puts it — an ended run keeps
        // it (the work is what the reader came for), it only loses the Merge.
        let changes = self.render_changes_bar(cx);
        v_flex()
            .key_context("SteerSession")
            .track_focus(&self.focus_handle)
            // EXP-820: the card keyboard lives on the ROOT — it has to work
            // with the composer hidden (a card pending) and while the
            // composer or the inline field is focused (both are descendants;
            // the handlers yield to a field that wants the keys).
            .capture_action(cx.listener(Self::on_answer_up))
            .capture_action(cx.listener(Self::on_answer_down))
            .capture_action(cx.listener(Self::on_answer_enter))
            .capture_key_down(cx.listener(Self::on_answer_key_down))
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .children(header)
            .children(strip)
            .child(feed)
            .children(banners)
            .children(compacting)
            .children(rate_limit)
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

    /// EXP-847: the chip names the JOB when the spawning call said one, and
    /// falls back to the agent type otherwise; the type then rides a secondary
    /// caption, and never twice.
    #[test]
    fn a_subagent_chip_prefers_the_spawning_calls_title() {
        assert_eq!(
            subagent_label(Some("  Audit the shape proxies  "), "explore"),
            "Audit the shape proxies"
        );
        assert_eq!(subagent_label(None, " explore "), "explore");
        assert_eq!(subagent_label(Some("   "), "explore"), "explore");
        // The type is a caption ONLY beside a title.
        assert_eq!(
            subagent_type_caption(Some("Audit the shape proxies"), "explore").as_deref(),
            Some("explore")
        );
        assert_eq!(subagent_type_caption(None, "explore"), None);
        // …and never a repeat of the title it sits beside.
        assert_eq!(subagent_type_caption(Some("explore"), "explore"), None);
        assert_eq!(subagent_type_caption(Some("Audit"), "  "), None);
    }

    /// EXP-848: the turn slot is the AUTHORITY and everything else is a veto
    /// — and the default (`turn_working: false`) never pulses.
    #[test]
    fn the_working_predicate_needs_a_turn_in_flight_and_no_veto() {
        let working = || WorkingFacts {
            empty_feed: false,
            live: true,
            row_ended: false,
            turn_working: true,
            awaiting_input: false,
            needs_input: false,
            staging: false,
            blocked: false,
            compacting: false,
        };
        assert!(is_working(&working()));
        // No turn in flight is the common case: a live run between turns.
        assert!(!is_working(&WorkingFacts { turn_working: false, ..working() }));
        // Every veto, one at a time.
        assert!(!is_working(&WorkingFacts { empty_feed: true, ..working() }));
        assert!(!is_working(&WorkingFacts { live: false, ..working() }));
        assert!(!is_working(&WorkingFacts { row_ended: true, ..working() }));
        assert!(!is_working(&WorkingFacts { awaiting_input: true, ..working() }));
        assert!(!is_working(&WorkingFacts { needs_input: true, ..working() }));
        assert!(!is_working(&WorkingFacts { staging: true, ..working() }));
        assert!(!is_working(&WorkingFacts { blocked: true, ..working() }));
        assert!(!is_working(&WorkingFacts { compacting: true, ..working() }));
    }

    /// FEED-26 + EXP-848: the idle caption and a pulsing "Working…" are
    /// mutually exclusive by construction — `stale_minutes` asks the same
    /// predicate, so a mid-turn run never reads as stalled.
    #[test]
    fn the_idle_caption_never_sits_under_a_working_run() {
        let mid_turn = WorkingFacts {
            empty_feed: false,
            live: true,
            row_ended: false,
            turn_working: true,
            awaiting_input: false,
            needs_input: false,
            staging: false,
            blocked: false,
            compacting: false,
        };
        assert!(is_working(&mid_turn), "the run the header must NOT call stale");
        // Quiet with no turn in flight is the real FEED-26 case, and the
        // caption is free to name it.
        assert!(!is_working(&WorkingFacts { turn_working: false, ..mid_turn }));
        assert_eq!(
            phase_label(&ViewerPhase::Live, Some("macbook"), false, false, Some(27)),
            "No activity for 27 min · macbook"
        );
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

    // ── EXP-820: ask completion, the card keyboard, inline rows ────────────

    fn step(id: FeedItemId, ask: &str, index: Option<u32>, total: u32, resolved: bool) -> FeedItem {
        FeedItem {
            id,
            kind: FeedKind::Question(steer::QuestionCard {
                text: format!("q{id}"),
                question_id: match index {
                    Some(n) => format!("{ask}#{n}"),
                    None => format!("{ask}#submit"),
                },
                ask_id: Some(ask.to_string()),
                index,
                total: Some(total),
                resolved,
                ..Default::default()
            }),
            seq: None,
        }
    }

    /// The submit step resolving ends the ask; an answered numbered step
    /// alone does not — the next step (or the submit step) is still owed.
    #[test]
    fn an_ask_completes_on_its_resolved_submit_step() {
        let a = step(1, "ask", Some(1), 3, true);
        let b = step(2, "ask", Some(2), 3, true);
        let c = step(3, "ask", Some(3), 3, true);
        assert!(!ask_complete(&[&a]));
        assert!(!ask_complete(&[&a, &b, &c]), "every step answered, submit still owed");
        let submit_open = step(4, "ask", None, 3, false);
        assert!(!ask_complete(&[&a, &b, &c, &submit_open]));
        let submit_done = step(4, "ask", None, 3, true);
        assert!(ask_complete(&[&a, &b, &c, &submit_done]));
    }

    /// A one-question ask has no submit step: its lone step answered IS the
    /// end (the bug: the stepper kept "Waiting for the next question…").
    #[test]
    fn a_lone_step_ask_completes_when_its_step_resolves() {
        let open = step(1, "ask", Some(1), 1, false);
        assert!(!ask_complete(&[&open]));
        let done = step(1, "ask", Some(1), 1, true);
        assert!(ask_complete(&[&done]));
        // `total` missing falls back to the step count.
        let mut untotaled = step(1, "ask", Some(1), 1, true);
        untotaled.question_mut().unwrap().total = None;
        assert!(ask_complete(&[&untotaled]));
        // No numbered step at all (a submit step alone, still open) is not
        // vacuously complete.
        let lone_submit = step(1, "ask", None, 0, false);
        assert!(!ask_complete(&[&lone_submit]));
        assert!(!ask_complete(&[]));
    }

    /// A dismissed step ends the ask whatever else is open.
    #[test]
    fn a_dismissed_step_completes_the_ask() {
        let a = step(1, "ask", Some(1), 3, true);
        let mut b = step(2, "ask", Some(2), 3, true);
        b.question_mut().unwrap().dismissed = true;
        assert!(ask_complete(&[&a, &b]));
    }

    /// The keyboard drives the card only while no field wants the keys: the
    /// inline answer field always wins, the composer once it has a draft or
    /// an open `/` menu; with the composer hidden the view holds the keys.
    #[test]
    fn the_card_keyboard_yields_to_a_field_that_wants_the_keys() {
        // (pending, inline_open, composer_visible, draft_empty, slash_open)
        assert!(!keyboard_drives_card(false, false, false, true, false), "no card");
        assert!(keyboard_drives_card(true, false, false, true, false), "composer hidden");
        assert!(
            keyboard_drives_card(true, false, false, false, false),
            "a stale draft never blocks a hidden composer"
        );
        assert!(!keyboard_drives_card(true, true, false, true, false), "inline field open");
        assert!(keyboard_drives_card(true, false, true, true, false), "composer shown, empty");
        assert!(!keyboard_drives_card(true, false, true, false, false), "composer has a draft");
        assert!(!keyboard_drives_card(true, false, true, true, true), "slash menu open");
    }

    fn option(label: &str, free_text: bool) -> QuestionOption {
        QuestionOption {
            label: label.to_string(),
            key: label.to_lowercase(),
            description: None,
            free_text,
        }
    }

    /// Which rows open the inline field instead of answering: a free-text
    /// row and a plan card's LAST option, never a multi-select's rows.
    #[test]
    fn inline_rows_are_the_free_text_row_and_the_plan_reject() {
        let question = steer::QuestionCard {
            options: vec![option("Yes", false), option("Type something.", true)],
            ..Default::default()
        };
        assert_eq!(inline_placeholder(&question, 0), None);
        assert_eq!(inline_placeholder(&question, 1), Some(FREE_TEXT_PLACEHOLDER));
        assert_eq!(inline_placeholder(&question, 2), None, "out of range");

        let plan = steer::QuestionCard {
            plan_mode: true,
            options: vec![
                option("Yes, and auto-accept edits", false),
                option("Yes, and manually approve edits", false),
                option("No, keep planning", false),
            ],
            ..Default::default()
        };
        assert_eq!(inline_placeholder(&plan, 0), None);
        assert_eq!(inline_placeholder(&plan, 1), None);
        assert_eq!(inline_placeholder(&plan, 2), Some(PLAN_REJECT_PLACEHOLDER));
        assert!(is_plan_reject(&plan, 2));
        assert!(!is_plan_reject(&question, 1));

        let multi = steer::QuestionCard {
            multi_select: true,
            options: vec![option("A", false), option("Type something.", true)],
            ..Default::default()
        };
        assert_eq!(inline_placeholder(&multi, 1), None, "multi-select rows only toggle");
    }

    #[test]
    fn the_ask_counter_hides_itself_for_a_single_step() {
        assert_eq!(ask_counter(Some(2), 3).as_deref(), Some("2 of 3"));
        assert_eq!(ask_counter(None, 3).as_deref(), Some("3 questions"));
        assert_eq!(ask_counter(Some(1), 1), None);
        assert_eq!(ask_counter(None, 0), None);
    }

    /// EXP-847: the caption says what the subagent DID (the collapsed group's
    /// own `toolGroupSummary`, its first letter joined into the sentence). With
    /// no rows left to summarise it falls back to the publisher's count, which
    /// pluralizes (the tool-RUN group never has to — it only forms at two or
    /// more).
    #[test]
    fn the_subagent_caption_reads_the_tool_group_summary() {
        assert_eq!(
            subagent_caption(false, 3, Some("Read 3 files")),
            "running · read 3 files"
        );
        assert_eq!(
            subagent_caption(true, 4, Some("Ran 2 commands · edited 2 files · 1 failed")),
            "done · ran 2 commands · edited 2 files · 1 failed"
        );
        // A blank summary is no summary at all.
        assert_eq!(subagent_caption(true, 0, Some("  ")), "done");
        // EXP-748: the replay whose rows were evicted keeps the count.
        assert_eq!(subagent_caption(false, 0, None), "running");
        assert_eq!(subagent_caption(false, 1, None), "running · 1 tool call");
        assert_eq!(subagent_caption(true, 7, None), "done · 7 tool calls");
    }

    /// EXP-846: a LIST answer's preview counts, and pluralizes.
    #[test]
    fn a_list_result_preview_counts_its_rows() {
        assert_eq!(exp_tool_result_count(0), "0 results");
        assert_eq!(exp_tool_result_count(1), "1 result");
        assert_eq!(exp_tool_result_count(17), "17 results");
    }

    /// EXP-846: a non-issue preview is NAMED by its identifier, its title
    /// second, and nothing when the answer named neither (the caption then
    /// stands alone).
    #[test]
    fn a_preview_is_named_by_its_identifier_then_its_title() {
        let preview = |identifier: Option<&str>, title: Option<&str>| steer::frames::ToolPreview {
            identifier: identifier.map(str::to_string),
            title: title.map(str::to_string),
            ..Default::default()
        };
        assert_eq!(
            exp_preview_label(&preview(Some("EXP-42"), Some("Fix it"))).as_deref(),
            Some("EXP-42")
        );
        assert_eq!(
            exp_preview_label(&preview(None, Some("  Release train  "))).as_deref(),
            Some("Release train")
        );
        assert_eq!(exp_preview_label(&preview(Some("  "), Some(" "))), None);
        assert_eq!(exp_preview_label(&steer::frames::ToolPreview::default()), None);
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
