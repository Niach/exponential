//! Multi-window undock (EXP-65): content tabs and terminal tabs can pop out
//! into their own native windows via an explicit hover-revealed button.
//!
//! Design constraints (researched against the pinned gpui rev):
//! - gpui has no cross-window drag / drag-out-of-window support (Zed's own
//!   drag-tab-to-new-window PR #44489 was closed unmerged and needs gpui core
//!   changes we won't fork for) — so undock/reattach are BUTTONS, not drags.
//! - Content screens get a FRESH view instance per undocked window
//!   ([`crate::screens::build_screen_content`]): the `ScreensPanel` keeps one
//!   shared instance per view type re-pointed on tab switch, which must never
//!   be moved out.
//! - Terminal tabs stay owned by their window's `TerminalManager` (exit
//!   hooks, run-bar stop, steer wiring, §6.13 persistence all key off it);
//!   undocking only moves WHERE the tab renders. The dock hides the tab while
//!   [`UndockState`] maps it to a window, so exactly one window paints the
//!   `TerminalView` at any time.
//!
//! [`UndockState`] is an observable entity (not a plain global): the screens
//! panel and terminal dock `cx.observe` it so hiding/reshowing tabs repaints
//! without bespoke plumbing. Undock state is deliberately NOT persisted
//! across restarts — everything restores docked.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui::{
    div, px, size, AnyView, AnyWindowHandle, App, AppContext as _, Bounds, ClickEvent, Entity,
    FocusHandle, Focusable, Global, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, Styled, Window, WindowBounds, WindowId, WindowKind, WindowOptions,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, ActiveTheme as _, Root, Sizable as _,
};
use terminal::{TabId, TerminalManager};

use crate::navigation::{self, Screen};
use crate::workspace::Workspace;

/// Cascade offset so stacked undocks don't open exactly on top of each other.
const CASCADE_STEP: f32 = 24.;
static UNDOCK_ORDINAL: AtomicUsize = AtomicUsize::new(0);

/// One undocked terminal-tab window: the handle plus the workspace window
/// whose `TerminalManager` owns the tab (reattach target; when that owner
/// closes, this window closes with it — the manager dies with the owner's
/// dock panel).
struct TerminalEntry {
    handle: AnyWindowHandle,
    owner: WindowId,
}

/// Runtime registry of undocked windows (dedupe + lifecycle). Observed by
/// the screens panel and terminal dock.
#[derive(Default)]
pub struct UndockState {
    screens: HashMap<Screen, AnyWindowHandle>,
    terminal_tabs: HashMap<TabId, TerminalEntry>,
}

struct UndockGlobal(Entity<UndockState>);

impl Global for UndockGlobal {}

/// Create the state entity (call once from `ui::init`).
pub(crate) fn init(cx: &mut App) {
    let state = cx.new(|_| UndockState::default());
    cx.set_global(UndockGlobal(state));
}

/// The observable state entity — panels `cx.observe` it to repaint when
/// tabs undock/reattach. `None` before `init` (headless unit tests).
pub(crate) fn state(cx: &App) -> Option<Entity<UndockState>> {
    cx.try_global::<UndockGlobal>().map(|global| global.0.clone())
}

/// Whether this terminal tab currently renders in its own window (the dock
/// hides it while true).
pub(crate) fn is_terminal_tab_undocked(id: TabId, cx: &App) -> bool {
    state(cx).is_some_and(|state| state.read(cx).terminal_tabs.contains_key(&id))
}

fn screen_window(screen: &Screen, cx: &App) -> Option<AnyWindowHandle> {
    state(cx).and_then(|state| state.read(cx).screens.get(screen).copied())
}

fn terminal_tab_window(id: TabId, cx: &App) -> Option<AnyWindowHandle> {
    state(cx).and_then(|state| state.read(cx).terminal_tabs.get(&id).map(|entry| entry.handle))
}

/// Drop a terminal tab's registry entry (its window closed / reattached).
/// Entity update → the owning dock panel repaints and shows the tab again.
pub(crate) fn unregister_terminal_tab(id: TabId, cx: &mut App) {
    let Some(state) = state(cx) else { return };
    if state.read(cx).terminal_tabs.contains_key(&id) {
        state.update(cx, |state, cx| {
            state.terminal_tabs.remove(&id);
            cx.notify();
        });
    }
}

