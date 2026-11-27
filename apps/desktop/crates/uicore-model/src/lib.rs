//! `uicore-model` (#EDE-50): the pure shell reducer both hosts share
//! (`ide = shell + uicore`, `ede = compositor + uicore`). No clock, no
//! I/O: the host feeds it what the compositor and the person do
//! ([`reduce`]) and reads the state back through the selectors. Byte-locked
//! by `packages/uicore/fixtures/shell.json` (its `$comment` is the spec; the
//! TypeScript reference is `packages/uicore/src/shell.ts`; `tests/replay.rs`
//! replays every case here) and `shell-layout.json` ([`layout`]).

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod chat;
pub mod chats;
pub mod ide;
pub mod layout;
pub mod session_area;
pub mod spaces;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SwitcherKind {
    Local,
    Team,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwitcherEntry {
    pub kind: SwitcherKind,
    pub id: String,
    pub name: String,
    /// EDE-56: the Exponential account a team belongs to (its id); a local desktop has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// EDE-56: the account's email, the section's label.
    #[serde(default, rename = "accountName", skip_serializing_if = "Option::is_none")]
    pub account_name: Option<String>,
}

impl SwitcherEntry {
    pub fn local(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self { kind: SwitcherKind::Local, id: id.into(), name: name.into(), account: None, account_name: None }
    }

