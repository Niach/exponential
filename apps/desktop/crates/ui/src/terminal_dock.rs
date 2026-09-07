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
//! - **bubble** (EXP-742): the collapsed dock has a second form — a glass
//!   card floating in the cutout panel's bottom-right corner that carries
//!   the entry count, an aggregate status dot and the strip's own rich-tab
//!   chips (`render_bubble`, painted by `Shell` OVER the panel because the
//!   upstream `Dock` clips its closed band to 29px). The open dock's header
//!   toggles which form a collapse lands on; the pick persists per device
//!   (`Settings::terminal_dock_bubble`). In the bubble form the 29px band
//!   paints nothing at all and reads as the panel's bottom padding.
//!
//! **PTY tabs, and nothing else (EXP-746).** The dock used to carry a second
//! chip kind: a "remote chip" per live run on another machine, swapping the
//! terminal grid for a steering view. Sessions have their own center screen
//! now ([`crate::session_screen`]), so every run — hosted here over ACP, on
//! another machine, or finished — opens there, and the dock is back to the
//! terminals it owns. [`reveal_pty_tab`] is the one seam left: the session
//! opener calls it for a run that IS a terminal here.
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
    v_flex, ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _,
    WindowExt as _,
};
use gpui::Task;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use terminal::{TabId, TabKind, TerminalManager, TerminalManagerEvent, TerminalView};

use crate::changes_bar;
use crate::coding_flow::{CodingHub, TokenRefreshers};
use crate::icons::{registry, ExpIcon};
use crate::navigation;
use crate::queries::CodingSessionDisplay;
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup, RepoResolver};

/// Stable serialization name for the panel registry (§3.3: never change it).
pub const PANEL_NAME: &str = "TerminalDock";

/// EXP-523: the bottom dock slides open and shut instead of snapping. The
/// duration is the shared `standard` motion token, the same one the left
/// column's rail-to-settings swap uses.
const DOCK_SLIDE_DURATION: Duration = theme::motion::STANDARD;

/// Upstream `Dock::render`'s CLOSED height — the toggle strip it keeps when
/// `open == false`. The slide's closed endpoint.
pub(crate) const DOCK_STRIP_H: f32 = 29.;

/// EXP-723: height of the OPEN dock's own header row — the web dock's shape,
/// carrying the window/collapse controls above the content while the bottom
/// strip keeps the tabs alone.
const DOCK_HEADER_H: f32 = 28.;

/// EXP-742: the bubble's inset from the cutout panel's bottom and right
/// edges — the same 10px the panel keeps from the window (`shell.rs`
/// `PANEL_MARGIN`), so the bubble sits in the corner the way the panel sits
/// in the window.
const BUBBLE_INSET: f32 = 10.;

/// EXP-742: how many rich-tab chips the bubble carries before folding the
/// rest into a "+N" count. The SELECTED entry is always one of them.
const BUBBLE_MAX_CHIPS: usize = 3;