fn unregister_screen(screen: &Screen, cx: &mut App) {
    let Some(state) = state(cx) else { return };
    if state.read(cx).screens.contains_key(screen) {
        let screen = screen.clone();
        state.update(cx, |state, cx| {
            state.screens.remove(&screen);
            cx.notify();
        });
    }
}

/// Whether the window's root view is a [`Workspace`] shell (as opposed to an
/// undocked panel window or a mid-teardown handle).
fn is_workspace_window(handle: AnyWindowHandle, cx: &App) -> bool {
    handle
        .downcast::<Root>()
        .and_then(|root| root.read(cx).ok())
        .is_some_and(|root| root.view().clone().downcast::<Workspace>().is_ok())
}

/// The workspace window a reattach should target: the originating window when
/// it is still alive, else any remaining workspace window.
pub(crate) fn find_workspace_window(
    preferred: Option<AnyWindowHandle>,
    cx: &App,
) -> Option<AnyWindowHandle> {
    if let Some(handle) = preferred {
        if is_workspace_window(handle, cx) {
            return Some(handle);
        }
    }
    cx.windows()
        .into_iter()
        .find(|handle| is_workspace_window(*handle, cx))
}

/// Workspace-window release hook (called after `windows_open` decrements):
/// close this owner's undocked terminal windows (their manager died with the
/// owner's dock panel), and when NO workspace window remains, close every
/// undocked window — there is nothing left to reattach to, and on non-macOS
/// the app is about to quit.
pub(crate) fn on_workspace_released(released: WindowId, cx: &mut App) {
    let Some(state) = state(cx) else { return };

    let orphaned: Vec<AnyWindowHandle> = state
        .read(cx)
        .terminal_tabs
        .values()
        .filter(|entry| entry.owner == released)
        .map(|entry| entry.handle)
        .collect();
    close_windows(orphaned, cx);

    if find_workspace_window(None, cx).is_some() {
        return;
    }
    let remaining: Vec<AnyWindowHandle> = {
        let state = state.read(cx);
        state
            .screens
            .values()
            .copied()
            .chain(state.terminal_tabs.values().map(|entry| entry.handle))
            .collect()
    };
    close_windows(remaining, cx);
}

fn close_windows(handles: Vec<AnyWindowHandle>, cx: &mut App) {
    for handle in handles {
        // Deferred: this runs from inside a window update (the release hook);
        // a synchronous re-entrant `window.update` would silently no-op (see
        // `navigation::on_active_window`).
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        });
    }
}

/// Shared `WindowOptions` for undocked windows — mirrors the workspace
/// window's options (`app/src/windows.rs`) at a smaller default size.
fn undocked_window_options(default_size: gpui::Size<gpui::Pixels>, cx: &App) -> WindowOptions {
    let ordinal = UNDOCK_ORDINAL.fetch_add(1, Ordering::SeqCst);
    let mut bounds = Bounds::centered(None, default_size, cx);
    let cascade = px(CASCADE_STEP * (ordinal % 8) as f32);
    bounds.origin.x += cascade;
    bounds.origin.y += cascade;
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(480.), px(320.))),
        kind: WindowKind::Normal,
        // Match the workspace window's Wayland app_id / X11 WM_CLASS so
        // undocked windows also pick up the `.desktop` taskbar icon (EXP-68).
        app_id: Some(CHANNEL_APP_ID.to_string()),
        // Linux: server-side decorations, same rationale as the main window.
        #[cfg(target_os = "linux")]
        window_decorations: Some(gpui::WindowDecorations::Server),
        ..Default::default()
    }
}

// Duplicates `app::channel::APP_ID` (ui cannot depend on the app crate) via
// the same compile-time channel feature — the CLOUD_INSTANCE precedent.
#[cfg(not(feature = "staging"))]
const CHANNEL_APP_ID: &str = "at.exponential";
#[cfg(feature = "staging")]
const CHANNEL_APP_ID: &str = "at.exponential.staging";

/// Activate an already-open undocked window (dedupe path), deferred out of
/// the caller's window update.
fn activate_window(handle: AnyWindowHandle, cx: &mut App) {
    cx.defer(move |cx| {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
    });
}

