//! Per-window screen routing (masterplan-v3 §4.2 — "sidebar selection →
//! center content swap"; the desktop analog of the web's TanStack routes
//! under `routes/w/$workspaceSlug/`).
//!
//! Model: every workspace window owns one [`Navigation`] entity — the active
//! workspace + the current [`Screen`] + a back stack. Views observe it
//! (`cx.observe`) and re-render on change; the `screens::ScreensPanel` in the
//! center dock swaps its content on the current screen.
//!
//! The entities live in a window-keyed registry global so BOTH construction
//! paths reach the same instance: the `Workspace` shell creates it, and
//! panels rebuilt by the §3.3 `register_panel` cold-restore path (which only
//! get `(window, cx)`) look it up by `WindowId`. The `Workspace` removes the
//! entry on window release.
//!
//! Chrome affordances dispatch typed actions (`crate::actions` — §3.6);
//! [`init`] registers **App-global** handlers that resolve the active window
//! and call [`navigate`]. Global handlers are load-bearing here: menu items
//! render in the `Root` overlay layer, so an element-tree `.on_action` on the
//! workspace div would never see actions dispatched from an open menu.
//! In-tree click handlers that already hold `(window, cx)` (issue rows,
//! sidebar items) may call [`navigate`] directly.

use std::collections::HashMap;

use gpui::{AnyWindowHandle, App, AppContext as _, Entity, Global, Window, WindowId};
use sync::Store;

use crate::actions::{
    OpenAccount, OpenInbox, OpenIssue, OpenMyIssues, OpenProject, OpenSettings, SwitchWorkspace,
};

/// Which surface fills the center panel (§4.2's screen map). `None` on
/// [`Navigation::screen`] means "not navigated yet" — the screens panel
/// resolves the default (first project board, else My Issues) once the
/// projects collection is ready (§4.1 `is_ready` gating).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Project board (`routes/w/$ws/projects/$project/index.tsx`).
    Board { project_id: String },
    /// Full-page issue detail (`routes/.../issues/$issueIdentifier`).
    IssueDetail { issue_id: String },
    /// `routes/w/$ws/my-issues/`.
    MyIssues,
    /// `routes/w/$ws/inbox/`.
    Inbox,
    /// `routes/w/$ws/settings/`.
    Settings,
    /// `routes/_authenticated/account/*` (integrations + notifications).
    Account,
    /// Trunk Source Control screen (masterplan v4 §4.4). Trunk-only — no
    /// project/issue scope (the active workspace's trunk clone).
    SourceControl,
    /// Read-only trunk file viewer (masterplan v4 §4.5); `path` is
    /// trunk-relative.
    FileViewer { path: String },
}

/// Per-window navigation state. Mutate through [`navigate`] /
/// [`switch_workspace`] / [`go_back`] so observers fire consistently.
pub struct Navigation {
    /// Selected workspace; `None` = "first synced workspace" (resolved at
    /// query time — a fresh install has no selection until sync lands).
    pub workspace_id: Option<String>,
    screen: Option<Screen>,
    back_stack: Vec<Screen>,
}

impl Navigation {
    fn new() -> Self {
        Self {
            // DEV-ONLY (§11.4 headless verification, same family as
            // EXP_DEV_SERVER/EXP_DEV_BOARD): pre-select a workspace and/or
            // pre-route the first screen so gate screenshots can reach
            // surfaces without synthetic input. Unset in normal runs.
            workspace_id: std::env::var("EXP_DEV_WORKSPACE").ok(),
            screen: std::env::var("EXP_DEV_SCREEN")
                .ok()
                .as_deref()
                .and_then(parse_dev_screen),
            back_stack: Vec::new(),
        }
    }

    /// The current screen, `None` until first navigation (default applies).
    #[allow(dead_code)] // views read via `resolved_screen`; raw access for later steps
    pub fn screen(&self) -> Option<&Screen> {
        self.screen.as_ref()
    }

    /// Whether [`go_back`] has anywhere to go.
    #[allow(dead_code)] // consumer = detail-header back affordance (later Phase-3 step)
    pub fn can_go_back(&self) -> bool {
        !self.back_stack.is_empty()
    }
}

