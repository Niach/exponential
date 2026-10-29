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
//! * **Row truth** — the synced `sessions` row (identity, device,
//!   `needs_input`, `status`). The view observes the collection and feeds
//!   `ended` back to the socket with
//!   [`ViewerHandle::note_session_ended`] so its redial loops stop.
//! * **Wakeups** — the three edges the web store's `kickAll` listens on
//!   (foreground, network, host device online) are wired here to
//!   [`ViewerHandle::kick`]; see the "Wakeups" section below. Without them a
//!   woken laptop waits out the transport's staleness window and backoff.
//!
//! EXP-1294: the view is the Session screen's body and ALWAYS the full
//! transcript (no turn thread, no Show work, no Guide or diff pages — the
//! Guide is the thread's tab). The database is the bus: the composer posts
//! a `messages` row (`sessions.send`), a card is answered in its row
//! (`sessions.answer`), Stop is `sessions.stop`; the live layer (local
//! engine, relay) stays what the transcript is READ from.
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
//! ([`ask_complete`]). EXP-850 added the derived rows around that
//! conversation — the §9 pending-card-last projection, the §3 workflow card,
//! the §4 duplicate warning, the §5 working caption, the §1/§2 strip and the
//! §12 file cards — whose RULES are pure in [`crate::session_rows`] and
//! `steer::work_card` (EXP-1240: the workflow card, the task list and the
//! subagent rows are ONE Work card, painted by [`crate::work_card`]).
//! The only deliberate gap left: no fullscreen toggle —
//! the screen's own chrome is the desktop's answer to that.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    div, list, prelude::FluentBuilder as _, px,
    AnyElement, App, AppContext as _, ClickEvent, ElementId, Entity, FocusHandle,
    Focusable, FollowMode, InteractiveElement as _, IntoElement, ListAlignment, ListState,
    ParentElement as _, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Task, Window,
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
use steer::feed::{
    COMPACTED_LABEL, COMPACTING_LABEL, COMPACTION_TIMEOUT, QUEUE_STRIP_TITLE,
};
use steer::{
    answer_key, build_steer_image_message, parse_steer_message,
    summarize_subagent_row, transcript_gap, AnswerStatus, FeedItem,
    FeedItemId, FeedKind, FeedRow, FeedRowSpec, Gap, QuestionOption, RowClass, SteerFeed,
    SubagentStatus, ViewerEvent, ViewerHandle, ViewerPhase, ANSWER_ACK_TIMEOUT,
    REPLAY_MAX, REPLAY_QUIET,
};
use theme::tokens::transcript;

use crate::composer_images::{self, PendingImages};
use crate::controls::{text_button, TextButtonVariant, WebText as _};
use crate::icons::registry;
use crate::native_dialog::{self, AlertSpec};
use crate::slash_commands;
use crate::trace::{trace_collapsible, trace_content, trace_line, trace_link, trace_stack, TraceLine, TraceSize};
use crate::transcript_rows::{self, facet, plan_list_sync, ItemFacets, ListOp, RowKey, ORPHAN_ROW_BASE, WORKING_ROW_ID};

/// EXP-1228: the transcript menu's Copy row — copies the window text
/// selection exactly like ⌘C/Ctrl+C does over the transcript. EXP-1266: the
/// text is snapshotted when the menu OPENS and rides the action, whose
/// handler is App-global ([`ensure_copy_action_registered`]): a native
/// menu's action dispatches after the menu closes and never reached the
/// view's element-tree `.on_action`, so the row copied nothing.
#[derive(Clone, gpui::Action, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = steer_viewer, no_json)]
struct CopyTranscriptSelection {
    text: String,
}

static REGISTER_COPY_ACTION: std::sync::Once = std::sync::Once::new();

/// EXP-1266: the global [`CopyTranscriptSelection`] handler, registered once
/// per process (the `file_tree::ensure_actions_registered` rule: menu
/// actions reach only App-global handlers).
fn ensure_copy_action_registered(cx: &mut App) {
    REGISTER_COPY_ACTION.call_once(|| {
        cx.on_action(|action: &CopyTranscriptSelection, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(action.text.clone()));
        });
    });
}

/// EXP-1228: the transcript's right-click menu row.
const FEED_MENU_COPY: &str = "Copy";

/// How long a body may run before it folds behind "Show more" (web
/// `clampable`: >600 chars or >6 lines).
const CLAMP_CHARS: usize = 600;
const CLAMP_LINES: usize = 6;

/// Free-text answers are capped like the web input.
const FREE_TEXT_MAX: usize = 4000;

/// How often the staged-replay fallback re-checks its quiet window.
const STAGING_TICK: Duration = Duration::from_millis(100);

/// EXP-850 §5: how often the working row repaints while the agent works —
/// its caption counts SECONDS, so the row needs a beat the clock-free feed
/// cannot give it.
const WORKING_TICK: Duration = Duration::from_secs(1);

/// EXP-850 §5: the working mark's pulse — opacity 0.4↔1 over 1.4 s,
/// ease-in-out, ×4.
pub(crate) const WORKING_PULSE: Duration = Duration::from_millis(1400);

/// How many synthetic rows (the orphan workflow cards, then the trailing
/// conversation cards) fit between [`ORPHAN_ROW_BASE`] and the working row's
/// id: positions past it are dropped, never keyed onto the working row or
/// wrapped into the feed's own ids.
const SYNTHETIC_ROWS: usize = (WORKING_ROW_ID - ORPHAN_ROW_BASE) as usize;

/// The list id of the synthetic row at position `at` past the transcript
/// (orphan workflow cards first, then the trailing conversation cards):
/// `None` once it would reach [`WORKING_ROW_ID`].
fn synthetic_row_id(at: usize) -> Option<FeedItemId> {
    ORPHAN_ROW_BASE
        .checked_add(at as FeedItemId)
        .filter(|id| *id < WORKING_ROW_ID)
}

/// A conversation card's measure: its whole row (an answer, a decision, a
/// draft-free state change re-measures it).
fn conversation_card_fingerprint(row: Option<&domain::session_tree::SessionThreadRow>) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::hash::DefaultHasher::new();
    serde_json::to_string(&row).unwrap_or_default().hash(&mut hasher);
    hasher.finish()
}

/// EXP-850 §3: an orphan card row's content identity — its ROW id is an
/// index (the cards are keyed positionally at the tail), so the workflow it
/// actually shows rides the fingerprint instead.
fn orphan_fingerprint(workflow_id: &str) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::hash::DefaultHasher::new();
    workflow_id.hash(&mut hasher);
    hasher.finish()
}

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
    items: Vec<slash_commands::SlashItem>,
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
pub(crate) const FREE_TEXT_PLACEHOLDER: &str = "Type your answer…";
/// …and on a plan card's reject row, where the text is the change request.
pub(crate) const PLAN_REJECT_PLACEHOLDER: &str = "Tell the agent what to change…";

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
            FeedSource::Local { .. } | FeedSource::Replay { .. } | FeedSource::Journal { .. } => None,
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
            FeedSource::Replay { .. } | FeedSource::Remote { .. } | FeedSource::Journal { .. } => None,
        }
    }

    /// A replay renders history and offers no composer at all — and so does a
    /// journal read, which is history off the disk.
    fn read_only(&self) -> bool {
        matches!(self, FeedSource::Replay { .. } | FeedSource::Journal { .. })
    }
}

/// The detail a `Remote` view shows while it has nothing to dial with (no
/// steer runtime, nobody signed in). A relay that is OFF answers the mint
/// with the same words from the viewer task, so both read alike.
const STEER_UNAVAILABLE_DETAIL: &str = "Live steering is unavailable on this instance.";

/// A `Remote` view built before an account is active (a session tab that
/// exists at boot: a restored layout, `EXP_DEV_SCREEN=session:<id>`) cannot
/// dial yet. It keeps the feed's sender here and retries on every session
/// edge until a dial succeeds; after that the socket owns reconnects and
/// this stays empty.
struct PendingDial(Option<flume::Sender<ViewerEvent>>);

impl PendingDial {
    fn is_pending(&self) -> bool {
        self.0.is_some()
    }

    /// Dial with a clone of the sender. A failed attempt keeps it for the
    /// next edge; a successful one consumes it, so a source dials ONCE.
    fn attempt<H>(
        &mut self,
        dial: impl FnOnce(flume::Sender<ViewerEvent>) -> Option<H>,
    ) -> Option<H> {
        let handle = dial(self.0.as_ref()?.clone())?;
        self.0 = None;
        Some(handle)
    }
}

/// EXP-895 — how much of a tool row is SHOWN.
///
/// Every row is its headline plus compact evidence (a collapsed edit card's
/// `+a −b`, a `failed` tint), the reader's Show more opening a settled one.
/// The rule behind the running row is shared ×4 —
/// [`steer::feed::live_tool_row_id`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolRowMode {
    /// The ONE call still running. EXP-1206: its headline ALONE — nothing
    /// streams under it, so the transcript never jumps while a command
    /// prints; its output is the wire's once it settles.
    Live,
    /// A call that ENDED: the wire's capped patch and tail-cut output, folded
    /// away until the reader asks — the same bytes on the runner and on every
    /// viewer (runner parity).
    Settled,
    /// A bare headline: a row nested inside a card that IS the summary (a
    /// workflow agent's rows), where per-call evidence belongs to the
    /// transcript row rather than to the digest.
    Nested,
}