// ---------------------------------------------------------------------------
// Undocked content screens
// ---------------------------------------------------------------------------

/// Open `screen` in its own native window (or focus the existing one). The
/// caller closes the source tab afterwards.
pub(crate) fn open_undocked_screen(screen: Screen, origin: AnyWindowHandle, cx: &mut App) {
    if let Some(existing) = screen_window(&screen, cx) {
        activate_window(existing, cx);
        return;
    }
    let title = navigation::screen_title(&screen, cx);
    let options = undocked_window_options(size(px(980.), px(720.)), cx);
    // The gpui-component-sanctioned pattern: open windows inside a foreground
    // spawn (also dodges the re-entrant window-update trap — this is called
    // from click handlers).
    cx.spawn(async move |cx| {
        let origin_id = origin.window_id();
        let window = cx.open_window(options, move |window, cx| {
            let view =
                cx.new(|cx| UndockedScreenWindow::new(screen.clone(), origin, window, cx));
            // Root MUST be the first view of every window (§3.3) — it hosts
            // the dialog/notification overlay layers issue detail relies on.
            cx.new(|cx| Root::new(view, window, cx))
        })?;
        let _ = origin_id;
        window.update(cx, |_, window, cx| {
            window.set_window_title(&title);
            window.activate_window();
            let _ = cx;
        })?;
        anyhow::Ok(())
    })
    .detach();
}

/// A slim native window hosting one content screen: header (title +
/// Reattach) over a fresh screen view, with the Root overlay layers so
/// dialogs/notifications opened from the content still paint.
pub(crate) struct UndockedScreenWindow {
    screen: Screen,
    origin: AnyWindowHandle,
    content: AnyView,
    focus_handle: FocusHandle,
    /// Last title pushed to the OS window (issue renames sync live).
    window_title: SharedString,
}

impl UndockedScreenWindow {
    fn new(
        screen: Screen,
        origin: AnyWindowHandle,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        // Register FIRST so the release hook (which unregisters) is always
        // balanced, and dedupe holds from the first frame.
        if let Some(state) = state(cx) {
            let handle = window.window_handle();
            let key = screen.clone();
            state.update(cx, |state, cx| {
                state.screens.insert(key, handle);
                cx.notify();
            });
        }

        // Scope the fresh window like the origin (workspace/project + the
        // screen itself) so git bar / shell cwd / SC file scope resolve.
        navigation::seed_window_scope(window, cx, origin.window_id(), screen.clone());

        let content = crate::screens::build_screen_content(&screen, window, cx);

        let window_id = window.window_handle().window_id();
        cx.on_release(move |this, cx| {
            // Reattach/close unmounts this window's own issue detail without
            // a blur — flush a pending description edit first (EXP-68), the
            // same contract as ScreensPanel's tab-close/workspace-switch.
            if let Ok(detail) = this
                .content
                .clone()
                .downcast::<crate::issue_detail::IssueDetailView>()
            {
                detail.update(cx, |detail, cx| detail.flush_description(cx));
            }
            unregister_screen(&this.screen, cx);
            // The content views lazily created this window's registries
            // (nav / repo resolver / rail) — mirror the Workspace teardown.
            navigation::remove_window(window_id, cx);
            crate::repo_resolver::remove_window(window_id, cx);
            crate::sidebar::remove_window(window_id, cx);
        })
        .detach();

        Self {
            screen,
            origin,
            content,
            focus_handle: cx.focus_handle(),
            window_title: SharedString::default(),
        }
    }

    /// Move the screen back into a workspace window as a regular tab, then
    /// close this window. Deferred: cross-window updates from inside a
    /// window update silently no-op otherwise.
    fn reattach(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let screen = self.screen.clone();
        let origin = self.origin;
        let this_window = window.window_handle();
        cx.defer(move |cx| {
            if let Some(target) = find_workspace_window(Some(origin), cx) {
                let _ = target.update(cx, |_, window, cx| {
                    navigation::navigate(window, cx, screen.clone());
                    window.activate_window();
                });
            }
            let _ = this_window.update(cx, |_, window, _| window.remove_window());
        });
    }
}