/// EXP-688: how often the Latest-changes bar re-reads the branch diff. The
/// same 3s cadence the steer emitter's `DiffSnapshots` publishes on. While
/// there is nothing to read (collapsed dock, no session tab) the loop idles
/// on the shorter beat instead — it runs no git at all there, and the bar
/// then appears within a blink of the dock opening rather than 3s later.
const CHANGES_POLL: Duration = Duration::from_secs(3);
const CHANGES_IDLE_POLL: Duration = Duration::from_millis(500);

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
    /// The 3s poll behind [`Self::changes`]. Lives as long as the panel; it
    /// only shells out to git while the dock is OPEN on a session tab.
    _changes_poll: Task<()>,
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
            cx.observe(&collections.issues, |_, _, cx| cx.notify()).detach();
            cx.observe(&collections.issue_statuses, |_, _, cx| cx.notify())
                .detach();
            cx.observe(&collections.boards, |_, _, cx| cx.notify()).detach();
            // A tab's own row carries its merge target and, for the bubble
            // dot, its `needs_input` edge.
            cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify())
                .detach();
        }
        let local_sessions = crate::coding_flow::LocalSessions::global(cx);
        // Which tab is a coding session (and on which branch) is the local
        // registry's answer, and the chips read it.
        cx.observe(&local_sessions, |_, _, cx| cx.notify()).detach();
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
        let changes_poll = cx.spawn_in(window, async move |this, window| loop {
            let Ok(job) = this.update_in(window, |this, _, cx| this.changes_job(cx)) else {
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
            _changes_poll: changes_poll,
            dock_slide: None,
            _dock_slide_task: None,
            _subscription: subscription,
        }
    }

    /// EXP-742: whether this machine's collapsed dock is the floating bubble
    /// (`Settings::terminal_dock_bubble`). A READ-ONLY peek: the shell warms
    /// the hub before the dock's first frame, and a headless panel (tests)
    /// simply keeps the strip.
    fn bubble_preferred(&self, cx: &App) -> bool {
        CodingHub::global_ref(cx)
            .is_some_and(|hub| hub.read(cx).settings.terminal_dock_bubble)
    }

    /// EXP-742: whether the dock band paints NOTHING right now because the
    /// bubble stands in for it — collapsed, at rest, bubble preferred.
    /// Mid-slide the content keeps rendering (EXP-523) and the bubble waits
    /// for the settle, so the two never overlap.
    fn bubble_showing(&self, cx: &App) -> bool {
        self.dock_collapsed(cx) && self.dock_slide.is_none() && self.bubble_preferred(cx)
    }

    /// EXP-760: is the bottom strip painted on the WINDOW GROUND right now?
    /// True whenever the strip renders at all — which is every form but the
    /// bubble ([`Self::bubble_showing`]), where the 29px band paints nothing.
    /// The Shell asks so it can stop the cutout panel's card face
    /// [`DOCK_STRIP_H`] short of its bottom edge
    /// (`shell::panel_backdrop_inset`) and let the chips sit outside it.
    pub(crate) fn strip_on_ground(&self, cx: &App) -> bool {
        !self.bubble_showing(cx)
    }

    /// EXP-742: pick this machine's collapsed form (bubble or strip) and
    /// collapse into it — the header toggle's action. The pick persists per
    /// device through the ui-prefs save (no doctor rerun, no launch-defaults
    /// push), and every later collapse, including the last tab closing,
    /// lands on it.
    fn collapse_into(
        &mut self,
        bubble: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let hub = CodingHub::global(cx);
        let mut settings = hub.read(cx).settings.clone();
        if settings.terminal_dock_bubble != bubble {
            settings.terminal_dock_bubble = bubble;
            CodingHub::save_ui_prefs(&hub, settings, cx);
        }
        self.collapse_dock(window, cx);
        cx.notify();
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
    fn pin_content<E: Styled + IntoElement>(&self, content: E, cx: &App) -> AnyElement {
        // EXP-760: the OPEN dock's body is the bottom of the cutout panel's
        // card now — the card face stops [`DOCK_STRIP_H`] short of the panel
        // edge (`shell::panel_backdrop_inset`) so the tab chips sit on the
        // window ground, and this body closes it off: the opaque `popover`
        // fill, the seam hairline to the content above, and the panel's own
        // bottom radius (gpui's content mask is rectangular — it rounds
        // itself). The root used to carry all three; there they also painted
        // BEHIND the strip.
        let content = content
            .bg(cx.theme().popover)
            .border_t_1()
            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
            .rounded_b(px(theme::tokens::radius::LG))
            .overflow_hidden();
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

    /// Focus what the dock is SHOWING: the active terminal. An empty,
    /// tab-less dock focuses nothing (EXP-369: expanding never starts
    /// anything, so there is no grid to hand the keyboard to).
    fn focus_visible_content(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
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
    fn activate_visible_step(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let visible = self.visible_indices(cx);
        let len = visible.len();
        if len == 0 {
            return;
        }
        let current_pos = self
            .manager
            .read(cx)
            .active_index()
            .and_then(|active| visible.iter().position(|ix| *ix == active));
        let next_pos = match current_pos {
            Some(pos) if forward => (pos + 1) % len,
            Some(pos) => (pos + len - 1) % len,
            None => 0,
        };
        self.activate_tab(visible[next_pos], window, cx);
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

    fn on_close_tab(
        &mut self,
        _: &CloseTerminalTab,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
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
    /// TABS, and nothing else (EXP-723): one chip per VISIBLE session (EXP-65:
    /// undocked tabs render in their own windows) with the `+` right after the
    /// last one. An EMPTY strip names itself instead — the terminal glyph plus
    /// the word "Terminal", since no chip is there to. The window/collapse
    /// controls moved up into [`Self::render_dock_header`]. Clicking the
    /// strip's empty space toggles the dock; a chip click activates its tab
    /// (and expands a collapsed dock) — chip/button handlers stop propagation
    /// so their clicks never fall through to the toggle.
    ///
    /// EXP-497: chips that don't fit collapse into a trailing "+N" dropdown —
    /// the center strip's EXP-288 treatment (the scrolled chips this replaces
    /// left overflowing tabs cut off). Chip widths are measured, not guessed
    /// (the EXP-326 lesson), against the recorded [`Self::chips_slot_width`];
    /// the SELECTED tab is always kept visible.
    fn render_strip(
        &self,
        metas: &[TabMeta],
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
        // EXP-739 put the one-click "Chat" button beside it, so the trailing
        // reserve is TWO of them.
        let widths: Vec<f32> = metas
            .iter()
            .map(|meta| measure_tab_chip_width(meta, window))
            .collect();
        let plus_reserve =
            2. * (1.25 * f32::from(window.rem_size()) + crate::screens::chip_gap(window));
        let available = self
            .chips_slot_width
            .map_or(f32::MAX, |slot| (slot - plus_reserve).max(0.));
        let visible = crate::screens::partition_tabs(
            &widths,
            available,
            crate::screens::chip_gap(window),
            crate::screens::overflow_button_width(window, metas.len().saturating_sub(1)),
            (!metas.is_empty()).then_some(selected_ix),
        );
        let hidden: Vec<usize> = (0..metas.len())
            .filter(|ix| !visible.contains(ix))
            .collect();

        let chips: Vec<AnyElement> = visible
            .into_iter()
            .map(|ix| self.render_local_chip(&metas[ix], ix, selected_ix, collapsed, true, cx))
            .collect();
        // EXP-497: the hidden tabs collapse into a "+N" dropdown; clicking
        // one activates it. Keyed by TabId, not strip index — the menu's
        // closures run at click time, and a tab closed while the dropdown is
        // open shifts every index after it (the center strip's EXP-288
        // rationale; the TabId is the stable identity here).
        let overflow_button = (!hidden.is_empty()).then(|| {
            // The menu rows mirror the chips: issue sessions carry the status
            // glyph + "IDENT title", the rest their plain terminal title.
            let hidden_entries: Vec<(TabId, Option<domain::statuses::ResolvedStatus>, SharedString)> =
                hidden
                    .iter()
                    .map(|&ix| {
                        let meta = &metas[ix];
                        match &meta.issue {
                            Some(issue) => {
                                let label = match &issue.title {
                                    Some(title) => SharedString::from(format!(
                                        "{} {title}",
                                        issue.identifier
                                    )),
                                    None => issue.identifier.clone(),
                                };
                                (meta.id, Some(issue.status.clone()), label)
                            }
                            None => (meta.id, None, meta.title.clone()),
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
                    for (id, status, label) in &hidden_entries {
                        let panel = panel.clone();
                        let id = *id;
                        let mut item = PopupMenuItem::new(label.clone());
                        if let Some(status) = status {
                            item = item.icon(crate::icons::resolved_status_icon(status, cx));
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
                                this.activate_tab(ix, window, cx);
                            });
                        }));
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
            // EXP-760: the strip paints NOTHING. It used to be the panel's
            // bottom edge (fill, hairline, bottom radius); now the panel's
            // card face stops above it (`shell::panel_backdrop_inset`) and
            // the chips sit directly on the window's gradient, JetBrains
            // style. Each chip and button is its own glass surface instead.
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                if showing {
                    this.collapse_dock(window, cx);
                } else {
                    this.expand_dock(window, cx);
                    // EXP-369: expanding NEVER starts anything — with zero
                    // sessions the dock opens on its launch cards; with
                    // sessions the active terminal takes the keyboard back.
                    this.focus_visible_content(window, cx);
                }
            }))
            .when(metas.is_empty(), |strip| {
                // EXP-723: the label is the EMPTY strip's whole content — with
                // chips present they name the dock themselves, and the glyph
                // only stole width from them.
                // EXP-760: on the ground it needs its own surface, so the
                // label is a readonly glass pill rather than bare text.
                strip.child(
                    crate::surface::glass_pill(
                        "terminal-strip-empty-label",
                        crate::surface::PillSize::Sm,
                        crate::surface::PillMode::Readonly,
                        cx,
                    )
                    .child(Icon::new(registry::NAV_TERMINAL).xsmall())
                    .child(div().text_xs().child("Terminal")),
                )
            })
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
                    .child(self.new_tab_menu(cx))
                    // EXP-739: the one-click repo-less chat rides beside it.
                    .child(self.chat_button(cx)),
            )
    }

    /// EXP-723: the OPEN dock's header row — web parity (`dock-header` in the
    /// styleguide). The controls that used to sit at the right end of the
    /// bottom strip live here instead, above the content: the strip is tabs
    /// (and the `+`) and nothing else now, so a chip can run the full width.
    ///
    /// Rendered INSIDE the sliding content, so it rides the open/close
    /// animation with the terminal rather than appearing before it.
    ///
    /// `active_tab` is the id of the selected tab; `None` for an empty dock
    /// (nothing to pop out).
    fn render_dock_header(
        &self,
        active_tab: Option<TabId>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        h_flex()
            .h(px(DOCK_HEADER_H))
            .flex_shrink_0()
            .px_2()
            .gap_0p5()
            .items_center()
            .border_b_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            .child(div().flex_1().min_w_0())
            // EXP-688: undock is ONE button on the ACTIVE tab (it used to be
            // a hover affordance on every chip).
            .child(
                Button::new("undock-active-terminal-tab")
                    .ghost().cursor_pointer()
                    .xsmall()
                    .icon(registry::UI_UNDOCK)
                    .tooltip("Open in new window")
                    .disabled(active_tab.is_none())
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        if let Some(id) = active_tab {
                            this.undock_tab(id, window, cx);
                        }
                    })),
            )
            // EXP-742: which form the collapse lands on. A two-way switch:
            // it flips the per-device pick AND collapses, so "Collapse to a
            // bubble" is one click and the way back to the strip is the same
            // button reading "Collapse to the strip" — selected while the
            // bubble is the pick. Desktop-only chrome, so the raw glyph is
            // fine (the `action-chat` concept is the Actions page's, not this).
            .child({
                let bubble = self.bubble_preferred(cx);
                Button::new("terminal-dock-collapsed-form")
                    .ghost().cursor_pointer()
                    .xsmall()
                    .icon(ExpIcon::MessageCircle)
                    .selected(bubble)
                    .tooltip(if bubble {
                        "Collapse to the strip"
                    } else {
                        "Collapse to a bubble"
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.collapse_into(!bubble, window, cx);
                    }))
            })
            .child(
                Button::new("collapse-terminal-dock")
                    .ghost().cursor_pointer()
                    .xsmall()
                    .icon(registry::UI_CHEVRON_DOWN)
                    .tooltip("Hide terminal")
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.collapse_dock(window, cx);
                    })),
            )
            .into_any_element()
    }

    /// The strip's LOCAL entries — the manager's tabs minus the undocked
    /// ones (EXP-65: those render in their own windows), in manager order.
    fn visible_tab_metas(&self, cx: &App) -> Vec<TabMeta> {
        self.manager
            .read(cx)
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
            .collect()
    }

    /// The strip index of the ACTIVE tab. 0 when nothing matches (an empty
    /// strip has nothing to select).
    fn selected_tab_ix(metas: &[TabMeta], active_id: Option<TabId>) -> usize {
        active_id
            .and_then(|id| metas.iter().position(|meta| meta.id == id))
            .unwrap_or(0)
    }

    /// EXP-742: the collapsed dock's BUBBLE — `None` unless it is showing
    /// ([`Self::bubble_showing`]).
    ///
    /// Painted by the SHELL over the cutout panel, not by this panel: the
    /// upstream `Dock` clips its closed band to a hard 29px, so nothing the
    /// panel renders can float above it. The listeners still bind to this
    /// entity, so a click here drives the same `expand_dock` as the strip.
    ///
    /// The card is the glass-card recipe (radius XL, card hairline) on the
    /// OPAQUE popover fill — `surface::glass_bar`'s rule: a floating surface
    /// over content that scrolls under it must not show that content
    /// through (gpui has no in-scene backdrop blur). Inside, left to right:
    /// an aggregate status dot + the entry count (the terminal glyph and
    /// "Terminal" when there is nothing), up to [`BUBBLE_MAX_CHIPS`] of the
    /// strip's own rich-tab chips (selected one guaranteed; the rest fold
    /// into "+N"), and the chevron that opens the dock. The whole card is a
    /// click target; chips keep their own actions (a chip click from a
    /// collapsed dock already expands it).
    pub(crate) fn render_bubble(&mut self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        if !self.bubble_showing(cx) {
            return None;
        }
        let metas = self.visible_tab_metas(cx);
        let active_id = self
            .manager
            .read(cx)
            .active_tab()
            .filter(|tab| !crate::undock::is_terminal_tab_undocked(tab.id, cx))
            .map(|tab| tab.id);
        let selected_ix = Self::selected_tab_ix(&metas, active_id);
        let total = metas.len();
        let visible = bubble_visible_entries(total, selected_ix);
        let hidden = total - visible.len();
        let chips: Vec<AnyElement> = visible
            .into_iter()
            .map(|ix| self.render_local_chip(&metas[ix], ix, selected_ix, true, false, cx))
            .collect();
        let signal = metas
            .iter()
            .map(|meta| local_tab_signal(meta, cx))
            .max()
            .unwrap_or(BubbleSignal::Quiet);
        let tone = bubble_tone(signal, cx);
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;

        let summary = h_flex()
            .flex_shrink_0()
            .gap_1p5()
            .items_center()
            .pl_1()
            .map(|cluster| {
                if total == 0 {
                    // EXP-723's empty-strip label, at bubble scale: the
                    // bubble is still the only way to open the dock.
                    cluster
                        .text_color(muted)
                        .child(Icon::new(registry::NAV_TERMINAL).xsmall())
                        .child(div().text_xs().child("Terminal"))
                } else {
                    cluster
                        .child(div().flex_shrink_0().size_1p5().rounded_full().bg(tone))
                        .child(
                            div()
                                .text_xs()
                                .text_color(foreground.opacity(0.7))
                                .font_family(theme::terminal::FONT_FAMILY)
                                .child(total.to_string()),
                        )
                }
            });

        let open = |this: &mut Self, window: &mut Window, cx: &mut gpui::Context<Self>| {
            this.expand_dock(window, cx);
            // EXP-369: expanding NEVER starts anything — with zero sessions
            // the dock opens on its launch cards.
            this.focus_visible_content(window, cx);
        };
        Some(
            crate::surface::glass_card()
                .id("terminal-dock-bubble")
                .absolute()
                .bottom(px(BUBBLE_INSET))
                .right(px(BUBBLE_INSET))
                .flex_row()
                .items_center()
                .gap_1()
                .pl_1p5()
                .pr_1()
                .py_1()
                .bg(cx.theme().popover)
                .shadow_md()
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    open(this, window, cx);
                }))
                .child(summary)
                .children(chips)
                .when(hidden > 0, |card| {
                    card.child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .px_1()
                            .child(format!("+{hidden}")),
                    )
                })
                .child(
                    Button::new("expand-terminal-bubble")
                        .ghost()
                        .cursor_pointer()
                        .xsmall()
                        .icon(registry::UI_CHEVRON_UP)
                        .tooltip("Show terminal")
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            open(this, window, cx);
                        })),
                )
                .into_any_element(),
        )
    }

    /// One terminal tab's chip (EXP-325/EXP-497).
    fn render_local_chip(
        &self,
        meta: &TabMeta,
        ix: usize,
        selected_ix: usize,
        collapsed: bool,
        ground: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let id = meta.id;
        let manager_ix = meta.manager_ix;
        // EXP-760: `ground` for the bottom STRIP's chips (they sit on the
        // window gradient now, so each is its own glass surface); the
        // bubble's are inside a card and keep the plain tab treatment.
        let mut tab =
            crate::surface::RichTab::new(("terminal-tab", ix), ix == selected_ix).ground(ground);
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
            // EXP-723: a plain shell chip gets the terminal glyph too — a
            // chip carrying only a title read as a nameless tab next to the
            // issue chips' status glyphs. `session-shell` (Lucide `terminal`)
            // is the LOCAL-shell concept; `nav-terminal` stays the dock's own
            // navigation glyph.
            None => {
                tab.status =
                    crate::surface::RichTabStatus::Glyph(Icon::new(registry::SESSION_SHELL));
                tab.title = Some(meta.title.clone());
            }
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

    /// The merge button for the ACTIVE session tab whose PR is open (issue
    /// AND batch since EXP-498). EXP-484 moved it off the chip into the
    /// per-tab toolbar: a labeled button never squeezes a title there, and
    /// the chip got its close button back (merging still closes the
    /// session).
    ///
    /// EXP-746 moved the button itself into [`crate::changes_bar`] — every
    /// session surface wears one, and the only thing the dock adds is the
    /// local tab close: the tab closes the moment the merge call fires (the
    /// `TabClosed` watcher fires the idempotent `codingSessions.end`), so a
    /// merge that fails on conflicts never leaves a live session holding the
    /// branch — the Reviews page's "Fix conflicts" recovery starts
    /// immediately instead of parking behind a busy worktree. The server ends
    /// the user's live sessions on OTHER devices after the merge.
    fn tab_close_on_merge(&self, tab: TabId) -> changes_bar::OnMerged {
        let manager = self.manager.downgrade();
        std::rc::Rc::new(move |cx: &mut App| {
            if let Some(manager) = manager.upgrade() {
                manager.update(cx, |manager, cx| manager.close_tab(tab, cx));
            }
        })
    }

    /// EXP-739: the agent a one-click chat launches — the device's configured
    /// `default_agent` when the doctor found it INSTALLED, else the first
    /// installed one (a default pointing at an agent this machine does not
    /// have must not disable the button). `None` while no report exists yet,
    /// or when nothing is runnable at all.
    fn default_chat_agent(cx: &gpui::App) -> Option<coding::CodingAgent> {
        let hub = CodingHub::global_ref(cx)?;
        let hub = hub.read(cx);
        let installed = hub.doctor.report.as_ref()?.installed_agents();
        let preferred = hub.settings.default_agent;
        if installed.contains(&preferred) {
            return Some(preferred);
        }
        installed.first().copied()
    }

    /// EXP-739: the strip's one-click "Chat" — a promptless, repo-LESS chat
    /// run on the device's default agent ([`Self::launch_chat_run`] with no
    /// repo), for the "just talk to the agent about the tracker" shape that
    /// used to need the Start-coding dialog and a repository pick. The `+`
    /// menu beside it keeps the repo-anchored launches.
    fn chat_button(&self, cx: &gpui::Context<Self>) -> impl IntoElement {
        let panel = cx.entity().downgrade();
        let agent = Self::default_chat_agent(cx);
        // A launch is already in flight (this button's, the `+` menu's or
        // cmd-t's). The strip is visible COLLAPSED, where the empty-state
        // progress line is not, so without this the only feedback is none and
        // two quick clicks buy two session rows, two scratch dirs and two
        // agents.
        let pending = self.pending_launch.clone();
        let tooltip: SharedString = match (&pending, agent) {
            (Some(label), _) => format!("Starting {label}…").into(),
            (None, Some(agent)) => format!("Chat with {}", agent.label()).into(),
            (None, None) => crate::coding_flow::NO_AGENT_COPY.into(),
        };
        // NEVER `.disabled(true)`: a disabled gpui-component button still
        // swallows the pointer event, so these ~20px of the toggle strip
        // would neither launch nor toggle the dock for the whole doctor probe
        // after launch. Dim the glyph instead and decide in the handler.
        let inert = pending.is_some() || agent.is_none();
        // EXP-760: a glass circle, not a ghost button — the strip is on the
        // window ground now and a fill-less button reads as a floating glyph.
        crate::controls::glass_icon_button("new-chat-tab", Icon::new(registry::ACTION_CHAT), cx)
            .cursor_pointer()
            .tooltip(tooltip)
            .when(inert, |this| this.opacity(0.4))
            .on_click(move |_, window, cx| {
                // No runnable agent: fall THROUGH, so the click still reaches
                // the strip and toggles the dock.
                let Some(agent) = agent else {
                    return;
                };
                // The strip's own click toggles the dock open/closed — from
                // here on this one is ours, whether it launches or is
                // swallowed as a double-click.
                cx.stop_propagation();
                if pending.is_some() {
                    return;
                }
                let Some(panel) = panel.upgrade() else {
                    return;
                };
                panel.update(cx, |panel, cx| {
                    panel.launch_chat_run(agent, None, window, cx);
                });
            })
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
        crate::controls::glass_icon_button("new-terminal-tab", Icon::new(registry::UI_ADD), cx)
            .cursor_pointer()
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
                                            Some((repository_id, full_name)),
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
                                                            Some((repository_id, full_name)),
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
                        .icon(Icon::new(registry::SESSION_SHELL))
                        .on_click(move |_, window, cx| {
                            let Some(panel) = shell_panel.upgrade() else {
                                return;
                            };
                            panel.update(cx, |panel, cx| panel.new_shell_tab(window, cx));
                        }),
                )
            })
    }

    /// EXP-703: the "+" menu / empty-state / "Chat" button agent launch — a
    /// promptless CHAT run over the builtin action rails
    /// ([`crate::action_run`] with the hidden `builtin:chat` action and, when
    /// one is given, its `repo` input filled). Unlike the EXP-325 agent shell
    /// it replaced on this surface, the run gets a `coding_sessions` row, a
    /// steer channel and the MCP session header — visible and steerable from
    /// web and mobile, and a child it starts via `exponential_sessions_start`
    /// gets parent linkage (EXP-700).
    ///
    /// EXP-739: `repo` is `Some((repository_id, full_name))` for a run
    /// anchored to a repository — its OWN worktree on `exp/chat-<id8>`, never
    /// the trunk clone — and `None` for a repo-LESS chat, which the launcher
    /// runs worktree-less in a scratch dir: a conversation with the tracker
    /// over MCP, no code checked out. The agent spawns with NO initial prompt
    /// and waits for input either way (the attended promptless shape
    /// `coding::prepare_action` allows since EXP-703).
    pub(crate) fn launch_chat_run(
        &mut self,
        agent: coding::CodingAgent,
        repo: Option<(String, String)>,
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
        // At most the `repo` input rides — deliberately NO `prompt`: the
        // person is sitting at the terminal and types the first message
        // themselves. EXP-739: a repo-less chat emits no `repo` key at all.
        let inputs: Vec<coding::ActionInputValue> = match &repo {
            Some((repository_id, full_name)) => action
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
                .collect(),
            None => Vec::new(),
        };
        // EXP-372: the cards must be gone for the launch's whole flight.
        // EXP-739: the SETTLED hook clears the progress line whichever way the
        // start ends — the TabOpened edge alone would strand it forever on an
        // ACP chat, which opens a center-panel session and no dock tab.
        self.set_pending_launch(Some(agent.label().into()), cx);
        let panel = cx.entity().downgrade();
        let on_settled: crate::action_run::ActionSettledHook = Box::new(move |cx, _started| {
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
                on_settled: Some(on_settled),
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
        let files = changes_bar::merge_changes_snapshot(previous_files, files);
        let (additions, deletions) = changes_bar::changes_totals(&files);
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
    /// The CHROME lives in [`crate::changes_bar`] (EXP-746), where the
    /// session screen's rail wears it too. Two hand-built copies of one bar
    /// is how they drift.
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
        if !changes_bar::changes_bar_visible(changes.is_some(), merge.is_some()) {
            return None;
        }
        let expanded = changes.is_some_and(|changes| changes.expanded);
        let totals = changes.map(|changes| (changes.additions, changes.deletions));
        Some(changes_bar::render(
            changes_bar::ChangesSpec {
                toggle_id: "terminal-changes-toggle",
                placement: changes_bar::ChangesPlacement::Bar,
                totals,
                expanded,
                merge,
                diff_view: self.changes_diff.clone(),
                on_toggle: Box::new(|this: &mut Self, cx| this.toggle_changes_expanded(cx)),
                on_merged: Some(self.tab_close_on_merge(tab)),
            },
            cx,
        ))
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
                                Some((repository_id, full_name)),
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
                                                Some((repository_id, full_name)),
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
                            Icon::new(registry::SESSION_SHELL),
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
        None => {
            // EXP-723: shell chips carry the `session-shell` glyph too.
            children.push(LEAD_ICON_REMS * rem);
            children.push(
                crate::screens::measure_text(
                    window,
                    &meta.title,
                    base_font.clone(),
                    gpui::rems(0.875),
                )
                .min(TITLE_MAX_W),
            );
        }
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

/// Resolve a tab's merge affordance: the [`changes_bar::merge_target_for_run`]
/// rules over
/// the LOCAL session behind the tab, with its synced `coding_sessions` row
/// looked up by the row id the launcher recorded (EXP-734 — an action or chat
/// run's own chore PR lives there and nowhere else).
fn merge_tab_meta(tab_id: TabId, cx: &App) -> Option<changes_bar::MergeTarget> {
    let sessions = crate::coding_flow::LocalSessions::global_ref(cx)?;
    let sessions = sessions.read(cx);
    let session = sessions.session_for_tab(tab_id)?;
    let store = sync::Store::try_global(cx)?;
    let collections = store.collections();
    let issues = collections.issues.read(cx);
    let rows = collections.coding_sessions.read(cx);
    let issue_id = match &session.subject {
        crate::coding_flow::SessionSubject::Issue(issue_id) => Some(issue_id.as_str()),
        // Batch and action/chat runs carry no issue of their own — the branch
        // and the session row answer for them.
        crate::coding_flow::SessionSubject::Batch(_)
        | crate::coding_flow::SessionSubject::Action(_) => None,
    };
    changes_bar::merge_target_for_run(
        issue_id,
        &session.branch,
        rows.get(&session.session_id),
        issues.iter(),
    )
}

/// EXP-742: what one strip tab contributes to the bubble's status dot.
/// Ordered by URGENCY — the bubble shows the `max` over every tab, so an
/// agent waiting on a question is never hidden behind two green runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum BubbleSignal {
    /// A plain shell, a paused host: nothing to say (muted dot).
    Quiet,
    /// A run whose PR is merged, a tab that exited 0 (blue).
    Done,
    /// A live run — running, or in review (green).
    Running,
    /// A tab whose child exited non-zero (red — the chip badge's tone).
    Failed,
    /// An agent parked on a question (amber). Wins over everything.
    NeedsInput,
}

impl BubbleSignal {
    /// The synced-row display's signal (the web tab's dot rules).
    fn from_display(display: CodingSessionDisplay) -> Self {
        match display {
            CodingSessionDisplay::NeedsInput => Self::NeedsInput,
            CodingSessionDisplay::Done => Self::Done,
            CodingSessionDisplay::Review | CodingSessionDisplay::Running => Self::Running,
        }
    }
}

/// A tab's signal: an exited child by its code; a live local coding session
/// by its synced row (the desktop writes `needs_input` there, so the amber
/// edge rides the same field every other client reads); a plain shell — no
/// session at all — stays quiet, the strip gives it no dot either.
fn local_tab_signal(meta: &TabMeta, cx: &App) -> BubbleSignal {
    match meta.exit_code {
        Some(0) => return BubbleSignal::Done,
        Some(_) => return BubbleSignal::Failed,
        None => {}
    }
    let Some(session_id) = crate::coding_flow::LocalSessions::global_ref(cx)
        .and_then(|sessions| sessions.read(cx).session_id_for_tab(meta.id).map(str::to_string))
    else {
        return BubbleSignal::Quiet;
    };
    let Some(store) = sync::Store::try_global(cx) else {
        return BubbleSignal::Running;
    };
    let collections = store.collections().clone();
    let sessions = collections.coding_sessions.read(cx);
    let Some(session) = sessions.get(&session_id) else {
        // Started, not yet synced back — a live run all the same.
        return BubbleSignal::Running;
    };
    let issues = collections.issues.read(cx);
    let pr_state = session
        .issue_id
        .as_deref()
        .and_then(|issue_id| issues.get(issue_id))
        .and_then(|issue| issue.pr_state.as_deref());
    BubbleSignal::from_display(crate::queries::coding_session_display(session, pr_state))
}

/// The bubble dot's colour for the aggregate signal — the session-status
/// palette plus the exit badge's red.
fn bubble_tone(signal: BubbleSignal, cx: &App) -> gpui::Hsla {
    match signal {
        BubbleSignal::Quiet => cx.theme().muted_foreground.opacity(0.4),
        BubbleSignal::Done => theme::tokens::BLUE.to_hsla(),
        BubbleSignal::Running => theme::tokens::GREEN.to_hsla(),
        BubbleSignal::Failed => cx.theme().danger,
        BubbleSignal::NeedsInput => theme::tokens::YELLOW.to_hsla(),
    }
}

/// Which strip indices the bubble paints as chips: the first
/// [`BUBBLE_MAX_CHIPS`] entries, with the selected one swapped in for the
/// last slot when it would otherwise fall off. Pure (unit-tested); the
/// rest is the "+N" count.
fn bubble_visible_entries(total: usize, selected_ix: usize) -> Vec<usize> {
    let mut visible: Vec<usize> = (0..total).take(BUBBLE_MAX_CHIPS).collect();
    if selected_ix < total && !visible.contains(&selected_ix) {
        visible.pop();
        visible.push(selected_ix);
    }
    visible
}

/// EXP-746: reveal the dock tab a PTY-hosted run occupies — the one seam
/// [`crate::session_screen::open_session`] still takes into the dock. The
/// dock expands and the tab becomes the active one; a session whose terminal
/// lives in ANOTHER window's manager is left alone (that window owns it, and
/// this one has nothing to show).
pub(crate) fn reveal_pty_tab(
    tab: TabId,
    manager: &WeakEntity<TerminalManager>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(panel) = crate::coding_flow::window_terminal_dock(window, cx) else {
        return;
    };
    if panel.read(cx).manager.entity_id() != manager.entity_id() {
        return;
    }
    panel.update(cx, |panel, cx| {
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
    });
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
        let metas = self.visible_tab_metas(cx);
        let (active_id, active_view, active_exit): (
            Option<TabId>,
            Option<Entity<TerminalView>>,
            Option<i32>,
        ) = {
            let manager = self.manager.read(cx);
            let active = manager
                .active_tab()
                .filter(|tab| !crate::undock::is_terminal_tab_undocked(tab.id, cx));
            (
                active.map(|tab| tab.id),
                active.map(|tab| tab.view.clone()),
                active.and_then(|tab| tab.exit_code()),
            )
        };
        let tab_count = self.manager.read(cx).len();
        let selected_ix = Self::selected_tab_ix(&metas, active_id);

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
        // EXP-742: in the bubble form the band paints NOTHING — no fill, no
        // strip — so it reads as the panel's bottom padding under the bubble
        // the shell floats over it (`render_bubble`).
        let bubble = collapsed && self.bubble_preferred(cx);
        // The tab the strip paints as selected — what the header's undock
        // acts on.
        let header_active_tab = metas.get(selected_ix).map(|meta| meta.id);
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
        // EXP-723 history: the ROOT used to paint the dock's opaque card
        // (popover fill, seam hairline, the panel's bottom radius). EXP-760
        // moved all three onto the dock BODY (`pin_content`), because the
        // root also spans the bottom strip and the strip is on the window
        // ground now — the card has to close ABOVE it, not behind it. The
        // root is pure layout.

        let content: Option<AnyElement> = if collapsed {
            None
        } else {
            let body = v_flex()
                .w_full()
                .overflow_hidden()
                .child(self.render_dock_header(header_active_tab, cx));
            Some(match active_view {
                Some(active_view) => self.pin_content(
                    body
                        // min_h(0) so the flex child can shrink with the
                        // dock; the grid element itself guards the 0-height
                        // collapsed case (§6.9).
                        .child(div().flex_1().min_h_0().child(active_view))
                        .when_some(active_exit, |this, code| this.child(exit_strip(code, cx)))
                        .children(active_id.and_then(|id| self.render_changes_bar(id, cx))),
                    cx,
                ),
                // Tabs exist but none is visible/active here — every one is
                // undocked (or the active tab just popped out mid-frame).
                None if tab_count > 0 => {
                    self.pin_content(body.child(self.render_undocked_hint(cx)), cx)
                }
                // EXP-369: an expanded, empty dock offers its launch cards —
                // nothing spawns until the user picks something.
                None => self.pin_content(body.child(self.render_empty_dock_options(window, cx)), cx),
            })
        };

        root.children(content).when(!bubble, |root| {
            root.child(self.render_strip(&metas, selected_ix, collapsed, window, cx))
        })
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;
    use gpui_component::dock::{DockAreaState, PanelInfo};

    use super::*;

    /// EXP-742: the bubble's dot is the MOST urgent entry, not the first —
    /// a question waiting behind two green runs must still turn it amber,
    /// and a paused host contributes nothing.
    #[test]
    fn bubble_signal_orders_by_urgency() {
        assert!(BubbleSignal::NeedsInput > BubbleSignal::Failed);
        assert!(BubbleSignal::Failed > BubbleSignal::Running);
        assert!(BubbleSignal::Running > BubbleSignal::Done);
        assert!(BubbleSignal::Done > BubbleSignal::Quiet);
        let signals = [
            BubbleSignal::Running,
            BubbleSignal::Running,
            BubbleSignal::NeedsInput,
            BubbleSignal::Quiet,
        ];
        assert_eq!(signals.iter().copied().max(), Some(BubbleSignal::NeedsInput));
        assert_eq!(
            BubbleSignal::from_display(CodingSessionDisplay::Review),
            BubbleSignal::Running
        );
        assert_eq!(
            BubbleSignal::from_display(CodingSessionDisplay::Done),
            BubbleSignal::Done
        );
        // An empty strip has no signal at all — the caller falls back to Quiet.
        assert_eq!(Vec::<BubbleSignal>::new().into_iter().max(), None);
    }

    /// EXP-742: the bubble paints at most [`BUBBLE_MAX_CHIPS`] chips and the
    /// selected entry is always among them — it takes the last slot when it
    /// would otherwise fold into the "+N" count.
    #[test]
    fn bubble_keeps_the_selected_chip_visible() {
        assert_eq!(bubble_visible_entries(0, 0), Vec::<usize>::new());
        assert_eq!(bubble_visible_entries(2, 1), vec![0, 1]);
        assert_eq!(bubble_visible_entries(5, 0), vec![0, 1, 2]);
        assert_eq!(bubble_visible_entries(5, 4), vec![0, 1, 4]);
        assert_eq!(bubble_visible_entries(5, 2), vec![0, 1, 2]);
        // An out-of-range selection (nothing selected) never adds a slot.
        assert_eq!(bubble_visible_entries(5, 9), vec![0, 1, 2]);
        assert_eq!(bubble_visible_entries(BUBBLE_MAX_CHIPS, 0).len(), BUBBLE_MAX_CHIPS);
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
}
