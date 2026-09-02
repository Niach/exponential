//! The bottom terminal dock (masterplan-v3 §6.13 / §7.5) — the
//! JetBrains-style multi-tab terminal panel.
//!
//! One [`TerminalDockPanel`] per window lives inside the bottom `Dock`'s
//! `TabPanel` (§3.3); *inside* it, a gpui-component `Tab`/`TabBar` strip
//! lists the [`terminal::TerminalManager`]'s sessions — **not** Zed's GPL
//! `Pane`/`Dock` (§6.13's licensing rule). Behavior:
//!
//! - **"+"** → a dropdown (EXP-325): one item per doctor-installed agent
//!   CLI, launching a PROMPTLESS chat run (EXP-703) — a real steerable
//!   `coding_sessions` row on the builtin `builtin:chat` rails, in its own
//!   `exp/chat-<id8>` worktree, the agent waiting at its prompt — on the
//!   current team's repo (a repo submenu when the team has several), plus
//!   "New shell" — the plain `Shell` tab (`$SHELL -l`, cwd = the active
//!   board's **trunk** clone root, v4 §4.6; `$HOME` only off a board screen
//!   or before the clone exists), which cmd-t / ctrl-shift-t inside the
//!   dock still opens directly. This is also the launch surface: the
//!   Start-coding launcher and the actions panel call the same
//!   `TerminalManager::open_tab`.
//! - close buttons per tab (middle-click too, and cmd-w / ctrl-shift-w),
//!   ctrl-tab / ctrl-shift-tab to switch; tabs that don't fit the strip
//!   collapse into a trailing "+N" dropdown exactly like the center tab
//!   strip (EXP-497 — no cut-off horizontal scroll);
//! - the strip is ONE fixed 29px band pinned to the panel's BOTTOM edge
//!   (EXP-688), open or collapsed, so the dock grows upward out of the tabs
//!   the way the web dock does. Its right cluster carries "Open in new
//!   window" (the active tab's undock, which used to be a per-chip hover
//!   affordance) and the open/close chevron. The EXP-484 per-tab toolbar is
//!   GONE: it repeated the chip it sat under, and usage moved to the agent's
//!   tab in Device settings;
//! - a **"Latest changes"** row above the strip (EXP-688) for the active
//!   session tab: the BRANCH diff (`coding::scm::branch_diff` — committed
//!   work included, so it survives the agent's commit) with `+adds -dels`,
//!   expanding into the shared side-by-side view, plus the "Merge" button.
//!   It renders for a diff OR an open PR, so Merge never stands alone;
//! - **empty state** (EXP-369): an expanded, tab-less dock NEVER spawns
//!   anything by itself — it renders the tab bar over a row of launch cards
//!   (`render_empty_dock_options`), one per doctor-installed agent plus "New
//!   shell", carrying exactly the `+` menu's options and gating. Picking one
//!   hides the whole row behind a progress line for the rest of the launch
//!   (EXP-372) — the agent prepare takes about a second, and cards that stayed
//!   clickable through it spawned one tab per click;
//! - the dock **expands when a tab is created** (`TabOpened` →
//!   `Dock::set_open`, §4's dock open/close) and the new tab's terminal is
//!   focused; the grid element resizes with the dock (§6.10);
//! - a dead tab **stays open** with its final scrollback and shows the
//!   JetBrains "Process finished with exit code N" strip + a green-0 /
//!   red-non-zero badge on the tab (§7.5's exit-code strip);
//! - persistence (EXP-301): **nothing terminal-side is persisted**. A launch
//!   never opens a terminal in the user's face — no tab is respawned, and the
//!   bottom dock always comes up collapsed (see `Shell::install_fixed_chrome`).
//!   Terminals only ever appear from an explicit user action: the "+" / cmd-t,
//!   an empty-state card, a Start-coding run, or an action run — expanding
//!   the dock itself spawns nothing (EXP-369).
//!
//! **Phase-5 deferral (§6.7):** "child exit ends the `coding_sessions` row"
//! is the launcher's wiring — it passes an `ExitHook` into `open_tab`; the
//! dock/manager only surface the exit edge.

use gpui::{
    actions, div, prelude::FluentBuilder as _, px, AnyElement, App, AppContext as _, Bounds,
    ClickEvent, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding,
    MouseButton, ParentElement, Pixels, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    dock::{register_panel, DockArea, Panel, PanelControl, PanelEvent, PanelState},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};
use gpui::Task;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use terminal::{TabId, TabKind, TerminalManager, TerminalManagerEvent, TerminalView};

use crate::coding_flow::{CodingHub, TokenRefreshers};
use crate::icons::{registry, ExpIcon};
use crate::navigation;
use crate::queries::CodingSessionDisplay;
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup, RepoResolver};
use crate::steer_viewer::SteerSessionView;

/// Stable serialization name for the panel registry (§3.3: never change it).
pub const PANEL_NAME: &str = "TerminalDock";

/// EXP-523: the bottom dock slides open and shut instead of snapping. The
/// duration is the shared `standard` motion token, the same one the left
/// column's rail-to-settings swap uses.
const DOCK_SLIDE_DURATION: Duration = theme::motion::STANDARD;

/// Upstream `Dock::render`'s CLOSED height — the toggle strip it keeps when
/// `open == false`. The slide's closed endpoint.
const DOCK_STRIP_H: f32 = 29.;

/// EXP-688: how often the Latest-changes bar re-reads the branch diff. The
/// same 3s cadence the steer emitter's `DiffSnapshots` publishes on. While
/// there is nothing to read (collapsed dock, no session tab) the loop idles
/// on the shorter beat instead — it runs no git at all there, and the bar
/// then appears within a blink of the dock opening rather than 3s later.
const CHANGES_POLL: Duration = Duration::from_secs(3);
const CHANGES_IDLE_POLL: Duration = Duration::from_millis(500);

/// The Latest-changes bar's own height, and the expanded diff's (the web's
/// `max-h-72`).
const CHANGES_BAR_H: f32 = 28.;
const CHANGES_DIFF_H: f32 = 288.;

/// Slide tick. ~120Hz, so the animation is smooth on high-refresh displays
/// and the cost is ~24 `set_size` calls over a whole open — trivial next to
/// the per-frame layout it triggers, and it never touches the PTY (see
/// [`DockSlide`]).
const DOCK_SLIDE_FRAME: Duration = Duration::from_millis(8);

/// The bottom dock's open/close animation (EXP-523).
///
/// # Why not just animate `Dock::set_size` and let the terminal follow
///
/// `TerminalView` derives `(cols, rows)` from its element bounds on every
/// prepaint and calls `session.resize`, which dedupes only on an INTEGER cell
/// change — there is no debounce. At a 17px line height a 29 -> 240px open
/// crosses ~12 row values, i.e. ~12 `TIOCSWINSZ` calls and ~12 SIGWINCHes to
/// the child in 180ms. Every shell reprints its prompt and every TUI does a
/// full-screen redraw per signal, and this happens at exactly the moment a
/// fresh agent is starting up. So the panel PINS its content to the slide's
/// resting height and lets the shrinking dock clip it: the grid's bounds never
/// move, and the PTY reshapes at most once per open (on the first frame, which
/// is strictly better than today's reshape after the snap).
///
/// # Why the content is bottom-anchored and clipped
///
/// `Dock::set_size` clamps to upstream's `PANEL_MIN_SIZE` (100px), so heights
/// between the 29px strip and that floor are not addressable — a naive
/// animation would snap the last ~71px at the slow end of the easing curve,
/// where the eye is most sensitive. So the panel does not follow the stored
/// height at all: EXP-688's layout renders the content into a CLIP sized off
/// what the tick REQUESTED ([`DockSlide::content_clip_height`]), with the
/// content itself kept at its resting height and anchored to the clip's
/// bottom edge. The clip eats it from the top, the grid's bounds never move,
/// and the visible top edge tracks the virtual height right through the
/// clamped region. The band this exposes above it is the same
/// `theme::background_gradient()` quad the center already paints, so it reads
/// as the center growing, not as a hole. (`applied`/`requested` are still
/// recorded — the drag-collision check in `tick_slide` compares against what
/// upstream actually stored.)
///
/// Materials, while we are here: gpui has NO in-scene backdrop blur, so the
/// strip's and the changes bar's "glass" is the Android approximation —
/// white-alpha `theme::tokens::glass` fills over the page gradient. They
/// STACK above the terminal rather than overlaying it: a translucent strip
/// with no blur sitting on top of a prompt line is a terminal-UX bug.
#[derive(Clone, Copy, Debug, PartialEq)]
struct DockSlide {
    /// Virtual height at the start of this leg. May be below the clamp floor.
    from: f32,
    /// Virtual height to land on: [`DOCK_STRIP_H`] closing, `rest_height`
    /// opening.
    to: f32,
    /// The user's dock height — the height the content is PINNED to for the
    /// whole flight, and the value re-asserted into `set_size` on settle so a
    /// close cannot persist a clamped intermediate.
    rest_height: f32,
    opening: bool,
    /// Guards a settle against a leg that has since been retargeted
    /// (`LeftColumnAnim::epoch`).
    epoch: u64,
    started: Instant,
    /// What our last `set_size` actually produced. Both the clamp-offset
    /// source and the "someone else owns the height now" detector.
    applied: f32,
    /// The virtual height that produced [`Self::applied`] — the pair is what
    /// makes the clamp offset a readback rather than a hardcoded 100px.
    requested: f32,
}

impl DockSlide {
    fn new(from: f32, to: f32, rest_height: f32, opening: bool, now: Instant) -> Self {
        Self {
            from,
            to,
            rest_height,
            opening,
            epoch: 0,
            started: now,
            applied: from,
            requested: from,
        }
    }

    /// Record what `set_size(requested)` actually stored.
    fn record_apply(&mut self, requested: f32, applied: f32) {
        self.requested = requested;
        self.applied = applied;
    }

    /// Eased 0..=1.
    fn progress(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.started).as_secs_f32();
        let total = DOCK_SLIDE_DURATION.as_secs_f32();
        let linear = if total <= 0.0 {
            1.0
        } else {
            (elapsed / total).clamp(0.0, 1.0)
        };
        theme::motion::standard()(linear)
    }

    fn done(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) >= DOCK_SLIDE_DURATION
    }

    /// The height the dock WANTS this frame — not necessarily what it gets.
    fn virtual_height(&self, now: Instant) -> f32 {
        let t = self.progress(now);
        self.from + (self.to - self.from) * t
    }

    /// The height of the CLIP the content is rendered into this frame: the
    /// virtual height minus the strip that always sits under it. The content
    /// itself keeps its resting height and is bottom-anchored inside, so the
    /// clip eats it from the TOP as the dock closes — the grid's bounds
    /// never move (EXP-523's PTY guarantee) and the visible top edge tracks
    /// the virtual height right through `set_size`'s clamp, because it is
    /// what we REQUESTED that sizes the clip, not what upstream stored.
    fn content_clip_height(&self) -> f32 {
        (self.requested - DOCK_STRIP_H).max(0.)
    }

    /// Reverse mid-flight. Unlike `LeftColumnAnim`, which jumps to its
    /// previous target, this restarts from the CURRENT virtual height — the
    /// value is a real scalar here, so the reversal is continuous.
    fn retarget(&mut self, now: Instant, to: f32, opening: bool) -> u64 {
        self.from = self.virtual_height(now);
        self.to = to;
        self.opening = opening;
        self.started = now;
        self.epoch += 1;
        self.epoch
    }
}

/// Keymap scope for the dock-local bindings — an ancestor of the focused
/// terminal view in the dispatch path, so the chords work while typing in
/// the terminal (bindings match before raw key-down listeners).
const KEY_CONTEXT: &str = "TerminalDock";

actions!(
    exp,
    [
        /// New plain shell tab in the terminal dock (§6.13 "+").
        NewTerminalTab,
        /// Close the active terminal tab (kills its child, §6.13).
        CloseTerminalTab,
        /// Switch to the next terminal tab.
        NextTerminalTab,
        /// Switch to the previous terminal tab.
        PrevTerminalTab,
    ]
);

/// EXP-523: open this dock area's bottom dock through the SLIDE, from outside
/// the panel. `undock`'s re-dock path used to poke `Dock::set_open` directly,
/// which would have been the one entry point that still snapped. Degrades to
/// the raw open if the panel cannot be resolved.
pub(crate) fn expand_terminal_dock(
    dock_area: &Entity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(dock) = dock_area.read(cx).bottom_dock().cloned() else {
        return;
    };
    let panel = crate::coding_flow::find_terminal_dock(dock.read(cx).panel());
    match panel {
        Some(panel) => {
            panel.update(cx, |panel, cx| panel.expand_dock(window, cx));
        }
        None if !dock.read(cx).is_open() => {
            dock.update(cx, |dock, cx| dock.set_open(true, window, cx));
        }
        None => {}
    }
}

/// Register the panel + bind the dock-scoped keys. Called once from
/// [`crate::init`].
pub(crate) fn init(cx: &mut App) {
    // Resolve the §6.12 login PATH off-thread now, so the first spawn's
    // `build_command` finds the OnceLock already filled instead of running
    // `$SHELL -lic` on the gpui foreground.
    terminal::prewarm_login_path();

    // EXP-301: the rehydration path builds an EMPTY panel — a restored layout
    // never brings terminals back (nothing tab-side is persisted anymore).
    register_panel(cx, PANEL_NAME, |dock_area, _state, _info, window, cx| {
        Box::new(cx.new(|cx| TerminalDockPanel::new(dock_area, window, cx)))
    });

    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-t", NewTerminalTab, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-w", CloseTerminalTab, Some(KEY_CONTEXT)),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-shift-t", NewTerminalTab, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-w", CloseTerminalTab, Some(KEY_CONTEXT)),
    ]);
    cx.bind_keys([
        KeyBinding::new("ctrl-tab", NextTerminalTab, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-tab", PrevTerminalTab, Some(KEY_CONTEXT)),
    ]);
}

/// The bottom-dock terminal panel: one per window, owning that window's
/// [`TerminalManager`] (multi-window = independent tab strips over the same
/// global store, §7.6).
pub struct TerminalDockPanel {
    focus_handle: FocusHandle,
    manager: Entity<TerminalManager>,
    dock_area: WeakEntity<DockArea>,
    /// EXP-325: the promptless agent-shell tabs' P9 token-refresher holds
    /// (tab id → the retained trunk clone), released on `TabClosed`. (A
    /// window closed with live holds leaks its refresh loops until quit —
    /// bounded and rare; sessions normally end by tab close.)
    agent_shell_holds: HashMap<TabId, PathBuf>,
    /// EXP-497: the painted width of the strip's chip slot (the `flex_1`
    /// container the chips + the `+` menu render into), recorded by an
    /// `on_children_prepainted` listener on the strip. Unlike the center
    /// strip, this width is NOT derivable from window chrome — the dock sits
    /// right of the rail and left of whatever tool windows are open — so it
    /// is measured off the real layout instead. `None` until the first paint
    /// (that one frame renders every chip; the partition kicks in as soon as
    /// the width lands).
    chips_slot_width: Option<f32>,
    /// EXP-372: the in-flight empty-state launch (its card label), if any.
    /// `prepare_agent_shell` (doctor → token → clone/autopull → MCP wiring)
    /// takes about a second, and the launch cards stayed live and clickable
    /// that whole time — every extra click spawned another tab. Set
    /// synchronously on click, so the very next paint replaces the cards with
    /// a progress line; cleared when the tab opens or the attempt fails (the
    /// cards come back).
    pending_launch: Option<SharedString>,
    /// EXP-688: the active session tab's branch diff — what the
    /// Latest-changes bar renders. `None` while the dock is collapsed or the
    /// active tab is not a local session.
    changes: Option<ChangesState>,
    /// The expanded bar's side-by-side view (built off [`ChangesState`],
    /// never fetched — the files are already in hand).
    changes_diff: Entity<crate::diff::DiffView>,
    /// EXP-698: the STEER arm's Latest-changes state — the relay-delivered
    /// diff of the active steer chip, parsed once per delivered string.
    steer_changes: Option<SteerChangesState>,
    /// Its own expanded view. Deliberately NOT shared with
    /// [`Self::changes_diff`]: the local poll keeps running while a steer
    /// chip is showing, and a rebuild off the local snapshot would silently
    /// swap the remote run's diff for a local tab's.
    steer_changes_diff: Entity<crate::diff::DiffView>,
    /// The 3s poll behind [`Self::changes`]. Lives as long as the panel; it
    /// only shells out to git while the dock is OPEN on a session tab.
    _changes_poll: Task<()>,
    /// EXP-696: the steering viewers behind the REMOTE chips, keyed by
    /// coding-session id.
    ///
    /// **Lifetime rule**: a viewer is created LAZILY on the first click of
    /// its chip and then kept connected for as long as the chip is displayed
    /// — collapsing the dock or switching to another tab does not drop the
    /// socket (the web dock keeps its stores alive the same way, and a
    /// reconnect would replay the journal only to re-render what the reader
    /// was already looking at). Dialing every live remote session up front,
    /// on the other hand, would open a relay socket per row nobody opened.
    steer_views: HashMap<String, Entity<SteerSessionView>>,
    /// The REMOTE chips the strip paints, CACHED. Building them is a scan of
    /// `coding_sessions` plus issue/device joins and a fistful of `String`s,
    /// and the dock repaints on every viewer frame and composer keystroke —
    /// so it is rebuilt on the row/device/session deltas it depends on and on
    /// the [`CHANGES_POLL`] tick (the staleness clock), never per render.
    /// `Rc` so a render can hold the list while `&mut self` methods run.
    remote_chips: std::rc::Rc<Vec<RemoteChip>>,
    /// The steered session the dock is CURRENTLY showing instead of a
    /// terminal, if any. Runtime-only — never serialized into the layout.
    active_steer: Option<String>,
    /// EXP-523: the open/close slide, `None` at rest. See [`DockSlide`].
    dock_slide: Option<DockSlide>,
    /// Dropping the task cancels the slide — same cancellation semantics as
    /// `Shell::_left_anim_task`.
    _dock_slide_task: Option<Task<()>>,
    _subscription: Subscription,
}

