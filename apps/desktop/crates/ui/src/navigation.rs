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

use gpui::{
    AnyWindowHandle, App, AppContext as _, Entity, Global, KeyBinding, Window, WindowId,
};
use sync::Store;

use crate::actions::{
    GoBack, OpenAccount, OpenInbox, OpenIssue, OpenMyIssues, OpenProject, OpenSettings,
    OpenSourceControl, SwitchBranch, SwitchWorkspace, SyncNow,
};

/// One center TAB (§4.2, reworked): the center pane is tab-based — every
/// `Screen` value identifies one openable tab (issues and files can be open
/// several at a time; Source Control / Settings / Account are singletons).
/// `None` on [`Navigation::screen`] means "no tab active" — the center shows
/// its empty state. Issue LISTS are not screens: they live in the sidebar
/// tool windows (the rail's Inbox / My Issues / All Issues).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Screen {
    /// Full-page issue detail (`routes/.../issues/$issueIdentifier`).
    IssueDetail { issue_id: String },
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

impl Screen {
    /// Whether the screen can be undocked into its own native window
    /// (EXP-65). Content screens only — Settings/Account are app-config
    /// singletons with near-zero value as standalone windows.
    pub(crate) fn undockable(&self) -> bool {
        matches!(
            self,
            Screen::IssueDetail { .. } | Screen::FileViewer { .. } | Screen::SourceControl
        )
    }
}

/// Human title for a screen — the center tab label, and the undocked
/// window's header/title (EXP-65). Issue titles join the synced identifier
/// live; unknown ids degrade to a generic label.
pub(crate) fn screen_title(screen: &Screen, cx: &App) -> gpui::SharedString {
    match screen {
        Screen::IssueDetail { issue_id } => Store::global(cx)
            .collections()
            .issues
            .read(cx)
            .get(issue_id)
            .map(|issue| gpui::SharedString::from(issue.identifier.clone()))
            .unwrap_or_else(|| "Issue".into()),
        Screen::FileViewer { path } => {
            gpui::SharedString::from(path.rsplit('/').next().unwrap_or(path).to_string())
        }
        Screen::SourceControl => "Source Control".into(),
        Screen::Settings => "Settings".into(),
        Screen::Account => "Account".into(),
    }
}

/// Per-window navigation state. Mutate through [`navigate`] /
/// [`switch_workspace`] / [`go_back`] so observers fire consistently.
pub struct Navigation {
    /// Selected workspace; `None` = "first synced workspace" (resolved at
    /// query time — a fresh install has no selection until sync lands).
    pub workspace_id: Option<String>,
    screen: Option<Screen>,
    back_stack: Vec<Screen>,
    /// The explicitly selected project (the top-bar picker) — the primary
    /// scope for [`active_project_id`] so the picker / files / git / run
    /// surfaces stay populated on every screen.
    last_project_id: Option<String>,
    /// The screen the last [`replace_screen`] displaced (EXP-48 prev/next):
    /// a pending marker the screens panel consumes to swap that tab's
    /// identity in place instead of opening a new tab. Cleared by every
    /// ordinary navigation.
    replaced_screen: Option<Screen>,
    /// EXP-67: a pending "open this issue on its Changes tab" request (the
    /// issue id), set by [`navigate_issue_changes`] and consumed by the
    /// screens panel after it re-points the detail view. Cleared by every
    /// ordinary navigation so a stale marker can't flip an unrelated
    /// issue-open to Changes later.
    pending_issue_changes: Option<String>,
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
            last_project_id: None,
            replaced_screen: None,
            pending_issue_changes: None,
        }
    }

    /// The current screen, `None` until first navigation (default applies).
    #[allow(dead_code)] // views read via `resolved_screen`; raw access for later steps
    pub fn screen(&self) -> Option<&Screen> {
        self.screen.as_ref()
    }

    /// Whether [`go_back`] has anywhere to go.
    #[allow(dead_code)]
    pub fn can_go_back(&self) -> bool {
        !self.back_stack.is_empty()
    }
}

