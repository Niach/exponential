//! The bottom terminal dock (masterplan-v3 §6.13 / §7.5) — the
//! JetBrains-style multi-tab terminal panel.
//!
//! One [`TerminalDockPanel`] per window lives inside the bottom `Dock`'s
//! `TabPanel` (§3.3); *inside* it, a gpui-component `Tab`/`TabBar` strip
//! lists the [`terminal::TerminalManager`]'s sessions — **not** Zed's GPL
//! `Pane`/`Dock` (§6.13's licensing rule). Behavior:
//!
//! - **"+"** → a dropdown (EXP-325): one item per doctor-installed agent
//!   CLI, launching an empty promptless agent session on the current
//!   board's trunk repo (a repo submenu when the team has several), plus
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
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use terminal::{TabId, TabKind, TerminalManager, TerminalManagerEvent, TerminalView};

use crate::coding_flow::{CodingHub, TokenRefreshers};
use crate::icons::{registry, ExpIcon};
use crate::navigation;
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup, RepoResolver};

/// Stable serialization name for the panel registry (§3.3: never change it).
pub const PANEL_NAME: &str = "TerminalDock";

/// Per-tab hover group (EXP-65): reveals the undock button, mirroring the
/// center tabs' `TAB_GROUP` idiom.
const TAB_GROUP: &str = "terminal-tab";

/// EXP-523: the bottom dock slides open and shut instead of snapping. The
/// duration is the shared `standard` motion token, the same one the left
/// column's rail-to-settings swap uses.
const DOCK_SLIDE_DURATION: Duration = theme::motion::STANDARD;