impl TerminalDockPanel {
    /// This window's tab-strip model — §07's Start-coding launcher / actions
    /// panel open their `Claude`/`Action` tabs through it (the §6.13 "same
    /// entry point" rule; resolved per window via `coding_flow`).
    pub(crate) fn manager(&self) -> &Entity<TerminalManager> {
        &self.manager
    }

    /// The only constructor — fresh AND rehydrated panels start with zero
    /// tabs (EXP-301: launching the app must never spawn a terminal).
    pub fn new(
        dock_area: WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let manager = cx.new(|_| TerminalManager::new());

        let subscription = cx.subscribe_in(
            &manager,
            window,
            |this, _, event: &TerminalManagerEvent, window, cx| {
                match event {
                    // §6.13: the panel expands when a tab is created — also
                    // the path Phase 5's play button / remote start rides.
                    TerminalManagerEvent::TabOpened(_) => {
                        // EXP-696: the new tab becomes the VISIBLE content —
                        // the same thing every other activation path does
                        // (chip click, overflow menu, ctrl-tab). Without it a
                        // launch behind an open steer view kept painting the
                        // remote feed while the keyboard went to the new,
                        // invisible PTY.
                        this.active_steer = None;
                        // EXP-703: the chat-run launch has no success
                        // callback of its own — the tab landing IS the
                        // success, so the EXP-372 progress line ends here
                        // (failures end it via the runner's failure hook).
                        this.set_pending_launch(None, cx);
                        this.expand_dock(window, cx);
                        this.focus_active_terminal(window, cx);
                    }
                    TerminalManagerEvent::TabClosed(id) => {
                        // EXP-325: a promptless agent-shell tab releases its
                        // token-refresher hold with the tab.
                        if let Some(clone) = this.agent_shell_holds.remove(id) {
                            TokenRefreshers::release(&clone, cx);
                        }
                        // §8.8b: closing the last tab collapses the bottom dock
                        // (mirror of the TabOpened expand); otherwise focus the
                        // tab that took over.
                        if this.manager.read(cx).is_empty() {
                            this.collapse_dock(window, cx);
                        } else {
                            this.focus_active_terminal(window, cx);
                        }
                    }
                    // Exit strip/badge render on notify; ending the
                    // coding_sessions row is Phase 5's ExitHook (§6.7).
                    TerminalManagerEvent::TabExited { .. } => {}
                }
                cx.notify();
            },
        );

        // Dev hook: EXP_DEV_OPEN_SHELL=1 opens one plain shell tab at startup
        // so the §11.4 terminal-dock smoke (tab strip + rendered prompt +
        // expanded dock) is demonstrable headlessly/in CI without
        // synthesizing a `+` click. Dev-only — never document for users.
        // Runs AFTER the subscription so TabOpened expands the dock.
        //
        // `EXP_DEV_SHELL_CWD` overrides the shell's cwd (EXP-651): the default
        // is `$HOME`, and a shell tab is TITLED after its cwd's directory name
        // — which put the capturing developer's account name in the committed
        // `terminal` shot. The capture lane points it at the same username-free
        // repos root the Tools and Worktrees panes already render verbatim.
        if std::env::var("EXP_DEV_OPEN_SHELL").is_ok_and(|value| value == "1") {
            let shell_override = crate::coding_flow::terminal_shell_override(cx);
            let cwd = std::env::var_os("EXP_DEV_SHELL_CWD")
                .filter(|dir| !dir.is_empty())
                .map(std::path::PathBuf::from);
            manager.update(cx, |manager, cx| {
                if let Err(error) = manager.open_shell(cwd, shell_override, cx) {
                    log::warn!("terminal dock: EXP_DEV_OPEN_SHELL spawn failed: {error:#}");
                }
            });
        }

        // EXP-65: undocked tabs are hidden from the strip — repaint when the
        // undock registry changes (tab popped out / reattached).
        if let Some(undock_state) = crate::undock::state(cx) {
            cx.observe(&undock_state, |_, _, cx| cx.notify()).detach();
        }

        // EXP-325: the issue-styled chips follow the synced issue rows
        // (title/status/pr_state — boards for the status resolution's team
        // lookup), the local session registry, and the shared merge state.
        // (`try_global`: the rehydrate test builds panels without a store.)
        let collections =
            sync::Store::try_global(cx).map(|store| store.collections().clone());
        if let Some(collections) = collections {
            // The remote chips carry issue identifiers/titles, so an issue
            // delta re-projects them (EXP-696).
            cx.observe(&collections.issues, |this: &mut Self, _, cx| {
                this.rebuild_remote_chips(cx);
                cx.notify();
            })
            .detach();
            cx.observe(&collections.issue_statuses, |_, _, cx| cx.notify())
                .detach();
            cx.observe(&collections.boards, |_, _, cx| cx.notify()).detach();
            // EXP-696: the REMOTE session chips are a projection of the
            // synced rows — a row going live, ending or going stale adds or
            // drops a chip (and, on the ended edge, tears its viewer down).
            cx.observe_in(&collections.coding_sessions, window, |this, _, window, cx| {
                let _ = this.reconcile_steer_views(window, cx);
                cx.notify();
            })
            .detach();
            // A host going offline greys its chip (EXP-550) and, since the
            // device rows carry the labels, renames one too.
            cx.observe(&collections.devices, |this: &mut Self, _, cx| {
                this.rebuild_remote_chips(cx);
                cx.notify();
            })
            .detach();
        }
        let local_sessions = crate::coding_flow::LocalSessions::global(cx);
        // A session this process picks up stops being remote (its tab owns it).
        cx.observe(&local_sessions, |this: &mut Self, _, cx| {
            this.rebuild_remote_chips(cx);
            cx.notify();
        })
        .detach();
        let merge_state = crate::pr_merge::MergeState::global(cx);
        cx.observe(&merge_state, |_, _, cx| cx.notify()).detach();

        // EXP-369: the empty state's cards mirror the `+` menu's availability
        // (installed agents from the doctor, board-backed repos from the
        // window resolver) — both land asynchronously, so repaint when they
        // do. Guarded like the collections above: the rehydrate test builds a
        // panel in a bare app with neither store nor nav registry.
        if sync::Store::try_global(cx).is_some() {
            let hub = CodingHub::global(cx);
            cx.observe(&hub, |_, _, cx| cx.notify()).detach();
            let resolver = repo_resolver_for_window(window, cx);
            cx.observe(&resolver, |_, _, cx| cx.notify()).detach();
        }

        // EXP-688: the Latest-changes poll. One timer for the panel's life —
        // it resolves the active session tab itself and does no git work at
        // all while the dock is collapsed.
        //
        // EXP-696: it is also the steer reconcile's CLOCK. A chip's liveness
        // is a staleness window on the row (`coding_session_is_live`), so a
        // host that dies without writing a final row goes stale by TIME and
        // no `coding_sessions` delta ever arrives — the observer-only
        // reconcile then left `active_steer` pointing at a chip that is no
        // longer painted (cmd-w dead, the orphaned viewer redialing the relay
        // forever). Cheap enough for the idle beat: it is a projection of
        // rows already in memory, and it repaints only when it changed.
        let changes_poll = cx.spawn_in(window, async move |this, window| loop {
            let Ok(job) = this.update_in(window, |this, window, cx| {
                if this.reconcile_steer_views(window, cx) {
                    cx.notify();
                }
                this.changes_job(cx)
            }) else {
                return; // panel gone with its window
            };
            let beat = match job {
                ChangesJob::Idle => CHANGES_IDLE_POLL,
                ChangesJob::Poll {
                    tab,
                    worktree,
                    base_ref,
                } => {
                    let files = window
                        .background_executor()
                        .spawn(async move {
                            coding::scm::branch_diff(&worktree, base_ref.as_deref()).ok()
                        })
                        .await;
                    if this
                        .update_in(window, |this, _, cx| this.apply_changes(tab, files, cx))
                        .is_err()
                    {
                        return;
                    }
                    CHANGES_POLL
                }
            };
            window.background_executor().timer(beat).await;
        });

        Self {
            focus_handle: cx.focus_handle(),
            manager,
            dock_area,
            agent_shell_holds: HashMap::new(),
            chips_slot_width: None,
            pending_launch: None,
            changes: None,
            changes_diff: cx.new(|cx| crate::diff::DiffView::new(window, cx)),
            steer_changes: None,
            steer_changes_diff: cx.new(|cx| crate::diff::DiffView::new(window, cx)),
            _changes_poll: changes_poll,
            steer_views: HashMap::new(),
            remote_chips: std::rc::Rc::new(Vec::new()),
            active_steer: None,
            dock_slide: None,
            _dock_slide_task: None,
            _subscription: subscription,
        }
    }

    /// Whether the bottom dock is collapsed to its 29px strip. A chrome-less
    /// `DockItem::Panel` keeps rendering its full content inside that strip
    /// (the Dock only shrinks the container), so the panel must render the
    /// compact strip itself when collapsed ("bottom bar cut off").
    fn dock_collapsed(&self, cx: &App) -> bool {
        self.dock_area
            .upgrade()
            .and_then(|dock_area| dock_area.read(cx).bottom_dock().cloned())
            .is_some_and(|dock| !dock.read(cx).is_open())
    }

    /// This window's bottom `Dock`, if it still exists.
    fn bottom_dock(&self, cx: &App) -> Option<Entity<gpui_component::dock::Dock>> {
        self.dock_area
            .upgrade()
            .and_then(|dock_area| dock_area.read(cx).bottom_dock().cloned())
    }

    /// Open the bottom dock if it is collapsed (§4 dock open/close), sliding
    /// it up (EXP-523).
    fn expand_dock(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(dock) = self.bottom_dock(cx) else {
            return;
        };
        let now = Instant::now();

        // Reversing a close: the dock never actually shut, so just retarget —
        // no `set_open`, no restart from the strip height.
        if let Some(slide) = self.dock_slide.as_mut() {
            if !slide.opening {
                let rest = slide.rest_height;
                let epoch = slide.retarget(now, rest, true);
                self.spawn_slide_task(epoch, window, cx);
            }
            return;
        }
        if dock.read(cx).is_open() {
            return;
        }

        // Drop to the strip height and open in ONE update, so the first frame
        // is already the closed height — never a full-height flash.
        let rest = f32::from(dock.read(cx).size()).max(DOCK_STRIP_H);
        let applied = dock.update(cx, |dock, cx| {
            dock.set_size(px(DOCK_STRIP_H), window, cx);
            dock.set_open(true, window, cx);
            f32::from(dock.size())
        });
        let mut slide = DockSlide::new(DOCK_STRIP_H, rest, rest, true, now);
        slide.record_apply(DOCK_STRIP_H, applied);
        self.dock_slide = Some(slide);
        let epoch = 0;
        self.spawn_slide_task(epoch, window, cx);
        cx.notify();
    }

    /// Collapse the bottom dock if it is open (§8.8b: the last tab closed) —
    /// the Dock keeps its 29px toggle strip so the user can re-open it. The
    /// dock slides down first; `set_open(false)` lands only on settle
    /// (EXP-523), which is what keeps the content rendered for the whole
    /// animation instead of swapping to the strip on frame 1.
    fn collapse_dock(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(dock) = self.bottom_dock(cx) else {
            return;
        };
        let now = Instant::now();

        if let Some(slide) = self.dock_slide.as_mut() {
            if slide.opening {
                let epoch = slide.retarget(now, DOCK_STRIP_H, false);
                self.spawn_slide_task(epoch, window, cx);
            }
            return;
        }
        if !dock.read(cx).is_open() {
            return;
        }

        let rest = f32::from(dock.read(cx).size());
        let mut slide = DockSlide::new(rest, DOCK_STRIP_H, rest, false, now);
        slide.record_apply(rest, rest);
        self.dock_slide = Some(slide);
        self.spawn_slide_task(0, window, cx);
        cx.notify();
    }

    /// Drive the slide from a timer loop rather than from `render`.
    ///
    /// The animated value has to be PUSHED into a foreign entity (`Dock`), so
    /// `gpui::Animation` / `EffectTransition` are out — their delta is only
    /// available inside an element closure with no `&mut App`. And
    /// `window.request_animation_frame()` is `on_next_frame(notify)`, so a
    /// write driven from `render` would land a frame late and make `render`
    /// impure. This is the same shape as the terminal's cursor-blink task and
    /// `Shell::sync_left_column`'s settle timer.
    fn spawn_slide_task(
        &mut self,
        epoch: u64,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self._dock_slide_task = Some(cx.spawn_in(window, async move |this, window| {
            loop {
                window
                    .background_executor()
                    .timer(DOCK_SLIDE_FRAME)
                    .await;
                let Ok(done) = this.update_in(window, |this, window, cx| {
                    this.tick_slide(epoch, window, cx)
                }) else {
                    return; // panel gone with its window
                };
                if done {
                    return;
                }
            }
        }));
    }

    /// One slide frame. Returns true when this leg is finished (settled, or
    /// abandoned because something else took over the height).
    fn tick_slide(
        &mut self,
        epoch: u64,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(slide) = self.dock_slide else {
            return true;
        };
        // A retarget spawned a newer loop; this one is stale.
        if slide.epoch != epoch {
            return true;
        }
        let Some(dock) = self.bottom_dock(cx) else {
            self.dock_slide = None;
            return true;
        };

        // Upstream's `resizing` flag is private, so detect the collision
        // instead: if the dock's height is not what our last `set_size`
        // stored, the user is dragging the resize handle (or some future
        // upstream writer owns it). Abandon the slide and keep THEIR value.
        if (f32::from(dock.read(cx).size()) - slide.applied).abs() > 0.5 {
            self.dock_slide = None;
            cx.notify();
            return true;
        }

        let now = Instant::now();
        if slide.done(now) {
            self.settle_slide(&dock, window, cx);
            return true;
        }

        let requested = slide.virtual_height(now);
        let applied = dock.update(cx, |dock, cx| {
            dock.set_size(px(requested), window, cx);
            f32::from(dock.size())
        });
        if let Some(slide) = self.dock_slide.as_mut() {
            slide.record_apply(requested, applied);
        }
        cx.notify();
        false
    }