/// DEV-ONLY `EXP_DEV_SCREEN` values: `settings` | `account` | `issue:<uuid>`
/// (anything else = no pre-route).
fn parse_dev_screen(spec: &str) -> Option<Screen> {
    match spec {
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

/// The window's navigation entity, created on first access. A fresh window
/// starts on the LAST workspace **and project** this install had active
/// (persisted in `settings.json`, EXP-116); ids that no longer sync fall back
/// via `active_workspace_id` / `active_project_id` at query time.
pub fn nav_for_window(window: &Window, cx: &mut App) -> Entity<Navigation> {
    let window_id = window.window_handle().window_id();
    if let Some(existing) = cx
        .try_global::<NavRegistry>()
        .and_then(|registry| registry.by_window.get(&window_id).cloned())
    {
        return existing;
    }
    let nav = cx.new(|_| Navigation::new());
    if nav.read(cx).workspace_id.is_none() {
        // The EXP_DEV_WORKSPACE override (Navigation::new) wins over the
        // persisted pair — dev runs must land where they were pointed.
        let last_workspace = load_settings_string(cx, LAST_WORKSPACE_KEY);
        let last_project = load_settings_string(cx, LAST_PROJECT_KEY);
        if last_workspace.is_some() || last_project.is_some() {
            nav.update(cx, |nav, _| {
                nav.workspace_id = last_workspace;
                nav.last_project_id = last_project;
            });
        }
    }
    cx.default_global::<NavRegistry>()
        .by_window
        .insert(window_id, nav.clone());
    nav
}

/// Registry lookup by raw `WindowId` (EXP-65 undock: the undocked window
/// seeds its scope from the ORIGIN window's nav, which it only knows by id).
pub(crate) fn nav_for_window_id(window_id: WindowId, cx: &App) -> Option<Entity<Navigation>> {
    cx.try_global::<NavRegistry>()
        .and_then(|registry| registry.by_window.get(&window_id).cloned())
}

/// Seed a fresh window's navigation from a source window (EXP-65 undock):
/// copy the workspace/project scope and pin the given screen so every
/// scope-resolving surface (`active_project_id` → git bar, `+` shell cwd,
/// Source Control file scope) sees the same context the tab had when it was
/// undocked. No-op scope copy when the source nav is already gone.
pub(crate) fn seed_window_scope(
    window: &Window,
    cx: &mut App,
    source: WindowId,
    screen: Screen,
) {
    let scope = nav_for_window_id(source, cx).map(|nav| {
        let nav = nav.read(cx);
        (nav.workspace_id.clone(), nav.last_project_id.clone())
    });
    let nav = nav_for_window(window, cx);
    nav.update(cx, |nav, cx| {
        if let Some((workspace_id, last_project_id)) = scope {
            nav.workspace_id = workspace_id;
            nav.last_project_id = last_project_id;
        }
        nav.screen = Some(screen);
        cx.notify();
    });
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
        // File-viewer tab switches (FileViewer → FileViewer) replace rather than
        // stack: the back stack keeps the screen the viewer was opened from (one
        // entry), not a trail of individual files. Closing the last tab then
        // returns there reliably instead of resurrecting an already-closed file.
        let replace = matches!(
            (nav.screen.as_ref(), &screen),
            (Some(Screen::FileViewer { .. }), Screen::FileViewer { .. })
        );
        if !replace {
            if let Some(previous) = nav.screen.take() {
                nav.back_stack.push(previous);
            }
        }
        nav.screen = Some(screen);
        nav.replaced_screen = None;
        nav.pending_issue_changes = None;
        cx.notify();
    });
}

/// Navigate to an issue AND land on its **Changes** tab (EXP-67: PR rows in
/// the Reviews tool and issue lanes in the flow graph — the diff is why you
/// clicked). The tab request rides a pending marker the screens panel
/// consumes after re-pointing the detail view; the extra `nav.update` runs
/// unconditionally so the marker is delivered even when the issue's tab is
/// already the active screen (where [`navigate`] no-ops without notifying).
pub fn navigate_issue_changes(window: &Window, cx: &mut App, issue_id: String) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    navigate(
        window,
        cx,
        Screen::IssueDetail {
            issue_id: issue_id.clone(),
        },
    );
    nav.update(cx, |nav, cx| {
        nav.pending_issue_changes = Some(issue_id);
        cx.notify();
    });
}