/// DEV-ONLY `EXP_DEV_SCREEN` values: `my-issues` | `inbox` | `settings` |
/// `account` | `issue:<uuid>` (anything else = no pre-route).
fn parse_dev_screen(spec: &str) -> Option<Screen> {
    match spec {
        "my-issues" => Some(Screen::MyIssues),
        "inbox" => Some(Screen::Inbox),
        "settings" => Some(Screen::Settings),
        "account" => Some(Screen::Account),
        _ => spec.strip_prefix("issue:").map(|id| Screen::IssueDetail {
            issue_id: id.to_string(),
        }),
    }
}

/// Window-keyed registry of navigation entities.
#[derive(Default)]
struct NavRegistry {
    by_window: HashMap<WindowId, Entity<Navigation>>,
}

impl Global for NavRegistry {}

/// The window's navigation entity, created on first access.
pub fn nav_for_window(window: &Window, cx: &mut App) -> Entity<Navigation> {
    let window_id = window.window_handle().window_id();
    if let Some(existing) = cx
        .try_global::<NavRegistry>()
        .and_then(|registry| registry.by_window.get(&window_id).cloned())
    {
        return existing;
    }
    let nav = cx.new(|_| Navigation::new());
    cx.default_global::<NavRegistry>()
        .by_window
        .insert(window_id, nav.clone());
    nav
}

/// Drop a closed window's entry (called from the `Workspace` release hook —
/// entities die with the window; the registry must not leak handles).
pub fn remove_window(window_id: WindowId, cx: &mut App) {
    if let Some(registry) = cx.try_global::<NavRegistry>() {
        if registry.by_window.contains_key(&window_id) {
            cx.global_mut::<NavRegistry>().by_window.remove(&window_id);
        }
    }
}

/// Navigate the window to `screen`, pushing the previous screen onto the
/// back stack (no-op when already there).
pub fn navigate(window: &Window, cx: &mut App, screen: Screen) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    nav.update(cx, |nav, cx| {
        if nav.screen.as_ref() == Some(&screen) {
            return;
        }
        if let Some(previous) = nav.screen.take() {
            nav.back_stack.push(previous);
        }
        nav.screen = Some(screen);
        cx.notify();
    });
}

/// Pop the back stack (issue detail → board, …).
#[allow(dead_code)] // consumer = the §3.6 keymap back binding (later step);
                    // the stack itself is maintained by `navigate`
pub fn go_back(window: &Window, cx: &mut App) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    nav.update(cx, |nav, cx| {
        if let Some(previous) = nav.back_stack.pop() {
            nav.screen = Some(previous);
            cx.notify();
        }
    });
}

/// Switch the window's active workspace. Resets the screen + back stack —
/// screens are workspace-scoped (a board of workspace A is meaningless in B);
/// the default-screen resolution then picks the new workspace's first board.
pub fn switch_workspace(window: &Window, cx: &mut App, workspace_id: String) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    nav.update(cx, |nav, cx| {
        if nav.workspace_id.as_deref() == Some(workspace_id.as_str()) {
            return;
        }
        nav.workspace_id = Some(workspace_id);
        nav.screen = None;
        nav.back_stack.clear();
        cx.notify();
    });
}

/// Read-only registry lookup (used by mutators so a dispatch on a window
/// whose workspace never initialized nav — e.g. the login surface — is a
/// clean no-op instead of creating orphan state).
fn nav_for_window_readonly(window: &Window, cx: &App) -> Option<Entity<Navigation>> {
    let window_id = window.window_handle().window_id();
    cx.try_global::<NavRegistry>()
        .and_then(|registry| registry.by_window.get(&window_id).cloned())
}

/// Resolve the window a global action should target.
///
/// [`App::active_window`] is the right source. The fallback to the sole window
/// only matters on the Linux backend, where `active_window` is derived from the
/// compositor's `keyboard_focused_window` and can momentarily be `None` (e.g.
/// under focus-follows-mouse before the first keyboard enter); with a single
/// window there is no ambiguity. With several windows and no active one we
/// can't tell them apart, so we return `None` (unchanged prior behavior).
pub fn active_or_primary_window(cx: &App) -> Option<AnyWindowHandle> {
    if let Some(window) = cx.active_window() {
        return Some(window);
    }
    let windows = cx.windows();
    if windows.len() == 1 {
        windows.into_iter().next()
    } else {
        None
    }
}