    /// Place the dock's content in the band ABOVE the always-present bottom
    /// strip (EXP-688).
    ///
    /// At rest that is simply `top_0 .. bottom(DOCK_STRIP_H)`. Mid-slide the
    /// content keeps its full RESTING height and is bottom-anchored inside a
    /// clip sized to [`DockSlide::content_clip_height`]: the terminal grid's
    /// bounds never move (so the PTY reshapes at most once per open,
    /// EXP-523), and the band the clip exposes above itself is the same
    /// `theme::background_gradient()` quad the center already paints, so it
    /// reads as the center growing rather than as a hole.
    fn pin_content<E: Styled + IntoElement>(&self, content: E) -> AnyElement {
        match self.dock_slide {
            Some(slide) => div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(DOCK_STRIP_H))
                .h(px(slide.content_clip_height()))
                .overflow_hidden()
                .child(
                    content
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(px((slide.rest_height - DOCK_STRIP_H).max(0.))),
                )
                .into_any_element(),
            None => content
                .absolute()
                .left_0()
                .right_0()
                .top_0()
                .bottom(px(DOCK_STRIP_H))
                .into_any_element(),
        }
    }

    /// End of the slide: land the real open state and restore the user's
    /// height. The height restore is what stops a close from persisting a
    /// clamped intermediate (the layout save reads `Dock::size`), and it is
    /// invisible either way — a closed dock renders its 29px strip whatever
    /// its stored size is.
    fn settle_slide(
        &mut self,
        dock: &Entity<gpui_component::dock::Dock>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(slide) = self.dock_slide.take() else {
            return;
        };
        dock.update(cx, |dock, cx| {
            if !slide.opening {
                dock.set_open(false, window, cx);
            }
            dock.set_size(px(slide.rest_height), window, cx);
        });
        cx.notify();
    }

    /// Focus follows the active tab (§6.13 "each tab hosting the terminal
    /// element focused"). Undocked tabs render in their own window — never
    /// steal this window's focus for them (EXP-65).
    fn focus_active_terminal(&self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if let Some(tab) = self.manager.read(cx).active_tab() {
            if crate::undock::is_terminal_tab_undocked(tab.id, cx) {
                return;
            }
            let handle = tab.view.focus_handle(cx);
            window.focus(&handle, cx);
        }
    }

    /// Focus what the dock is SHOWING (EXP-696): the steered composer when a
    /// steer view owns the content area, else the active terminal. Focusing
    /// the terminal while a steer view is painted typed into a PTY nobody
    /// could see — the expand paths take this instead of the raw terminal
    /// focus. An empty, tab-less dock focuses nothing (EXP-369: expanding
    /// never starts anything).
    fn focus_visible_content(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if let Some(view) = self
            .active_steer
            .as_deref()
            .and_then(|id| self.steer_views.get(id))
            .cloned()
        {
            view.update(cx, |view, cx| view.focus_composer(window, cx));
            return;
        }
        if !self.manager.read(cx).is_empty() {
            self.focus_active_terminal(window, cx);
        }
    }

    /// Manager indices of the tabs the dock still shows (EXP-65: undocked
    /// tabs render in their own windows and are hidden here).
    fn visible_indices(&self, cx: &App) -> Vec<usize> {
        self.manager
            .read(cx)
            .tabs()
            .iter()
            .enumerate()
            .filter(|(_, tab)| !crate::undock::is_terminal_tab_undocked(tab.id, cx))
            .map(|(ix, _)| ix)
            .collect()
    }

    /// Ctrl-tab / ctrl-shift-tab step over VISIBLE tabs only (an undocked
    /// tab must not flash through the dock while cycling).
    /// EXP-696: ctrl-tab cycles the WHOLE strip — the local terminal tabs
    /// first (manager order), then the remote steer chips (newest run first),
    /// exactly as they are painted.
    fn activate_visible_step(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let visible = self.visible_indices(cx);
        // The cached projection — cycling needs the ids, not the chrome.
        let remote: Vec<String> = self
            .remote_chips
            .iter()
            .map(|chip| chip.session_id.clone())
            .collect();
        let len = visible.len() + remote.len();
        if len == 0 {
            return;
        }
        let current_pos = match self.active_steer.as_deref() {
            Some(active) => remote
                .iter()
                .position(|id| id == active)
                .map(|pos| visible.len() + pos),
            None => self
                .manager
                .read(cx)
                .active_index()
                .and_then(|active| visible.iter().position(|ix| *ix == active)),
        };
        let next_pos = match current_pos {
            Some(pos) if forward => (pos + 1) % len,
            Some(pos) => (pos + len - 1) % len,
            None => 0,
        };
        match next_pos.checked_sub(visible.len()) {
            Some(remote_pos) => {
                let session_id = remote[remote_pos].clone();
                self.activate_steer(&session_id, window, cx);
            }
            None => {
                self.active_steer = None;
                self.activate_tab(visible[next_pos], window, cx);
            }
        }
    }

    /// Make the manager's `manager_ix`th tab the active one and focus its
    /// terminal — the one path every activation takes (chip click, the
    /// overflow menu, ctrl-tab).
    fn activate_tab(
        &mut self,
        manager_ix: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.manager
            .update(cx, |manager, cx| manager.activate(manager_ix, cx));
        self.focus_active_terminal(window, cx);
    }

    // ── EXP-696: remote (steered) session chips ────────────────────────────

    /// The user's OTHER live coding sessions, as strip chips. Empty when the
    /// instance runs no steer relay: without one there is nothing to view,
    /// and a chip that opens a dead feed is worse than no chip (the
    /// `remote_start_enabled` rule, §8.2).
    fn build_remote_chips(cx: &mut App) -> Vec<RemoteChip> {
        if !crate::queries::remote_start_enabled(cx) {
            return Vec::new();
        }
        let own_device_id = crate::queries::own_device_id(cx);
        let Some(me) = crate::queries::active_account(cx).map(|account| account.user_id) else {
            return Vec::new();
        };
        let Some(store) = sync::Store::try_global(cx) else {
            return Vec::new();
        };
        // Belt and braces: a session this process hosts already has a tab.
        let local: HashSet<String> = crate::coding_flow::LocalSessions::global_ref(cx)
            .map(|sessions| sessions.read(cx).session_ids().into_iter().collect())
            .unwrap_or_default();
        let collections = store.collections().clone();
        let now = chrono::Utc::now().timestamp();
        let sessions = collections.coding_sessions.read(cx);
        let rows = remote_session_rows(sessions.iter(), &me, &own_device_id, &local, now);
        if rows.is_empty() {
            return Vec::new();
        }
        let issues = collections.issues.read(cx);
        let devices = collections.devices.read(cx);
        rows.into_iter()
            .map(|session| {
                let issue = session
                    .issue_id
                    .as_deref()
                    .and_then(|issue_id| issues.get(issue_id));
                let presentation = crate::queries::session_device_presentation(
                    session,
                    devices.iter(),
                    now * 1_000,
                );
                let display = crate::queries::coding_session_display(
                    session,
                    issue.and_then(|issue| issue.pr_state.as_deref()),
                );
                let paused = crate::queries::session_is_paused(display, &presentation);
                RemoteChip {
                    session_id: session.id.clone(),
                    identifier: issue.map(|issue| SharedString::from(issue.identifier.clone())),
                    title: remote_chip_title(session, issue),
                    device: presentation.label.map(SharedString::from),
                    display,
                    paused,
                    // Web `ownsLiveRow`: the row is the caller's and still
                    // live, and a paused host is never killed (it resumes).
                    killable: !paused,
                    measured: std::cell::Cell::new(None),
                }
            })
            .collect()
    }

    /// Re-project [`Self::remote_chips`]; `true` when the strip actually
    /// changed (the callers repaint on that alone — an unchanged rebuild also
    /// KEEPS the chips' measured widths, which is the point of caching them
    /// on the chip).
    fn rebuild_remote_chips(&mut self, cx: &mut App) -> bool {
        let chips = Self::build_remote_chips(cx);
        if *self.remote_chips == chips {
            return false;
        }
        self.remote_chips = std::rc::Rc::new(chips);
        true
    }

    /// Show `session_id`'s steering view as the dock's content, dialing the
    /// relay on the first activation. Also the entry point the issue-detail
    /// "coding now" pill rides.
    pub(crate) fn activate_steer(
        &mut self,
        session_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.steer_views.contains_key(session_id) {
            let id = session_id.to_string();
            let view = cx.new(|cx| SteerSessionView::new(id.clone(), window, cx));
            // No `cx.observe(&view, notify)`: the dock's own chrome reads
            // nothing off the viewer (the chips are a projection of the
            // synced rows), and gpui already marks a notifying view's
            // ANCESTORS dirty (`Window::mark_view_dirty` walks the view
            // path), which is what invalidates the Dock's cached panel
            // element. The observer only added a second repaint per feed
            // frame and per composer keystroke.
            self.steer_views.insert(id, view);
        }
        self.active_steer = Some(session_id.to_string());
        self.expand_dock(window, cx);
        // Without this the hidden terminal grid keeps the keyboard.
        if let Some(view) = self.steer_views.get(session_id).cloned() {
            view.update(cx, |view, cx| view.focus_composer(window, cx));
        }
        cx.notify();
    }

    /// Re-project the chips and drop the viewers whose chip is gone (the row
    /// ended, went stale, or the account changed). A session that ends while
    /// it is NOT the active content just disappears; the active one falls
    /// back to the terminal side — or collapses the dock when nothing is
    /// left, the mirror of the `TabClosed` behavior.
    ///
    /// Runs on the `coding_sessions` observer AND on the poll clock: chip
    /// liveness is a staleness WINDOW, so a host that dies without writing a
    /// final row produces no delta at all. Returns whether the chips changed.
    fn reconcile_steer_views(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let changed = self.rebuild_remote_chips(cx);
        if self.steer_views.is_empty() {
            return changed;
        }
        let live: HashSet<&str> = self
            .remote_chips
            .iter()
            .map(|chip| chip.session_id.as_str())
            .collect();
        let gone: Vec<String> = self
            .steer_views
            .keys()
            .filter(|id| !live.contains(id.as_str()))
            .cloned()
            .collect();
        let mut fell_back = false;
        for id in gone {
            if let Some(view) = self.steer_views.remove(&id) {
                view.update(cx, |view, _| view.shutdown());
            }
            if self.active_steer.as_deref() == Some(id.as_str()) {
                self.active_steer = None;
                fell_back = true;
            }
        }
        if fell_back {
            if self.manager.read(cx).is_empty() && self.steer_views.is_empty() {
                self.collapse_dock(window, cx);
            } else {
                self.focus_active_terminal(window, cx);
            }
            return true;
        }
        changed
    }

    /// Pop the tab out into its own native window (EXP-65). The tab stays in
    /// the manager (exit hooks / stop button / persistence untouched) — the
    /// registry hides it here and the new window renders its view. If it was
    /// the active tab, activate the nearest still-visible neighbor first so
    /// the dock never points at a hidden tab.
    fn undock_tab(&mut self, id: TabId, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let neighbor = {
            let manager = self.manager.read(cx);
            if manager.active_tab().map(|tab| tab.id) == Some(id) {
                let current = manager.active_index().unwrap_or(0);
                let visible: Vec<usize> = manager
                    .tabs()
                    .iter()
                    .enumerate()
                    .filter(|(_, tab)| {
                        tab.id != id && !crate::undock::is_terminal_tab_undocked(tab.id, cx)
                    })
                    .map(|(ix, _)| ix)
                    .collect();
                visible
                    .iter()
                    .copied()
                    .find(|ix| *ix > current)
                    .or_else(|| visible.last().copied())
            } else {
                None
            }
        };
        if let Some(ix) = neighbor {
            self.manager.update(cx, |manager, cx| manager.activate(ix, cx));
        }
        crate::undock::open_undocked_terminal_tab(
            self.manager.clone(),
            id,
            window.window_handle(),
            cx,
        );
        self.focus_active_terminal(window, cx);
        cx.notify();
    }

    /// The `+` shell tab (v4 §4.6): cwd = the **trunk** clone root of this
    /// window's active board; `$HOME` only off a board screen or while the
    /// clone doesn't exist yet. The repo→trunk-root resolution needs a
    /// (tRPC-only, never synced) `repositories.list` lookup, so the resolve
    /// runs off the foreground and the tab opens once the cwd is known; a
    /// non-board screen (or missing session/board) opens at `$HOME`
    /// immediately (`open_shell(None)`).
    fn new_shell_tab(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        // EXP-372: hide the empty-state cards for the whole resolve — even the
        // cwd lookup is a frame or two of clickable window.
        self.set_pending_launch(Some("New shell".into()), cx);
        let Some((resolver, board_id, settings)) = self.shell_scope(window, cx) else {
            self.open_shell_cwd(None, cx);
            return;
        };
        // The repo comes from the shared window resolver (the run/git bars keep
        // it warm); a still-loading / unlinked repo just opens at `$HOME`.
        resolver.update(cx, |resolver, cx| resolver.ensure_loaded(cx));
        let full_name = match resolver.read(cx).lookup_board(&board_id) {
            RepoLookup::Found(repo) => repo.full_name,
            _ => {
                self.open_shell_cwd(None, cx);
                return;
            }
        };
        cx.spawn(async move |this, cx| {
            let cwd = cx
                .background_executor()
                .spawn(async move {
                    let root = coding::clone_path(&settings.repos_root_path(), &full_name);
                    // `$HOME` (None) until the clone actually exists on disk.
                    coding::shell_cwd(Some(root))
                })
                .await;
            let _ = this.update(cx, |this, cx| this.open_shell_cwd(cwd, cx));
        })
        .detach();
    }

    /// Spawn a shell tab at `cwd` (`None` → `$HOME`, resolved by the manager).
    fn open_shell_cwd(&mut self, cwd: Option<PathBuf>, cx: &mut gpui::Context<Self>) {
        // The launch is over either way — a spawn failure must put the empty
        // state's cards back rather than leave a stuck progress line.
        self.set_pending_launch(None, cx);
        let shell_override = crate::coding_flow::terminal_shell_override(cx);
        let result = self
            .manager
            .update(cx, |manager, cx| manager.open_shell(cwd, shell_override, cx));
        if let Err(error) = result {
            log::error!("terminal dock: shell spawn failed: {error:#}");
        }
    }

    /// EXP-372: flip the in-flight-launch state and repaint. Only the empty
    /// state reads it (a dock with tabs renders its terminals regardless), so
    /// a launch started from the `+` menu / cmd-t / the file tree just sets and
    /// clears it unobserved.
    fn set_pending_launch(&mut self, label: Option<SharedString>, cx: &mut gpui::Context<Self>) {
        if self.pending_launch != label {
            self.pending_launch = label;
            cx.notify();
        }
    }

    /// The sync-resolvable inputs for the `+` shell cwd: the shared window repo
    /// resolver, the window's active board (screen scope with the
    /// last-board fallback), and the coding settings (repos root). `None`
    /// with no resolvable board — the caller then opens the shell at
    /// `$HOME`.
    fn shell_scope(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<(Entity<RepoResolver>, String, coding::Settings)> {
        let nav = navigation::nav_for_window(window, cx);
        let board_id = navigation::active_board_id(&nav, cx)?;
        let resolver = repo_resolver_for_window(window, cx);
        let settings = CodingHub::global(cx).read(cx).settings.clone();
        Some((resolver, board_id, settings))
    }

    fn on_new_tab(&mut self, _: &NewTerminalTab, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.new_shell_tab(window, cx);
    }

    /// EXP-696: cmd-w on a STEER chip is a deliberate NO-OP. A remote chip
    /// lives as long as its synced row does — there is no local tab to close,
    /// and the only thing "closing" it could mean is killing someone's
    /// running agent, which must never happen without the chip's own
    /// confirmed X. (Local tabs close exactly as before.)
    fn on_close_tab(
        &mut self,
        _: &CloseTerminalTab,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.active_steer.is_some() {
            return;
        }
        self.manager.update(cx, |manager, cx| manager.close_active(cx));
    }

    fn on_next_tab(
        &mut self,
        _: &NextTerminalTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.activate_visible_step(true, window, cx);
    }

    fn on_prev_tab(
        &mut self,
        _: &PrevTerminalTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.activate_visible_step(false, window, cx);
    }

    /// ONE bottom strip, open or collapsed (EXP-688): a fixed
    /// [`DOCK_STRIP_H`] glass band pinned to the panel's bottom edge, so the
    /// tabs sit where the web dock's do and expanding grows the content
    /// UPWARD out of them instead of pushing them down.
    ///
    /// Leading terminal glyph (plus the word "Terminal" only when there are
    /// no tabs to name it), then one chip per VISIBLE session (EXP-65:
    /// undocked tabs render in their own windows) with the `+` right after
    /// the last one, then the right cluster: "Open in new window" for the
    /// ACTIVE tab and the open/close chevron. Clicking the strip's empty
    /// space toggles the dock; a chip click activates its tab (and expands a
    /// collapsed dock) — chip/button handlers stop propagation so their
    /// clicks never fall through to the toggle.
    ///
    /// EXP-497: chips that don't fit collapse into a trailing "+N" dropdown —
    /// the center strip's EXP-288 treatment (the scrolled chips this replaces
    /// left overflowing tabs cut off). Chip widths are measured, not guessed
    /// (the EXP-326 lesson), against the recorded [`Self::chips_slot_width`];
    /// the SELECTED tab is always kept visible.
    ///
    /// EXP-696: the strip additionally lists the user's LIVE sessions hosted
    /// on other machines, after the local tabs. Clicking one of those swaps
    /// the dock's content for its steering view instead of a terminal grid.
    fn render_strip(
        &self,
        metas: &[TabMeta],
        remote: &[RemoteChip],
        selected_ix: usize,
        collapsed: bool,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        // EXP-277: hand-rolled rounded chips (crate::surface::rich_tab), same
        // treatment as the center tab strip — gpui-component's TabBar is
        // square with a strip-wide bottom border.
        //
        // EXP-688: the chip has no hover-revealed undock any more — the
        // right cluster's "Open in new window" undocks the ACTIVE tab, which
        // is one affordance instead of one per chip and buys every chip back
        // 20px of title.

        // EXP-497: partition the chips against the slot's painted width. The
        // `+` new-session menu rides INSIDE the slot right after the chips —
        // an xsmall icon button (`size_5`) plus one gap comes off the budget.
        let entries: Vec<StripEntry<'_>> = metas
            .iter()
            .map(StripEntry::Local)
            .chain(remote.iter().map(StripEntry::Remote))
            .collect();
        let widths: Vec<f32> = entries
            .iter()
            .map(|entry| match entry {
                StripEntry::Local(meta) => measure_tab_chip_width(meta, window),
                StripEntry::Remote(chip) => measure_remote_chip_width(chip, window),
            })
            .collect();
        let plus_reserve = 1.25 * f32::from(window.rem_size()) + crate::screens::chip_gap(window);
        let available = self
            .chips_slot_width
            .map_or(f32::MAX, |slot| (slot - plus_reserve).max(0.));
        let visible = crate::screens::partition_tabs(
            &widths,
            available,
            crate::screens::chip_gap(window),
            crate::screens::overflow_button_width(window, entries.len().saturating_sub(1)),
            (!entries.is_empty()).then_some(selected_ix),
        );
        let hidden: Vec<usize> = (0..entries.len())
            .filter(|ix| !visible.contains(ix))
            .collect();

        let chips: Vec<AnyElement> = visible
            .into_iter()
            .map(|ix| match &entries[ix] {
                StripEntry::Local(meta) => {
                    self.render_local_chip(meta, ix, selected_ix, collapsed, cx)
                }
                StripEntry::Remote(chip) => {
                    self.render_remote_chip(chip, ix, selected_ix, cx)
                }
            })
            .collect();
        // EXP-497: the hidden tabs collapse into a "+N" dropdown; clicking
        // one activates it. Keyed by TabId, not strip index — the menu's
        // closures run at click time, and a tab closed while the dropdown is
        // open shifts every index after it (the center strip's EXP-288
        // rationale; the TabId is the stable identity here).
        let overflow_button = (!hidden.is_empty()).then(|| {
            /// A hidden strip entry: a local tab (by id) or a remote session
            /// (by coding-session id), plus the row's glyph and label.
            enum HiddenEntry {
                Local(TabId, Option<domain::statuses::ResolvedStatus>, SharedString),
                Remote(String, SharedString),
            }
            let hidden_entries: Vec<HiddenEntry> = hidden
                .iter()
                .map(|&ix| match &entries[ix] {
                    // The menu rows mirror the chips: issue sessions carry
                    // the status glyph + "IDENT title", the rest their plain
                    // terminal title.
                    StripEntry::Local(meta) => match &meta.issue {
                        Some(issue) => {
                            let label = match &issue.title {
                                Some(title) => SharedString::from(format!(
                                    "{} {title}",
                                    issue.identifier
                                )),
                                None => issue.identifier.clone(),
                            };
                            HiddenEntry::Local(meta.id, Some(issue.status.clone()), label)
                        }
                        None => HiddenEntry::Local(meta.id, None, meta.title.clone()),
                    },
                    StripEntry::Remote(chip) => {
                        let label = match chip.identifier.as_ref() {
                            Some(identifier) => {
                                SharedString::from(format!("{identifier} {}", chip.title))
                            }
                            None => chip.title.clone(),
                        };
                        HiddenEntry::Remote(chip.session_id.clone(), label)
                    }
                })
                .collect();
            let panel = cx.entity().downgrade();
            Button::new("terminal-tab-overflow")
                .ghost().cursor_pointer()
                .xsmall()
                .label(format!("+{}", hidden_entries.len()))
                .tooltip("More tabs")
                .dropdown_menu(move |mut menu, _window, cx| {
                    menu = menu.scrollable(true).max_h(px(320.));
                    for entry in &hidden_entries {
                        let panel = panel.clone();
                        match entry {
                            HiddenEntry::Local(id, status, label) => {
                                let id = *id;
                                let mut item = PopupMenuItem::new(label.clone());
                                if let Some(status) = status {
                                    item =
                                        item.icon(crate::icons::resolved_status_icon(status, cx));
                                }
                                menu = menu.item(item.on_click(move |_, window, cx| {
                                    let _ = panel.update(cx, |this, cx| {
                                        let Some(ix) = this
                                            .manager
                                            .read(cx)
                                            .tabs()
                                            .iter()
                                            .position(|tab| tab.id == id)
                                        else {
                                            return;
                                        };
                                        this.active_steer = None;
                                        this.activate_tab(ix, window, cx);
                                    });
                                }));
                            }
                            HiddenEntry::Remote(session_id, label) => {
                                let session_id = session_id.clone();
                                let item = PopupMenuItem::new(label.clone())
                                    .icon(Icon::new(registry::UI_DEVICE));
                                menu = menu.item(item.on_click(move |_, window, cx| {
                                    let session_id = session_id.clone();
                                    let _ = panel.update(cx, |this, cx| {
                                        this.activate_steer(&session_id, window, cx);
                                    });
                                }));
                            }
                        }
                    }
                    menu
                })
        });

        // Whether the dock is (or is becoming) OPEN: mid-slide the chevron
        // must already point where the animation is going, not where the
        // Dock's `is_open` still says it is.
        let showing = match self.dock_slide {
            Some(slide) => slide.opening,
            None => !collapsed,
        };
        // "Open in new window" undocks a LOCAL tab; a steered session has no
        // terminal grid to pop out, so the button disables on a steer chip.
        let active_tab = match entries.get(selected_ix) {
            Some(StripEntry::Local(meta)) => Some(meta.id),
            _ => None,
        };

        // Clicking the strip's empty space toggles the dock — the whole
        // strip is the toggle (chip/button handlers stop propagation).
        h_flex()
            // EXP-497: record the chip slot's painted width (the `flex_1`
            // child below, a pure-stretch flex item the chips cannot
            // inflate) so the partition above budgets against the real
            // layout. It is child ONE — the leading glyph is child zero.
            // Change-gated: only a real width change repaints. (On the bare
            // `Div` — the method is not exposed on `Stateful`, so it rides
            // ahead of `.id()`.)
            .on_children_prepainted({
                let panel = cx.entity().downgrade();
                move |bounds: Vec<Bounds<Pixels>>, _window, cx| {
                    let Some(slot) = bounds.get(1) else {
                        return;
                    };
                    let width = f32::from(slot.size.width);
                    let _ = panel.update(cx, |this, cx| {
                        if this
                            .chips_slot_width
                            .is_none_or(|prev| (prev - width).abs() > 0.5)
                        {
                            this.chips_slot_width = Some(width);
                            cx.notify();
                        }
                    });
                }
            })
            .id("terminal-tab-strip")
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .h(px(DOCK_STRIP_H))
            .px_2()
            .gap_1()
            .items_center()
            .flex_shrink_0()
            .border_t_1()
            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
            .bg(theme::tokens::glass::FILL_CARD.to_hsla())
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                if showing {
                    this.collapse_dock(window, cx);
                } else {
                    this.expand_dock(window, cx);
                    // EXP-369: expanding NEVER starts anything — with zero
                    // sessions the dock opens on its launch cards; with
                    // sessions the visible content takes focus back (EXP-696:
                    // an open steer view keeps it, and keeps the keyboard).
                    this.focus_visible_content(window, cx);
                }
            }))
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_1p5()
                    .items_center()
                    .text_color(cx.theme().muted_foreground)
                    .child(Icon::new(registry::NAV_TERMINAL).xsmall())
                    // The word only when no chip names the dock.
                    .when(entries.is_empty(), |this| {
                        this.child(div().text_xs().child("Terminal"))
                    }),
            )
            .child(
                // EXP-497: chips never scroll — non-fitting tabs fold into
                // the "+N" dropdown (partition above). `overflow_x_hidden`
                // covers the one unmeasured first frame, which renders every
                // chip.
                h_flex()
                    .id("terminal-tab-chips")
                    .min_w_0()
                    .flex_1()
                    .overflow_x_hidden()
                    .gap_1()
                    .items_center()
                    .children(chips)
                    .when_some(overflow_button, |this, button| this.child(button))
                    // The `+` rides the slot right AFTER the last tab
                    // (JetBrains placement), not the far-right suffix.
                    // EXP-325: a dropdown — the doctor-installed agent CLIs
                    // (immediate empty session on the current board's trunk
                    // repo; a repo submenu when the team has several) plus
                    // the plain shell (cmd-t unchanged).
                    .child(self.new_tab_menu(cx)),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_0p5()
                    .items_center()
                    // EXP-688: undock is one strip button on the ACTIVE tab
                    // (it used to be a hover affordance on every chip).
                    .child(
                        Button::new("undock-active-terminal-tab")
                            .ghost().cursor_pointer()
                            .xsmall()
                            .icon(ExpIcon::ExternalLink)
                            .tooltip("Open in new window")
                            .disabled(active_tab.is_none())
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                if let Some(id) = active_tab {
                                    this.undock_tab(id, window, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("toggle-terminal-dock")
                            .ghost().cursor_pointer()
                            .xsmall()
                            .icon(if showing {
                                registry::UI_CHEVRON_DOWN
                            } else {
                                registry::UI_CHEVRON_UP
                            })
                            .tooltip(if showing {
                                "Hide terminal"
                            } else {
                                "Show terminal"
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                if showing {
                                    this.collapse_dock(window, cx);
                                } else {
                                    this.expand_dock(window, cx);
                                    this.focus_visible_content(window, cx);
                                }
                            })),
                    ),
            )
    }

    /// One LOCAL terminal tab's chip (EXP-325/EXP-497) — extracted from
    /// `render_strip` when the strip grew its second chip KIND (EXP-696).
    fn render_local_chip(
        &self,
        meta: &TabMeta,
        ix: usize,
        selected_ix: usize,
        collapsed: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let id = meta.id;
        let manager_ix = meta.manager_ix;
        let mut tab = crate::surface::RichTab::new(("terminal-tab", ix), ix == selected_ix);
        // EXP-325: an issue-session tab renders the center issue-tab
        // treatment (status glyph + mono identifier + synced title,
        // mirroring `screens::render_tab_strip`); everything else keeps
        // the plain terminal title.
        match &meta.issue {
            Some(issue) => {
                tab.status = crate::surface::RichTabStatus::Glyph(
                    crate::icons::resolved_status_icon(&issue.status, cx),
                );
                tab.identifier = Some(issue.identifier.clone());
                tab.title = issue.title.clone();
            }
            None => tab.title = Some(meta.title.clone()),
        }
        tab.badge = meta.exit_code.map(|code| {
            let color = if code == 0 {
                cx.theme().success
            } else {
                cx.theme().danger
            };
            (SharedString::from(code.to_string()), color)
        });
        let chip = crate::surface::rich_tab(tab, cx)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                // EXP-688: from the collapsed strip a chip click is also
                // "open the dock" — the tab it names has to become
                // visible, not just active.
                if collapsed {
                    this.expand_dock(window, cx);
                }
                // EXP-696: a local chip always returns the dock to the
                // terminal side.
                this.active_steer = None;
                this.activate_tab(manager_ix, window, cx);
            }))
            // Middle-click closes (EXP-497 — the center tabs' EXP-235
            // behavior; same as the chip's own close button, so the
            // TabClosed watcher handles focus/collapse).
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _, _window, cx| {
                    cx.stop_propagation();
                    this.manager
                        .update(cx, |manager, cx| manager.close_tab(id, cx));
                }),
            );
        chip.child(
            h_flex()
                .gap_0p5()
                .items_center()
                .child(
                    Button::new(("close-terminal-tab", ix))
                        .ghost()
                        .cursor_pointer()
                        .xsmall()
                        .icon(registry::UI_CLOSE)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            this.manager
                                .update(cx, |manager, cx| manager.close_tab(id, cx));
                        })),
                ),
        )
        .into_any_element()
    }

    /// EXP-696: one REMOTE session's chip — the web dock tab, translated:
    /// a status dot, the mono identifier, the subject, and the host machine's
    /// name (with several machines it is the only thing telling two runs
    /// apart). The X KILLS the run behind a confirm; it never merely hides
    /// the chip, which lives as long as the synced row does. A run this
    /// client may not kill (a paused host — it resumes on its own) shows no X
    /// at all rather than a button with nothing to do.
    fn render_remote_chip(
        &self,
        chip: &RemoteChip,
        ix: usize,
        selected_ix: usize,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let tone = remote_chip_tone(chip.display, chip.paused, cx);
        let session_id = chip.session_id.clone();
        let kill_id = chip.session_id.clone();
        let kill_label = chip.device.clone();
        let mut tab = crate::surface::RichTab::new(("steer-tab", ix), ix == selected_ix);
        tab.paused = chip.paused;
        tab.status = crate::surface::RichTabStatus::Dot(tone);
        tab.identifier = chip.identifier.clone();
        tab.title = Some(chip.title.clone());
        tab.caption = chip
            .device
            .clone()
            .map(|device| SharedString::from(format!(" · {device}")));
        crate::surface::rich_tab(tab, cx)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                this.activate_steer(&session_id, window, cx);
            }))
            .when(chip.killable, |this| {
                this.child(
                    Button::new(("kill-steer-tab", ix))
                        .ghost()
                        .cursor_pointer()
                        .xsmall()
                        .icon(registry::UI_CLOSE)
                        .tooltip("Kill session")
                        .on_click(cx.listener(move |_this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            prompt_kill_remote(kill_id.clone(), kill_label.clone(), window, cx);
                        })),
                )
            })
            .into_any_element()
    }

    /// The merge button for the ACTIVE session tab whose PR is open (issue
    /// AND batch since EXP-498). EXP-484 moved it off the chip into the
    /// per-tab toolbar: a labeled button never squeezes a title there, and
    /// the chip got its close button back (merging still closes the
    /// session). Two-click confirm via the shared `pr_merge`
    /// state ("Merge" → "Confirm merge", ~5s auto-disarm). A failed merge
    /// (typically conflicts) jumps to the Reviews PAGE, where the shared error
    /// caption + Fix-conflicts button render exactly as a Reviews-originated
    /// failure.
    ///
    /// The tab closes LOCALLY the moment the merge call fires (the
    /// `TabClosed` watcher fires the idempotent `codingSessions.end`), so a
    /// merge that fails on conflicts never leaves a live session holding the
    /// branch — the Reviews page's "Fix conflicts" recovery starts
    /// immediately instead of parking behind a busy worktree. The server
    /// ends the user's live sessions on OTHER devices after the merge.
    fn tab_merge_button(
        &self,
        tab: TabId,
        merge: &MergeTabMeta,
        merge_state: &Entity<crate::pr_merge::MergeState>,
        cx: &gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let (armed, merging) = {
            let state = merge_state.read(cx);
            (state.armed(&merge.issue_id), state.merging(&merge.issue_id))
        };
        // EXP-484: one button per dock (the toolbar renders the ACTIVE tab
        // only), so the id no longer carries a strip index.
        let mut button = Button::new("merge-session-changes").xsmall();
        if merging {
            button = button
                .outline().cursor_pointer()
                .label("Merging…")
                .loading(true)
                .disabled(true);
        } else if armed {
            button = button.outline().cursor_pointer().label("Confirm merge").danger().cursor_pointer();
        } else {
            button = button
                .ghost().cursor_pointer()
                .icon(ExpIcon::GitMerge)
                .label("Merge")
                .tooltip("Merge: completes every linked issue and closes this coding session");
        }
        let issue_id = merge.issue_id.clone();
        let button = button.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            cx.stop_propagation();
            let handle = window.window_handle();
            let outcome = crate::pr_merge::two_click(
                crate::pr_merge::MergeOp::MergeIssuePr {
                    issue_id: issue_id.clone(),
                },
                Some(Box::new(move |cx: &mut App| {
                    let _ = handle.update(cx, |_, window, cx| {
                        // EXP-706: Reviews is a full-page screen now, not a
                        // rail tool window.
                        crate::navigation::navigate(
                            window,
                            cx,
                            crate::navigation::Screen::Reviews,
                        );
                    });
                })),
                None,
                cx,
            );
            if outcome == crate::pr_merge::TwoClick::Fired {
                // Confirmed: close the session tab NOW, not off the server's
                // →`ended` echo — a conflict failure must find the branch
                // free so the "Fix conflicts" recovery can start right away.
                this.manager
                    .update(cx, |manager, cx| manager.close_tab(tab, cx));
            }
        }));
        button.into_any_element()
    }

    /// The "+" dropdown (EXP-325): one item per doctor-INSTALLED agent CLI —
    /// clicking immediately launches a promptless CHAT run of that agent
    /// (EXP-703, [`Self::launch_chat_run`]) on the current team's repo
    /// (several board-backed repos → a repo picker submenu; resolver still
    /// loading / no repo → disabled) — plus the plain "New shell" (the
    /// pre-EXP-325 `+` behavior; cmd-t unchanged). Installed agents and
    /// repos resolve fresh at OPEN time (the closure outlives renders); no
    /// doctor report yet → only the shell item.
    fn new_tab_menu(&self, cx: &gpui::Context<Self>) -> impl IntoElement {
        let panel = cx.entity().downgrade();
        Button::new("new-terminal-tab")
            .ghost().cursor_pointer()
            .xsmall()
            .icon(registry::UI_ADD)
            .tooltip("New session")
            .dropdown_menu(move |mut menu, window, cx| {
                let hub = CodingHub::global(cx);
                let installed = hub
                    .read(cx)
                    .doctor
                    .report
                    .as_ref()
                    .map(|report| report.installed_agents())
                    .unwrap_or_default();
                let resolver = repo_resolver_for_window(window, cx);
                resolver.update(cx, |resolver, cx| resolver.ensure_loaded(cx));
                let repos = resolver.read(cx).board_backed_repos();
                // EXP-367: a REPORTED empty agent set gets a disabled hint
                // item (a bare "New shell" menu otherwise reads like the
                // agent launches vanished for no reason); a missing report
                // stays silent — the probe is still running. Signed-out
                // agents (EXP-409) get their own disabled rows below, so the
                // "nothing installed" copy only shows when that is true.
                let has_unauthed = hub
                    .read(cx)
                    .doctor
                    .report
                    .as_ref()
                    .is_some_and(|report| !report.unauthed_agents().is_empty());
                if installed.is_empty() && !has_unauthed && hub.read(cx).doctor.report.is_some() {
                    menu = menu.item(
                        PopupMenuItem::new(crate::coding_flow::NO_AGENT_COPY).disabled(true),
                    );
                }
                for agent in installed {
                    let icon = Icon::from(crate::coding_selects::agent_icon(agent));
                    let label = agent.label();
                    match repos.as_deref() {
                        Some([repo]) => {
                            // One repo — launch directly, no submenu.
                            let panel = panel.clone();
                            let repository_id = repo.repository_id.clone();
                            let full_name = repo.full_name.clone();
                            menu = menu.item(PopupMenuItem::new(label).icon(icon).on_click(
                                move |_, window, cx| {
                                    let Some(panel) = panel.upgrade() else {
                                        return;
                                    };
                                    let repository_id = repository_id.clone();
                                    let full_name = full_name.clone();
                                    panel.update(cx, |panel, cx| {
                                        panel.launch_chat_run(
                                            agent,
                                            repository_id,
                                            full_name,
                                            window,
                                            cx,
                                        );
                                    });
                                },
                            ));
                        }
                        Some(repos) if repos.len() > 1 => {
                            // Several distinct repos — pick one explicitly.
                            let panel = panel.clone();
                            let repos: Vec<(String, String)> = repos
                                .iter()
                                .map(|repo| {
                                    (repo.repository_id.clone(), repo.full_name.clone())
                                })
                                .collect();
                            menu = menu.submenu_with_icon(
                                Some(icon),
                                label,
                                window,
                                cx,
                                move |mut submenu, _window, _cx| {
                                    for (repository_id, full_name) in &repos {
                                        let panel = panel.clone();
                                        let repository_id = repository_id.clone();
                                        let full_name = full_name.clone();
                                        let item_label = SharedString::from(full_name.clone());
                                        submenu = submenu.item(
                                            PopupMenuItem::new(item_label).on_click(
                                                move |_, window, cx| {
                                                    let Some(panel) = panel.upgrade() else {
                                                        return;
                                                    };
                                                    let repository_id = repository_id.clone();
                                                    let full_name = full_name.clone();
                                                    panel.update(cx, |panel, cx| {
                                                        panel.launch_chat_run(
                                                            agent,
                                                            repository_id,
                                                            full_name,
                                                            window,
                                                            cx,
                                                        );
                                                    });
                                                },
                                            ),
                                        );
                                    }
                                    submenu
                                },
                            );
                        }
                        // Resolver loading, fetch failed, or zero
                        // board-backed repos — the agent has no trunk to
                        // land on; keep the item visible but inert.
                        _ => {
                            menu = menu.item(PopupMenuItem::new(label).icon(icon).disabled(true));
                        }
                    }
                }
                // EXP-409: installed-but-signed-out agents stay visible as
                // disabled rows with the fix, instead of silently vanishing
                // from the menu.
                let unauthed = hub
                    .read(cx)
                    .doctor
                    .report
                    .as_ref()
                    .map(|report| report.unauthed_agents())
                    .unwrap_or_default();
                for agent in unauthed {
                    let icon = Icon::from(crate::coding_selects::agent_icon(agent));
                    menu = menu.item(
                        PopupMenuItem::new(SharedString::from(format!(
                            "{} — not signed in",
                            agent.label()
                        )))
                        .icon(icon)
                        .disabled(true),
                    );
                }
                // EXP-697: no dividers in menus.
                let shell_panel = panel.clone();
                menu.item(
                    PopupMenuItem::new("New shell")
                        .icon(Icon::new(registry::NAV_TERMINAL))
                        .on_click(move |_, window, cx| {
                            let Some(panel) = shell_panel.upgrade() else {
                                return;
                            };
                            panel.update(cx, |panel, cx| panel.new_shell_tab(window, cx));
                        }),
                )
            })
    }

    /// EXP-703: the "+" menu / empty-state agent launch — a promptless CHAT
    /// run over the builtin action rails ([`crate::action_run`] with the
    /// hidden `builtin:chat` action and only its `repo` input filled).
    /// Unlike the EXP-325 agent shell it replaced on this surface, the run
    /// gets a `coding_sessions` row, a steer channel and the MCP session
    /// header — visible and steerable from web and mobile, and a child it
    /// starts via `exponential_sessions_start` gets parent linkage (EXP-700)
    /// — plus its OWN worktree on `exp/chat-<id8>` instead of the trunk
    /// clone. The agent still spawns with NO initial prompt and waits for
    /// input, exactly like the shell it replaces (the attended promptless
    /// shape `coding::prepare_action` allows since EXP-703).
    pub(crate) fn launch_chat_run(
        &mut self,
        agent: coding::CodingAgent,
        repository_id: String,
        full_name: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(deps) = crate::coding_flow::build_action_deps(cx) else {
            log::warn!("terminal dock: chat launch ignored — not signed in");
            return;
        };
        let nav = navigation::nav_for_window(window, cx);
        let Some(team_id) = navigation::active_team_id(&nav, cx) else {
            log::warn!("terminal dock: chat launch ignored — no active team");
            return;
        };
        let options = coding::LaunchOptions::defaults_for(&deps.settings, agent);
        let action = api::actions::builtin_chat_action(&team_id);
        // Only the `repo` input rides — deliberately NO `prompt`: the person
        // is sitting at the terminal and types the first message themselves.
        let inputs: Vec<coding::ActionInputValue> = action
            .inputs
            .iter()
            .filter(|input| input.key == "repo")
            .map(|input| coding::ActionInputValue {
                key: input.key.clone(),
                label: input.label.clone(),
                input_type: input.input_type.clone(),
                value: repository_id.clone(),
                display: Some(full_name.clone()),
            })
            .collect();
        // EXP-372: the cards must be gone for the launch's whole flight. The
        // runner has no success callback — the TabOpened edge clears the
        // progress line; the failure hook covers every refused/failed start.
        self.set_pending_launch(Some(agent.label().into()), cx);
        let panel = cx.entity().downgrade();
        let on_failed: crate::action_run::ActionFailureHook = Box::new(move |cx| {
            if let Some(panel) = panel.upgrade() {
                panel.update(cx, |panel, cx| panel.set_pending_launch(None, cx));
            }
        });
        crate::action_run::start_action_run(
            crate::action_run::StartActionArgs {
                action_id: action.id,
                team_id,
                repo: crate::action_run::ActionRepo::Resolve,
                options,
                origin: coding::LaunchOrigin::Local,
                inputs,
                target: Some(window.window_handle()),
                activate_app: false,
                reservation: None,
                trigger: None,
                automation_id: None,
                on_failed: Some(on_failed),
            },
            cx,
        );
    }

    /// The EXP-325 promptless agent launch: background
    /// [`coding::prepare_agent_shell`] (doctor → token → clone/autopull →
    /// MCP wiring → promptless argv with the agent's settings defaults) →
    /// foreground [`TabKind::AgentShell`] tab in THIS dock. No
    /// `coding_sessions` row / heartbeat / exit hook — the session has no
    /// issue/batch/action subject; the P9 token-refresher hold keeps `git
    /// push` working past the token TTL, released on tab close.
    ///
    /// EXP-703 moved the dock's own "+"/empty-state launches onto
    /// [`Self::launch_chat_run`]; this path stays for the EXP-369
    /// worktree-PINNED terminals (the settings pane's per-worktree button and
    /// the file tree's "Open agent here"), which must run in an EXISTING
    /// worktree — the chat rails always cut a fresh one. `cwd_override` pins
    /// the run to one of the clone's worktrees; `None` runs on the trunk
    /// clone root.
    pub(crate) fn launch_agent_shell(
        &mut self,
        agent: coding::CodingAgent,
        repository_id: String,
        full_name: String,
        cwd_override: Option<PathBuf>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(deps) = crate::coding_flow::build_action_deps(cx) else {
            log::warn!("terminal dock: agent shell ignored — not signed in");
            return;
        };
        let options = coding::LaunchOptions::defaults_for(&deps.settings, agent);
        let request = coding::AgentShellRequest {
            options,
            repository_id,
            full_name,
            cwd_override,
        };
        // EXP-372: the prepare below is ~a second of work; the empty state's
        // cards must be gone before it starts, not after it lands.
        self.set_pending_launch(Some(agent.label().into()), cx);
        cx.spawn_in(window, async move |this, cx| {
            let prepared = cx
                .background_executor()
                .spawn(async move { coding::prepare_agent_shell(&request, &deps) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                // Landed — success or not, the cards come back if no tab opens.
                this.set_pending_launch(None, cx);
                let launch = match prepared {
                    Ok(coding::PreparedAgentShell::Ready(launch)) => launch,
                    Ok(coding::PreparedAgentShell::Disabled(reason)) => {
                        window.push_notification(
                            Notification::error(SharedString::from(reason.message())),
                            cx,
                        );
                        return;
                    }
                    Err(err) => {
                        log::warn!("terminal dock: agent shell prepare failed: {err}");
                        window.push_notification(
                            Notification::error(SharedString::from(err.to_string())),
                            cx,
                        );
                        return;
                    }
                };
                let opened = this.manager.update(cx, |manager, cx| {
                    manager.open_tab(
                        TabKind::AgentShell,
                        launch.tab_title.clone(),
                        Some(launch.tab_title_prefix.clone().into()),
                        &launch.spawn,
                        None,
                        cx,
                    )
                });
                match opened {
                    Ok(tab_id) => {
                        TokenRefreshers::retain(&launch.clone, &launch.repository_id, cx);
                        this.agent_shell_holds.insert(tab_id, launch.clone.clone());
                    }
                    Err(error) => {
                        log::warn!("terminal dock: agent shell spawn failed: {error:#}");
                        window.push_notification(
                            Notification::error(SharedString::from(error.to_string())),
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }

    /// EXP-484 (C1): open one agent-LOGIN tab — `claude auth login
    /// --claudeai`, `codex login --device-auth`, or pi's bare TUI with
    /// `/login` typed at its prompt.
    ///
    /// A plain [`TerminalManager::open_tab`], deliberately NOT the
    /// [`Self::launch_agent_shell`] path: `coding::prepare_agent_shell`
    /// refuses a signed-out agent, which is exactly who needs to sign in.
    /// No `coding_sessions` row, no token hold, no MCP wiring — just the
    /// CLI's own login command in a visible tab. The caller (
    /// [`crate::agent_login`]) owns the logout-first switch, the pi typing
    /// and the exit hook.
    pub(crate) fn launch_agent_login(
        &mut self,
        agent: coding::CodingAgent,
        plan: &coding::LoginPlan,
        on_exit: Option<terminal::tab::ExitHook>,
        cx: &mut gpui::Context<Self>,
    ) -> anyhow::Result<TabId> {
        self.manager.update(cx, |manager, cx| {
            manager.open_tab(
                TabKind::AgentLogin(agent.id().to_string()),
                SharedString::from(plan.title.clone()),
                Some(SharedString::from(agent.label().to_string())),
                &plan.spawn,
                on_exit,
                cx,
            )
        })
    }

    /// What the next Latest-changes poll should do. Clears the snapshot (and
    /// skips git entirely) whenever there is nothing to show: a collapsed
    /// dock, no active tab, an undocked one, or a tab that is not a local
    /// coding session (a plain shell has no branch to diff).
    fn changes_job(&mut self, cx: &mut gpui::Context<Self>) -> ChangesJob {
        let idle = |this: &mut Self, cx: &mut gpui::Context<Self>| {
            if this.changes.take().is_some() {
                cx.notify();
            }
            ChangesJob::Idle
        };
        if self.dock_collapsed(cx) {
            return idle(self, cx);
        }
        let Some(tab) = self
            .manager
            .read(cx)
            .active_tab()
            .map(|tab| tab.id)
            .filter(|id| !crate::undock::is_terminal_tab_undocked(*id, cx))
        else {
            return idle(self, cx);
        };
        let Some(sessions) = crate::coding_flow::LocalSessions::global_ref(cx) else {
            return idle(self, cx);
        };
        let scope = {
            let sessions = sessions.read(cx);
            sessions
                .session_for_tab(tab)
                .map(|session| (session.worktree.clone(), session.base_ref.clone()))
        };
        let Some((worktree, base_ref)) = scope else {
            return idle(self, cx);
        };
        ChangesJob::Poll {
            tab,
            worktree,
            base_ref,
        }
    }

    /// Install a poll's answer. A FAILED poll (`None`) keeps the previous
    /// snapshot (`merge_changes_snapshot`): a diff momentarily unreadable —
    /// mid-rebase, mid-checkout — must not blank the bar and strand the Merge
    /// pill alone. A real empty answer (the branch was reset) clears it.
    fn apply_changes(
        &mut self,
        tab: TabId,
        files: Option<Vec<coding::scm::DiffFile>>,
        cx: &mut gpui::Context<Self>,
    ) {
        let previous = self
            .changes
            .take()
            .filter(|changes| changes.tab == tab);
        let expanded = previous.as_ref().is_some_and(|changes| changes.expanded);
        let generation = previous.as_ref().map_or(0, |changes| changes.generation);
        let previous_files = previous.map(|changes| changes.files).unwrap_or_default();
        let changed = files.as_ref().is_some_and(|files| *files != previous_files);
        let files = merge_changes_snapshot(previous_files, files);
        let (additions, deletions) = changes_totals(&files);
        self.changes = Some(ChangesState {
            tab,
            files,
            additions,
            deletions,
            expanded,
            generation: generation + u64::from(changed),
        });
        if changed && expanded {
            self.rebuild_changes_diff(cx);
        }
        cx.notify();
    }

    /// Rebuild the expanded side-by-side view from the current snapshot.
    fn rebuild_changes_diff(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(changes) = self.changes.as_ref() else {
            return;
        };
        let prepared = crate::diff::build_scm_diff(&changes.files, &cx.theme().highlight_theme);
        self.changes_diff
            .update(cx, |diff, cx| diff.set_prepared(prepared, cx));
    }

    /// EXP-678/EXP-688: "Latest changes" — the branch's diff (everything the
    /// PR carries, committed work included) plus the Merge button, in one
    /// row directly above the bottom strip. Mirrors the web session view's
    /// row; it renders when there IS a diff or an open PR to merge, so the
    /// Merge pill never stands alone.
    ///
    /// EXP-698 split the CHROME out into [`Self::changes_bar_chrome`] — the
    /// steer arm renders the same row off the relay-delivered diff, and two
    /// hand-built copies of one bar is how they drift.
    fn render_changes_bar(
        &self,
        tab: TabId,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let merge = merge_tab_meta(tab, cx);
        let changes = self
            .changes
            .as_ref()
            .filter(|changes| changes.tab == tab && !changes.files.is_empty());
        if !changes_bar_visible(changes.is_some(), merge.is_some()) {
            return None;
        }
        let expanded = changes.is_some_and(|changes| changes.expanded);
        let totals = changes.map(|changes| (changes.additions, changes.deletions));
        let merge_button = merge.as_ref().map(|merge| {
            let merge_state = crate::pr_merge::MergeState::global(cx);
            self.tab_merge_button(tab, merge, &merge_state, cx)
        });
        Some(self.changes_bar_chrome(
            "terminal-changes-toggle",
            totals,
            expanded,
            merge_button,
            self.changes_diff.clone(),
            |this, cx| this.toggle_changes_expanded(cx),
            cx,
        ))
    }

    /// EXP-698 — the ONE Latest-changes row: the collapsible `+N −M` summary
    /// on the left, the Merge capsule on the right, and (expanded) the
    /// side-by-side diff underneath. Both arms of the dock's content — a
    /// LOCAL terminal tab and a STEER viewer — render through this, so the
    /// bar is one design with one set of metrics.
    ///
    /// `totals` is `None` when there is no diff at all (an open PR whose
    /// branch no longer differs): the row still draws, carrying only the
    /// Merge pill, and nothing is clickable on the left.
    #[allow(clippy::too_many_arguments)] // two call sites, one row
    fn changes_bar_chrome(
        &self,
        toggle_id: &'static str,
        totals: Option<(u32, u32)>,
        expanded: bool,
        merge_button: Option<gpui::AnyElement>,
        diff_view: Entity<crate::diff::DiffView>,
        on_toggle: impl Fn(&mut Self, &mut gpui::Context<Self>) + 'static,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut left = h_flex()
            .id(toggle_id)
            .min_w_0()
            .flex_1()
            .gap_1p5()
            .items_center()
            .text_xs()
            .text_color(muted);
        if let Some((additions, deletions)) = totals {
            left = left
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    on_toggle(this, cx);
                }))
                .child(
                    Icon::new(if expanded {
                        registry::UI_CHEVRON_DOWN
                    } else {
                        registry::UI_CHEVRON_RIGHT
                    })
                    .xsmall(),
                )
                .child(Icon::new(registry::CODING_DIFF).xsmall())
                .child("Latest changes")
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
                );
        }

        let mut row = h_flex()
            .w_full()
            .h(px(CHANGES_BAR_H))
            .px_2()
            .gap_2()
            .items_center()
            .flex_shrink_0()
            .border_t_1()
            .border_color(theme::tokens::glass::STROKE_SECTION.to_hsla())
            .bg(theme::tokens::glass::FILL_SECTION.to_hsla())
            .child(left);
        if let Some(merge_button) = merge_button {
            row = row.child(
                // READONLY: the shell is only the capsule around the merge
                // button — the button owns the cursor and the hover, and a
                // second hover lift on the wrapper would light up on the
                // capsule's own padding, which does nothing.
                crate::surface::glass_pill(
                    "changes-bar-merge",
                    crate::surface::PillSize::Sm,
                    crate::surface::PillMode::Readonly,
                    cx,
                )
                .px_0()
                .child(merge_button),
            );
        }

        let bar = v_flex().w_full().flex_shrink_0().child(row);
        if expanded {
            bar.child(div().w_full().h(px(CHANGES_DIFF_H)).child(diff_view))
                .into_any_element()
        } else {
            bar.into_any_element()
        }
    }

    /// EXP-698 — the steer arm's Latest-changes bar. The relay hands the
    /// viewer the host's worktree diff as a unified-diff STRING
    /// ([`crate::steer_viewer::SteerSessionView::latest_diff`]), so the same
    /// summary and the same Merge affordance a local tab gets are available
    /// for a run on another machine; only the SOURCE of the diff differs (a
    /// string off the wire instead of a `git` shell-out here).
    fn render_steer_changes_bar(
        &mut self,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let view = self.steer_views.get(session_id)?.clone();
        let (raw, merge, over) = {
            let view = view.read(cx);
            (
                view.latest_diff().map(str::to_string),
                view.session_row()
                    .and_then(|row| merge_meta_for_session(row, cx)),
                view.session_over(),
            )
        };
        // A finished run offers no Merge (iOS/web `canMerge` gate the same
        // way) — the PR merges from Reviews once the session is over.
        let merge = merge.filter(|_| !over);
        if !changes_bar_visible(raw.is_some(), merge.is_some()) {
            return None;
        }
        self.sync_steer_changes(session_id, raw.as_deref(), cx);
        let state = self
            .steer_changes
            .as_ref()
            .filter(|state| state.session_id == session_id);
        let expanded = state.is_some_and(|state| state.expanded);
        let totals = state.map(|state| (state.additions, state.deletions));
        let merge_button = merge.as_ref().map(|merge| {
            let merge_state = crate::pr_merge::MergeState::global(cx);
            self.steer_merge_button(merge, &merge_state, cx)
        });
        Some(self.changes_bar_chrome(
            "steer-changes-toggle",
            totals,
            expanded,
            merge_button,
            self.steer_changes_diff.clone(),
            |this, cx| this.toggle_steer_changes_expanded(cx),
            cx,
        ))
    }

    /// Re-parse the steer diff only when the relay actually delivered a new
    /// one (the raw string is the cache key): a unified-diff parse per
    /// repaint of a live feed is real work for no new information.
    fn sync_steer_changes(
        &mut self,
        session_id: &str,
        raw: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let same = self
            .steer_changes
            .as_ref()
            .is_some_and(|state| state.session_id == session_id && state.raw.as_deref() == raw);
        if same {
            return;
        }
        let Some(raw) = raw else {
            self.steer_changes = None;
            return;
        };
        let expanded = self
            .steer_changes
            .as_ref()
            .filter(|state| state.session_id == session_id)
            .is_some_and(|state| state.expanded);
        let files = coding::scm::parse_unified_diff(raw);
        let (additions, deletions) = changes_totals(&files);
        self.steer_changes = Some(SteerChangesState {
            session_id: session_id.to_string(),
            raw: Some(raw.to_string()),
            files,
            additions,
            deletions,
            expanded,
        });
        if expanded {
            self.rebuild_steer_changes_diff(cx);
        }
    }

    fn rebuild_steer_changes_diff(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(state) = self.steer_changes.as_ref() else {
            return;
        };
        let prepared = crate::diff::build_scm_diff(&state.files, &cx.theme().highlight_theme);
        self.steer_changes_diff
            .update(cx, |diff, cx| diff.set_prepared(prepared, cx));
    }

    fn toggle_steer_changes_expanded(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(state) = self.steer_changes.as_mut() else {
            return;
        };
        state.expanded = !state.expanded;
        if state.expanded {
            self.rebuild_steer_changes_diff(cx);
        }
        cx.notify();
    }

    /// The steer arm's Merge — the same two-click arm/confirm machinery every
    /// other Merge surface drives ([`crate::pr_merge`]), minus the local
    /// tab close: a remote run has no terminal tab here, and the server ends
    /// the session on merge anyway (EXP-498).
    fn steer_merge_button(
        &self,
        merge: &MergeTabMeta,
        merge_state: &Entity<crate::pr_merge::MergeState>,
        cx: &gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let (armed, merging) = {
            let state = merge_state.read(cx);
            (state.armed(&merge.issue_id), state.merging(&merge.issue_id))
        };
        let mut button = Button::new("merge-steer-session-changes").xsmall();
        if merging {
            button = button
                .outline()
                .cursor_pointer()
                .label("Merging…")
                .loading(true)
                .disabled(true);
        } else if armed {
            button = button
                .outline()
                .cursor_pointer()
                .label("Confirm merge")
                .danger();
        } else {
            button = button
                .ghost()
                .cursor_pointer()
                .icon(ExpIcon::GitMerge)
                .label("Merge")
                .tooltip("Merge: completes every linked issue and closes this coding session");
        }
        let issue_id = merge.issue_id.clone();
        button
            .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                let handle = window.window_handle();
                crate::pr_merge::two_click(
                    crate::pr_merge::MergeOp::MergeIssuePr {
                        issue_id: issue_id.clone(),
                    },
                    Some(Box::new(move |cx: &mut App| {
                        let _ = handle.update(cx, |_, window, cx| {
                            crate::navigation::navigate(
                                window,
                                cx,
                                crate::navigation::Screen::Reviews,
                            );
                        });
                    })),
                    None,
                    cx,
                );
            }))
            .into_any_element()
    }

    /// Flip the Latest-changes bar open/shut, building the diff rows the
    /// first time it opens (they are only worth rendering when visible).
    fn toggle_changes_expanded(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(changes) = self.changes.as_mut() else {
            return;
        };
        changes.expanded = !changes.expanded;
        if changes.expanded {
            self.rebuild_changes_diff(cx);
        }
        cx.notify();
    }

    /// EXP-65: every visible tab popped out into its own window — the dock
    /// stays usable (the bar keeps the `+`), with a hint instead of a
    /// terminal. Deliberately NOT the empty state: the manager isn't empty,
    /// the tabs just live elsewhere — offering launch cards here would read
    /// as "your sessions are gone".
    fn render_undocked_hint(&self, cx: &gpui::Context<Self>) -> impl IntoElement {
        v_flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .gap_1()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(Icon::new(registry::NAV_TERMINAL).small())
            .child("All terminal tabs are open in separate windows.")
    }

    /// EXP-369: the expanded-and-empty dock. Same options as the `+` menu —
    /// one card per doctor-INSTALLED agent plus "New shell" — as icon-over-
    /// label cards with no explanatory text. An agent card launches straight
    /// away on the single board-backed repo, opens a repo picker when the team
    /// has several, and stays inert while the resolver is loading or no board
    /// is backed by a repo (the menu's exact gating). No doctor report yet
    /// (the probe is still running) → the shell card alone.
    ///
    /// EXP-372: once a card is clicked the row is REPLACED by a progress line
    /// until the tab opens (or the launch fails) — the agent prepare takes
    /// about a second, and live cards in that window spawned a tab per click.
    fn render_empty_dock_options(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if let Some(label) = self.pending_launch.clone() {
            return v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(Spinner::new().icon(registry::UI_LOADING).xsmall())
                        .child(format!("Starting {label}\u{2026}")),
                )
                .into_any_element();
        }

        let installed = CodingHub::global(cx)
            .read(cx)
            .doctor
            .report
            .as_ref()
            .map(|report| report.installed_agents())
            .unwrap_or_default();
        let resolver = repo_resolver_for_window(window, cx);
        resolver.update(cx, |resolver, cx| resolver.ensure_loaded(cx));
        let repos = resolver.read(cx).board_backed_repos();
        let panel = cx.entity().downgrade();

        let cards = installed.into_iter().enumerate().map(|(ix, agent)| {
            let card = empty_dock_card(
                ("terminal-empty-agent", ix),
                Icon::from(crate::coding_selects::agent_icon(agent)),
                agent.label(),
                cx,
            );
            match repos.as_deref() {
                Some([repo]) => {
                    let panel = panel.clone();
                    let repository_id = repo.repository_id.clone();
                    let full_name = repo.full_name.clone();
                    card.on_click(move |_, window, cx| {
                        let Some(panel) = panel.upgrade() else {
                            return;
                        };
                        let repository_id = repository_id.clone();
                        let full_name = full_name.clone();
                        panel.update(cx, |panel, cx| {
                            panel.launch_chat_run(
                                agent,
                                repository_id,
                                full_name,
                                window,
                                cx,
                            );
                        });
                    })
                    .into_any_element()
                }
                // Several distinct repos — the card opens the same picker the
                // `+` menu offers as a submenu (repos re-resolve at open time).
                Some([_, _, ..]) => {
                    let panel = panel.clone();
                    card.dropdown_menu(move |mut menu, window, cx| {
                        let resolver = repo_resolver_for_window(window, cx);
                        let repos = resolver.read(cx).board_backed_repos().unwrap_or_default();
                        for repo in repos {
                            let panel = panel.clone();
                            let repository_id = repo.repository_id.clone();
                            let full_name = repo.full_name.clone();
                            menu = menu.item(
                                PopupMenuItem::new(SharedString::from(full_name.clone())).on_click(
                                    move |_, window, cx| {
                                        let Some(panel) = panel.upgrade() else {
                                            return;
                                        };
                                        let repository_id = repository_id.clone();
                                        let full_name = full_name.clone();
                                        panel.update(cx, |panel, cx| {
                                            panel.launch_chat_run(
                                                agent,
                                                repository_id,
                                                full_name,
                                                window,
                                                cx,
                                            );
                                        });
                                    },
                                ),
                            );
                        }
                        menu
                    })
                    .into_any_element()
                }
                // No board-backed repo — the agent has no trunk to land on.
                Some([]) => card
                    .disabled(true)
                    .tooltip("No repository is linked to a board in this team yet.")
                    .into_any_element(),
                // Resolver still loading (or its fetch failed).
                None => card
                    .disabled(true)
                    .tooltip("Looking up this team's repositories…")
                    .into_any_element(),
            }
        });

        v_flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .justify_center()
                    .flex_wrap()
                    .children(cards)
                    .child(
                        empty_dock_card(
                            "terminal-empty-shell",
                            Icon::new(registry::NAV_TERMINAL),
                            "New shell",
                            cx,
                        )
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.new_shell_tab(window, cx);
                        })),
                    ),
            )
            .into_any_element()
    }
}