/// Consume the pending "open on Changes" marker (the issue id it targets).
/// The screens panel calls this from its nav observer after `set_issue`.
pub fn take_pending_issue_changes(nav: &Entity<Navigation>, cx: &mut App) -> Option<String> {
    nav.update(cx, |nav, _| nav.pending_issue_changes.take())
}

/// Swap the current screen IN PLACE (EXP-48 prev/next issue switcher): no
/// back-stack push, and the screens panel replaces the active tab's identity
/// instead of opening a new tab (via the consumed [`take_replaced_screen`]
/// marker). No-op when already on `screen`.
pub fn replace_screen(window: &Window, cx: &mut App, screen: Screen) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    nav.update(cx, |nav, cx| {
        if nav.screen.as_ref() == Some(&screen) {
            return;
        }
        nav.replaced_screen = nav.screen.replace(screen);
        cx.notify();
    });
}

/// Consume the pending in-place-replacement marker (the screen the last
/// [`replace_screen`] displaced). The screens panel calls this from its nav
/// observer to swap that tab's identity instead of pushing a new tab.
pub fn take_replaced_screen(nav: &Entity<Navigation>, cx: &mut App) -> Option<Screen> {
    nav.update(cx, |nav, _| nav.replaced_screen.take())
}

/// Set the active tab DIRECTLY — no back-stack push. Tab clicks and
/// tab-close reactivation use this (only real navigations stack); `None`
/// clears the center (last tab closed).
pub fn set_screen(window: &Window, cx: &mut App, screen: Option<Screen>) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    nav.update(cx, |nav, cx| {
        if nav.screen != screen {
            nav.screen = screen;
            nav.replaced_screen = None;
            nav.pending_issue_changes = None;
            cx.notify();
        }
    });
}

/// Select the window's active project (the top-bar picker) — re-scopes the
/// Files / Source Control / run / shell surfaces. Persisted alongside the
/// workspace so the next launch reopens on the same project (EXP-116).
pub fn set_active_project(window: &Window, cx: &mut App, project_id: String) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    let changed = nav.update(cx, |nav, cx| {
        if nav.last_project_id.as_deref() == Some(project_id.as_str()) {
            return false;
        }
        nav.last_project_id = Some(project_id.clone());
        cx.notify();
        true
    });
    if changed {
        let workspace_id = nav.read(cx).workspace_id.clone();
        persist_nav_state(cx, workspace_id, Some(project_id));
    }
}