/// Run `f` with the target window's `&mut Window`, **deferred**.
///
/// Every App-global `cx.on_action` handler runs INSIDE gpui's `window.update`:
/// [`gpui::Window::dispatch_action`] defers its work into
/// `window.update(|_, window, cx| dispatch_action_on_node(..))`, and the global
/// action listeners fire from there. `update_window_id` `.take()`s the window
/// out of its slot for the duration of an update, so calling `window.update`
/// *again* on that same window — synchronously, from the handler — finds `None`
/// and returns `Err("window not found")` WITHOUT ever running the closure (the
/// usual `let _ =` then swallows the error). That silent re-entrancy failure is
/// why every action-dispatched nav/dialog appeared completely dead.
///
/// Deferring lets the outer dispatch update unwind first (window back in its
/// slot), so the re-entrant `window.update` succeeds and `f` actually runs.
pub fn on_active_window(cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
    let Some(window) = active_or_primary_window(cx) else {
        return;
    };
    cx.defer(move |cx| {
        let _ = window.update(cx, move |_, window, cx| f(window, cx));
    });
}

/// Register the App-global action handlers (call once from `ui::init`).
/// Actions navigate the **active** window — nav actions only ever originate
/// from user interaction (sidebar, menus, future keymap), which happens in
/// the active window.
pub fn init(cx: &mut App) {
    cx.on_action(|_: &OpenMyIssues, cx| navigate_active(cx, Screen::MyIssues));
    cx.on_action(|_: &OpenInbox, cx| navigate_active(cx, Screen::Inbox));
    cx.on_action(|_: &OpenSettings, cx| navigate_active(cx, Screen::Settings));
    cx.on_action(|_: &OpenAccount, cx| navigate_active(cx, Screen::Account));
    cx.on_action(|action: &OpenProject, cx| {
        let project_id = action.project_id.clone();
        navigate_active(cx, Screen::Board { project_id });
    });
    cx.on_action(|action: &OpenIssue, cx| {
        let issue_id = action.issue_id.clone();
        navigate_active(cx, Screen::IssueDetail { issue_id });
    });
    cx.on_action(|action: &SwitchWorkspace, cx| {
        let workspace_id = action.workspace_id.clone();
        on_active_window(cx, move |window, cx| {
            switch_workspace(window, cx, workspace_id);
        });
    });
}

fn navigate_active(cx: &mut App, screen: Screen) {
    on_active_window(cx, move |window, cx| {
        navigate(window, cx, screen);
    });
}

// -----------------------------------------------------------------------
// Default-screen resolution (shared by screens panel + sidebar highlight)
// -----------------------------------------------------------------------

/// What the center should show right now — [`Screen`] resolution including
/// the defaults (§4.1): explicit navigation wins; otherwise the first project
/// board of the active workspace; otherwise My Issues; and while the
/// `projects`/`workspaces` shapes have not seen their first `up-to-date`,
/// `None` = "still syncing, render a skeleton, never a wrong default".
pub fn resolved_screen(nav: &Entity<Navigation>, cx: &App) -> Option<Screen> {
    if let Some(screen) = nav.read(cx).screen.clone() {
        return Some(screen);
    }
    let collections = Store::global(cx).collections();
    if !collections.projects.read(cx).is_ready()
        || !collections.workspaces.read(cx).is_ready()
    {
        return None;
    }
    let workspace_id = active_workspace_id(nav, cx)?;
    let projects = collections.projects_in_workspace(&workspace_id, cx);
    match projects.first() {
        Some(project) => Some(Screen::Board {
            project_id: project.id.clone(),
        }),
        None => Some(Screen::MyIssues),
    }
}

/// The active workspace id: the explicit selection when it still exists,
/// else the first synced workspace (name-sorted, web picker order).
pub fn active_workspace_id(nav: &Entity<Navigation>, cx: &App) -> Option<String> {
    let collections = Store::global(cx).collections();
    let selected = nav.read(cx).workspace_id.clone();
    if let Some(id) = selected {
        if collections.workspaces.read(cx).get(&id).is_some() {
            return Some(id);
        }
    }
    collections
        .workspaces_sorted(cx)
        .first()
        .map(|workspace| workspace.id.clone())
}