/// One empty-dock launch card (EXP-369): icon over label, nothing else. A
/// `Button` rather than a plain surface so the multi-repo agent case can hang
/// the very same `.dropdown_menu` the `+` menu uses on it.
fn empty_dock_card(
    id: impl Into<gpui::ElementId>,
    icon: Icon,
    label: &'static str,
    cx: &App,
) -> Button {
    Button::new(id)
        .ghost().cursor_pointer()
        .w(px(104.))
        .h(px(88.))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .bg(theme::tokens::glass::FILL_CARD.to_hsla())
        .child(
            v_flex()
                .gap_2()
                .items_center()
                .justify_center()
                .child(icon.large())
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(label),
                ),
        )
}

/// The JetBrains "process finished" strip under a dead tab's final
/// scrollback (§7.5 exit-code strip; the tab stays open). Free function so
/// the EXP-65 undocked terminal window renders the identical strip.
pub(crate) fn exit_strip(code: i32, cx: &App) -> impl IntoElement {
    let color = if code == 0 {
        cx.theme().success
    } else {
        cx.theme().danger
    };
    h_flex()
        .gap_2()
        .px_3()
        .py_1()
        .items_center()
        .border_t_1()
        .border_color(cx.theme().border)
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(div().size(px(6.)).rounded_full().bg(color))
        .child(SharedString::from(format!(
            "Process finished with exit code {code}"
        )))
}