/// The steering view for ONE coding session — remote, local or replayed.
pub(crate) struct SteerSessionView {
    session_id: String,
    /// The synced row, re-snapshotted whenever `sessions` notifies.
    row: Option<domain::rows::SessionRow>,
    /// EXP-1294: whether the signed-in person may message (and Stop) this
    /// session — a SHARED session any team member, a private one only its
    /// starter. Re-read with the row.
    may_message: bool,
    /// EXP-1294: THE Guide of this session's tree (the ROOT's `guide`) —
    /// where an inline `sessions_show` picture is looked up.
    guide_results: Option<serde_json::Value>,
    feed: SteerFeed,
    /// EXP-746: what drives the feed. The relay handle lives inside it.
    source: FeedSource,
    /// A `Remote` source with no handle yet: dialled once an account is
    /// active (see [`PendingDial`]).
    pending_dial: PendingDial,
    /// EXP-1051: whether the usage sheet's context-window LEGEND is unfolded.
    /// View state, not run state: the bar and its headline are always there,
    /// the per-layer breakdown is one click away and stays where the reader
    /// left it while the popover is open.
    context_legend_open: bool,
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
    /// EXP-1334: the "+" menu's MCP servers row — the team's servers and
    /// this session's server-only pick (`sessions.mcpPicks`), loaded once
    /// (a failed load re-arms after [`MCP_RETRY_AFTER`], up to
    /// [`MCP_LOAD_ATTEMPTS`] tries).
    mcp: Option<SessionMcp>,
    mcp_requested: bool,
    mcp_attempts: u8,
    /// EXP-1334: the option picks ahead of their synced row (web
    /// `useSessionComposerOptions`' overrides): read before the row, dropped
    /// once the row says the same (never on a DISPATCHER root, whose picks
    /// are its WORK picks and never read back) or when the configure fails.
    option_picks: OptionPicks,
    /// EXP-1334: the configures waiting their turn — ONE request in flight at
    /// a time, in pick order — and whether the worker draining them runs.
    configure_queue: std::collections::VecDeque<QueuedConfigure>,
    configure_busy: bool,
    /// The team the completion source was last pointed at.
    mention_team: Option<String>,
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
    /// The slash menu's scroll (release review R5): ↑/↓ keep the selected
    /// command in view past the cap.
    slash_scroll: ScrollHandle,
    /// The exact draft Escape dismissed the menu for — it stays shut until
    /// the draft changes again (and after an accept, so inserting `/clear`
    /// does not immediately re-open the menu on its own result).
    slash_dismissed_for: Option<String>,
    /// EXP-698/EXP-825: the staged images (the shared composer strip).
    pending_images: PendingImages,
    /// Hosted as a ROOT session's composer ([`Self::host_root_composer`]):
    /// Send⇄Stop follows the whole tree.
    tree_scope: bool,
    /// The composer controls' element ids.
    composer_ids: ComposerIds,
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
    /// The view's own measured width (the recorded-px canvas recipe) — the
    /// transcript's bubble cap reads it.
    view_width: std::rc::Rc<std::cell::Cell<Pixels>>,
    /// EXP-884/EXP-916: the memo behind the edited-files cards — a card's
    /// patches are parsed once, not once per frame.
    edit_memo: crate::session_rows::EditMemo,
    /// EXP-1324: the failed-tests line's parse, once per SETTLED `execute`
    /// row (keyed by the row, re-read only when its output changed) — the
    /// regex pass over every visible row's output ran on every frame while
    /// the shimmer kept frames coming. Refilled by [`Self::refresh_derived`].
    test_results: HashMap<FeedItemId, (usize, Option<steer::trace::TestResults>)>,
    /// EXP-916: which PATHS the reader unfolded inside each edited-files
    /// card, keyed by the card (its first member's id). The live row opens
    /// itself and closes again when the transcript moves on, so nothing here
    /// survives the run's own flow.
    expanded_edit_rows: HashMap<FeedItemId, HashSet<String>>,
    /// EXP-884: the [`SteerFeed::generation`] the derived state (`active`,
    /// `subagents`, `strip_lines`, `duplicates`) was computed
    /// for; `None` before the first derivation.
    derived_for: Option<u64>,
    /// EXP-884: `feed.subagents()` — the strip's tabs and the focused tab's
    /// validity read it twice a frame; it walks and buckets the whole run.
    subagents: Vec<steer::SubagentSummary>,
    /// EXP-884/EXP-1324: the Queue's open waits, a whole-feed walk.
    queue_waits: Vec<steer::task_queue::TaskQueueWait>,
    /// EXP-1324: this client's first sight of each background task — the
    /// Queue's elapsed time for a publisher that sent no `startedAt`.
    background_seen: HashMap<String, i64>,
    /// EXP-1324: the reader's Queue toggles, sticky for the session: "to do"
    /// opens by default, "completed" folds by default.
    queue_todo_closed: bool,
    queue_completed_open: bool,
    /// EXP-1240: the workflow Work cards whose folded row is open (keyed by
    /// workflow id). VIEW state, never persisted.
    open_work_folds: HashSet<String>,
    /// EXP-884: the §4 duplicate edges, asked per workflow row.
    duplicates: Vec<crate::session_rows::DuplicateWarning>,
    /// EXP-916: the edited-files cards whose `{n} more` half is unfolded,
    /// keyed by the card (its first member's id).
    expanded_cards: HashSet<FeedItemId>,
    /// EXP-856: the duplicate edges this view has already raised an OS
    /// notification for — one per agent id, however often the edge repeats.
    duplicates_notified: HashSet<String>,
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
    /// EXP-850 §3: the workflow cards with no tool row in the projection (the
    /// call scrolled out of the window, or the feed trimmed it), rendered as
    /// their own rows at the tail — between the transcript and the working
    /// row, in feed order. Web, iOS and Android render the same orphans.
    orphan_workflows: Vec<String>,
    row_keys: Vec<RowKey>,
    active: HashSet<FeedItemId>,
    /// Thread 58cb32df (chats.json, FEED-140): this session's conversation
    /// cards the relay feed does not paint itself (the TOOL-posted plan /
    /// question cards in every state, the agent's surface cards), in
    /// message order; each sits after the call that posted it
    /// (`conv_anchors`: feed item id → card indices); the rest (their call is
    /// above the loaded window) sit at the HEAD, before the first row, like
    /// web's `renderToolCards(unanchored, 'head')` (`conv_unanchored`, see
    /// [`Self::conv_head`] / [`Self::conv_tail`]).
    conv_cards: Vec<domain::session_tree::SessionThreadRow>,
    conv_anchors: HashMap<FeedItemId, Vec<usize>>,
    conv_unanchored: Vec<usize>,
    /// N-12: THE Dock's state (the fan, the open sheet, the band height).
    dock: Entity<crate::dock::DockState>,
    /// EXP-1211: this session's open quick replies (its newest synced
    /// `replies` card), drawn above THE composer box.
    replies: Option<domain::session_tree::ComposerReplies>,
    /// Whether the synthetic trailing "Working…" row is present — the list's
    /// last index when it is.
    working: bool,
    /// #EXP-1389: the feed was seeded from THIS device's journal of a live
    /// (not ended) session the relay has no room for yet: an idle run on
    /// this machine shows its transcript, never an endless "starting".
    journal_seeded: bool,
    /// EXP-1259 (DEV-ONLY, `EXP_DEV_SCROLL_TO=script-card`): the transcript
    /// row the first Script card sat at when the list was last parked on it.
    dev_scrolled_to: Option<usize>,
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
        // EXP-1294: THE session composer field. EXP-724: the `/` menu is
        // invisible until it is typed, so the placeholder is the only hint
        // it exists; every agent's catalog is non-empty, which is why this is
        // a constant here and a conditional on web/Android.
        let (input, mention) =
            crate::composer::session_composer_field(COMPOSER_PLACEHOLDER, window, cx);

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
            subscriptions.push(cx.observe(&collections.sessions, |this, _, cx| {
                this.refresh_row(cx);
            }));
            // EXP-1211: the quick replies follow the synced `replies` card.
            subscriptions.push(cx.observe(&collections.messages, |this, _, cx| {
                this.refresh_replies(cx);
            }));
            // NEW-8: a message's session is its conversation's.
            subscriptions.push(cx.observe(&collections.conversations, |this, _, cx| {
                this.refresh_replies(cx);
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
                // …and the session edge itself: a tab built before anyone
                // was signed in dials the moment an account is active.
                this.dial_pending(cx);
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
            may_message: false,
            guide_results: None,
            feed: SteerFeed::new(),
            source,
            pending_dial: PendingDial(None),
            context_legend_open: false,
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
            mcp: None,
            mcp_requested: false,
            mcp_attempts: 0,
            option_picks: OptionPicks::default(),
            configure_queue: std::collections::VecDeque::new(),
            configure_busy: false,
            mention_team: None,
            inline: None,
            editing_step: None,
            // Seeded TRUE: a view built over an already-pending card takes
            // the keyboard on its first frame (the hide edge fires once).
            composer_shown: true,
            focused_subagent: None,
            answer_cursor: None,
            answer_cursor_for: None,
            slash: None,
            slash_scroll: ScrollHandle::new(),
            slash_dismissed_for: None,
            pending_images: PendingImages::default(),
            tree_scope: false,
            composer_ids: ComposerIds::session(),
            sending: false,
            notice: None,
            picked: HashMap::new(),
            images: image_cache,
            expanded_groups: HashSet::new(),
            expanded_bodies: HashSet::new(),
            extras: crate::session_extras::LocalExtras::default(),
            view_width: std::rc::Rc::new(std::cell::Cell::new(px(0.))),
            edit_memo: Default::default(),
            test_results: HashMap::new(),
            expanded_edit_rows: HashMap::new(),
            derived_for: None,
            subagents: Vec::new(),
            queue_waits: Vec::new(),
            background_seen: HashMap::new(),
            queue_todo_closed: false,
            queue_completed_open: false,
            open_work_folds: HashSet::new(),
            duplicates: Vec::new(),
            expanded_cards: HashSet::new(),
            duplicates_notified: HashSet::new(),
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
            orphan_workflows: Vec::new(),
            row_keys: Vec::new(),
            active: HashSet::new(),
            conv_cards: Vec::new(),
            conv_anchors: HashMap::new(),
            conv_unanchored: Vec::new(),
            dock: cx.new(|_| crate::dock::DockState::default()),
            replies: None,
            working: false,
            journal_seeded: false,
            dev_scrolled_to: None,
            chips: None,
            chip_cache: Default::default(),
            focus_handle: cx.focus_handle(),
            _drain: drain,
            _subscriptions: subscriptions,
        };
        this.refresh_row(cx);
        this.refresh_replies(cx);
        this.arm_stale_tick(cx);
        this.arm_working_tick(cx);
        // Seed the wakeup edges from the world as it is right now, so the
        // first observer call is a comparison rather than a false edge.
        this.device_offline = this.device(cx).offline;
        this.sync_offline = sync_offline(cx);
        // Only the relay source dials — and only when the caller did not hand
        // one in. A local source subscribes to its engine instead: same
        // reducer, same rendering, no socket.
        if let FeedSource::Remote { handle: None } = &this.source {
            this.pending_dial = PendingDial(Some(events_tx));
            this.dial_pending(cx);
            this.seed_from_journal(cx);
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
            this.refresh_derived();
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
            this.subscribe_local(&session, window, cx);
        }
        // #EXP-1383: a LOCAL source follows its session's engine. An agent's
        // `/new` (and any revive) ends the engine process and launches a
        // FRESH one for the same row; a view still bound to the old engine
        // showed its last turn forever and nothing the new one did.
        // #EXP-1389: a RELAY view of a live row follows too: once this
        // process hosts the session (a revive, a pickup after the view was
        // built), the view reads the engine instead of waiting on a room.
        if matches!(this.source, FeedSource::Local { .. } | FeedSource::Remote { .. }) {
            let local = crate::coding_flow::LocalSessions::global(cx);
            this._subscriptions.push(cx.observe_in(&local, window, |this, _, window, cx| {
                this.follow_local_engine(window, cx);
            }));
        }
        this
    }

    /// Drain `session`'s local feed into this view. `subscribe` replays the
    /// buffered backlog first, so a view built long after the run started (a
    /// reopened tab, a rebuilt screen) sees the whole session and not just
    /// the tail.
    fn subscribe_local(&mut self, session: &engine::EngineSession, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let events = session.subscribe();
        self._drain = cx.spawn_in(window, async move |this, cx| {
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

    /// #EXP-1383: re-bind a LOCAL source whose engine finished to the live
    /// engine that now hosts the same session here (the agent's `/new`
    /// restart, a revive): a fresh feed over the new engine, its phase seeded
    /// like a fresh view's. A live engine of ours, or none hosted here, is
    /// left alone.
    fn follow_local_engine(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let current_done = match &self.source {
            FeedSource::Local { session } => session.is_done(),
            // #EXP-1389: a relay view has no engine of its own: any live one
            // hosting this session here wins.
            FeedSource::Remote { .. } => true,
            FeedSource::Replay { .. } | FeedSource::Journal { .. } => return,
        };
        let next = crate::coding_flow::LocalSessions::global_ref(cx)
            .and_then(|sessions| sessions.read(cx).session_by_id(&self.session_id).map(|entry| entry.host.session.clone()));
        let Some(next) = next else {
            return;
        };
        if !follows_engine(current_done, next.is_done()) {
            return;
        }
        if let FeedSource::Remote { handle: Some(handle) } = &self.source {
            handle.shutdown();
        }
        self.pending_dial = PendingDial(None);
        self.journal_seeded = false;
        log::info!("[ui] session {}: the view follows its restarted engine", self.session_id);
        self.feed = SteerFeed::new();
        self.derived_for = None;
        self.extras = crate::session_extras::LocalExtras::default();
        self.picked.clear();
        self.answer_cursor = None;
        self.answer_cursor_for = None;
        self.refresh_derived();
        self.phase = next.phase().map(viewer_phase).unwrap_or(ViewerPhase::Connecting);
        self.connected = !matches!(self.phase, ViewerPhase::Ended { .. });
        self.subscribe_local(&next, window, cx);
        self.source = FeedSource::Local { session: next };
        self.list.scroll_to_end();
        cx.notify();
    }

    /// #EXP-1389: a relay view of a LIVE row this machine ran (an idle
    /// session whose process went away, a publisher that stopped) starts
    /// from the device's own journal: the relay answers `no_such_session`
    /// until a publisher is back, and the screen sat on "The agent is
    /// starting" forever while the transcript was on this disk. The room's
    /// replay (`activity_reset`) replaces it once the run publishes again.
    fn seed_from_journal(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.feed.is_empty()
            || crate::session_data::session(&self.session_id, cx).is_none_or(|row| row.is_ended())
        {
            return;
        }
        let Some(events) = steer::read_journal(&crate::coding_flow::coding_data_dir(cx), &self.session_id)
        else {
            return;
        };
        if events.is_empty() {
            return;
        }
        for event in events {
            self.feed.apply(event);
        }
        self.refresh_derived();
        self.journal_seeded = true;
        self.list.scroll_to_end();
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

    /// The synced `sessions` row behind this viewer — what the header names
    /// the session from and gates its Stop on.
    pub(crate) fn session_row(&self) -> Option<&domain::rows::SessionRow> {
        self.row.as_ref()
    }

    /// EXP-760: the team whose issues this feed's `#IDENT` / bare `EXP-1`
    /// tokens resolve against — the session row's own `team_id` (EXP-1294:
    /// every session carries it). `None` before the row syncs.
    fn ref_team_id(&self, _cx: &App) -> Option<String> {
        self.row.as_ref()?.team_id.clone().filter(|id| !id.is_empty())
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
        self.ref_team_id(cx)
            .map(|team_id| crate::markdown::RefResolver::from_store(team_id).bare(true).memoized(self.chip_cache.clone()))
    }

    /// EXP-760: hang the feed's bare-mode chip resolver (and the in-app
    /// open) on one prose view. A run with no resolvable team leaves its
    /// tokens as plain text. EXP-1188: links route through
    /// `open_agent_link` either way (a run or issue on this instance opens
    /// in the app).
    fn with_issue_chips(
        &self,
        view: crate::markdown::MarkdownView,
        cx: &App,
    ) -> crate::markdown::MarkdownView {
        let view = view.on_open_link(crate::markdown::open_agent_link);
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

    /// EXP-884 — re-derive every whole-feed projection this view keeps (the
    /// answerable cards, the subagent summaries, the strip lines, the
    /// duplicate edges, the duplicate edges), but ONLY when the feed's
    /// generation moved since the last derivation.
    ///
    /// Called wherever the feed mutates and at the top of every frame. The
    /// first cut recomputed all of these per frame, straight off the run —
    /// six walks over every item, one of them re-parsing every edit's diff —
    /// so a scroll frame on a long transcript cost tens of milliseconds
    /// before a single row was painted. Now a frame in which nothing arrived
    /// pays an integer compare, and a streaming frame pays one walk per
    /// derivation (the diff reads are memoised on top, [`EditMemo`]).
    fn refresh_derived(&mut self) {
        let generation = self.feed.generation();
        if self.derived_for == Some(generation) {
            return;
        }
        self.derived_for = Some(generation);
        self.active = self.feed.active_question_ids();
        self.subagents = self.feed.subagents();
        self.queue_waits = crate::session_rows::queue_waits(self.feed.items());
        let now = now_epoch_ms();
        let tasks = self.feed.background_tasks();
        self.background_seen
            .retain(|id, _| tasks.iter().any(|task| task.id == *id));
        for task in tasks {
            self.background_seen.entry(task.id.clone()).or_insert(now);
        }
        self.duplicates = crate::session_rows::duplicate_warnings(self.feed.items());
        self.refresh_test_results();
    }

    /// EXP-1324: parse each settled `execute` row's output for test results
    /// ONCE (a row's entry is kept while its output is the same bytes), so
    /// the render reads a lookup instead of running the regex pass.
    fn refresh_test_results(&mut self) {
        let mut cached = std::mem::take(&mut self.test_results);
        self.test_results = self
            .feed
            .items()
            .iter()
            .filter_map(|item| {
                let FeedKind::Tool { tool_kind, settled: true, output: Some(output), .. } = &item.kind
                else {
                    return None;
                };
                if tool_kind.map(|kind| kind.as_str()) != Some("execute") || output.trim().is_empty() {
                    return None;
                }
                let results = match cached.remove(&item.id) {
                    Some((len, results)) if len == output.len() => results,
                    _ => steer::trace::parse_test_results(Some(output)),
                };
                Some((item.id, (output.len(), results)))
            })
            .collect();
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
    pub(crate) fn working_now(&self) -> bool {
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
        self.row.as_ref().is_some_and(|row| row.needs_input)
    }

    /// The view-side bits of one item that move its rendered height — see
    /// [`transcript_rows::row_fingerprint`].
    fn item_facets(&self, item: &FeedItem) -> ItemFacets {
        let mut facets = 0;
        if self.expanded_bodies.contains(&item.id) {
            facets |= facet::BODY_EXPANDED;
        }
        // EXP-786: a remote row's wire diff is the same fold as a local
        // row's extras — same card, same "Show more" — and so is a settled
        // row's wire output (EXP-1206: the only output a row ever carries).
        let has_extras = self.extras.has_extras(item.id)
            || matches!(&item.kind, FeedKind::Tool { output: Some(_), .. })
            // EXP-1324: a Reasoning line this run kept the text of opens.
            || matches!(&item.kind, FeedKind::Thought { thought_id, .. }
                if self.extras.thought_text(thought_id).is_some())
            || (self.source.session().is_none()
                && matches!(&item.kind, FeedKind::Tool { diff: Some(_), .. }));
        if has_extras {
            facets |= facet::HAS_EXTRAS;
            if self.expanded_extras.contains(&item.id) {
                facets |= facet::EXTRAS_EXPANDED;
            }
        }
        // EXP-1324: the failed-tests line opens onto the output.
        if item.is_tool() && self.expanded_cards.contains(&item.id) {
            facets |= facet::TESTS_EXPANDED;
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
        // EXP-916: an edited-files card is keyed by its FIRST member, so it
        // is evicted with that row like every other per-row flag.
        self.expanded_cards.retain(|id| *id >= first);
        self.expanded_edit_rows.retain(|id, _| *id >= first);
        // `picked` is keyed by `answer_key`, not by row id, so it has to be
        // retained against the question ids still in the feed. Read them
        // borrow-free (`items()` and `picked` are disjoint fields) and without
        // allocating a set: a transcript holds a handful of question cards.
        let items = self.feed.items();
        self.picked.retain(|key, _| items.iter().any(|item| item.question().is_some_and(|card| card.question_id == *key)));
        self.extras.prune_before(first);
        self.edit_memo.prune_before(first);
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
    /// pages below it exist only in the device's journal. A feed holding only
    /// LOCAL rows (echoes, synthetic cards) carries no wire sequence to ask
    /// relative to, and cannot be paged.
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
        self.refresh_derived();
        // #EXP-1383 FOLLOW: an arrival keeps a reader inside the slack on
        // the tail (the chats' rule).
        crate::chat_transcript::rearm_near_tail(&self.list);
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
        // EXP-850 §3: the lane projection needs them too — a workflow call is
        // its own card inside a lane exactly as it is in the main transcript.
        let held = self.feed.workflow_ids();
        match focus.as_deref() {
            Some(subagent_id) => {
                steer::feed::group_subagent_row_specs_into(self.feed.items(), start, subagent_id, &held, &mut self.rows)
            }
            None => {
                self.rows.clear();
                self.feed.row_specs_from_into(start, &mut self.rows);
            }
        }
        // EXP-850 §3/§4: a workflow's agents are the CARD's rows, never loose
        // ones, and §9 floats an unanswered card under everything that
        // followed it. Both are pure ([`crate::session_rows`]) and both run
        // on the projection, so the list diff sees one settled order.
        if focus.is_none() {
            crate::session_rows::hide_workflow_agent_rows(&mut self.rows, self.feed.items(), &held);
            crate::session_rows::pending_last(&mut self.rows, self.feed.items());
            // §3: a card whose `Workflow` row is outside the window (or gone
            // with the feed's trim) still renders — at the tail, never lost.
            self.orphan_workflows =
                crate::session_rows::orphan_workflow_ids(&self.rows, self.feed.items(), &held);
        } else {
            self.orphan_workflows.clear();
        }
        // S-3 (dock.json): the relay's waiting card stays inline, answerable
        // in place; it is never a DockCard any more.
        // EXP-850 §12: the per-turn file cards are derived state now
        // (EXP-884, [`Self::refresh_derived`] above).
        self.prune_dropped_rows();
        // Thread 58cb32df: the conversation's own cards beside the relay rows.
        self.refresh_conversation_cards(focus.is_none(), cx);
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
            // EXP-850: a row can grow a workflow card (§3) or a file card
            // (§12) whose height moves without the row's own content moving —
            // the card's state is part of the row's measure.
            let fingerprint = transcript_rows::fold_extra(fingerprint, self.row_extra(spec, cx));
            // The head cards ride the first row's measure.
            let fingerprint = match ix {
                0 if !self.conv_head().is_empty() => transcript_rows::fold_extra(
                    fingerprint,
                    self.conv_head().iter().fold(0u64, |acc, &card| {
                        acc.rotate_left(7) ^ conversation_card_fingerprint(self.conv_cards.get(card))
                    }),
                ),
                _ => fingerprint,
            };
            keys.push(RowKey { id, fingerprint });
        }
        // The synthetic rows share the ids between ORPHAN_ROW_BASE and the
        // working row's: whatever does not fit is dropped (keys and paint
        // read the same truncated lists).
        self.orphan_workflows.truncate(SYNTHETIC_ROWS);
        if self.rows.is_empty() {
            self.conv_unanchored.truncate(SYNTHETIC_ROWS - self.orphan_workflows.len());
        }
        // §3: the orphan cards, in feed order, keyed above every feed id and
        // below the working row's so the list sync keeps its ascending order.
        for (ix, id) in self.orphan_workflows.iter().enumerate() {
            let Some(row_id) = synthetic_row_id(ix) else { break };
            let gap = f32::from(self.row_gap(self.rows.len() + ix));
            let fingerprint = transcript_rows::fold_gap(orphan_fingerprint(id), gap);
            let workflow = self.feed.workflow_for(id);
            let fingerprint = transcript_rows::fold_extra(
                fingerprint,
                workflow.map_or(0, |workflow| {
                    (workflow.agents.len() as u64) << 32
                        | (workflow.done_agents() as u64) << 16
                        | (self.open_work_folds.contains(&workflow.id) as u64) << 8
                        | workflow.status as u64
                }),
            );
            keys.push(RowKey { id: row_id, fingerprint });
        }
        // Thread 58cb32df: with no row to sit above, the unanchored cards are
        // rows of their own, keyed right after the orphan cards and below
        // the working row.
        for (ix, &card) in self.conv_tail().iter().enumerate() {
            let Some(row_id) = synthetic_row_id(self.orphan_workflows.len() + ix) else { break };
            let at = self.rows.len() + self.orphan_workflows.len() + ix;
            let gap = f32::from(self.row_gap(at));
            let fingerprint = transcript_rows::fold_gap(conversation_card_fingerprint(self.conv_cards.get(card)), gap);
            keys.push(RowKey { id: row_id, fingerprint });
        }
        if self.working {
            let gap = f32::from(self.row_gap(self.rows.len() + self.orphan_workflows.len() + self.conv_tail().len()));
            // EXP-850 §5: the working row's text is no longer a constant (a
            // verb, a ticking duration, a workflow's caption), so its
            // fingerprint folds in what it will say.
            let caption = self.working_caption();
            keys.push(RowKey {
                fingerprint: transcript_rows::fold_extra(
                    transcript_rows::fold_gap(RowKey::WORKING.fingerprint, gap),
                    caption.len() as u64,
                ),
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

    /// EXP-1259 (DEV-ONLY): `EXP_DEV_SCROLL_TO=script-card` parks the
    /// transcript with its first code-mode Script card near
    /// the top of the viewport (one row of lead-in) instead of on the tail,
    /// so the `script-card` capture has the card in frame. Re-applied only
    /// while that row's index moves (the replay still landing rows above
    /// it), so a wheel is never fought.
    fn dev_scroll_to_script_card(&mut self) {
        let Some(target) = crate::screens::dev_scroll_to() else {
            return;
        };
        let items = self.feed.items();
        let script_row = self.rows.iter().position(|spec| {
            spec.item_indices().iter().any(|&ix| {
                let Some(item) = items.get(ix) else { return false };
                match target {
                    // EXP-1324: the workflow's Work card rides its tool row.
                    "work-card" => self.workflow_of(item).is_some(),
                    _ => matches!(
                        &item.kind,
                        FeedKind::Tool { name, .. } if steer::script_card::is_script_tool(name)
                    ),
                }
            })
        });
        let Some(row) = script_row else {
            return;
        };
        if self.dev_scrolled_to == Some(row) {
            return;
        }
        self.dev_scrolled_to = Some(row);
        // One row of lead-in above it: the list's top edge sits under the
        // status row's fade.
        self.list.scroll_to(gpui::ListOffset {
            item_ix: row.saturating_sub(1),
            offset_in_item: px(0.),
        });
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

    /// EXP-1211: re-derive the quick replies ([`domain::session_tree::
    /// composer_replies`]); repaints only when they changed.
    fn refresh_replies(&mut self, cx: &mut gpui::Context<Self>) {
        let messages = crate::session_data::messages_of_session(&self.session_id, cx);
        let messages: Vec<&domain::rows::MessageRow> = messages.iter().collect();
        let replies = domain::session_tree::composer_replies(&self.session_id, &messages);
        if replies != self.replies {
            self.replies = replies;
            cx.notify();
        }
    }

    fn refresh_row(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(store) = sync::Store::try_global(cx) else {
            return;
        };
        let row = store
            .collections()
            .sessions
            .read(cx)
            .get(&self.session_id)
            .cloned();
        if row.as_ref().is_some_and(domain::rows::SessionRow::is_ended) {
            // EXP-639: the redial loops treat the synced row as the truth —
            // a `no_such_session` for an ended run would otherwise park the
            // viewer in `Starting` forever.
            if let Some(handle) = self.source.handle() {
                handle.note_session_ended();
            }
            // EXP-724: a run that ended mid-compaction never sends `ended`.
            self.feed.clear_compaction();
        }
        let me = crate::queries::active_account(cx).map(|account| account.user_id);
        let may_message = row
            .as_ref()
            .is_some_and(|row| may_steer(row, me.as_deref()));
        if self.row != row || self.may_message != may_message {
            self.row = row;
            self.may_message = may_message;
            // EXP-1334: the synced row now says what a pick asked for — the
            // pick has settled.
            if let Some(row) = self.row.as_ref() {
                self.option_picks.drop_matching(row);
            }
            cx.notify();
        }
        // The ROOT's Guide rides the same collection (EXP-1294).
        self.refresh_guide(cx);
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
        self.row.as_ref().is_some_and(domain::rows::SessionRow::is_ended)
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
    ///
    /// Never for a run this process hosts: it is running right here, whatever
    /// the synced row's last heartbeat says.
    fn paused(&self, cx: &App) -> bool {
        !self.row_ended() && !self.is_local() && self.device(cx).offline
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

    /// Dial a `Remote` source that has no socket yet. Until it can (no
    /// steer runtime, nobody signed in) the view says steering is
    /// unavailable; the first successful dial clears that back to
    /// `Connecting`, and from there the viewer task reports the relay's own
    /// answer (a relay that is off says the same words, for good).
    fn dial_pending(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.pending_dial.is_pending() {
            return;
        }
        let FeedSource::Remote { handle } = &mut self.source else {
            return;
        };
        let session_id = self.session_id.clone();
        match self.pending_dial.attempt(|tx| spawn_viewer(&session_id, tx, cx)) {
            Some(dialled) => {
                *handle = Some(dialled);
                self.phase = ViewerPhase::Connecting;
                cx.notify();
            }
            None => {
                self.phase = ViewerPhase::Unauthorized {
                    detail: Some(STEER_UNAVAILABLE_DETAIL.to_string()),
                };
            }
        }
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
                // EXP-856: the duplicate toast rides the EVENT, not the feed
                // — one per agent id, before the row it becomes.
                self.note_duplicate_agent(&activity, cx);
                self.feed.apply_seq(Some(seq), activity);
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
                // A swap re-mints row ids over ranges the memo may still
                // hold, so its length-keyed entries are stale from here on.
                if self.feed.is_staging() {
                    self.edit_memo.clear();
                }
                self.feed.apply_synced_from(first_seq);
                self.history_truncated = truncated;
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
                    // Re-minted ids: see the `Synced` arm above.
                    self.edit_memo.clear();
                    self.feed.force_swap();
                }
            }
            ViewerEvent::LocalMessage(text) => {
                // EXP-861/EXP-873: sent while the agent is mid-turn or
                // compacting (the RAW slot values, ×4 parity), the message
                // lives in the queue bar until the agent takes it in, and its
                // `user_message` row arrives THEN — no echo: one here would
                // sit in the transcript ahead of the tool call the message
                // actually lands behind, and its dedupe would swallow the
                // real row.
                let unread = self.feed.turn_state() == steer::TurnState::Started
                    || self.feed.compacting().is_some();
                if !unread {
                    self.feed.push_local_message(&text);
                }
            }
        }
        self.refresh_derived();
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
                self.note_duplicate_agent(&event, cx);
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
                self.refresh_derived();
                self.note_feed_moved(pulse);
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
                    // Re-minted ids: see the `ViewerEvent::Synced` arm.
                    this.edit_memo.clear();
                    this.feed.force_swap();
                    this.refresh_derived();
                    this.note_feed_moved(pulse);
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
        feedback: Option<String>,
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
        // EXP-1294: the database is the bus — a card the host filed as a
        // `messages` row is answered IN that row (`sessions.answer`); the
        // live layer (local engine / relay) answers only a card no row
        // carries yet.
        if let Some(message_id) = self.card_message_id(&card, cx) {
            let input = card_answer_input(&card, message_id, &keys, &labels, text.as_deref(), feedback);
            self.answer_via_server(&key, keys, labels, input, item_id, cx);
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
            FeedSource::Remote { handle } => handle
                .as_ref()
                .is_some_and(|handle| handle.send_answer(question_id, card.ask_id.as_deref(), &keys, text.as_deref())),
        };
        if !sent {
            self.notice = Some(SharedString::from(NOT_CONNECTED));
            cx.notify();
            return;
        }
        // A plan's "keep planning" note follows the reject as a message.
        if let Some(feedback) = feedback {
            if !self.deliver(&feedback) {
                self.notice = Some(SharedString::from(NOT_CONNECTED));
            }
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

    /// EXP-1294 — the `messages` row behind a pending card: this session's
    /// unanswered card row carrying the card's wire id ([`card_row_id`]).
    /// `None` = no row carries it (yet), and the live layer answers.
    fn card_message_id(&self, card: &steer::feed::QuestionCard, cx: &App) -> Option<String> {
        let messages = crate::session_data::messages_of_session(&self.session_id, cx);
        card_row_id(&messages, &card.question_id)
    }

    /// EXP-1294 — `sessions.answer` for a card's row. The card locks at once
    /// (the host's `question_resolved` settles it); a refusal unlocks it
    /// with the usual "Couldn't answer the question" toast.
    fn answer_via_server(
        &mut self,
        key: &str,
        keys: Vec<String>,
        labels: Vec<String>,
        input: api::sessions::AnswerInput,
        item_id: FeedItemId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("answer", "the question", None, cx);
            return;
        };
        self.feed.note_answer_sent(key, keys, labels);
        self.last_activity = std::time::Instant::now();
        self.picked.remove(key);
        self.answer_cursor = None;
        self.answer_cursor_for = None;
        if self.inline.as_ref().is_some_and(|inline| inline.item == item_id) {
            self.inline = None;
        }
        if self.editing_step == Some(item_id) {
            self.editing_step = None;
        }
        let key = key.to_string();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::sessions::answer(&trpc, &input) })
                .await;
            let Err(err) = result else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.feed.fail_answer(&key);
                crate::action_error::failed_action_err("answer", "the question", &err, cx);
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
            .filter(|item| item.question().is_some_and(|card| card.ask_id.as_deref() == Some(ask_id)))
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
        self.answer(item_id, vec![option_key], vec![option_label], None, None, cx);
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
        self.answer(item_id, picked, labels, None, None, cx);
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
        let subscription = cx.subscribe_in(&state, window, |this, _, event: &InputEvent, window, cx| match event {
            InputEvent::PressEnter { shift: false, .. } => this.send_inline(window, cx),
            // The Send button's enabled state follows the draft.
            InputEvent::Change => cx.notify(),
            _ => {}
        });
        state.update(cx, |state, cx| state.focus(window, cx));
        self.inline = Some(InlineAnswer { item: item_id, option_key, state, _subscription: subscription });
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
            let feedback = (!text.is_empty()).then(|| text.clone());
            self.answer(item_id, vec![option_key], vec![option_label], None, feedback, cx);
        } else {
            if text.is_empty() {
                return;
            }
            self.answer(item_id, vec![option_key], vec![text.clone()], Some(text.clone()), None, cx);
        }
        // `answer` dropped the field once the answer went out; a refused send
        // (not connected) keeps it — and its text — for a retry.
        if self.inline.is_some() {
            return;
        }
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    // ── Composer ───────────────────────────────────────────────────────────

    /// EXP-1294: a draft goes out whenever the reader may message the
    /// session — the database is the bus, so no live socket is needed, and a
    /// message to an idle (or ended) session resumes (continues) it.
    fn can_send(&self, cx: &App) -> bool {
        let has_content =
            !self.input.read(cx).value().trim().is_empty() || !self.pending_images.is_empty();
        !self.sending && self.may_message && has_content
    }

    /// Whether the run can still take input at all: a live row, not a
    /// replay or journal. EXP-746: a replay is a transcript — there is
    /// nothing to type at. The composer AND the queue strip both hang off
    /// this; only the composer additionally yields to a pending card.
    fn run_open(&self) -> bool {
        !(self.source.read_only()
            || self.row_ended()
            || matches!(self.phase, ViewerPhase::Ended { .. }))
    }

    /// Whether the reader may message this session at all (the thread's
    /// ask dock gate: a reader who may not reply keeps the cards inline).
    pub(crate) fn may_message(&self) -> bool {
        self.may_message
    }

    /// N-12: THE composer never yields its slot (dock.json): it shows
    /// whenever the reader may message the session.
    fn composer_visible(&self) -> bool {
        self.may_message
    }

    /// Thread 58cb32df (chats.json, FEED-140): the conversation's cards the
    /// relay feed does not paint (web `AgentSessionView`'s tool cards + its
    /// surface cards): every TOOL-posted plan / question card (waiting ones
    /// stay inline beside THE Dock; answered ones closed; a superseded plan
    /// collapsed) and every surface card, anchored after the settled call
    /// that posted it. An engine ask is the feed's own Question row (never
    /// duplicated). Never on a root's host (its thread paints the tree) nor
    /// on a subagent tab.
    fn refresh_conversation_cards(&mut self, main: bool, cx: &mut gpui::Context<Self>) {
        if self.tree_scope || !main {
            self.conv_cards.clear();
            self.conv_anchors.clear();
            self.conv_unanchored.clear();
            return;
        }
        // EXP-1398 (ask-dock.json `sessionCardsCases`): THE card set ×4.
        let messages = crate::session_data::messages_of_session(&self.session_id, cx);
        let by_id: HashMap<&str, &domain::rows::MessageRow> =
            messages.iter().map(|message| (message.id.as_str(), message)).collect();
        self.conv_cards = domain::ask_dock::session_cards(&messages)
            .iter()
            .filter_map(|card| {
                let message = by_id.get(card.id.as_str())?;
                if card.kind == domain::ask_dock::SessionCardKind::Surface {
                    domain::session_tree::surface_card_row(message)
                } else {
                    domain::session_tree::ask_card_row(message)
                }
            })
            .collect();
        crate::surface_card::sync_rows(cx.entity_id(), &self.conv_cards, cx);
        // The calls the window shows (a card whose call is above it sits at
        // the head).
        let shown: HashSet<FeedItemId> = self
            .rows
            .iter()
            .flat_map(|spec| spec.item_indices().iter().filter_map(|&ix| self.feed.items().get(ix)).map(|item| item.id))
            .collect();
        let calls: Vec<(u64, &str)> = self
            .feed
            .items()
            .iter()
            .filter(|item| shown.contains(&item.id))
            .filter_map(|item| match &item.kind {
                FeedKind::Tool { name, settled: true, failed: false, .. } => {
                    domain::ask_dock::card_call_row(name).map(|row| (item.id, row))
                }
                _ => None,
            })
            .collect();
        let (anchors, tail) = crate::surface_card::anchor_cards(&calls, &self.conv_cards);
        self.conv_anchors = anchors;
        self.conv_unanchored = tail;
    }

    /// The unanchored conversation cards painted above the first row (web's
    /// `head`): all of them while the projection has a row to sit on.
    fn conv_head(&self) -> &[usize] {
        if self.rows.is_empty() {
            &[]
        } else {
            &self.conv_unanchored
        }
    }

    /// …and with no row at all they are synthetic rows of their own, after
    /// the orphan workflow cards and before the working row.
    fn conv_tail(&self) -> &[usize] {
        if self.rows.is_empty() {
            &self.conv_unanchored
        } else {
            &[]
        }
    }

    /// The conversation cards at `indices`, painted by the thread screen's
    /// own row painter (the inline plan / question answers while the reader
    /// may message the session).
    fn render_conversation_cards(&self, indices: &[usize], window: &Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let team_id = self.row.as_ref().and_then(|row| row.team_id.clone());
        let text = crate::thread_screen::ThreadText::team(team_id.as_deref()).answering(self.may_message);
        let cards: Vec<AnyElement> = indices
            .iter()
            .filter_map(|&index| self.conv_cards.get(index).cloned())
            .filter_map(|row| crate::thread_screen::card_row(&row, &self.session_id, &text, &self.images, window, cx))
            .collect();
        v_flex().w_full().min_w_0().gap(px(transcript::GAP_BLOCK)).children(cards).into_any_element()
    }

    /// A `/new` / `/compact` divider's `HH:mm`: the feed's divider lines of
    /// that kind paired newest back with the session's synced ones.
    fn divider_time(&self, item_id: FeedItemId, kind: domain::chats::DividerKind, cx: &App) -> String {
        let is_kind = |text: &str| domain::chats::command_divider(text) == Some(kind);
        let from_end = self
            .feed
            .items()
            .iter()
            .rev()
            .filter(|item| matches!(&item.kind, FeedKind::UserMessage { text, .. } if is_kind(text)))
            .position(|item| item.id == item_id);
        let Some(from_end) = from_end else { return String::new() };
        let messages = crate::session_data::messages_of_session(&self.session_id, cx);
        let mut synced: Vec<&domain::rows::MessageRow> = messages
            .iter()
            .filter(|message| {
                message.author_kind.as_deref() == Some(domain::contract::MESSAGE_AUTHOR_KIND_USER)
                    && message.card.is_none()
                    && is_kind(message.body_text())
            })
            .collect();
        synced.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        synced
            .iter()
            .rev()
            .nth(from_end)
            .map(|message| crate::thread_screen::clock(message.created_at.as_deref()))
            .unwrap_or_default()
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
        // EXP-746: a local run KNOWS its agent.
        if let Some(session) = self.source.session() {
            return match session.agent() {
                coding::CodingAgent::Claude => SessionAgent::Claude,
                coding::CodingAgent::Codex => SessionAgent::Codex,
            };
        }
        // A REMOTE run has only its row to go on; a row that names no agent
        // (an older or foreign build) is told apart by the `config_state` it
        // published.
        let acp = self.feed.config().is_some();
        self.row
            .as_ref()
            .map_or(SessionAgent::Claude, |row| slash_commands::agent_of(row, acp))
    }

    /// EXP-746: the run's context/spend meter (the usage sheet's own block).
    pub(crate) fn usage(&self) -> Option<steer::SessionUsage> {
        self.feed.usage()
    }

    /// EXP-1051: the device's own accounting of what it SEEDED the context
    /// window with — latest-wins state beside [`Self::usage`], empty until the
    /// device publishes one (an older build never will, and then the sheet's
    /// bar is the conversation alone).
    pub(crate) fn context_layout(&self) -> Vec<steer::ContextSegment> {
        self.feed.context_layout().to_vec()
    }

    /// EXP-1261: whether the transcript left its top (the work body's top
    /// fade waits for it).
    pub(crate) fn body_scrolled(&self) -> bool {
        crate::work_header::list_scrolled(&self.list)
    }

    /// EXP-1294 — re-read THE Guide: the tree ROOT's `guide` (the thread's
    /// Guide), which is where this session's `sessions_show` pictures land.
    fn refresh_guide(&mut self, cx: &mut gpui::Context<Self>) {
        let guide = crate::session_data::root_of(&self.session_id, cx).and_then(|root| root.guide);
        if guide != self.guide_results {
            self.guide_results = guide;
            // EXP-1172: a `sessions_show` tile appears (or goes) under its
            // call when the Guide changes — the list re-measures its rows so
            // an off-screen one does not keep a stale height.
            self.list.remeasure();
            cx.notify();
        }
    }

    // ── EXP-850 §3/§4/§5/§12: the derived rows ────────────────────────────

    /// The workflow card patched onto a tool row (§3: latest-wins per id, the
    /// `Workflow` call's own id), when this item IS that call.
    fn workflow_of(&self, item: &FeedItem) -> Option<&steer::WorkflowState> {
        let FeedKind::Tool { call_id, .. } = &item.kind else {
            return None;
        };
        self.feed.workflow_for(call_id.as_deref()?)
    }

    /// The DUPLICATE warnings belonging to `workflow_id` (§4: they render
    /// under the card, and stay there when it collapses).
    fn workflow_duplicates(&self, workflow_id: &str) -> Vec<crate::session_rows::DuplicateWarning> {
        // EXP-884: off the derived list, never a fresh walk per workflow row.
        self.duplicates
            .iter()
            .filter(|warning| warning.workflow_id.as_deref() == Some(workflow_id))
            .cloned()
            .collect()
    }

    /// What a row carries BEYOND its feed item — the workflow card's live
    /// state, the file card under it, the fold flags. Folded into the row's
    /// fingerprint ([`transcript_rows::fold_extra`]) so a card that grew
    /// re-measures the row it hangs under.
    fn row_extra(&self, spec: &FeedRowSpec, cx: &App) -> u64 {
        use std::hash::{Hash as _, Hasher as _};
        let mut hasher = std::hash::DefaultHasher::new();
        let items = self.feed.items();
        for &ix in spec.item_indices() {
            let Some(item) = items.get(ix) else {
                continue;
            };
            // EXP-1279/EXP-1241: the Script card's disclosure, and the
            // comment card's body once its row syncs.
            if let FeedKind::Tool { name, .. } = &item.kind {
                if steer::script_card::is_script_tool(name) {
                    self.expanded_cards.contains(&item.id).hash(&mut hasher);
                }
                if steer::exp_tool_row(name) == Some(COMMENTS_CREATE_ROW) {
                    self.comment_call_body_len(item, cx).hash(&mut hasher);
                }
            }
            // Thread 58cb32df: the conversation cards after this call.
            if let Some(cards) = self.conv_anchors.get(&item.id) {
                for &card in cards {
                    conversation_card_fingerprint(self.conv_cards.get(card)).hash(&mut hasher);
                }
            }
            if let Some(workflow) = self.workflow_of(item) {
                // EXP-1240: the Work card's folded row opens in place.
                self.open_work_folds.contains(&workflow.id).hash(&mut hasher);
                workflow.agents.len().hash(&mut hasher);
                workflow.done_agents().hash(&mut hasher);
                workflow.phases.len().hash(&mut hasher);
                workflow.status.as_str().hash(&mut hasher);
                workflow.summary.as_ref().map(String::len).hash(&mut hasher);
                self.workflow_duplicates(&workflow.id).len().hash(&mut hasher);
                for agent in &workflow.agents {
                    agent.state.as_str().hash(&mut hasher);
                }
            }
        }
        // EXP-916: an edited-files card re-measures when its fold or any of
        // its rows opens (the card itself grows with the run, which the
        // member ids already fold in above).
        if let FeedRowSpec::Edits { id, .. } = spec {
            self.expanded_cards.contains(id).hash(&mut hasher);
            // WHICH paths are open, not how many: a reader who folds the live
            // row and opens another keeps the count and changes the height.
            match self.expanded_edit_rows.get(id) {
                None => u64::MAX.hash(&mut hasher),
                Some(open) => {
                    open.len().hash(&mut hasher);
                    // Order-free, because a `HashSet` has no order to hash.
                    open.iter()
                        .map(|path| {
                            let mut one = std::hash::DefaultHasher::new();
                            path.hash(&mut one);
                            one.finish()
                        })
                        .fold(0u64, |acc, one| acc ^ one)
                        .hash(&mut hasher);
                }
            }
        }
        hasher.finish()
    }

    /// EXP-1241: how long the comment a settled comment call posted is, as
    /// this client sees it (the synced row's body, `None` before it syncs).
    fn comment_call_body_len(&self, item: &FeedItem, cx: &App) -> Option<usize> {
        let FeedKind::Tool { preview: Some(preview), .. } = &item.kind else {
            return None;
        };
        let comment_ref = preview.refs.iter().find(|r| r.kind == "comment")?;
        let store = sync::Store::try_global(cx)?;
        let comment = store.collections().comments.read(cx).get(&comment_ref.id).cloned()?;
        Some(comment.body.as_deref().map_or(0, str::len))
    }

    /// §5 — the caption of the newest RUNNING workflow, which replaces the
    /// working verb (and, on the session list rows, the row's second line).
    pub(crate) fn running_workflow_caption(&self) -> Option<String> {
        self.feed.running_workflow().map(steer::workflow_caption)
    }

    /// §5 — what the synthetic working row says right now.
    fn working_caption(&self) -> String {
        let workflow = self.running_workflow_caption();
        crate::session_rows::working_caption(&crate::session_rows::WorkingFacts {
            started_at: self.feed.turn_started_at(),
            tokens: self.feed.turn_tokens(),
            now_ms: chrono::Utc::now().timestamp_millis(),
            workflow: workflow.as_deref(),
        })
    }

    /// §5 — the working row's own beat. The duration ticks in SECONDS and the
    /// feed is clock-free, so the row needs a wakeup of its own; it repaints
    /// only while the run is actually working, so an idle session pays a
    /// timer and nothing else.
    fn arm_working_tick(&self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(WORKING_TICK).await;
            if this
                .update(cx, |this, cx| {
                    // EXP-1324: a background task's elapsed time ticks in the
                    // Queue even between turns.
                    if this.working_now() || !this.feed.background_tasks().is_empty() {
                        cx.notify();
                    }
                })
                .is_err()
            {
                return;
            }
        })
        .detach();
    }

    /// EXP-856 §4 — raise the OS notification for a `duplicate` edge, ONCE
    /// per agent id however often the edge repeats (a republished frame, a
    /// reopened tab's replay). The amber row in the transcript is the
    /// in-app half and renders regardless.
    fn note_duplicate_agent(&mut self, event: &steer::ActivityEvent, cx: &mut App) {
        let steer::ActivityEvent::Subagent {
            id,
            status: SubagentStatus::Duplicate,
            detail,
            ..
        } = event
        else {
            return;
        };
        let Some(detail) = detail.as_deref().map(str::trim).filter(|d| !d.is_empty()) else {
            return;
        };
        if !self.duplicates_notified.insert(id.clone()) {
            return;
        }
        crate::os_notifications::raise_duplicate_agent(&self.session_id, detail, cx);
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
        // NEW-2: the contract catalog, then the team's prompts; the agent's
        // own commands stay hidden.
        let prompts: Vec<slash_commands::SlashPrompt> = self
            .row
            .as_ref()
            .and_then(|row| row.team_id.as_deref())
            .filter(|_| sync::Store::try_global(cx).is_some())
            .map(|team_id| {
                crate::queries::team_prompts(cx, team_id)
                    .0
                    .iter()
                    .map(slash_commands::SlashPrompt::from)
                    .collect()
            })
            .unwrap_or_default();
        let items = slash_commands::slash_items(&draft, Some(self.agent()), &prompts);
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
                self.slash_scroll.scroll_to_item(menu.selected);
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
        let command = match menu.items.get(menu.selected).cloned() {
            Some(slash_commands::SlashItem::Command(command)) => command,
            Some(slash_commands::SlashItem::Prompt(prompt)) => {
                self.pick_prompt(prompt, window, cx);
                return;
            }
            None => return,
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

    /// NEW-2 (web `session-composer` `pickPrompt`): a picked prompt's BODY
    /// (`prompts.get`, the only body path: the shape is body-less) REPLACES
    /// the `/name` draft the menu was open on, to be edited and sent. A draft
    /// that moved on meanwhile is left alone; a failed fetch is the
    /// `Couldn't load <name>` toast. The menu stays shut for the draft it
    /// was picked on either way.
    fn pick_prompt(
        &mut self,
        prompt: slash_commands::SlashPrompt,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.slash_dismissed_for = Some(self.input.read(cx).value().to_string());
        cx.notify();
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("load", &prompt.name, None, cx);
            return;
        };
        let id = prompt.id.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::prompts::get(&trpc, &id) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(full) => {
                    if slash_commands::slash_query(&this.input.read(cx).value()).is_none() {
                        return;
                    }
                    let body = full.body;
                    let position = crate::markdown::byte_offset_to_position(&body, body.len());
                    this.slash = None;
                    this.input.update(cx, |state, cx| {
                        state.set_value(body, window, cx);
                        state.set_cursor_position(position, window, cx);
                    });
                    cx.notify();
                }
                Err(err) => crate::action_error::failed_action_err("load", &prompt.name, &err, cx),
            });
        })
        .detach();
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
            // A slash command is the WHOLE message (web's rule): staged
            // attachments would ride a command as a message, so the send
            // refuses with the ×4 copy.
            if !self.pending_images.is_empty() {
                crate::toast::show(
                    crate::toast::Toast::error(slash_commands::ATTACHMENTS_BLOCK_COMMAND),
                    window,
                    cx,
                );
                return;
            }
            if parsed.command.confirm {
                self.prompt_command(parsed.command.name, window, cx);
                return;
            }
        }
        self.send_confirmed(window, cx);
    }

    /// EXP-790: the Stop half of the composer's one button — interrupt the
    /// running turn without ending the session. EXP-1294: `sessions.stop`,
    /// the same call as the header's Stop (the server interrupts the host
    /// through the relay), so it reaches a session hosted anywhere.
    ///
    /// EXP-873: a Stop drops every message the agent has not read yet (the
    /// queue bar, held and sent alike — the CLI's `cancel_queued`), so their
    /// text goes back into the composer, in order, rather than vanishing:
    /// the draft is empty whenever the button shows Stop, and the person
    /// can edit and re-send. The device's empty `queue` frame confirms.
    fn stop_turn(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.tree_scope {
            // A root's composer stops the TREE: every session of it that is
            // working right now, the root's own turn included — and nothing
            // else (a Stop on an idle or queued root would be a no-op at
            // best).
            let mut busy = busy_tree_sessions(&self.session_id, cx);
            if self.working_now() && !busy.contains(&self.session_id) {
                busy.push(self.session_id.clone());
            }
            // One stop action = at most one toast, however many it stops.
            stop_sessions(busy, cx);
        } else {
            stop_session(&self.session_id, cx);
        }
        let unread = self.feed.take_queue();
        if !unread.is_empty() {
            let held = unread
                .iter()
                .map(|message| message.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let draft = restore_unread_to_draft(&self.input.read(cx).value(), &held);
            self.input
                .update(cx, |state, cx| state.set_value(draft, window, cx));
        }
        cx.notify();
    }

    /// EXP-790: Stop shows while the agent is WORKING — a live turn with
    /// nothing waiting on the reader — and only to a reader who may stop it;
    /// Send otherwise, and always once there is a draft to send.
    fn shows_stop(&self, cx: &App) -> bool {
        let has_draft =
            !self.input.read(cx).value().trim().is_empty() || !self.pending_images.is_empty();
        let working = if self.tree_scope {
            self.working_now() || !busy_tree_sessions(&self.session_id, cx).is_empty()
        } else {
            self.working
        };
        working && !has_draft && self.may_message
    }

    fn send_confirmed(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if !self.can_send(cx) {
            return;
        }
        let text = self.input.read(cx).value().to_string();
        self.notice = None;
        // NEW-2: `/effort <level>` changes the launch option exactly like
        // the "+" menu's Effort pick (`sessions.configure`), never a message.
        if let Some(level) = slash_commands::effort_level(&text, self.agent()) {
            self.input.update(cx, |state, cx| state.set_value("", window, cx));
            self.configure_now(
                api::sessions::ConfigureInput {
                    session_id: self.session_id.to_string(),
                    effort: Some(Some(level)),
                    ..Default::default()
                },
                cx,
            );
            return;
        }
        if self.pending_images.is_empty() {
            self.post_message(text, Vec::new(), window, cx);
            return;
        }
        // EXP-702: steer images upload to the SESSION route, not the issue's
        // — a screenshot pasted into a steering conversation is not part of
        // the issue's Files.
        let session_id = self.session_id.clone();
        let Some(transport) = crate::queries::attachment_transport(cx) else {
            crate::action_error::failed_action("upload", "the attachment", None, cx);
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
                        let message = this.pending_images.message(&text);
                        let ids = resolved.into_iter().map(|(_, id, _)| id).collect();
                        this.post_message(message, ids, window, cx);
                    }
                    Err((resolved, error)) => {
                        // Keep what landed so a retry uploads only the rest.
                        this.pending_images.note_uploaded(&resolved);
                        log::warn!("[ui] steer composer upload failed: {error}");
                        // FEED-79: the error toast; Retry sends again (only
                        // the rest uploads).
                        let viewer = cx.entity().downgrade();
                        crate::action_error::failed_action_with_retry(
                            "upload",
                            "the attachment",
                            None,
                            move |window, cx| {
                                let _ = viewer.update(cx, |this, cx| this.send(window, cx));
                            },
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// EXP-1294 — the database is the bus: a message is a `messages` row
    /// (`sessions.send{sessionId}`) the hosting device picks up; one to an
    /// idle session resumes it, one to an ended session continues it. The
    /// draft clears once the row is in; a refusal keeps it and toasts.
    fn post_message(
        &mut self,
        text: String,
        attachment_ids: Vec<String>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let input = api::sessions::SendInput::message(&self.session_id, &text, attachment_ids);
        self.post_message_with(input, window, cx);
    }

    /// [`Self::post_message`]'s send for any `sessions.send` input: the draft
    /// clears once it is in.
    fn post_message_with(
        &mut self,
        input: api::sessions::SendInput,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("send", "the message", None, cx);
            return;
        };
        self.sending = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::sessions::send(&trpc, &input) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.sending = false;
                match result {
                    Ok(_) => this.clear_draft(window, cx),
                    Err(err) => crate::action_error::failed_action_err("send", "the message", &err, cx),
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

    /// EXP-877 — send `text` to the agent as a message, WITHOUT going near
    /// the composer. The model picker's whole implementation: `/model
    /// <alias>` is a prompt the CLI acts on, so it rides `sessions.send` —
    /// the same path Enter ends up on — queues mid-turn like anything else
    /// and the republished `config_state` is the confirmation.
    ///
    /// Deliberately NOT "type it into the field and press Enter": that
    /// clobbered a half-written draft, and a pick made while one was in the
    /// field either did nothing or left `/model opus` sitting there when the
    /// send bailed. A refusal is a toast, never silence.
    fn send_command(&mut self, text: &str, cx: &mut gpui::Context<Self>) {
        if !self.run_open() {
            self.notice = Some(SharedString::from(NOT_CONNECTED));
            cx.notify();
            return;
        }
        // The database is the bus (EXP-1294): the command is a `messages`
        // row like any other, so it reaches a session hosted anywhere —
        // the session view and a root's composer alike.
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("change", "the model", None, cx);
            return;
        };
        let input = api::sessions::SendInput::message(&self.session_id, text, Vec::new());
        self.notice = None;
        cx.notify();
        cx.spawn(async move |_this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::sessions::send(&trpc, &input) })
                .await;
            if let Err(err) = result {
                let _ = cx.update(|cx| crate::action_error::failed_action_err("change", "the model", &err, cx));
            }
        })
        .detach();
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

    /// Attach files (and the "+" menu's "Add file or image"): the native
    /// prompt with NO type filter, like web's any-file input.
    fn pick_files(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        composer_images::pick_attachment_files(window, cx, |this, read, window, cx| this.stage_images(read, window, cx));
    }
}

/// A Past row opened off its recorded transcript.
pub(crate) const REPLAY_BANNER: &str = "Replaying transcript…";

/// …and the same row when the run left nothing to replay (its workspace,
/// and with it the device journal, is gone).
pub(crate) const REPLAY_EMPTY_BANNER: &str = "No transcript for this session";

/// The sessions of `root_id`'s tree that are working right now (the synced
/// `busy`, the ONE spinner input ×4) — what a root's composer Stops.
pub(crate) fn busy_tree_sessions(root_id: &str, cx: &App) -> Vec<String> {
    crate::session_data::tree_of_root(root_id, cx)
        .into_iter()
        .filter(|session| session.busy && !session.is_ended())
        .map(|session| session.id)
        .collect()
}

/// EXP-1294 — who may message and Stop a session: any team member when it
/// is SHARED, only its starter when it is private.
pub(crate) fn may_steer(row: &domain::rows::SessionRow, me: Option<&str>) -> bool {
    row.shared || (me.is_some() && row.user_id.as_deref() == me)
}

/// EXP-1294 — Stop: `sessions.stop` interrupts the session's turn through
/// the relay (a queued one simply ends). S-3: the composer stop-turn's
/// toasts read chats.json `copy.stopTurn`: a refusal = `failed` (+ the
/// reason), a stop no device got (`delivered: false`) = `notDelivered`.
/// Shared with the terminal strip's per-row stop.
pub(crate) fn stop_session(session_id: &str, cx: &mut App) {
    stop_sessions(vec![session_id.to_string()], cx);
}

/// [`stop_session`] over several sessions as ONE stop action (a root's
/// composer stopping its busy tree): every `sessions.stop` goes out, and the
/// outcome is at most one toast — the first refusal.
pub(crate) fn stop_sessions(session_ids: Vec<String>, cx: &mut App) {
    if session_ids.is_empty() {
        return;
    }
    let copy = &domain::chats::CHATS.copy.stop_turn;
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        crate::toast::show_in_active_window(crate::toast::Toast::error(copy.failed.clone()), cx);
        return;
    };
    cx.spawn(async move |cx| {
        let results = cx
            .background_executor()
            .spawn(async move {
                session_ids
                    .iter()
                    .map(|session_id| api::sessions::stop(&trpc, session_id))
                    .collect::<Vec<_>>()
            })
            .await;
        let mut not_delivered = false;
        let mut failure = None;
        for result in results {
            match result {
                Ok(out) => not_delivered |= out.not_delivered(),
                Err(err) => {
                    failure.get_or_insert(err);
                }
            }
        }
        let copy = &domain::chats::CHATS.copy.stop_turn;
        let _ = cx.update(|cx| {
            let toast = match failure {
                Some(err) => {
                    let toast = crate::toast::Toast::error(copy.failed.clone());
                    match crate::action_error::action_error_description(&err) {
                        Some(description) => toast.description(description),
                        None => toast,
                    }
                }
                None if not_delivered => crate::toast::Toast::error(copy.not_delivered.clone()),
                None => return,
            };
            crate::toast::show_in_active_window(toast, cx);
        });
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
        session_id: session_id.to_string(),
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

/// What every failure line starts with, and what one with no reason at all
/// says instead. Both are the marks [`ended_banner`] recognises.
const FAILURE_PREFIX: &str = "Session failed: ";
pub(crate) const FAILURE_FALLBACK: &str = "The session ended with an error.";

/// EXP-758: the banner an ACP run that DIED renders. Pure so the shaping (one
/// line, capped) is unit-tested; a blank reason still says the run FAILED,
/// because the exit callback only hands one over when it did (EXP-877: it
/// used to degrade to the plain "this session has ended" wording, which now
/// means the opposite — nothing at all).
pub(crate) fn failure_banner(error: &str) -> String {
    let flattened = error.split_whitespace().collect::<Vec<_>>().join(" ");
    if flattened.is_empty() {
        return FAILURE_FALLBACK.to_string();
    }
    format!(
        "{FAILURE_PREFIX}{}",
        steer::truncate(&flattened, FAILURE_BANNER_MAX)
    )
}

/// EXP-877 — the ONE line an ENDED run may still put over its transcript: a
/// real failure ([`failure_banner`]). Everything else an `Ended` phase can
/// carry is a publisher token off the wire (`exit:0`, `history` — how the
/// ROOM closed), which is not a sentence and not something a reader can act
/// on; and the plain "this session has ended" strip is gone with them,
/// because the header's status already says so on every transcript anybody
/// ever reopens. Pure, so the rule is a unit test rather than a screenshot.
pub(crate) fn ended_banner(outcome: Option<&str>) -> Option<String> {
    let outcome = outcome?;
    (outcome.starts_with(FAILURE_PREFIX) || outcome == FAILURE_FALLBACK)
        .then(|| outcome.to_string())
}

/// EXP-758: the engine emits `Failed(reason)` and THEN `Ended`, and the feed
/// closing can add a third plain end after both. A later, less specific end
/// must never erase the reason the first one carried.
pub(crate) fn merge_ended_phase(current: &ViewerPhase, next: ViewerPhase) -> ViewerPhase {
    match (current, &next) {
        (ViewerPhase::Ended { outcome: Some(_) }, ViewerPhase::Ended { outcome: None }) => current.clone(),
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

/// EXP-948 — the caption a run of OUR OWN calls renders: the contract's plural
/// copy for the group's tool ("Reading 3 issues" while any call is in flight,
/// "Read 3 issues" once they all settled, plus "· N failed"). The shared
/// [`steer::exp_tool_group_caption`], fed the group's calls, ×4.
pub(crate) fn exp_tool_group_caption(items: &[&FeedItem]) -> String {
    // Every member, like the fixture replay in `steer::feed` — a non-tool
    // degrades inside `exp_tool_group_call` itself, so both read the same
    // group (release review R3).
    let calls: Vec<steer::ExpToolGroupCall<'_>> =
        items.iter().map(|item| item.exp_tool_group_call()).collect();
    steer::exp_tool_group_caption(&calls)
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
/// composer and takes its free text inline. EXP-877 shortened it to the ONE
/// thing it has to teach — byte-identical with the web.
pub(crate) const COMPOSER_PLACEHOLDER: &str = "Type / for commands";

/// The element ids of one composer's controls — the session view's own,
/// or a host's ([`SteerSessionView::host_root_composer`]: the thread screen
/// and the agent channel name theirs, so captures and tests tell them apart).
/// EXP-1334: a running session's MCP pick as the "+" menu offers it.
#[derive(Clone, Debug, Default)]
struct SessionMcp {
    /// `(id, name, blocked)`: the team's servers, greyed where the viewer
    /// has not connected.
    servers: Vec<(String, String, bool)>,
    /// The session's pick (sorted ids).
    picks: Vec<String>,
}

/// EXP-1334: how many times the "+" menu's MCP row may be loaded before the
/// view gives up on it for good, and how long a failed load waits before
/// the next try.
const MCP_LOAD_ATTEMPTS: u8 = 3;
const MCP_RETRY_AFTER: Duration = Duration::from_secs(5);

/// EXP-1334: the option picks ahead of the synced row (web
/// `useSessionComposerOptions`' `Overrides`). `None` = read the row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct OptionPicks {
    /// `Some(None)` = the default effort was picked.
    effort: Option<Option<String>>,
    ultracode: Option<bool>,
    computer_use: Option<bool>,
}

impl OptionPicks {
    /// Take `input`'s picks (a later pick replaces an earlier one).
    fn apply(&mut self, input: &api::sessions::ConfigureInput) {
        if let Some(effort) = input.effort.as_ref() {
            self.effort = Some(effort.clone());
        }
        if let Some(on) = input.ultracode {
            self.ultracode = Some(on);
        }
        if let Some(on) = input.computer_use {
            self.computer_use = Some(on);
        }
    }

    /// Drop the picks `made`, unless a later pick replaced them (web
    /// `dropSettled`).
    fn drop_settled(&mut self, made: &OptionPicks) {
        if made.effort.is_some() && self.effort == made.effort {
            self.effort = None;
        }
        if made.ultracode.is_some() && self.ultracode == made.ultracode {
            self.ultracode = None;
        }
        if made.computer_use.is_some() && self.computer_use == made.computer_use {
            self.computer_use = None;
        }
    }

    /// Drop every pick the synced row already says.
    fn drop_matching(&mut self, row: &domain::rows::SessionRow) {
        if self.effort.as_ref() == Some(&row.effort) {
            self.effort = None;
        }
        if self.ultracode == Some(row.ultracode) {
            self.ultracode = None;
        }
        if self.computer_use == Some(row.computer_use) {
            self.computer_use = None;
        }
    }
}

/// EXP-1334: one `sessions.configure` waiting its turn, with what to put
/// back if the server refuses it.
struct QueuedConfigure {
    input: api::sessions::ConfigureInput,
    /// The option picks this configure made (dropped again on failure).
    picks: OptionPicks,
    /// The MCP pick this configure replaced (restored on failure).
    previous_mcp: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ComposerIds {
    pub(crate) send: SharedString,
    pub(crate) attach: SharedString,
    pub(crate) model: SharedString,
    pub(crate) context: SharedString,
}

impl ComposerIds {
    /// The session view's own.
    pub(crate) fn session() -> Self {
        Self::new("steer-send", "steer-attach", "steer-model", "steer-context")
    }

    pub(crate) fn new(
        send: impl Into<SharedString>,
        attach: impl Into<SharedString>,
        model: impl Into<SharedString>,
        context: impl Into<SharedString>,
    ) -> Self {
        Self {
            send: send.into(),
            attach: attach.into(),
            model: model.into(),
            context: context.into(),
        }
    }
}

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

/// EXP-873: the composer draft after a Stop hands the bar's unread text
/// back — an empty draft becomes the text, a draft in progress keeps its
/// place and the text lands after a blank line (web parity).
fn restore_unread_to_draft(draft: &str, held: &str) -> String {
    if draft.trim().is_empty() {
        held.to_string()
    } else {
        format!("{draft}\n\n{held}")
    }
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

/// EXP-1294 — the unanswered `question` / `plan` row whose `questionId` is
/// `question_id` (the newest, should a re-emission have posted two). Only
/// that: an engine ask with no row yet, while an agent's
/// `exponential_sessions_ask` card (no `questionId`) waits, used to fall
/// back onto the newest open card of its kind and stamp the engine's
/// answer into the tool card.
fn card_row_id(messages: &[domain::rows::MessageRow], question_id: &str) -> Option<String> {
    messages
        .iter()
        .rev()
        .find(|message| {
            message.answered_at.is_none()
                && matches!(message.card_kind(), Some("question") | Some("plan"))
                && message.card.as_ref().and_then(|card| card.get("questionId")).and_then(serde_json::Value::as_str)
                    == Some(question_id)
        })
        .map(|message| message.id.clone())
}

/// EXP-820 — a plan card's LAST option is its reject ("No, keep planning"),
/// the one whose inline text is a change request.
fn is_plan_reject(card: &steer::feed::QuestionCard, index: usize) -> bool {
    card.plan_mode && !card.options.is_empty() && index + 1 == card.options.len()
}

/// EXP-1294 — a card's answer as `sessions.answer` takes it: a plan picker
/// DECIDES (its last option is the reject, carrying the "keep planning"
/// note as `feedback`); a question answers with its free text, else the
/// picked labels.
fn card_answer_input(
    card: &steer::feed::QuestionCard,
    message_id: String,
    keys: &[String],
    labels: &[String],
    text: Option<&str>,
    feedback: Option<String>,
) -> api::sessions::AnswerInput {
    if card.plan_mode {
        let rejected = keys.iter().any(|key| {
            card.options
                .iter()
                .position(|option| &option.key == key)
                .is_some_and(|index| is_plan_reject(card, index))
        });
        // The engine's plan picker has no fresh-context option (an
        // ExitPlanMode is a plain allow): a session-view approval is never
        // `fresh`.
        return api::sessions::AnswerInput {
            message_id,
            answer: None,
            decision: Some(if rejected { "rejected" } else { "approved" }.to_string()),
            fresh: None,
            feedback: feedback.filter(|_| rejected),
            ..Default::default()
        };
    }
    // EXP-1280: a multi-select's picks ride `answers` too (`answer` = the
    // labels joined, the compat string).
    let answers = (text.is_none() && card.multi_select).then(|| serde_json::json!(labels));
    api::sessions::AnswerInput {
        message_id,
        answer: Some(text.map_or_else(|| labels.join(", "), str::to_string)),
        answers,
        ..Default::default()
    }
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

/// EXP-818: how much of the transcript column a user bubble may take (web
/// `max-w-[85%]`), and the narrowest bubble worth drawing.
const USER_BUBBLE_MAX_FRACTION: f32 = 0.85;
const USER_BUBBLE_MIN_W: f32 = 48.;

/// The `config_state.options` id the footer's model pin reads (EXP-877 — the
/// only option the wire carries).
const CONFIG_MODEL_ID: &str = "model";

/// What a message that could not leave says. One string, so the composer's
/// send and the footer's model pick report the same failure.
const NOT_CONNECTED: &str = "The session is no longer connected";


/// EXP-877 — a model VALUE as the footer prints it: the picker's own label
/// for an alias this build knows (`opus` → `Opus`, byte-identical with
/// [`crate::coding_selects::MODEL_CHOICES`] and the web's `modelLabel`), else
/// the raw value with its first letter raised — a codex slug or a tier this
/// build has never heard of still reads as a name rather than as an id.
pub(crate) fn model_label(value: &str) -> String {
    if let Some((label, _)) = crate::coding_selects::MODEL_CHOICES
        .iter()
        .find(|(_, alias)| *alias == value)
    {
        return (*label).to_string();
    }
    if let Some((label, _)) = crate::coding_selects::CODEX_MODEL_CHOICES
        .iter()
        .find(|(_, slug)| !slug.is_empty() && *slug == value)
    {
        return (*label).to_string();
    }
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
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

/// EXP-784 — the rate-limit banner's two spans: the agent's message (or a
/// generic one) and, when the report named a reset, `resets in 2h 10m`
/// (EXP-818: relative, the usage cards' countdown — a clock reading `00:00`
/// looked like a zero). Two SEPARATE spans, never ` · `-joined: the message
/// is amber and truncates, the countdown stays muted and whole (the web
/// banner, exactly).
pub(crate) fn rate_limit_caption(
    message: Option<&str>,
    countdown: Option<String>,
) -> (String, Option<String>) {
    let message = message
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or("Rate limit reached");
    (message.to_string(), countdown)
}

/// FEED-61: what a wall is scoped to. A usage window belongs to the ACCOUNT,
/// so a model switch never lifts it — another account does (the pill beside
/// it). Byte-identical ×4 (web `RATE_LIMIT_ACCOUNT_NOTE`).
pub(crate) const RATE_LIMIT_ACCOUNT_NOTE: &str = "per account";

/// The banner's muted span: the countdown when a reset is known, then the
/// scope — `resets in 2h 10m · per account`, or the bare note (web
/// `rateLimitDetail`).
pub(crate) fn rate_limit_detail(countdown: Option<String>) -> String {
    match countdown {
        Some(countdown) => format!("{countdown} · {RATE_LIMIT_ACCOUNT_NOTE}"),
        None => RATE_LIMIT_ACCOUNT_NOTE.to_string(),
    }
}

/// EXP-1191: an option row's number key — the web `kbd` at the RIGHT end of
/// the row: a 4px-rounded hairline chip, monospace 10px on a 16px line,
/// muted (on the primary row the primary foreground at 30% / 80%).
fn key_chip(index: usize, primary: bool, cx: &App) -> Option<AnyElement> {
    let digit = (index < 9).then(|| (index + 1).to_string())?;
    let (border, text) = if primary {
        let fg = cx.theme().primary_foreground;
        (fg.opacity(0.3), fg.opacity(0.8))
    } else {
        (
            theme::tokens::glass::STROKE_CARD.to_hsla(),
            cx.theme().muted_foreground,
        )
    };
    Some(
        div()
            .flex_shrink_0()
            .px_1()
            .rounded(px(4.))
            .border_1()
            .border_color(border)
            .text_size(px(10.))
            .line_height(px(16.))
            .font_family(theme::terminal::FONT_FAMILY)
            .text_color(text)
            .child(SharedString::from(digit))
            .into_any_element(),
    )
}

/// EXP-1316 — ONE option row of THE question / plan card (the session
/// view's and the thread's, EXP-788/EXP-1191): a full-width button, the
/// label column (its description under it) and the number key chip at the
/// right end. `primary` = the promoted option (the app's PRIMARY button),
/// `picked` = the glass active fill, `highlighted` = the keyboard cursor's
/// strong stroke. The caller wires the click.
#[allow(clippy::too_many_arguments)] // every flag is a render decision
pub(crate) fn ask_option_button(
    id: impl Into<gpui::ElementId>,
    label: String,
    description: Option<String>,
    primary: bool,
    picked: bool,
    highlighted: bool,
    index: usize,
    cx: &App,
) -> Button {
    let trailing = key_chip(index, primary, cx);
    ask_option_button_trailing(id, label, description, primary, picked, highlighted, None, trailing, cx)
}

/// EXP-1280 — a CHOICE row of the plan / question card (plan-card.json
/// `card`): THE option row; single = the foreground check glyph at its
/// right end when picked, multi = a leading checkbox (THE square,
/// `controls::checkbox`) and no trailing check (a choice toggles; it never answers by itself, so
/// it carries no number key).
pub(crate) fn ask_choice_button(
    id: impl Into<gpui::ElementId>,
    label: String,
    description: Option<String>,
    picked: bool,
    multi: bool,
    cx: &App,
) -> Button {
    // THE checkbox (the button's click toggles it; the box only paints).
    let leading = multi.then(|| {
        crate::controls::checkbox("choice-check", picked.into(), false, cx).into_any_element()
    });
    let check = (picked && !multi)
        .then(|| Icon::new(registry::UI_CHECK).xsmall().flex_shrink_0().text_color(cx.theme().foreground).into_any_element());
    ask_option_button_trailing(id, label, description, false, picked, false, leading, check, cx)
}

/// [`ask_option_button`] with any `trailing` element at the row's right end.
#[allow(clippy::too_many_arguments)] // every flag is a render decision
fn ask_option_button_trailing(
    id: impl Into<gpui::ElementId>,
    label: String,
    description: Option<String>,
    primary: bool,
    picked: bool,
    highlighted: bool,
    leading: Option<AnyElement>,
    trailing: Option<AnyElement>,
    cx: &App,
) -> Button {
    let muted = cx.theme().muted_foreground;
    let label_color = if primary { cx.theme().primary_foreground } else { cx.theme().foreground };
    let mut button = Button::new(id)
        .cursor_pointer()
        .w_full()
        .h_auto()
        .min_h(px(domain::plan_card::card().row_min_height))
        .px_3()
        .py_1p5()
        .rounded(px(domain::plan_card::card().row_radius));
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
    let description = description.filter(|text| !text.trim().is_empty());
    button.child(
        // EXP-1191: web — the label column, then the key chip at
        // the RIGHT end of the row.
        h_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .items_center()
            .children(leading)
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
                            .font_weight(gpui::FontWeight::MEDIUM)
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
                                .text_color(if primary { label_color.opacity(0.8) } else { muted })
                                .child(SharedString::from(description)),
                        )
                    }),
            )
            .children(trailing),
    )
}

/// EXP-1316 — THE card's closed line: the check (or ×) and the chosen
/// answer / decision (`Answered` without one, `Dismissed`).
pub(crate) fn ask_resolved_line(answer: Option<&str>, dismissed: bool, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    h_flex()
        .gap_1p5()
        .items_center()
        .mt_1()
        .child(
            Icon::new(if dismissed { registry::UI_CLOSE } else { registry::UI_CHECK })
                .xsmall()
                .text_color(if dismissed { muted } else { theme::tokens::GREEN.to_hsla() }),
        )
        .child(div().text_xs().font_weight(gpui::FontWeight::MEDIUM).child(SharedString::from(if dismissed {
            "Dismissed".to_string()
        } else {
            answer.unwrap_or(&domain::plan_card::copy().answered).to_string()
        })))
        .into_any_element()
}

/// EXP-820/EXP-1316 — THE card's inline free-text field under its option
/// row: the glass text field plus the composer's round Send (`send`, wired
/// by the caller).
pub(crate) fn ask_inline_field(
    state: &Entity<InputState>,
    send: impl IntoElement,
    window: &Window,
    cx: &App,
) -> gpui::Div {
    h_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .items_center()
        .mt_0p5()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(crate::controls::glass_input(state, window, cx).small()),
        )
        .child(send)
}

/// EXP-1191: the shared question / plan card (web `AskCard`) — the glass
/// card, 12px padding, ONE top-aligned row: the 14px accent glyph beside a
/// column holding the optional heading (12px medium, accent; `meta` = the
/// stepper's "k of n" caption), then `body`. The glyph sits on the first
/// text line, the options indent under the text.
pub(crate) fn ask_card(
    plan: bool,
    heading: Option<SharedString>,
    meta: Option<String>,
    body: gpui::Div,
    cx: &App,
) -> AnyElement {
    let meta = meta.map(|meta| {
        // EXP-698: 11px — a caption beside the heading, never a peer of it.
        div()
            .flex_shrink_0()
            .text_2xs()
            .text_color(cx.theme().muted_foreground)
            .child(SharedString::from(meta))
            .into_any_element()
    });
    ask_card_chrome(plan, heading, meta, None, body, cx).into_any_element()
}

/// [`ask_card`]'s chrome with any `meta` element beside the heading (the
/// ask dock's session caption) and an optional `back` control LEADING the
/// header row, before the kind glyph (the ask dock's focused card,
/// `fixtures/ask-dock.json`). The bare card, so a caller can make it a
/// button (the dock's compact cards).
pub(crate) fn ask_card_chrome(
    plan: bool,
    heading: Option<SharedString>,
    meta: Option<AnyElement>,
    back: Option<AnyElement>,
    body: gpui::Div,
    cx: &App,
) -> gpui::Div {
    let accent = if plan {
        cx.theme().primary
    } else {
        theme::tokens::YELLOW.to_hsla()
    };
    let has_heading = heading.is_some() || meta.is_some();
    crate::surface::glass_card()
        .w_full()
        .min_w_0()
        .p_3()
        .child(
            h_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .items_start()
                // Web `-my-1 -ml-1`: the 24px ghost button overhangs the
                // 16px heading line instead of growing the card.
                .children(back.map(|back| div().flex_shrink_0().my(px(-4.)).ml(px(-4.)).child(back)))
                .child(
                    // Centred on the first line: the 16px heading line, or
                    // the body's 22px one.
                    div()
                        .flex_shrink_0()
                        .mt(px(if has_heading { 1. } else { 4. }))
                        .child(
                            Icon::new(if plan {
                                registry::CODING_PLAN
                            } else {
                                registry::UI_HELP
                            })
                            .with_size(px(14.))
                            .text_color(accent),
                        ),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .when(has_heading, |this| {
                            this.child(
                                h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .mb_1()
                                    .gap_2()
                                    .items_center()
                                    .when_some(heading, |this, heading| {
                                        this.child(
                                            div()
                                                .min_w_0()
                                                .truncate()
                                                .text_xs()
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(accent)
                                                .child(heading),
                                        )
                                    })
                                    .children(meta),
                            )
                        })
                        .child(body),
                ),
        )
}

/// The question body (web `text-sm text-foreground/90`).
pub(crate) fn ask_body_text(cx: &App) -> gpui::Div {
    body_text(div())
        .w_full()
        .min_w_0()
        .text_color(cx.theme().foreground.opacity(0.9))
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// EXP-1175 → EXP-1294 — the session view's status row: the agent's mark
/// for `state` ([`crate::run_rows::run_mark`]) beside the state word in its
/// tone (`session-card.json`): the styleguide's `run-status-row` (N-11: the
/// session view no longer draws it over the transcript).
pub(crate) fn session_status_row(
    id: impl Into<gpui::ElementId>,
    agent: Option<coding::CodingAgent>,
    state: domain::session_tree::SessionState,
    word: &str,
    cx: &App,
) -> AnyElement {
    h_flex()
        .id(id)
        .w_full()
        .min_w_0()
        .items_start()
        .gap_2()
        .child(crate::run_rows::run_mark(agent, state, cx))
        .child(
            v_flex().flex_1().min_w_0().child(
                div()
                    .text_sm()
                    .text_color(crate::work_card::session_tone_color(state.tone(), cx))
                    .child(SharedString::from(word.to_string())),
            ),
        )
        .into_any_element()
}

impl SteerSessionView {
    fn render_feed(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        if self.feed.is_empty() {
            // S-3: no paused body (an offline host is invisible).
            let body: AnyElement = if matches!(
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
        // EXP-787: the reading column (and, EXP-1191, the work gutter) is
        // laid out per ROW ([`Self::transcript_row`]) rather than by wrapping
        // the list in a narrower box, so the list keeps measuring and
        // scrolling over the full pane width; it carries no side inset of
        // its own, so its rows centre on the pane exactly as the header's
        // column does.
        let list = list(
            self.list.clone(),
            cx.processor(|this, ix: usize, window, cx| this.render_list_row(ix, window, cx)),
        )
        .pt_2()
        // N-12: the last row clears THE Dock's band (its measured height).
        .pb(px(self.dock.read(cx).band_height() + crate::dock::DOCK_GAP));
        crate::scroll_pane::v_list_pane(list, &self.list).into_any_element()
    }

    /// EXP-1228 — the transcript's right-click menu: the composer's own
    /// native menu (gpui-component `Textarea` ships Cut · Copy · Paste ·
    /// Select All), reduced to the one row a read-only transcript can take.
    /// Copy is disabled without a selection. It copies what ⌘C copies over
    /// the transcript (gpui-component `Root`'s copy: the window text
    /// selection), read NOW and carried by [`CopyTranscriptSelection`]
    /// (EXP-1266) — a focused composer's own Copy would swallow the keyboard
    /// action, so the row is an action of its own.
    fn on_feed_context_menu(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        ensure_copy_action_registered(cx);
        let text = gpui_base::TextSelection::selected_text(window, cx)
            .trim()
            .to_string();
        gpui_component::native_menu::NativeMenu::new()
            .menu_with_disabled(
                FEED_MENU_COPY,
                text.is_empty(),
                Box::new(CopyTranscriptSelection { text }),
            )
            .show(event.position, window, cx);
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
    /// EXP-927 moved that clamp into [`work_column_row`] — the tabs above the
    /// transcript and the strip blocks below it take the same one now, so a
    /// row here is the ladder gap and nothing else.
    fn transcript_row(&self, ix: usize, element: AnyElement) -> AnyElement {
        work_column_row(element)
            .pt(self.row_gap(ix))
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
            // §3: past the transcript sit the orphan workflow cards, then the
            // synthetic "Working…" line.
            let past = ix - self.rows.len();
            let orphan = self
                .orphan_workflows
                .get(past)
                .and_then(|id| self.feed.workflow_for(id))
                .cloned();
            if let Some(workflow) = orphan {
                let card = self.render_workflow_card(&workflow, None, cx);
                return self.transcript_row(ix, card);
            }
            // Thread 58cb32df: the unanchored conversation cards.
            let tail = past.checked_sub(self.orphan_workflows.len()).and_then(|t| self.conv_tail().get(t)).copied();
            return match tail {
                Some(card) => {
                    let element = self.render_conversation_cards(&[card], window, cx);
                    self.transcript_row(ix, element)
                }
                None => self.transcript_row(ix, self.render_working_row(cx)),
            };
        };
        let live = self.phase == ViewerPhase::Live;
        let last_row = self.rows.len().saturating_sub(1);
        let spec = spec.clone();
        let row = spec.resolve(self.feed.items());
        let element = self.render_row(&row, ix == last_row && live, &self.active, window, cx);
        // Thread 58cb32df: the conversation cards the row's calls posted.
        let anchored: Vec<usize> = spec
            .item_indices()
            .iter()
            .filter_map(|&index| self.feed.items().get(index))
            .filter_map(|item| self.conv_anchors.get(&item.id))
            .flatten()
            .copied()
            .collect();
        // …and the first row carries the unanchored ones above it (web's
        // `head`: their calls are above the loaded window).
        let head: Vec<usize> = if ix == 0 { self.conv_head().to_vec() } else { Vec::new() };
        let element = if anchored.is_empty() && head.is_empty() {
            element
        } else {
            v_flex()
                .w_full()
                .min_w_0()
                .gap(px(transcript::GAP_BLOCK))
                .when(!head.is_empty(), |column| column.child(self.render_conversation_cards(&head, window, cx)))
                .child(element)
                .when(!anchored.is_empty(), |column| column.child(self.render_conversation_cards(&anchored, window, cx)))
                .into_any_element()
        };
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
            // EXP-850 §3: a `Workflow` call never reaches a tool RUN — the
            // grouping keeps it a row of its own (`is_workflow_call`), which
            // renders as the card.
            FeedRow::ToolRun { id, items } => self.render_tool_run(*id, items, live_tail, window, cx),
            // EXP-916: a run of edits is the ONE edited-files card.
            FeedRow::Edits { id, items } => self.render_edits_card(*id, items, cx),
            // EXP-948: a run of the SAME Exponential tool is its own captioned
            // row — our work is never folded away as "N other tools".
            FeedRow::ExpRun { id, items } => self.render_exp_tool_run(*id, items, live_tail, window, cx),
            FeedRow::Ask { id, items, .. } => self.render_ask(*id, items, active, window, cx),
            FeedRow::Subagent { id, items, .. } => self.render_subagent(*id, items, window, cx),
        }
    }

    /// The agent's prose: its mark beside the chat-rhythm markdown.
    fn render_narration(&self, key: SharedString, text: String, cx: &App) -> AnyElement {
        narration_row(self.with_issue_chips(self.prose(key, text), cx), cx)
    }

    /// The transcript's prose view. EXP-698: the feed reads at the chat
    /// rhythm, and its inline code takes the semantic tint.
    fn prose(&self, key: SharedString, text: String) -> crate::markdown::MarkdownView {
        crate::markdown::MarkdownView::new(key, text)
            .chat(true)
            .autolink(true)
            .selectable(true)
            .images(self.images.clone())
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
            FeedKind::Narration { text, .. } => {
                self.render_narration(SharedString::from(format!("steer-narration-{}", item.id)), text.clone(), cx)
            }
            // Thread 58cb32df (chats.json TRANSCRIPT): a person's `/new` or
            // `/compact` line is THE divider pill, never a bubble.
            FeedKind::UserMessage { text, .. } if domain::chats::command_divider(text).is_some() => {
                let kind = domain::chats::command_divider(text).expect("matched above");
                let time = self.divider_time(item.id, kind, cx);
                crate::surface_card::divider_pill(&format!("steer-{}", item.id), kind, &time, cx)
            }
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
                        .child(Icon::new(registry::CODING_COMMAND).xsmall().text_color(muted.opacity(0.7)))
                        .child(
                            div()
                                .flex_shrink_0()
                                .font_family(theme::terminal::FONT_FAMILY)
                                .child(SharedString::from(format!("/{}", parsed.command.name))),
                        )
                        .when(!parsed.args.is_empty(), |this| {
                            this.child(
                                div().min_w_0().truncate().text_color(muted).child(SharedString::from(parsed.args.clone())),
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
            // EXP-850 §3: a `Workflow` call renders as its CARD, never as a
            // tool row — the card is the row (looked up by the call's id).
            FeedKind::Tool { .. } => match self.workflow_of(item).cloned() {
                Some(workflow) => self.render_workflow_card(&workflow, Some(item), cx),
                None => self.render_tool_item(item, self.tool_row_mode(item), window, cx),
            },
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
            FeedKind::ApiError {
                message,
                error_type,
                ..
            } => self.render_api_error_row(message, error_type.as_deref(), cx),
            FeedKind::Thought { .. } => self.render_thought(item, window, cx),
        }
    }

    /// EXP-1324: the Reasoning line — "Thinking" (shimmering) while the
    /// thought streams, "Thought for 5s" once it closed. This run's own
    /// engine kept the text, so here the line opens to it; a remote
    /// viewer's line is not expandable (the wire carries no thought text).
    fn render_thought(&self, item: &FeedItem, window: &Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let FeedKind::Thought { thought_id, running, duration_ms, .. } = &item.kind else {
            return div().into_any_element();
        };
        let model = steer::trace::trace_reasoning(*running, *duration_ms);
        let line = TraceLine::from_model(("steer-thought", item.id as usize), &model);
        let Some(text) = self.extras.thought_text(thought_id) else {
            return div()
                .id(("session-thought-line", item.id as usize))
                .w_full()
                .child(trace_line(line, window, cx))
                .into_any_element();
        };
        let id = item.id;
        let open = self.expanded_extras.contains(&id);
        let content = open.then(|| {
            div()
                .w_full()
                .min_w_0()
                .whitespace_normal()
                .child(SharedString::from(text.trim().to_string()))
                .into_any_element()
        });
        let entity = cx.entity().downgrade();
        div()
            .id(("session-thought-line", id as usize))
            .w_full()
            .child(trace_collapsible(
                line,
                open,
                move |_, cx| {
                    let _ = entity.update(cx, |this, cx| {
                        if !this.expanded_extras.insert(id) {
                            this.expanded_extras.remove(&id);
                        }
                        cx.notify();
                    });
                },
                content,
                window,
                cx,
            ))
            .into_any_element()
    }

    /// A transient failure the agent CLI reported inline (claude's
    /// `API Error: No response from API…`, a dropped connection, a 400): a
    /// quiet transcript ROW like narration — never a slot, never a wall, so
    /// it never reads as a rate limit. The message is PLAIN text (never
    /// markdown) and wraps; the error type trails as a muted caption.
    fn render_api_error_row(
        &self,
        message: &str,
        error_type: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let caption = error_type
            .map(|kind| kind.trim().replace('_', " "))
            .filter(|kind| !kind.is_empty());
        // ONE line, the web `ApiErrorRow`: glyph · wrapping message · the
        // error type TRAILING on the same line (never under the message).
        h_flex()
            .w_full()
            .min_w_0()
            .gap_1p5()
            .items_start()
            .text_xs()
            .child(
                div().mt(px(2.)).flex_shrink_0().child(Icon::new(registry::UI_WARNING).xsmall().text_color(danger.opacity(0.7))),
            )
            .child(div().flex_1().min_w_0().text_color(muted).child(SharedString::from(message.trim_end().to_string())))
            .when_some(caption, |this, caption| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_color(muted.opacity(0.6))
                        .child(SharedString::from(caption)),
                )
            })
            .into_any_element()
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
        // EXP-877: the transcript column is the work column, narrowed by a
        // pane too small to hold it (`transcript_row` lays the row out at
        // `flex_basis(WORK_COLUMN_W)` with `flex_shrink(1)`). The unmeasured
        // first frame takes the full column.
        let view = f32::from(self.view_width.get());
        let column = if view > 0. {
            view.min(crate::work_header::WORK_COLUMN_W)
        } else {
            crate::work_header::WORK_COLUMN_W
        } - 2. * crate::work_header::WORK_GUTTER;
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
        // Wave D: trailing FILE lines render as the comment thread's file
        // chips under the rest of the message (never a link, never a broken
        // image); the rest takes the images-only path unchanged.
        if !parsed.files.is_empty() {
            let rest = build_steer_image_message(&parsed.text, &parsed.attachment_ids);
            let mut chips = h_flex().w_full().min_w_0().flex_wrap().gap_1();
            for (index, file) in parsed.files.iter().enumerate() {
                chips = chips.child(crate::comment_attachments::steer_file_chip(
                    SharedString::from(format!("steer-msg-file-{id}-{index}")),
                    file.id.clone(),
                    file.name.clone(),
                    cx,
                ));
            }
            return v_flex()
                .w_full()
                .min_w_0()
                .gap_1()
                .when(!rest.is_empty(), |this| this.child(self.render_user_message(id, &rest, cx)))
                .child(chips)
                .into_any_element();
        }
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
                    MarkerSegment::Text(body) => row.child(div().min_w_0().child(SharedString::from(body))),
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

    /// EXP-895 — the mode row `item` renders in: [`ToolRowMode::Live`] for the
    /// ONE call still running ([`steer::feed::live_tool_row_id`], and only in a
    /// LIVE run — an ended run's trailing unsettled row is history, not a
    /// tail), [`ToolRowMode::Settled`] for every other row. A
    /// [`ToolRowMode::Nested`] row is named by its caller, never derived here.
    fn tool_row_mode(&self, item: &FeedItem) -> ToolRowMode {
        if self.phase == ViewerPhase::Live
            && steer::feed::live_tool_row_id(self.feed.items()) == Some(item.id)
        {
            ToolRowMode::Live
        } else {
            ToolRowMode::Settled
        }
    }

    /// EXP-1172 — the inline tile a feed item carries: a SETTLED, successful
    /// call whose contract result kind is `picture` (`sessions_show`) and
    /// whose `preview.id` resolves in the run's synced results; `None` for
    /// anything else (an upload in flight included).
    fn inline_picture_for(
        &self,
        item: &FeedItem,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let FeedKind::Tool {
            name,
            failed,
            settled,
            preview,
            ..
        } = &item.kind
        else {
            return None;
        };
        let display = steer::exp_tool_display(name, *settled)?;
        if display.result != steer::exp_tool::result::PICTURE || !*settled || *failed {
            return None;
        }
        let attachment_id = preview.as_ref()?.id.as_deref()?;
        self.render_inline_picture(item.id, attachment_id, cx)
    }

    /// EXP-1172 — the picture an `exponential_sessions_show` call filed, as
    /// ONE Results tile under its row: looked up by `attachment_id` (the
    /// call's `preview.id`) in the tree root's synced Guide, so it appears the
    /// moment the row syncs and follows a re-publish or removal. The base is
    /// [`SESSION_INLINE_TILE_HEIGHT`], fitted to the transcript column by the
    /// shared ×4 rule; the caption is the call's text, else the label; a
    /// click opens the same lightbox a Results tile does. `None` = nothing
    /// to show yet.
    fn render_inline_picture(
        &self,
        id: FeedItemId,
        attachment_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        use domain::session_results as results;
        let entry = results::session_result_picture(
            self.guide_results.as_ref(),
            attachment_id,
        )?;
        Some(self.inline_tile(
            &entry,
            SharedString::from(format!("steer-exp-picture-{}", id as usize)),
            cx,
        ))
    }

    /// EXP-1172 — one published picture as the transcript's inline tile.
    fn inline_tile(
        &self,
        entry: &domain::session_results::SessionResultEntry,
        id: SharedString,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        use domain::session_results as results;
        // The transcript column's content box (the work column, narrowed by
        // a pane too small for it, inside the work gutter) minus the row's
        // `pl_5` indent. The unmeasured first frame takes the full column.
        let view = f32::from(self.view_width.get());
        let column = if view > 0. {
            view.min(crate::work_header::WORK_COLUMN_W)
        } else {
            crate::work_header::WORK_COLUMN_W
        } - 2. * crate::work_header::WORK_GUTTER;
        let height = results::session_result_tile_height_fitting_from(
            std::slice::from_ref(entry),
            column - 20.,
            results::SESSION_INLINE_TILE_HEIGHT,
        );
        crate::session_results::tile(
            entry,
            height,
            id,
            results::session_result_tile_caption(entry).to_string(),
            &self.images,
            None,
            cx,
        )
    }

    /// One tool call's row plus whatever hangs off it — EXP-1324: a Trace
    /// line ("Ran ./gradlew …"). A SETTLED call that printed is collapsible
    /// onto its Output CodeBlock (`execute` = `ansi`, a failed call the
    /// destructive variant); a settled `execute` whose output parses as a
    /// test run with failures adds the failed-tests line under it.
    ///
    /// EXP-1206: the row still RUNNING is its headline alone (it shimmers);
    /// every settled row folds the WIRE's own tail-cut output, on the runner
    /// exactly as on a viewer (EXP-895 runner parity).
    fn render_tool_item(
        &self,
        item: &FeedItem,
        mode: ToolRowMode,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let FeedKind::Tool {
            name,
            detail,
            failed,
            output,
            settled,
            preview,
            diff,
            tool_kind,
            ..
        } = &item.kind
        else {
            return div().into_any_element();
        };
        // EXP-1279: a code-mode script is its Script card.
        if steer::script_card::is_script_tool(name) {
            return self.render_script_card(item, cx);
        }
        // EXP-1241: a settled comment call shows the comment it posted.
        if let Some(card) = self.render_comment_call(item, cx) {
            return card;
        }
        let id = item.id;
        let kind = tool_kind.map(|kind| kind.as_str()).unwrap_or("other");
        // The live tail never streams output (EXP-1206).
        let printed = (mode != ToolRowMode::Live)
            .then_some(output.as_deref())
            .flatten()
            .filter(|text| !text.trim().is_empty());
        let weak = cx.entity().downgrade();
        let toggle = |set: fn(&mut Self) -> &mut HashSet<FeedItemId>| {
            let entity = weak.clone();
            move |_: &mut Window, cx: &mut App| {
                let _ = entity.update(cx, |this, cx| {
                    let set = set(this);
                    if !set.insert(id) {
                        set.remove(&id);
                    }
                    cx.notify();
                });
            }
        };
        let output_block = |printed: &str, destructive: bool, key: &str, cx: &App| {
            crate::code_block::code_block(
                crate::code_block::CodeBlockProps {
                    id: SharedString::from(format!("steer-tool-{key}-{id}")),
                    title: Some(SharedString::from(steer::trace::copy::OUTPUT)),
                    code: printed.to_string(),
                    language: if kind == "execute" { "ansi" } else { "plain" },
                    destructive,
                    copyable: true,
                },
                cx,
            )
        };
        // EXP-846: one of OUR MCP calls reads as product work (the mark, the
        // contract's caption, the answer's preview); everything else is the
        // Trace tool line.
        let row = match steer::exp_tool_display(name, *settled) {
            Some(display) => {
                // EXP-1172: a settled `sessions_show` call's picture, once it
                // is in the run's synced results (an upload in flight = none).
                let picture = self.inline_picture_for(item, cx);
                exp_tool_call_row(
                    item.id,
                    display,
                    detail.as_deref(),
                    *failed,
                    *settled,
                    preview.as_ref(),
                    picture,
                    window,
                    cx,
                )
            }
            None => {
                let model = steer::trace::trace_tool(kind, name, detail.as_deref(), *settled, *failed);
                let line = TraceLine::from_model(("steer-tool", id as usize), &model);
                match printed {
                    Some(printed) => {
                        let open = self.expanded_extras.contains(&id);
                        let content = open.then(|| output_block(printed, *failed, "output", cx));
                        div()
                            .id(("session-tool-output", id as usize))
                            .w_full()
                            .child(trace_collapsible(
                                line,
                                open,
                                toggle(|this| &mut this.expanded_extras),
                                content,
                                window,
                                cx,
                            ))
                            .into_any_element()
                    }
                    None => trace_line(line, window, cx),
                }
            }
        };
        // EXP-1324: a settled test run with failures IS its failed-tests
        // line (trace.json), opening onto the same output — it replaces the
        // call's own "Ran …" line, as on web, iOS and Android.
        let tests = (kind == "execute" && *settled)
            .then_some(printed)
            .flatten()
            .and_then(|printed| {
                // Parsed once at settle ([`Self::refresh_test_results`]).
                let results = self.test_results.get(&id).and_then(|(_, results)| results.as_ref());
                let model = steer::trace::trace_failed_tests(results)?;
                let open = self.expanded_cards.contains(&id);
                let content = open.then(|| output_block(printed, true, "tests", cx));
                Some(trace_collapsible(
                    TraceLine::from_model(("steer-tool-tests", id as usize), &model),
                    open,
                    toggle(|this| &mut this.expanded_cards),
                    content,
                    window,
                    cx,
                ))
            });
        let row = tests.unwrap_or(row);
        if mode == ToolRowMode::Nested {
            return row;
        }
        // EXP-916: a patch never hangs off a tool row — it belongs to the
        // edited-files card its run grouped into. An edit the engine tagged
        // with a workflow id whose frame never arrived is grouped into
        // NOTHING, though, so it gets a card of its own rather than losing
        // its patch.
        let card = diff
            .as_deref()
            .filter(|diff| !diff.is_empty())
            .map(|_| self.render_edits_card(id, &[item], cx));
        if card.is_none() {
            return row;
        }
        trace_stack(
            std::iter::once(row).chain(card).collect(),
            TraceSize::Sm,
        )
        .into_any_element()
    }

    /// EXP-1279 — the Script card of a `codemode__exec` call. The call log
    /// and the script source ride the settled tool item once the engine
    /// publishes them ([`script_card_input`]); until then the card is its
    /// title, the spinner while it runs.
    fn render_script_card(&self, item: &FeedItem, cx: &mut gpui::Context<Self>) -> AnyElement {
        let (input, failed, source) = script_card_input(item);
        let card = steer::script_card::script_card(&input);
        let id = item.id;
        crate::script_card::script_card(
            crate::script_card::ScriptCardProps {
                id: SharedString::from(format!("steer-script-{id}")),
                card: &card,
                running: input.running,
                failed,
                source,
                source_open: self.expanded_cards.contains(&id),
                on_toggle_source: Some({
                    let entity = cx.entity().downgrade();
                    std::rc::Rc::new(move |_window: &mut Window, cx: &mut App| {
                        let _ = entity.update(cx, |this, cx| {
                            if !this.expanded_cards.insert(id) {
                                this.expanded_cards.remove(&id);
                            }
                            cx.notify();
                        });
                    })
                }),
            },
            cx,
        )
    }

    /// EXP-1241 — a SETTLED `exponential_comments_create` call renders the
    /// real comment: "Commented on <issue chip>" over the comment in the
    /// timeline's style (the agent's mark, the author, `via MCP`, the time;
    /// the body clamped to three lines). The comment id is the answer's
    /// `comment@` ref; the body is the synced row's (else the ref's excerpt).
    /// A click opens the issue scrolled to the comment. `None` for any other
    /// call — and for a comment call still running or failed, which keeps
    /// its ordinary row.
    fn render_comment_call(&self, item: &FeedItem, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let FeedKind::Tool { name, settled, failed, preview, settled_at, .. } = &item.kind else {
            return None;
        };
        if !*settled || *failed || steer::exp_tool_row(name) != Some(COMMENTS_CREATE_ROW) {
            return None;
        }
        let preview = preview.as_ref()?;
        let comment_ref = preview.refs.iter().find(|r| r.kind == "comment")?;
        let issue_ref = preview.refs.iter().find(|r| r.kind == "issue");
        let muted = cx.theme().muted_foreground;
        let collections = sync::Store::try_global(cx).map(|store| store.collections().clone());
        let comment = collections
            .as_ref()
            .and_then(|collections| collections.comments.read(cx).get(&comment_ref.id).cloned());
        let author = match comment.as_ref() {
            Some(comment) => {
                let user = comment
                    .author_id
                    .as_deref()
                    .and_then(|author_id| collections.as_ref()?.users.read(cx).get(author_id).cloned());
                // #EXP-1383: an agent's session wrote it → the agent's name.
                let agent = comment
                    .author_agent_id
                    .as_deref()
                    .and_then(|agent_id| crate::queries::agent_actor(cx, agent_id))
                    .map(|agent| agent.name);
                Some(agent.unwrap_or_else(|| crate::comments::thread_comment_author(comment, user.as_ref())))
            }
            // Not synced here yet: the run's owner, whose key posted it.
            None => self.row.as_ref().and_then(|row| {
                let owner = row.user_id.as_deref()?;
                let user = collections.as_ref()?.users.read(cx).get(owner).cloned()?;
                user.name.clone().filter(|name| !name.trim().is_empty())
            }),
        };
        let now = chrono::Utc::now().timestamp();
        let meta = match comment.as_ref() {
            Some(comment) => crate::comments::thread_comment_meta(comment, now),
            // The settle's own time until the row syncs (the synced meta's
            // "<time> · via MCP" order). #EXP-1383: an agent's own comment
            // never reads via MCP.
            None => {
                let agent_tree = self
                    .row
                    .as_ref()
                    .is_some_and(|row| row.agent_id.is_some() || row.started_by_agent_id.is_some());
                let ago = settled_at.map(|ms| crate::comments::relative_time_epoch(ms.div_euclid(1000), now));
                match (ago, agent_tree) {
                    (Some(ago), false) => format!("{ago} · via MCP"),
                    (Some(ago), true) => ago,
                    (None, false) => "via MCP".to_string(),
                    (None, true) => String::new(),
                }
            }
        };
        let body = comment
            .as_ref()
            .and_then(|comment| comment.body.clone())
            .or_else(|| comment_ref.title.clone())
            .unwrap_or_default();
        // The synced row's issue, else the answer's own issue ref.
        let issue_id = comment
            .as_ref()
            .map(|comment| comment.issue_id.clone())
            .or_else(|| issue_ref.map(|issue_ref| issue_ref.id.clone()).filter(|id| !id.is_empty()));
        let id = item.id;
        let mark = match self.builtin_agent() {
            Some(agent) => crate::coding_selects::agent_mark(agent).xsmall().into_any_element(),
            None => Icon::from(crate::icons::ExpIcon::Logo).xsmall().text_color(muted).into_any_element(),
        };
        let entity = cx.entity().downgrade();
        let on_toggle: std::rc::Rc<dyn Fn(&mut Window, &mut App)> = std::rc::Rc::new(move |_window, cx| {
            let _ = entity.update(cx, |this, cx| {
                if !this.expanded_bodies.insert(id) {
                    this.expanded_bodies.remove(&id);
                }
                cx.notify();
            });
        });
        let on_open = issue_id.map(|issue_id| {
            let comment_id = comment_ref.id.clone();
            std::rc::Rc::new(move |window: &mut Window, cx: &mut App| {
                crate::comments::open_issue_at_comment(&issue_id, &comment_id, window, cx)
            }) as std::rc::Rc<dyn Fn(&mut Window, &mut App)>
        });
        let header = tool_text(h_flex())
            .w_full()
            .min_w_0()
            .gap_2()
            .items_center()
            .child(Icon::from(crate::icons::ExpIcon::Logo).xsmall().text_color(muted))
            .child(div().flex_shrink_0().child(SharedString::from(COMMENTED_ON_LABEL)))
            .children(
                issue_ref.map(|issue_ref| exp_entity_chip_row(id, std::slice::from_ref(issue_ref), cx)),
            );
        let card = crate::comments::thread_comment_card(
            crate::comments::ThreadCommentProps {
                id: SharedString::from(format!("steer-comment-{id}")),
                mark,
                author,
                meta,
                body,
                team_id: self.ref_team_id(cx),
                bare_refs: false,
                expanded: self.expanded_bodies.contains(&id),
                on_toggle: Some(on_toggle),
                on_open,
                images: Some(self.images.clone()),
            },
            cx,
        );
        Some(
            v_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .p_3()
                .rounded(px(theme::tokens::radius::LG))
                .border_1()
                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                .bg(crate::dock::card_fill())
                .child(header)
                .child(card)
                .into_any_element(),
        )
    }

    /// EXP-916 — THE edited-files card for a [`steer::feed::FeedRow::Edits`].
    ///
    /// The members' patches are the wire's own (`tool_update`), falling back
    /// while a call is still RUNNING to what the local engine has written so
    /// far ([`crate::session_extras::LocalExtras::edit_patch`]) — so the
    /// runner sees a live edit's diff instead of a `pending` row, and a
    /// remote viewer sees exactly what the publisher sent. The view itself is
    /// the contract's ([`domain::edit_card::edit_card`]), memoised per card
    /// (EXP-884: a long run must not re-parse every patch every frame).
    fn render_edits_card(
        &self,
        id: FeedItemId,
        items: &[&FeedItem],
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let members: Vec<domain::edit_card::EditCardMember<'_>> = items
            .iter()
            .map(|item| {
                let (detail, diff, settled, failed) = match &item.kind {
                    FeedKind::Tool { detail, diff, settled, failed, .. } => {
                        (detail.as_deref(), diff.as_deref(), *settled, *failed)
                    }
                    _ => (None, None, true, false),
                };
                domain::edit_card::EditCardMember {
                    id: item.id,
                    detail,
                    diff: diff.or_else(|| self.extras.edit_patch(item.id)),
                    settled,
                    failed,
                }
            })
            .collect();
        // The ONE live row ×4: the transcript's running tool row, and only
        // when it is this card's last member.
        let live = steer::feed::live_tool_row_id(self.feed.items())
            .filter(|_| self.phase == ViewerPhase::Live);
        let view = self.edit_memo.card(id, &members, live);
        // The reader's OWN set, or none yet — then the card follows the live
        // row, and the first click copies THAT (so the live row folds too).
        let open = self.expanded_edit_rows.get(&id);
        let live_path = view
            .live_index
            .and_then(|at| view.rows.get(at))
            .map(|row| row.path.clone());
        crate::session_extras::render_edit_card(
            id,
            &view,
            open,
            self.expanded_cards.contains(&id),
            std::rc::Rc::new(cx.listener(move |this, path: &str, _window, cx| {
                let rows = this
                    .expanded_edit_rows
                    .entry(id)
                    .or_insert_with(|| live_path.iter().cloned().collect());
                if !rows.insert(path.to_string()) {
                    rows.remove(path);
                }
                cx.notify();
            })),
            Box::new(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                if !this.expanded_cards.insert(id) {
                    this.expanded_cards.remove(&id);
                }
                cx.notify();
            })),
            cx,
        )
    }

    fn render_body_folding(
        &self,
        id: FeedItemId,
        text: &str,
        fold: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        // EXP-698: every body this renders is a CHAT body — the user bubble,
        // the plan card, the ask card, a stepper step — so the rhythm and the
        // code tint are set once, here.
        let view = self.with_issue_chips(
            crate::markdown::MarkdownView::new(
                SharedString::from(format!("steer-body-{id}")),
                text.to_string(),
            )
            .chat(true)
            .autolink(true)
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
                // EXP-963: the shared in-place text toggle, not a muted line
                // of this view's own.
                text_button(
                    ("steer-body-toggle", id as usize),
                    if expanded { "Show less" } else { "Show more" },
                    TextButtonVariant::Text,
                    cx,
                )
                .mt_1()
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
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        // EXP-1324: the group is a TraceCollapsible captioned by the ONE
        // summary every client derives ("Ran 3 commands · edited 2 files ·
        // 1 failed", EXP-785), its calls the Trace lines inside.
        let line = TraceLine::new(
            ("steer-tool-run", id as usize),
            Icon::new(registry::TRACE_TOOL),
            tool_group_caption(items),
        );
        self.render_group(id, line, items, live_tail, false, window, cx)
    }

    /// EXP-1324 — a tool group as ONE Trace collapsible: closed it is the
    /// caption alone (a run still going keeps its newest call visible under
    /// it, EXP-895 — the live tail never streams output); open, every call's
    /// line sits on the rail. `pictures` = a folded Exponential run still
    /// shows its calls' inline tiles (EXP-1172).
    #[allow(clippy::too_many_arguments)]
    fn render_group(
        &self,
        id: FeedItemId,
        line: TraceLine,
        items: &[&FeedItem],
        live_tail: bool,
        pictures: bool,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let expanded = self.expanded_groups.contains(&id);
        let content = expanded.then(|| {
            trace_stack(
                items
                    .iter()
                    .filter(|item| item.is_tool())
                    .map(|item| self.render_tool_item(item, self.tool_row_mode(item), window, cx))
                    .collect(),
                TraceSize::Sm,
            )
            .into_any_element()
        });
        let entity = cx.entity().downgrade();
        let header = trace_collapsible(
            line,
            expanded,
            move |_, cx| {
                let _ = entity.update(cx, |this, cx| {
                    if !this.expanded_groups.insert(id) {
                        this.expanded_groups.remove(&id);
                    }
                    cx.notify();
                });
            },
            content,
            window,
            cx,
        );
        if expanded {
            return header;
        }
        let tail = live_tail
            .then(|| items.last().filter(|item| item.is_tool()).copied())
            .flatten();
        let mut folded: Vec<AnyElement> = Vec::new();
        if pictures {
            // EXP-1172: a picture never hides in a fold — a FOLDED run of
            // `sessions_show` calls draws its calls' inline tiles in call
            // order under the caption. The live tail's own row draws its.
            for item in items {
                if tail.is_some_and(|tail| tail.id == item.id) {
                    continue;
                }
                if let Some(tile) = self.inline_picture_for(item, cx) {
                    folded.push(tile);
                }
            }
        }
        if let Some(item) = tail {
            folded.push(self.render_tool_item(item, ToolRowMode::Live, window, cx));
        }
        if folded.is_empty() {
            return header;
        }
        v_flex()
            .w_full()
            .min_w_0()
            .child(header)
            .child(trace_content(
                trace_stack(folded, TraceSize::Sm).into_any_element(),
                TraceSize::Sm,
                cx,
            ))
            .into_any_element()
    }

    /// EXP-948 — a run of ≥2 consecutive calls to the SAME Exponential MCP
    /// tool. Our own work never hides inside a generic "N other tools" fold:
    /// the run wears the brand mark every single call of ours carries and the
    /// contract's own plural caption, and it opens to the individual rows
    /// ("Read issue EXP-901") like any tool group.
    fn render_exp_tool_run(
        &self,
        id: FeedItemId,
        items: &[&FeedItem],
        live_tail: bool,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        // The app's own mark, exactly as a single call of ours draws it, and
        // the contract's own plural caption.
        let line = TraceLine::new(
            ("steer-exp-tool-run", id as usize),
            Icon::from(crate::icons::ExpIcon::Logo),
            exp_tool_group_caption(items),
        );
        self.render_group(id, line, items, live_tail, true, window, cx)
    }

    /// EXP-1324 — a subagent is its Trace line: "Running the Explore agent"
    /// (shimmering) → "Ran the Explore agent" with its duration as the
    /// outline Badge. A click opens its tab (its own transcript), as the
    /// Work card did (EXP-1240).
    fn render_subagent(
        &self,
        id: FeedItemId,
        items: &[&FeedItem],
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let summary = summarize_subagent_row(items);
        let running = matches!(
            items.iter().find_map(|item| match &item.kind {
                FeedKind::Subagent { status, .. } => Some(*status),
                _ => None,
            }),
            Some(SubagentStatus::Started)
        ) && !summary.done;
        // EXP-1259 #11: elapsed from the `started` marker to the settling
        // one (to now while it runs).
        let marker_at = |wanted: SubagentStatus| {
            items.iter().find_map(|item| match &item.kind {
                FeedKind::Subagent { status, at, .. } if *status == wanted => *at,
                _ => None,
            })
        };
        let elapsed_ms = work_elapsed_ms(
            marker_at(SubagentStatus::Started),
            marker_at(SubagentStatus::Completed),
            running,
            now_epoch_ms(),
        );
        let label = if summary.agent_type.trim().is_empty() {
            summary.title.clone().unwrap_or_default()
        } else {
            summary.agent_type.clone()
        };
        let model = steer::trace::trace_subagent(
            &label,
            running,
            false,
            elapsed_ms.map(|ms| ms as i64),
        );
        let line = TraceLine::from_model(("steer-subagent", id as usize), &model).badge(true);
        let element = match items.iter().find_map(|item| item.subagent_id().map(str::to_string)) {
            Some(subagent_id) => trace_link(
                line.chevron(true),
                cx.listener(move |this, _: &ClickEvent, window, cx| this.open_lane(subagent_id.clone(), window, cx)),
                window,
                cx,
            ),
            None => trace_line(line, window, cx),
        };
        // EXP-856 §4: a duplicate edge on an ORDINARY subagent stays under
        // its line.
        let warnings: Vec<AnyElement> = crate::session_rows::duplicate_warnings(
            &items.iter().map(|item| (*item).clone()).collect::<Vec<_>>(),
        )
        .iter()
        .map(|warning| self.render_duplicate_row(warning, cx))
        .collect();
        if warnings.is_empty() {
            return element;
        }
        trace_stack(std::iter::once(element).chain(warnings).collect(), TraceSize::Sm)
            .into_any_element()
    }

    // ── EXP-850: the working row, the workflow card, the strips ───────────

    /// §5 — the synthetic trailing row: the turn's verb (or the running
    /// workflow's caption) with its duration and token count, under the
    /// RUNNING AGENT's working mark — EXP-1191: the very spark the sidebar's
    /// Running rows and the tabs draw ([`crate::coding_selects::agent_working_mark`];
    /// codex and an external agent keep the pulse).
    ///
    /// gpui exposes no OS reduce-motion signal (see `theme::motion`), so the
    /// animation is unconditional here; the hook, if one is ever wanted, is
    /// the shared settings file, not a speculative flag.
    fn render_working_row(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        tool_text(h_flex())
            .w_full()
            .min_w_0()
            .gap_2()
            .items_center()
            .child(
                div()
                    .text_color(muted)
                    .child(crate::coding_selects::agent_working_mark(self.builtin_agent(), 12.)),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(muted)
                    .child(SharedString::from(self.working_caption())),
            )
            .into_any_element()
    }

    /// EXP-1240 — THE Work card of a workflow, rendered in place of its
    /// `Workflow` tool row (§3): the contract model over the latest-wins
    /// payload, the §4 duplicate warnings as its footer. A lane opens that
    /// agent's transcript.
    fn render_workflow_card(
        &self,
        workflow: &steer::WorkflowState,
        call: Option<&FeedItem>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        // EXP-1259 #11: elapsed from the `Workflow` call's start to its
        // settle (to now while it runs); without the call's stamps, the sum
        // of its phases' spans (the web's fallback).
        let (started_at, settled_at) = match call.map(|item| &item.kind) {
            Some(FeedKind::Tool { started_at, settled_at, .. }) => (*started_at, *settled_at),
            _ => (None, None),
        };
        let elapsed_ms = work_elapsed_ms(
            started_at,
            settled_at,
            workflow.status == steer::WorkflowStatus::Running,
            now_epoch_ms(),
        )
        .or_else(|| workflow_span_ms(workflow));
        let input = steer::work_card::WorkCardInput::from_workflow(workflow, elapsed_ms);
        let card = steer::work_card::work_card(&input);
        let behind = steer::work_card::work_card_lanes(&input);
        // The agent id behind every lane, row-aligned like the lanes.
        let agent_ids: Vec<Vec<Option<String>>> = behind
            .iter()
            .map(|lanes| {
                lanes
                    .iter()
                    .map(|(index, _)| {
                        workflow
                            .agents
                            .iter()
                            .find(|agent| agent.index == *index)
                            .and_then(|agent| agent.agent_id.clone())
                    })
                    .collect()
            })
            .collect();
        let lanes = behind
            .into_iter()
            .map(|lanes| lanes.into_iter().map(|(_, lane)| lane).collect())
            .collect();
        let entity = cx.entity().downgrade();
        let workflow_id = workflow.id.clone();
        let on_toggle_folded: crate::work_card::CardClick = {
            let entity = entity.clone();
            std::rc::Rc::new(move |_window, cx| {
                let workflow_id = workflow_id.clone();
                let _ = entity.update(cx, |this, cx| {
                    if !this.open_work_folds.remove(&workflow_id) {
                        this.open_work_folds.insert(workflow_id);
                    }
                    cx.notify();
                });
            })
        };
        let on_lane: Option<crate::work_card::LaneClick> = agent_ids
            .iter()
            .flatten()
            .any(Option::is_some)
            .then(|| {
                let entity = entity.clone();
                std::rc::Rc::new(move |(row, lane): (usize, usize), window: &mut Window, cx: &mut App| {
                    let Some(Some(agent_id)) = agent_ids.get(row).and_then(|lanes| lanes.get(lane))
                    else {
                        return;
                    };
                    let agent_id = agent_id.clone();
                    let _ = entity.update(cx, |this, cx| this.open_lane(agent_id, window, cx));
                }) as crate::work_card::LaneClick
            });
        let footer = self
            .workflow_duplicates(&workflow.id)
            .iter()
            .map(|warning| self.render_duplicate_row(warning, cx))
            .collect();
        crate::work_card::work_card(
            crate::work_card::WorkCardProps {
                id: SharedString::from(format!("steer-work-{}", workflow.id)),
                card: &card,
                lanes,
                folded_open: self.open_work_folds.contains(&workflow.id),
                on_toggle_folded: Some(on_toggle_folded),
                on_lane,
                on_card: None,
                footer,
            },
            cx,
        )
    }

    /// EXP-1240 — open a lane's (a subagent's, a workflow agent's)
    /// transcript in the existing nested view: its subagent tab.
    fn open_lane(&mut self, subagent_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.focus_subagent(Some(subagent_id), cx);
        window.refresh();
    }

    /// §4 — the amber duplicate warning: the wire's own sentence, verbatim.
    fn render_duplicate_row(
        &self,
        warning: &crate::session_rows::DuplicateWarning,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let amber = theme::tokens::YELLOW.to_hsla();
        let _ = cx;
        tool_text(h_flex())
            .w_full()
            .min_w_0()
            .gap_1p5()
            .items_start()
            .child(
                div().mt(px(2.)).flex_shrink_0().child(
                    Icon::new(registry::UI_WARNING)
                        .xsmall()
                        .text_color(amber.opacity(0.9)),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_2xs()
                    .text_color(amber.opacity(0.9))
                    .child(SharedString::from(warning.detail.clone())),
            )
            .into_any_element()
    }

    /// EXP-1321/1324 — THE Queue (`fixtures/task-queue.json`): ONE block at
    /// the bottom of the transcript, in its reading column, right above the
    /// queued-messages strip. Two Trace collapsibles at size sm: "n to do"
    /// (open: the running work shimmering, then the pending tasks dimmed)
    /// and "n completed" (folded, its lines dimmed). It replaced the
    /// background-task + "Waiting on" lines and the Work card's `tasks`
    /// variant. A background item's elapsed time ticks once a second.
    /// EXP-1321: THE Queue's model over this view's relay state.
    fn task_queue_model(&self) -> Option<steer::task_queue::TaskQueue> {
        let input = crate::session_rows::task_queue_input(
            self.feed.task_list(),
            self.feed.background_tasks(),
            self.queue_waits.clone(),
            &self.background_seen,
            now_epoch_ms(),
        );
        steer::task_queue::task_queue(&input)
    }

    /// N-12: THE Queue's Trace, the `tasks` DockCard's sheet body.
    fn render_task_queue(&self, window: &Window, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        use steer::task_queue::{test_ids, TaskQueueItemState, TaskQueueSection};
        let queue = self.task_queue_model()?;
        let lines = |section: &TaskQueueSection, cx: &App| {
            trace_stack(
                section
                    .items
                    .iter()
                    .map(|item| {
                        let status = match item.state {
                            TaskQueueItemState::Running => Some(steer::trace::TraceStatus::Running),
                            TaskQueueItemState::Pending | TaskQueueItemState::Done => Some(steer::trace::TraceStatus::Pending),
                        };
                        trace_line(
                            TraceLine::new(
                                SharedString::from(format!("steer-queue-{}", item.key)),
                                Icon::from(crate::trace::trace_icon(&item.icon)),
                                item.label.clone(),
                            )
                            .description(item.description.clone())
                            .status(status),
                            window,
                            cx,
                        )
                    })
                    .collect(),
                TraceSize::Sm,
            )
            .into_any_element()
        };
        let todo_open = !self.queue_todo_closed;
        let todo = div().id(test_ids::TODO).w_full().child(trace_collapsible(
            TraceLine::new(
                "steer-queue-todo-line",
                Icon::from(crate::trace::trace_icon(&queue.todo.icon)),
                queue.todo.title.clone(),
            ),
            todo_open,
            {
                let entity = cx.entity().downgrade();
                move |_, cx| {
                    let _ = entity.update(cx, |this, cx| {
                        this.queue_todo_closed = !this.queue_todo_closed;
                        cx.notify();
                    });
                }
            },
            todo_open.then(|| lines(&queue.todo, cx)),
            window,
            cx,
        ));
        let completed = queue.completed.as_ref().map(|section| {
            let open = self.queue_completed_open;
            let entity = cx.entity().downgrade();
            div().id(test_ids::COMPLETED).w_full().child(trace_collapsible(
                TraceLine::new(
                    "steer-queue-completed-line",
                    Icon::from(crate::trace::trace_icon(&section.icon)),
                    section.title.clone(),
                ),
                open,
                move |_, cx| {
                    let _ = entity.update(cx, |this, cx| {
                        this.queue_completed_open = !this.queue_completed_open;
                        cx.notify();
                    });
                },
                open.then(|| lines(section, cx)),
                window,
                cx,
            ))
        });
        let block = v_flex()
            .id(test_ids::QUEUE)
            .w_full()
            .min_w_0()
            .gap(px(TraceSize::Sm.geometry().line_gap))
            .child(todo)
            .children(completed);
        Some(block.into_any_element())
    }

    /// EXP-1323 — a thread's LIVE TAIL (`session-thread.json`
    /// `liveSessionId`): the end of the Conversation, read off THIS view's
    /// relay state: the Working row while the agent works.
    pub(crate) fn live_tail(&self, _window: &Window, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        // N-12: THE Queue left the Conversation for the Dock; the tail keeps
        // only the Working row (web `SessionLiveTail`).
        if self.session_over() || !self.working_now() {
            return None;
        }
        Some(
            v_flex()
                .id(SharedString::from(domain::session_tree::THREAD.test_ids.live_tail.clone()))
                .w_full()
                .min_w_0()
                .child(self.render_working_row(cx))
                .into_any_element(),
        )
    }

    /// EXP-861: the messages the agent has not read yet — held behind an
    /// open compaction, or sent mid-turn and awaiting the agent's replay
    /// (EXP-873) — one line each, directly above the composer. A held line
    /// carries a × that revokes it and hands the text back to an empty
    /// draft; a sent one cannot be taken back from the agent (only Stop
    /// does), so it draws none. The same strip recipe as the background
    /// tasks; absent while nothing is unread.
    fn render_queue_strip(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let queued = self.feed.queue();
        if queued.is_empty() {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let mut column = v_flex()
            .id("steer-queue-strip")
            .w_full()
            .min_w_0()
            .gap_0p5()
            .tooltip(|window, cx| gpui_component::tooltip::Tooltip::new(QUEUE_STRIP_TITLE).build(window, cx));
        for message in queued.iter() {
            let id = message.id.clone();
            let text = message.text.clone();
            let line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
            let mut row = tool_text(h_flex())
                .w_full()
                .min_w_0()
                .gap_1p5()
                .items_center()
                .text_color(muted)
                .child(
                    Icon::new(registry::UI_QUEUED)
                        .xsmall()
                        .text_color(muted.opacity(0.7)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(SharedString::from(line)),
                );
            if !message.sent {
                // EXP-1317: keyed on the message, the native tooltip dies
                // with the X when the line is consumed.
                let button_id = id.clone();
                row = row.child(crate::thread_screen::unqueue_button(
                    "steer-unqueue",
                    &button_id,
                    true,
                    cx,
                    cx.listener(move |this, _, window, cx| {
                        this.unqueue(&id, &text, window, cx);
                    }),
                ));
            }
            column = column.child(row);
        }
        Some(column.into_any_element())
    }

    /// EXP-861: the × on a queued line. Local run → the engine drops it;
    /// remote → the `unqueue` frame. The line goes at once (the device's next
    /// `queue` frame is the truth), and an EMPTY draft takes the text back so
    /// it can be edited and re-sent — the CLI's "edit queued message".
    fn unqueue(&mut self, id: &str, text: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        // EXP-1294: a queued line that IS a `messages` row the device has not
        // consumed is revoked in that row (`sessions.unqueue`); the live
        // layer revokes only what no row carries.
        let row_queued = crate::session_data::messages_of_session(&self.session_id, cx)
            .iter()
            .any(|message| message.id == id && message.is_queued());
        if row_queued {
            let Some(trpc) = crate::queries::trpc_client(cx) else {
                crate::action_error::failed_action("remove", "the message", None, cx);
                return;
            };
            let message_id = id.to_string();
            cx.spawn(async move |_, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { api::sessions::unqueue(&trpc, &message_id) })
                    .await;
                if let Err(err) = result {
                    let _ = cx.update(|cx| {
                        crate::action_error::failed_action_err("remove", "the message", &err, cx);
                    });
                }
            })
            .detach();
        } else {
            match &self.source {
                FeedSource::Local { session } => session.unqueue(id),
                FeedSource::Replay { .. } | FeedSource::Journal { .. } => return,
                FeedSource::Remote { handle } => {
                    if !handle
                        .as_ref()
                        .is_some_and(|handle| handle.send_unqueue(id))
                    {
                        self.notice = Some(SharedString::from(NOT_CONNECTED));
                        cx.notify();
                        return;
                    }
                }
            }
        }
        self.feed.remove_queued(id);
        if self.input.read(cx).value().trim().is_empty() {
            let draft = text.to_string();
            self.input
                .update(cx, |state, cx| state.set_value(draft, window, cx));
        }
        cx.notify();
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
        let complete = ask_complete(items);
        // The re-opened step, if it is one of THIS ask's rows.
        let editing = self
            .editing_card_id()
            .filter(|id| items.iter().any(|item| item.id == *id));
        let answered: Vec<&&FeedItem> = items
            .iter()
            .filter(|item| item.question().is_some_and(|card| card.resolved) || self.is_answer_locked(item))
            .collect();
        let current =
            items.iter().find(|item| !item.question().is_some_and(|card| card.resolved) && !self.is_answer_locked(item));
        let numbered: Vec<&&FeedItem> =
            items.iter().filter(|item| item.question().and_then(|card| card.index).is_some()).collect();
        let total =
            numbered.first().and_then(|item| item.question().and_then(|card| card.total)).unwrap_or(numbered.len() as u32);
        let submit_step = current.map(|item| item.question().and_then(|card| card.index).is_none()).unwrap_or(false);
        let header = current
            .and_then(|item| item.question().and_then(|card| card.header.clone()))
            .or_else(|| items.first().and_then(|item| item.question().and_then(|card| card.header.clone())))
            .unwrap_or_else(|| if submit_step { "Review answers".to_string() } else { "Question".to_string() });
        let counter =
            ask_counter(current.filter(|_| !submit_step).and_then(|item| item.question().and_then(|card| card.index)), total);

        // EXP-698/1191: the stepper wears the SAME card as `render_question`
        // (web `AskCard`): the glyph beside one column of steps.
        let mut card = v_flex().w_full().min_w_0();
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
                        .mt_1()
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
                card = card.child(
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .mt_1p5()
                        .child(self.render_card_body(item.id, &text, false, window, cx))
                        .child(self.render_prompt(item, active, submit_step, false, window, cx)),
                );
            }
            // EXP-820: the spinner only while a next step is actually owed —
            // a finished ask is just its answered rows.
            None if !complete => {
                card = card.child(
                    h_flex()
                        .mt_2()
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
        ask_card(false, Some(SharedString::from(header)), counter, card, cx)
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
            .mt_1p5()
            .child(ask_body_text(cx).child(self.render_body(item.id, &text, cx)))
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
            .unwrap_or_else(|| domain::plan_card::copy().answered.clone());
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
            .child(div().max_w(px(200.)).truncate().child(SharedString::from(if dismissed {
                "Dismissed".to_string()
            } else {
                answer
            })))
            .when(editable, |this| this.child(Icon::new(registry::UI_EDIT).xsmall().text_color(muted)))
            .into_any_element()
    }

    /// A card's body: the folding chat body (a plan never folds: a
    /// read-only reader sees it whole, EXP-738).
    fn render_card_body(
        &self,
        id: FeedItemId,
        text: &str,
        plan: bool,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let _ = window;
        ask_body_text(cx).child(self.render_body_folding(id, text, !plan, cx)).into_any_element()
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
        let heading = if card.plan_mode {
            Some(SharedString::from(domain::ask_dock::ASK_DOCK.copy.plan_ready.clone()))
        } else {
            card.header.clone().map(SharedString::from)
        };
        // EXP-698/1191: the shared `AskCard` — neutral glass chrome, only the
        // glyph and the heading carry the accent (primary for a ready plan,
        // yellow for a question).
        let body = v_flex()
            .w_full()
            .min_w_0()
            .child(self.render_card_body(item.id, &card.text, card.plan_mode, window, cx))
            .child(self.render_prompt(item, active, false, false, window, cx));
        ask_card(card.plan_mode, heading, None, body, cx)
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
            return ask_resolved_line(card.answer.as_deref(), card.dismissed, cx);
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
                .child(SharedString::from(domain::plan_card::copy().answering.clone()))
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
                        .when(index < 9, |this| {
                            this.child(
                                div()
                                    .flex_shrink_0()
                                    .font_family(theme::terminal::FONT_FAMILY)
                                    .child(SharedString::from((index + 1).to_string())),
                            )
                        })
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
        let send = crate::composer::composer_submit_kind(
            ("steer-inline-send", inline.item as usize),
            crate::composer::SubmitKind::Send,
            !always_sendable && !has_text,
            cx,
        )
        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            cx.stop_propagation();
            this.send_inline(window, cx);
        }));
        ask_inline_field(&inline.state, send, window, cx)
            .key_context("SteerInlineAnswer")
            .capture_action(cx.listener(Self::on_inline_escape))
            .into_any_element()
    }

    /// One option row (EXP-788): a full-width button carrying its label,
    /// under it its description, and its number key chip at the right end
    /// (EXP-1191, web parity). EXP-820
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
        highlighted: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let label = if submit_label {
            "Submit answers".to_string()
        } else {
            option.label.clone()
        };
        ask_option_button(
            ("steer-option", item_id as usize * 64 + index),
            label,
            option.description.clone(),
            primary,
            picked,
            highlighted,
            index,
            cx,
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
        // EXP-884: the derived summaries, not a walk of the run per call.
        steer::feed::visible_subagent_tabs(&self.subagents, Some(focused))
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
        let tabs = steer::feed::visible_subagent_tabs(&self.subagents, active.as_deref());
        if tabs.is_empty() {
            return None;
        }
        let mut strip = h_flex()
            .w_full()
            .min_w_0()
            .flex_wrap()
            .gap_1()
            .items_center()
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
            .and_then(|id| self.subagents.iter().find(|agent| agent.subagent_id == id));
        // EXP-927: the tabs sit in the transcript's reading column, like
        // every strip block ([`work_column_row`]). EXP-1191: no hairline
        // under them (web parity).
        let column = v_flex()
            .w_full()
            .min_w_0()
            .child(strip)
            .when_some(summary, |this, summary| {
                this.child(
                    tool_text(h_flex())
                        .w_full()
                        .min_w_0()
                        .gap_2()
                        .items_center()
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
        Some(
            div()
                .w_full()
                .flex_shrink_0()
                .child(work_column_row(column.into_any_element()))
                .into_any_element(),
        )
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

    /// EXP-849 — "Switch account" over the rate-limit wall: the same account
    /// rows the session's usage readout shows, in a popover off a primary
    /// button. `None` when nothing could take the run (not claude, one login,
    /// a remote run whose machine this client cannot resolve) — a button that
    /// can only ever explain itself is worse than no button.
    fn switch_account_control(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        // NEW-1 (web `agent-session`): only a USAGE wall is lifted by
        // another login; a watchdog flag (device offline, not picked up,
        // stuck) offers no switch.
        if self
            .row
            .as_ref()
            .is_some_and(|row| row.blocked.is_some() && !domain::session_tree::is_usage_wall(row))
        {
            return None;
        }
        let agent = self.builtin_agent()?;
        let context = crate::account_switch::SwitchContext {
            session_id: self.session_id.to_string(),
            device_id: self.session_row().and_then(|row| row.device_id.clone()),
            local: self.is_local(),
            agent,
            // A walled run is stalled, not mid-turn (`working_now` vetoes on
            // the wall), but ask anyway: the one authority stays the one
            // authority.
            working: self.working_now(),
        };
        // Ask once whether there is anything to offer, then let the popover
        // re-render the rows itself: an `AnyElement` is consumed when it is
        // painted, so the content closure has to build its own.
        context.render(cx)?;
        Some(
            // Pushed to the row's right edge. The wrapper carries it: a
            // Popover's own style paints its CONTENT panel, not the trigger.
            div()
                .ml_auto()
                .flex_shrink_0()
                .child(
                    gpui_component::popover::Popover::new("steer-switch-account")
                        .p_2()
                        // The web `<Pill size="sm" mode="action" primary>`: the
                        // surface's PRIMARY pill recipe (a `Button`, so it stays a
                        // `Selectable` popover trigger).
                        .trigger(
                            crate::surface::glass_pill_button_primary(
                                "steer-switch-account-trigger",
                                crate::surface::PillSize::Sm,
                            )
                            .cursor_pointer()
                            .icon(Icon::new(registry::UI_SWAP).xsmall().text_color(cx.theme().primary_foreground))
                            .label(crate::account_switch::WALL_SWITCH_LABEL),
                        )
                        .content(move |_, _window, cx| context.render(cx).unwrap_or_else(|| gpui::Empty.into_any_element())),
                )
                .into_any_element(),
        )
    }

    /// N-12: the session view's status strips as THE Dock's banner cards
    /// (dock.json `banner`: the host's words and tone; web
    /// `useSessionDock`): a replay, a failed end, an unavailable relay, a
    /// dropped stream (Reconnect), the agent starting, the rate-limit wall (Switch account), compacting, a notice.
    fn dock_banners(&self, cx: &App) -> Vec<domain::dock::DockSource> {
        use domain::dock::{DockSource, DockTint};
        let banner = |id: &str, icon: &str, label: &str, text: String, tone: DockTint, action: Option<&str>| DockSource::Banner {
            id: id.to_string(),
            icon: icon.to_string(),
            label: label.to_string(),
            text,
            tone,
            action: action.map(str::to_string),
        };
        let mut out = Vec::new();
        let paused = self.paused(cx);
        if self.source.read_only() && !matches!(self.phase, ViewerPhase::Ended { .. }) {
            out.push(banner("replay", "ui-info", "Replay", REPLAY_BANNER.to_string(), DockTint::Muted, None));
        } else if self.feed.is_empty() && !self.is_local() && self.session_over() {
            out.push(banner("replay", "ui-info", "Replay", REPLAY_EMPTY_BANNER.to_string(), DockTint::Muted, None));
        }
        match &self.phase {
            ViewerPhase::Ended { outcome } => {
                if let Some(text) = ended_banner(outcome.as_deref()) {
                    out.push(banner("ended", "ui-error", "Failed", text, DockTint::Warning, None));
                }
            }
            ViewerPhase::Unauthorized { detail } => out.push(banner(
                "unavailable",
                "ui-info",
                "Unavailable",
                detail.clone().unwrap_or_else(|| STEER_UNAVAILABLE_DETAIL.to_string()),
                DockTint::Muted,
                None,
            )),
            ViewerPhase::Reconnecting if !paused => out.push(banner(
                "closed",
                "ui-refresh",
                "Connection lost",
                "Connection lost.".to_string(),
                DockTint::Muted,
                Some("Reconnect"),
            )),
            // #EXP-1405: an idle row's transcript waits for its next turn,
            // not for a start: only a busy run is "starting".
            ViewerPhase::Starting
                if !paused
                    && !self.feed.is_empty()
                    && !self.journal_seeded
                    && self.row.as_ref().is_some_and(|row| row.busy) =>
            {
                out.push(banner(
                    "starting",
                    "ui-loading",
                    "Starting",
                    "The agent is starting. Waiting for the live stream…".to_string(),
                    DockTint::Muted,
                    None,
                ))
            }
            _ => {}
        }
        // EXP-1374 (session-card.json `banner`): a background or queued
        // session's own card. S-3: no paused strip (an offline host is
        // invisible) and no interrupted card.
        if let Some(session) = self.row.as_ref().and_then(domain::session_tree::session_banner) {
            let id = match session.id {
                domain::session_tree::SessionBannerId::Background => "background",
                domain::session_tree::SessionBannerId::Queued => "queued",
            };
            out.push(banner(id, &session.icon, &session.label, session.text, DockTint::Muted, None));
        }
        if self.run_open() {
            if let Some(compacting) = self.feed.compacting() {
                let _ = compacting;
                out.push(banner("compacting", "coding-compact", "Compacting", COMPACTING_LABEL.to_string(), DockTint::Muted, None));
            }
            let now_ms = chrono::Utc::now().timestamp_millis();
            if let Some(limit) = self.feed.rate_limit().filter(|_| self.rate_limit_wall_showing(now_ms)) {
                let (message, countdown) = rate_limit_caption(
                    limit.message.as_deref(),
                    limit.resets_at.map(|at| crate::usage_bar::countdown_from_secs((at - now_ms) / 1000)),
                );
                out.push(banner(
                    "rate-limit",
                    "ui-usage",
                    "Rate limit",
                    format!("{message} · {}", rate_limit_detail(countdown)),
                    DockTint::Warning,
                    Some(crate::account_switch::WALL_SWITCH_LABEL),
                ));
            }
        }
        if let Some(notice) = self.notice.clone() {
            out.push(banner("notice", "ui-warning", "Notice", notice.to_string(), DockTint::Warning, None));
        }
        // NEW-53 (voice.json): this conversation's reply is being read aloud.
        if let Some(speaking) = etalk_gpui::speak_replies::speaking(cx).filter(|speaking| speaking.session_id == self.session_id) {
            let copy = &etalk::voice::VOICE.copy;
            out.push(banner("speaking", "media-volume-up", &copy.speaking, speaking.title, DockTint::Muted, Some(&copy.stop)));
        }
        // #EXP-1383 (chats.json `agentHint`): an agent chat whose root still
        // runs on instructions older than the agent's current ones.
        if self.agent_hint_agent(cx).is_some() {
            let hint = &domain::chats::CHATS.agent_hint.banner;
            out.push(banner(
                &hint.id,
                &hint.icon,
                &hint.label,
                hint.text.clone(),
                DockTint::Muted,
                Some(&hint.action),
            ));
        }
        out
    }

    /// #EXP-1383 (chats.json `agentHint`): the agent whose chat THIS root is,
    /// while the root fetched its brief before the agent's instructions last
    /// changed (`agent_instructions_changed`); `None` = no hint (a thread, a
    /// child, an agent never stamped, a fresh launch since the edit).
    fn agent_hint_agent(&self, cx: &App) -> Option<String> {
        let row = self.row.as_ref()?;
        if row.parent_id.is_some() || row.depth.is_some_and(|depth| depth != 0) {
            return None;
        }
        let agent_id = row.agent_id.as_deref()?;
        let agent = crate::session_data::agent_def(agent_id, cx)?;
        domain::chats::agent_instructions_changed(
            row.agent_instructions_at.as_deref(),
            agent.instructions_updated_at.as_deref(),
        )
        .then(|| agent_id.to_string())
    }

    /// EXP-1191 — the transcript's jump-to-bottom button
    /// ([`crate::controls::jump_to_bottom_button`]): shown while the reader
    /// is scrolled up off the tail of a non-empty feed, centred over the
    /// column, [`crate::controls::JUMP_TO_BOTTOM_GAP`] above the composer
    /// card (the band's `p_2` already gives 8 of it). A click scrolls to the
    /// newest row and re-arms tail follow.
    ///
    /// gpui re-arms the follow itself when a scroll lands back on the end,
    /// but in LAYOUT, after this frame rendered — so a paint-only probe
    /// compares the list's state with what was drawn and asks for ONE
    /// repaint when they differ (the button would otherwise linger).
    fn render_jump_to_bottom(
        &self,
        list: &ListState,
        has_rows: bool,
        composer_visible: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        // #EXP-1383 (chats.json FOLLOW + JUMP): the run view reads the same
        // `followSlack` as the chats.
        let following = crate::chat_transcript::near_tail(list);
        let shown = has_rows && !following;
        let target = list.clone();
        let list = list.clone();
        let entity_id = cx.entity_id();
        let probe = gpui::canvas(
            move |_, _, cx| {
                if crate::chat_transcript::near_tail(&list) != following {
                    cx.defer(move |cx| cx.notify(entity_id));
                }
            },
            |_, _: (), _, _| {},
        )
        .absolute()
        .size_0();
        let band = self.dock.read(cx).band_height();
        let button = shown.then(|| {
            // N-12: over THE Dock's band (its measured height).
            let _ = composer_visible;
            let gap = band + crate::controls::JUMP_TO_BOTTOM_GAP;
            h_flex()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(gap))
                .justify_center()
                .child(crate::controls::jump_to_bottom_button(
                    "steer-jump-to-bottom",
                    cx.listener(move |_, _: &ClickEvent, _window, cx| {
                        target.set_follow_mode(FollowMode::Tail);
                        target.scroll_to_end();
                        cx.notify();
                    }),
                    cx,
                ))
        });
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .child(probe)
            .children(button)
            .into_any_element()
    }

    fn render_composer(&self, window: &Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let can_send = self.can_send(cx);
        let stop = self.shows_stop(cx);
        // EXP-790: ONE round button — Stop while the agent works, Send
        // otherwise (and always once there is a draft). Same ring, the glyph
        // swaps. No tooltip — a floating label beside a circled glyph reads
        // as a second button (EXP-698).
        let kind = if stop {
            crate::composer::SubmitKind::Stop
        } else {
            crate::composer::SubmitKind::Send
        };
        let on_click = cx.listener(move |this, _: &ClickEvent, window, cx| {
            if stop {
                this.stop_turn(window, cx);
            } else {
                this.send(window, cx);
            }
        });
        let submit = crate::composer::SessionComposerSubmit::send(
            self.composer_ids.send.clone(),
            !stop && !can_send,
            on_click,
        )
        .kind(kind)
        .loading(self.sending);
        // The captures run before the field's own handlers, so with a `/`
        // menu open Enter/Tab accept and never reach the textarea (so
        // `PressEnter`, the thing that SENDS, never fires for them). EXP-820:
        // the card keyboard's captures sit on the view's ROOT, since the
        // composer is hidden while a card is pending.
        let (up, down, escape, enter, tab) = (
            cx.listener(Self::on_slash_up),
            cx.listener(Self::on_slash_down),
            cx.listener(Self::on_slash_escape),
            cx.listener(Self::on_slash_enter),
            cx.listener(Self::on_slash_tab),
        );
        let paste = cx.listener(Self::on_paste);
        // EXP-1294: THE session composer box — the same card the thread
        // reply and the agent channel paint. EXP-724: the `/` menu sits
        // INSIDE the card, above the field.
        let slash = self.slash.as_ref().map(|menu| {
            crate::composer::session_slash_menu(
                "steer-slash-menu",
                "steer-slash-row",
                &menu.items,
                menu.selected,
                &self.slash_scroll,
                |this: &mut Self, index, cx| {
                    if let Some(menu) = this.slash.as_mut() {
                        if menu.selected != index {
                            menu.selected = index;
                            cx.notify();
                        }
                    }
                },
                |this: &mut Self, index, window, cx| {
                    if let Some(menu) = this.slash.as_mut() {
                        menu.selected = index;
                    }
                    this.accept_slash(window, cx);
                },
                window,
                cx,
            )
        });
        let replies = self
            .replies
            .as_ref()
            .map(|replies| crate::composer::composer_replies_row(replies, true, cx));
        crate::composer::session_composer_card(
            crate::composer::SessionComposerBox::new(self.mention.clone(), submit)
                // EXP-1211: the quick replies sit DIRECTLY above the box.
                .replies(replies)
                .above_field(slash)
                .field_hook(move |field| {
                    field
                        .key_context("SteerComposer")
                        .capture_action(up)
                        .capture_action(down)
                        .capture_action(escape)
                        .capture_action(enter)
                        .capture_action(tab)
                })
                .strip((!self.pending_images.is_empty()).then(|| self.render_pending_strip(cx)))
                // N-12: the "+" options menu is the only tool (no inline
                // image / file / # / emoji tools).
                .tool(self.render_plus_menu(cx))
                .card_hook(move |card| card.capture_action(paste))
                .controls(self.render_composer_controls(cx)),
            cx,
        )
    }

    /// N-12: the session's own DockCards (web `useSessionDock`): the queued
    /// messages, THE Queue and the status banners. The session view adds
    /// its waiting cards; a root's host (thread, agent channel) adds the
    /// tree's.
    pub(crate) fn session_dock_sources(&self, cx: &App) -> Vec<domain::dock::DockSource> {
        use domain::dock::{DockQueue, DockQueueCompleted, DockQueueItem, DockQueueTodo, DockQueuedMessage, DockSource};
        let queued = if self.run_open() {
            self.feed
                .queue()
                .iter()
                .map(|message| DockQueuedMessage { id: message.id.clone(), text: message.text.split_whitespace().collect::<Vec<_>>().join(" ") })
                .collect()
        } else {
            Vec::new()
        };
        let queue = self.task_queue_model().map(|queue| DockQueue {
            todo: DockQueueTodo {
                title: queue.todo.title.clone(),
                items: queue
                    .todo
                    .items
                    .iter()
                    .map(|item| DockQueueItem {
                        label: item.label.clone(),
                        description: item.description.clone(),
                        state: format!("{:?}", item.state).to_lowercase(),
                    })
                    .collect(),
            },
            completed: queue.completed.as_ref().map(|section| DockQueueCompleted { title: section.title.clone() }),
        });
        let mut out = vec![DockSource::Queued { messages: queued }, DockSource::Tasks { queue }];
        out.extend(self.dock_banners(cx));
        out
    }

    /// N-12: the sheet of one of [`Self::session_dock_sources`]' cards: THE
    /// Queue's Trace, the queued list with its remove glyphs.
    pub(crate) fn render_session_dock_sheet(
        &mut self,
        row: &domain::dock::DockRow,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<crate::dock::SheetContent> {
        let body = match row.kind {
            domain::dock::DockKind::Tasks => self.render_task_queue(window, cx)?,
            domain::dock::DockKind::Queued => self.render_queue_strip(cx)?,
            _ => return None,
        };
        Some(crate::dock::SheetContent {
            meta: None,
            action: None,
            body: crate::dock::sheet_body(SharedString::from(format!("steer-dock-sheet-{}", row.id)), body),
            footer: None,
        })
    }

    /// N-12: the banners' action buttons (Reconnect, Switch account).
    pub(crate) fn session_dock_actions(&self, cx: &mut gpui::Context<Self>) -> Vec<(String, AnyElement)> {
        let mut out = Vec::new();
        if matches!(self.phase, ViewerPhase::Reconnecting) {
            out.push((
                "banner:closed".to_string(),
                Button::new("steer-dock-reconnect")
                    .outline()
                    .small()
                    .cursor_pointer()
                    .icon(Icon::new(registry::UI_REFRESH).xsmall())
                    .label("Reconnect")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.wake("reconnect");
                        cx.notify();
                    }))
                    .into_any_element(),
            ));
        }
        if let Some(switch) = self.switch_account_control(cx) {
            out.push(("banner:rate-limit".to_string(), switch));
        }
        // NEW-53: the Speaking banner's Stop ends the voice.
        if etalk_gpui::speak_replies::speaking(cx).is_some_and(|speaking| speaking.session_id == self.session_id) {
            out.push((
                "banner:speaking".to_string(),
                Button::new("steer-dock-stop-speaking")
                    .outline()
                    .small()
                    .cursor_pointer()
                    .icon(Icon::new(registry::UI_STOP).xsmall())
                    .label(etalk::voice::VOICE.copy.stop.clone())
                    .on_click(|_: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        etalk_gpui::speak_replies::stop(cx);
                    })
                    .into_any_element(),
            ));
        }
        // #EXP-1383: the hint's action word sends exactly what the agent
        // menu's New conversation sends (`sessions.send({agentId, text:
        // "/new"})`), a MESSAGE the hosting device's pickup turns into the
        // engine restart; the row stays until the fresh launch re-fetches
        // the brief (the shape update hides it).
        if let Some(agent_id) = self.agent_hint_agent(cx) {
            let hint = &domain::chats::CHATS.agent_hint;
            let line = hint.sends.clone();
            out.push((
                format!("banner:{}", hint.banner.id),
                Button::new("dock-instructions-new")
                    .outline()
                    .small()
                    .cursor_pointer()
                    .label(hint.banner.action.clone())
                    .on_click(move |_: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        crate::chat_actions::send_to_agent(&agent_id, &line, cx);
                    })
                    .into_any_element(),
            ));
        }
        out
    }

    /// N-12: THE Dock of the session view: the session's own cards, then
    /// THE composer; and the open card's sheet. S-3 (dock.json): a waiting
    /// plan / question (the relay's own ask or a tool-posted card, the
    /// latter a story) is never a DockCard; it stays inline, answerable.
    fn render_session_dock(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> (Option<AnyElement>, Option<AnyElement>) {
        let sources = self.session_dock_sources(cx);
        let rows = domain::dock::dock_rows(&sources);
        let dock = self.dock.clone();
        // The relay replays its history after the first frame: cards in it
        // were already waiting.
        let loaded = matches!(self.phase, ViewerPhase::Live | ViewerPhase::Ended { .. }) || self.source.read_only();
        dock.update(cx, |state, _| state.auto_open(&rows, true, loaded));
        let sheet = self.render_dock_sheet(&rows, window, cx);
        let actions = self.session_dock_actions(cx);
        let composer = self.composer_visible().then(|| self.render_composer_card(window, cx)).flatten();
        let band = crate::dock::band(
            &dock,
            crate::dock::Band { id: "steer".into(), rows, actions, composer },
            window,
            cx,
        );
        (band, sheet)
    }

    /// The open card's sheet (the Queue, the queued list).
    fn render_dock_sheet(&mut self, rows: &[domain::dock::DockRow], window: &mut Window, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let open = self.dock.read(cx).sheet_id()?.to_string();
        let Some(row) = rows.iter().find(|row| row.id == open).cloned() else {
            self.dock.update(cx, |state, _| state.close());
            return None;
        };
        // THE sheet is a sheet host: its pickers open inline.
        let content = crate::dock::sheet_host(|| self.render_session_dock_sheet(&row, window, cx))?;
        Some(crate::dock::sheet(&self.dock.clone(), &row, content, window, cx))
    }

    /// THE composer card (the notice above it) for THE Dock's band.
    pub(crate) fn render_composer_card(&mut self, window: &Window, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        self.ensure_mcp_loaded(cx);
        Some(self.render_composer(window, cx))
    }

    /// THE composer card for a screen hosting this view as a ROOT session's
    /// composer (the thread screen, the agent channel), for its Dock's
    /// band: the SAME composer the session view paints, targeting this
    /// session. `None` for a reader who may not steer.
    pub(crate) fn render_session_composer(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        self.refresh_derived();
        if !self.composer_visible() {
            return None;
        }
        self.render_composer_card(window, cx)
    }

    /// Host this view as a ROOT session's composer (the thread screen, the
    /// agent channel): its own placeholder and element ids, and Send⇄Stop
    /// following the whole TREE — Stop shows while any session of it works
    /// and interrupts every working one.
    pub(crate) fn host_root_composer(
        &mut self,
        placeholder: impl Into<SharedString>,
        ids: ComposerIds,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.tree_scope = true;
        self.composer_ids = ids;
        self.set_placeholder(placeholder, window, cx);
    }

    /// The composer field's placeholder.
    pub(crate) fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let placeholder = placeholder.into();
        self.input
            .update(cx, |input, cx| input.set_placeholder(placeholder, window, cx));
    }

    /// EXP-877 — the run's controls in the composer card's bottom row
    /// ([`crate::composer::SessionComposerControls`]): the device pin, the
    /// model pin, the context ring (S-3: no Plan switch).
    fn render_composer_controls(&self, cx: &mut gpui::Context<Self>) -> crate::composer::SessionComposerControls {
        // S-3: NO options line (the model is a "+" row; a running session
        // never moves device here): the context ring stays.
        crate::composer::SessionComposerControls {
            device_pin: None,
            model_pin: None,
            context_ring: self.render_context_ring(cx),
        }
    }

    /// EXP-1334: whether this viewer may change the run's options mid-session
    /// (`sessions.configure`): a live run it may message.
    fn configurable(&self) -> bool {
        self.may_message && self.run_open() && self.row.is_some()
    }

    /// EXP-1334: load the "+" menu's MCP row once per view — the team's
    /// servers (`mcpServers.list`) and the session's pick
    /// (`sessions.mcpPicks`), only while the run may be configured. A load
    /// that fails re-arms after [`MCP_RETRY_AFTER`], [`MCP_LOAD_ATTEMPTS`]
    /// times in all, so one bad request does not hide the row for the life
    /// of the view.
    fn ensure_mcp_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        if self.mcp_requested || self.mcp_attempts >= MCP_LOAD_ATTEMPTS || !self.configurable() {
            return;
        }
        let Some(team_id) = self.row.as_ref().and_then(|row| row.team_id.clone()) else {
            return;
        };
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };
        self.mcp_requested = true;
        let session_id = self.session_id.to_string();
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    let servers = api::mcp_servers::list(&trpc, &team_id)
                        .inspect_err(|err| log::debug!("[steer] mcpServers.list: {err}"))
                        .ok()?;
                    let picks = api::session_device::mcp_picks(&trpc, &session_id)
                        .inspect_err(|err| log::debug!("[steer] sessions.mcpPicks: {err}"))
                        .ok()?;
                    Some(SessionMcp {
                        servers: servers
                            .into_iter()
                            .map(|entry| {
                                let blocked = crate::launch_options::mcp_block_reason(&entry.connection).is_some();
                                (entry.config.id, entry.config.name, blocked)
                            })
                            .collect(),
                        picks,
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| match loaded {
                Some(loaded) => {
                    this.mcp = Some(loaded);
                    cx.notify();
                }
                None => {
                    this.mcp_attempts = this.mcp_attempts.saturating_add(1);
                    if this.mcp_attempts >= MCP_LOAD_ATTEMPTS {
                        return;
                    }
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(MCP_RETRY_AFTER).await;
                        let _ = this.update(cx, |this, cx| {
                            this.mcp_requested = false;
                            cx.notify();
                        });
                    })
                    .detach();
                }
            });
        })
        .detach();
    }

    /// EXP-1334 — `sessions.configure` from outside the view (a menu row):
    /// see [`Self::configure_now`]. A view that is gone drops the pick.
    fn configure(view: gpui::WeakEntity<Self>, input: api::sessions::ConfigureInput, cx: &mut App) {
        let _ = view.update(cx, |this, cx| this.configure_now(input, cx));
    }

    /// EXP-1334 — `sessions.configure`: a running session's options, changed
    /// mid-session (the host device applies them; the synced row confirms).
    /// The pick shows AT ONCE — [`OptionPicks`] ahead of the row, the MCP
    /// pick on the row itself — and the request joins the queue: one in
    /// flight at a time, in pick order, so two quick picks land in the order
    /// they were made. A refusal puts the pick back and toasts.
    fn configure_now(&mut self, input: api::sessions::ConfigureInput, cx: &mut gpui::Context<Self>) {
        if crate::queries::trpc_client(cx).is_none() {
            crate::action_error::failed_action("change", "the session", None, cx);
            return;
        }
        let mut picks = OptionPicks::default();
        picks.apply(&input);
        self.option_picks.apply(&input);
        let previous_mcp = match (input.mcp_server_ids.as_ref(), self.mcp.as_mut()) {
            (Some(next), Some(mcp)) => Some(std::mem::replace(&mut mcp.picks, next.clone())),
            _ => None,
        };
        self.configure_queue.push_back(QueuedConfigure {
            input,
            picks,
            previous_mcp,
        });
        cx.notify();
        self.drain_configures(cx);
    }

    /// The ONE worker behind [`Self::configure_now`]: pops the queue until
    /// it is empty, one request at a time.
    fn drain_configures(&mut self, cx: &mut gpui::Context<Self>) {
        if self.configure_busy {
            return;
        }
        self.configure_busy = true;
        cx.spawn(async move |this, cx| {
            loop {
                let Some(next) = this.update(cx, |this, _| this.configure_queue.pop_front()).ok().flatten() else {
                    break;
                };
                let QueuedConfigure {
                    input,
                    picks,
                    previous_mcp,
                } = next;
                let trpc = cx.update(|cx| crate::queries::trpc_client(cx));
                let outcome: Result<(), Option<api::ApiError>> = match trpc {
                    Some(trpc) => cx
                        .background_executor()
                        .spawn(async move { api::sessions::configure(&trpc, &input).map(|_| ()) })
                        .await
                        .map_err(Some),
                    // Signed out between the pick and its turn.
                    None => Err(None),
                };
                if let Err(err) = outcome {
                    if let Some(err) = err.as_ref() {
                        log::warn!("[steer] configure the session failed: {err}");
                    }
                    let _ = this.update(cx, |this, cx| {
                        this.option_picks.drop_settled(&picks);
                        if let (Some(previous), Some(mcp)) = (previous_mcp, this.mcp.as_mut()) {
                            mcp.picks = previous;
                        }
                        cx.notify();
                    });
                    let _ = cx.update(|cx| match err.as_ref() {
                        Some(err) => crate::action_error::failed_action_err("change", "the session", err, cx),
                        None => crate::action_error::failed_action("change", "the session", None, cx),
                    });
                }
            }
            let _ = this.update(cx, |this, _| this.configure_busy = false);
        })
        .detach();
    }

    /// EXP-1334 / S-3 — the composer's "+": THE composer menu
    /// (composer-menu.json) on the `session` surface (`agent` for an agent
    /// conversation): Photo or file, Issue › (THE issue search list; a pick
    /// inserts `#IDENT` at the caret), then Model › while the run may switch
    /// models (claude and codex: `sessions.configure({model})`). A running
    /// session never moves device or login here; the work rows belong to a
    /// start.
    fn render_plus_menu(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        use crate::chat_screen::{composer_menu_rows, titled_row, ComposerMenuConditions, ComposerMenuRow as Row, ComposerSurface};
        let view = cx.entity().downgrade();
        let session_id = self.session_id.to_string();
        let surface = if self.row.as_ref().and_then(|row| row.agent_id.as_deref()).is_some_and(|id| !id.is_empty()) {
            ComposerSurface::Agent
        } else {
            ComposerSurface::Session
        };
        let team_id = self.row.as_ref().and_then(|row| row.team_id.clone());
        // #EXP-1434: the team's prompts, the `/` menu's list.
        let prompts: Vec<slash_commands::SlashPrompt> = team_id
            .as_deref()
            .filter(|_| sync::Store::try_global(cx).is_some())
            .map(|team_id| {
                crate::queries::team_prompts(cx, team_id)
                    .0
                    .iter()
                    .map(slash_commands::SlashPrompt::from)
                    .collect()
            })
            .unwrap_or_default();
        let rows = composer_menu_rows(
            surface,
            ComposerMenuConditions {
                model: self.configurable(),
                prompts: !prompts.is_empty(),
                ..ComposerMenuConditions::default()
            },
        );
        let (models, model_picked): (&'static [(&'static str, &'static str)], String) = {
            let value = crate::session_extras::option_chip(self.feed.config(), CONFIG_MODEL_ID)
                .map(|chip| chip.value)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| crate::composer::session_fallback_model(self.row.as_ref(), cx));
            let choices: &'static [(&'static str, &'static str)] = if self.agent() == SessionAgent::Claude {
                &crate::coding_selects::MODEL_CHOICES
            } else {
                &crate::coding_selects::CODEX_MODEL_CHOICES
            };
            (choices, value)
        };
        let input = self.input.clone();
        let trigger = crate::composer::composer_tool(
            SharedString::from(format!("{}-plus", self.composer_ids.attach)),
            registry::UI_ADD,
            cx,
        )
        .tooltip(crate::chat_screen::PLUS_TOOLTIP)
        .disabled(self.sending);
        crate::controls::PointerMenu::for_button(trigger, move |mut menu, window, cx| {
            use crate::controls::{pointer_check_item, pointer_menu_item};
            let root = cx.entity().downgrade();
            menu = crate::picker::search_list::search_submenu_parent(menu.min_w(px(260.)));
            for row in rows.iter().copied() {
                menu = match row {
                    Row::Separator => menu.separator(),
                    Row::AddFile => {
                        let view = view.clone();
                        menu.item(pointer_menu_item(row.icon(), row.label(), |_| None, move |window, cx| {
                            if let Some(view) = view.upgrade() {
                                view.update(cx, |this, cx| this.pick_files(window, cx));
                            }
                        }))
                    }
                    Row::ImplementIssue => {
                        let Some(team_id) = team_id.clone() else {
                            continue;
                        };
                        let input = input.clone();
                        let root = root.clone();
                        menu.submenu_with_icon(row.icon().map(Icon::new), row.label(), window, cx, move |sub, window, cx| {
                            let input = input.clone();
                            let insert: std::rc::Rc<dyn Fn(String, &mut Window, &mut App)> =
                                std::rc::Rc::new(move |text: String, window: &mut Window, cx: &mut App| {
                                    input.update(cx, |state, cx| state.insert(text, window, cx));
                                    input.read(cx).focus_handle(cx).focus(window, cx);
                                });
                            crate::picker::search_list::menu_search_list_submenu(
                                "steer-issue-ref",
                                crate::picker::search_list::issue_ref_source(team_id.clone(), insert),
                                sub,
                                root.clone(),
                                window,
                                cx,
                            )
                        })
                    }
                    // #EXP-1434: Prompts › — the `/` pick's body, inserted at
                    // the caret.
                    Row::Prompts => {
                        let input = input.clone();
                        let root = root.clone();
                        let prompts = prompts.clone();
                        menu.submenu_with_icon(row.icon().map(Icon::new), row.label(), window, cx, move |sub, window, cx| {
                            let input = input.clone();
                            let insert: std::rc::Rc<dyn Fn(String, &mut Window, &mut App)> =
                                std::rc::Rc::new(move |text: String, window: &mut Window, cx: &mut App| {
                                    input.update(cx, |state, cx| state.insert(text, window, cx));
                                    input.read(cx).focus_handle(cx).focus(window, cx);
                                });
                            let pick: std::rc::Rc<dyn Fn(slash_commands::SlashPrompt, &mut Window, &mut App)> =
                                std::rc::Rc::new(move |prompt, window: &mut Window, cx: &mut App| {
                                    crate::picker::search_list::insert_prompt_body(prompt, insert.clone(), window, cx)
                                });
                            crate::picker::search_list::menu_search_list_submenu(
                                "steer-prompts",
                                crate::picker::search_list::prompt_source(prompts.clone(), pick),
                                sub,
                                root.clone(),
                                window,
                                cx,
                            )
                        })
                    }
                    Row::Model => {
                        let value = models
                            .iter()
                            .find(|(_, value)| *value == model_picked)
                            .map(|(label, _)| (*label).to_string())
                            .unwrap_or_else(|| crate::steer_viewer::model_label(&model_picked));
                        let picked = model_picked.clone();
                        let view = view.clone();
                        let session_id = session_id.clone();
                        menu.submenu_with_icon(row.icon().map(Icon::new), titled_row(row, Some(value)), window, cx, move |mut sub, _, _| {
                            for (label, value) in models {
                                let view = view.clone();
                                let session_id = session_id.clone();
                                let value = (*value).to_string();
                                sub = sub.item(pointer_check_item(*label, picked == value, move |_, cx| {
                                    Self::configure(
                                        view.clone(),
                                        api::sessions::ConfigureInput {
                                            session_id: session_id.clone(),
                                            model: Some(value.clone()),
                                            ..Default::default()
                                        },
                                        cx,
                                    );
                                }));
                            }
                            sub
                        })
                    }
                    // Launch-only rows never reach a session's surface.
                    _ => menu,
                };
            }
            menu
        })
        .anchor(gpui::Anchor::BottomLeft)
        .into_any_element()
    }

    /// EXP-877 — the context ring, and the usage sheet it opens. Hidden on a
    /// replay and once the run is over (a finished run's window is a number
    /// about a machine that is no longer working) and whenever the run has
    /// published no window at all.
    fn render_context_ring(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        if self.source.read_only() || self.session_over() {
            return None;
        }
        // EXP-1051: the sheet is a pure render — the layout and the legend's
        // fold are read off the view HERE (the popover's content closure runs
        // while this view is leased, so it can never read the entity back) and
        // the toggle flips the field and notifies, which rebuilds the popover
        // with the rest of this view's tree.
        let view = cx.entity();
        let on_toggle_legend: std::rc::Rc<dyn Fn(&mut Window, &mut gpui::App)> = std::rc::Rc::new(move |_window, cx| {
            view.update(cx, |view, cx| {
                view.context_legend_open = !view.context_legend_open;
                cx.notify();
            });
        });
        crate::composer::session_context_ring(
            crate::composer::ContextRingProps {
                id: self.composer_ids.context.clone(),
                session_id: self.session_id.to_string(),
                device_id: self.session_row().and_then(|row| row.device_id.clone()),
                agent: self.builtin_agent(),
                local: self.is_local(),
                working: self.working_now(),
                usage: self.usage(),
                layout: self.context_layout(),
                legend_open: self.context_legend_open,
                on_toggle_legend,
            },
            cx,
        )
    }

    /// EXP-724: the pinned strip while the agent folds its context. The bar
    /// is INDETERMINATE on purpose — no agent reports compaction progress,
    /// and the only honest signal is "still going". EXP-1157: a quiet
    /// status line, not a banner — a short hairline track beside the label,
    /// no divider above it.
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

/// #EXP-1383: a view over a FINISHED local engine follows a LIVE one hosting
/// the same session (the restarted agent); a live engine of its own stays.
fn follows_engine(current_done: bool, next_done: bool) -> bool {
    current_done && !next_done
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

/// EXP-909 — the account a run hosted HERE is spending, as the launcher
/// recorded it (`runs.json`): its profile id. `None` when this machine has no
/// record of the run (or a legacy one naming none) — and then nothing is
/// guessed, the sheet falls back to the machine's own report.
pub(crate) fn run_account(session_id: &str, cx: &App) -> Option<String> {
    coding::run_registry::get(&crate::coding_flow::coding_data_dir(cx), session_id)
        .and_then(|record| record.account())
}

/// EXP-909 — the one refresh the usage sheet asks for when it OPENS: read the
/// run's own login now, so a run parked on an account nobody else spends does
/// not show numbers from its last turn.
///
/// A run hosted here reads it in-process ([`crate::device_sync::
/// refresh_agent_usage_here`], the polite read); one hosted on another of MY
/// machines gets the `agent_usage_refresh` command — but only while that
/// machine is online and its last fetch is past the shared floor, which is
/// the same gate the Devices page applies. A
/// teammate's machine is never asked: it is not mine to poll.
pub(crate) fn refresh_run_usage(
    session_id: &str,
    device_id: Option<&str>,
    agent: coding::CodingAgent,
    local: bool,
    cx: &mut App,
) {
    if local {
        // No recorded account (a legacy run): the device's last used login.
        let profile = run_account(session_id, cx).unwrap_or_default();
        crate::device_sync::refresh_agent_usage_here(agent, profile, cx);
        return;
    }
    let Some(device_id) = device_id.filter(|id| !id.is_empty()) else {
        return;
    };
    let Some(session_account) = sync::Store::try_global(cx).and_then(|store| {
        store
            .collections()
            .sessions
            .read(cx)
            .get(session_id)
            .map(|row| row.account.clone().unwrap_or_default())
    }) else {
        return;
    };
    let profile = coding::profile_id(Some(session_account.as_str()));
    let now_epoch = chrono::Utc::now().timestamp();
    let Some(row) = crate::usage_bar::device_profile_rows(cx)
        .into_iter()
        .find(|row| row.device_id == device_id && row.agent == agent.id() && row.profile_id == profile)
    else {
        return;
    };
    if !row.mine
        || !row.online
        || crate::usage_bar::refresh_allowed_at(row.usage.as_ref(), now_epoch).is_some()
    {
        return;
    }
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        return;
    };
    let device_id = device_id.to_string();
    let agent_id = agent.id().to_string();
    cx.spawn(async move |cx| {
        // A refusal (a command from the last round still queued) is
        // swallowed: nobody asked for this, and the next open looks again.
        let _ = cx
            .background_executor()
            .spawn(async move {
                api::devices::create_agent_usage_refresh_command(
                    &trpc,
                    &device_id,
                    &agent_id,
                    &profile,
                )
            })
            .await;
    })
    .detach();
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

/// EXP-787 — the transcript's PROSE rung: narration, a sent message, a card's
/// body. 14/22 from the shared tokens, on every client; the rem-relative
/// `text_sm` these used to take resolved to 12.25px here and to something else
/// on web, so a run read differently per platform.
fn body_text<E: Styled>(element: E) -> E {
    element
        .text_size(px(transcript::BODY_SIZE))
        .line_height(px(transcript::BODY_LINE_HEIGHT))
}

/// EXP-787/EXP-927/EXP-1191 — the reading column every part of the
/// conversation takes: the work column ([`crate::work_header::WORK_COLUMN_W`],
/// the same measure as the run header, the issue body and the diff page)
/// centred on the PANE exactly like [`crate::issue_detail::centered_column`],
/// its content inset by [`crate::work_header::WORK_GUTTER`]. So transcript
/// text, the user bubbles' right edge, tool / subagent cards, the working
/// row, the conversation tabs, every strip block and the composer box all
/// sit on the header's content box — the same left and right edges as its
/// collapsed title, chips and trailing cluster, at every pane width.
///
/// EXP-927 (§2c "Alignment"): the transcript rows
/// ([`SteerSessionView::transcript_row`]) were the only thing that took it.
/// The conversation TABS and every strip block (task list, tasks and waits,
/// the queue bar) wrap their CONTENT in this too. Only the backgrounds
/// still span the panel.
/// The agent's prose row: its mark beside the markdown.
pub(crate) fn narration_row(view: crate::markdown::MarkdownView, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    h_flex()
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
        .child(body_text(div()).flex_1().min_w_0().child(view))
        .into_any_element()
}

/// One row in the centred work column (`WORK_COLUMN_W` inside the
/// `WORK_GUTTER`) — the transcript's rows and the session header.
pub(crate) fn work_column_row(element: AnyElement) -> gpui::Div {
    h_flex().w_full().justify_center().child(
        div()
            .flex_basis(px(crate::work_header::WORK_COLUMN_W))
            .flex_shrink(1.)
            .min_w_0()
            .px(px(crate::work_header::WORK_GUTTER))
            .child(element),
    )
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

/// EXP-1241: the contract row of the comment call that renders as the
/// comment it posted.
const COMMENTS_CREATE_ROW: &str = "comments_create";
/// EXP-1241: the comment card's lead-in, before the issue chip (×4).
pub(crate) const COMMENTED_ON_LABEL: &str = "Commented on";

/// EXP-1279 — what a `codemode__exec` row's Script card is built from: the
/// model input, whether the call failed, and the script source.
///
/// The settle's `tool_update.script` (wire doc §1b, folded into the row by
/// the feed) carries the call log and the source; a row without one (still
/// running, an older host, a script refused before it ran) is the card's
/// "older codemode without a log" case: the title alone, plus the source
/// when that came alone.
fn script_card_input(item: &FeedItem) -> (steer::script_card::ScriptCardInput, bool, Option<String>) {
    let FeedKind::Tool { settled, failed, script, .. } = &item.kind else {
        return (steer::script_card::ScriptCardInput::default(), false, None);
    };
    let log = script.as_ref();
    let calls = log.map(|log| {
        log.calls
            .iter()
            .map(|call| steer::script_card::ScriptCall {
                tool: call.tool.clone(),
                subject: call.subject.clone(),
                ok: call.ok,
                ms: call.ms,
            })
            .collect()
    });
    (
        steer::script_card::ScriptCardInput {
            running: !*settled,
            count: log.and_then(|log| log.count).map(u64::from),
            calls,
            elapsed_ms: log.and_then(|log| log.elapsed_ms),
        },
        *failed,
        log.and_then(|log| log.source.clone()),
    )
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
///
/// EXP-1172: a `picture` call (`sessions_show`) hangs its `picture` tile (the
/// caller resolved it off the synced results) under the row instead of a
/// preview; none yet = the bare row.
#[allow(clippy::too_many_arguments)]
fn exp_tool_call_row(
    id: FeedItemId,
    display: steer::ExpToolDisplay,
    detail: Option<&str>,
    failed: bool,
    settled: bool,
    preview: Option<&steer::frames::ToolPreview>,
    picture: Option<AnyElement>,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    // EXP-1324: the sentence + the mark as a Trace line — the contract's
    // caption the title, the subject the description.
    let status = if failed {
        Some(steer::trace::TraceStatus::Error)
    } else if settled {
        None
    } else {
        Some(steer::trace::TraceStatus::Running)
    };
    let header = trace_line(
        TraceLine::new(
            ("steer-exp-tool", id as usize),
            Icon::from(crate::icons::ExpIcon::Logo),
            display.caption.to_string(),
        )
        .description(detail.map(|detail| detail.to_string()))
        .status(status),
        window,
        cx,
    );
    let body = if display.result == steer::exp_tool::result::PICTURE {
        picture
    } else {
        (!failed)
            .then(|| preview.and_then(|preview| exp_tool_preview_row(id, display.result, preview, cx)))
            .flatten()
    };
    match body {
        Some(body) => v_flex()
            .w_full()
            .min_w_0()
            .child(header)
            .child(trace_content(body, TraceSize::Sm, cx))
            .into_any_element(),
        None => header,
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
/// * `session` / `board` / `action` / `comment` — a name chip;
/// * `none` — nothing: the caption said it all.
///
/// EXP-920: a preview carrying `refs` renders the ENTITY CHIP ROW instead
/// ([`exp_entity_chip_row`]) — one chip per ref group, each with a hover
/// card resolved from the synced store and a click into the entity. A
/// pre-EXP-920 preview (no refs) keeps the single-subject rendering above.
fn exp_tool_preview_row(
    id: FeedItemId,
    result: &str,
    preview: &steer::frames::ToolPreview,
    cx: &mut App,
) -> Option<AnyElement> {
    use steer::exp_tool::result as kind;
    if !preview.refs.is_empty() {
        return Some(exp_entity_chip_row(id, &preview.refs, cx));
    }
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
            // Not synced here (another team's board, a trashed one): the ONE
            // issue chip over what the answer itself named. EXP-885 — it used
            // to be `exp_preview_chip`, a bare glyph+text row that looked
            // nothing like the chip the SYNCED path hovers into. No status:
            // there is no row here to resolve one from, and a chip must not
            // invent one.
            let identifier = preview
                .identifier
                .clone()
                .map(|identifier| identifier.trim().to_string())
                .filter(|identifier| !identifier.is_empty());
            let title = preview
                .title
                .clone()
                .map(|title| title.trim().to_string())
                .filter(|title| !title.is_empty());
            if identifier.is_none() && title.is_none() {
                return None;
            }
            Some(
                crate::issue_chip::issue_chip(
                    ("steer-exp-issue-chip", id as usize),
                    identifier.unwrap_or_default(),
                    title.unwrap_or_default(),
                )
                .flexible()
                .into_any_element(),
            )
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
        kind::NONE | kind::RESULTS | kind::PICTURE => None,
        // session / board / action / comment: a name chip.
        _ => exp_preview_label(preview).map(|label| exp_preview_chip(registry::CODING_TOOL, label, cx)),
    }
}

/// EXP-920 — the wrapping row of entity chips under a settled Exponential
/// tool row: one [`crate::entity_chip::entity_chip`] per
/// [`domain::entity_preview::group_preview_refs`] group (a `list` ref
/// absorbs the member refs behind it into ONE chip whose card lists them).
/// Hover = the card; click = [`crate::entity_preview::entity_target`], or
/// nothing when the target does not resolve here (a `list`, an unsynced row).
fn exp_entity_chip_row(id: FeedItemId, refs: &[steer::EntityRef], cx: &mut App) -> AnyElement {
    let views: Vec<domain::entity_preview::EntityRefView<'_>> =
        refs.iter().map(crate::entity_preview::view).collect();
    let groups = domain::entity_preview::group_preview_refs(&views);
    let mut row = h_flex().w_full().min_w_0().flex_wrap().gap_1().items_center();
    for (index, group) in groups.iter().enumerate() {
        let r#ref = &refs[group.r#ref];
        let members: Vec<steer::EntityRef> =
            group.members.iter().map(|&member| refs[member].clone()).collect();
        let key = format!("steer-exp-entity-{}-{index}", id as usize);
        let mut chip = crate::entity_chip::entity_chip(
            ElementId::from(SharedString::from(key.clone())),
            r#ref,
            &members,
        );
        // A card exists for a synced row, a list, and the one never-synced
        // kind (a slim card); anything else hovers into nothing, so it
        // does not even ask.
        let has_card = matches!(r#ref.kind.as_str(), "list" | "repository")
            || crate::entity_preview::row_facts(&r#ref.kind, &r#ref.id, cx).synced;
        if has_card {
            chip = chip.hover_card(key);
        }
        if let Some(target) = crate::entity_preview::entity_target(&r#ref.kind, &r#ref.id, cx) {
            chip = chip.on_click(move |_, window, cx| crate::entity_preview::open_target(target.clone(), window, cx));
        }
        row = row.child(chip);
    }
    row.into_any_element()
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
        // #NEW-54: an utterance push to talk could not send waits in this
        // chat's composer (the member's conversation with the voice agent).
        if let Some(unsent) = crate::talk_host::take_unsent(&self.session_id, cx) {
            let draft = restore_unread_to_draft(&self.input.read(cx).value(), &unsent);
            self.input.update(cx, |state, cx| state.set_value(draft, window, cx));
        }
        self.dev_scroll_to_script_card();
        self.sync_card_focus(window, cx);
        // EXP-789: the subagent strip sits between the header and the feed.
        let strip = self.render_subagent_strip(cx);
        // N-11 (owner 2026-10-24): no status row over the transcript; the
        // live "Working…" row at its bottom is the one working signal.
        // N-12: THE Dock (the waiting cards, THE Queue, the queued messages,
        // the banners, then THE composer) floats over the transcript's
        // bottom edge; the transcript is the ONE scroll region and pads by
        // the band's height.
        let (dock, sheet) = self.render_session_dock(window, cx);
        let composer_visible = self.composer_visible();
        let feed = self.render_feed(cx);
        let width_probe = self.view_width.clone();
        let jump =
            self.render_jump_to_bottom(&self.list, !self.feed.is_empty(), composer_visible, cx);
        let conversation = v_flex()
            .flex_1()
            .h_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .children(strip)
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    // EXP-1228: the transcript's right-click menu. A div
                    // listener fires after its children's, so an image or
                    // media tile's own menu claims the press first
                    // (`controls::claim_right_click`).
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(Self::on_feed_context_menu),
                    )
                    .child(feed)
                    .children(dock)
                    .child(jump)
                    .children(sheet),
            );
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
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .min_w_0()
                    .child(
                        // Deliberately NOT `h_flex` — its `items_center`
                        // would size the child to its content rather than to
                        // the full height.
                        div()
                            .flex()
                            .flex_col()
                            .size_full()
                            .min_w_0()
                            .min_h_0()
                            .child(conversation),
                    )
                    // The view's width, read back for the next frame's bubble
                    // cap — the canvas paints nothing.
                    .child(
                        gpui::canvas(
                            move |bounds, _, _| {
                                width_probe.set(bounds.size.width);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    ),
            )
    }
}

impl Drop for SteerSessionView {
    fn drop(&mut self) {
        if let Some(handle) = self.source.handle() {
            handle.shutdown();
        }
    }
}

/// EXP-1259 #11 — a Work card's elapsed: from the call's start to its settle
/// `end`, or to `now` while it still runs. `None` without a start stamp (an
/// older publisher), or a finished call without its settle stamp.
pub(crate) fn work_elapsed_ms(
    start: Option<i64>,
    end: Option<i64>,
    running: bool,
    now: i64,
) -> Option<u64> {
    let start = start?;
    let end = if running { now } else { end? };
    Some(end.saturating_sub(start).max(0) as u64)
}

/// The workflow's elapsed when its call carries no stamps: the sum of its
/// phases' spans (each phase = its slowest settled lane; phases run in
/// sequence). `None` before any lane settled — the web's
/// `workflowElapsedMs`.
fn workflow_span_ms(workflow: &steer::WorkflowState) -> Option<u64> {
    let mut spans: Vec<(Option<u32>, u64)> = Vec::new();
    for agent in &workflow.agents {
        let Some(duration) = agent.duration_ms else {
            continue;
        };
        match spans.iter_mut().find(|(phase, _)| *phase == agent.phase_index) {
            Some((_, span)) => *span = (*span).max(duration),
            None => spans.push((agent.phase_index, duration)),
        }
    }
    (!spans.is_empty()).then(|| spans.iter().map(|(_, span)| span).sum())
}

fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod work_elapsed_tests {
    use super::work_elapsed_ms;

    #[test]
    fn a_work_card_counts_from_the_call_start() {
        // Running: start to now.
        assert_eq!(work_elapsed_ms(Some(1_000), None, true, 6_000), Some(5_000));
        // Finished: start to the settle, never to now.
        assert_eq!(work_elapsed_ms(Some(1_000), Some(4_000), false, 60_000), Some(3_000));
        // No start stamp, or a finished call without its settle: nothing.
        assert_eq!(work_elapsed_ms(None, Some(4_000), false, 60_000), None);
        assert_eq!(work_elapsed_ms(Some(1_000), None, false, 60_000), None);
        // A skewed clock never goes negative.
        assert_eq!(work_elapsed_ms(Some(9_000), None, true, 6_000), Some(0));
    }
}

#[cfg(test)]
mod tests {
    /// EXP-1266: the transcript menu's Copy dispatches after the native menu
    /// closes, through whatever holds focus — here nothing does — so only an
    /// App-global handler reaches it, and it copies the text snapshotted
    /// when the menu opened.
    #[gpui::test]
    async fn transcript_menu_copy_reaches_the_clipboard_without_focus(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(super::ensure_copy_action_registered);
        let cx = cx.add_empty_window();
        cx.update(|window, cx| {
            window.dispatch_action(
                Box::new(super::CopyTranscriptSelection { text: "docker exec -it openclaw".into() }),
                cx,
            );
        });
        cx.run_until_parked();
        let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
        assert_eq!(copied.as_deref(), Some("docker exec -it openclaw"));
    }

    /// The synthetic rows (orphan workflow cards, then the conversation
    /// cards with no row to sit above) key ascending between the feed and
    /// the working row, and a position past the room never reaches the
    /// working row's id nor wraps into the feed's ids.
    /// #EXP-1383: the chat follows its restarted engine — only a FINISHED
    /// local engine hands over, and only to a LIVE one.
    #[test]
    fn a_finished_local_engine_hands_the_view_to_the_live_one() {
        assert!(follows_engine(true, false));
        assert!(!follows_engine(false, false), "our own engine is still live");
        assert!(!follows_engine(true, true), "nothing live to follow");
        assert!(!follows_engine(false, true));
    }
    #[test]
    fn synthetic_row_ids_stay_below_the_working_row() {
        use crate::transcript_rows::{ORPHAN_ROW_BASE, WORKING_ROW_ID};
        assert_eq!(super::synthetic_row_id(0), Some(ORPHAN_ROW_BASE));
        let last = super::synthetic_row_id(super::SYNTHETIC_ROWS - 1).unwrap();
        assert!(last < WORKING_ROW_ID);
        assert!(super::synthetic_row_id(511) < super::synthetic_row_id(512));
        assert_eq!(super::synthetic_row_id(super::SYNTHETIC_ROWS), None);
        assert_eq!(super::synthetic_row_id(usize::MAX), None);
    }

    #[test]
    fn rate_limit_caption_keeps_message_and_countdown_apart() {
        assert_eq!(
            rate_limit_caption(Some("  Weekly limit hit "), Some("resets in 2h 57m".into())),
            ("Weekly limit hit".to_string(), Some("resets in 2h 57m".to_string())),
        );
        assert_eq!(
            rate_limit_caption(Some("   "), None),
            ("Rate limit reached".to_string(), None),
        );
        assert_eq!(rate_limit_caption(None, None).0, "Rate limit reached");
        // FEED-61: the muted span names the wall's scope, ×4.
        assert_eq!(
            rate_limit_detail(Some("resets in 2h 57m".into())),
            "resets in 2h 57m · per account"
        );
        assert_eq!(rate_limit_detail(None), "per account");
    }

    use super::*;

    /// EXP-1294 rfix: a card's row is the one carrying ITS question id —
    /// never the newest open card of its kind (an agent's tool-posted ask
    /// waiting beside an engine ask that has no row yet).
    #[test]
    fn a_cards_row_is_the_one_with_its_question_id_never_the_newest_open_card() {
        let row = |id: &str, card: serde_json::Value, answered: bool| domain::rows::MessageRow {
            id: id.into(),
            card: Some(card),
            answered_at: answered.then(|| "2026-10-19T00:00:00Z".to_string()),
            ..Default::default()
        };
        let messages = vec![
            row("m-engine", serde_json::json!({"kind": "question", "question": "Which?", "questionId": "q-1"}), false),
            row("m-tool", serde_json::json!({"kind": "question", "question": "Ship?", "options": ["Yes"]}), false),
        ];
        assert_eq!(card_row_id(&messages, "q-1").as_deref(), Some("m-engine"));
        // The engine's next ask has no row yet: nobody's row (the live
        // layer answers), never the tool card's.
        assert_eq!(card_row_id(&messages, "q-2"), None);
        // An answered row is never answered again; a re-emitted id takes
        // the newest open row, whichever kind.
        let messages = vec![
            row("m-old", serde_json::json!({"kind": "question", "question": "Which?", "questionId": "q-1"}), true),
            row("m-new", serde_json::json!({"kind": "plan", "plan": "# Plan", "questionId": "q-1"}), false),
            row("m-text", serde_json::json!({"kind": "text", "questionId": "q-1"}), false),
        ];
        assert_eq!(card_row_id(&messages, "q-1").as_deref(), Some("m-new"));
        assert_eq!(card_row_id(&messages[..1], "q-1"), None);
    }

    // ── A session tab that exists at boot dials once an account is active ──

    #[test]
    fn a_pending_dial_retries_until_it_succeeds_then_never_again() {
        let (tx, rx) = flume::unbounded::<ViewerEvent>();
        let mut pending = PendingDial(Some(tx));
        // Boot: nobody signed in, the dial has nothing to dial with.
        assert_eq!(pending.attempt(|_| None::<()>), None);
        assert!(pending.is_pending(), "a failed dial keeps the sender for the next edge");
        // The account edge: the dial gets a live sender into the feed.
        let dialled = pending.attempt(|tx| {
            tx.send(ViewerEvent::Phase(ViewerPhase::Live)).ok()?;
            Some("handle")
        });
        assert_eq!(dialled, Some("handle"));
        assert!(matches!(rx.try_recv(), Ok(ViewerEvent::Phase(ViewerPhase::Live))));
        assert!(!pending.is_pending());
        // Later session edges never dial a second socket.
        assert_eq!(pending.attempt(|_| -> Option<&str> { panic!("dialled twice") }), None);
    }

    #[test]
    fn a_source_that_never_waited_has_nothing_to_dial() {
        let mut pending = PendingDial(None);
        assert!(!pending.is_pending());
        assert_eq!(pending.attempt(|_| -> Option<()> { panic!("dialled") }), None);
    }

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
    }

    /// FEED-26: the threshold is shared byte-for-byte with the other three
    /// clients — 10 minutes, and the caption counts WHOLE minutes.
    #[test]
    fn the_stale_activity_window_is_ten_minutes() {
        assert_eq!(STALE_ACTIVITY_AFTER, Duration::from_secs(600));
        assert!(STALE_TICK < STALE_ACTIVITY_AFTER);
    }

    /// EXP-1279: the settle's `script` log (folded into the row by the feed)
    /// is what the desktop Script card groups; without one it is the title.
    #[test]
    fn the_script_card_reads_the_settled_log() {
        let row = |script: Option<steer::frames::ScriptLog>| FeedItem {
            id: 1,
            kind: FeedKind::Tool {
                name: "mcp__codemode__exec".into(),
                detail: None,
                subagent_id: None,
                call_id: Some("tc-1".into()),
                tool_kind: None,
                settled: true,
                failed: false,
                diff: None,
                output: None,
                preview: None,
                script,
                settled_at: None,
                started_at: None,
            },
            seq: None,
        };
        let log = steer::frames::ScriptLog {
            count: Some(3),
            elapsed_ms: Some(2_140),
            calls: vec![
                steer::frames::ScriptLogCall {
                    tool: "exponential.exponential_issues_get".into(),
                    subject: Some("APP-5".into()),
                    ok: true,
                    ms: Some(180),
                },
                steer::frames::ScriptLogCall {
                    tool: "exponential.exponential_issues_pr_files".into(),
                    subject: Some("APP-9".into()),
                    ok: false,
                    ms: None,
                },
            ],
            source: Some("return 1".into()),
        };
        let (input, failed, source) = script_card_input(&row(Some(log)));
        assert!(!input.running && !failed);
        assert_eq!(input.count, Some(3));
        assert_eq!(input.elapsed_ms, Some(2_140));
        assert_eq!(input.calls.as_ref().map(Vec::len), Some(2));
        assert_eq!(source.as_deref(), Some("return 1"));
        let card = steer::script_card::script_card(&input);
        assert_eq!(card.groups.len(), 2);
        assert!(card.meta.is_some());

        let (bare, _, source) = script_card_input(&row(None));
        assert_eq!(bare, steer::script_card::ScriptCardInput::default());
        assert_eq!(source, None);
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
        // EXP-861: the queue bar's two captions, byte-identical ×4 too.
        assert_eq!(QUEUE_STRIP_TITLE, "Queued");
        assert_eq!(steer::feed::QUEUE_REMOVE_LABEL, "Remove from queue");
        assert_eq!(COMPACTED_LABEL, "Context compacted");
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
        // A blank reason is still a FAILURE — the exit callback only hands
        // one over when the run died (EXP-877).
        assert_eq!(failure_banner("   \n "), FAILURE_FALLBACK);
        // The phase mapping carries it.
        assert_eq!(
            viewer_phase(engine::EnginePhase::Failed("boom".to_string())),
            ViewerPhase::Ended {
                outcome: Some("Session failed: boom".to_string())
            }
        );
    }

    /// EXP-877: the footer prints a model's NAME, not its wire value — the
    /// picker's own label for an alias, the codex label for a slug, and a
    /// capitalised fallback for anything this build does not know (so a new
    /// tier still reads as a name). Byte-identical with `MODEL_CHOICES`.
    #[test]
    fn the_footer_prints_a_models_name() {
        assert_eq!(model_label("opus"), "Opus");
        assert_eq!(model_label("fable"), "Fable");
        assert_eq!(model_label("sonnet"), "Sonnet");
        assert_eq!(model_label("gpt-5.6-sol"), "GPT-5.6 Sol");
        // Unknown to this build: raised, never dropped.
        assert_eq!(model_label("claude-opus-4-7"), "Claude-opus-4-7");
        // A BLANK value never reaches here (`render_model_pin` renders
        // nothing for codex's CLI default), and produces nothing if it does.
        assert_eq!(model_label(""), "");
        // The picker's labels ARE the table — a rename there renames here.
        for (label, value) in crate::coding_selects::MODEL_CHOICES {
            assert_eq!(model_label(value), label);
        }
    }

    /// EXP-877: an ended run banners a FAILURE and nothing else. A plain end
    /// (the engine's `Ended`) and a publisher's `bye` token (`exit:0`, the
    /// history handshake's `history`) render no strip at all — the header's
    /// status is where "ended" belongs.
    #[test]
    fn only_a_real_failure_still_banners_an_ended_run() {
        assert_eq!(
            ended_banner(Some(&failure_banner("boom"))).as_deref(),
            Some("Session failed: boom")
        );
        assert_eq!(
            ended_banner(Some(&failure_banner(""))).as_deref(),
            Some(FAILURE_FALLBACK)
        );
        // A plain `bye`, whatever outcome it carried.
        assert_eq!(ended_banner(None), None);
        assert_eq!(ended_banner(Some("exit:0")), None);
        assert_eq!(ended_banner(Some("history")), None);
        assert_eq!(ended_banner(Some("")), None);
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

    /// EXP-1294: a SHARED session is any member's to message and Stop, a
    /// private one only its starter's; nobody signed in steers nothing
    /// private.
    #[test]
    fn who_may_steer_a_session() {
        let row = |shared: bool| -> domain::rows::SessionRow {
            serde_json::from_value(serde_json::json!({
                "id": "sess-1",
                "user_id": "user-1",
                "shared": shared,
            }))
            .unwrap()
        };
        assert!(may_steer(&row(true), Some("user-2")));
        assert!(may_steer(&row(true), None));
        assert!(may_steer(&row(false), Some("user-1")));
        assert!(!may_steer(&row(false), Some("user-2")));
        assert!(!may_steer(&row(false), None));
    }

    /// EXP-1294: a plan picker DECIDES (its last option rejects, carrying
    /// the note), a question answers with its free text, else its picks.
    #[test]
    fn a_card_answer_maps_onto_sessions_answer() {
        let plan = steer::QuestionCard {
            plan_mode: true,
            options: vec![option("Yes", false), option("No, keep planning", false)],
            ..Default::default()
        };
        let approve = card_answer_input(&plan, "m-1".into(), &["yes".into()], &[], None, Some("x".into()));
        assert_eq!(approve.decision.as_deref(), Some("approved"));
        assert_eq!(approve.feedback, None, "only a reject carries the note");
        let reject = card_answer_input(
            &plan,
            "m-1".into(),
            &["no, keep planning".into()],
            &[],
            None,
            Some("smaller steps".into()),
        );
        assert_eq!(reject.decision.as_deref(), Some("rejected"));
        assert_eq!(reject.feedback.as_deref(), Some("smaller steps"));
        assert_eq!(approve.fresh, None, "the engine's picker has no fresh-context option");

        let question = steer::QuestionCard { options: vec![option("A", false), option("B", false)], ..Default::default() };
        let picks = card_answer_input(&question, "m-2".into(), &[], &["A".into(), "B".into()], None, None);
        assert_eq!(picks.answer.as_deref(), Some("A, B"));
        assert_eq!(picks.decision, None);
        let typed = card_answer_input(&question, "m-2".into(), &[], &["Other".into()], Some("free"), None);
        assert_eq!(typed.answer.as_deref(), Some("free"));
        assert_eq!(typed.message_id, "m-2");
    }
}