    pub fn team(id: impl Into<String>, name: impl Into<String>, account: impl Into<String>, account_name: impl Into<String>) -> Self {
        Self { kind: SwitcherKind::Team, id: id.into(), name: name.into(), account: Some(account.into()), account_name: Some(account_name.into()) }
    }
}

/// EDE-56: one section of the switcher ([`switcher_sections`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwitcherSection {
    pub kind: SwitcherSectionKind,
    pub id: String,
    pub label: String,
    pub entries: Vec<SwitcherSectionEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SwitcherSectionKind {
    Local,
    Account,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwitcherSectionEntry {
    #[serde(flatten)]
    pub entry: SwitcherEntry,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Switcher {
    pub current: SwitcherEntry,
    pub entries: Vec<SwitcherEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellWindow {
    pub id: String,
    pub app: String,
    pub title: String,
    pub focused: bool,
    pub workspace: usize,
    pub minimized: bool,
    /// #EDE-255: asks for attention until focused (absent when false).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub urgent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspaces {
    pub active: usize,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Taskbar {
    pub pinned: Vec<String>,
    pub hover: Option<String>,
    pub previews: Option<String>,
    pub menu: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartMenu {
    pub open: bool,
    pub query: String,
    pub category: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AltTab {
    pub open: bool,
    pub selected: usize,
    /// The popup is up (a quick Alt+Tab commits before the shell's delay shows it).
    pub shown: bool,
    /// Alt+`: only this app's windows cycle.
    pub app: Option<String>,
}

impl AltTab {
    pub fn closed() -> Self {
        Self { open: false, selected: 0, shown: false, app: None }
    }
}

/// The whole shell state; every key is always present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellState {
    pub switcher: Switcher,
    pub windows: Vec<ShellWindow>,
    pub mru: Vec<String>,
    pub workspaces: Workspaces,
    pub taskbar: Taskbar,
    pub start_menu: StartMenu,
    pub alt_tab: AltTab,
}

impl Default for ShellState {
    /// The fixture's `initial`: one local desktop, no windows, one workspace.
    fn default() -> Self {
        Self {
            switcher: Switcher {
                current: SwitcherEntry::local("local", "Local"),
                entries: vec![SwitcherEntry::local("local", "Local")],
            },
            windows: Vec::new(),
            mru: Vec::new(),
            workspaces: Workspaces { active: 0, count: 1 },
            taskbar: Taskbar { pinned: Vec::new(), hover: None, previews: None, menu: None },
            start_menu: StartMenu { open: false, query: String::new(), category: String::new() },
            alt_tab: AltTab::closed(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScrollDirection {
    Up,
    Down,
}

/// What the compositor and the person do, in the fixture's wire form
/// (`{"kind": "window.opened", "id": …}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ShellEvent {
    #[serde(rename = "switcher.pick")]
    SwitcherPick {
        #[serde(rename = "entryKind")]
        entry_kind: SwitcherKind,
        id: String,
    },
    #[serde(rename = "switcher.set")]
    SwitcherSet { entries: Vec<SwitcherEntry> },
    #[serde(rename = "window.opened")]
    WindowOpened { id: String, app: String, title: String, workspace: usize },
    #[serde(rename = "window.closed")]
    WindowClosed { id: String },
    #[serde(rename = "window.focused")]
    WindowFocused { id: String },
    #[serde(rename = "window.titled")]
    WindowTitled { id: String, title: String },
    #[serde(rename = "window.minimized")]
    WindowMinimized { id: String },
    #[serde(rename = "window.restored")]
    WindowRestored { id: String },
    #[serde(rename = "window.moved")]
    WindowMoved { id: String, workspace: i64 },
    /// #EDE-255: the compositor marked it urgent (true) or the call ended.
    #[serde(rename = "window.urgent")]
    WindowUrgent { id: String, urgent: bool },
    #[serde(rename = "workspace.switched")]
    WorkspaceSwitched { active: i64 },
    #[serde(rename = "workspace.count")]
    WorkspaceCount { count: i64 },
    #[serde(rename = "taskbar.pin")]
    TaskbarPin { app: String },
    #[serde(rename = "taskbar.unpin")]
    TaskbarUnpin { app: String },
    #[serde(rename = "taskbar.hover")]
    TaskbarHover { app: String },
    #[serde(rename = "taskbar.unhover")]
    TaskbarUnhover,
    #[serde(rename = "taskbar.previews")]
    TaskbarPreviews { app: Option<String> },
    #[serde(rename = "taskbar.menu")]
    TaskbarMenu { app: Option<String> },
    #[serde(rename = "startMenu.toggle")]
    StartMenuToggle,
    #[serde(rename = "startMenu.open")]
    StartMenuOpen,
    #[serde(rename = "startMenu.close")]
    StartMenuClose,
    #[serde(rename = "startMenu.query")]
    StartMenuQuery { query: String },
    #[serde(rename = "startMenu.category")]
    StartMenuCategory { category: String },
    #[serde(rename = "altTab.next")]
    AltTabNext,
    #[serde(rename = "altTab.prev")]
    AltTabPrev,
    /// Alt+` / Shift+Alt+`: the focused app's windows only.
    #[serde(rename = "altTab.nextApp")]
    AltTabNextApp,
    #[serde(rename = "altTab.prevApp")]
    AltTabPrevApp,
    /// The shell's delay elapsed with Alt still held: the popup shows.
    #[serde(rename = "altTab.show")]
    AltTabShow,
    /// A hovered entry, or an arrow key's target.
    #[serde(rename = "altTab.select")]
    AltTabSelect { index: i64 },
    #[serde(rename = "altTab.commit")]
    AltTabCommit,
    #[serde(rename = "altTab.cancel")]
    AltTabCancel,
}

fn closed_start_menu() -> StartMenu {
    StartMenu { open: false, query: String::new(), category: String::new() }
}

fn focus(s: &mut ShellState, id: &str) {
    if !s.windows.iter().any(|w| w.id == id) {
        return;
    }
    for w in &mut s.windows {
        w.focused = w.id == id;
        if w.focused {
            w.minimized = false;
            w.urgent = false;
        }
    }
    s.mru.retain(|m| m != id);
    s.mru.insert(0, id.to_string());
}

fn clamp_alt_tab(s: &mut ShellState) {
    if !s.alt_tab.open {
        return;
    }
    let n = alt_tab_order(s).len();
    if n == 0 {
        s.alt_tab = AltTab::closed();
    } else if s.alt_tab.selected >= n {
        s.alt_tab.selected = n - 1;
    }
}

fn open_start_menu(s: &mut ShellState) {
    s.start_menu.open = true;
    s.taskbar.previews = None;
    s.taskbar.menu = None;
}

/// `reduce(state, event) -> state`: pure; an unknown id or
/// workspace leaves the state as it was.
pub fn reduce(given: &ShellState, event: &ShellEvent) -> ShellState {
    let mut s = given.clone();
    match event {
        ShellEvent::SwitcherPick { entry_kind, id } => {
            if let Some(hit) = s.switcher.entries.iter().find(|x| &x.kind == entry_kind && &x.id == id) {
                s.switcher.current = hit.clone();
                // #EDE-65: the switcher lives in the start menu; a pick closes it.
                s.start_menu = closed_start_menu();
            }
        }
        ShellEvent::SwitcherSet { entries } => {
            if !entries.is_empty() {
                let kept = entries.iter().find(|x| x.kind == s.switcher.current.kind && x.id == s.switcher.current.id).cloned();
                s.switcher.entries = entries.clone();
                s.switcher.current = kept.unwrap_or_else(|| entries[0].clone());
            }
        }
        ShellEvent::WindowOpened { id, app, title, workspace } => {
            if !s.windows.iter().any(|w| &w.id == id) {
                s.windows.push(ShellWindow { id: id.clone(), app: app.clone(), title: title.clone(), focused: false, workspace: *workspace, minimized: false, urgent: false });
                s.mru.push(id.clone());
            }
        }
        ShellEvent::WindowClosed { id } => {
            if let Some(gone) = s.windows.iter().find(|w| &w.id == id).cloned() {
                s.windows.retain(|w| &w.id != id);
                s.mru.retain(|m| m != id);
                if s.taskbar.previews.as_deref() == Some(gone.app.as_str()) && !s.windows.iter().any(|w| w.app == gone.app) {
                    s.taskbar.previews = None;
                }
                clamp_alt_tab(&mut s);
            }
        }
        ShellEvent::WindowFocused { id } => focus(&mut s, id),
        ShellEvent::WindowTitled { id, title } => {
            if let Some(w) = s.windows.iter_mut().find(|w| &w.id == id) {
                w.title = title.clone();
            }
        }
        ShellEvent::WindowMinimized { id } => {
            if let Some(w) = s.windows.iter_mut().find(|w| &w.id == id) {
                w.minimized = true;
                w.focused = false;
            }
        }
        ShellEvent::WindowUrgent { id, urgent } => {
            if let Some(w) = s.windows.iter_mut().find(|w| &w.id == id) {
                w.urgent = *urgent && !w.focused;
            }
        }
        ShellEvent::WindowRestored { id } => {
            if let Some(w) = s.windows.iter_mut().find(|w| &w.id == id) {
                w.minimized = false;
            }
        }
        ShellEvent::WindowMoved { id, workspace } => {
            let count = s.workspaces.count as i64;
            if let Some(w) = s.windows.iter_mut().find(|w| &w.id == id) {
                if *workspace >= 0 && *workspace < count {
                    w.workspace = *workspace as usize;
                }
            }
            clamp_alt_tab(&mut s);
        }
        ShellEvent::WorkspaceSwitched { active } => {
            if *active >= 0 && *active < s.workspaces.count as i64 {
                s.workspaces.active = *active as usize;
            }
            clamp_alt_tab(&mut s);
        }
        ShellEvent::WorkspaceCount { count } => {
            let count = (*count).max(1) as usize;
            s.workspaces.count = count;
            if s.workspaces.active >= count {
                s.workspaces.active = count - 1;
            }
            for w in &mut s.windows {
                if w.workspace >= count {
                    w.workspace = count - 1;
                }
            }
        }
        ShellEvent::TaskbarPin { app } => {
            if !s.taskbar.pinned.iter().any(|a| a == app) {
                s.taskbar.pinned.push(app.clone());
            }
        }
        ShellEvent::TaskbarUnpin { app } => s.taskbar.pinned.retain(|a| a != app),
        ShellEvent::TaskbarHover { app } => s.taskbar.hover = Some(app.clone()),
        ShellEvent::TaskbarUnhover => s.taskbar.hover = None,
        ShellEvent::TaskbarPreviews { app } => {
            s.taskbar.previews = app.clone();
            if s.taskbar.previews.is_some() {
                s.taskbar.menu = None;
                s.start_menu = closed_start_menu();
            }
        }
        ShellEvent::TaskbarMenu { app } => {
            s.taskbar.menu = app.clone();
            if s.taskbar.menu.is_some() {
                s.taskbar.previews = None;
                s.start_menu = closed_start_menu();
            }
        }
        ShellEvent::StartMenuToggle => {
            if s.start_menu.open {
                s.start_menu = closed_start_menu();
            } else {
                open_start_menu(&mut s);
            }
        }
        ShellEvent::StartMenuOpen => open_start_menu(&mut s),
        ShellEvent::StartMenuClose => s.start_menu = closed_start_menu(),
        ShellEvent::StartMenuQuery { query } => s.start_menu.query = query.clone(),
        ShellEvent::StartMenuCategory { category } => s.start_menu.category = category.clone(),
        ShellEvent::AltTabNext | ShellEvent::AltTabPrev | ShellEvent::AltTabNextApp | ShellEvent::AltTabPrevApp => {
            let forward = matches!(event, ShellEvent::AltTabNext | ShellEvent::AltTabNextApp);
            if s.alt_tab.open {
                let n = alt_tab_order(&s).len();
                if n > 0 {
                    s.alt_tab.selected = (s.alt_tab.selected + if forward { 1 } else { n - 1 }) % n;
                }
            } else if let Some(first) = alt_tab_order(&s).first().cloned() {
                let app = matches!(event, ShellEvent::AltTabNextApp | ShellEvent::AltTabPrevApp).then(|| s.windows.iter().find(|w| w.id == first).map(|w| w.app.clone())).flatten();
                s.alt_tab = AltTab { open: true, selected: 0, shown: false, app };
                let n = alt_tab_order(&s).len();
                s.alt_tab.selected = if n >= 2 { if forward { 1 } else { n - 1 } } else { 0 };
            }
        }
        ShellEvent::AltTabShow => {
            if s.alt_tab.open {
                s.alt_tab.shown = true;
            }
        }
        ShellEvent::AltTabSelect { index } => {
            if s.alt_tab.open && *index >= 0 && (*index as usize) < alt_tab_order(&s).len() {
                s.alt_tab.selected = *index as usize;
            }
        }
        ShellEvent::AltTabCommit => {
            if s.alt_tab.open {
                if let Some(id) = alt_tab_order(&s).get(s.alt_tab.selected).cloned() {
                    focus(&mut s, &id);
                }
                s.alt_tab = AltTab::closed();
            }
        }
        ShellEvent::AltTabCancel => s.alt_tab = AltTab::closed(),
    }
    s
}

// ---- selectors -------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskbarButton {
    pub app: String,
    pub pinned: bool,
    pub windows: Vec<String>,
    pub focused: bool,
    pub minimized: bool,
    pub running: bool,
    /// #EDE-255: one of its windows asks for attention (absent when false).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub urgent: bool,
}

fn on_active(state: &ShellState) -> Vec<&ShellWindow> {
    state.windows.iter().filter(|w| w.workspace == state.workspaces.active).collect()
}

/// Mint's grouped window list: pinned apps first in pin order, then the
/// running unpinned apps in the order their first window appeared; the
/// active workspace only.
pub fn taskbar_buttons(state: &ShellState) -> Vec<TaskbarButton> {
    let active = on_active(state);
    let mut apps: Vec<String> = state.taskbar.pinned.clone();
    for w in &active {
        if !apps.contains(&w.app) {
            apps.push(w.app.clone());
        }
    }
    apps.into_iter()
        .map(|app| {
            let ws: Vec<&&ShellWindow> = active.iter().filter(|w| w.app == app).collect();
            TaskbarButton {
                pinned: state.taskbar.pinned.contains(&app),
                windows: ws.iter().map(|w| w.id.clone()).collect(),
                focused: ws.iter().any(|w| w.focused),
                minimized: !ws.is_empty() && ws.iter().all(|w| w.minimized),
                running: !ws.is_empty(),
                urgent: ws.iter().any(|w| w.urgent),
                app,
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum TaskbarPress {
    Launch,
    Minimize { id: String },
    Focus { id: String },
}

/// #EDE-254: the group's last focused window: of the app's
/// active-workspace windows the one earliest in the mru, else the first.
/// #EDE-255: an urgent one first (Cinnamon makes the window that demands
/// attention the group's lastFocused, so a click answers it).
pub fn last_focused<'a>(state: &'a ShellState, app: &str) -> Option<&'a ShellWindow> {
    let ws: Vec<&ShellWindow> = on_active(state).into_iter().filter(|w| w.app == app).collect();
    let mru = |ws: &[&'a ShellWindow]| state.mru.iter().find_map(|id| ws.iter().find(|w| &w.id == id).copied()).or(ws.first().copied());
    let urgent: Vec<&ShellWindow> = ws.iter().copied().filter(|w| w.urgent).collect();
    mru(&urgent).or_else(|| mru(&ws))
}

/// #EDE-255: grouped-window-list's flash (FLASH_INTERVAL 500 ms, FLASH_MAX_COUNT
/// 4): is an urgent group's highlight lit `ms` after the call? Off for the
/// first interval, then on and off by turns, lit for good from the fifth.
pub const TASKBAR_FLASH_MS: u64 = 500;

pub fn urgent_lit(ms: u64) -> bool {
    let phase = ms / TASKBAR_FLASH_MS;
    phase >= 5 || phase % 2 == 1
}

/// Mint's click on a button (grouped-window-list's left-click-action 2):
/// launch, else toggle the group's last focused window (minimize it while
/// it has the focus, focus it otherwise). Previews show on hover only.
pub fn taskbar_press(state: &ShellState, app: &str) -> TaskbarPress {
    match last_focused(state, app) {
        None => TaskbarPress::Launch,
        Some(w) if w.focused && !w.minimized => TaskbarPress::Minimize { id: w.id.clone() },
        Some(w) => TaskbarPress::Focus { id: w.id.clone() },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum TaskbarMiddle {
    Close { id: String },
}

/// #EDE-254: the middle button (middle-click-action 3): the group's last
/// focused window closes; `None` without a window.
pub fn taskbar_middle(state: &ShellState, app: &str) -> Option<TaskbarMiddle> {
    last_focused(state, app).map(|w| TaskbarMiddle::Close { id: w.id.clone() })
}

/// One row of a genUI Menu's `items` (a separator, a submenu, a checkbox
/// row or an item).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MenuRow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<MenuRow>>,
}

impl MenuRow {
    pub fn item(label: impl Into<String>, value: impl Into<String>, icon: Option<&str>) -> Self {
        Self { label: Some(label.into()), value: Some(value.into()), icon: icon.map(String::from), ..Self::default() }
    }

    pub fn separator() -> Self {
        Self { kind: Some("separator".into()), ..Self::default() }
    }

    pub fn submenu(label: impl Into<String>, items: Vec<MenuRow>) -> Self {
        Self { kind: Some("submenu".into()), label: Some(label.into()), items: Some(items), ..Self::default() }
    }

    fn is_separator(&self) -> bool {
        self.kind.as_deref() == Some("separator")
    }
}

/// What [`taskbar_menu`] knows beyond the model (every key optional).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskbarMenuContext {
    #[serde(default = "one")]
    pub outputs: usize,
    #[serde(default)]
    pub windows: std::collections::BTreeMap<String, WindowFacts>,
    #[serde(default)]
    pub recent: Vec<RecentFile>,
    #[serde(default)]
    pub actions: Vec<DesktopAction>,
    #[serde(default = "yes")]
    pub entry: bool,
}

fn one() -> usize {
    1
}

fn yes() -> bool {
    true
}

impl Default for TaskbarMenuContext {
    fn default() -> Self {
        Self { outputs: 1, windows: Default::default(), recent: Vec::new(), actions: Vec::new(), entry: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WindowFacts {
    #[serde(default)]
    pub output: usize,
    #[serde(default)]
    pub maximized: bool,
    /// #EDE-255: Always on Visible Workspace.
    #[serde(default)]
    pub sticky: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentFile {
    pub name: String,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopAction {
    pub id: String,
    pub name: String,
}

/// The Recent submenu's length (menus.js).
pub const TASKBAR_RECENT_MAX: usize = 10;

/// #EDE-254: a group's right-click menu, grouped-window-list's menus.js in
/// its order (shell.json's `$comment`).
pub fn taskbar_menu(state: &ShellState, app: &str, context: &TaskbarMenuContext) -> Vec<MenuRow> {
    let count = on_active(state).into_iter().filter(|w| w.app == app).count();
    let w = last_focused(state, app);
    let facts = w.and_then(|w| context.windows.get(&w.id)).cloned().unwrap_or_default();
    let mut rows = Vec::new();
    if let Some(w) = w {
        if context.outputs >= 2 {
            for i in (0..context.outputs).filter(|i| *i != facts.output) {
                let label = if context.outputs == 2 { "Move to the other monitor".to_string() } else { format!("Move to monitor {}", i + 1) };
                rows.push(MenuRow::item(label, format!("monitor:{i}"), None));
            }
        }
        if state.workspaces.count >= 2 {
            // #EDE-255: menus.js: a sticky window offers to stay on this
            // workspace only, any other to be on all, then the moves.
            if facts.sticky {
                rows.push(MenuRow::item("Only on this workspace", "unstick", None));
            } else {
                rows.push(MenuRow::item("Visible on all workspaces", "stick", None));
                let items = (0..state.workspaces.count)
                    .map(|i| MenuRow { disabled: (i == w.workspace).then_some(true), ..MenuRow::item(format!("Workspace {}", i + 1), format!("workspace:{i}"), None) })
                    .collect();
                rows.push(MenuRow::submenu("Move to another workspace", items));
            }
        }
        rows.push(MenuRow::separator());
    }
    rows.push(MenuRow::submenu("Applet preferences", vec![MenuRow::item("About...", "about", Some("ui-help")), MenuRow::item("Configure...", "configure", Some("nav-settings"))]));
    rows.push(MenuRow::separator());
    if !context.recent.is_empty() {
        let items = context.recent.iter().take(TASKBAR_RECENT_MAX).map(|r| MenuRow::item(r.name.clone(), format!("recent:{}", r.uri), Some("ui-add"))).collect();
        rows.push(MenuRow::submenu("Recent", items));
        rows.push(MenuRow::separator());
    }
    if context.entry {
        rows.push(MenuRow::separator());
        rows.extend(context.actions.iter().map(|a| MenuRow::item(a.name.clone(), format!("action:{}", a.id), Some("nav-apps"))));
        rows.push(MenuRow::separator());
        rows.push(MenuRow { kind: Some("checkbox".into()), checked: Some(state.taskbar.pinned.iter().any(|p| p == app)), ..MenuRow::item("Pinned to the taskbar", "pin", Some("ui-pin")) });
    } else {
        rows.push(MenuRow::separator());
    }
    rows.push(MenuRow::separator());
    if let Some(w) = w {
        rows.push(if w.minimized { MenuRow::item("Restore", "restore", Some("ui-redock")) } else { MenuRow::item("Minimize", "minimize", Some("ui-minimize")) });
        rows.push(if facts.maximized { MenuRow::item("Unmaximize", "unmaximize", Some("ui-fullscreen-exit")) } else { MenuRow::item("Maximize", "maximize", Some("ui-maximize")) });
        if count > 1 {
            rows.push(MenuRow::item("Close others", "close-others", Some("ui-close")));
            rows.push(MenuRow::item("Close all", "close-all", Some("nav-sign-out")));
        } else {
            rows.push(MenuRow::item("Close", "close", Some("ui-close")));
        }
        rows.push(MenuRow::separator());
    }
    // Cinnamon's PopupMenu hides a leading, a trailing and a doubled separator.
    let mut out: Vec<MenuRow> = Vec::new();
    for r in rows {
        if r.is_separator() && out.last().is_none_or(MenuRow::is_separator) {
            continue;
        }
        out.push(r);
    }
    while out.last().is_some_and(MenuRow::is_separator) {
        out.pop();
    }
    out
}

/// Mint's scroll on a button: cycles the group's windows from the focused
/// one (down = next, up = previous, wrapping); none focused = the first
/// (down) or the last (up); no window = `None`.
pub fn taskbar_scroll(state: &ShellState, app: &str, direction: ScrollDirection) -> Option<TaskbarPress> {
    let ws: Vec<&ShellWindow> = on_active(state).into_iter().filter(|w| w.app == app).collect();
    if ws.is_empty() {
        return None;
    }
    let n = ws.len();
    let at = ws.iter().position(|w| w.focused);
    let next = match (direction, at) {
        (ScrollDirection::Down, Some(i)) => (i + 1) % n,
        (ScrollDirection::Down, None) => 0,
        (ScrollDirection::Up, Some(i)) => (i + n - 1) % n,
        (ScrollDirection::Up, None) => n - 1,
    };
    Some(TaskbarPress::Focus { id: ws[next].id.clone() })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceBadge {
    pub index: usize,
    pub active: bool,
    pub count: usize,
}

/// One badge per workspace with its window count (minimized included).
pub fn workspace_badges(state: &ShellState) -> Vec<WorkspaceBadge> {
    (0..state.workspaces.count)
        .map(|index| WorkspaceBadge { index, active: index == state.workspaces.active, count: state.windows.iter().filter(|w| w.workspace == index).count() })
        .collect()
}

/// EDE-56: the switcher's sections: the Local desktops first, then one per
/// account in first-appearance order, the current entry marked.
pub fn switcher_sections(state: &ShellState) -> Vec<SwitcherSection> {
    let current = &state.switcher.current;
    let mark = |e: &SwitcherEntry| SwitcherSectionEntry { entry: e.clone(), current: e.kind == current.kind && e.id == current.id };
    let mut sections = vec![SwitcherSection {
        kind: SwitcherSectionKind::Local,
        id: "local".into(),
        label: "Local desktops".into(),
        entries: state.switcher.entries.iter().filter(|e| e.kind == SwitcherKind::Local).map(mark).collect(),
    }];
    for entry in state.switcher.entries.iter().filter(|e| e.kind != SwitcherKind::Local) {
        let id = entry.account.clone().unwrap_or_default();
        let index = match sections.iter().position(|s| s.kind == SwitcherSectionKind::Account && s.id == id) {
            Some(i) => i,
            None => {
                let label = entry.account_name.clone().unwrap_or_else(|| if id.is_empty() { "Exponential".into() } else { id.clone() });
                sections.push(SwitcherSection { kind: SwitcherSectionKind::Account, id: id.clone(), label, entries: Vec::new() });
                sections.len() - 1
            }
        };
        sections[index].entries.push(mark(entry));
    }
    sections
}

/// The switcher's order: the active workspace's windows in mru order
/// (minimized included, Mint's default); with `alt_tab.app` only that app's.
pub fn alt_tab_order(state: &ShellState) -> Vec<String> {
    let listed = |id: &String| {
        state.windows.iter().find(|w| &w.id == id).is_some_and(|w| w.workspace == state.workspaces.active && state.alt_tab.app.as_ref().is_none_or(|app| &w.app == app))
    };
    state.mru.iter().filter(|id| listed(id)).cloned().collect()
}

/// One installed application as the start menu sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartMenuApp {
    pub id: String,
    pub name: String,
    pub comment: String,
    pub categories: Vec<String>,
    pub keywords: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartMenuEntry {
    pub id: String,
    pub name: String,
    pub comment: String,
    pub category: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartMenuCategory {
    pub id: String,
    pub label: String,
    pub count: usize,
}

/// Mint's main groups in its order; the third field = the freedesktop
/// categories that put an app there.
pub const START_MENU_GROUPS: &[(&str, &str, &[&str])] = &[
    ("all", "All applications", &[]),
    ("favourites", "Favourites", &[]),
    ("exponential", "Exponential", &[]),
    ("accessories", "Accessories", &["Utility"]),
    ("administration", "Administration", &["System", "Settings"]),
    ("education", "Education", &["Education"]),
    ("games", "Games", &["Game"]),
    ("graphics", "Graphics", &["Graphics"]),
    ("internet", "Internet", &["Network"]),
    ("office", "Office", &["Office"]),
    ("programming", "Programming", &["Development"]),
    ("science", "Science", &["Science"]),
    ("sound-video", "Sound & Video", &["AudioVideo", "Audio", "Video"]),
    ("other", "Other", &[]),
];

pub const EXPONENTIAL_APP_PREFIX: &str = "at.exponential.";

/// The groups an app belongs to (never all/favourites), in Mint's order;
/// `other` when none matches.
pub fn app_groups(app: &StartMenuApp) -> Vec<&'static str> {
    let mut out = Vec::new();
    if app.id.starts_with(EXPONENTIAL_APP_PREFIX) {
        out.push("exponential");
    }
    for (id, _, matches) in START_MENU_GROUPS {
        if matches.iter().any(|c| app.categories.iter().any(|a| a == c)) {
            out.push(*id);
        }
    }
    if out.is_empty() {
        out.push("other");
    }
    out
}

fn by_name(a: &StartMenuApp, b: &StartMenuApp) -> std::cmp::Ordering {
    a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id))
}

fn entry(app: &StartMenuApp) -> StartMenuEntry {
    StartMenuEntry { id: app.id.clone(), name: app.name.clone(), comment: app.comment.clone(), category: app_groups(app)[0].to_string() }
}

/// The rank of an app for a query every word of which matches somewhere:
/// 0 = the name starts with the query, 1 = every word starts a word of the
/// name, 2 = found in the comment or keywords; `None` = no match.
pub fn query_rank(app: &StartMenuApp, query: &str) -> Option<u8> {
    let words: Vec<&str> = query.split_whitespace().collect();
    let name = app.name.to_lowercase();
    let haystack = format!("{name} {} {}", app.comment.to_lowercase(), app.keywords.to_lowercase());
    if !words.iter().all(|w| haystack.contains(w)) {
        return None;
    }
    if name.starts_with(query) {
        return Some(0);
    }
    let name_words: Vec<&str> = name.split_whitespace().collect();
    if words.iter().all(|w| name_words.iter().any(|n| n.starts_with(w))) {
        return Some(1);
    }
    Some(2)
}

/// The menu's app list: a query (≥ 1 char after trim + lowercase) ranks
/// over every app; else the state's category: All, Favourites (in their
/// order) or one group, by name.
pub fn start_menu_apps(state: &ShellState, apps: &[StartMenuApp], favourites: &[String]) -> Vec<StartMenuEntry> {
    let query = state.start_menu.query.trim().to_lowercase();
    if !query.is_empty() {
        let mut ranked: Vec<(u8, &StartMenuApp)> = apps.iter().filter_map(|a| query_rank(a, &query).map(|r| (r, a))).collect();
        ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| by_name(a.1, b.1)));
        return ranked.into_iter().map(|(_, a)| entry(a)).collect();
    }
    let category = state.start_menu.category.as_str();
    let mut picked: Vec<&StartMenuApp> = match category {
        "all" => apps.iter().collect(),
        "favourites" => return favourites.iter().flat_map(|id| apps.iter().filter(move |a| &a.id == id)).map(entry).collect(),
        _ => apps.iter().filter(|a| app_groups(a).contains(&category)).collect(),
    };
    picked.sort_by(|a, b| by_name(a, b));
    picked.into_iter().map(entry).collect()
}

/// The category column: Mint's order, the non-empty groups only, All and
/// Favourites always.
pub fn start_menu_categories(apps: &[StartMenuApp], favourites: &[String]) -> Vec<StartMenuCategory> {
    let favourite_count = favourites.iter().filter(|id| apps.iter().any(|a| &a.id == *id)).count();
    START_MENU_GROUPS
        .iter()
        .filter_map(|(id, label, _)| {
            let count = match *id {
                "all" => apps.len(),
                "favourites" => favourite_count,
                _ => apps.iter().filter(|a| app_groups(a).contains(id)).count(),
            };
            (*id == "all" || *id == "favourites" || count > 0).then(|| StartMenuCategory { id: id.to_string(), label: label.to_string(), count })
        })
        .collect()
}

/// #EDE-65: one issue-search hit for the start menu: the engine
/// (`domain::issue_search`) ranks every connected team's issues and every
/// Local desktop's as one pool; this is a hit in its order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartMenuIssue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub status: String,
    pub source: SwitcherKind,
    pub source_id: String,
    pub source_name: String,
    pub board: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartMenuIssueRow {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub status: String,
    pub icon: String,
    pub source: SwitcherKind,
    pub source_id: String,
    pub context: String,
}

/// #EDE-65: the start menu's issue rows under its apps.
pub const START_MENU_ISSUE_LIMIT: usize = 8;

const ISSUE_ANCHORS: &[&str] = &["backlog", "in_progress", "in_review", "done", "cancelled", "duplicate"];

/// The ranked hits as rows: none without a query, else the first eight in
/// the engine's order with a status icon and `<source> · <board>`.
pub fn start_menu_issues(state: &ShellState, issues: &[StartMenuIssue]) -> Vec<StartMenuIssueRow> {
    if state.start_menu.query.trim().is_empty() {
        return Vec::new();
    }
    issues
        .iter()
        .take(START_MENU_ISSUE_LIMIT)
        .map(|i| StartMenuIssueRow {
            id: i.id.clone(),
            identifier: i.identifier.clone(),
            title: i.title.clone(),
            status: i.status.clone(),
            icon: if ISSUE_ANCHORS.contains(&i.status.as_str()) { format!("status-{}", i.status.replace('_', "-")) } else { "ui-issue".into() },
            source: i.source.clone(),
            source_id: i.source_id.clone(),
            context: if i.board.is_empty() { i.source_name.clone() } else { format!("{} · {}", i.source_name, i.board) },
        })
        .collect()
}

/// #EDE-258: one root of the IDE sidebar tree as the holder publishes it
/// (the feeds' `tree`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartMenuNode {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub icon: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub tone: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub items: Vec<StartMenuItem>,
}

/// #EDE-258: one row inside a tree node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartMenuItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    pub icon: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub depth: u8,
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub tone: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
}

/// #EDE-258: `domain::sidebar_tree::sidebar_local_rows` with each row's
/// name; `kind` = local | desktop | board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartMenuLocalRow {
    pub kind: String,
    pub id: String,
    pub desktop_id: Option<String>,
    pub depth: u8,
    pub title: String,
}

/// #EDE-258: one settings module the search can open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartMenuSetting {
    pub id: String,
    pub title: String,
    pub icon: String,
    #[serde(default)]
    pub keywords: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartMenuNavCategory {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub icon: String,
    pub image: Option<String>,
    pub count: u32,
    pub tone: Option<String>,
    pub separator: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartMenuRow {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: String,
    pub image: Option<String>,
    pub depth: u8,
    pub count: u32,
    pub tone: Option<String>,
    pub identifier: Option<String>,
    pub heading: Option<String>,
    /// `{page}`, `{app}`, `{settings}`, `{issue, source, sourceId}`,
    /// `{local, board}`, or null (a folder).
    pub target: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartMenuNav {
    pub category: String,
    pub categories: Vec<StartMenuNavCategory>,
    pub assistant: bool,
    pub rows: Vec<StartMenuRow>,
}

/// The app groups' icons in the category column (concepts).
pub fn start_menu_group_icon(group: &str) -> &'static str {
    match group {
        "all" => "layout-grid",
        "favourites" => "star",
        "exponential" => "brand-exponential",
        "accessories" => "wrench",
        "administration" => "settings",
        "education" => "graduation-cap",
        "games" => "gamepad-2",
        "graphics" => "palette",
        "internet" => "globe",
        "office" => "briefcase",
        "programming" => "code",
        "science" => "flask-conical",
        "sound-video" => "music",
        _ => "nav-apps",
    }
}

pub const START_MENU_PLACES_LIMIT: usize = 8;
pub const START_MENU_SETTINGS_LIMIT: usize = 4;
pub const START_MENU_APPS_LIMIT: usize = 8;

impl StartMenuRow {
    fn new(id: impl Into<String>, kind: impl Into<String>, title: impl Into<String>, icon: impl Into<String>) -> Self {
        StartMenuRow {
            id: id.into(),
            kind: kind.into(),
            title: title.into(),
            subtitle: None,
            icon: icon.into(),
            image: None,
            depth: 0,
            count: 0,
            tone: None,
            identifier: None,
            heading: None,
            target: Value::Null,
        }
    }
}

/// 0 = the title starts with the query, 1 = every word starts a word of the
/// title, 2 = every word is in the title (or `extra`); `None` = out.
fn title_rank(title: &str, query: &str, extra: &str) -> Option<u8> {
    let words: Vec<&str> = query.split_whitespace().collect();
    let lower = title.to_lowercase();
    if lower.starts_with(query) {
        return Some(0);
    }
    let title_words: Vec<&str> = lower.split_whitespace().collect();
    if words.iter().all(|w| title_words.iter().any(|t| t.starts_with(w))) {
        return Some(1);
    }
    let haystack = format!("{lower} {}", extra.to_lowercase());
    words.iter().all(|w| haystack.contains(w)).then_some(2)
}

/// Stable: the best rank first, ties keep the given order.
fn ranked<T>(list: Vec<T>, rank: impl Fn(&T) -> Option<u8>) -> Vec<T> {
    let mut out: Vec<(u8, T)> = list.into_iter().filter_map(|x| rank(&x).map(|r| (r, x))).collect();
    out.sort_by_key(|(r, _)| *r);
    out.into_iter().map(|(_, x)| x).collect()
}

fn item_row(item: &StartMenuItem) -> StartMenuRow {
    StartMenuRow {
        subtitle: item.subtitle.clone(),
        image: item.image.clone(),
        depth: item.depth,
        count: item.count,
        tone: item.tone.clone(),
        target: item.key.as_ref().map_or(Value::Null, |key| serde_json::json!({ "page": key })),
        ..StartMenuRow::new(&item.id, &item.kind, &item.title, &item.icon)
    }
}

fn app_row(app: &StartMenuEntry) -> StartMenuRow {
    StartMenuRow {
        subtitle: Some(app.comment.clone()),
        target: serde_json::json!({ "app": app.id }),
        ..StartMenuRow::new(format!("app:{}", app.id), "app", &app.name, "nav-apps")
    }
}

fn local_row(r: &StartMenuLocalRow, depth: u8) -> StartMenuRow {
    let desktop = r.desktop_id.clone().unwrap_or_default();
    let (id, icon, board) = if r.kind == "desktop" {
        (format!("local:{}", r.id), "computer", Value::Null)
    } else {
        (format!("local:{desktop}:{}", r.id), "square-kanban", Value::String(r.id.clone()))
    };
    StartMenuRow {
        depth,
        target: serde_json::json!({ "local": r.desktop_id, "board": board }),
        ..StartMenuRow::new(id, &r.kind, &r.title, icon)
    }
}

fn headed(mut rows: Vec<StartMenuRow>, heading: &str) -> Vec<StartMenuRow> {
    if let Some(first) = rows.first_mut() {
        first.heading = Some(heading.to_string());
    }
    rows
}

/// #EDE-258: the start menu as navigation: the sidebar tree's roots, Local,
/// the app groups; a query = Places, Settings, Apps, Issues.
pub fn start_menu_nav(
    state: &ShellState,
    tree: &[StartMenuNode],
    local: &[StartMenuLocalRow],
    apps: &[StartMenuApp],
    favourites: &[String],
    settings: &[StartMenuSetting],
    issues: &[StartMenuIssue],
) -> StartMenuNav {
    let category_of = |id: &str, kind: &str, title: &str, icon: &str| StartMenuNavCategory {
        id: id.into(),
        kind: kind.into(),
        title: title.into(),
        icon: icon.into(),
        image: None,
        count: 0,
        tone: None,
        separator: false,
        selected: false,
    };
    let mut categories: Vec<StartMenuNavCategory> = tree
        .iter()
        .map(|node| StartMenuNavCategory {
            image: node.image.clone(),
            count: node.count,
            tone: node.tone.clone(),
            ..category_of(&node.id, &node.kind, &node.title, &node.icon)
        })
        .collect();
    let local_title = "Local";
    if !local.is_empty() {
        categories.push(category_of("local", "local", local_title, "computer"));
    }
    let first_app = categories.len();
    for group in start_menu_categories(apps, favourites) {
        categories.push(category_of(&group.id, "apps", &group.label, start_menu_group_icon(&group.id)));
    }
    if let Some(first) = categories.iter_mut().find(|c| c.kind == "team" || c.kind == "local") {
        first.separator = true;
    }
    if let Some(first) = categories.get_mut(first_app) {
        first.separator = true;
    }

    let wanted = &state.start_menu.category;
    let category = if categories.iter().any(|c| &c.id == wanted) {
        wanted.clone()
    } else if let Some(team) = categories.iter().find(|c| c.kind == "team") {
        team.id.clone()
    } else if !local.is_empty() {
        "local".into()
    } else {
        tree.first().map_or_else(|| "all".into(), |node| node.id.clone())
    };
    for c in &mut categories {
        c.selected = c.id == category;
    }
    let kind = categories.iter().find(|c| c.id == category).map(|c| c.kind.clone());

    let query = state.start_menu.query.trim().to_lowercase();
    if query.is_empty() {
        let rows = match kind.as_deref() {
            Some("local") => local.iter().filter(|r| r.kind != "local").map(|r| local_row(r, r.depth.saturating_sub(1))).collect(),
            Some("apps") => {
                let mut scoped = state.clone();
                scoped.start_menu.category = category.clone();
                start_menu_apps(&scoped, apps, favourites).iter().map(app_row).collect()
            }
            Some("assistant") | None => Vec::new(),
            Some(_) => tree.iter().find(|n| n.id == category).map(|n| n.items.iter().map(item_row).collect()).unwrap_or_default(),
        };
        return StartMenuNav { category, categories, assistant: kind.as_deref() == Some("assistant"), rows };
    }

    let mut places: Vec<StartMenuRow> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut add = |mut row: StartMenuRow| {
        if seen.insert(row.id.clone()) {
            row.depth = 0;
            places.push(row);
        }
    };
    for node in tree {
        if let Some(key) = &node.key {
            add(StartMenuRow {
                image: node.image.clone(),
                count: node.count,
                tone: node.tone.clone(),
                target: serde_json::json!({ "page": key }),
                ..StartMenuRow::new(&node.id, &node.kind, &node.title, &node.icon)
            });
        }
        for item in node.items.iter().filter(|item| item.key.is_some()) {
            add(StartMenuRow { subtitle: Some(node.title.clone()), ..item_row(item) });
        }
    }
    for r in local.iter().filter(|r| r.kind != "local") {
        add(StartMenuRow { subtitle: Some(local_title.into()), ..local_row(r, 0) });
    }

    let place_rows: Vec<StartMenuRow> = ranked(places, |r| title_rank(&r.title, &query, "")).into_iter().take(START_MENU_PLACES_LIMIT).collect();
    let setting_rows: Vec<StartMenuRow> = ranked(settings.iter().collect(), |s| title_rank(&s.title, &query, &s.keywords))
        .into_iter()
        .take(START_MENU_SETTINGS_LIMIT)
        .map(|s| StartMenuRow {
            target: serde_json::json!({ "settings": s.id }),
            ..StartMenuRow::new(format!("settings:{}", s.id), "settings", &s.title, &s.icon)
        })
        .collect();
    let app_rows: Vec<StartMenuRow> = start_menu_apps(state, apps, favourites).iter().take(START_MENU_APPS_LIMIT).map(app_row).collect();
    let issue_rows: Vec<StartMenuRow> = start_menu_issues(state, issues)
        .into_iter()
        .map(|i| StartMenuRow {
            subtitle: Some(i.context.clone()),
            identifier: Some(i.identifier.clone()),
            target: serde_json::json!({ "issue": i.identifier, "source": i.source, "sourceId": i.source_id }),
            ..StartMenuRow::new(format!("issue:{}", i.id), "issue", &i.title, &i.icon)
        })
        .collect();
    let mut rows = headed(place_rows, "Places");
    rows.extend(headed(setting_rows, "Settings"));
    rows.extend(headed(app_rows, "Apps"));
    rows.extend(headed(issue_rows, "Issues"));
    StartMenuNav { category, categories, assistant: false, rows }
}

/// The fixture's case `state`: each top-level key given is complete, the
/// others come from the initial state.
pub fn hydrate(partial: &Value) -> Result<ShellState, serde_json::Error> {
    let mut base = serde_json::to_value(ShellState::default())?;
    if let (Some(base), Some(given)) = (base.as_object_mut(), partial.as_object()) {
        for (k, v) in given {
            base.insert(k.clone(), v.clone());
        }
    }
    serde_json::from_value(base)
}