/// Per-tab render snapshot (cloned out so the manager borrow ends before the
/// listeners borrow `cx`). One entry per VISIBLE tab — undocked tabs are
/// filtered out, so `manager_ix` keeps the strip position → manager index
/// mapping honest (EXP-65).
struct TabMeta {
    manager_ix: usize,
    id: TabId,
    title: SharedString,
    exit_code: Option<i32>,
    /// EXP-325: present when this tab is a LOCAL issue coding session whose
    /// issue row is synced — the chip then renders the center issue-tab
    /// treatment (status glyph + mono identifier + synced title) instead of
    /// the plain terminal title. Batch/action/shell tabs (and unsynced
    /// issues) stay `None`.
    issue: Option<IssueTabMeta>,
}

/// The issue-chip snapshot of one issue-session terminal tab (EXP-325).
struct IssueTabMeta {
    status: domain::statuses::ResolvedStatus,
    identifier: SharedString,
    /// `None` for a blank issue title — the identifier already labels the
    /// chip (the EXP-310 center-tab rule).
    title: Option<SharedString>,
}

/// EXP-688: the Latest-changes snapshot for ONE session tab.
struct ChangesState {
    tab: TabId,
    files: Vec<coding::scm::DiffFile>,
    additions: u32,
    deletions: u32,
    /// Whether the bar is showing its side-by-side diff.
    expanded: bool,
    /// Bumped on every CHANGED snapshot — the expanded view rebuilds off it.
    generation: u64,
}