impl Focusable for UndockedScreenWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for UndockedScreenWindow {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let title = navigation::screen_title(&self.screen, cx);
        if self.window_title != title {
            self.window_title = title.clone();
            window.set_window_title(&title);
        }

        let header = h_flex()
            .h(px(34.))
            .w_full()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .px_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().title_bar)
            .child(
                Button::new("reattach-screen")
                    .ghost()
                    .xsmall()
                    .icon(crate::icons::ExpIcon::ExternalLinkIn)
                    .tooltip("Move back into the main window")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.reattach(window, cx);
                    })),
            )
            .child(div().text_sm().child(title))
            .child(div().flex_1());

        // Root overlay layers — same composition rule as `Workspace::render`:
        // without them, `window.open_dialog` (issue delete confirm, image
        // preview) would silently never paint in this window.
        let sheet_layer = Root::render_sheet_layer(window, cx);
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        div()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus_handle)
            .child(
                gpui_component::v_flex()
                    .size_full()
                    .child(header)
                    .child(div().flex_1().min_h_0().child(self.content.clone())),
            )
            .children(sheet_layer)
            .children(dialog_layer)
            .children(notification_layer)
    }
}

// ---------------------------------------------------------------------------
// Undocked terminal tabs
// ---------------------------------------------------------------------------

/// Open one terminal tab in its own native window (or focus the existing
/// one). The tab STAYS in `manager` — the dock hides it via the registry and
/// the new window renders the same `TerminalView`.
pub(crate) fn open_undocked_terminal_tab(
    manager: Entity<TerminalManager>,
    tab_id: TabId,
    origin: AnyWindowHandle,
    cx: &mut App,
) {
    if let Some(existing) = terminal_tab_window(tab_id, cx) {
        activate_window(existing, cx);
        return;
    }
    let title = manager
        .read(cx)
        .tab(tab_id)
        .map(|tab| tab.title().clone())
        .unwrap_or_else(|| "Terminal".into());
    let options = undocked_window_options(size(px(920.), px(520.)), cx);
    cx.spawn(async move |cx| {
        let window = cx.open_window(options, move |window, cx| {
            let view = cx.new(|cx| {
                crate::undocked_terminal::UndockedTerminalWindow::new(
                    manager, tab_id, origin, window, cx,
                )
            });
            cx.new(|cx| Root::new(view, window, cx))
        })?;
        window.update(cx, |_, window, cx| {
            window.set_window_title(&title);
            window.activate_window();
            let _ = cx;
        })?;
        anyhow::Ok(())
    })
    .detach();
}

/// Register an undocked terminal window (called by the view's constructor so
/// registration/unregistration are symmetric around its lifetime).
pub(crate) fn register_terminal_tab(
    tab_id: TabId,
    handle: AnyWindowHandle,
    owner: WindowId,
    cx: &mut App,
) {
    let Some(state) = state(cx) else { return };
    state.update(cx, |state, cx| {
        state.terminal_tabs.insert(tab_id, TerminalEntry { handle, owner });
        cx.notify();
    });
}

/// Bring an undocked-then-returned tab back into view in its owner window:
/// expand the bottom dock, activate the tab, optionally raise the window.
/// Deferred + best-effort (the owner may already be gone).
pub(crate) fn restore_tab_in_owner(
    owner: AnyWindowHandle,
    manager: Entity<TerminalManager>,
    tab_id: TabId,
    raise: bool,
    cx: &mut App,
) {
    cx.defer(move |cx| {
        let _ = owner.update(cx, |_, window, cx| {
            if let Some(workspace) = window
                .root::<Root>()
                .flatten()
                .and_then(|root| root.read(cx).view().clone().downcast::<Workspace>().ok())
            {
                let dock_area = workspace.read(cx).dock_area().clone();
                if let Some(dock) = dock_area.read(cx).bottom_dock().cloned() {
                    if !dock.read(cx).is_open() {
                        dock.update(cx, |dock, cx| dock.set_open(true, window, cx));
                    }
                }
            }
            manager.update(cx, |manager, cx| {
                if let Some(ix) = manager.tabs().iter().position(|tab| tab.id == tab_id) {
                    manager.activate(ix, cx);
                }
            });
            if raise {
                window.activate_window();
            }
        });
    });
}