/// Pop the back stack (issue detail → board, …).
pub fn go_back(window: &Window, cx: &mut App) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    nav.update(cx, |nav, cx| {
        if let Some(previous) = nav.back_stack.pop() {
            nav.screen = Some(previous);
            nav.replaced_screen = None;
            nav.pending_issue_changes = None;
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
    let changed = nav.update(cx, |nav, cx| {
        if nav.workspace_id.as_deref() == Some(workspace_id.as_str()) {
            return false;
        }
        nav.workspace_id = Some(workspace_id.clone());
        nav.screen = None;
        nav.back_stack.clear();
        nav.last_project_id = None;
        nav.replaced_screen = None;
        nav.pending_issue_changes = None;
        cx.notify();
        true
    });
    if changed {
        // Clearing the project key keeps the file consistent with the
        // in-memory reset above — a restart must not resurrect a project
        // from the workspace this window just left.
        persist_nav_state(cx, Some(workspace_id), None);
    }
}

// -----------------------------------------------------------------------
// Last-workspace/-project persistence (`settings.json`, merge-preserving
// like the `deviceId` key — other subsystems' keys survive)
// -----------------------------------------------------------------------

const LAST_WORKSPACE_KEY: &str = "lastWorkspaceId";
const LAST_PROJECT_KEY: &str = "lastProjectId";

fn settings_json_path(cx: &App) -> Option<std::path::PathBuf> {
    cx.try_global::<crate::session::AuthContext>()
        .map(|auth| auth.data_dir.join("settings.json"))
}

/// A persisted non-empty string value (a fresh window's starting point).
fn load_settings_string(cx: &App, key: &str) -> Option<String> {
    let raw = std::fs::read_to_string(settings_json_path(cx)?).ok()?;
    serde_json::from_str::<serde_json::Value>(&raw)
        .ok()?
        .get(key)?
        .as_str()
        .filter(|id| !id.trim().is_empty())
        .map(str::to_string)
}

/// Remember the window's workspace/project selection for the next launch
/// (best-effort, off-thread). ONE writer for BOTH keys: the `OpenProject`
/// cross-workspace path mutates workspace then project back-to-back, and two
/// independent read-modify-write tasks against the shared `settings.json`
/// could land in either order — the sequence stamp (checked under the write
/// lock) lets a superseded snapshot skip instead of clobbering the newest.
/// `workspace_id: None` leaves the workspace key untouched;
/// `project_id: None` REMOVES the project key (workspace switch reset).
fn persist_nav_state(cx: &mut App, workspace_id: Option<String>, project_id: Option<String>) {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;
    static SEQ: AtomicU64 = AtomicU64::new(0);
    static WRITE_LOCK: Mutex<()> = Mutex::new(());

    let Some(path) = settings_json_path(cx) else {
        return;
    };
    let seq = SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    cx.background_executor()
        .spawn(async move {
            let _guard = WRITE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
            if SEQ.load(Ordering::SeqCst) != seq {
                return; // a newer snapshot is queued (or already written)
            }
            let mut root = std::fs::read_to_string(&path)
                .ok()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                .filter(serde_json::Value::is_object)
                .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
            if let Some(object) = root.as_object_mut() {
                if let Some(workspace_id) = workspace_id {
                    object.insert(
                        LAST_WORKSPACE_KEY.to_string(),
                        serde_json::Value::String(workspace_id),
                    );
                }
                match project_id {
                    Some(project_id) => {
                        object.insert(
                            LAST_PROJECT_KEY.to_string(),
                            serde_json::Value::String(project_id),
                        );
                    }
                    None => {
                        object.remove(LAST_PROJECT_KEY);
                    }
                }
            }
            let write = || -> std::io::Result<()> {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut rendered =
                    serde_json::to_string_pretty(&root).unwrap_or_else(|_| "{}".to_string());
                rendered.push('\n');
                std::fs::write(&path, rendered)
            };
            if let Err(err) = write() {
                log::warn!("[ui] persisting nav state failed: {err}");
            }
        })
        .detach();
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
    // My Issues / Inbox are tool windows, not screens — the actions select
    // the rail tool (the sidebar swaps to the mini list; the center stays).
    cx.on_action(|_: &OpenMyIssues, cx| {
        on_active_window(cx, |window, cx| {
            crate::sidebar::activate_tool(window, cx, crate::sidebar::ToolWindow::MyIssues);
        });
    });
    cx.on_action(|_: &OpenInbox, cx| {
        on_active_window(cx, |window, cx| {
            crate::sidebar::activate_tool(window, cx, crate::sidebar::ToolWindow::Inbox);
        });
    });
    cx.on_action(|_: &OpenSettings, cx| navigate_active(cx, Screen::Settings));
    cx.on_action(|_: &OpenAccount, cx| navigate_active(cx, Screen::Account));
    cx.on_action(|_: &OpenSourceControl, cx| {
        on_active_window(cx, |window, cx| {
            crate::sidebar::activate_tool(window, cx, crate::sidebar::ToolWindow::SourceControl);
        });
    });
    // Branch chip menu → checkout on the window's shared git bar.
    cx.on_action(|action: &SwitchBranch, cx| {
        let branch = action.branch.clone();
        on_active_window(cx, move |window, cx| {
            let shared = crate::sidebar::rail_shared_for_window(window, cx);
            let git_bar = shared.read(cx).git_bar().clone();
            git_bar.update(cx, |bar, cx| bar.checkout(branch, window, cx));
        });
    });
    // Branch chip menu → manual freshness sync (fetch + ff-only catch-up).
    cx.on_action(|_: &SyncNow, cx| {
        on_active_window(cx, |window, cx| {
            let shared = crate::sidebar::rail_shared_for_window(window, cx);
            let git_bar = shared.read(cx).git_bar().clone();
            git_bar.update(cx, |bar, cx| bar.refresh(cx));
        });
    });
    // The picker selects a project (scope) and brings up its issue list —
    // there is no board screen; the All Issues tool window IS the board.
    cx.on_action(|action: &OpenProject, cx| {
        let project_id = action.project_id.clone();
        on_active_window(cx, move |window, cx| {
            // EXP-69 merged picker: a project picked from ANOTHER workspace
            // switches the window's workspace first (same reset semantics as
            // the old footer switcher — screen + back stack cleared), then
            // scopes to the picked project. One action, one gesture.
            let nav = nav_for_window(window, cx);
            let project_workspace = Store::global(cx)
                .collections()
                .projects
                .read(cx)
                .get(&project_id)
                .map(|project| project.workspace_id.clone());
            if let Some(project_workspace) = project_workspace {
                if active_workspace_id(&nav, cx).as_deref()
                    != Some(project_workspace.as_str())
                {
                    switch_workspace(window, cx, project_workspace);
                }
            }
            set_active_project(window, cx, project_id);
            crate::sidebar::activate_tool(window, cx, crate::sidebar::ToolWindow::AllIssues);
        });
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
    cx.on_action(|_: &GoBack, cx| {
        on_active_window(cx, |window, cx| go_back(window, cx));
    });
    // App-global back binding (§8.11): `cmd-[` on macOS, `Alt+Left` everywhere
    // (the browser-style back chord). `None` context = fires regardless of
    // focus, matching the ⌘K search binding.
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-[", GoBack, None),
        KeyBinding::new("alt-left", GoBack, None),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([KeyBinding::new("alt-left", GoBack, None)]);
}

fn navigate_active(cx: &mut App, screen: Screen) {
    on_active_window(cx, move |window, cx| {
        navigate(window, cx, screen);
    });
}

// -----------------------------------------------------------------------
// Default-screen resolution (shared by screens panel + sidebar highlight)
// -----------------------------------------------------------------------

/// The active center TAB — `None` = nothing open (the center renders its
/// empty state once [`shapes_ready`]; a skeleton before). There is no default
/// screen anymore: issue lists live in the sidebar, the center only shows
/// what was explicitly opened.
pub fn resolved_screen(nav: &Entity<Navigation>, cx: &App) -> Option<Screen> {
    nav.read(cx).screen.clone()
}

/// Whether the workspaces + projects shapes have seen their first
/// `up-to-date` — the gate between "skeleton" and real empty states.
pub fn shapes_ready(cx: &App) -> bool {
    let collections = Store::global(cx).collections();
    collections.projects.read(cx).is_ready() && collections.workspaces.read(cx).is_ready()
}

/// The window's active PROJECT — the scope every repo-backed surface (files
/// tree, git chrome, run configs, source control, `+` shell cwd, project
/// picker, All Issues list) resolves through. Resolution order: the
/// explicitly picked project (while it still exists in the active
/// workspace), then the active issue tab's project (its synced row), then
/// the workspace's first project.
pub fn active_project_id(nav: &Entity<Navigation>, cx: &App) -> Option<String> {
    let collections = Store::global(cx).collections();
    let workspace_id = active_workspace_id(nav, cx)?;
    if let Some(picked) = nav.read(cx).last_project_id.clone() {
        let still_here = collections
            .projects
            .read(cx)
            .get(&picked)
            .is_some_and(|project| project.workspace_id == workspace_id);
        if still_here {
            return Some(picked);
        }
    }
    if let Some(Screen::IssueDetail { issue_id }) = resolved_screen(nav, cx) {
        if let Some(project_id) = collections
            .issues
            .read(cx)
            .get(&issue_id)
            .map(|issue| issue.project_id.clone())
        {
            return Some(project_id);
        }
    }
    collections
        .projects_in_workspace(&workspace_id, cx)
        .first()
        .map(|project| project.id.clone())
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