/// EXP-698: the STEER arm's Latest-changes snapshot. Unlike [`ChangesState`]
/// nothing is polled here — the host publishes the worktree diff on the
/// activity channel and the viewer's feed keeps the latest one, so this is
/// only the PARSE of that string plus the bar's expanded flag.
struct SteerChangesState {
    /// Which steer chip the snapshot belongs to.
    session_id: String,
    /// The raw unified diff it was parsed from — the cache key, so a repaint
    /// of an unchanged feed re-parses nothing.
    raw: Option<String>,
    files: Vec<coding::scm::DiffFile>,
    additions: u32,
    deletions: u32,
    expanded: bool,
}

/// What one Latest-changes tick has to do.
enum ChangesJob {
    /// Nothing to show (collapsed dock / no session tab) — no git, no bar.
    Idle,
    Poll {
        tab: TabId,
        worktree: PathBuf,
        base_ref: Option<String>,
    },
}

/// The bar shows for a diff OR an open PR: a Merge button with nothing above
/// it is the EXP-688 complaint, and a diff with no PR yet is still the
/// session's work. Pure (unit-tested).
fn changes_bar_visible(has_diff: bool, has_open_pr: bool) -> bool {
    has_diff || has_open_pr
}

/// Keep the last snapshot across a FAILED poll (`None`): git errors for a
/// moment during a rebase/checkout, and blanking the bar on that would strand
/// the Merge button alone — the exact shape of the bug EXP-688 fixes. A real
/// answer always wins, an empty one included (a reset branch has no changes).
/// Pure.
fn merge_changes_snapshot(
    previous: Vec<coding::scm::DiffFile>,
    next: Option<Vec<coding::scm::DiffFile>>,
) -> Vec<coding::scm::DiffFile> {
    next.unwrap_or(previous)
}

/// `+adds -dels` over every file in the snapshot. Pure.
fn changes_totals(files: &[coding::scm::DiffFile]) -> (u32, u32) {
    files.iter().fold((0, 0), |(adds, dels), file| {
        (adds + file.additions, dels + file.deletions)
    })
}

/// The tab's merge affordance (EXP-498): the representative synced issue
/// with an open PR — `issues.mergePr` on it fans out to every issue sharing
/// the prUrl, so any batch sibling merges the whole PR.
struct MergeTabMeta {
    issue_id: String,
}

/// Measured width of one tab chip, for the EXP-497 overflow partition —
/// mirrors the chip layout in `render_strip` piece for piece, the way
/// `screens::measure_chip_width` mirrors the center chips (EXP-326: spacing
/// helpers resolve against the rem size and labels are SHAPED with the
/// window's text system, so "fits" means fits).
fn measure_tab_chip_width(meta: &TabMeta, window: &Window) -> f32 {
    /// `surface::rich_tab`'s `px_2p5`, both sides.
    const CHIP_PADDING_REMS: f32 = 0.625 * 2.;
    /// `Icon::xsmall()` — `size_3` (the issue chip's status glyph).
    const LEAD_ICON_REMS: f32 = 0.75;
    /// An icon-only xsmall `Button` — `size_5` (the chip's close).
    const XSMALL_BUTTON_REMS: f32 = 1.25;
    /// The trailing button cluster's own `gap_0p5`.
    const CLUSTER_GAP_REMS: f32 = 0.125;
    /// The exit badge's `px_1`, both sides.
    const BADGE_PADDING_REMS: f32 = 0.25 * 2.;
    /// `surface::RICH_TAB_TITLE_MAX_W` on the title child — a real pixel
    /// value, so it does NOT scale with the rem.
    const TITLE_MAX_W: f32 = crate::surface::RICH_TAB_TITLE_MAX_W;

    let rem = f32::from(window.rem_size());
    let base_font = window.text_style().font();
    let mut children: Vec<f32> = Vec::with_capacity(3);
    match &meta.issue {
        Some(issue) => {
            children.push(LEAD_ICON_REMS * rem);
            // EXP-310 treatment: the identifier renders `text_xs` in the
            // terminal mono family, not the bar's proportional font.
            let mut mono = base_font.clone();
            mono.family = theme::terminal::FONT_FAMILY.into();
            children.push(crate::screens::measure_text(
                window,
                &issue.identifier,
                mono,
                gpui::rems(0.75),
            ));
            if let Some(title) = issue.title.as_ref() {
                children.push(
                    crate::screens::measure_text(
                        window,
                        title,
                        base_font.clone(),
                        gpui::rems(0.875),
                    )
                    .min(TITLE_MAX_W),
                );
            }
        }
        None => children.push(
            crate::screens::measure_text(window, &meta.title, base_font.clone(), gpui::rems(0.875))
                .min(TITLE_MAX_W),
        ),
    }

    // EXP-698: the exit badge is a `rich_tab` CHILD now (the builder renders
    // it), not a member of the trailing cluster — so it costs a chip gap
    // (`gap_1p5`), not the cluster's `gap_0p5`.
    if let Some(code) = meta.exit_code {
        children.push(
            BADGE_PADDING_REMS * rem
                + crate::screens::measure_text(
                    window,
                    &code.to_string(),
                    base_font.clone(),
                    gpui::rems(0.75),
                ),
        );
    }
    // The trailing cluster is the close button alone. EXP-484 removed the
    // merge button from the chip and EXP-688 the hover-undock slot (the
    // strip's right cluster undocks the active tab), so nothing here reserves
    // a variable-width labeled button or a second icon button any more — the
    // cluster's own `gap_0p5` has nothing left to separate.
    let _ = CLUSTER_GAP_REMS;
    children.push(XSMALL_BUTTON_REMS * rem);

    let gaps =
        crate::screens::rich_tab_child_gap(window) * children.len().saturating_sub(1) as f32;
    CHIP_PADDING_REMS * rem + gaps + children.into_iter().sum::<f32>()
}