/// Upstream `Dock::render`'s CLOSED height — the toggle strip it keeps when
/// `open == false`. The slide's closed endpoint.
const DOCK_STRIP_H: f32 = 29.;

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
/// # Why the content also needs a vertical offset
///
/// `Dock::set_size` clamps to upstream's `PANEL_MIN_SIZE` (100px), so heights
/// between the 29px strip and that floor are not addressable — a naive
/// animation would snap the last ~71px at the slow end of the easing curve,
/// where the eye is most sensitive. Each tick therefore READS BACK what
/// `set_size` actually stored and pushes the content down by the difference,
/// so the panel's visible top edge tracks the virtual height continuously
/// through the clamped region. The band this exposes above it is the same
/// `theme::background_gradient()` quad the center already paints, so it reads
/// as the center growing, not as a hole. Reading the clamp back rather than
/// hardcoding it also keeps this correct if upstream ever changes the floor.
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

    /// How far to push the content down so the visible top edge sits at the
    /// virtual height even while `set_size` is clamping.
    fn content_offset(&self) -> f32 {
        (self.applied - self.requested).max(0.)
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
        }
        let local_sessions = crate::coding_flow::LocalSessions::global(cx);
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

        Self {
            focus_handle: cx.focus_handle(),
            manager,
            dock_area,
            agent_shell_holds: HashMap::new(),
            chips_slot_width: None,
            pending_launch: None,
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

    /// Mid-slide the content is PINNED to the resting height and pushed down
    /// by the clamp offset, inside the panel's `overflow_hidden` root: the
    /// terminal grid's bounds never move (so the PTY reshapes at most once per
    /// open), and the visible top edge tracks the virtual height continuously
    /// through `Dock::set_size`'s `PANEL_MIN_SIZE` floor. At rest it is just
    /// `size_full`, exactly as before.
    fn pin_content<E: Styled>(&self, content: E) -> E {
        match self.dock_slide {
            Some(slide) => content
                .absolute()
                .left_0()
                .right_0()
                .top(px(slide.content_offset()))
                .h(px(slide.rest_height)),
            None => content.size_full(),
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
        if visible.is_empty() {
            return;
        }
        let current_pos = self
            .manager
            .read(cx)
            .active_index()
            .and_then(|active| visible.iter().position(|ix| *ix == active));
        let next_pos = match current_pos {
            Some(pos) if forward => (pos + 1) % visible.len(),
            Some(pos) => (pos + visible.len() - 1) % visible.len(),
            None => 0,
        };
        self.manager
            .update(cx, |manager, cx| manager.activate(visible[next_pos], cx));
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

    /// The session tab strip: one `Tab` per VISIBLE session (title + exit
    /// badge + hover-revealed undock + close button; EXP-65 undocked tabs
    /// are hidden — they render in their own windows), the `+` right after
    /// the last tab, and the collapse chevron at the far right (§6.13).
    /// Clicking the bar's empty space collapses the dock — the whole strip is
    /// the toggle, mirroring the collapsed strip's whole-bar expand
    /// (tab/button handlers stop propagation so their clicks never fall
    /// through to the collapse).
    ///
    /// EXP-497: chips that don't fit collapse into a trailing "+N" dropdown —
    /// the center strip's EXP-288 treatment (the scrolled chips this replaces
    /// left overflowing tabs cut off). Chip widths are measured, not guessed
    /// (the EXP-326 lesson), against the recorded [`Self::chips_slot_width`];
    /// the SELECTED tab is always kept visible.
    fn render_tab_bar(
        &self,
        metas: &[TabMeta],
        selected_ix: usize,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        // EXP-277: hand-rolled rounded chips (crate::surface::tab_chip), same
        // treatment as the center tab strip — gpui-component's TabBar is
        // square with a strip-wide bottom border.
        // EXP-325: the shared merge state drives the hover merge button —
        // materialized here (needs `&mut App`), read inside the chip closure.
        let merge_state = crate::pr_merge::MergeState::global(cx);

        // EXP-497: partition the chips against the slot's painted width. The
        // `+` new-session menu rides INSIDE the slot right after the chips —
        // an xsmall icon button (`size_5`) plus one gap comes off the budget.
        let widths: Vec<f32> = metas
            .iter()
            .map(|meta| measure_tab_chip_width(meta, &merge_state, window, cx))
            .collect();
        let plus_reserve = 1.25 * f32::from(window.rem_size()) + crate::screens::chip_gap(window);
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

        let chips = visible.into_iter().map(|ix| {
            let meta = &metas[ix];
            let id = meta.id;
            let manager_ix = meta.manager_ix;
            let merge_button = meta
                .merge
                .as_ref()
                .map(|merge| self.tab_merge_button(ix, id, merge, &merge_state, cx));
            let has_merge = merge_button.is_some();
            let chip = crate::surface::tab_chip(ix == selected_ix, cx)
                .id(("terminal-tab", ix))
                .group(TAB_GROUP)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.manager
                        .update(cx, |manager, cx| manager.activate(manager_ix, cx));
                    this.focus_active_terminal(window, cx);
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
            // EXP-325: an issue-session tab renders the center issue-tab
            // treatment (status glyph + mono identifier + synced title,
            // mirroring `screens::render_tab_strip`); everything else keeps
            // the plain terminal title.
            let chip = match &meta.issue {
                Some(issue) => chip
                    .child(crate::icons::resolved_status_icon(&issue.status, cx).xsmall())
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .font_family(theme::terminal::FONT_FAMILY)
                            .whitespace_nowrap()
                            .child(issue.identifier.clone()),
                    )
                    .when_some(issue.title.clone(), |chip, title| {
                        chip.child(div().max_w(px(180.)).truncate().child(title))
                    }),
                None => chip.child(div().max_w(px(180.)).truncate().child(meta.title.clone())),
            };
            chip.child(
                    h_flex()
                        .gap_0p5()
                        .items_center()
                        .when_some(meta.exit_code, |this, code| {
                            let color = if code == 0 {
                                cx.theme().success
                            } else {
                                cx.theme().danger
                            };
                            this.child(
                                div()
                                    .text_xs()
                                    .px_1()
                                    .rounded(px(3.))
                                    .bg(color.opacity(0.15))
                                    .text_color(color)
                                    .child(SharedString::from(code.to_string())),
                            )
                        })
                        // Hover-revealed undock (EXP-65) — same treatment as
                        // the center tabs; `invisible` keeps the layout slot.
                        .child(
                            div()
                                .invisible()
                                .group_hover(TAB_GROUP, |style| style.visible())
                                .child(
                                    Button::new(("undock-terminal-tab", ix))
                                        .ghost().cursor_pointer()
                                        .xsmall()
                                        .icon(ExpIcon::ExternalLink)
                                        .tooltip("Open in new window")
                                        .on_click(cx.listener(
                                            move |this, _: &ClickEvent, window, cx| {
                                                cx.stop_propagation();
                                                this.undock_tab(id, window, cx);
                                            },
                                        )),
                                ),
                        )
                        // EXP-498: the always-visible "Merge" shortcut for a
                        // session (issue OR batch) with an open PR — it takes
                        // the close button's slot (merging closes the
                        // session, so the one affordance does both). Undock +
                        // middle-click keep a close-without-merging escape
                        // hatch.
                        .when_some(merge_button, |this, button| this.child(button))
                        .when(!has_merge, |this| {
                            this.child(
                                Button::new(("close-terminal-tab", ix))
                                    .ghost().cursor_pointer()
                                    .xsmall()
                                    .icon(registry::UI_CLOSE)
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, _window, cx| {
                                            cx.stop_propagation();
                                            this.manager.update(cx, |manager, cx| {
                                                manager.close_tab(id, cx)
                                            });
                                        },
                                    )),
                            )
                        }),
                )
        });
        // EXP-497: the hidden tabs collapse into a "+N" dropdown; clicking
        // one activates it. Keyed by TabId, not strip index — the menu's
        // closures run at click time, and a tab closed while the dropdown is
        // open shifts every index after it (the center strip's EXP-288
        // rationale; the TabId is the stable identity here).
        let overflow_button = (!hidden.is_empty()).then(|| {
            type HiddenEntry = (
                TabId,
                Option<domain::statuses::ResolvedStatus>,
                SharedString,
            );
            let hidden_entries: Vec<HiddenEntry> = hidden
                .iter()
                .map(|&ix| {
                    let meta = &metas[ix];
                    // The menu rows mirror the chips: issue sessions carry
                    // the status glyph + "IDENT title", the rest their plain
                    // terminal title.
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
                                this.manager
                                    .update(cx, |manager, cx| manager.activate(ix, cx));
                                this.focus_active_terminal(window, cx);
                            });
                        }));
                    }
                    menu
                })
        });

        // Clicking the strip's empty space collapses the dock — the whole
        // strip is the toggle (chip/button handlers stop propagation).
        h_flex()
            // EXP-497: record the chip slot's painted width (`bounds[0]` —
            // the `flex_1` child below, a pure-stretch flex item the chips
            // cannot inflate) so the partition above budgets against the real
            // layout. Change-gated: only a real width change repaints. (On
            // the bare `Div` — the method is not exposed on `Stateful`, so it
            // rides ahead of `.id()`.)
            .on_children_prepainted({
                let panel = cx.entity().downgrade();
                move |bounds: Vec<Bounds<Pixels>>, _window, cx| {
                    let Some(first) = bounds.first() else {
                        return;
                    };
                    let width = f32::from(first.size.width);
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
            .w_full()
            .px_1()
            .py_0p5()
            .gap_1()
            .items_center()
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.collapse_dock(window, cx);
            }))
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
                Button::new("collapse-terminal-dock")
                    .ghost().cursor_pointer()
                    .xsmall()
                    .icon(registry::UI_CHEVRON_DOWN)
                    .tooltip("Hide terminal")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.collapse_dock(window, cx);
                    })),
            )
    }

    /// The merge button on a session tab whose PR is open (issue AND batch
    /// since EXP-498): always visible, reads "Merge", and REPLACES the tab's
    /// close button — merging always closes the session, so the one
    /// affordance does both. Two-click confirm via the shared `pr_merge`
    /// state ("Merge" → "Confirm merge", ~5s auto-disarm). A failed merge
    /// (typically conflicts) jumps to the Reviews tool window, where the
    /// shared error caption + Fix-conflicts button render exactly as a
    /// Reviews-originated failure.
    ///
    /// The tab closes LOCALLY the moment the merge call fires (the
    /// `TabClosed` watcher fires the idempotent `codingSessions.end`), so a
    /// merge that fails on conflicts never leaves a live session holding the
    /// branch — the Reviews rail's "Fix conflicts" recovery starts
    /// immediately instead of parking behind a busy worktree. The server
    /// ends the user's live sessions on OTHER devices after the merge.
    fn tab_merge_button(
        &self,
        ix: usize,
        tab: TabId,
        merge: &MergeTabMeta,
        merge_state: &Entity<crate::pr_merge::MergeState>,
        cx: &gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let (armed, merging) = {
            let state = merge_state.read(cx);
            (state.armed(&merge.issue_id), state.merging(&merge.issue_id))
        };
        let mut button = Button::new(("merge-terminal-tab", ix)).xsmall();
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
                        crate::sidebar::activate_tool(
                            window,
                            cx,
                            crate::sidebar::ToolWindow::Reviews,
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
    /// clicking immediately launches an empty promptless session of that
    /// agent on the current team's trunk repo (several board-backed repos →
    /// a repo picker submenu; resolver still loading / no repo → disabled) —
    /// plus the plain "New shell" (the pre-EXP-325 `+` behavior; cmd-t
    /// unchanged). Installed agents and repos resolve fresh at OPEN time
    /// (the closure outlives renders); no doctor report yet → only the
    /// shell item.
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
                                        panel.launch_agent_shell(
                                            agent,
                                            repository_id,
                                            full_name,
                                            None,
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
                                                        panel.launch_agent_shell(
                                                            agent,
                                                            repository_id,
                                                            full_name,
                                                            None,
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
                // `separator` no-ops on an empty menu — no leading rule when
                // there are no agent items.
                let shell_panel = panel.clone();
                menu.separator().item(
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

    /// The EXP-325 promptless agent launch: background
    /// [`coding::prepare_agent_shell`] (doctor → token → clone/autopull →
    /// MCP wiring → promptless argv with the agent's settings defaults) →
    /// foreground [`TabKind::AgentShell`] tab in THIS dock. No
    /// `coding_sessions` row / heartbeat / exit hook — the session has no
    /// issue/batch/action subject; the P9 token-refresher hold keeps `git
    /// push` working past the token TTL, released on tab close.
    ///
    /// `cwd_override` pins the run to one of the clone's worktrees (EXP-369 —
    /// the settings pane's per-worktree terminal button); `None` runs on the
    /// trunk clone root.
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

    /// The collapsed-dock strip: the bottom dock keeps a 29px band
    /// when closed, and a chrome-less panel renders its full content clipped
    /// into it — instead render this compact one-line strip. Clicking it (or
    /// the chevron) re-opens the dock.
    fn render_collapsed_strip(
        &self,
        tab_count: usize,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let label: SharedString = if tab_count > 0 {
            format!("Terminal ({tab_count})").into()
        } else {
            "Terminal".into()
        };
        h_flex()
            .id("terminal-collapsed-strip")
            .w_full()
            .h(px(29.))
            .px_3()
            .gap_2()
            .items_center()
            .flex_shrink_0()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().title_bar)
            .text_color(cx.theme().muted_foreground)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.expand_dock(window, cx);
                // EXP-369: expanding NEVER starts anything — with zero
                // sessions the dock opens on its launch cards; with sessions
                // the active terminal takes focus back.
                if !this.manager.read(cx).is_empty() {
                    this.focus_active_terminal(window, cx);
                }
            }))
            .child(Icon::new(registry::NAV_TERMINAL).xsmall())
            .child(div().text_xs().child(label))
            .child(div().flex_1())
            .child(Icon::new(registry::UI_CHEVRON_UP).xsmall())
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
                            panel.launch_agent_shell(
                                agent,
                                repository_id,
                                full_name,
                                None,
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
                                            panel.launch_agent_shell(
                                                agent,
                                                repository_id,
                                                full_name,
                                                None,
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
    /// EXP-498: present when this tab's session — issue OR batch — has an
    /// open PR. The chip then swaps its close button for the "Merge" button
    /// (merging always closes the session). Deliberately separate from
    /// `issue` so batch tabs gain the merge affordance without inheriting
    /// the issue-chip content.
    merge: Option<MergeTabMeta>,
}

/// The issue-chip snapshot of one issue-session terminal tab (EXP-325).
struct IssueTabMeta {
    status: domain::statuses::ResolvedStatus,
    identifier: SharedString,
    /// `None` for a blank issue title — the identifier already labels the
    /// chip (the EXP-310 center-tab rule).
    title: Option<SharedString>,
}

/// The tab's merge affordance (EXP-498): the representative synced issue
/// with an open PR — `issues.mergePr` on it fans out to every issue sharing
/// the prUrl, so any batch sibling merges the whole PR.
struct MergeTabMeta {
    issue_id: String,
}

/// Measured width of one tab chip, for the EXP-497 overflow partition —
/// mirrors the chip layout in `render_tab_bar` piece for piece, the way
/// `screens::measure_chip_width` mirrors the center chips (EXP-326: spacing
/// helpers resolve against the rem size and labels are SHAPED with the
/// window's text system, so "fits" means fits).
fn measure_tab_chip_width(
    meta: &TabMeta,
    merge_state: &Entity<crate::pr_merge::MergeState>,
    window: &Window,
    cx: &App,
) -> f32 {
    /// `tab_chip`'s `px_2`, both sides.
    const CHIP_PADDING_REMS: f32 = 0.5 * 2.;
    /// `Icon::xsmall()` — `size_3` (the issue chip's status glyph).
    const LEAD_ICON_REMS: f32 = 0.75;
    /// An icon-only xsmall `Button` — `size_5` (undock/close).
    const XSMALL_BUTTON_REMS: f32 = 1.25;
    /// The trailing button cluster's own `gap_0p5`.
    const CLUSTER_GAP_REMS: f32 = 0.125;
    /// The exit badge's `px_1`, both sides.
    const BADGE_PADDING_REMS: f32 = 0.25 * 2.;
    /// A labeled xsmall `Button`'s `px_1` (both sides) plus its outline
    /// border — the armed/merging merge button.
    const LABEL_BUTTON_CHROME: f32 = 0.5;
    /// `.max_w(px(180.)).truncate()` on the title child — a real pixel
    /// value, so it does NOT scale with the rem.
    const TITLE_MAX_W: f32 = 180.;

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

    // The trailing cluster: exit badge, merge button (EXP-498: always a
    // LABELED button — idle "Merge" carries the GitMerge icon, armed
    // "Confirm merge" / "Merging…" drop it), undock slot (`invisible` keeps
    // its box), and close — which the merge button REPLACES when present.
    let mut cluster: Vec<f32> = Vec::with_capacity(4);
    if let Some(code) = meta.exit_code {
        cluster.push(
            BADGE_PADDING_REMS * rem
                + crate::screens::measure_text(
                    window,
                    &code.to_string(),
                    base_font.clone(),
                    gpui::rems(0.75),
                ),
        );
    }
    if let Some(merge) = meta.merge.as_ref() {
        let state = merge_state.read(cx);
        let (label, icon) = if state.merging(&merge.issue_id) {
            ("Merging…", false)
        } else if state.armed(&merge.issue_id) {
            ("Confirm merge", false)
        } else {
            ("Merge", true)
        };
        // Labeled xsmall button: px_1 chrome (+2px outline border slack) +
        // shaped label, plus the size_3 icon and the button's gap_1 when the
        // idle state renders the GitMerge glyph next to the text.
        cluster.push(
            LABEL_BUTTON_CHROME * rem
                + 2.
                + crate::screens::measure_text(window, label, base_font.clone(), gpui::rems(0.75))
                + if icon { (0.75 + 0.25) * rem } else { 0. },
        );
    }
    cluster.push(XSMALL_BUTTON_REMS * rem);
    if meta.merge.is_none() {
        cluster.push(XSMALL_BUTTON_REMS * rem);
    }
    let cluster_width = cluster.iter().sum::<f32>()
        + CLUSTER_GAP_REMS * rem * cluster.len().saturating_sub(1) as f32;
    children.push(cluster_width);

    let gaps = crate::screens::chip_gap(window) * children.len().saturating_sub(1) as f32;
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
            self.focus_active_terminal(window, cx);
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
                    merge: merge_tab_meta(tab.id, cx),
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

        let outer = div()
            .id("terminal-dock-clip")
            .relative()
            .size_full()
            .overflow_hidden();
        let root = v_flex()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_new_tab))
            .on_action(cx.listener(Self::on_close_tab))
            .on_action(cx.listener(Self::on_next_tab))
            .on_action(cx.listener(Self::on_prev_tab));

        // Collapsed dock: only the compact strip — never the full
        // content squeezed/clipped into the 29px band.
        // EXP-523: `dock_slide.is_none()` holds this back while a CLOSE
        // animation runs. `collapse_dock` deliberately does not flip
        // `set_open` until it settles, so the content stays rendered for the
        // whole slide and the swap lands on a near-identical frame (the 29px
        // tab strip replaced by the 29px collapsed strip). On open the flip
        // happens up front, so this branch is already false on frame 1.
        if self.dock_collapsed(cx) && self.dock_slide.is_none() {
            return outer.child(root.size_full().child(
                self.render_collapsed_strip(metas.len(), cx),
            ));
        }

        let Some(active_view) = active_view else {
            if tab_count > 0 {
                // Tabs exist but none is visible/active here — every one is
                // undocked (or the active tab just popped out mid-frame).
                // Keep the bar (the `+` stays reachable) over a hint.
                return outer.child(self.pin_content(
                    root.child(self.render_tab_bar(&metas, 0, window, cx))
                        .child(self.render_undocked_hint(cx)),
                ));
            }
            // EXP-369: an expanded, empty dock offers its launch cards — the
            // bar stays (the `+` / collapse chevron keep working) and nothing
            // spawns until the user picks something.
            return outer.child(self.pin_content(
                root.child(self.render_tab_bar(&metas, 0, window, cx))
                    .child(self.render_empty_dock_options(window, cx)),
            ));
        };

        let selected_ix = active_id
            .and_then(|id| metas.iter().position(|meta| meta.id == id))
            .unwrap_or(0);
        outer.child(
            self.pin_content(
                root.child(self.render_tab_bar(&metas, selected_ix, window, cx))
                    // min_h(0) so the flex child can shrink with the dock; the
                    // grid element itself guards the 0-height collapsed case
                    // (§6.9).
                    .child(div().flex_1().min_h_0().child(active_view))
                    .when_some(active_exit, |this, code| {
                        this.child(exit_strip(code, cx))
                    }),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;
    use gpui_component::dock::{DockAreaState, PanelInfo};

    use super::*;

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

    // The clamp offset is what keeps the panel's visible edge moving through
    // the region `Dock::set_size` refuses to store (PANEL_MIN_SIZE).
    #[test]
    fn content_offset_is_zero_above_the_clamp_floor_and_covers_it_below() {
        let t0 = Instant::now();
        let mut slide = DockSlide::new(240., DOCK_STRIP_H, 240., false, t0);

        slide.record_apply(180., 180.);
        assert_eq!(slide.content_offset(), 0., "no clamp above the floor");

        // Asked for 29, upstream stored 100 — the content must drop by 71 so
        // the visible top edge is still where 29 would put it.
        slide.record_apply(DOCK_STRIP_H, 100.);
        assert!((slide.content_offset() - (100. - DOCK_STRIP_H)).abs() < 0.01);

        // Never negative, whatever upstream does.
        slide.record_apply(300., 240.);
        assert_eq!(slide.content_offset(), 0.);
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