/// Resolve a tab back to its issue chip, when it is an issue session over a
/// synced issue row (mirrors `screens::chip_content`).
fn issue_tab_meta(tab_id: TabId, cx: &App) -> Option<IssueTabMeta> {
    let sessions = crate::coding_flow::LocalSessions::global_ref(cx)?;
    let sessions = sessions.read(cx);
    let crate::coding_flow::SessionSubject::Issue(issue_id) = sessions.subject_for_tab(tab_id)?
    else {
        return None;
    };
    let store = sync::Store::try_global(cx)?;
    let issue = store.collections().issues.read(cx).get(issue_id)?;
    let title = issue.title.trim();
    Some(IssueTabMeta {
        status: crate::queries::resolve_issue_status(cx, issue),
        identifier: SharedString::from(issue.identifier.clone()),
        title: (!title.is_empty()).then(|| SharedString::from(title.to_string())),
    })
}

/// Resolve a tab's merge affordance (EXP-498): the representative synced
/// open-PR issue for the tab's session. Issue sessions match their own
/// issue; batch sessions match any synced issue on the session's branch
/// (the batch PR links every one of them to the same prUrl, so any sibling
/// is a valid merge target). Action/shell tabs never merge.
fn merge_tab_meta(tab_id: TabId, cx: &App) -> Option<MergeTabMeta> {
    let sessions = crate::coding_flow::LocalSessions::global_ref(cx)?;
    let sessions = sessions.read(cx);
    let session = sessions.session_for_tab(tab_id)?;
    let store = sync::Store::try_global(cx)?;
    let issues = store.collections().issues.read(cx);
    let issue_id = match &session.subject {
        crate::coding_flow::SessionSubject::Issue(issue_id) => {
            let issue = issues.get(issue_id)?;
            issue_has_open_pr(issue).then(|| issue.id.clone())?
        }
        crate::coding_flow::SessionSubject::Batch(_) => {
            open_pr_issue_on_branch(&session.branch, issues.iter())?
                .id
                .clone()
        }
        crate::coding_flow::SessionSubject::Action(_) => return None,
    };
    Some(MergeTabMeta { issue_id })
}

/// EXP-698 — [`merge_tab_meta`]'s twin for a REMOTE run: the merge target of
/// a synced `coding_sessions` row, with no local session to consult. The two
/// rules are the same ones every client applies (iOS `AgentSessionModel.
/// mergeIssue`, web `use-agents-data`): an issue-linked run merges its OWN
/// issue, a BATCH run resolves the representative open-PR issue through the
/// head branch `pr_open` stamped on the row (EXP-545), and an action run
/// merges nothing.
fn merge_meta_for_session(
    session: &domain::rows::CodingSession,
    cx: &App,
) -> Option<MergeTabMeta> {
    if session.action_id.is_some() {
        return None;
    }
    let store = sync::Store::try_global(cx)?;
    let issues = store.collections().issues.read(cx);
    let issue_id = match session.issue_id.as_deref() {
        Some(issue_id) => {
            let issue = issues.get(issue_id)?;
            issue_has_open_pr(issue).then(|| issue.id.clone())?
        }
        None => open_pr_issue_on_branch(session.branch.as_deref().unwrap_or_default(), issues.iter())?
            .id
            .clone(),
    };
    Some(MergeTabMeta { issue_id })
}

fn issue_has_open_pr(issue: &domain::rows::Issue) -> bool {
    issue.pr_state.as_deref() == Some("open")
}

/// Any synced open-PR issue on `branch` — a batch tab's representative merge
/// target. Pure (unit-tested); an empty branch never matches (trunk/scratch
/// runs record no branch).
fn open_pr_issue_on_branch<'a>(
    branch: &str,
    issues: impl Iterator<Item = &'a domain::rows::Issue>,
) -> Option<&'a domain::rows::Issue> {
    if branch.is_empty() {
        return None;
    }
    let mut issues =
        issues.filter(|issue| issue.branch.as_deref() == Some(branch) && issue_has_open_pr(issue));
    issues.next()
}

// ---------------------------------------------------------------------------
// EXP-696: remote session chips
// ---------------------------------------------------------------------------

/// One chip for a coding session running on ANOTHER of the user's machines
/// (a second desktop, the headless CLI daemon, a shared server). Clicking it
/// opens the steering view; the chip lives as long as the synced row is live.
struct RemoteChip {
    session_id: String,
    /// The linked issue's identifier, when its row has synced.
    identifier: Option<SharedString>,
    title: SharedString,
    /// The host machine's name — web's tabs carry it, and with several
    /// machines it is the only thing telling two runs of one issue apart.
    device: Option<SharedString>,
    display: CodingSessionDisplay,
    paused: bool,
    killable: bool,
    /// `(rem size bits, width)` of the last [`measure_remote_chip_width`] —
    /// the strip re-measures every chip on every repaint, and measuring
    /// SHAPES three labels. Interior mutability so `render` stays `&self`;
    /// the chip is rebuilt (memo and all) whenever its content changes.
    measured: std::cell::Cell<Option<(u32, f32)>>,
}

impl PartialEq for RemoteChip {
    /// The memo is not identity — two chips describing the same row are the
    /// same chip whether or not either has been measured yet.
    fn eq(&self, other: &Self) -> bool {
        self.session_id == other.session_id
            && self.identifier == other.identifier
            && self.title == other.title
            && self.device == other.device
            && self.display == other.display
            && self.paused == other.paused
            && self.killable == other.killable
    }
}

/// The user's live sessions hosted ELSEWHERE, newest start first.
///
/// The device filter is what makes this correct for the CLI daemon: the
/// daemon registers its own `device_id` even when it runs on this very
/// machine, so its runs are remote to the IDE — which is exactly right, the
/// IDE has no terminal tab for them. Pure (unit-tested).
fn remote_session_rows<'a>(
    sessions: impl Iterator<Item = &'a domain::rows::CodingSession>,
    user_id: &str,
    own_device_id: &str,
    local_session_ids: &HashSet<String>,
    now_epoch: i64,
) -> Vec<&'a domain::rows::CodingSession> {
    let mut rows: Vec<&domain::rows::CodingSession> = sessions
        .filter(|session| session.user_id.as_deref() == Some(user_id))
        .filter(|session| {
            session
                .device_id
                .as_deref()
                .is_some_and(|device_id| device_id != own_device_id)
        })
        .filter(|session| !local_session_ids.contains(&session.id))
        .filter(|session| crate::queries::coding_session_is_live(session, now_epoch))
        .collect();
    // Newest first (web `useAgentsData`); the id is the tiebreak so the strip
    // order is stable across renders.
    rows.sort_by(|a, b| {
        b.started_at
            .cmp(&a.started_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    rows
}

/// The chip's subject line (web `sessionIdentity`): the issue title, the
/// action name, else "Batch".
fn remote_chip_title(
    session: &domain::rows::CodingSession,
    issue: Option<&domain::rows::Issue>,
) -> SharedString {
    if let Some(issue) = issue {
        let title = issue.title.trim();
        return SharedString::from(if title.is_empty() {
            "Untitled issue".to_string()
        } else {
            title.to_string()
        });
    }
    if session.issue_id.is_some() {
        return SharedString::from("Issue syncing…");
    }
    match session.action_name.as_deref() {
        Some(name) if !name.trim().is_empty() => SharedString::from(name.to_string()),
        _ => SharedString::from("Batch"),
    }
}

/// The chip dot's tone, mirroring the web tab's dot rules.
fn remote_chip_tone(display: CodingSessionDisplay, paused: bool, cx: &App) -> gpui::Hsla {
    if paused {
        return cx.theme().muted_foreground.opacity(0.4);
    }
    match display {
        CodingSessionDisplay::NeedsInput => theme::tokens::YELLOW.to_hsla(),
        CodingSessionDisplay::Done => theme::tokens::BLUE.to_hsla(),
        CodingSessionDisplay::Review | CodingSessionDisplay::Running => {
            theme::tokens::GREEN.to_hsla()
        }
    }
}

/// The chip X's confirm, sharing the web's `useKillSession` copy.
fn prompt_kill_remote(
    session_id: String,
    device: Option<SharedString>,
    window: &mut Window,
    cx: &mut App,
) {
    let spec = crate::native_dialog::AlertSpec::new(
        "Kill this coding session?",
        crate::steer_viewer::kill_description(device.as_deref()),
        "Kill session",
    )
    .ok_variant(gpui_component::button::ButtonVariant::Danger)
    .on_ok(move |_, cx| {
        crate::steer_viewer::kill_session(&session_id, cx);
        true
    });
    crate::native_dialog::open_alert(window, cx, spec);
}

/// EXP-696: open `session_id`'s steering view in this window's bottom dock —
/// the routine both the chip click and the issue-detail "coding now" pill
/// ride. A session this process HOSTS focuses its terminal tab instead (there
/// is nothing to steer remotely about a run whose PTY is right here).
pub(crate) fn open_steer_session(session_id: &str, window: &mut Window, cx: &mut App) {
    let Some(panel) = crate::coding_flow::window_terminal_dock(window, cx) else {
        return;
    };
    let local_tab = crate::coding_flow::LocalSessions::global_ref(cx).and_then(|sessions| {
        let sessions = sessions.read(cx);
        sessions
            .session_by_id(session_id)
            .map(|session| session.tab)
    });
    let session_id = session_id.to_string();
    panel.update(cx, |panel, cx| match local_tab {
        Some(tab) => {
            panel.active_steer = None;
            let Some(ix) = panel
                .manager
                .read(cx)
                .tabs()
                .iter()
                .position(|candidate| candidate.id == tab)
            else {
                return;
            };
            panel.expand_dock(window, cx);
            panel.activate_tab(ix, window, cx);
        }
        None => panel.activate_steer(&session_id, window, cx),
    });
}

/// One entry of the (local tabs + remote sessions) strip. Local chips come
/// first, in manager order; remote ones follow, newest run first.
enum StripEntry<'a> {
    Local(&'a TabMeta),
    Remote(&'a RemoteChip),
}

/// Measured width of a remote chip, mirroring its layout piece for piece the
/// way [`measure_tab_chip_width`] mirrors a local one (EXP-326: "fits" means
/// fits, so labels are SHAPED, never guessed).
fn measure_remote_chip_width(chip: &RemoteChip, window: &Window) -> f32 {
    // Memoized per chip: the labels only change when the chip is rebuilt,
    // and the rem size is the one thing that can move under a live chip.
    let rem_bits = f32::from(window.rem_size()).to_bits();
    if let Some((bits, width)) = chip.measured.get() {
        if bits == rem_bits {
            return width;
        }
    }
    let width = shape_remote_chip_width(chip, window);
    chip.measured.set(Some((rem_bits, width)));
    width
}

/// The measurement itself (see [`measure_remote_chip_width`], which memoizes
/// it).
fn shape_remote_chip_width(chip: &RemoteChip, window: &Window) -> f32 {
    /// `surface::rich_tab`'s `px_2p5`, both sides.
    const CHIP_PADDING_REMS: f32 = 0.625 * 2.;
    /// The `size_1p5` status dot.
    const DOT_REMS: f32 = 0.375;
    /// An icon-only xsmall `Button` — `size_5` (the chip's kill button).
    const XSMALL_BUTTON_REMS: f32 = 1.25;
    const TITLE_MAX_W: f32 = crate::surface::RICH_TAB_TITLE_MAX_W;
    const DEVICE_MAX_W: f32 = crate::surface::RICH_TAB_CAPTION_MAX_W;

    let rem = f32::from(window.rem_size());
    let base_font = window.text_style().font();
    let mut children: Vec<f32> = vec![DOT_REMS * rem];
    if let Some(identifier) = chip.identifier.as_ref() {
        let mut mono = base_font.clone();
        mono.family = theme::terminal::FONT_FAMILY.into();
        children.push(crate::screens::measure_text(
            window,
            identifier,
            mono,
            gpui::rems(0.75),
        ));
    }
    children.push(
        crate::screens::measure_text(window, &chip.title, base_font.clone(), gpui::rems(0.875))
            .min(TITLE_MAX_W),
    );
    if let Some(device) = chip.device.as_ref() {
        children.push(
            crate::screens::measure_text(
                window,
                &format!(" · {device}"),
                base_font.clone(),
                gpui::rems(0.75),
            )
            .min(DEVICE_MAX_W),
        );
    }
    if chip.killable {
        children.push(XSMALL_BUTTON_REMS * rem);
    }
    let gaps =
        crate::screens::rich_tab_child_gap(window) * children.len().saturating_sub(1) as f32;
    CHIP_PADDING_REMS * rem + gaps + children.into_iter().sum::<f32>()
}

impl Panel for TerminalDockPanel {
    fn panel_name(&self) -> &'static str {
        PANEL_NAME
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        "Terminal"
    }

    /// Fixed chrome: the dock collapses via the Dock toggle, tabs close via
    /// their own close buttons — the panel itself is not closable.
    fn closable(&self, _cx: &App) -> bool {
        false
    }

    fn zoomable(&self, _cx: &App) -> Option<PanelControl> {
        None
    }

    /// Focus the active terminal when the dock panel becomes active (tab
    /// click on the outer `TabPanel` / dock re-open). Always notify: the Dock
    /// caches this panel's element, and collapse/expand arrives here (via
    /// `DockItem::set_collapsed`) — without the notify the collapsed strip /
    /// full content swap would not repaint.
    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if active {
            // EXP-696: whatever the dock SHOWS takes the keyboard — a steered
            // session's composer, else the active terminal.
            self.focus_visible_content(window, cx);
        }
        cx.notify();
    }

    /// EXP-301: the dock persists NOTHING about its tabs — a relaunch must
    /// never put a terminal in the user's face. `PanelState::new` still carries
    /// the registry name (+ the default `PanelInfo::Panel`), so a saved layout
    /// rehydrates an EMPTY dock panel in the right slot.
    fn dump(&self, _cx: &App) -> PanelState {
        PanelState::new(self)
    }
}

impl gpui::EventEmitter<PanelEvent> for TerminalDockPanel {}

impl Focusable for TerminalDockPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalDockPanel {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Snapshot the strip so the manager borrow ends before listeners.
        // EXP-65: undocked tabs render in their own windows — the strip
        // skips them, and the active view is only painted here when the
        // active tab is NOT undocked (one window paints a view at a time).
        let (metas, active_id, active_view, active_exit): (
            Vec<TabMeta>,
            Option<TabId>,
            Option<Entity<TerminalView>>,
            Option<i32>,
        ) = {
            let manager = self.manager.read(cx);
            let metas = manager
                .tabs()
                .iter()
                .enumerate()
                .filter(|(_, tab)| !crate::undock::is_terminal_tab_undocked(tab.id, cx))
                .map(|(manager_ix, tab)| TabMeta {
                    manager_ix,
                    id: tab.id,
                    title: tab.title().clone(),
                    exit_code: tab.exit_code(),
                    issue: issue_tab_meta(tab.id, cx),
                })
                .collect();
            let active = manager
                .active_tab()
                .filter(|tab| !crate::undock::is_terminal_tab_undocked(tab.id, cx));
            (
                metas,
                active.map(|tab| tab.id),
                active.map(|tab| tab.view.clone()),
                active.and_then(|tab| tab.exit_code()),
            )
        };
        let tab_count = self.manager.read(cx).len();
        // EXP-696: the user's live sessions on OTHER machines ride the strip
        // after the local tabs; the active one takes the content area. The
        // list is the CACHED projection (rebuilt on its deltas + the poll
        // clock) — never rebuilt per repaint.
        let remote = self.remote_chips.clone();
        let active_steer = self
            .active_steer
            .clone()
            .filter(|id| remote.iter().any(|chip| chip.session_id == *id))
            .and_then(|id| self.steer_views.get(&id).cloned());
        let selected_ix = match self.active_steer.as_deref().filter(|_| active_steer.is_some()) {
            Some(active) => remote
                .iter()
                .position(|chip| chip.session_id == active)
                .map(|pos| metas.len() + pos)
                .unwrap_or(0),
            None => active_id
                .and_then(|id| metas.iter().position(|meta| meta.id == id))
                .unwrap_or(0),
        };

        // EXP-688: the strip is ABSOLUTE at the bottom edge and the content
        // fills the band above it, so opening the dock grows the content
        // upward out of the tabs (the web dock's behaviour) instead of
        // pushing them down.
        //
        // EXP-523: `dock_slide.is_none()` holds the collapsed branch back
        // while a CLOSE animation runs — `collapse_dock` deliberately does
        // not flip `set_open` until it settles, so the content stays
        // rendered for the whole slide. On open the flip happens up front,
        // so this is already false on frame 1.
        let collapsed = self.dock_collapsed(cx) && self.dock_slide.is_none();
        let root = div()
            .id("terminal-dock-clip")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_new_tab))
            .on_action(cx.listener(Self::on_close_tab))
            .on_action(cx.listener(Self::on_next_tab))
            .on_action(cx.listener(Self::on_prev_tab))
            .relative()
            .size_full()
            .overflow_hidden();

        let content: Option<AnyElement> = if collapsed {
            None
        } else {
            let body = v_flex().w_full().overflow_hidden();
            Some(match (active_steer, active_view) {
                // EXP-696: a steered session owns the whole content area —
                // no exit strip (there is no local child to exit).
                //
                // EXP-698: it DOES get the Latest-changes bar and the Merge
                // pill. The old rationale ("the diff is on the other
                // machine") was stale: the host publishes its worktree diff
                // on the activity channel, so the viewer has it — and the
                // merge target resolves off the synced row exactly as the
                // web and iOS session views resolve theirs.
                (Some(view), _) => {
                    let session_id = self.active_steer.clone();
                    let bar = session_id
                        .as_deref()
                        .and_then(|id| self.render_steer_changes_bar(id, cx));
                    self.pin_content(
                        body.child(div().flex_1().min_h_0().child(view)).children(bar),
                    )
                }
                (None, Some(active_view)) => self.pin_content(
                    body
                        // min_h(0) so the flex child can shrink with the
                        // dock; the grid element itself guards the 0-height
                        // collapsed case (§6.9).
                        .child(div().flex_1().min_h_0().child(active_view))
                        .when_some(active_exit, |this, code| this.child(exit_strip(code, cx)))
                        .children(active_id.and_then(|id| self.render_changes_bar(id, cx))),
                ),
                // Tabs exist but none is visible/active here — every one is
                // undocked (or the active tab just popped out mid-frame).
                (None, None) if tab_count > 0 => {
                    self.pin_content(body.child(self.render_undocked_hint(cx)))
                }
                // EXP-369: an expanded, empty dock offers its launch cards —
                // nothing spawns until the user picks something.
                (None, None) => {
                    self.pin_content(body.child(self.render_empty_dock_options(window, cx)))
                }
            })
        };

        root.children(content)
            .child(self.render_strip(&metas, &remote, selected_ix, collapsed, window, cx))
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;
    use gpui_component::dock::{DockAreaState, PanelInfo};

    use super::*;

    /// EXP-688: the bar's `+adds -dels` counts the WHOLE snapshot, not the
    /// first file.
    #[test]
    fn changes_totals_sum_every_file() {
        let file = |additions, deletions| coding::scm::DiffFile {
            path: "f".to_string(),
            previous_path: None,
            status: coding::scm::FileStatus::Modified,
            additions,
            deletions,
            hunks: Vec::new(),
            binary: false,
        };
        assert_eq!(changes_totals(&[]), (0, 0));
        assert_eq!(
            changes_totals(&[file(3, 1), file(0, 7), file(10, 0)]),
            (13, 8)
        );
    }

    /// A momentarily empty answer (mid-rebase, mid-checkout) keeps the last
    /// real diff — blanking the bar there is what left the Merge button
    /// standing alone.
    #[test]
    fn an_empty_snapshot_keeps_the_last_non_empty_diff() {
        let file = |path: &str| coding::scm::DiffFile {
            path: path.to_string(),
            previous_path: None,
            status: coding::scm::FileStatus::Modified,
            additions: 1,
            deletions: 0,
            hunks: Vec::new(),
            binary: false,
        };
        let previous = vec![file("a.rs")];
        // A failed poll keeps the previous answer.
        let kept = merge_changes_snapshot(previous.clone(), None);
        assert_eq!(kept, previous);
        // A real answer always wins, even a smaller one.
        let next = vec![file("b.rs")];
        assert_eq!(merge_changes_snapshot(previous.clone(), Some(next.clone())), next);
        // A real EMPTY answer clears the bar (the branch was reset).
        assert!(merge_changes_snapshot(previous, Some(Vec::new())).is_empty());
        // Nothing either way is still nothing.
        assert!(merge_changes_snapshot(Vec::new(), None).is_empty());
    }

    /// The bar renders for a diff OR an open PR — and for neither it is not
    /// painted at all (a shell tab has no session to describe).
    #[test]
    fn changes_bar_shows_for_diff_or_open_pr() {
        assert!(changes_bar_visible(true, false));
        assert!(changes_bar_visible(false, true));
        assert!(changes_bar_visible(true, true));
        assert!(!changes_bar_visible(false, false));
    }

    /// EXP-498: the batch tab's merge target — any synced OPEN-PR issue on
    /// the session's branch; nothing else qualifies.
    #[test]
    fn open_pr_issue_on_branch_picks_only_open_prs_on_the_branch() {
        let issue = |id: &str,
                     branch: Option<&str>,
                     pr_state: Option<&str>|
         -> domain::rows::Issue {
            serde_json::from_value(serde_json::json!({
                "id": id, "board_id": "b-1", "number": 1,
                "identifier": "EXP-1", "title": "t", "status": "in_review",
                "branch": branch, "pr_state": pr_state,
            }))
            .unwrap()
        };
        let open = issue("i-open", Some("exp/batch-a1b2c3d4"), Some("open"));
        let merged = issue("i-merged", Some("exp/batch-a1b2c3d4"), Some("merged"));
        let other = issue("i-other", Some("exp/EXP-9"), Some("open"));
        let branchless = issue("i-none", None, Some("open"));

        let found = open_pr_issue_on_branch(
            "exp/batch-a1b2c3d4",
            [&merged, &other, &open, &branchless].into_iter(),
        );
        assert_eq!(found.map(|issue| issue.id.as_str()), Some("i-open"));
        assert!(open_pr_issue_on_branch(
            "exp/batch-a1b2c3d4",
            [&merged, &other, &branchless].into_iter()
        )
        .is_none());
        // Trunk/scratch sessions record no branch — never a merge target.
        assert!(open_pr_issue_on_branch("", [&open].into_iter()).is_none());
    }

    /// A real pre-EXP-301 `window-0.json`: an OPEN bottom dock whose panel
    /// info still carries a persisted login shell and a claude tab. Old files
    /// like this stay on disk after the upgrade, so the restore path has to
    /// stay immune to them — not merely stop writing them.
    const LEGACY_LAYOUT: &str = r#"{
      "version": 8,
      "center": { "panel_name": "Center", "children": [], "info": { "panel": null } },
      "bottom_dock": {
        "panel": {
          "panel_name": "TerminalDock",
          "children": [],
          "info": {
            "panel": {
              "tabs": [
                { "kind": "shell", "cwd": "/tmp" },
                { "kind": "claude", "cwd": "/tmp" }
              ],
              "active": 1
            }
          }
        },
        "placement": "bottom",
        "size": 547.9297,
        "open": true
      }
    }"#;

    /// EXP-301: opening the app must never put a terminal in the user's face.
    /// Rehydrating a saved layout builds an EMPTY dock panel — no PTY is
    /// spawned for a persisted `shell` tab (nor for the legacy `run` kind),
    /// and the panel re-dumps without any tab payload.
    #[gpui::test]
    async fn a_saved_layout_rehydrates_the_dock_with_zero_terminals(cx: &mut TestAppContext) {
        cx.update(|cx| {
            // Same order as `app::main` — the component/theme globals must
            // exist before a layout can rehydrate.
            gpui_component::init(cx);
            theme::init(cx);
            init(cx);
        });

        let state: DockAreaState = serde_json::from_str(LEGACY_LAYOUT).expect("parse layout");
        let window = cx.add_window(|window, cx| DockArea::new("test-dock", Some(8), window, cx));

        window
            .update(cx, |dock_area, window, cx| {
                dock_area.load(state, window, cx).expect("load layout");

                let dock = dock_area
                    .bottom_dock()
                    .cloned()
                    .expect("bottom dock restored");
                let item = dock.read(cx).panel().clone();
                let panel = crate::coding_flow::find_terminal_dock(&item)
                    .expect("terminal dock panel rehydrated");

                assert!(
                    panel.read(cx).manager().read(cx).is_empty(),
                    "a restored layout must not respawn terminal tabs"
                );

                // The dump carries the registry name only — nothing to
                // resurrect on the NEXT launch either.
                let dumped = panel.read(cx).dump(cx);
                assert_eq!(dumped.panel_name, PANEL_NAME);
                assert_eq!(dumped.info, PanelInfo::Panel(serde_json::Value::Null));
            })
            .expect("window update");
    }

    // ── EXP-523: the bottom dock's open/close slide ─────────────────────────
    //
    // Pure state-machine tests, like `shell.rs`'s `LeftColumnAnim` ones: no
    // gpui context, synthetic `Instant`s. `Instant::now()` is real under the
    // test executor, so the timer loop itself is not driveable — the contract
    // that matters is the arithmetic these pin down.

    fn slide_at(offset_ms: u64, base: Instant) -> Instant {
        base + Duration::from_millis(offset_ms)
    }

    #[test]
    fn slide_progress_clamps_at_both_ends_and_settles() {
        let t0 = Instant::now();
        let slide = DockSlide::new(DOCK_STRIP_H, 240., 240., true, t0);

        assert_eq!(slide.virtual_height(t0), DOCK_STRIP_H);
        assert!(!slide.done(t0));

        let end = slide_at(DOCK_SLIDE_DURATION.as_millis() as u64, t0);
        assert!(slide.done(end));
        assert!((slide.virtual_height(end) - 240.).abs() < 0.01);
        // Past the end it pins rather than overshooting.
        let past = slide_at(DOCK_SLIDE_DURATION.as_millis() as u64 + 500, t0);
        assert!((slide.virtual_height(past) - 240.).abs() < 0.01);
    }

    #[test]
    fn slide_stays_inside_its_endpoints_the_whole_way() {
        let t0 = Instant::now();
        let slide = DockSlide::new(240., DOCK_STRIP_H, 240., false, t0);
        let total = DOCK_SLIDE_DURATION.as_millis() as u64;
        let mut previous = slide.virtual_height(t0);
        for step in 1..=20u64 {
            let v = slide.virtual_height(slide_at(total * step / 20, t0));
            assert!(v <= previous + 0.01, "a close must never grow: {previous} -> {v}");
            assert!((DOCK_STRIP_H - 0.01..=240.01).contains(&v), "out of range: {v}");
            previous = v;
        }
    }

    // The clip tracks what the tick REQUESTED, not what `Dock::set_size`
    // stored — that is what keeps the panel's visible edge moving through
    // the region upstream refuses to store (PANEL_MIN_SIZE).
    #[test]
    fn content_clip_height_tracks_the_virtual_height_not_the_clamp() {
        let t0 = Instant::now();
        let mut slide = DockSlide::new(240., DOCK_STRIP_H, 240., false, t0);

        // Asked for 29, upstream stored 100: the clip is still 0 — the strip
        // alone is showing, exactly as 29 asks for.
        slide.record_apply(DOCK_STRIP_H, 100.);
        assert_eq!(slide.content_clip_height(), 0.);

        // Above the floor the clip is the height minus the strip.
        slide.record_apply(180., 180.);
        assert!((slide.content_clip_height() - 151.).abs() < 0.01);

        // Never negative, whatever upstream does.
        slide.record_apply(10., 100.);
        assert_eq!(slide.content_clip_height(), 0.);
    }

    // `LeftColumnAnim` jumps to its previous target on a mid-flight reversal
    // (documented upstream limitation). This one has a real scalar, so it can
    // do better — and must, or reopening a half-closed dock visibly snaps.
    #[test]
    fn retarget_restarts_from_the_current_height_not_the_old_target() {
        let t0 = Instant::now();
        let mut slide = DockSlide::new(DOCK_STRIP_H, 240., 240., true, t0);
        let half = slide_at(DOCK_SLIDE_DURATION.as_millis() as u64 / 2, t0);
        let mid = slide.virtual_height(half);
        assert!(mid > DOCK_STRIP_H && mid < 240., "midpoint should be in flight: {mid}");

        let epoch = slide.retarget(half, DOCK_STRIP_H, false);
        assert_eq!(epoch, 1, "a retarget must bump the epoch");
        assert!(!slide.opening);
        assert!(
            (slide.virtual_height(half) - mid).abs() < 0.01,
            "the reversal must start where the curve actually was"
        );
        assert_eq!(slide.to, DOCK_STRIP_H);
    }

    // The regression this guards: `settle` restoring `to` instead of
    // `rest_height` would persist the 29px (or the clamped 100px) as the
    // user's dock height, silently shrinking every future open.
    #[test]
    fn a_close_keeps_the_users_resting_height() {
        let t0 = Instant::now();
        let mut slide = DockSlide::new(312., DOCK_STRIP_H, 312., false, t0);
        slide.retarget(slide_at(40, t0), DOCK_STRIP_H, false);
        assert_eq!(slide.rest_height, 312.);
        assert_eq!(slide.to, DOCK_STRIP_H);
    }

    // A stale timer loop from a superseded leg must not drive the new one.
    #[test]
    fn epochs_identify_the_leg_a_tick_belongs_to() {
        let t0 = Instant::now();
        let mut slide = DockSlide::new(DOCK_STRIP_H, 240., 240., true, t0);
        assert_eq!(slide.epoch, 0);
        let first = slide.retarget(slide_at(20, t0), DOCK_STRIP_H, false);
        let second = slide.retarget(slide_at(40, t0), 240., true);
        assert_eq!((first, second), (1, 2));
        assert_ne!(slide.epoch, first, "the older loop's epoch is now stale");
    }

    // Drag detection: upstream's `resizing` flag is private, so the slide
    // notices the user grabbing the handle by the dock's height no longer
    // matching what its own last `set_size` stored.
    #[test]
    fn an_external_height_write_is_detectable_from_the_readback() {
        let t0 = Instant::now();
        let mut slide = DockSlide::new(240., DOCK_STRIP_H, 240., false, t0);
        slide.record_apply(200., 200.);

        let ours = 200.;
        assert!((ours - slide.applied).abs() <= 0.5, "our own write is not a collision");
        let dragged = 264.;
        assert!((dragged - slide.applied).abs() > 0.5, "a drag must be detected");
    }

    // ── EXP-696: remote session chips ──────────────────────────────────────

    /// Rows are heartbeat-dated so `coding_session_is_live` keeps them:
    /// 2026-07-17T12:00:00Z, beating a minute ago.
    const NOW: i64 = 1784289600;

    fn remote_row(
        id: &str,
        user_id: &str,
        device_id: Option<&str>,
        started_at: &str,
    ) -> domain::rows::CodingSession {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "issue_id": "issue-1",
            "user_id": user_id,
            "device_id": device_id,
            "status": "running",
            "started_at": started_at,
            "updated_at": "2026-07-17T11:59:00Z",
        }))
        .unwrap()
    }

    /// Only the caller's own live rows, hosted somewhere that is not this
    /// install, newest run first.
    #[test]
    fn remote_chips_keep_only_other_devices_own_live_rows() {
        let rows = vec![
            remote_row("mine-old", "me", Some("laptop"), "2026-07-17T10:00:00Z"),
            remote_row("mine-new", "me", Some("server"), "2026-07-17T11:00:00Z"),
            remote_row("here", "me", Some("this-ide"), "2026-07-17T11:30:00Z"),
            remote_row("theirs", "someone", Some("laptop"), "2026-07-17T11:45:00Z"),
        ];
        let picked = remote_session_rows(
            rows.iter(),
            "me",
            "this-ide",
            &HashSet::new(),
            NOW,
        );
        let ids: Vec<&str> = picked.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, vec!["mine-new", "mine-old"]);
    }

    /// A row this process HOSTS already has a terminal tab — it must never
    /// also grow a steer chip (belt-and-braces next to the device filter).
    #[test]
    fn remote_chips_skip_sessions_this_process_hosts() {
        let rows = vec![remote_row(
            "sess-1",
            "me",
            Some("laptop"),
            "2026-07-17T11:00:00Z",
        )];
        let local: HashSet<String> = ["sess-1".to_string()].into_iter().collect();
        assert!(remote_session_rows(rows.iter(), "me", "this-ide", &local, NOW).is_empty());
    }

    /// An ENDED or stale row drops off the strip entirely (web parity: a
    /// stale run renders as absent, never as a dead tab).
    #[test]
    fn remote_chips_drop_ended_and_stale_rows() {
        let ended: domain::rows::CodingSession = serde_json::from_value(serde_json::json!({
            "id": "ended",
            "user_id": "me",
            "device_id": "laptop",
            "status": "ended",
            "updated_at": "2026-07-17T11:59:00Z",
        }))
        .unwrap();
        let stale: domain::rows::CodingSession = serde_json::from_value(serde_json::json!({
            "id": "stale",
            "user_id": "me",
            "device_id": "laptop",
            "status": "running",
            // 3h old — past the 2h contract window.
            "updated_at": "2026-07-17T09:00:00Z",
        }))
        .unwrap();
        let rows = vec![ended, stale];
        assert!(remote_session_rows(rows.iter(), "me", "this-ide", &HashSet::new(), NOW).is_empty());
    }

    /// A pre-EXP-549 row with no `device_id` cannot be proven to live
    /// elsewhere — it stays off the strip rather than claiming to be remote.
    #[test]
    fn remote_chips_ignore_rows_without_a_device() {
        let rows = vec![remote_row("sess-1", "me", None, "2026-07-17T11:00:00Z")];
        assert!(remote_session_rows(rows.iter(), "me", "this-ide", &HashSet::new(), NOW).is_empty());
    }

    /// The subject line falls back the way the web `sessionIdentity` does.
    #[test]
    fn the_chip_title_names_the_action_batch_or_syncing_issue() {
        let mut row = remote_row("sess-1", "me", Some("laptop"), "2026-07-17T11:00:00Z");
        assert_eq!(remote_chip_title(&row, None).as_ref(), "Issue syncing…");
        row.issue_id = None;
        assert_eq!(remote_chip_title(&row, None).as_ref(), "Batch");
        row.action_name = Some("Nightly triage".to_string());
        assert_eq!(remote_chip_title(&row, None).as_ref(), "Nightly triage");
    }
}
