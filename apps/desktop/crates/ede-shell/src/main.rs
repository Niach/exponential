//! `exponential-shell`: eDE's shell, drawn from genUI documents on disk (#EDE-10).
//! Its own process next to `exponential-compositor`: a wlr-layer-shell client
//! (gpui's `WindowKind::LayerShell`) painting eui-gpui surfaces, so a shell
//! crash never takes the session down and the compositor never links gpui.
//!
//! Since #EDE-50 the shell is `compositor + uicore`: the pure shell model
//! (`uicore-model`: the taskbar's groups and pins, the start menu,
//! Alt-Tab), what the documents bind (`uicore-data`: the [`Feed`], the
//! desktop entries, the window list as reducer events) and the gpui host
//! (`uicore-gpui`: [`Surfaces`], one layer-shell surface per document, popup
//! documents and hosted Popover / Menu layers as `xdg_popup`s) are crates the
//! IDE will host too. This binary is what is eDE-specific: the ede-protocol
//! connection, the session's notification server and tray host ([`dbus`]),
//! the services socket, the hotkeys, the `ede_*` host functions.
//!
//! The documents come from `exponential-ui` (`$EDE_UI_DIR`, else
//! `~/.config/exponential-desktop/ui`; the supervisor `exponential-ui supervise --
//! exponential-shell` hands the directory over and owns the history): every
//! document with a layer `surface` header is a layer surface, every popup
//! document opens from its anchor component in its parent as an `xdg_popup`
//! of that surface. A change on disk redraws the surface; after each draw the
//! shell prints the document's frame line on stdout, which is how the
//! supervisor knows an edit reached the screen. The data every document may
//! bind: `/time` + `/date` + `/dateText` (local), `/windows` (the active workspace's
//! windows over ede-protocol: id, title, app, focused, minimized), `/shell`
//! (the model: taskbar buttons, workspaces, the start menu's categories
//! and apps, Alt-Tab), `/apps` + `/query` (the launcher), `/switcher`
//! (Alt+Tab with thumbnails), `/previews` (a taskbar group's windows with
//! thumbnails), `/osd`, `/notifications` (#EDE-65: with the accounts'
//! unread inbox merged in), `/needsYou`, `/tray` (#EDE-252: each item's dbusmenu as Menu `items`),
//! `/services/<domain>`, `/outputs`, `/config`, `/user`. The host functions a
//! document may call are the `ede_*` names its `surface.functions` lists
//! (#EDE-8's Tier C rule, checked again here): `ede_launch {command}`,
//! `ede_focus {window}`, `ede_close {window}`, `ede_minimize {window}`,
//! `ede_lock`, `ede_logout`, `ede_power {action}`, `ede_select {window}`,
//! `ede_alt_tab_select {window}` (#EDE-65: a hovered Alt-Tab tile),
//! `ede_notify_open {id}`, `ede_notify_close {id, toast?}`, `ede_notify_action
//! {id, key}`, `ede_notify_clear` (an inbox entry's id is `inbox:<id>`),
//! `ede_needs_you_open {id}` (#EDE-65: both hand off to the Exponential app),
//! `ede_tray {service, refresh?}`, `ede_tray_menu {service, id}`, `ede_panel_menu {value}`
//! (#EDE-252: the start button's and the clock's context menus), `ede_open_document
//! {name, page?}` (#EDE-214: Settings on `page`, #EDE-273 `~/settings/<page>`; open popups close),
//! `ede_close_document {name}`, `ede_service {domain, action,
//! args}`, `ede_output {name, mode?, scale?}`, `ede_config {…}`, `ede_settings
//! {page, key, value}` (#EDE-47: the pages `/pages` feeds), `ede_settings_nav
//! {action, module?, query?, target?}` (#EDE-273: System Settings' grid,
//! search and modules, `system_settings.rs`), and the
//! shell model's: `ede_pin {app}` / `ede_unpin {app}`,
//! `ede_taskbar {app}` (Mint's click), `ede_taskbar_scroll {app, direction}`,
//! `ede_previews {app|null}` (the hover previews), `ede_start_menu {open?,
//! query?, category?}`, `ede_chat` (#EDE-239: opens the Assistant, the
//! Exponential app's one window), `ede_workspace {index}`; #EDE-65: `ede_issue
//! {identifier, source, sourceId}` (a start-menu issue, opened in the
//! Exponential app on its team or Local desktop); #EDE-56: `ede_switcher {action:
//! pick|new|share|connect, kind?, id?}` (the Local desktops and the connected
//! accounts' teams, `/shell/switcher/sections`), `ede_connect {action:
//! start|code|open|cancel, instance?}` (Connect Exponential: `/connecting`,
//! `/accounts`; `exponential-shell connect <url>` is the scheme handler's
//! command, forwarding the browser's callback to the running shell);
//! #EDE-57: `ede_share {action: start|run|cancel, name?, account?}` (Share a
//! Local desktop as a team: `/sharing`);
//! #EXP-2024: `/spaces` (the machine's agent spaces: the taskbar's Agent
//! spaces button and list) and `/spaceView` (the ONE pinned view),
//! `ede_space {session | null, size?}` (pins a session's view, sizes it,
//! or unpins) and `ede_space_close {space}` (ends a space, not its session);
//! #EDE-266: `/cuTray` (the taskbar's computer-use status icon and its
//! popup, cu-tray.express: uicore spaces.json `tray` over the holder's
//! `sessions`) and `ede_cu {action, space?}` (select, watch, unwatch,
//! pause, resume, takeOver, handBack, stop: the compositor's `control_space`
//! and `destroy_space` on the capture loop's connection);
//! #EDE-267: `/voice` (the holder's Voice tray board: the taskbar's Voice
//! status icon and voice-tray.express) and `ede_voice {action, item?}`
//! (call, mute, end, menu: the holder's `voice-act`; chat: the Assistant).
//!
//! `exponential-shell [--windowed] [--popup] [--open <document>] [--exit-after <ms>]`
//!   --windowed    normal windows instead of layer surfaces (macOS, any desktop)
//!   --popup NAME  open a popup document right away (the goldens; `all` = every one)
//!   --open NAME   open an on-demand layer document right away (the goldens)
//!   --exit-after  quit after this many milliseconds (the headless check)
//! `EDE_SHELL_DEMO=1` feeds fixed sample data (apps, windows) instead
//! of the machine's, so a golden never depends on what is installed. Since
//! #EDE-238 the sidebar is never drawn (the start menu's tree is the
//! navigation); the taskbar holds the start, Assistant and new-issue
//! buttons, `ede_launch {new: issue}` hands off (`exponential://new-issue`);
//! `EDE_SHELL_DEMO_TRAY=1` adds the demo's inbox entries and Needs you;
//! `EDE_SHELL_DEMO_KEYBOARD=1|open` two keyboard layouts (#EDE-253: the
//! keyboard applet, `open` with its menu open); the demo's window menu
//! (`--open window-menu`) is the Terminal's.
//! `EDE_SHELL_DEMO_TASKBAR_MENU=<app>` that taskbar group's menu open (#EDE-254).
//! `EDE_SHELL_DEMO_SPACES=list|view` three sample agent spaces, their list
//! open or the first one's view pinned (#EXP-2024).

mod accounts;
mod captures;
mod engine;
mod holder_windows;
mod holder_feeds;
mod voice_tray;
mod issues;
mod assets;
mod connect;
mod session_account;
mod control;
mod dbus;
mod display_confirm;
mod inbox;
mod off_loop;
mod settings;
mod settings_pages;
mod system_settings;
mod share;
mod sealed;
mod spaces;
mod screenshot;
mod sessions_area;
mod start_menu;
mod taskbar_menu;
mod system;
mod thumbs;
mod tray_icon;
mod window_icons;
mod window_menu;

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};

use ede_ui::{
    supervise::FrameLine,
    surface::{Layer as DocLayer, SurfaceHeader},
    Change, Document, Runtime,
};
use eui_core::{host::MediaOptions, surface::Layer};
use eui_gpui::host::{ActionEvent, FunctionCallEvent, HostPlugin, InputEvent};
use gpui::App;
use serde_json::{json, Value};
use uicore_data::{apps, clock, Feed};
use uicore_gpui::{LayerShellHost, Surfaces, WindowedHost};
use uicore_model::{layout::shell_layout, reduce, ShellEvent, ShellState, StartMenuApp, TaskbarPress};

struct Options {
    windowed: bool,
    popup: Vec<String>,
    open: Vec<String>,
    /// #VAPP-273: Engine apps whose window opens at start (the goldens).
    open_app: Vec<String>,
    exit_after: Option<u64>,
}

fn parse_args() -> Options {
    let mut o = Options { windowed: false, popup: Vec::new(), open: Vec::new(), open_app: Vec::new(), exit_after: None };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            // #EDE-56: the exponential:// handler's command: hand the URL to the running shell.
            "connect" => {
                let Some(url) = args.next() else {
                    eprintln!("usage: exponential-shell connect <url>");
                    std::process::exit(2);
                };
                std::process::exit(if connect::forward_url(&url) { 0 } else { 1 });
            }
            "--windowed" => o.windowed = true,
            "--popup" => o.popup.extend(args.next()),
            "--open" => o.open.extend(args.next()),
            "--open-app" => o.open_app.extend(args.next()),
            "--exit-after" => o.exit_after = args.next().and_then(|v| v.parse().ok()),
            "-h" | "--help" => {
                println!("usage: exponential-shell [--windowed] [--popup <document>] [--open <document>] [--open-app <app id>] [--exit-after <ms>] | connect <url>");
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown argument {other:?} (try --help)");
                std::process::exit(2);
            }
        }
    }
    o
}

/// The notifications heard this session, newest first, and (#EDE-65) the
/// signed-in accounts' unread inbox entries ([`inbox`]) merged into them.
#[derive(Default)]
struct Notifications {
    list: Vec<dbus::Notification>,
    /// Ids still showing as toasts.
    toasts: Vec<u32>,
    /// #EDE-65: the inbox entries, newest first.
    inbox: Vec<inbox::InboxItem>,
    /// The inbox entries showing as toasts, until when.
    inbox_toasts: Vec<(String, Instant)>,
    /// Every inbox entry heard, so each toasts once.
    seen: HashSet<String>,
    /// Notification ids dismissed here, until the shape's echo marks them read.
    read_locally: HashSet<String>,
    /// The demo's desktop notifications (the goldens), `(received ms, row)`.
    demo: Vec<(i64, Value)>,
}

/// How long an inbox entry's toast stays (a desktop notification's default).
const INBOX_TOAST: Duration = Duration::from_secs(6);

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl Notifications {
    fn to_json(&self) -> Value {
        let desktop: Vec<(i64, Value)> = if self.demo.is_empty() { self.list.iter().map(|n| (n.received_ms, n.to_json())).collect() } else { self.demo.clone() };
        let toasts: HashSet<u64> = if self.demo.is_empty() { self.toasts.iter().map(|id| *id as u64).collect() } else { [1].into() };
        let mut inbox_toasts: HashSet<String> = self.inbox_toasts.iter().map(|(key, _)| key.clone()).collect();
        if !self.demo.is_empty() {
            // The goldens' toast stack (`--open notifications`): the newest inbox entry too.
            inbox_toasts.extend(self.inbox.first().map(|i| i.key.clone()));
        }
        inbox::merged_feed(&desktop, &toasts, &self.inbox, &inbox_toasts, now_ms())
    }

    fn any_toast(&self) -> bool {
        !self.toasts.is_empty() || !self.inbox_toasts.is_empty()
    }

    /// Expired toasts leave the stack; `true` when one did. Their rows stay
    /// in the bell with their actions, so their ids stay alive: the app hears
    /// NotificationClosed only when the row goes (#EDE-135).
    fn expire(&mut self, now: Instant) -> bool {
        let gone: Vec<u32> = self.list.iter().filter(|n| self.toasts.contains(&n.id) && n.expires_at.is_some_and(|t| now >= t)).map(|n| n.id).collect();
        self.toasts.retain(|id| !gone.contains(id));
        let before = self.inbox_toasts.len();
        self.inbox_toasts.retain(|(_, until)| now < *until);
        !gone.is_empty() || before != self.inbox_toasts.len()
    }

    /// #EDE-65: the inbox as it stands now; what is new since the shell
    /// started toasts (`since_ms`).
    fn set_inbox(&mut self, mut items: Vec<inbox::InboxItem>, since_ms: i64) {
        items.retain(|i| !i.unread_ids.iter().all(|id| self.read_locally.contains(id)));
        items.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
        let until = Instant::now() + INBOX_TOAST;
        for key in inbox::fresh_items(&items, &mut self.seen, since_ms) {
            self.inbox_toasts.insert(0, (key, until));
        }
        self.inbox_toasts.retain(|(key, _)| items.iter().any(|i| &i.key == key));
        self.inbox = items;
    }

    /// An inbox entry by its feed id.
    fn inbox_item(&self, key: &str) -> Option<inbox::InboxItem> {
        self.inbox.iter().find(|i| i.key == key).cloned()
    }

    /// The entry leaves the bell (marked read here first); the item, to mark read on its server.
    fn inbox_read(&mut self, key: &str) -> Option<inbox::InboxItem> {
        let item = self.inbox_item(key)?;
        self.read_locally.extend(item.unread_ids.iter().cloned());
        self.inbox.retain(|i| i.key != key);
        self.inbox_toasts.retain(|(k, _)| k != key);
        Some(item)
    }

    /// The ids the 100-row cut removed (their NotificationClosed is due).
    fn push(&mut self, n: dbus::Notification) -> Vec<u32> {
        self.list.retain(|x| x.id != n.id);
        self.toasts.retain(|id| *id != n.id);
        self.toasts.insert(0, n.id);
        self.list.insert(0, n);
        let cut: Vec<u32> = self.list.iter().skip(100).map(|x| x.id).collect();
        self.list.truncate(100);
        self.toasts.retain(|id| !cut.contains(id));
        cut
    }

    /// Removes it everywhere; `true` if it was there.
    fn remove(&mut self, id: u32) -> bool {
        let before = self.list.len();
        self.list.retain(|x| x.id != id);
        self.toasts.retain(|x| *x != id);
        before != self.list.len()
    }

    /// #EDE-196: a press on desktop notification `id` as the freedesktop
    /// spec has it. The app hears `ActionInvoked(id, key)`: the pressed
    /// action `key` (only one it offered), or for a press on the
    /// notification itself (`None`) its `default` action when it gave one.
    /// Then the notification closes (`NotificationClosed(id, 2)`, the
    /// person dismissed it), unless the app made it `resident`: then only
    /// its toast goes and the bell keeps it. `true` when anything changed.
    fn invoke(&mut self, dbus: Option<&dbus::Dbus>, id: u32, key: Option<&str>) -> bool {
        let Some(n) = self.list.iter().find(|n| n.id == id) else { return false };
        let action = match key {
            Some(key) => n.has_action(key).then_some(key),
            None => n.has_action(dbus::DEFAULT_ACTION).then_some(dbus::DEFAULT_ACTION),
        };
        if key.is_some() && action.is_none() {
            tracing::warn!(id, key, "a press on an action the notification does not offer");
            return false;
        }
        let resident = n.resident;
        if let (Some(d), Some(action)) = (dbus, action) {
            d.action_invoked(id, action);
        }
        if resident {
            let before = self.toasts.len();
            self.toasts.retain(|x| *x != id);
            return before != self.toasts.len();
        }
        self.remove(id);
        if let Some(d) = dbus {
            d.notification_closed(id, 2);
        }
        true
    }
}

/// What the hosted-popups hook reports: a document's hosted layers.
type HostedReport = (String, Vec<Layer>);

/// The last of each surface's hosted-popup reports, in arrival order of
/// those last ones.
/// #EDE-252: the hover previews (`previews…` Popovers) open while a Menu
/// of the same surface is: the ones to close.
fn previews_under_a_menu(layers: &[eui_core::surface::Layer]) -> Vec<String> {
    if !layers.iter().any(|l| l.hosted && l.kind == "Menu") {
        return Vec::new();
    }
    layers.iter().filter(|l| l.hosted && l.kind == "Popover" && l.owner.starts_with("previews")).map(|l| l.owner.clone()).collect()
}

fn latest_reports(reports: impl IntoIterator<Item = HostedReport>) -> Vec<HostedReport> {
    let mut out: Vec<HostedReport> = Vec::new();
    for (document, layers) in reports {
        out.retain(|(d, _)| d != &document);
        out.push((document, layers));
    }
    out
}

/// The shell's state, shared by the host plugin and the main loop.
struct Shell {
    options: Options,
    ui: Arc<Mutex<Runtime>>,
    surfaces: Surfaces,
    /// What every document binds.
    feed: Feed,
    /// The shell model (#EDE-50): the taskbar, the start menu, Alt-Tab.
    model: ShellState,
    ede: Option<ede_protocol::Client>,
    /// The installed applications, scanned at start (the launcher, the start menu).
    apps: Vec<apps::App>,
    menu_apps: Vec<StartMenuApp>,
    /// #EDE-242: what Xwayland says about classes no entry names.
    x11_hints: window_icons::X11Hints,
    /// The compositor's last window list (ede-protocol), to shape `/windows`.
    windows: Vec<ede_protocol::WindowInfo>,
    /// Live thumbnails by window id, refreshed by the capture loop while
    /// the switcher or the previews are open.
    thumbnails: HashMap<String, String>,
    /// #EDE-237: what the Exponential app's windows are (the holder's pages).
    holder_windows: holder_windows::HolderWindows,
    /// #EDE-258: what the start menu navigates (the holder's tree, recents, Assistant).
    holder_feeds: holder_feeds::HolderFeeds,
    /// #EDE-258: the machine's Local desktops with their boards (the tree's Local node).
    local_tree: Vec<uicore_model::StartMenuLocalRow>,
    /// #EXP-2024: THE capture loop's plan and asks (`captures.rs`): the
    /// windows and agent spaces it photographs, derived from what is open.
    captures: Arc<captures::Shared>,
    /// #EXP-2024: the machine's agent spaces, their titles and pictures, the pinned view.
    spaces: spaces::Spaces,
    /// The screen is locked (`session.locked` / `session.unlocked`): nothing is captured.
    locked: bool,
    /// The first person output's x, y, width, height (a fullscreen window over it covers the view).
    first_output: Option<(i32, i32, i32, i32)>,
    /// #EDE-250: a Display page change waiting for Keep.
    display_confirm: display_confirm::Confirm,
    /// #EDE-225: every person output's x, y, width, height: the switcher
    /// opens on the one with the focused window.
    person_outputs: Vec<(i32, i32, i32, i32)>,
    /// #EXP-2024: when each layer document last opened (a counter): a later
    /// top layer lies above an earlier one.
    layers_opened: HashMap<String, u64>,
    /// The OSD closes when this passes.
    osd_until: Option<Instant>,
    /// #EDE-253: the window menu open now (or last), from `window.menu`.
    window_menu: Option<ede_protocol::WindowMenu>,
    /// #EDE-89: when each popup document last opened (epoch ms, the clock of
    /// the compositor's event stamps).
    popups_opened: HashMap<String, i64>,
    /// #EDE-168: the height each open taskbar popup was drawn to fit in.
    popup_rooms: HashMap<String, (u32, u32)>,
    /// Bumped by every fresh Alt+Tab: a delay timer of an earlier one shows nothing.
    switcher_generation: u64,
    notifications: Notifications,
    /// #EDE-65: Needs you across the accounts' teams (the tray button, needs-you.json).
    needs_you: Vec<inbox::NeedsYouItem>,
    /// #EDE-141: the tray's read of the accounts' stores runs off the main
    /// loop, one at a time; a refresh asked for meanwhile runs after it.
    tray_gate: off_loop::Gate,
    /// When the shell started (epoch ms): only inbox entries since then toast.
    started_ms: i64,
    /// `EDE_SHELL_DEMO`: the sample data, nothing leaves the shell.
    demo: bool,
    dbus: Option<dbus::Dbus>,
    tray: Vec<dbus::TrayItem>,
    /// #EDE-56: the Local desktops and the connected accounts' teams.
    workspaces: accounts::Workspaces,
    /// #EDE-56: Connect Exponential's state.
    connect: connect::Connect,
    /// #EDE-226: the installs waiting for approval, as the holder tells it.
    installs: session_account::Installs,
    /// #EDE-226: the account being signed out (Settings' button waits).
    signing_out: Option<String>,
    /// #EDE-226: a sign-out the shell did itself (the account's id).
    signed_out_tx: mpsc::Sender<String>,
    /// #EDE-57: Share as a team's state.
    share: share::Share,
    /// The Local desktop opened for a share (its store, blobs, identity).
    local: Option<Arc<elocal::LocalDesktop>>,
    /// A team to make current once the account's teams shape carries it
    /// (#EDE-223: or the signed-in account's first team).
    pending_pick: accounts::Pending,
    /// #VAPP-273: the apps on the Engine (their windows and widgets).
    engine: Option<engine::SharedEngine>,
    /// #EDE-141: the desktop agent's seats on the running apps: replaced at
    /// every Engine boot, empty after a shutdown.
    engine_seats: engine::exposes::Seats,
    /// #EDE-64: the apps launched in the last seconds.
    launches: system::LaunchGuard,
    /// #EDE-224: the Exponential app's next window after a hand-off.
    focus_next: system::FocusNext,
    /// The installed packages (the factory and the person's directories).
    engine_apps: Vec<engine::EngineApp>,
    /// The launcher's entries, for the `launch` capability.
    launchable: Arc<Vec<apps::App>>,
    /// #VAPP-273: the connected team's synced store, when the current entry is a team.
    team_stores: Option<Arc<engine::synced::TeamStores>>,
    /// #EDE-65: the start menu's issue pool (built when the menu opens),
    /// the hits for the current query and the debounce's generation.
    issue_index: Arc<Vec<issues::Indexed>>,
    issue_hits: Vec<uicore_model::StartMenuIssue>,
    issue_search: u64,
    /// #EDE-92: the search's selected result (Up, Down; Enter runs it).
    menu_selection: start_menu::Selection,
    /// #EDE-182: the Date & Time changes timedated is on (a prompt may be
    /// open): the same change again (the zone picker's close re-fires the
    /// zone it just picked) waits for that one instead of asking twice.
    time_changes: Vec<settings_pages::TimeChange>,
    /// #EDE-273: System Settings' place (the grid or a module, the search).
    settings_nav: system_settings::Nav,
    /// Watches while the shell lives (dropping it ends the hot reload).
    _watcher: ede_ui::Watcher,
}

type SharedShell = Rc<RefCell<Shell>>;

/// Actions from the documents: a press on a popup's anchor opens that
/// popup document; the `ede_*` host functions go to the compositor, the
/// system and the shell model, each only from a document whose header
/// allows it; the hosted popups of the document reach the main loop.
#[derive(Clone)]
struct Host {
    shell: SharedShell,
    document: String,
    /// The hosted-popups hook fires inside a view update, while the shell
    /// may be borrowed: it reports over its own channel, never through the shell.
    hosted_tx: mpsc::Sender<HostedReport>,
}

/// Does `document`'s header list `function`? (Tier C: a document grants no
/// privileges; `exponential-ui` refuses such a document already, this is the
/// shell's own check.)
fn allowed(shell: &Shell, document: &str, function: &str) -> bool {
    let ui = shell.ui.lock().unwrap_or_else(|p| p.into_inner());
    ui.document(document).is_some_and(|d| d.surface.functions().iter().any(|f| f == function))
}

fn with_ede<R>(shell: &SharedShell, f: impl FnOnce(&mut ede_protocol::Client) -> Result<R, ede_protocol::Error>) -> Option<R> {
    let mut s = shell.borrow_mut();
    let Some(ede) = s.ede.as_mut() else {
        tracing::warn!("no ede-protocol connection");
        return None;
    };
    match f(ede) {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::warn!("ede-protocol: {e}");
            None
        }
    }
}

fn window_arg(args: &Value) -> Option<ede_protocol::WindowId> {
    args.get("window").and_then(Value::as_str).and_then(|s| ede_protocol::WindowId::new(s).ok())
}

/// Mint's click on the focused window's button: the compositor's minimize
/// (#EDE-48); a `focus` restores it.
/// A launch the person made (the menu, the taskbar, a hotkey; #EDE-76):
/// the compositor's one-shot launch token goes into the app's environment,
/// so its window takes the keyboard. Anything else the shell starts (an
/// Engine app's `launch`, its own tools) opens without it.
fn person_launch(shell: &SharedShell, command: &str) {
    let token = with_ede(shell, |ede| ede.launch_token());
    system::launch_with(command, token.as_deref());
}

fn minimize(ede: &mut ede_protocol::Client, id: ede_protocol::WindowId) -> Result<(), ede_protocol::Error> {
    ede.minimize(id)
}

fn document(shell: &Shell, name: &str) -> Option<Document> {
    let mut doc = shell.ui.lock().unwrap_or_else(|p| p.into_inner()).document(name).cloned()?;
    drawn(&mut doc);
    Some(doc)
}

/// A document as eDE draws it: the shared sidebar without the IDE's parts
/// (#EDE-65).
fn drawn(doc: &mut Document) {
    if doc.name == "sidebar" {
        uicore_gpui::inline::without(doc, uicore_gpui::inline::EDE_SIDEBAR_PRUNED);
    }
}

impl HostPlugin for Host {
    fn icon(&self, name: &str) -> Option<gpui::SharedString> {
        assets::icon_path(name)
    }

    /// #EDE-65: the glyphs as the IDE paints them (an agent's mark, its
    /// count badge or working dot).
    fn icon_element(&self, name: &str, size: f32, color: gpui::Hsla, cx: &App) -> Option<gpui::AnyElement> {
        uicore_gpui::glyphs::paint(name, size, color, &assets::icon_path, cx)
    }

    fn on_action(&self, event: &ActionEvent, cx: &mut App) {
        // #EDE-75: fired by a view the shell updates while it is borrowed: after that borrow.
        if self.shell.try_borrow_mut().is_err() {
            let (host, event) = (self.clone(), event.clone());
            cx.defer(move |cx| host.on_action(&event, cx));
            return;
        }
        tracing::info!(surface = %event.surface_id, component = %event.component_id, name = %event.name, context = %event.context, "action");
        // A Menu's pick: its function with the picked value (window_menu::MENU_PICKS).
        if let Some(args) = window_menu::menu_pick_args(&event.name, &event.context) {
            let call = FunctionCallEvent { surface_id: event.surface_id.clone(), component_id: event.component_id.clone(), name: event.name.clone(), args };
            self.on_function_call(&call, cx);
            return;
        }
        // The view is mid-update while it dispatches; a popup reads its
        // painted bounds, so that waits for the update to end.
        // #EDE-226: the sign-out dialog's answer is an event (a function
        // call there would replace the dialog's own close).
        if event.name == "ede_account" && event.context["action"] == "sign-out" && allowed(&self.shell.borrow(), &event.surface_id, "ede_account") {
            let shell = self.shell.clone();
            cx.defer(move |cx| account_sign_out(&shell, cx));
            return;
        }
        // #EDE-247: a tray item's middle or right button or wheel step
        // (`auxPress {button}`, `scroll {direction}`): an event, its payload
        // reaches the context (never a function's args).
        if event.name == "ede_tray_input" && allowed(&self.shell.borrow(), &event.surface_id, "ede_tray_input") {
            let input = dbus::TrayInput::from_event(event.context["button"].as_str(), event.context["direction"].as_str());
            if let (Some(service), Some(input)) = (event.context["service"].as_str(), input) {
                if let Some(d) = self.shell.borrow().dbus.as_ref() {
                    // On the tray thread (#EDE-135).
                    d.tray_input(service, input);
                }
            }
            return;
        }
        // #EDE-254: a group button's middle button (Box auxPress {button}):
        // grouped-window-list's default closes the last focused window.
        if event.name == "ede_taskbar_input" && allowed(&self.shell.borrow(), &event.surface_id, "ede_taskbar_input") {
            if let (Some(app), Some("middle")) = (event.context["app"].as_str(), event.context["button"].as_str()) {
                let close = uicore_model::taskbar_middle(&self.shell.borrow().model, app);
                if let Some(uicore_model::TaskbarMiddle::Close { id }) = close {
                    if let Ok(id) = ede_protocol::WindowId::new(&id) {
                        with_ede(&self.shell, |ede| ede.close_window(id));
                    }
                }
            }
            return;
        }
        // #EDE-75: the hover previews' change is an event: its {open} reaches
        // a context, never a function's args, and a close ends the thumbnails.
        if event.name == "ede_previews" && allowed(&self.shell.borrow(), &event.surface_id, "ede_previews") {
            let shell = self.shell.clone();
            let app = event.context["app"].as_str().map(String::from);
            let open = event.context["open"].as_bool().unwrap_or(app.is_some());
            cx.defer(move |cx| previews_changed(&shell, app, open, cx));
            return;
        }
        let popups = popups_of(&self.shell.borrow(), &event.surface_id, &event.component_id);
        if !popups.is_empty() {
            let shell = self.shell.clone();
            cx.defer(move |cx| {
                for name in &popups {
                    toggle_popup(&shell, name, cx);
                }
            });
        }
    }

    fn on_function_call(&self, event: &FunctionCallEvent, cx: &mut App) {
        // #EDE-75: as `on_action`.
        if self.shell.try_borrow_mut().is_err() {
            let (host, event) = (self.clone(), event.clone());
            cx.defer(move |cx| host.on_function_call(&event, cx));
            return;
        }
        let (name, args) = (event.name.as_str(), &event.args);
        if !allowed(&self.shell.borrow(), &event.surface_id, name) {
            tracing::warn!(document = %event.surface_id, function = name, "refused: not in the document's surface.functions");
            return;
        }
        tracing::info!(document = %event.surface_id, function = name, %args, "host function");
        let shell = self.shell.clone();
        match name {
            "ede_launch" => {
                if args.get("new").and_then(Value::as_str) == Some("issue") {
                    new_issue(&self.shell);
                } else if let Some(command) = args.get("command").and_then(Value::as_str) {
                    person_launch(&self.shell, command);
                } else if let Some(app) = args.get("app").and_then(Value::as_str) {
                    let engine_app = self.shell.borrow().engine.as_ref().is_some_and(|e| e.borrow().has(app));
                    if engine_app {
                        // #VAPP-273: an app on the Engine: its window, in this process.
                        let (s, app) = (self.shell.clone(), app.to_string());
                        cx.defer(move |cx| {
                            let engine = s.borrow().engine.clone();
                            if let Some(engine) = engine {
                                engine::open_window(&engine, &app, cx);
                            }
                        });
                    } else if app.trim_end_matches(".desktop") == system_settings::APP_ID {
                        // #EDE-273: System Settings is the shell's own window.
                        let s = self.shell.clone();
                        cx.defer(move |cx| settings_show(&s, cx));
                    } else if app.trim_end_matches(".desktop") == HISTORY_APP {
                        // The shell's own Version history (#EDE-53): a document, not a process.
                        let s = self.shell.clone();
                        cx.defer(move |cx| {
                            history_refresh(&s);
                            open_on_demand(&s, "history", cx);
                            refresh(&s, cx);
                        });
                    } else {
                        let (s, app) = (self.shell.clone(), app.to_string());
                        cx.defer(move |_| launch_app(&s, &app));
                    }
                }
                cx.defer(move |cx| {
                    close(&shell, "menu", cx);
                    close(&shell, "start-menu", cx);
                    dispatch(&shell, ShellEvent::StartMenuClose, cx);
                });
            }
            "ede_focus" => {
                if let Some(id) = window_arg(args) {
                    with_ede(&self.shell, |ede| ede.focus(id));
                }
                if event.surface_id == "expo" || event.surface_id == "scale" {
                    let doc = event.surface_id.clone();
                    cx.defer(move |cx| overlay_close(&shell, &doc, cx));
                }
            }
            "ede_history" => {
                match args.get("action").and_then(Value::as_str) {
                    Some("undo") => system::launch("exponential-ui undo"),
                    Some("reset") => system::launch("exponential-ui reset"),
                    other => tracing::warn!(action = ?other, "unknown history action"),
                }
            }
            // #EDE-245: the corner bar, Muffin's toggle_desktop.
            "ede_show_desktop" => {
                with_ede(&self.shell, |ede| ede.toggle_desktop());
            }
            "ede_close" => {
                if let Some(id) = window_arg(args) {
                    with_ede(&self.shell, |ede| ede.close_window(id));
                }
            }
            "ede_minimize" => {
                if let Some(id) = window_arg(args) {
                    with_ede(&self.shell, |ede| minimize(ede, id));
                }
            }
            "ede_lock" => {
                with_ede(&self.shell, |ede| ede.lock());
                cx.defer(move |cx| {
                    close(&shell, "menu", cx);
                    close(&shell, "start-menu", cx);
                });
            }
            "ede_logout" => {
                with_ede(&self.shell, |ede| ede.logout());
            }
            "ede_power" => {
                if let Some(action) = args.get("action").and_then(Value::as_str).map(String::from) {
                    power(&shell, action, cx);
                }
            }
            "ede_select" => {
                if let Some(id) = window_arg(args) {
                    cx.defer(move |cx| switcher_commit(&shell, Some(id), cx));
                }
            }
            // #EDE-65: the pointer entered a tile (its Box `on.hover`): Mint selects it.
            "ede_alt_tab_select" => {
                if let Some(id) = args.get("window").and_then(Value::as_str).map(String::from) {
                    cx.defer(move |cx| switcher_hover(&shell, &id, cx));
                }
            }
            // #EDE-65: a row's press; an inbox entry hands off to the Exponential app.
            // #EDE-196: a toast's own press too: a desktop notification's default action.
            "ede_notify_open" => {
                if let Some(key) = args.get("id").and_then(Value::as_str).map(String::from) {
                    cx.defer(move |cx| inbox_open(&shell, &key, cx));
                } else if let Some(id) = args.get("id").and_then(Value::as_u64) {
                    cx.defer(move |cx| {
                        notification_invoke(&shell, id as u32, None, cx);
                        close(&shell, "notification-center", cx);
                    });
                }
            }
            "ede_notify_close" => {
                if let Some(key) = args.get("id").and_then(Value::as_str).map(String::from) {
                    // A toast's close only hides it; the bell's marks it read.
                    let toast = args.get("toast").and_then(Value::as_bool).unwrap_or(false);
                    cx.defer(move |cx| if toast { inbox_toast_hide(&shell, &key, cx) } else { inbox_mark_read(&shell, &key, cx) });
                } else if let Some(id) = args.get("id").and_then(Value::as_u64) {
                    cx.defer(move |cx| notification_close(&shell, id as u32, 2, cx));
                }
            }
            "ede_notify_action" => {
                let inbox_key = args.get("id").and_then(Value::as_str).map(String::from);
                let (id, key) = (args.get("id").and_then(Value::as_u64), args.get("key").and_then(Value::as_str).map(String::from));
                if let (Some(inbox_key), Some(key)) = (inbox_key, key.clone()) {
                    cx.defer(move |cx| if key == "open" { inbox_open(&shell, &inbox_key, cx) } else { inbox_mark_read(&shell, &inbox_key, cx) });
                } else if let (Some(id), Some(key)) = (id, key) {
                    cx.defer(move |cx| notification_invoke(&shell, id as u32, Some(&key), cx));
                } else {
                    // #EDE-196: a press that names no notification reaches no app (it was silent).
                    tracing::warn!(%args, "a notification action without its notification id or key");
                }
            }
            "ede_notify_clear" => cx.defer(move |cx| notifications_clear(&shell, cx)),
            // #EDE-239: the session area's applet menu and its panel's presses (the holder does them over the bus).
            "ede_sessions" => {
                if let Some(press) = sessions_area::press(args) {
                    cx.defer(move |cx| {
                        let token = matches!(press, sessions_area::Press::Menu(_) | sessions_area::Press::Open(_)).then(|| start_menu_token(&shell)).flatten();
                        if token.is_some() {
                            close(&shell, "sessions-panel", cx);
                        }
                        sessions_area::send(press, token);
                    });
                }
            }
            "ede_needs_you_open" => {
                if let Some(key) = args.get("id").and_then(Value::as_str).map(String::from) {
                    cx.defer(move |cx| needs_you_open(&shell, &key, cx));
                }
            }
            "ede_tray" => {
                // #EDE-252: `refresh` = the pointer is on an item with a menu:
                // its rows are read again (dbusmenu's AboutToShow) before a
                // press opens them; a press on an item without one activates it.
                if args.get("refresh").and_then(Value::as_bool) == Some(true) {
                    tray_refresh(&shell);
                } else if let Some(service) = args.get("service").and_then(Value::as_str).map(String::from) {
                    cx.defer(move |_| tray_press(&shell, &service));
                }
            }
            "ede_panel_menu" => {
                if let Some(value) = args.get("value").and_then(Value::as_str).map(String::from) {
                    cx.defer(move |cx| panel_menu(&shell, &value, cx));
                }
            }
            // #EDE-253: a pick of the window menu (or its close, `dismiss`).
            "ede_window_menu" => {
                if let Some(value) = args.get("value").and_then(Value::as_str).map(String::from) {
                    cx.defer(move |cx| window_menu_pick(&shell, &value, cx));
                }
            }
            // #EDE-253: the keyboard applet's menu: a layout's index, or `settings`.
            "ede_keyboard_layout" => {
                if let Some(value) = args.get("value").and_then(Value::as_str).map(String::from) {
                    cx.defer(move |cx| keyboard_pick(&shell, &value, cx));
                }
            }
            "ede_open_document" | "ede_close_document" => {
                if let Some(doc) = args.get("name").and_then(Value::as_str).map(String::from) {
                    let open_it = name == "ede_open_document";
                    // #EDE-214: a tray popup's settings link names its page.
                    let page = args.get("page").and_then(Value::as_str).filter(|p| !p.is_empty()).map(String::from);
                    cx.defer(move |cx| {
                        if open_it {
                            // #EDE-214: the popup the link sat in goes, as the menus do.
                            // The borrow ends before the closes (a for loop's head lives through it).
                            let popups = open_popups(&shell.borrow());
                            for popup in popups {
                                close(&shell, &popup, cx);
                            }
                            close(&shell, "menu", cx);
                            close(&shell, "start-menu", cx);
                            dispatch(&shell, ShellEvent::StartMenuClose, cx);
                            match page.as_deref() {
                                Some(page) if doc == "settings" => settings_open_page(&shell, page, cx),
                                None if doc == "settings" => settings_show(&shell, cx),
                                _ => {
                                    open_on_demand(&shell, &doc, cx);
                                    refresh(&shell, cx);
                                }
                            }
                        } else {
                            close(&shell, &doc, cx);
                        }
                    });
                }
            }
            // #EDE-273: System Settings' navigation (a tile, back, the search, Enter).
            "ede_settings_nav" => {
                if let Some(event) = system_settings::event_of(args) {
                    cx.defer(move |cx| settings_step(&shell, event, false, cx));
                }
            }
            "ede_service" => {
                let (domain, action) = (args.get("domain").and_then(Value::as_str).map(String::from), args.get("action").and_then(Value::as_str).map(String::from));
                if let (Some(domain), Some(action)) = (domain, action) {
                    let call_args = args.get("args").cloned().unwrap_or_else(|| json!({}));
                    service_call(&shell, domain, action, call_args, cx);
                }
            }
            "ede_output" => {
                // #EDE-250: a change applies at once and is written only once
                // kept; Revert, or no answer within 15 s, goes back.
                match args.get("action").and_then(Value::as_str) {
                    Some("keep") => {
                        let kept = self.shell.borrow_mut().display_confirm.keep();
                        if let Some((name, setting)) = kept {
                            persist_display(&name, &setting);
                        }
                    }
                    Some("revert") => {
                        let back = self.shell.borrow_mut().display_confirm.revert();
                        if let Some((name, setting)) = back {
                            with_ede(&self.shell, |ede| ede.set_output(&name, setting.mode.as_deref(), setting.scale, None));
                        }
                    }
                    _ => {
                        if let Some(output) = args.get("name").and_then(Value::as_str).map(String::from) {
                            let mode = args.get("mode").and_then(Value::as_str).map(String::from);
                            let scale = args.get("scale").and_then(|s| s.as_f64().or_else(|| s.as_str().and_then(|t| t.parse().ok())));
                            let current = current_output_setting(&self.shell.borrow(), &output);
                            // A Select fires `change` as it opens (the value it has):
                            // no change, no question.
                            let same_mode = mode.is_none() || mode == current.mode;
                            let same_scale = scale.is_none_or(|s| current.scale.is_some_and(|c| (c - s).abs() < 0.001));
                            if !(same_mode && same_scale) {
                                let step = self.shell.borrow_mut().display_confirm.change(&output, current, display_confirm::OutputSetting { mode, scale });
                                let display_confirm::Step::Apply { name, after, persist } = step;
                                if let Some((other, setting)) = persist {
                                    persist_display(&other, &setting);
                                }
                                with_ede(&self.shell, |ede| ede.set_output(&name, after.mode.as_deref(), after.scale, None));
                                let generation = self.shell.borrow().display_confirm.generation();
                                display_countdown(&shell, generation, cx);
                            }
                        }
                    }
                }
                cx.defer(move |cx| {
                    outputs_refresh_relayout(&shell, cx);
                    refresh(&shell, cx);
                });
            }
            "ede_config" => {
                match settings::persist_config(args) {
                    Ok(()) => {
                        with_ede(&self.shell, |ede| ede.call(ede_protocol::Request::ReloadConfig));
                        if args.get("theme").is_some() {
                            // #EDE-19: apply or remove, as the switch says.
                            system::launch("exponential-theme sync");
                        }
                    }
                    Err(e) => tracing::warn!("compositor.toml: {e}"),
                }
                cx.defer(move |cx| {
                    shell.borrow_mut().feed.set("/config", settings::config_json());
                    // #EDE-250: the scale steps follow "Fractional scaling".
                    outputs_refresh(&shell);
                    refresh(&shell, cx);
                });
            }
            "ede_tray_menu" => {
                // #EDE-252: a Menu row's value is its dbusmenu id, as a string.
                let id = args.get("id").and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())));
                if let (Some(service), Some(id)) = (args.get("service").and_then(Value::as_str), id) {
                    if let Some(d) = self.shell.borrow().dbus.as_ref() {
                        // On the tray thread (#EDE-135).
                        d.tray_menu_event(service, id as i32);
                    }
                }
            }
            // The shell model (#EDE-50).
            // The IDE's rail toggle in the header it shares (#EXP-1725); eDE's
            // sidebar is the rail for good (#EDE-65) and prunes that header.
            "ede_rail" => {}
            "ede_pin" | "ede_unpin" => {
                if let Some(app) = args.get("app").and_then(Value::as_str).map(String::from) {
                    let pin = name == "ede_pin";
                    cx.defer(move |cx| {
                        dispatch(&shell, if pin { ShellEvent::TaskbarPin { app } } else { ShellEvent::TaskbarUnpin { app } }, cx);
                        pins_persist(&shell);
                    });
                }
            }
            "ede_taskbar" => {
                if let Some(app) = args.get("app").and_then(Value::as_str).map(String::from) {
                    cx.defer(move |cx| taskbar_press(&shell, &app, cx));
                }
            }
            "ede_taskbar_scroll" => {
                let (app, direction) = (args.get("app").and_then(Value::as_str).map(String::from), args.get("direction").and_then(Value::as_str).map(String::from));
                if let (Some(app), Some(direction)) = (app, direction) {
                    let direction = if direction == "up" { uicore_model::ScrollDirection::Up } else { uicore_model::ScrollDirection::Down };
                    if let Some(TaskbarPress::Focus { id }) = uicore_model::taskbar_scroll(&self.shell.borrow().model, &app, direction) {
                        if let Ok(id) = ede_protocol::WindowId::new(&id) {
                            with_ede(&self.shell, |ede| ede.focus(id));
                        }
                    }
                }
            }
            // #EXP-2024: a row of the Agent spaces list pins its session's view
            // (null unpins; `size` = "large" | "default", the view's size button).
            "ede_space" => {
                let session = args.get("session").and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from);
                let large = args.get("size").and_then(Value::as_str).map(|s| s == "large");
                cx.defer(move |cx| {
                    let picked = session.is_some() && large.is_none();
                    shell.borrow_mut().spaces.pin(session.as_deref(), large);
                    // A row's press: the list makes way for the view it pinned
                    // (as the switcher's pick closes its menu, #EDE-65).
                    if picked {
                        let hosted: Vec<_> = shell.borrow().surfaces.hosted_layers().into_iter().filter(|(_, owner)| owner == spaces::LIST).collect();
                        uicore_gpui::dismiss_hosted(hosted, cx);
                    }
                    spaces_changed(&shell, cx);
                });
            }
            // #EDE-267: the Voice tray's presses: to the holder, Open chat = the Assistant's window.
            "ede_voice" => match voice_tray::press(args) {
                Some(voice_tray::Press::Chat) => cx.defer(move |cx| {
                    close(&shell, voice_tray::POPUP, cx);
                    assistant_open(&shell, cx);
                }),
                Some(voice_tray::Press::Act { action, item }) => voice_tray::act(action, item),
                None => {}
            },
            // #EDE-266: the computer-use tray's presses (spaces.json `tray`).
            "ede_cu" => {
                if let Some((press, space)) = spaces::tray_press(args) {
                    cx.defer(move |cx| cu_press(&shell, press, space, cx));
                }
            }
            // #EXP-2024: a row's Close ends that space (never its session), on the capture loop's connection.
            "ede_space_close" => {
                if let Some(space) = args.get("space").and_then(Value::as_str) {
                    self.shell.borrow().captures.ask(captures::Ask::Close(space.to_string()));
                }
            }
            "ede_previews" => {
                // A hover Popover's change as a function (a document from before
                // #EDE-75, whose `open` never arrives): the event in `on_action` is the factory's.
                let app = args.get("app").and_then(Value::as_str).map(String::from);
                let open = args.get("open").and_then(Value::as_bool).unwrap_or(app.is_some());
                cx.defer(move |cx| previews_changed(&shell, app, open, cx));
            }
            // #EDE-47: the settings pages the shell feeds itself (/pages); a compositor.toml key reloads it.
            "ede_settings" => match settings_pages::handle(args) {
                Ok(settings_pages::Handled::Applied { reload }) => {
                    if reload {
                        with_ede(&self.shell, |ede| ede.call(ede_protocol::Request::ReloadConfig));
                    }
                    cx.defer(move |cx| {
                        shell.borrow_mut().feed.set("/pages", settings_pages::pages_json());
                        shell.borrow_mut().feed.set("/config", settings::config_json());
                        refresh(&shell, cx);
                    });
                }
                Ok(settings_pages::Handled::Unchanged) => {}
                Ok(settings_pages::Handled::Asks(change)) => cx.defer(move |cx| settings_time(&shell, change, cx)),
                Err(e) => tracing::warn!("ede_settings: {e}"),
            },
            "ede_switcher" => {
                let args = args.clone();
                cx.defer(move |cx| match args.get("action").and_then(Value::as_str) {
                    Some("pick") => {
                        let kind: Option<uicore_model::SwitcherKind> = args.get("kind").cloned().and_then(|k| serde_json::from_value(k).ok());
                        if let (Some(entry_kind), Some(id)) = (kind, args.get("id").and_then(Value::as_str)) {
                            // #EDE-65: the menu goes with the pick (it stayed open, the pick looked like nothing).
                            let hosted = shell.borrow().surfaces.hosted_layers();
                            uicore_gpui::dismiss_hosted(hosted, cx);
                            switcher_picked(&shell, entry_kind, id, cx);
                        }
                    }
                    Some("new") => {
                        let created = shell.borrow_mut().workspaces.desktops.create(&next_desktop_name(&shell.borrow()));
                        match created {
                            Ok(entry) => {
                                switcher_refresh_entries(&shell, cx);
                                switcher_picked(&shell, uicore_model::SwitcherKind::Local, &entry.id, cx);
                            }
                            Err(e) => tracing::warn!("new local desktop: {e}"),
                        }
                    }
                    // #EDE-226: one account per login: signed in, Connect is
                    // Settings' Account page.
                    Some("connect") if shell.borrow().workspaces.session_account().is_some() => settings_open_page(&shell, "account", cx),
                    Some("connect") => {
                        close(&shell, "start-menu", cx);
                        close(&shell, "settings", cx);
                        open_on_demand(&shell, "connect", cx);
                        refresh(&shell, cx);
                    }
                    Some("share") => {
                        settings_open_page(&shell, "this-device", cx);
                        share_start(&shell, cx);
                    }
                    Some(other) => tracing::warn!(action = other, "unknown switcher action"),
                    None => {}
                });
            }
            // #EDE-57: Share a Local desktop as a team.
            "ede_share" => {
                let args = args.clone();
                cx.defer(move |cx| {
                    match args.get("action").and_then(Value::as_str) {
                        Some("start") => share_start(&shell, cx),
                        Some("run") => share_run(&shell, args.get("name").and_then(Value::as_str).unwrap_or(""), args.get("account").and_then(Value::as_str).unwrap_or("")),
                        Some("cancel") => shell.borrow_mut().share.cancel(),
                        other => tracing::warn!(?other, "unknown share action"),
                    }
                    sharing_changed(&shell, cx);
                });
            }
            // #EDE-226: Approve / Remove on an install waiting in Needs you.
            "ede_install" => {
                let arg = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or("").to_string();
                let (action, id, code) = (arg("action"), arg("id"), arg("code"));
                cx.defer(move |cx| install_press(&shell, &action, id, code, cx));
            }
            // #EDE-56: Connect Exponential.
            "ede_connect" => {
                let args = args.clone();
                cx.defer(move |cx| {
                    let instance = args.get("instance").and_then(Value::as_str).unwrap_or("").to_string();
                    // #EDE-226: exactly one account: a second sign-in never starts.
                    let starts = matches!(args.get("action").and_then(Value::as_str), Some("start" | "code"));
                    if starts && shell.borrow().workspaces.session_account().is_some() {
                        tracing::info!("an account is signed in: no second one");
                        return;
                    }
                    {
                        let mut s = shell.borrow_mut();
                        match args.get("action").and_then(Value::as_str) {
                            Some("start") => s.connect.start(&instance),
                            Some("code") => {
                                let at = if instance.is_empty() { s.connect.instance.clone() } else { instance.clone() };
                                s.connect.start_code(&at)
                            }
                            Some("open") => s.connect.open_page(),
                            Some("cancel") => s.connect.cancel(),
                            other => tracing::warn!(?other, "unknown connect action"),
                        }
                        let feed = s.connect.feed();
                        s.feed.set("/connecting", feed);
                    }
                    // #EDE-226: the browser is what the person looks at now;
                    // Connect comes back with the result (or when opened again).
                    if starts && shell.borrow().connect.in_browser() {
                        close(&shell, "connect", cx);
                    }
                    refresh(&shell, cx);
                });
            }
            "ede_taskbar_menu" => {
                let (app, value) = (args.get("app").and_then(Value::as_str).map(String::from), args.get("value").and_then(Value::as_str).map(String::from));
                if let (Some(app), Some(value)) = (app, value) {
                    cx.defer(move |cx| match value.as_str() {
                        // #EDE-252: the menu's "Pinned to the taskbar" checkbox toggles.
                        "pin" if shell.borrow().model.taskbar.pinned.contains(&app) => {
                            dispatch(&shell, ShellEvent::TaskbarUnpin { app }, cx);
                            pins_persist(&shell);
                        }
                        "pin" => {
                            dispatch(&shell, ShellEvent::TaskbarPin { app }, cx);
                            pins_persist(&shell);
                        }
                        "unpin" => {
                            dispatch(&shell, ShellEvent::TaskbarUnpin { app }, cx);
                            pins_persist(&shell);
                        }
                        other => taskbar_menu_pick(&shell, &app, other, cx),
                    });
                }
            }
            "ede_start_menu" => {
                let args = args.clone();
                // #EDE-92: Enter in the search box opens the selected row as a press on it does
                // (after the box's text, which is deferred just before).
                if args.get("activate").and_then(Value::as_bool) == Some(true) {
                    let (host, event) = (self.clone(), event.clone());
                    cx.defer(move |cx| {
                        if let Some(target) = start_menu_activate(&host.shell, cx) {
                            host.start_menu_open(target, &event, cx);
                        }
                    });
                    return;
                }
                // #EDE-258: a press on a row; the Assistant's composer.
                if let Some(id) = args.get("open").and_then(Value::as_str).map(String::from) {
                    let (host, event) = (self.clone(), event.clone());
                    cx.defer(move |cx| {
                        let target = host.shell.borrow().feed.get("/shell").and_then(|v| start_menu::row_target(&v["startMenu"]["nav"], &id));
                        if let Some(target) = target {
                            host.start_menu_open(target, &event, cx);
                        }
                    });
                    return;
                }
                if let Some(text) = args.get("ask").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).map(String::from) {
                    // #EDE-239: the line lands in the Assistant's one window, which takes the keyboard.
                    holder_feeds::ask(text, start_menu_token(&shell));
                    cx.defer(move |cx| {
                        close(&shell, "start-menu", cx);
                        dispatch(&shell, ShellEvent::StartMenuClose, cx);
                    });
                    let shell = self.shell.clone();
                    cx.defer(move |cx| {
                        shell.borrow_mut().feed.set("/shell/startMenu/ask", json!(""));
                        model_changed(&shell, cx);
                    });
                    return;
                }
                cx.defer(move |cx| {
                    if args.get("toggle").and_then(Value::as_bool) == Some(true) {
                        start_menu_toggle(&shell, cx);
                        return;
                    }
                    if let Some(open) = args.get("open").and_then(Value::as_bool) {
                        dispatch(&shell, if open { ShellEvent::StartMenuOpen } else { ShellEvent::StartMenuClose }, cx);
                    }
                    if let Some(query) = args.get("query").and_then(Value::as_str) {
                        start_menu_query(&shell, query.into(), cx);
                    }
                    if let Some(category) = args.get("category").and_then(Value::as_str) {
                        dispatch(&shell, ShellEvent::StartMenuCategory { category: category.into() }, cx);
                    }
                });
            }
            // #EDE-65: a start-menu issue opens in the Exponential app (until the shell shows issues).
            // #EDE-65 J4: on another team or Local desktop, the switcher (and so the app) goes there first.
            "ede_issue" => {
                if let Some(identifier) = args.get("identifier").and_then(Value::as_str).map(String::from) {
                    let source = issues::issue_source(&args);
                    cx.defer(move |cx| {
                        close(&shell, "start-menu", cx);
                        dispatch(&shell, ShellEvent::StartMenuClose, cx);
                        let current = shell.borrow().model.switcher.current.clone();
                        if let Some((kind, id)) = source.filter(|(kind, id)| !(current.kind == *kind && current.id == *id)) {
                            switcher_picked(&shell, kind, &id, cx);
                        }
                        ide_open(&shell, Some(&issues::issue_deep_link(&identifier)));
                    });
                }
            }
            // #EDE-239: the taskbar's Assistant: the Exponential app's one window.
            "ede_chat" => cx.defer(move |cx| assistant_open(&shell, cx)),
            "ede_workspace" => {
                if let Some(index) = args.get("index").and_then(Value::as_u64) {
                    with_ede(&self.shell, |ede| ede.switch_workspace(index as usize));
                }
                if event.surface_id == "expo" {
                    cx.defer(move |cx| overlay_close(&shell, "expo", cx));
                }
            }
            other => tracing::warn!(function = other, "unknown host function"),
        }
    }

    /// The launcher's search box (`/query` filters `/apps`) and the start
    /// menu's (`/shell/startMenu/query`).
    fn on_input(&self, event: &InputEvent, cx: &mut App) {
        let query = event.value.as_str().unwrap_or_default().to_string();
        let shell = self.shell.clone();
        match event.path.as_deref() {
            Some("/query") => cx.defer(move |cx| {
                {
                    let mut s = shell.borrow_mut();
                    let filtered = apps::filter(&s.apps, &query);
                    s.feed.set("/query", json!(query));
                    s.feed.set("/apps", Value::Array(filtered));
                }
                refresh(&shell, cx);
            }),
            Some("/shell/startMenu/query") => cx.defer(move |cx| start_menu_query(&shell, query, cx)),
            _ => {}
        }
    }

    /// VAPP-281: the document's hosted Popover / Menu layers become popup
    /// windows (the main loop opens and closes them).
    fn on_hosted_popups(&self, layers: &[Layer]) {
        let _ = self.hosted_tx.send((self.document.clone(), layers.to_vec()));
    }

    /// App icons are files in the icon themes; thumbnails are `data:` URLs.
    fn media_options(&self) -> MediaOptions {
        MediaOptions { schemes: Some(vec!["https".into(), "http".into(), "data".into(), "file".into()]), ..Default::default() }
    }
}

/// The start menu's own entries (#EDE-53): Files (Nemo, else the default
/// file manager), Version history (the shell's edit history), and the
/// Exponential app when it is installed (it has its own desktop entry).
const FILES_APP: &str = "at.exponential.files";
const HISTORY_APP: &str = "at.exponential.history";

fn builtin_apps(demo: bool) -> Vec<apps::App> {
    let files_exec = if demo || which("nemo") { "nemo".to_string() } else { "xdg-open .".to_string() };
    let files_icon = if demo { None } else { apps::icon_file("system-file-manager").or_else(|| apps::icon_file("folder")) };
    vec![
        apps::App { id: format!("{FILES_APP}.desktop"), name: "Files".into(), comment: "Browse and organize your files".into(), exec: files_exec, exec_url: None, icon: files_icon, keywords: "files nemo file manager folders".into(), categories: vec!["Utility".into(), "System".into()], wm_class: None, steam_app: None },
        apps::App { id: format!("{}.desktop", system_settings::APP_ID), name: "System Settings".into(), comment: "Control Center".into(), exec: String::new(), exec_url: None, icon: None, keywords: "preferences settings control center".into(), categories: vec!["Settings".into(), "System".into()], wm_class: None, steam_app: None },
        apps::App { id: format!("{HISTORY_APP}.desktop"), name: "Version history".into(), comment: "Every edit of the desktop's shell, undo any".into(), exec: String::new(), exec_url: None, icon: None, keywords: "version history git undo shell edits".into(), categories: vec!["Utility".into()], wm_class: None, steam_app: None },
    ]
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file())).unwrap_or(false)
}

/// `/history`: the shell documents' commits, newest first (#EDE-53).
fn history_refresh(shell: &SharedShell) {
    if std::env::var_os("EDE_SHELL_DEMO").is_some_and(|v| !v.is_empty() && v != "0") {
        // The goldens: fixed sample edits instead of the scratch copy's commits.
        let entries = json!([
            {"id": "a1b2c3d4e5f6", "short": "a1b2c3d", "summary": "Edit taskbar.json", "at": "2026-11-06 11:58"},
            {"id": "f6a7b8c9d0e1", "short": "f6a7b8c", "summary": "Edit sidebar.json", "at": "2026-11-06 11:40"},
            {"id": "0f1e2d3c4b5a", "short": "0f1e2d3", "summary": "Factory sidebar.json", "at": "2026-11-06 09:00"}
        ]);
        shell.borrow_mut().feed.set("/history", json!({"entries": entries}));
        return;
    }
    let entries: Vec<Value> = {
        let s = shell.borrow();
        let ui = s.ui.lock().unwrap_or_else(|p| p.into_inner());
        ui.log(60).unwrap_or_default().into_iter().map(|c| json!({"id": c.id, "short": c.id.chars().take(7).collect::<String>(), "summary": c.summary, "at": clock::stamp(c.time)})).collect()
    };
    shell.borrow_mut().feed.set("/history", json!({"entries": entries}));
}

/// `/expo` and `/scale`: the workspaces with their windows and the active
/// workspace's windows, each with its thumbnail (#EDE-49).
fn overlays_refresh(shell: &SharedShell) {
    let mut s = shell.borrow_mut();
    let thumb = |s: &Shell, id: &str| s.thumbnails.get(id).cloned().unwrap_or_default();
    let workspaces: Vec<Value> = uicore_model::workspace_badges(&s.model)
        .into_iter()
        .map(|b| {
            let windows: Vec<Value> = s.model.windows.iter().filter(|w| w.workspace == b.index).map(|w| json!({"id": w.id, "title": w.title, "focused": w.focused, "thumbnail": thumb(&s, &w.id)})).collect();
            json!({"index": b.index, "label": format!("Workspace {}", b.index + 1), "active": b.active, "count": b.count, "countText": if b.count == 0 { String::new() } else { b.count.to_string() }, "windows": windows})
        })
        .collect();
    let active = s.model.workspaces.active;
    let scale_windows: Vec<Value> = s.model.windows.iter().filter(|w| w.workspace == active).map(|w| json!({"id": w.id, "title": w.title, "focused": w.focused, "thumbnail": thumb(&s, &w.id)})).collect();
    s.feed.set("/expo", json!({"workspaces": workspaces}));
    s.feed.set("/scale", json!({"label": format!("Workspace {}", active + 1), "windows": scale_windows}));
}

/// Launches the desktop entry `app` (its id, with or without `.desktop`).
/// #EDE-273: `~/settings/<module>` (a bare id or an old page id too): the
/// start menu's search, an applet's link, the panel menu. The window opens
/// on that module (a Mint tool's module launches the tool instead).
fn settings_open_page(shell: &SharedShell, page: &str, cx: &mut App) {
    close(shell, "start-menu", cx);
    dispatch(shell, ShellEvent::StartMenuClose, cx);
    settings_step(shell, uicore_model::settings::SettingsEvent::Link { target: page.to_string() }, true, cx);
}

/// #EDE-273: System Settings comes up where it was (the grid on its first
/// open, and after it was closed); a single instance: an open one comes to
/// the front.
fn settings_show(shell: &SharedShell, cx: &mut App) {
    settings_reap(shell);
    if shell.borrow().surfaces.activate(system_settings::DOCUMENT, cx) {
        return;
    }
    settings_feed(shell);
    open_on_demand(shell, system_settings::DOCUMENT, cx);
    let title = shell.borrow().settings_nav.feed()["title"].as_str().unwrap_or_default().to_string();
    shell.borrow().surfaces.set_title(system_settings::DOCUMENT, &title, cx);
    refresh(shell, cx);
    settings_focus_search(shell, cx);
}

/// #EDE-274: on the grid the keyboard is in the search, as cinnamon-settings
/// has it (the window opening, Back, Alt+Left), after the frame that shows it.
fn settings_focus_search(shell: &SharedShell, cx: &mut App) {
    if !shell.borrow().settings_nav.state.module.is_empty() {
        return;
    }
    // The search shows from the next layout on: focus it once that frame is drawn.
    let shell = shell.clone();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(Duration::from_millis(120)).await;
        let _ = cx.update(|cx| shell.borrow().surfaces.focus_component(system_settings::DOCUMENT, "search", cx));
    })
    .detach();
}

/// A settings window the person closed from its frame starts on the grid next time.
fn settings_reap(shell: &SharedShell) {
    let closed = shell.borrow_mut().surfaces.reap_closed();
    if closed.iter().any(|n| n == system_settings::DOCUMENT) {
        shell.borrow_mut().settings_nav.reset();
    }
}

fn settings_feed(shell: &SharedShell) {
    let feed = shell.borrow().settings_nav.feed();
    shell.borrow_mut().feed.set("/settings", feed);
}

/// #EDE-273: one step of the window (a tile, back, the search, Enter, a
/// link): a Mint tool launches; `show` = the window comes up (a link).
fn settings_step(shell: &SharedShell, event: uicore_model::settings::SettingsEvent, show: bool, cx: &mut App) {
    settings_reap(shell);
    let next = shell.borrow_mut().settings_nav.step(&event);
    if let Some(command) = next.launch.as_deref() {
        person_launch(shell, command);
    }
    settings_feed(shell);
    if show && next.launch.is_none() {
        settings_show(shell, cx);
    }
    shell.borrow().surfaces.set_title(system_settings::DOCUMENT, &next.title, cx);
    refresh(shell, cx);
    if matches!(event, uicore_model::settings::SettingsEvent::Back) || (show && matches!(event, uicore_model::settings::SettingsEvent::Link { .. })) {
        settings_focus_search(shell, cx);
    }
}

/// #VAPP-273: the apps on the Engine for the current switcher entry: a
/// Local desktop runs the installed packages over the local records; a
/// connected team runs the team's published apps over its synced records
/// as the member (`engine::synced`). Any earlier set goes first.
fn engine_boot(shell: &SharedShell, cx: &mut App) {
    if let Some(engine) = shell.borrow_mut().engine.take() {
        engine::shutdown(&engine, cx);
    }
    // #EDE-141: the desktop agent's seats on those apps go with them (a
    // chat that stays holds none of their stores); the boot below seats the new ones.
    let seats = shell.borrow().engine_seats.clone();
    seats.write().unwrap_or_else(|p| p.into_inner()).clear();
    shell.borrow_mut().team_stores = None;
    let current = shell.borrow().model.switcher.current.clone();
    let connect: Arc<dyn engine::capabilities::Connect> = Arc::new(engine::capabilities::Minted::default());
    let capabilities = engine::capabilities::ede_capabilities(connect, shell.borrow().launchable.clone());
    let (apps, stores) = if current.kind == uicore_model::SwitcherKind::Team {
        let Some(account_id) = current.account.clone() else { return };
        let (account, store) = {
            let s = shell.borrow();
            (s.workspaces.account(&account_id), s.workspaces.store(&account_id))
        };
        let (Some(account), Some(store)) = (account, store) else {
            tracing::info!(team = %current.id, "no synced store for the team yet");
            return;
        };
        let apps_rows = sync::shapes::shape_by_name("apps").and_then(|spec| store.read_all(spec).ok()).unwrap_or_default();
        let members = sync::shapes::shape_by_name("team_members").and_then(|spec| store.read_all(spec).ok()).unwrap_or_default();
        let team_apps = engine::synced::team_apps(&apps_rows, &current.id);
        if team_apps.is_empty() {
            return;
        }
        let team = match engine::synced::TeamStores::open(&shell.borrow().workspaces, &account, &current.id) {
            Ok(t) => Arc::new(t),
            Err(e) => {
                tracing::warn!(team = %current.id, "team records: {e}");
                return;
            }
        };
        team.feed(&store);
        let identity: Arc<dyn eengine::Identity> = Arc::new(engine::synced::MemberIdentity(engine::synced::member_identity(&members, &account.user_id, &current.id)));
        let data_dir = engine::data_dir().join("teams").join(&current.id);
        let stores = match engine::Stores::open(data_dir, capabilities) {
            Ok(s) => s.connected(identity, team.clone()),
            Err(e) => {
                tracing::warn!("apps: {e}");
                return;
            }
        };
        shell.borrow_mut().team_stores = Some(team);
        (team_apps.into_iter().map(|t| t.app).collect::<Vec<_>>(), stores)
    } else {
        let apps = shell.borrow().engine_apps.clone();
        if apps.is_empty() {
            return;
        }
        // #EDE-65 J5b: the Local desktop's own records, files and vaults.
        let Some(desktop) = local_desktop(shell) else { return };
        let current = shell.borrow().workspaces.desktops.current().map(str::to_string);
        match engine::Stores::open(engine::local_data_dir(&desktop.dir, &desktop.entry.id, current.as_deref(), &engine::data_dir()), capabilities) {
            Ok(stores) => (apps, stores),
            Err(e) => {
                tracing::warn!("apps: {e}");
                return;
            }
        }
    };
    let eng = engine::boot(apps, &stores, cx);
    *seats.write().unwrap_or_else(|p| p.into_inner()) = engine::app_seats(&eng.borrow());
    eng.borrow_mut().window_max = {
        let s = shell.borrow();
        engine::window_max(output_size(&s))
    };
    engine::open_widgets(&eng, cx);
    if let Some(team) = &stores.synced {
        team.flush();
    }
    shell.borrow_mut().engine = Some(eng);
}

/// The current Local desktop, opened once (#EDE-57): the switcher's (the
/// index's current is the IDE's pick too, #EDE-65).
fn local_desktop(shell: &SharedShell) -> Option<Arc<elocal::LocalDesktop>> {
    let current = {
        let s = shell.borrow();
        let picked = &s.model.switcher.current;
        if picked.kind == uicore_model::SwitcherKind::Local { picked.id.clone() } else { s.workspaces.desktops.current()?.to_string() }
    };
    if let Some(open) = shell.borrow().local.clone().filter(|d| d.entry.id == current) {
        return Some(open);
    }
    let opened = match shell.borrow().workspaces.desktops.open_desktop(&current) {
        Ok(d) => Arc::new(d),
        Err(e) => {
            tracing::warn!("local desktop: {e}");
            return None;
        }
    };
    shell.borrow_mut().local = Some(opened.clone());
    Some(opened)
}

/// Share as a team…: the accounts to pick from and what goes up.
fn share_start(shell: &SharedShell, _cx: &mut App) {
    let Some(desktop) = local_desktop(shell) else { return };
    let accounts: Vec<(String, String)> = shell.borrow().workspaces.session_accounts().iter().map(|a| (a.id.clone(), a.email.clone())).collect();
    let summary = share::summary(&desktop);
    shell.borrow_mut().share.start(&desktop.entry.id, accounts, summary);
}

/// The upload, to the picked account, named as asked (else as the desktop).
fn share_run(shell: &SharedShell, name: &str, account: &str) {
    let Some(desktop) = local_desktop(shell) else { return };
    let accounts = shell.borrow().workspaces.session_accounts();
    let Some(account) = accounts.iter().find(|a| a.id == account).or_else(|| accounts.first()).cloned() else {
        shell.borrow_mut().share.state = share::State::NeedsAccount;
        return;
    };
    let name = if name.trim().is_empty() { desktop.entry.name.clone() } else { name.trim().to_string() };
    let client = api::TrpcClient::new(&account.instance_url, shell.borrow().workspaces.auth.token_provider(&account.id));
    let device_id = api::device_identity::device_id(&api::default_data_dir());
    let mut s = shell.borrow_mut();
    s.share.account_id = Some(account.id.clone());
    s.share.run(client, desktop, name, Some(device_id));
}

/// `/sharing` into the views.
fn sharing_changed(shell: &SharedShell, cx: &mut App) {
    let feed = shell.borrow().share.feed();
    shell.borrow_mut().feed.set("/sharing", feed);
    refresh(shell, cx);
}

/// The share is done: the desktop becomes the team (its file archived), the
/// team takes its place in the switcher once the account's shape carries it.
fn share_done(shell: &SharedShell, shared: elocal::share::Shared, cx: &mut App) {
    let (desktop_id, account) = {
        let s = shell.borrow();
        (s.share.desktop_id.clone(), s.share.account_id.clone())
    };
    if let (Some(desktop_id), Some(account)) = (desktop_id, account) {
        let shared_as = elocal::SharedAs { account, team_id: shared.team_id.clone(), slug: shared.slug.clone(), shared_at: elocal::time::now() };
        let archived = shell.borrow_mut().workspaces.desktops.archive(&desktop_id, Some(shared_as));
        if let Err(e) = archived {
            tracing::warn!("archive: {e}");
        }
        shell.borrow_mut().local = None;
    }
    shell.borrow_mut().pending_pick = accounts::Pending { team: Some(shared.team_id), account: None };
    switcher_refresh_entries(shell, cx);
}

/// #EDE-223: `accounts.json` moved under the shell (the app or the CLI signed
/// in or out): the account's sync starts or stops, Settings' Account page
/// follows; a sign-in with nothing saved moves a Local start onto the
/// account's team once listed, a sign-out leaves its team for Local.
fn account_changed(shell: &SharedShell, event: api::AuthEvent, cx: &mut App) {
    tracing::info!(?event, "accounts.json changed");
    match &event {
        api::AuthEvent::SignedIn { account_id } => {
            // #EDE-226: exactly one account per login: another one's sign-in changes nothing here.
            let account = shell.borrow().workspaces.session_account().filter(|a| &a.id == account_id);
            if account.is_none() {
                return;
            }
            if let Some(account) = account {
                shell.borrow().workspaces.start_account(&account);
            }
            let wait = shell.borrow().workspaces.nothing_saved() && on_local(shell) && !shell.borrow().pending_pick.is_some();
            if wait {
                shell.borrow_mut().pending_pick = accounts::Pending { team: None, account: Some(account_id.clone()) };
            }
        }
        api::AuthEvent::SignedOut { account_id } | api::AuthEvent::Unauthorized { account_id } => {
            shell.borrow().workspaces.stop_account(account_id);
            // #EDE-226: a deliberate sign-out leaves no store behind.
            shell.borrow().workspaces.wipe_when_forgotten(account_id);
            if shell.borrow().signing_out.as_ref() == Some(account_id) {
                shell.borrow_mut().signing_out = None;
            }
            connect_reset(shell);
            let pending = shell.borrow().pending_pick.account.as_ref() == Some(account_id);
            if pending {
                shell.borrow_mut().pending_pick = accounts::Pending::default();
            }
            let left = shell.borrow().model.switcher.current.account.as_ref() == Some(account_id);
            if left {
                switcher_refresh_entries(shell, cx);
                let local = shell.borrow().workspaces.current_local();
                if let Some(local) = local {
                    switcher_picked(shell, uicore_model::SwitcherKind::Local, &local.id, cx);
                }
                shell.borrow().workspaces.forget_current();
            }
        }
    }
    accounts_feed(shell);
    installs_follow(shell, cx);
}

/// `/accounts` (one at most) and whether its sign-out runs.
fn accounts_feed(shell: &SharedShell) {
    let (accounts, busy) = {
        let s = shell.borrow();
        (s.workspaces.accounts_json(), s.signing_out.is_some())
    };
    let mut s = shell.borrow_mut();
    s.feed.set("/accounts", accounts);
    s.feed.set("/account", json!({"signingOut": busy}));
}

/// #EDE-226: the account left: Connect starts over.
fn connect_reset(shell: &SharedShell) {
    let mut s = shell.borrow_mut();
    if matches!(s.connect.state, connect::State::Done { .. }) {
        s.connect.cancel();
        let feed = s.connect.feed();
        s.feed.set("/connecting", feed);
    }
}

/// #EDE-226: Connect with the sign-in's result, still on the instance it was for.
fn connect_reopen(shell: &SharedShell, cx: &mut App) {
    if shell.borrow().surfaces.is_open("connect") {
        return;
    }
    let instance = shell.borrow().connect.instance.clone();
    shell.borrow_mut().feed.set("/connect", json!({"instance": instance}));
    open_on_demand(shell, "connect", cx);
    refresh(shell, cx);
    // The person's to edit from here.
    shell.borrow_mut().feed.remove("/connect");
}

/// #EDE-226: Sign out of Exponential, the holder's to do; the shell does it
/// when no holder takes it ([`session_account::sign_out`]).
fn account_sign_out(shell: &SharedShell, cx: &mut App) {
    let account = shell.borrow().workspaces.session_account();
    let Some(account) = account.filter(|_| shell.borrow().signing_out.is_none() && !shell.borrow().demo) else { return };
    shell.borrow_mut().signing_out = Some(account.id.clone());
    accounts_feed(shell);
    refresh(shell, cx);
    let asked = accounts::holder_command(api::holder::Command::SignOut { account: account.id.clone() });
    let (auth, done) = (shell.borrow().workspaces.auth.clone(), shell.borrow().signed_out_tx.clone());
    let spawned = std::thread::Builder::new().name("ede-shell-sign-out".into()).spawn(move || {
        if session_account::sign_out(&auth, &api::default_data_dir(), &account, asked) {
            let _ = done.send(account.id);
        }
    });
    if let Err(e) = spawned {
        shell.borrow_mut().signing_out = None;
        accounts_feed(shell);
        failed_action(shell, "sign out of", "Exponential", &e.to_string(), cx);
    }
}

/// #EDE-226: the press goes to the holder, with the code the person compared.
fn install_press(shell: &SharedShell, action: &str, install: String, code: String, cx: &mut App) {
    let account = shell.borrow().workspaces.session_account().map(|a| a.id);
    let Some(account) = account.filter(|_| !shell.borrow().demo) else { return };
    let command = match action {
        "approve" => api::holder::Command::Approve { account, install, code },
        "remove" => api::holder::Command::Remove { account, install, code },
        other => return tracing::warn!(action = other, "unknown install action"),
    };
    if !accounts::holder_command(command) {
        failed_action(shell, action, "the install", "The Exponential app is not running", cx);
    }
}

/// #EDE-226: `/installs` as the holder's file has it now, a failed press told once.
fn installs_follow(shell: &SharedShell, cx: &mut App) {
    if shell.borrow().demo {
        return;
    }
    let (changed, failed) = {
        let mut s = shell.borrow_mut();
        let account = s.workspaces.session_account().map(|a| a.id);
        let changed = s.installs.follow(&api::default_data_dir(), account.as_deref());
        (changed, s.installs.failed())
    };
    if changed {
        {
            let mut s = shell.borrow_mut();
            let installs = s.installs.feed();
            s.feed.set("/installs", installs);
            let needs = needs_feed(&s);
            s.feed.set("/needsYou", needs);
        }
        refresh(shell, cx);
    }
    if let Some((verb, message)) = failed {
        failed_action(shell, &verb, "the install", &message, cx);
    }
}

/// `/needsYou`: the installs waiting for approval count too (the app's rule,
/// `ui::install_approval::counted`); this install waiting shows the list.
fn needs_feed(s: &Shell) -> Value {
    let mut feed = inbox::needs_you_feed(&s.needs_you);
    feed["count"] = json!(s.needs_you.len() + s.installs.count());
    feed
}

/// `Local`, then `Local 2`, `Local 3`… among the desktops that exist.
fn next_desktop_name(shell: &Shell) -> String {
    let taken: Vec<String> = shell.workspaces.desktops.active().iter().map(|d| d.name.clone()).collect();
    if !taken.iter().any(|n| n == "Local") {
        return "Local".into();
    }
    (2..).map(|n| format!("Local {n}")).find(|name| !taken.contains(name)).unwrap_or_else(|| "Local".into())
}

/// The switcher's entries from the desktops index and the accounts' teams
/// (#EDE-56), into the model (the current entry stays when it is still listed).
fn switcher_refresh_entries(shell: &SharedShell, cx: &mut App) {
    // The index is the IDE's too (#EDE-65): its new or renamed desktops show.
    shell.borrow_mut().workspaces.desktops.reload();
    let entries = shell.borrow().workspaces.entries();
    if entries.is_empty() {
        return;
    }
    let pending = shell.borrow().pending_pick.resolve(&entries);
    dispatch(shell, ShellEvent::SwitcherSet { entries }, cx);
    if let Some(id) = pending {
        shell.borrow_mut().pending_pick = accounts::Pending::default();
        switcher_picked(shell, uicore_model::SwitcherKind::Team, &id, cx);
    }
}

/// #EDE-65: a pick from the switcher (or a team that arrived for a pending
/// one): the current entry, remembered across logins; the apps and
/// the Exponential app follow it (a team: the app switches to it too).
fn switcher_picked(shell: &SharedShell, entry_kind: uicore_model::SwitcherKind, id: &str, cx: &mut App) {
    let picked_local = entry_kind == uicore_model::SwitcherKind::Local;
    if picked_local {
        // The desktops index remembers the current Local desktop.
        if let Err(e) = shell.borrow_mut().workspaces.desktops.set_current(id) {
            tracing::warn!("desktops.json: {e}");
        }
    }
    dispatch(shell, ShellEvent::SwitcherPick { entry_kind, id: id.into() }, cx);
    // The switcher lives in the start menu: the reducer closed it, the popup goes too.
    if !shell.borrow().model.start_menu.open {
        close(shell, "start-menu", cx);
    }
    let current = shell.borrow().model.switcher.current.clone();
    if current.id != id {
        tracing::warn!(id, "the switcher does not list the pick");
        return;
    }
    shell.borrow().workspaces.save_current(&current);
    accounts::ide_follow(if picked_local { uicore_model::SwitcherKind::Local } else { uicore_model::SwitcherKind::Team }, id);
    // Another desktop or a team: the opened desktop goes.
    let changed = shell.borrow().local.as_ref().is_some_and(|d| !picked_local || d.entry.id != id);
    if changed {
        shell.borrow_mut().local = None;
    }
    // #VAPP-273: the apps follow the desktop (local records or the team's).
    engine_boot(shell, cx);
}

/// The Exponential app on `link` (its single-instance socket), else started.
fn ide_open(shell: &SharedShell, link: Option<&str>) {
    if shell.borrow().workspaces.demo() {
        tracing::info!(?link, "demo: no hand-off");
        return;
    }
    if link.is_some_and(accounts::ide_send) {
        bring_forward(shell, &EXPONENTIAL_APPS);
        return;
    }
    bring_forward(shell, &EXPONENTIAL_APPS);
    // A cold start keeps the link: the entry's Exec takes it as `%U` (its
    // single-instance boot feeds command-line `exponential://` args in).
    let command = app_entry(&shell.borrow(), "at.exponential").map(|a| ide_command(a, link));
    match command {
        Some(command) => person_launch(shell, &command),
        None => tracing::warn!("no at.exponential desktop entry to launch"),
    }
}

/// The Exponential app's launch command, opening `link` when there is one.
fn ide_command(app: &apps::App, link: Option<&str>) -> String {
    match (app.exec_url.as_deref(), link) {
        (Some(exec), Some(link)) => apps::exec_with_url(exec, link),
        _ => app.exec.clone(),
    }
}

/// #EDE-227: eDE's component runs as the session's helper: the package
/// ships `exponential-app` beside this shell (the compositor, else
/// exponential-device.service, starts it).
fn component_runs() -> bool {
    std::env::current_exe().ok().and_then(|me| me.parent().map(|dir| dir.join("exponential-app"))).is_some_and(|app| app.is_file())
}

/// #EDE-227: a saved pin of the person's own app entry (`standalone`) is
/// the component's: its windows group under the component's app id, so
/// the pin and they are one taskbar button.
fn pinned_as(app: &str, standalone: &[String]) -> String {
    let id = app.trim_end_matches(".desktop");
    if standalone.iter().any(|s| s == id) { apps::COMPONENT_APP_ID.to_string() } else { app.to_string() }
}

/// The Exponential app's ids: one IDE per person, a hand-off target.
const EXPONENTIAL_APPS: [&str; 2] = ["at.exponential", "at.exponential.staging"];

/// #EDE-273: a taskbar button's launch: the shell's own System Settings
/// opens its window, any other app launches.
fn launch_or_own(shell: &SharedShell, app: &str, cx: &mut App) {
    if app.trim_end_matches(".desktop") == system_settings::APP_ID {
        settings_show(shell, cx);
    } else {
        launch_app(shell, app);
    }
}

/// #EDE-64: the sidebar hands off to the Exponential app (Inbox, New issue,
/// Needs you, Chats): its window comes forward when one is up, a burst of
/// clicks launches anything once.
fn launch_app(shell: &SharedShell, app: &str) {
    let wanted = app.trim_end_matches(".desktop");
    // #EDE-227: the login's holder brings a window up itself, with this
    // press's launch token.
    if EXPONENTIAL_APPS.contains(&wanted) && !demo() && open_in_holder(shell, None) {
        return;
    }
    if EXPONENTIAL_APPS.contains(&wanted) && bring_forward(shell, &[wanted]) {
        return;
    }
    let exec = shell.borrow().apps.iter().find(|a| a.id.trim_end_matches(".desktop") == wanted).map(|a| a.exec.clone());
    let Some(exec) = exec else {
        tracing::warn!(app, "no desktop entry to launch");
        return;
    };
    if shell.borrow_mut().launches.admit(wanted, Instant::now()) {
        person_launch(shell, &exec);
    } else {
        tracing::info!(app, "already starting");
    }
}

/// #EDE-224: after a hand-off the app's window comes forward: its window up
/// now (true), else the next one it opens within a launch token's lifetime
/// (the app gets the link on its socket, without a token, so that window
/// opens behind the focused one and the app cannot raise it itself).
fn bring_forward(shell: &SharedShell, apps: &[&str]) -> bool {
    let open = {
        let s = shell.borrow();
        let mine: Vec<_> = s.model.windows.iter().filter(|w| apps.contains(&w.app.as_str())).collect();
        mine.iter().find(|w| w.focused).or(mine.first()).map(|w| w.id.clone())
    };
    if let Some(id) = open.and_then(|id| ede_protocol::WindowId::new(&id).ok()) {
        with_ede(shell, |ede| ede.focus(id));
        return true;
    }
    let mut s = shell.borrow_mut();
    let known: Vec<String> = s.windows.iter().map(|w| w.id.to_string()).collect();
    s.focus_next.arm(apps, known, Instant::now());
    false
}

/// The installed entry for an app id (a window's app_id, a pin), if any:
/// Cinnamon's window tracker's order ([`apps::for_window`]), then (#EDE-242)
/// the Steam game an X11 window's process runs.
fn app_entry<'a>(shell: &'a Shell, app: &str) -> Option<&'a apps::App> {
    apps::for_window(&shell.apps, app).or_else(|| shell.x11_hints.cached(app).and_then(|h| h.steam_game.as_deref()).and_then(|game| apps::for_steam_game(&shell.apps, game)))
}

/// The entry's icon, else (#EDE-242) the window's own `_NET_WM_ICON`.
fn app_icon(shell: &Shell, app: &str) -> String {
    app_entry(shell, app)
        .and_then(|a| a.icon.clone())
        // #EDE-227: eDE's component has no start menu entry; its icon is the theme's.
        .or_else(|| EXPONENTIAL_APPS.contains(&app).then(|| apps::icon_file(app)).flatten())
        .or_else(|| shell.x11_hints.cached(app).and_then(|h| h.icon.clone()))
        .map(|p| format!("file://{}", p.display()))
        .unwrap_or_default()
}

/// The entry's name, else (#EDE-242, Cinnamon's window-backed app) a
/// window's title, else the id.
fn app_name(shell: &Shell, app: &str) -> String {
    app_entry(shell, app)
        .map(|a| a.name.clone())
        // #EDE-227: eDE's component (no start menu entry).
        .or_else(|| EXPONENTIAL_APPS.contains(&app).then(|| "Exponential".to_string()))
        .or_else(|| shell.windows.iter().filter(|w| w.app_id.as_deref() == Some(app)).find_map(|w| w.title.clone().filter(|t| !t.is_empty())))
        .unwrap_or_else(|| app.to_string())
}

/// #EDE-242: the X11 hints of every class no entry names, read once.
fn learn_x11_apps(shell: &mut Shell, list: &[ede_protocol::WindowInfo]) {
    for w in list.iter().filter(|w| w.x11 && !w.protected) {
        let Some(class) = w.app_id.as_deref() else { continue };
        if shell.x11_hints.cached(class).is_none() && apps::for_window(&shell.apps, class).is_none() {
            shell.x11_hints.lookup(class);
        }
    }
}

/// #EDE-254: the desktop notifications of `app` (by its id, its entry's id
/// or its name, as the sender names itself), the group's notification badge.
fn app_notifications(shell: &Shell, app: &str) -> usize {
    let entry = app_entry(shell, app);
    let names: Vec<String> = [Some(app.trim_end_matches(".desktop").to_string()), entry.map(|e| e.id.trim_end_matches(".desktop").to_string()), entry.map(|e| e.name.clone())].into_iter().flatten().filter(|n| !n.is_empty()).collect();
    shell.notifications.list.iter().filter(|n| names.iter().any(|name| name.eq_ignore_ascii_case(n.app.trim_end_matches(".desktop")))).count()
}

/// #EDE-254: what the group's menu knows beyond the model: the outputs, the
/// app's windows' output and maximized state, its recent files and its
/// desktop entry's actions.
fn taskbar_menu_context(shell: &Shell, app: &str) -> uicore_model::TaskbarMenuContext {
    let windows = shell
        .model
        .windows
        .iter()
        .filter(|w| w.app == app)
        .filter_map(|w| {
            let info = shell.windows.iter().find(|i| i.id.as_str() == w.id)?;
            let output = output_of(&shell.person_outputs, info).and_then(|o| shell.person_outputs.iter().position(|p| *p == o)).unwrap_or(0);
            Some((w.id.clone(), uicore_model::WindowFacts { output, maximized: info.maximized }))
        })
        .collect();
    let entry = app_entry(shell, app);
    let (recent, actions) = match entry {
        Some(e) if !demo() => (taskbar_menu::recent(&e.id), taskbar_menu::menu_actions(&taskbar_menu::actions(&e.id))),
        _ => (Vec::new(), Vec::new()),
    };
    uicore_model::TaskbarMenuContext { outputs: shell.person_outputs.len().max(1), windows, recent, actions, entry: entry.is_some() }
}

/// Mint's category glyphs for the start menu's column.
fn category_icon(id: &str) -> &'static str {
    match id {
        "all" => "nav-apps",
        "favourites" => "ui-pin",
        "exponential" => "brand-exponential",
        "accessories" => "ui-properties",
        "administration" => "nav-settings",
        "education" => "ui-help",
        "games" => "ui-run",
        "graphics" => "ui-edit",
        "internet" => "ui-link",
        "office" => "ui-file",
        "programming" => "nav-terminal",
        "science" => "ui-info",
        "sound-video" => "media-volume-up",
        _ => "nav-apps",
    }
}

/// What the documents need on top of the reducer's `/shell`: names and
/// icons for apps, the start menu's favourites.
fn decorate_shell(shell: &Shell, mut value: Value) -> Value {
    if let Some(buttons) = value["taskbar"]["buttons"].as_array_mut() {
        for b in buttons {
            let app = b["app"].as_str().unwrap_or_default().to_string();
            b["name"] = json!(app_name(shell, &app));
            b["icon"] = json!(app_icon(shell, &app));
            // #EDE-254: grouped-window-list's badges: the window count on a
            // group of two or more, the app's notification count.
            b["count"] = json!(b["windows"].as_array().map_or(0, Vec::len));
            b["notifications"] = json!(app_notifications(shell, &app));
            b["menu"] = json!(uicore_model::taskbar_menu(&shell.model, &app, &taskbar_menu_context(shell, &app)));
            if demo() && std::env::var("EDE_SHELL_DEMO_TASKBAR_MENU").is_ok_and(|open| open == app) {
                b["menuOpen"] = json!(true);
            }
        }
    }
    // Settings' Workspaces page lists them.
    let workspaces: Vec<Value> = uicore_model::workspace_badges(&shell.model)
        .into_iter()
        .map(|b| json!({"index": b.index, "label": format!("Workspace {}", b.index + 1), "active": b.active, "count": b.count, "countText": if b.count == 0 { String::new() } else { b.count.to_string() }}))
        .collect();
    value["workspaces"]["rows"] = Value::Array(workspaces);
    // #EDE-65: Connect only without an account, Share as a team only on a Local desktop.
    value["switcher"]["connection"] = accounts::connection_json(&shell.model.switcher.current, &shell.workspaces.signed_in());
    if let Some(entries) = value["switcher"]["entries"].as_array_mut() {
        for e in entries {
            let current = e["kind"] == shell.model.switcher.current.kind.clone().into_json() && e["id"] == json!(shell.model.switcher.current.id);
            e["current"] = json!(current);
        }
    }
    let current_category = shell.model.start_menu.category.clone();
    if let Some(categories) = value["startMenu"]["categories"].as_array_mut() {
        for c in categories {
            let id = c["id"].as_str().unwrap_or_default().to_string();
            c["icon"] = json!(category_icon(&id));
            c["selected"] = json!(id == current_category);
        }
    }
    if let Some(apps) = value["startMenu"]["apps"].as_array_mut() {
        for a in apps {
            let id = a["id"].as_str().unwrap_or_default().to_string();
            a["icon"] = json!(app_icon(shell, &id));
        }
    }
    let favourites: Vec<Value> = {
        let mut state = shell.model.clone();
        state.start_menu.query.clear();
        state.start_menu.category = "favourites".into();
        uicore_model::start_menu_apps(&state, &shell.menu_apps, &shell.model.taskbar.pinned).into_iter().map(|e| json!({"id": e.id, "name": e.name, "icon": app_icon(shell, &e.id)})).collect()
    };
    value["startMenu"]["favourites"] = Value::Array(favourites);
    value["startMenu"]["nav"] = start_menu_nav(shell);
    value["startMenu"]["assistant"] = holder_feeds::assistant_json(&shell.holder_feeds);
    value
}

/// #EDE-258: `/shell/startMenu/nav`: Cinnamon's columns over the holder's
/// tree, Local and the app groups (uicore shell.json `startMenuNav`), the
/// apps' pictures and the keyboard's selection on top.
fn start_menu_nav(shell: &Shell) -> Value {
    let tree: Vec<uicore_model::StartMenuNode> = shell.holder_feeds.tree().iter().filter_map(|node| serde_json::from_value(node.clone()).ok()).collect();
    let nav = uicore_model::start_menu_nav(&shell.model, &tree, &shell.local_tree, &shell.menu_apps, &shell.model.taskbar.pinned, start_menu::settings_modules(), &shell.issue_hits);
    let mut nav = serde_json::to_value(nav).unwrap_or(Value::Null);
    for row in nav["rows"].as_array_mut().into_iter().flatten() {
        if let Some(app) = row["target"]["app"].as_str().map(String::from) {
            let icon = app_icon(shell, &app);
            if !icon.is_empty() {
                // The app's own picture in place of the concept glyph.
                row["image"] = json!(icon);
                row["icon"] = Value::Null;
            }
        }
    }
    start_menu::mark(&mut nav, &shell.menu_selection, &shell.model.start_menu.query);
    nav
}

trait IntoJson {
    fn into_json(self) -> Value;
}
impl IntoJson for uicore_model::SwitcherKind {
    fn into_json(self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// `/applets`: what the taskbar's applet buttons show, from the services.
fn applets_json(services: &Value) -> Value {
    let network = &services["network"];
    let network_icon = if network["available"].as_bool() != Some(true) {
        "ui-offline"
    } else if network["wiredConnected"].as_bool() == Some(true) {
        "ede-wired"
    } else if network["wifiEnabled"].as_bool() == Some(false) {
        "ui-offline"
    } else {
        "ede-wifi"
    };
    let sound = &services["sound"];
    let volume = sound["volume"].as_u64().unwrap_or(0);
    let sound_icon = if sound["muted"].as_bool() == Some(true) {
        "media-volume-mute"
    } else if volume == 0 {
        "media-volume-off"
    } else if volume < 50 {
        "media-volume-down"
    } else {
        "media-volume-up"
    };
    let bluetooth = &services["bluetooth"];
    let battery = &services["power"]["battery"];
    let charging = battery["state"].as_str() == Some("charging") || battery["state"].as_str() == Some("full");
    json!({
        "network": {"icon": network_icon},
        "sound": {"icon": sound_icon},
        "bluetooth": {"show": bluetooth["available"].as_bool() == Some(true) && bluetooth["powered"].as_bool() == Some(true)},
        "battery": {"show": battery["present"].as_bool() == Some(true), "icon": if charging { "ede-battery-charging" } else { "ede-battery" }},
        // #EDE-245: Cinnamon's printers applet "When printers exist", its
        // removable-drives applet while one is mounted.
        "printers": {"show": services["printers"]["printers"].as_array().is_some_and(|p| !p.is_empty())},
        "drives": {"show": services["disks"]["removable"].as_array().is_some_and(|d| !d.is_empty())},
    })
}

/// The popup documents anchored at `component` of `parent`.
fn popups_of(shell: &Shell, parent: &str, component: &str) -> Vec<String> {
    let ui = shell.ui.lock().unwrap_or_else(|p| p.into_inner());
    ui.documents()
        .filter(|d| matches!(&d.surface, SurfaceHeader::Popup { popup, .. } if popup.parent == parent && popup.anchor == component))
        .map(|d| d.name.clone())
        .collect()
}

/// The first output's logical size (the goldens' headless output, the
/// session's primary), 1280×720 without a compositor.
fn output_size(shell: &Shell) -> (u32, u32) {
    shell
        .feed
        .get("/outputs")
        .and_then(Value::as_array)
        .and_then(|o| o.first())
        .and_then(|o| Some((o["width"].as_u64()? as u32, o["height"].as_u64()? as u32)))
        .filter(|(w, h)| *w > 0 && *h > 0)
        .unwrap_or((1280, 720))
}

/// The person outputs' x, y, width, height.
fn person_outputs(outputs: &[ede_protocol::OutputInfo]) -> Vec<(i32, i32, i32, i32)> {
    outputs.iter().filter(|o| o.is_persons()).map(|o| (o.x, o.y, o.width, o.height)).collect()
}

/// #EDE-225: the output most of `window` is on, as the compositor picks the
/// output a shell overlay that names none opens on.
fn output_of(outputs: &[(i32, i32, i32, i32)], window: &ede_protocol::WindowInfo) -> Option<(i32, i32, i32, i32)> {
    let overlap = |&(x, y, w, h): &(i32, i32, i32, i32)| {
        let width = (x + w).min(window.x + window.width) - x.max(window.x);
        let height = (y + h).min(window.y + window.height) - y.max(window.y);
        if width > 0 && height > 0 { width as i64 * height as i64 } else { 0 }
    };
    outputs.iter().copied().filter(|o| overlap(o) > 0).max_by_key(overlap)
}

/// #EDE-225: the logical size of the output the switcher opens on: the
/// focused window's, else the first.
fn focused_output_size(shell: &Shell) -> (u32, u32) {
    let focused = shell.windows.iter().find(|w| w.focused && w.is_persons() && !w.minimized);
    focused.and_then(|w| output_of(&shell.person_outputs, w)).filter(|o| o.2 > 0 && o.3 > 0).map(|o| (o.2 as u32, o.3 as u32)).unwrap_or_else(|| output_size(shell))
}

/// A stretched axis (anchored to both of its edges) sized to the output,
/// since the window takes the size it asks for; the sidebar the rail's
/// width and exclusive zone (`shell-layout.json`), standing on the taskbar
/// (#EDE-65); any other document's as written.
fn header_for(shell: &Shell, doc: &Document) -> SurfaceHeader {
    let out = output_size(shell);
    let mut header = layer_header(doc, out);
    // The Alt-Tab popup hugs its tiles (#EDE-65), capped at the output it
    // opens on (#EDE-225: the focused window's).
    if let (true, SurfaceHeader::Layer { size, .. }) = (doc.name == "switcher", &mut header) {
        size.width = Some(switcher_width(uicore_model::alt_tab_order(&shell.model).len(), focused_output_size(shell).0));
    }
    // #EXP-2024: the pinned view is sized by the display and its space (spaces.json).
    if let (true, SurfaceHeader::Layer { size, .. }) = (doc.name == spaces::VIEW, &mut header) {
        if let Some((_, view)) = shell.spaces.view(first_display(shell)) {
            size.width = Some(view.width);
            size.height = Some(view.height);
        }
    }
    // #EDE-253: the window menu's one-pixel layer stands at its point.
    if let (true, SurfaceHeader::Layer { margin, .. }, Some(menu)) = (doc.name == window_menu::DOCUMENT, &mut header, shell.window_menu.as_ref()) {
        let (left, top) = window_menu::margin_on(&shell.person_outputs, menu.x, menu.y);
        margin.left = left;
        margin.top = top;
    }
    taskbar_popup_room(&mut header, popup_output_size(shell));
    header
}

/// #EDE-168: the size a taskbar popup measures its room on: the
/// compositor's first output (the goldens' demo feeds sample outputs to the
/// Displays page, never the headless output the shell stands on).
fn popup_output_size(shell: &Shell) -> (u32, u32) {
    shell.first_output.filter(|o| o.2 > 0 && o.3 > 0).map(|o| (o.2 as u32, o.3 as u32)).unwrap_or_else(|| output_size(shell))
}

/// #EDE-168: a taskbar popup never reaches past the output's work area: it
/// stands on its button, at most the height above the taskbar less the
/// panel's margin (854×534 logical, 1280×800 at 1.5: the sound applet's
/// 600 became 476). Its root follows ([`size_root`]) and its content
/// scrolls; a root that hugs its content takes the window down to it
/// ([`fit_popups`]). #EDE-259: and never wider than the output less the
/// panel's margin either side (Cinnamon's menu shrinks to the work area:
/// the 860 px start menu on an 854 px output was cut off at the right).
fn taskbar_popup_room(header: &mut SurfaceHeader, (out_w, out_h): (u32, u32)) {
    use ede_ui::surface::Placement;
    let SurfaceHeader::Popup { popup, .. } = header else { return };
    if popup.parent != "taskbar" || !matches!(popup.placement, Placement::TopStart | Placement::TopEnd) {
        return;
    }
    let layout = shell_layout();
    let room = out_h.saturating_sub(layout.taskbar_height + layout.panel_margin).max(1);
    popup.size.height = popup.size.height.min(room);
    let room_w = out_w.saturating_sub(2 * layout.panel_margin).max(1);
    popup.size.width = popup.size.width.min(room_w);
}

/// #EDE-168: the open taskbar popups (every tick): drawn again when the
/// output's room for them changed (an output that arrived or changed after
/// the popup opened), their windows at their roots' laid-out heights (a
/// popup's rows arrive after it opened).
fn fit_popups(shell: &SharedShell, cx: &mut App) {
    let open: Vec<Document> = {
        let s = shell.borrow();
        let ui = s.ui.lock().unwrap_or_else(|p| p.into_inner());
        ui.documents()
            .filter(|d| matches!(&d.surface, SurfaceHeader::Popup { popup, .. } if popup.parent == "taskbar"))
            .filter(|d| s.surfaces.is_open(&d.name))
            .cloned()
            .collect()
    };
    shell.borrow_mut().popup_rooms.retain(|n, _| open.iter().any(|d| &d.name == n));
    for doc in open {
        let SurfaceHeader::Popup { popup, .. } = header_for(&shell.borrow(), &doc) else { continue };
        let room = popup.size.height;
        let before = shell.borrow_mut().popup_rooms.insert(doc.name.clone(), (popup.size.width, room));
        if before.is_some_and(|b| b != (popup.size.width, room)) {
            let (drawn, feed) = {
                let s = shell.borrow();
                (drawn_now(&s, &doc), s.feed.clone())
            };
            shell.borrow_mut().surfaces.redraw(&drawn, &feed, cx);
            tracing::info!(document = %doc.name, room, "popup room changed");
        }
        if shell.borrow_mut().surfaces.fit_popup(&doc.name, room as f32, cx) {
            tracing::info!(document = %doc.name, "popup fitted");
        }
    }
}

/// [`header_for`] on an output of `(out_w, out_h)` logical px.
fn layer_header(doc: &Document, (out_w, out_h): (u32, u32)) -> SurfaceHeader {
    let mut header = doc.surface.clone();
    if let SurfaceHeader::Layer { size, exclusive_zone, anchor, margin, .. } = &mut header {
        let layout = shell_layout();
        // #EDE-65: a stretched surface keeps its margins (Settings stands on the taskbar).
        if anchor.contains(&ede_ui::surface::Edge::Left) && anchor.contains(&ede_ui::surface::Edge::Right) {
            size.width = Some(out_w.saturating_sub(margin.left + margin.right).max(1));
        }
        if anchor.contains(&ede_ui::surface::Edge::Top) && anchor.contains(&ede_ui::surface::Edge::Bottom) {
            let taskbar = if doc.name == "sidebar" { layout.taskbar_height } else { 0 };
            size.height = Some(out_h.saturating_sub(margin.top + margin.bottom + taskbar).max(1));
        }
        if doc.name == "sidebar" {
            size.width = Some(layout.rail_width);
            if *exclusive_zone > 0 {
                *exclusive_zone = layout.rail_width as i32;
            }
        }
    }
    header
}

/// A layer document's root takes the surface's size in px (`100%` of a
/// layer window is whatever it was opened at); a popup's root is at most
/// its window's height (#EDE-168: [`taskbar_popup_room`]).
fn size_root(doc: &mut Document, header: &SurfaceHeader) {
    if let (SurfaceHeader::Popup { popup, .. }, ede_ui::Body::Nested(root)) = (header, &mut doc.body) {
        let (room, room_w) = (popup.size.height, popup.size.width);
        let style = root.style.get_or_insert_with(Default::default);
        if style.get("height").and_then(Value::as_f64).is_some_and(|h| h > room as f64) {
            style.insert("height".into(), json!(room));
        }
        style.insert("maxHeight".into(), json!(room));
        // #EDE-259: a root wider than its window takes the window's width.
        if style.get("width").and_then(Value::as_f64).is_some_and(|w| w > room_w as f64) {
            style.insert("width".into(), json!(room_w));
        }
        return;
    }
    let SurfaceHeader::Layer { size, .. } = header else { return };
    if let ede_ui::Body::Nested(root) = &mut doc.body {
        let style = root.style.get_or_insert_with(Default::default);
        if let Some(w) = size.width {
            style.insert("width".into(), json!(w));
        }
        if let Some(h) = size.height {
            style.insert("height".into(), json!(h));
        }
    }
}

/// The order layer documents open in: the ground first, then the taskbar
/// (its exclusive zone takes the whole bottom edge, #EDE-65), then the
/// sidebar (the rail on the left edge above it), then the others by name.
/// smithay arranges layer surfaces in the order they were mapped.
fn layer_rank(doc: &Document) -> (u8, String) {
    let rank = match (&doc.surface, doc.name.as_str()) {
        (SurfaceHeader::Layer { layer: DocLayer::Background, .. }, _) => 0,
        (_, "taskbar") => 1,
        (_, "panel") => 1,
        (_, "sidebar") => 2,
        _ => 3,
    };
    (rank, doc.name.clone())
}

/// THE document as the shell draws it (#EDE-141): the file's, without the
/// IDE's sidebar parts, the root at the surface's size, the header it opens
/// with. Every load goes through it: an open, an edit on disk, an output
/// change.
fn as_drawn(doc: &Document, header: SurfaceHeader) -> Document {
    let mut doc = doc.clone();
    drawn(&mut doc);
    size_root(&mut doc, &header);
    doc.surface = header;
    doc
}

/// [`as_drawn`] on the output of now.
fn drawn_now(shell: &Shell, doc: &Document) -> Document {
    as_drawn(doc, header_for(shell, doc))
}

/// #EDE-238: documents eDE never draws: the left rail (the start menu's
/// tree is the navigation; the IDE still mounts `sidebar`); #EDE-239 the
/// chat overlay (the Assistant is the holder's window; the IDE's chat tab
/// still draws `chat`).
pub(crate) const NOT_DRAWN: &[&str] = &["sidebar", "chat"];

fn open_layer(shell: &SharedShell, doc: &Document, cx: &mut App) {
    if NOT_DRAWN.contains(&doc.name.as_str()) {
        return;
    }
    let (doc, feed) = {
        let s = shell.borrow();
        (drawn_now(&s, doc), s.feed.clone())
    };
    if shell.borrow_mut().surfaces.open_layer(&doc, &feed, cx) {
        let mut s = shell.borrow_mut();
        let next = s.layers_opened.values().max().map_or(1, |n| n + 1);
        s.layers_opened.insert(doc.name.clone(), next);
    }
}

fn close(shell: &SharedShell, name: &str, cx: &mut App) {
    shell.borrow_mut().surfaces.close(name, cx);
}

fn toggle_popup(shell: &SharedShell, name: &str, cx: &mut App) {
    if shell.borrow().surfaces.is_open(name) {
        close(shell, name, cx);
        return;
    }
    let Some(doc) = document(&shell.borrow(), name) else { return };
    // #EDE-168: at the room the output has for it.
    let doc = drawn_now(&shell.borrow(), &doc);
    if let SurfaceHeader::Popup { popup, .. } = &doc.surface {
        shell.borrow_mut().popup_rooms.insert(name.to_string(), (popup.size.width, popup.size.height));
    }
    let feed = shell.borrow().feed.clone();
    shell.borrow_mut().popups_opened.insert(name.to_string(), now_ms());
    shell.borrow_mut().surfaces.open_popup(&doc, &feed, cx);
}

/// A change on disk: redraw an open surface, open a new layer surface,
/// close a removed one. Popups re-open from their anchor.
fn apply_change(shell: &SharedShell, change: Change, cx: &mut App) {
    match change {
        Change::Applied { name, .. } => {
            history_refresh(shell);
            let Some(doc) = document(&shell.borrow(), &name) else { return };
            let open = shell.borrow().surfaces.is_open(&name);
            match (open, &doc.surface) {
                (true, _) => {
                    let (doc, feed) = {
                        let s = shell.borrow();
                        (drawn_now(&s, &doc), s.feed.clone())
                    };
                    shell.borrow_mut().surfaces.redraw(&doc, &feed, cx);
                    tracing::info!(document = %name, "redrawn");
                }
                (false, SurfaceHeader::Layer { on_demand: false, .. }) => open_layer(shell, &doc, cx),
                // An on-demand surface (the switcher, the OSD) waits for its hotkey, a popup for its anchor.
                (false, SurfaceHeader::Layer { on_demand: true, .. }) | (false, SurfaceHeader::Popup { .. }) | (false, SurfaceHeader::Window { .. }) => {
                    if let Some(line) = loaded_frame(false, &doc) {
                        println!("{line}");
                    }
                }
            }
        }
        Change::Removed { name, .. } => close(shell, &name, cx),
        Change::Rejected { name, issues, .. } => tracing::warn!(document = %name, ?issues, "refused; the last good version stays"),
        // #EDE-235: every surface redraws in the new theme; a refused one
        // leaves the last good theme on screen.
        Change::ThemeApplied { id, .. } => {
            history_refresh(shell);
            let theme = shell_theme(shell.borrow().ui.lock().unwrap_or_else(|p| p.into_inner()).theme());
            shell.borrow_mut().surfaces.set_theme(theme, cx);
            tracing::info!(theme = %id, "theme applied");
        }
        Change::ThemeRejected { issues, .. } => tracing::warn!(?issues, "theme refused; the last good one stays"),
    }
}

/// The theme the shell's surfaces paint in: `base` (the ui directory's)
/// with the IDE's ground rule (no Sidebar fill), and the rail's glyphs in
/// its colours.
fn shell_theme(base: &eui_core::theme::ResolvedTheme) -> std::sync::Arc<eui_core::theme::ResolvedTheme> {
    uicore_gpui::glyphs::set_palette(uicore_gpui::glyphs::Palette::of(base));
    uicore_gpui::inline::theme(base)
}

/// The frame line a change owes the supervisor without drawing (#EDE-110):
/// a closed popup or on-demand layer is loaded, not drawn, and loading it
/// proves the edit (one that breaks once opened crashes the shell, and
/// three crashes revert it). An open or opening surface reports its frame.
fn loaded_frame(open: bool, doc: &Document) -> Option<String> {
    let closed = !open && matches!(doc.surface, SurfaceHeader::Layer { on_demand: true, .. } | SurfaceHeader::Popup { .. } | SurfaceHeader::Window { .. });
    closed.then(|| FrameLine::line_for_digest(&doc.name, &doc.digest))
}

/// Pushes the feed into every open view.
fn refresh(shell: &SharedShell, cx: &mut App) {
    let feed = shell.borrow().feed.clone();
    shell.borrow().surfaces.feed_all(&feed, cx);
}

/// `/shell` from the model, then every view.
fn model_changed(shell: &SharedShell, cx: &mut App) {
    {
        let mut s = shell.borrow_mut();
        let favourites: Vec<String> = s.model.taskbar.pinned.clone();
        let value = uicore_data::shell_feed(&s.model, &s.menu_apps, &favourites);
        let value = decorate_shell(&s, value);
        s.feed.set("/shell", value);
        // #EDE-239: the taskbar's session area over the holder's sessions.
        let sessions = sessions_area::feed(&s.holder_feeds, popup_output_size(&s).0);
        s.feed.set("/sessions", sessions);
        // #EDE-266: the computer-use tray's steps come from the same feed.
        let tray = cu_tray_feed(&s);
        s.feed.set("/cuTray", tray);
    }
    refresh(shell, cx);
}

/// One event through the reducer, then `/shell` into every view.
fn dispatch(shell: &SharedShell, event: ShellEvent, cx: &mut App) {
    {
        let mut s = shell.borrow_mut();
        let before = s.model.start_menu.query.clone();
        s.model = reduce(&s.model, &event);
        // #EDE-92: an open, a close or new text = the first result again.
        let open = s.model.start_menu.open;
        s.menu_selection.follow(&event, &before, open);
    }
    model_changed(shell, cx);
}

/// `/windows` and the model from the compositor's list.
fn windows_changed(shell: &SharedShell, list: Vec<ede_protocol::WindowInfo>, workspaces: Vec<ede_protocol::WorkspaceInfo>, cx: &mut App) {
    let next = {
        let mut s = shell.borrow_mut();
        for e in uicore_data::workspace_events(&s.model, &workspaces) {
            s.model = reduce(&s.model, &e);
        }
        for e in uicore_data::window_events(&s.model, &list) {
            s.model = reduce(&s.model, &e);
        }
        let active = s.model.workspaces.active;
        let feed = uicore_data::windows_feed(&list, active);
        s.feed.set("/windows", Value::Array(feed));
        for app in list.iter().filter_map(|w| w.app_id.as_deref()) {
            s.launches.mapped(app);
        }
        let next = s.focus_next.opened(list.iter().map(|w| (w.id.as_str(), w.app_id.as_deref())), Instant::now());
        learn_x11_apps(&mut s, &list);
        s.windows = list;
        next
    };
    // #EDE-224: the hand-off's window, opened behind: forward now.
    if let Some(id) = next.and_then(|id| ede_protocol::WindowId::new(&id).ok()) {
        tracing::info!(window = %id, "the hand-off's window comes forward");
        with_ede(shell, |ede| ede.focus(id));
    }
    previews_refresh(shell);
    overlays_refresh(shell);
    // A window closed or renamed under the switcher: its tiles follow, and
    // the last one gone closes it.
    if shell.borrow().surfaces.is_open("switcher") {
        if shell.borrow().model.alt_tab.shown {
            switcher_refresh(shell);
        } else {
            switcher_close(shell, cx);
        }
    }
    model_changed(shell, cx);
}

/// Every open layer document at the size its header asks for now (the
/// output changed): resized in place, never re-opened (#EDE-65:
/// after a VM window resize the taskbar kept its old width, centred, its
/// first button floating mid-bar, and the ground and sidebar kept theirs).
fn relayout(shell: &SharedShell, cx: &mut App) {
    let docs: Vec<Document> = {
        let s = shell.borrow();
        let names: Vec<String> = s.surfaces.open_names().map(String::from).collect();
        names.iter().filter_map(|n| document(&s, n)).filter(|d| matches!(d.surface, SurfaceHeader::Layer { .. })).collect()
    };
    for doc in docs {
        let (doc, feed) = {
            let s = shell.borrow();
            (relaid(&doc, header_for(&s, &doc)), s.feed.clone())
        };
        let Some(doc) = doc else { continue };
        shell.borrow_mut().surfaces.resize_layer(&doc, &feed, cx);
    }
}

/// An open layer document as [`relayout`] resizes it to `header`, `None`
/// when it has that size already.
fn relaid(doc: &Document, header: SurfaceHeader) -> Option<Document> {
    (header != doc.surface).then(|| as_drawn(doc, header))
}

/// Mint's click on a taskbar button: launch, or toggle the group's last
/// focused window (#EDE-254; previews show on hover only).
fn taskbar_press(shell: &SharedShell, app: &str, cx: &mut App) {
    let press = uicore_model::taskbar_press(&shell.borrow().model, app);
    match press {
        TaskbarPress::Launch => launch_or_own(shell, app, cx),
        TaskbarPress::Focus { id } => {
            if let Ok(id) = ede_protocol::WindowId::new(&id) {
                with_ede(shell, |ede| ede.focus(id));
            }
        }
        TaskbarPress::Minimize { id } => {
            if let Ok(id) = ede_protocol::WindowId::new(&id) {
                with_ede(shell, |ede| minimize(ede, id));
            }
        }
    }
}

/// #EDE-254: a row of a group's menu (`uicore_model::taskbar_menu`'s values)
/// on the group's last focused window, or the whole group.
fn taskbar_menu_pick(shell: &SharedShell, app: &str, value: &str, cx: &mut App) {
    let (last, group) = {
        let s = shell.borrow();
        let last = uicore_model::last_focused(&s.model, app).map(|w| w.id.clone());
        let group: Vec<String> = s.model.windows.iter().filter(|w| w.app == app && w.workspace == s.model.workspaces.active).map(|w| w.id.clone()).collect();
        (last, group)
    };
    let id = |s: &str| ede_protocol::WindowId::new(s).ok();
    let last_id = last.as_deref().and_then(id);
    match value {
        "about" => panel_menu(shell, "history", cx),
        "configure" => panel_menu(shell, "settings", cx),
        "minimize" => {
            if let Some(w) = last_id {
                with_ede(shell, |ede| minimize(ede, w));
            }
        }
        "restore" => {
            if let Some(w) = last_id {
                with_ede(shell, |ede| ede.focus(w));
            }
        }
        "maximize" | "unmaximize" => {
            if let Some(w) = last_id {
                let action = if value == "maximize" { ede_protocol::WindowAction::Maximize } else { ede_protocol::WindowAction::Unmaximize };
                with_ede(shell, |ede| ede.window_action(w, action));
            }
        }
        "close" => {
            if let Some(w) = last_id {
                with_ede(shell, |ede| ede.close_window(w));
            }
        }
        // Cinnamon's Close all asks the app to quit: every window of the group.
        "close-all" | "close-others" => {
            for w in group.iter().filter(|w| value == "close-all" || Some(w.as_str()) != last.as_deref()).filter_map(|w| id(w.as_str())) {
                with_ede(shell, |ede| ede.close_window(w));
            }
        }
        _ => {
            if let Some(index) = value.strip_prefix("workspace:").and_then(|i| i.parse::<usize>().ok()) {
                if let Some(w) = last_id {
                    with_ede(shell, |ede| ede.move_to_workspace(w, index));
                }
            } else if let Some(index) = value.strip_prefix("monitor:").and_then(|i| i.parse::<usize>().ok()) {
                if let Some(w) = last_id {
                    move_to_output(shell, w, index);
                }
            } else if let Some(uri) = value.strip_prefix("recent:") {
                // Cinnamon opens the file with its type's default application.
                person_launch(shell, &format!("xdg-open '{}'", uri.replace('\'', "'\\''")));
            } else if let Some(action) = value.strip_prefix("action:") {
                let exec = app_entry(&shell.borrow(), app).and_then(|e| taskbar_menu::actions(&e.id).into_iter().find(|a| a.id == action)).map(|a| a.exec);
                match exec {
                    Some(exec) => person_launch(shell, &exec),
                    None => tracing::warn!(app, action, "no such desktop action"),
                }
            } else {
                tracing::warn!(value, "unknown taskbar menu entry");
            }
        }
    }
}

/// #EDE-254: Cinnamon's `move_to_monitor`: the window keeps its place
/// relative to the output's corner (pulled in to fit), a maximized one is
/// maximized again there; then it takes the focus.
fn move_to_output(shell: &SharedShell, window: ede_protocol::WindowId, index: usize) {
    let target = {
        let s = shell.borrow();
        let Some(info) = s.windows.iter().find(|w| w.id == window).cloned() else { return };
        let Some(to) = s.person_outputs.get(index).copied() else { return };
        let from = output_of(&s.person_outputs, &info).unwrap_or(to);
        let (x, y) = moved_into(from, to, (info.x, info.y, info.width, info.height));
        (x, y, info.maximized)
    };
    let (x, y, maximized) = target;
    with_ede(shell, |ede| ede.move_window(window.clone(), x, y));
    if maximized {
        with_ede(shell, |ede| ede.window_action(window.clone(), ede_protocol::WindowAction::Maximize));
    }
    with_ede(shell, |ede| ede.focus(window));
}

/// Where a window at `w` (x, y, width, height) on output `from` lands on
/// `to`: the same offset from the corner, kept on `to`.
fn moved_into(from: (i32, i32, i32, i32), to: (i32, i32, i32, i32), w: (i32, i32, i32, i32)) -> (i32, i32) {
    let dx = (w.0 - from.0).clamp(0, (to.2 - w.2).max(0));
    let dy = (w.1 - from.1).clamp(0, (to.3 - w.3).max(0));
    (to.0 + dx, to.1 + dy)
}

/// The hover previews of a group: `/previews` lists its windows with live
/// thumbnails while `app` is set.
fn previews_set(shell: &SharedShell, app: Option<String>, cx: &mut App) {
    dispatch(shell, ShellEvent::TaskbarPreviews { app }, cx);
    previews_refresh(shell);
    thumbnails_retarget(shell);
    refresh(shell, cx);
}

/// A previews Popover opened or closed: open shows its app's, closed hides
/// them unless another app's already show.
fn previews_changed(shell: &SharedShell, app: Option<String>, open: bool, cx: &mut App) {
    let current = shell.borrow().model.taskbar.previews.clone();
    if let Some(next) = previews_after(&current, app, open) {
        previews_set(shell, next, cx);
    }
}

/// The previews after a Popover's change, `None` = as they are.
fn previews_after(current: &Option<String>, app: Option<String>, open: bool) -> Option<Option<String>> {
    if open {
        Some(app)
    } else if *current == app || app.is_none() {
        Some(None)
    } else {
        None
    }
}

fn previews_refresh(shell: &SharedShell) {
    let mut s = shell.borrow_mut();
    let value = {
        let s = &*s;
        previews_feed(&s.model, &s.holder_windows, |app| app_icon(s, app), |id| s.thumbnails.get(id).cloned().unwrap_or_default())
    };
    s.feed.set("/previews", value);
}

/// The hovered app's thumbnails (Cinnamon's grouped window list): each
/// window's title and icon over its picture. #EDE-237: an Exponential page
/// window wears its page's concept icon (`concept`, from the holder), any
/// other window its app's icon.
fn previews_feed(model: &ShellState, holder: &holder_windows::HolderWindows, icon: impl Fn(&str) -> String, thumbnail: impl Fn(&str) -> String) -> Value {
    let Some(app) = model.taskbar.previews.clone() else {
        return json!({"app": null, "windows": []});
    };
    let app_icon = icon(&app);
    let windows: Vec<Value> = model
        .windows
        .iter()
        .filter(|w| w.app == app && w.workspace == model.workspaces.active)
        .map(|w| {
            let page = holder.page(&w.app, &w.title);
            json!({
                "id": w.id,
                "title": w.title,
                "focused": w.focused,
                "minimized": w.minimized,
                "thumbnail": thumbnail(&w.id),
                "icon": app_icon,
                // A hidden Icon still names a concept.
                "page": page.is_some(),
                "concept": page.map_or_else(|| "nav-apps".to_string(), |page| page.icon.clone()),
                "team": page.and_then(|page| page.team.clone()),
            })
        })
        .collect();
    json!({"app": app, "windows": windows})
}

/// #EDE-239: Super+A and the taskbar's Assistant: the Assistant is ONE
/// window of the holder (`exponential-app`), opened over the session bus on
/// the page key its feeds carry (`agent:<id>`, else `home`), coming forward
/// with an xdg-activation token of the press like a start menu page.
fn assistant_open(shell: &SharedShell, _cx: &mut App) {
    let key = assistant_key(&shell.borrow().holder_feeds);
    let token = start_menu_token(shell);
    holder_feeds::open_page(key, token);
}

/// The Assistant's page key in the holder's feeds (`home` without one).
fn assistant_key(feeds: &holder_feeds::HolderFeeds) -> String {
    feeds.feeds["assistant"]["key"].as_str().map_or_else(|| "home".to_string(), String::from)
}

fn demo() -> bool {
    std::env::var_os("EDE_SHELL_DEMO").is_some_and(|v| !v.is_empty() && v != "0")
}

/// #EDE-65: the new-issue button: the Exponential app's new issue (on the
/// current team), until issues come into eDE.
fn new_issue(shell: &SharedShell) {
    // #EDE-65 J3: a Local desktop's team id IS its desktop id, so the app opens the new issue there.
    let link = accounts::new_issue_deep_link(Some(shell.borrow().model.switcher.current.id.as_str()).filter(|id| !id.is_empty()));
    hand_off(shell, &link);
}

/// A deep link to the Exponential app: the running one, else a new one
/// started with it (its single-instance guard reads the link off argv).
fn hand_off(shell: &SharedShell, url: &str) {
    if demo() {
        tracing::info!(url, "hand-off (demo: not sent)");
        return;
    }
    if open_in_holder(shell, Some(url)) {
        tracing::info!(url, "handed to the Exponential app over the session bus");
        return;
    }
    if accounts::ide_send(url) {
        tracing::info!(url, "handed to the Exponential app");
        bring_forward(shell, &EXPONENTIAL_APPS);
        return;
    }
    bring_forward(shell, &EXPONENTIAL_APPS);
    let command = EXPONENTIAL_APPS.iter().find_map(|id| app_entry(&shell.borrow(), id).map(|app| format!("{} '{url}'", app.exec)));
    match command {
        Some(command) => person_launch(shell, &command),
        None => tracing::warn!(url, "no Exponential app to hand off to"),
    }
}

/// #EDE-227: `link` (none: a window) opens in the login's holder over the
/// session bus, with a launch token of the person's press, so the window it
/// lands in comes to the front (xdg-activation); false = no holder serves
/// the bus.
fn open_in_holder(shell: &SharedShell, link: Option<&str>) -> bool {
    use api::bus::{Reply, Request};
    if !api::bus::socket_path(&api::default_data_dir()).exists() {
        return false;
    }
    let activation_token = with_ede(shell, |ede| ede.launch_token());
    let reply = accounts::holder_call(&Request::Open { link: link.map(String::from), activation_token });
    matches!(reply, Some(Reply::Done))
}

/// Is the switcher on a Local desktop?
fn on_local(shell: &SharedShell) -> bool {
    shell.borrow().model.switcher.current.kind == uicore_model::SwitcherKind::Local
}

/// A failed action the person just took (feedback-states.json `errors`):
/// the toast `Couldn't <verb> <thing>` with the server's message, as one of
/// the shell's own notifications (the IDE's toast on this desktop).
fn failed_action(shell: &SharedShell, verb: &str, thing: &str, message: &str, cx: &mut App) {
    tracing::warn!("{verb} {thing}: {message}");
    notification_push(shell, failed_toast(verb, thing, message, now_ms()), cx);
}

/// #EDE-182: a Date & Time change through timedated, off the main loop
/// (polkit's prompt may stay open as long as the person takes); then the
/// page as the machine has it, and a refusal (a cancelled prompt, the wrong
/// password) as the failed action's toast.
fn settings_time(shell: &SharedShell, change: settings_pages::TimeChange, cx: &mut App) {
    if shell.borrow().time_changes.contains(&change) {
        return;
    }
    shell.borrow_mut().time_changes.push(change.clone());
    // The feed follows the person's flip while timedated decides, so the
    // page a refusal brings back moves the control back.
    let flipped = shell.borrow().feed.get("/pages").cloned().map(|mut pages| {
        pages["datetime"] = change.applied(pages["datetime"].take());
        pages
    });
    if let Some(pages) = flipped {
        shell.borrow_mut().feed.set("/pages", pages);
        refresh(shell, cx);
    }
    let shell = shell.clone();
    cx.spawn(async move |cx| {
        let asked = change.clone();
        let (applied, pages) = blocking::unblock(move || settings_pages::apply_time_on_system_bus(&asked)).await;
        cx.update(|cx| {
            shell.borrow_mut().time_changes.retain(|c| *c != change);
            shell.borrow_mut().feed.set("/pages", pages);
            refresh(&shell, cx);
            if let Some(toast) = time_toast(&change, &applied, now_ms()) {
                notification_push(&shell, toast, cx);
            }
        });
    })
    .detach();
}

/// [`settings_time`]'s toast: `None` when timedated took the change.
fn time_toast(change: &settings_pages::TimeChange, applied: &Result<(), String>, received_ms: i64) -> Option<dbus::Notification> {
    let message = applied.as_ref().err()?;
    tracing::warn!("datetime {change:?}: {message}");
    let (verb, thing) = change.failed();
    Some(failed_toast(verb, thing, message, received_ms))
}

/// A page's service action (#EDE-13), off the main loop: a daemon round
/// trip, and polkit may ask the person for as long as its prompt is open
/// (#EDE-182); a failure is the failed action's toast.
fn service_call(shell: &SharedShell, domain: String, action: String, args: Value, cx: &mut App) {
    let shell = shell.clone();
    cx.spawn(async move |cx| {
        let call = json!({"op": "call", "domain": domain, "action": action, "args": args});
        let result = blocking::unblock(move || control::services().and_then(|mut c| c.call(call))).await;
        match result {
            Ok(v) => tracing::info!(%domain, %action, result = %v, "service"),
            Err(e) => {
                let (verb, thing) = settings_pages::service_failed(&domain, &action, &args);
                cx.update(|cx| failed_action(&shell, &verb, &thing, &e, cx));
            }
        }
    })
    .detach();
}

/// The start menu's power rows through logind, off the main loop (#EDE-182:
/// polkit may ask); a refusal toasts as the Power page's does.
fn power(shell: &SharedShell, action: String, cx: &mut App) {
    let shell = shell.clone();
    cx.spawn(async move |cx| {
        let asked = action.clone();
        if let Err(e) = blocking::unblock(move || system::power(&asked)).await {
            let (verb, thing) = settings_pages::service_failed("power", &action, &json!({}));
            cx.update(|cx| failed_action(&shell, &verb, &thing, &e, cx));
        }
    })
    .detach();
}

/// A toast of the shell's own (`summary`, `body` may be empty).
fn toast(shell: &SharedShell, summary: &str, body: &str, cx: &mut App) {
    tracing::warn!("{summary} {body}");
    shell.borrow_mut().notifications.push(own_toast(summary.to_string(), body.to_string(), now_ms()));
    notifications_changed(shell, cx);
}

/// The shell's own ids count down from the top, clear of the bus's.
static OWN_NOTIFICATION: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::MAX);

/// [`failed_action`]'s toast.
fn failed_toast(verb: &str, thing: &str, message: &str, received_ms: i64) -> dbus::Notification {
    own_toast(domain::action_error::title(verb, thing), domain::action_error::description_of(message).unwrap_or_default(), received_ms)
}

fn own_toast(summary: String, body: String, received_ms: i64) -> dbus::Notification {
    dbus::Notification {
        id: OWN_NOTIFICATION.fetch_sub(1, std::sync::atomic::Ordering::SeqCst),
        app: "Exponential".into(),
        summary,
        body,
        icon: String::new(),
        actions: Vec::new(),
        resident: false,
        urgency: 1,
        expires_at: Some(Instant::now() + INBOX_TOAST),
        at: clock::time(),
        received_ms,
    }
}

/// The start menu: the popup document `start-menu` (or the older `menu`),
/// with a fresh search every time.
fn start_menu_toggle(shell: &SharedShell, cx: &mut App) {
    let name = if document(&shell.borrow(), "start-menu").is_some() { "start-menu" } else { "menu" };
    let was_open = shell.borrow().surfaces.is_open(name);
    if was_open {
        close(shell, name, cx);
        dispatch(shell, ShellEvent::StartMenuClose, cx);
        return;
    }
    let started = std::time::Instant::now();
    {
        let mut s = shell.borrow_mut();
        let filtered = apps::filter(&s.apps, "");
        s.feed.set("/query", json!(""));
        s.feed.set("/apps", Value::Array(filtered));
    }
    dispatch(shell, ShellEvent::StartMenuOpen, cx);
    issues_reindex(shell, cx);
    toggle_popup(shell, name, cx);
    refresh(shell, cx);
    shell.borrow().surfaces.focus_component(name, "search", cx);
    // #EDE-258: the menu is built from the feeds it already has (< 100 ms).
    tracing::info!(ms = started.elapsed().as_secs_f64() * 1000.0, "start menu opened");
}

/// #EDE-92, #EDE-258: an arrow pressed in `window`: in the start menu it
/// moves the keyboard's place (a category, a row, the column) and the key
/// is the menu's; anywhere else, or when the menu has nothing for it (Left
/// and Right while the box has text: its caret), it is not.
fn start_menu_step(shell: &SharedShell, window: gpui::AnyWindowHandle, key: start_menu::Key, cx: &mut App) -> bool {
    let moved = {
        let Ok(mut s) = shell.try_borrow_mut() else { return false };
        let Some(name) = ["start-menu", "menu"].into_iter().find(|name| s.surfaces.region(name).is_some_and(|r| gpui::AnyWindowHandle::from(r.handle) == window)) else { return false };
        let Some(nav) = s.feed.get("/shell").map(|shell| shell["startMenu"]["nav"].clone()) else { return false };
        let query = s.model.start_menu.query.clone();
        let Some(moved) = s.menu_selection.key(&nav, &query, key) else { return false };
        (name, moved)
    };
    // The key is still being dispatched in that window: its view redraws after it.
    let shell = shell.clone();
    cx.defer(move |cx| {
        let (name, moved) = moved;
        match moved {
            start_menu::Moved::Category(category) => {
                dispatch(&shell, ShellEvent::StartMenuCategory { category }, cx);
                shell.borrow().surfaces.reveal_top(name, "rows_scroll", cx);
                let at = shell.borrow().feed.get("/shell").and_then(|v| v["startMenu"]["nav"]["categories"].as_array().and_then(|c| c.iter().position(|c| c["selected"] == true)));
                if let Some(at) = at {
                    shell.borrow().surfaces.reveal_row(name, "categories_scroll", "categories", at, cx);
                }
            }
            start_menu::Moved::Row(row) => {
                model_changed(&shell, cx);
                match row {
                    // #EDE-133: the first row = the results' top, its group heading included.
                    Some(0) => {
                        shell.borrow().surfaces.reveal_top(name, "rows_scroll", cx);
                    }
                    Some(row) => {
                        shell.borrow().surfaces.reveal_row(name, "rows_scroll", "rows", row, cx);
                    }
                    None => {}
                }
            }
        }
    });
    true
}

/// #EDE-258: Enter in the search box: the selected row opens; on a
/// category its page opens (Home, Chats, the Assistant…), or the keyboard
/// goes into its contents when it has none (a team, Devices).
fn start_menu_activate(shell: &SharedShell, cx: &mut App) -> Option<Value> {
    let (activation, nav) = {
        let s = shell.borrow();
        let nav = s.feed.get("/shell").map(|v| v["startMenu"]["nav"].clone())?;
        (s.menu_selection.activation(&nav, &s.model.start_menu.query), nav)
    };
    match activation? {
        start_menu::Activation::Target(target) => Some(target),
        start_menu::Activation::Category(category) => {
            let key = shell.borrow().holder_feeds.tree().iter().find(|node| node["id"] == category.as_str()).and_then(|node| node["key"].as_str().map(String::from));
            if key.is_none() && shell.borrow_mut().menu_selection.enter_contents(&nav) {
                model_changed(shell, cx);
            }
            key.map(|key| json!({"page": key}))
        }
    }
}

impl Host {
    /// #EDE-258: a row's target: an app launches as a press on it in the
    /// menu did (`ede_launch`); the rest is [`start_menu_open`].
    fn start_menu_open(&self, target: Value, event: &FunctionCallEvent, cx: &mut App) {
        if let Some(app) = target["app"].as_str() {
            self.on_function_call(&FunctionCallEvent { name: "ede_launch".into(), args: json!({"app": app}), ..event.clone() }, cx);
            close(&self.shell, "start-menu", cx);
            dispatch(&self.shell, ShellEvent::StartMenuClose, cx);
            return;
        }
        start_menu_open(&self.shell, target, cx);
    }
}

/// #EDE-258: a row of the menu opens, and the menu closes: a page as its own
/// window through the holder's focus-or-open, an app, a Settings module, an
/// issue, a Local desktop's board.
fn start_menu_open(shell: &SharedShell, target: Value, cx: &mut App) {
    let text = |key: &str| target[key].as_str().map(String::from);
    if let Some(page) = text("settings") {
        settings_open_page(shell, &page, cx);
    } else if let Some(key) = text("page") {
        close(shell, "start-menu", cx);
        dispatch(shell, ShellEvent::StartMenuClose, cx);
        let token = start_menu_token(shell);
        // #EDE-258: a Local desktop's holder goes back to the account first.
        holder_back_from_local(shell, cx);
        holder_feeds::open_page(key, token);
    } else if let Some(identifier) = text("issue") {
        let source = issues::issue_source(&json!({"source": target["source"], "sourceId": target["sourceId"]}));
        close(shell, "start-menu", cx);
        dispatch(shell, ShellEvent::StartMenuClose, cx);
        let current = shell.borrow().model.switcher.current.clone();
        if let Some((kind, id)) = source.filter(|(kind, id)| !(current.kind == *kind && current.id == *id)) {
            switcher_picked(shell, kind, &id, cx);
        }
        ide_open(shell, Some(&issues::issue_deep_link(&identifier)));
    } else if let Some(desktop) = text("local") {
        close(shell, "start-menu", cx);
        dispatch(shell, ShellEvent::StartMenuClose, cx);
        let current = shell.borrow().model.switcher.current.clone();
        if !(current.kind == uicore_model::SwitcherKind::Local && current.id == desktop) {
            switcher_picked(shell, uicore_model::SwitcherKind::Local, &desktop, cx);
        }
        let token = start_menu_token(shell);
        match text("board") {
            Some(board) => holder_feeds::open_page(format!("board:{board}"), token),
            None => ide_open(shell, None),
        }
    }
}

/// The holder on a Local desktop goes back to the person's account (the
/// tree's pages are the account's).
fn holder_back_from_local(shell: &SharedShell, cx: &mut App) {
    let (on_local, account_team) = {
        let s = shell.borrow();
        let team = s.model.switcher.entries.iter().find(|e| e.kind == uicore_model::SwitcherKind::Team).map(|e| e.id.clone());
        (s.model.switcher.current.kind == uicore_model::SwitcherKind::Local, team)
    };
    if let (true, Some(team)) = (on_local, account_team) {
        switcher_picked(shell, uicore_model::SwitcherKind::Team, &team, cx);
    }
}

/// An xdg-activation token for the window a menu press opens.
fn start_menu_token(shell: &SharedShell) -> Option<String> {
    with_ede(shell, |ede| ede.launch_token())
}

/// #EDE-65: the start menu's query: the apps at once, the issues once the
/// keys rest (the debounce) and ranked off the main loop.
const ISSUE_SEARCH_DEBOUNCE: Duration = Duration::from_millis(120);

fn start_menu_query(shell: &SharedShell, query: String, cx: &mut App) {
    let changed = shell.borrow().model.start_menu.query != query;
    dispatch(shell, ShellEvent::StartMenuQuery { query }, cx);
    // #EDE-92: new text = the results start at their top, on the first row
    // (Enter's flush of the same text keeps the place).
    if changed {
        for name in ["start-menu", "menu"] {
            shell.borrow().surfaces.reveal_top(name, "rows_scroll", cx);
        }
    }
    issues_search(shell, cx);
}

fn issues_search(shell: &SharedShell, cx: &mut App) {
    let generation = {
        let mut s = shell.borrow_mut();
        s.issue_search += 1;
        if s.model.start_menu.query.trim().is_empty() {
            s.issue_hits.clear();
            return;
        }
        s.issue_search
    };
    let shell = shell.clone();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(ISSUE_SEARCH_DEBOUNCE).await;
        let Some((index, query)) = ({
            let s = shell.borrow();
            (s.issue_search == generation).then(|| (s.issue_index.clone(), s.model.start_menu.query.clone()))
        }) else {
            return;
        };
        // #EDE-258: the teams' issues are the holder's (its one store), the
        // Local desktops' the shell's own; team hits first, as the engine
        // ranks within each.
        let hits = cx
            .background_executor()
            .spawn(async move {
                let mut hits = holder_feeds::search(&query);
                hits.extend(issues::search(&index, &query));
                hits.truncate(uicore_model::START_MENU_ISSUE_LIMIT);
                hits
            })
            .await;
        if shell.borrow().issue_search != generation {
            return;
        }
        shell.borrow_mut().issue_hits = hits;
        cx.update(|cx| model_changed(&shell, cx));
    })
    .detach();
}

/// #EDE-65: the issue pool, read again each time the menu opens (the
/// stores' SQLite off the main loop); a query typed meanwhile ranks after.
fn issues_reindex(shell: &SharedShell, cx: &mut App) {
    let sources = shell.borrow().workspaces.issue_sources();
    let shell = shell.clone();
    cx.spawn(async move |cx| {
        let index = match sources {
            Some(sources) => cx.background_executor().spawn(async move { issues::build(sources) }).await,
            None => issues::demo_index(),
        };
        tracing::debug!(issues = index.len(), "start menu issue pool");
        shell.borrow_mut().issue_index = Arc::new(index);
        let desktops = shell.borrow().workspaces.local_desktops();
        if !shell.borrow().workspaces.demo() {
            let local = cx.background_executor().spawn(async move { issues::local_tree(desktops) }).await;
            shell.borrow_mut().local_tree = local;
        }
        cx.update(|cx| model_changed(&shell, cx));
        cx.update(|cx| issues_search(&shell, cx));
    })
    .detach();
}

/// Opens an on-demand layer document by name, if it is not open.
fn open_on_demand(shell: &SharedShell, name: &str, cx: &mut App) {
    if shell.borrow().surfaces.is_open(name) {
        return;
    }
    let doc = document(&shell.borrow(), name);
    match doc {
        Some(doc) if matches!(doc.surface, SurfaceHeader::Layer { .. } | SurfaceHeader::Window { .. }) => open_layer(shell, &doc, cx),
        _ => tracing::warn!(document = name, "no such layer document"),
    }
}

/// The popup documents a press outside them closes (`popups.dismiss`):
/// those open when it was pressed (`pressed_at`, the event's stamp). One the
/// press itself opened (the start menu from its button) stays: its press
/// reaches the shell before the event does, or after (#EDE-89).
/// The popup documents open now (the tray applets, the menus).
fn open_popups(s: &Shell) -> Vec<String> {
    let ui = s.ui.lock().unwrap_or_else(|p| p.into_inner());
    s.surfaces.open_names().filter(|n| ui.document(n).is_some_and(|d| matches!(d.surface, SurfaceHeader::Popup { .. }))).map(String::from).collect()
}

fn popups_to_dismiss(open: &[(String, Option<i64>)], pressed_at: i64) -> Vec<String> {
    open.iter().filter(|(_, opened)| opened.is_none_or(|at| at < pressed_at)).map(|(name, _)| name.clone()).collect()
}

/// A key the compositor kept for the shell (#EDE-12), at the compositor's
/// stamp `at` (epoch ms).
fn hotkey(shell: &SharedShell, name: &str, at: i64, cx: &mut App) {
    tracing::info!(name, "hotkey");
    match name {
        "switcher.next" => switcher_step(shell, ShellEvent::AltTabNext, cx),
        "switcher.prev" => switcher_step(shell, ShellEvent::AltTabPrev, cx),
        "switcher.next-app" => switcher_step(shell, ShellEvent::AltTabNextApp, cx),
        "switcher.prev-app" => switcher_step(shell, ShellEvent::AltTabPrevApp, cx),
        "switcher.commit" => switcher_commit(shell, None, cx),
        "switcher.cancel" => switcher_close(shell, cx),
        "menu.toggle" => start_menu_toggle(shell, cx),
        "agent.toggle" => assistant_open(shell, cx),
        // #EDE-243: Cinnamon's notifications applet keys and the grouped window list's Super+1…9.
        "notifications.open" => toggle_popup(shell, "notification-center", cx),
        "notifications.clear" => notifications_clear(shell, cx),
        n if n.starts_with("taskbar.activate.") || n.starts_with("taskbar.launch.") => {
            if let Some(i) = n.rsplit('.').next().and_then(|d| d.parse::<usize>().ok()).filter(|i| (1..=9).contains(i)) {
                taskbar_hotkey(shell, i - 1, n.starts_with("taskbar.launch."), cx);
            }
        }
        "expo.toggle" => overlay_toggle(shell, "expo", cx),
        "scale.toggle" => overlay_toggle(shell, "scale", cx),
        // #EDE-263: Escape (the compositor keeps it while one is open) closes Expo and Scale, as on Cinnamon.
        "overlay.close" => {
            for name in ["expo", "scale"] {
                if shell.borrow().surfaces.is_open(name) {
                    overlay_close(shell, name, cx);
                }
            }
        }
        "volume.up" | "volume.down" => osd_show(shell, system::volume_step(name == "volume.up"), cx),
        "volume.mute" => osd_show(shell, system::volume_toggle_mute(), cx),
        "mic.mute" => osd_show(shell, system::mic_toggle_mute(), cx),
        "brightness.up" | "brightness.down" => osd_show(shell, system::brightness_step(name == "brightness.up"), cx),
        "media.play" | "media.next" | "media.prev" => system::media(name),
        "terminal" => match system::terminal_command() {
            Some(t) => person_launch(shell, t),
            None => tracing::warn!("no terminal emulator installed"),
        },
        name if screenshot::from_hotkey(name).is_some() => {
            let (what, clip) = screenshot::from_hotkey(name).unwrap_or((screenshot::What::Screen, false));
            screenshot_key(shell, what, clip, cx);
        }
        "popups.dismiss" => {
            let (names, menu_stays) = {
                let s = shell.borrow();
                let ui = s.ui.lock().unwrap_or_else(|p| p.into_inner());
                let open: Vec<(String, Option<i64>)> = s.surfaces.open_names().filter(|n| ui.document(n).is_some_and(|d| matches!(d.surface, SurfaceHeader::Popup { .. }))).map(|n| (n.to_string(), s.popups_opened.get(n).copied())).collect();
                let names = popups_to_dismiss(&open, at);
                let menu_stays = open.iter().any(|(n, _)| (n == "start-menu" || n == "menu") && !names.contains(n));
                (names, menu_stays)
            };
            for n in &names {
                close(shell, n, cx);
            }
            // #EDE-75: a closing Popover's change reads the shell: the borrow ends first.
            let hosted = shell.borrow().surfaces.hosted_layers();
            uicore_gpui::dismiss_hosted(hosted, cx);
            let (previews, menu_open) = {
                let s = shell.borrow();
                (s.model.taskbar.previews.is_some(), s.model.start_menu.open)
            };
            if !names.is_empty() || previews || menu_open {
                if !menu_stays {
                    dispatch(shell, ShellEvent::StartMenuClose, cx);
                }
                previews_set(shell, None, cx);
            }
        }
        other => tracing::info!(name = other, "hotkey without a handler"),
    }
}

/// Clear all: every notification closed and every inbox entry the bell
/// lists marked read (Shift+Super+C, #EDE-243, and the bell's button).
fn notifications_clear(shell: &SharedShell, cx: &mut App) {
    let ids: Vec<u32> = shell.borrow().notifications.list.iter().map(|n| n.id).collect();
    for id in ids {
        notification_close(shell, id, 2, cx);
    }
    // #EDE-65: and every inbox entry it lists is marked read.
    let keys: Vec<String> = shell.borrow().notifications.inbox.iter().map(|i| i.key.clone()).collect();
    for key in keys {
        inbox_mark_read(shell, &key, cx);
    }
    close(shell, "notification-center", cx);
}

/// #EDE-243, Cinnamon's grouped window list's Super+1…9: the nth button
/// (pinned apps first): no window launches it, else its last focused window
/// comes forward, and a press while it is focused cycles the group; with
/// Shift a new instance.
fn taskbar_hotkey(shell: &SharedShell, index: usize, launch: bool, cx: &mut App) {
    let (app, next) = {
        let s = shell.borrow();
        let Some(button) = uicore_model::taskbar_buttons(&s.model).into_iter().nth(index) else { return };
        let next = if launch || button.windows.is_empty() {
            None
        } else if button.focused {
            match uicore_model::taskbar_scroll(&s.model, &button.app, uicore_model::ScrollDirection::Down) {
                Some(TaskbarPress::Focus { id }) => Some(id),
                _ => None,
            }
        } else {
            s.model.mru.iter().find(|id| button.windows.contains(id)).cloned().or_else(|| button.windows.first().cloned())
        };
        (button.app, next)
    };
    match next.and_then(|id| ede_protocol::WindowId::new(&id).ok()) {
        Some(id) => {
            with_ede(shell, |ede| ede.focus(id));
        }
        None => launch_or_own(shell, &app, cx),
    }
}

/// Expo / Scale (#EDE-49): an on-demand overlay document with every
/// window's thumbnail, toggled by its hotkey.
fn overlay_toggle(shell: &SharedShell, name: &str, cx: &mut App) {
    if shell.borrow().surfaces.is_open(name) {
        overlay_close(shell, name, cx);
        return;
    }
    if document(&shell.borrow(), name).is_none() {
        tracing::info!(document = name, "no overlay document");
        return;
    }
    overlays_refresh(shell);
    open_on_demand(shell, name, cx);
    thumbnails_retarget(shell);
    refresh(shell, cx);
}

fn overlay_close(shell: &SharedShell, name: &str, cx: &mut App) {
    close(shell, name, cx);
    thumbnails_retarget(shell);
}

/// Cinnamon's `alttab-switcher-delay`: a quick Alt+Tab flips windows
/// without the popup ever showing.
const SWITCHER_DELAY: Duration = Duration::from_millis(100);

/// Alt+Tab / Alt+`: the first step opens Alt-Tab and arms the delay, the
/// next ones move the selection.
fn switcher_step(shell: &SharedShell, event: ShellEvent, cx: &mut App) {
    let fresh = !shell.borrow().model.alt_tab.open;
    dispatch(shell, event, cx);
    if !shell.borrow().model.alt_tab.open {
        return;
    }
    if !fresh {
        if shell.borrow().model.alt_tab.shown {
            switcher_refresh(shell);
            refresh(shell, cx);
        }
        return;
    }
    let generation = {
        let mut s = shell.borrow_mut();
        s.switcher_generation += 1;
        s.switcher_generation
    };
    let s = shell.clone();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(SWITCHER_DELAY).await;
        cx.update(|cx| {
            let current = s.borrow().switcher_generation == generation && s.borrow().model.alt_tab.open;
            if current {
                switcher_show(&s, cx);
            }
        });
    })
    .detach();
}

/// The delay elapsed with Alt held: the popup, sized to its windows, centred.
fn switcher_show(shell: &SharedShell, cx: &mut App) {
    dispatch(shell, ShellEvent::AltTabShow, cx);
    if !shell.borrow().model.alt_tab.shown {
        return;
    }
    switcher_refresh(shell);
    open_on_demand(shell, "switcher", cx);
    thumbnails_retarget(shell);
    refresh(shell, cx);
}

/// `/switcher`: the Alt-Tab order with the selection, the app icons, the
/// thumbnails and the selected window's title (the line under the tiles).
fn switcher_refresh(shell: &SharedShell) {
    let value = {
        let s = shell.borrow();
        switcher_feed(&s.model, |app| app_icon(&s, app), |id| s.thumbnails.get(id).cloned().unwrap_or_default())
    };
    shell.borrow_mut().feed.set("/switcher", value);
}

fn switcher_feed(model: &ShellState, icon: impl Fn(&str) -> String, thumbnail: impl Fn(&str) -> String) -> Value {
    let order = uicore_model::alt_tab_order(model);
    let selected = model.alt_tab.selected;
    let mut title = String::new();
    let windows: Vec<Value> = order
        .iter()
        .enumerate()
        .filter_map(|(i, id)| {
            let w = model.windows.iter().find(|w| &w.id == id)?;
            if i == selected {
                title = w.title.clone();
            }
            Some(json!({"id": id, "title": w.title, "app": w.app, "icon": icon(&w.app), "minimized": w.minimized, "selected": i == selected, "thumbnail": thumbnail(id)}))
        })
        .collect();
    json!({"windows": windows, "title": title})
}

/// The switcher's width for `n` tiles: Mint's popup hugs its entries,
/// capped at the output. Tile 208 px (`tile` in switcher.json: a 192 px
/// thumbnail and `$spacing.sm` around it), `sm` gaps, `lg` padding.
fn switcher_width(n: usize, output_width: u32) -> u32 {
    const TILE: u32 = 208;
    const GAP: u32 = 8;
    const PADDING: u32 = 16;
    let n = n.max(1) as u32;
    (2 * PADDING + n * TILE + (n - 1) * GAP).min(output_width.saturating_sub(2 * PADDING)).max(TILE + 2 * PADDING)
}

/// The windows the capture loop photographs: the switcher's while it is
/// open, the previews' while they show, every window while Expo or Scale
/// is up; nothing otherwise.
fn thumbnail_windows(s: &Shell) -> Vec<String> {
    if s.surfaces.is_open("expo") || s.surfaces.is_open("scale") {
        s.model.windows.iter().map(|w| w.id.clone()).collect()
    } else if s.model.alt_tab.shown {
        uicore_model::alt_tab_order(&s.model)
    } else if let Some(app) = s.model.taskbar.previews.as_deref() {
        s.model.windows.iter().filter(|w| w.app == app).map(|w| w.id.clone()).collect()
    } else {
        Vec::new()
    }
}

/// #EXP-2024: the first display as the live view's box reads it (`/outputs`).
fn first_display(s: &Shell) -> uicore_model::spaces::Display {
    let (width, height) = output_size(s);
    let scale = s.feed.get("/outputs").and_then(Value::as_array).and_then(|o| o.first()).and_then(|o| o["scale"].as_f64()).filter(|v| *v > 0.0).unwrap_or(1.0);
    uicore_model::spaces::Display { width, height, scale }
}

/// #EXP-2024: the taskbar's Agent spaces list is open (its hosted Popover).
fn spaces_list_open(s: &Shell) -> bool {
    s.surfaces.hosted_layers().iter().any(|(_, owner)| owner == spaces::LIST)
}

/// #EXP-2024: why a space's picture cannot be seen now: the lock; a
/// fullscreen window over the first display; Settings opened
/// after the view (a later top layer lies above an earlier one).
fn spaces_pause(s: &Shell) -> spaces::Pause {
    let opened = |name: &str| s.surfaces.is_open(name).then(|| s.layers_opened.get(name).copied().unwrap_or(0));
    let view = opened(spaces::VIEW).unwrap_or(u64::MAX);
    // #EDE-273: Settings is a normal window now, under every layer: nothing covers the view.
    let view_covered = false;
    let _ = (opened(spaces::VIEW), view);
    spaces::Pause { locked: s.locked, fullscreen: spaces::fullscreen_over(&s.windows, s.model.workspaces.active, s.first_output), view_covered }
}

/// What THE capture loop photographs, derived from what is open now (the
/// switcher, the previews, Expo; the Agent spaces list, the pinned view),
/// nothing while locked; the loop wakes only when it moved.
fn thumbnails_retarget(shell: &SharedShell) {
    let s = shell.borrow();
    let plan = spaces::plan(thumbnail_windows(&s), &s.spaces, s.surfaces.is_open(spaces::VIEW), spaces_list_open(&s), s.surfaces.is_open(spaces::TRAY), spaces_pause(&s), first_display(&s));
    s.captures.set_plan(plan);
}

/// #EXP-2024: what the capture loop said of the spaces, into the shell.
fn capture_out(shell: &SharedShell, out: captures::Out, cx: &mut App) {
    match out {
        captures::Out::Window { id, url } => thumbnail_arrived(shell, id, url),
        captures::Out::Spaces(list) => {
            let new_session = {
                let mut s = shell.borrow_mut();
                let known: HashSet<String> = s.spaces.sessions.iter().map(|x| x.id.clone()).collect();
                let new = list.iter().any(|x| !known.contains(x.session_id.as_str()));
                s.spaces.listed(list);
                new
            };
            // A space of a session the titles have not read yet: read them.
            if new_session {
                exp_tray_refresh(shell, cx);
            }
            spaces_changed(shell, cx);
        }
        captures::Out::Picture { space, view, url } => {
            shell.borrow_mut().spaces.picture(&space, view, url);
            spaces_feed(shell);
        }
        captures::Out::Failing { space, view } => {
            shell.borrow_mut().spaces.picture(&space, view, String::new());
            spaces_feed(shell);
        }
        captures::Out::Parked(list) => {
            let new_session = {
                let mut s = shell.borrow_mut();
                let known: HashSet<String> = s.spaces.sessions.iter().map(|x| x.id.clone()).collect();
                let new = list.iter().any(|x| !known.contains(x));
                s.spaces.parked(list);
                new
            };
            if new_session {
                exp_tray_refresh(shell, cx);
            }
            spaces_feed(shell);
        }
        captures::Out::Gone { space } => {
            shell.borrow_mut().spaces.gone(&space);
            shell.borrow().captures.ask(captures::Ask::List);
            spaces_changed(shell, cx);
        }
    }
}

/// `/spaces`, `/spaceView` and `/cuTray` from what the shell knows now.
fn spaces_feed(shell: &SharedShell) {
    let mut s = shell.borrow_mut();
    let display = first_display(&s);
    let (list, view, tray) = (s.spaces.feed(), s.spaces.view_feed(display), cu_tray_feed(&s));
    s.feed.set("/spaces", list);
    s.feed.set("/spaceView", view);
    s.feed.set("/cuTray", tray);
}

/// #EDE-266: the computer-use tray over the holder's `sessions` feed (each
/// chat's working sessions and their steps).
fn cu_tray_feed(s: &Shell) -> Value {
    let sessions: Vec<uicore_model::spaces::TraySession> =
        s.holder_feeds.feeds["sessions"].as_array().into_iter().flatten().filter_map(|e| serde_json::from_value(e.clone()).ok()).collect();
    let sessions = if s.spaces.demo && sessions.is_empty() { spaces::demo_tray_sessions() } else { sessions };
    s.spaces.tray_feed(&sessions, s.surfaces.is_open(spaces::VIEW))
}

/// #EDE-266: a press in the computer-use tray, on the space it names (else
/// the tray's selected one).
fn cu_press(shell: &SharedShell, press: spaces::TrayPress, space: Option<String>, cx: &mut App) {
    let target = {
        let s = shell.borrow();
        let id = space.clone().or_else(|| s.spaces.tray_space().map(|sp| sp.id.to_string()));
        id.and_then(|id| s.spaces.list.iter().find(|sp| sp.id.as_str() == id).map(|sp| (id, sp.session_id.to_string())))
    };
    match press {
        spaces::TrayPress::Select(id) => shell.borrow_mut().spaces.selected = Some(id),
        spaces::TrayPress::Watch => {
            if let Some((_, session)) = target {
                shell.borrow_mut().spaces.pin(Some(&session), None);
            }
        }
        spaces::TrayPress::Unwatch => shell.borrow_mut().spaces.pin(None, None),
        // #EDE-272: Stop parks the session's computer use first (its next
        // call is refused "stopped by user" instead of making a new space),
        // then ends the space.
        spaces::TrayPress::Stop => {
            if let Some((id, session)) = target {
                let s = shell.borrow();
                s.captures.ask(captures::Ask::Park { session, parked: true });
                s.captures.ask(captures::Ask::Close(id));
            }
        }
        spaces::TrayPress::Allow(session) => {
            shell.borrow().captures.ask(captures::Ask::Park { session, parked: false });
        }
        spaces::TrayPress::Control(control) => {
            let Some((id, session)) = target else { return };
            // Take over drives through the pinned view: pin it first.
            if control == uicore_model::spaces::SpaceControl::Person {
                shell.borrow_mut().spaces.pin(Some(&session), None);
            }
            shell.borrow_mut().spaces.asked.insert(id, control);
        }
    }
    spaces_changed(shell, cx);
}

/// #EDE-266: what the compositor is told of who drives each space: a space
/// the person drives gets its pinned view's picture rect (again when the
/// view grows or shrinks); one whose view went (closed, pinned elsewhere)
/// is paused, never handed back unasked; the rest as the tray asked.
fn cu_control_sync(shell: &SharedShell) {
    let asks = {
        let mut s = shell.borrow_mut();
        let display = first_display(&s);
        let view_open = s.surfaces.is_open(spaces::VIEW);
        let viewed = s.spaces.view(display).filter(|_| view_open).map(|(space, view)| (space.id.to_string(), view));
        // spaces.json `constants`: the view's border and header over its picture.
        let (border, header) = (1, 40);
        let mut asks = Vec::new();
        for (space, control) in s.spaces.controls() {
            let (control, view) = match control {
                uicore_model::spaces::SpaceControl::Person => match viewed.as_ref().filter(|(id, _)| *id == space) {
                    Some((_, view)) => {
                        let rect = ede_protocol::SpaceViewRect { namespace: ede_protocol::SPACE_VIEW_NAMESPACE.into(), x: border, y: border + header, width: view.picture.width as i32, height: view.picture.height as i32 };
                        (control, Some(rect))
                    }
                    // The view went (closed, pinned elsewhere): the agent waits.
                    None => (uicore_model::spaces::SpaceControl::Paused, None),
                },
                other => (other, None),
            };
            let wanted = (control, view.clone());
            // What the compositor already says needs no ask.
            let listed = s.spaces.list.iter().find(|sp| sp.id.as_str() == space).map(|sp| spaces::control_of(sp.control));
            if !s.spaces.sent.contains_key(&space) && view.is_none() && listed == Some(control) {
                s.spaces.sent.insert(space, wanted);
                continue;
            }
            if s.spaces.sent.get(&space) != Some(&wanted) {
                s.spaces.sent.insert(space.clone(), wanted);
                if control != uicore_model::spaces::SpaceControl::Person || view.is_some() {
                    s.spaces.asked.insert(space.clone(), control);
                }
                asks.push(captures::Ask::Control { space, control: spaces::wire_control(control), view });
            }
        }
        asks
    };
    for ask in asks {
        shell.borrow().captures.ask(ask);
    }
}

/// The spaces, the titles or the pin moved: the feeds, the pinned view's
/// layer (opened, resized, closed), the list closed with the last space,
/// the capture targets.
fn spaces_changed(shell: &SharedShell, cx: &mut App) {
    spaces_feed(shell);
    let (wanted, open, none) = {
        let s = shell.borrow();
        (s.spaces.view_wanted(), s.surfaces.is_open(spaces::VIEW), s.spaces.list.is_empty())
    };
    let size = {
        let s = shell.borrow();
        s.spaces.view(first_display(&s)).map(|(_, v)| (v.width, v.height))
    };
    match (wanted, open) {
        (true, false) => {
            open_on_demand(shell, spaces::VIEW, cx);
            shell.borrow_mut().spaces.shown_size = size;
        }
        (false, true) => {
            close(shell, spaces::VIEW, cx);
            shell.borrow_mut().spaces.shown_size = None;
        }
        // The size button, another space of another size: resized in place.
        (true, true) if size.is_some() && size != shell.borrow().spaces.shown_size => {
            let (doc, feed) = {
                let s = shell.borrow();
                (document(&s, spaces::VIEW).map(|d| drawn_now(&s, &d)), s.feed.clone())
            };
            if let Some(doc) = doc {
                shell.borrow_mut().surfaces.resize_layer(&doc, &feed, cx);
            }
            shell.borrow_mut().spaces.shown_size = size;
        }
        _ => {}
    }
    cu_control_sync(shell);
    spaces_feed(shell);
    // The last space ended: the list (and #EDE-266's tray popup) closes with its button.
    if none && shell.borrow().surfaces.is_open(spaces::TRAY) {
        close(shell, spaces::TRAY, cx);
    }
    if none {
        let hosted: Vec<_> = shell.borrow().surfaces.hosted_layers().into_iter().filter(|(_, owner)| owner == spaces::LIST).collect();
        uicore_gpui::dismiss_hosted(hosted, cx);
    }
    thumbnails_retarget(shell);
    refresh(shell, cx);
}

/// A thumbnail arrived: the documents that show it.
fn thumbnail_arrived(shell: &SharedShell, id: String, url: String) {
    shell.borrow_mut().thumbnails.insert(id, url);
    switcher_refresh(shell);
    previews_refresh(shell);
    overlays_refresh(shell);
    let thumbs: Value = {
        let s = shell.borrow();
        json!(s.thumbnails)
    };
    shell.borrow_mut().feed.set("/thumbnails", thumbs);
}

/// Alt released (or a tile clicked): the selected window gets focus.
fn switcher_commit(shell: &SharedShell, pick: Option<ede_protocol::WindowId>, cx: &mut App) {
    let target = pick.or_else(|| {
        let s = shell.borrow();
        if !s.model.alt_tab.open {
            return None;
        }
        uicore_model::alt_tab_order(&s.model).get(s.model.alt_tab.selected).and_then(|id| ede_protocol::WindowId::new(id).ok())
    });
    if let Some(id) = target {
        with_ede(shell, |ede| ede.focus(id));
    }
    dispatch(shell, ShellEvent::AltTabCommit, cx);
    switcher_close(shell, cx);
}

/// The hovered tile's `altTab.select`: its index in the Alt-Tab order
/// (closed, or a window not listed: none).
fn alt_tab_hover(model: &ShellState, window: &str) -> Option<ShellEvent> {
    if !model.alt_tab.open {
        return None;
    }
    let index = uicore_model::alt_tab_order(model).iter().position(|id| id == window)?;
    (index != model.alt_tab.selected).then_some(ShellEvent::AltTabSelect { index: index as i64 })
}

/// #EDE-65: a hovered tile becomes the selection; the tiles and the title follow.
fn switcher_hover(shell: &SharedShell, window: &str, cx: &mut App) {
    let Some(event) = alt_tab_hover(&shell.borrow().model, window) else { return };
    dispatch(shell, event, cx);
    switcher_refresh(shell);
    refresh(shell, cx);
}

fn switcher_close(shell: &SharedShell, cx: &mut App) {
    dispatch(shell, ShellEvent::AltTabCancel, cx);
    shell.borrow_mut().feed.set("/switcher", json!({"windows": [], "title": ""}));
    close(shell, "switcher", cx);
    thumbnails_retarget(shell);
}

/// The OSD with a level, for 1.5 s.
fn osd_show(shell: &SharedShell, level: Option<system::Level>, cx: &mut App) {
    let Some(level) = level else { return };
    {
        let mut s = shell.borrow_mut();
        s.feed.set("/osd", json!({"icon": level.icon, "level": level.percent.min(100), "label": level.label()}));
        s.osd_until = Some(Instant::now() + Duration::from_millis(1500));
    }
    open_on_demand(shell, "osd", cx);
    refresh(shell, cx);
}

/// A screenshot key (Cinnamon's six): off the main loop, since the
/// area waits for the person's pick and a 7K picture takes a moment to
/// encode. A Cinnamon custom keybinding on the key runs instead (a launch
/// of the person's, so Flameshot's overlay takes the keyboard).
fn screenshot_key(shell: &SharedShell, what: screenshot::What, clip: bool, cx: &mut App) {
    let shell = shell.clone();
    cx.spawn(async move |cx| {
        let action = cx.background_executor().spawn(async move { screenshot::cinnamon_action(what, clip) }).await;
        match action {
            screenshot::Action::Nothing => return,
            screenshot::Action::Command(command) => {
                cx.update(|_| person_launch(&shell, &command));
                return;
            }
            screenshot::Action::Take => {}
        }
        let taken = cx
            .background_executor()
            .spawn(async move {
                let mut ede = control::connect().map_err(|e| e.to_string())?;
                screenshot::run(&mut ede, what, clip)
            })
            .await;
        match taken {
            Ok(Some(path)) => {
                // gnome-screenshot's shutter, and a short toast as the
                // flash: the file (its picture as the icon) or the clipboard.
                screenshot::shutter();
                let toast = match &path {
                    Some(path) => {
                        tracing::info!(path = %path.display(), "screenshot saved");
                        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let mut toast = own_toast("Screenshot saved".into(), name, now_ms());
                        toast.icon = format!("file://{}", path.display());
                        toast
                    }
                    None => {
                        tracing::info!("screenshot copied");
                        own_toast("Screenshot copied".into(), String::new(), now_ms())
                    }
                };
                cx.update(|cx| notification_push(&shell, toast, cx));
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("screenshot: {e}");
                cx.update(|cx| failed_action(&shell, "take", "the screenshot", &e, cx));
            }
        }
    })
    .detach();
}

/// Something on the session bus (#EDE-12).
fn dbus_event(shell: &SharedShell, event: dbus::Event, cx: &mut App) {
    match event {
        dbus::Event::Notified(n) => {
            tracing::info!(id = n.id, app = %n.app, summary = %n.summary, "notification");
            notification_push(shell, n, cx);
        }
        dbus::Event::CloseRequested(id) => notification_close(shell, id, 3, cx),
        dbus::Event::TrayChanged => tray_refresh(shell),
        dbus::Event::TrayItems(items) => {
            {
                let mut s = shell.borrow_mut();
                s.feed.set("/tray", Value::Array(items.iter().map(dbus::TrayItem::to_json).collect()));
                s.tray = items;
            }
            refresh(shell, cx);
        }
    }
}

/// A new row and its toast; a row the 100-row cut removes closes (#EDE-135).
fn notification_push(shell: &SharedShell, n: dbus::Notification, cx: &mut App) {
    let cut = shell.borrow_mut().notifications.push(n);
    if let Some(d) = shell.borrow().dbus.as_ref() {
        for id in cut {
            d.notification_closed(id, 1);
        }
    }
    notifications_changed(shell, cx);
}

/// A notification leaves (reason: 1 expired, 2 dismissed, 3 the app closed it).
fn notification_close(shell: &SharedShell, id: u32, reason: u32, cx: &mut App) {
    let was_there = shell.borrow_mut().notifications.remove(id);
    if was_there {
        if let Some(d) = shell.borrow().dbus.as_ref() {
            d.notification_closed(id, reason);
        }
    }
    notifications_changed(shell, cx);
}

/// #EDE-196: a press on desktop notification `id`: the action `key`, or
/// (`None`) the notification itself; see [`Notifications::invoke`].
fn notification_invoke(shell: &SharedShell, id: u32, key: Option<&str>, cx: &mut App) {
    let changed = {
        let mut s = shell.borrow_mut();
        let s = &mut *s;
        s.notifications.invoke(s.dbus.as_ref(), id, key)
    };
    if changed {
        notifications_changed(shell, cx);
    }
}

/// The toast surface shows while toasts exist; every document sees the data.
fn notifications_changed(shell: &SharedShell, cx: &mut App) {
    let any_toast = {
        let mut s = shell.borrow_mut();
        let json = s.notifications.to_json();
        s.feed.set("/notifications", json);
        s.notifications.any_toast()
    };
    if any_toast {
        open_on_demand(shell, "notifications", cx);
    } else {
        close(shell, "notifications", cx);
    }
    refresh(shell, cx);
}

/// #EDE-65: the accounts' inbox and Needs you again (a delta on a shape the
/// tray reads, a minute passed: the ages move). #EDE-141: the stores'
/// SQLite off the main loop (every row of eight shapes per account, once a
/// second while a sync streams), like the start menu's issue pool; one read
/// at a time, a refresh asked for meanwhile runs once it is in.
fn exp_tray_refresh(shell: &SharedShell, cx: &mut App) {
    if shell.borrow().demo {
        tray_apply(shell, inbox::demo_inbox(now_ms()), inbox::demo_needs_you(), cx);
        return;
    }
    off_loop::read(
        shell,
        |s| &mut s.tray_gate,
        |shell| {
            let s = shell.borrow();
            let (sources, read, wanted) = (s.workspaces.tray_sources(), s.notifications.read_locally.clone(), s.spaces.wanted_sessions().into_iter().collect::<HashSet<String>>());
            move || tray_read(&sources, &read, &wanted, now_ms())
        },
        |shell, (items, needs, space_sessions), cx| {
            // #EXP-2024: the spaces' titles ride on the same read.
            if shell.borrow().spaces.sessions != space_sessions {
                shell.borrow_mut().spaces.sessions = space_sessions;
                spaces_changed(shell, cx);
            }
            tray_apply(shell, items, needs, cx);
        },
        exp_tray_refresh,
        cx,
    );
}

/// The tray's rows off the accounts' stores: BLOCKING (SQLite), off the
/// main loop; #EXP-2024: and the titles of the agent spaces' sessions
/// (`wanted`), sealed ones opened with the keyrings of the account's
/// installs on this machine (#EDE-194: the app's and the CLI daemon's).
fn tray_read(sources: &[(String, String, Arc<sync::store::ShapeStore>)], read_locally: &HashSet<String>, wanted: &HashSet<String>, now_ms: i64) -> (Vec<inbox::InboxItem>, Vec<inbox::NeedsYouItem>, Vec<uicore_model::spaces::SpaceSession>) {
    let rows: Vec<inbox::AccountRows> = sources.iter().map(|(account, user, store)| inbox::AccountRows::read(account, user, store)).collect();
    let space_sessions = if wanted.is_empty() {
        Vec::new()
    } else {
        rows.iter().flat_map(|r| inbox::space_sessions(r, wanted, &sealed::space_opener(&r.account_id))).collect()
    };
    (rows.iter().flat_map(|r| inbox::inbox_items(r, read_locally)).collect(), rows.iter().flat_map(|r| inbox::needs_you_items(r, now_ms)).collect(), space_sessions)
}

/// The inbox and Needs you as read, into the bell and the tray.
fn tray_apply(shell: &SharedShell, items: Vec<inbox::InboxItem>, needs: Vec<inbox::NeedsYouItem>, cx: &mut App) {
    {
        let mut s = shell.borrow_mut();
        let since = s.started_ms;
        s.notifications.set_inbox(items, since);
        s.needs_you = needs;
        let needs = needs_feed(&s);
        s.feed.set("/needsYou", needs);
    }
    notifications_changed(shell, cx);
}

/// An inbox entry's press: marked read, the bell closes, the Exponential
/// app opens on its issue or session (started when it does not run).
fn inbox_open(shell: &SharedShell, key: &str, cx: &mut App) {
    let link = shell.borrow().notifications.inbox_item(key).and_then(|i| i.link);
    inbox_mark_read(shell, key, cx);
    close(shell, "notification-center", cx);
    ide_open(shell, link.as_deref());
}

/// The inbox has no archive: the entry is marked read (the IDE's row press).
fn inbox_mark_read(shell: &SharedShell, key: &str, cx: &mut App) {
    let item = shell.borrow_mut().notifications.inbox_read(key);
    if let Some(item) = item {
        shell.borrow().workspaces.mark_read(&item.account_id, item.unread_ids);
    }
    notifications_changed(shell, cx);
}

fn inbox_toast_hide(shell: &SharedShell, key: &str, cx: &mut App) {
    shell.borrow_mut().notifications.inbox_toasts.retain(|(k, _)| k != key);
    notifications_changed(shell, cx);
}

/// A Needs you row: the Exponential app on that chat's card.
fn needs_you_open(shell: &SharedShell, key: &str, cx: &mut App) {
    let link = shell.borrow().needs_you.iter().find(|i| i.key() == key).map(inbox::NeedsYouItem::link);
    close(shell, "needs-you", cx);
    ide_open(shell, link.as_deref());
}

/// The outputs as the Displays page binds them (#EDE-14).
/// #EDE-250: what `name` runs at now, as the Display page shows it.
fn current_output_setting(s: &Shell, name: &str) -> display_confirm::OutputSetting {
    let outputs = s.feed.get("/outputs").and_then(Value::as_array).cloned().unwrap_or_default();
    let o = outputs.iter().find(|o| o["name"].as_str() == Some(name));
    display_confirm::OutputSetting {
        mode: o.and_then(|o| o["current"].as_str()).filter(|m| !m.is_empty()).map(String::from),
        scale: o.and_then(|o| o["scale"].as_f64()),
    }
}

/// A kept Display change into `compositor.toml`.
fn persist_display(name: &str, setting: &display_confirm::OutputSetting) {
    if let Err(e) = settings::persist_output(name, setting.mode.as_deref(), setting.scale) {
        tracing::warn!("compositor.toml: {e}");
    }
}

/// One second at a time until Keep, Revert, a newer change, or the end:
/// then the output goes back. `/display/confirm` shows the seconds.
fn display_countdown(shell: &SharedShell, generation: u64, cx: &mut App) {
    let shell = shell.clone();
    {
        let mut s = shell.borrow_mut();
        let confirm = s.display_confirm.json();
        s.feed.set("/display/confirm", confirm);
    }
    cx.spawn(async move |cx| loop {
        cx.background_executor().timer(Duration::from_secs(1)).await;
        let ran_out = {
            let mut s = shell.borrow_mut();
            if s.display_confirm.generation() != generation {
                return;
            }
            let ran_out = s.display_confirm.tick(generation);
            let confirm = s.display_confirm.json();
            s.feed.set("/display/confirm", confirm);
            ran_out
        };
        let done = ran_out.is_some();
        if let Some((name, setting)) = ran_out {
            tracing::info!("display change on {name} not kept: back to {:?} at {:?}", setting.mode, setting.scale);
            with_ede(&shell, |ede| ede.set_output(&name, setting.mode.as_deref(), setting.scale, None));
            let _ = cx.update(|cx| outputs_refresh_relayout(&shell, cx));
        }
        let _ = cx.update(|cx| refresh(&shell, cx));
        if done {
            return;
        }
    })
    .detach();
}

/// [`outputs_refresh`], and the open layer surfaces take the output's new
/// logical size when it changed. #EDE-250: a scale picked on the Display
/// page refreshed `/outputs` itself before the compositor's event came, so
/// the event saw no change and the taskbar and Settings kept the old size
/// (overflowing the output at a larger scale, short of it at a smaller one).
fn outputs_refresh_relayout(shell: &SharedShell, cx: &mut App) {
    let before = output_size(&shell.borrow());
    outputs_refresh(shell);
    if output_size(&shell.borrow()) != before {
        relayout(shell, cx);
    }
}

fn outputs_refresh(shell: &SharedShell) {
    let outputs = with_ede(shell, |ede| ede.outputs()).unwrap_or_default();
    let mut s = shell.borrow_mut();
    s.first_output = outputs.iter().find(|o| o.is_persons()).map(|o| (o.x, o.y, o.width, o.height));
    s.person_outputs = person_outputs(&outputs);
    s.feed.set("/outputs", Value::Array(settings::outputs_json(&outputs)));
    // #EDE-247: the tray's icons are drawn at the taskbar's output scale
    // (the taskbar stands on the person's first output).
    if let (Some(d), Some(o)) = (s.dbus.as_ref(), outputs.iter().find(|o| o.is_persons())) {
        d.tray_scale(o.scale);
    }
}

/// Reads the tray items again, on the tray thread: [`dbus::Event::TrayItems`]
/// (#EDE-135: a hung tray app never holds up the shell).
fn tray_refresh(shell: &SharedShell) {
    if let Some(d) = shell.borrow().dbus.as_ref() {
        d.tray_refresh();
    }
}

/// A tray item pressed: `Activate` (#EDE-252, Cinnamon's xapp status
/// applet: an `ItemIsMenu` item's press opens its menu, the taskbar's Menu;
/// a right click opens any item's).
fn tray_press(shell: &SharedShell, service: &str) {
    if let Some(d) = shell.borrow().dbus.as_ref() {
        d.tray_activate(service);
    }
}

/// #EDE-252: a row of the start button's or the clock's context menu.
fn panel_menu(shell: &SharedShell, value: &str, cx: &mut App) {
    match value {
        "settings" => settings_show(shell, cx),
        "datetime" => settings_open_page(shell, "date-and-time", cx),
        "history" => {
            history_refresh(shell);
            open_on_demand(shell, "history", cx);
            refresh(shell, cx);
        }
        "lock" => {
            with_ede(shell, |ede| ede.lock());
        }
        "logout" => {
            with_ede(shell, |ede| ede.logout());
        }
        other => tracing::warn!(value = other, "unknown panel menu entry"),
    }
}

/// #EDE-253: the compositor's `window.menu` (the window menu opens at its
/// point, placed again when it was open) and `keyboard.layout` (the
/// keyboard applet).
fn menu_event(shell: &SharedShell, kind: &str, data: Value, cx: &mut App) {
    if kind == ede_protocol::events::KEYBOARD_LAYOUT {
        let Ok(layouts) = serde_json::from_value::<ede_protocol::KeyboardLayouts>(data) else { return };
        shell.borrow_mut().feed.set("/keyboard", window_menu::keyboard_feed(&layouts));
        refresh(shell, cx);
        return;
    }
    let Ok(menu) = serde_json::from_value::<ede_protocol::WindowMenu>(data) else { return };
    close(shell, window_menu::DOCUMENT, cx);
    {
        let mut s = shell.borrow_mut();
        s.feed.set("/windowMenu", window_menu::feed(Some(&menu), true));
        s.window_menu = Some(menu);
    }
    open_on_demand(shell, window_menu::DOCUMENT, cx);
    refresh(shell, cx);
}

/// #EDE-253: the window menu's pick, then it closes.
fn window_menu_pick(shell: &SharedShell, value: &str, cx: &mut App) {
    let menu = shell.borrow().window_menu.clone();
    if let Some(menu) = menu.as_ref() {
        let id = menu.window.clone();
        match window_menu::pick(value, menu) {
            Some(window_menu::Pick::Action(action)) => {
                with_ede(shell, |ede| ede.window_action(id, action));
            }
            Some(window_menu::Pick::Workspace(index)) => {
                with_ede(shell, |ede| ede.move_to_workspace(id, index));
            }
            Some(window_menu::Pick::Close) => {
                with_ede(shell, |ede| ede.close_window(id));
            }
            Some(window_menu::Pick::Dismiss) => {}
            None => tracing::warn!(value, "unknown window menu entry"),
        }
    }
    shell.borrow_mut().feed.set("/windowMenu", window_menu::feed(menu.as_ref(), false));
    close(shell, window_menu::DOCUMENT, cx);
}

/// #EDE-253: the keyboard applet's pick: a layout, or Keyboard settings.
fn keyboard_pick(shell: &SharedShell, value: &str, cx: &mut App) {
    if value == "settings" {
        settings_open_page(shell, "keyboard", cx);
        return;
    }
    let Ok(index) = value.parse::<usize>() else { return };
    if let Some(layouts) = with_ede(shell, |ede| ede.set_keyboard_layout(index)) {
        shell.borrow_mut().feed.set("/keyboard", window_menu::keyboard_feed(&layouts));
        refresh(shell, cx);
    }
}

/// The taskbar's pins, kept in the shell's own file.
fn pins_persist(shell: &SharedShell) {
    let pinned = shell.borrow().model.taskbar.pinned.clone();
    settings::persist_shell(json!({"pinned": pinned}));
}

/// The compositor's window and workspace lists (one round trip each); the
/// person's windows only (#EDE-91).
/// #EDE-225: runs `session` on each connection `connect` makes, for the
/// shell's life: one that drops (`session` returns `true`) or cannot be made
/// is tried again after `sleep` (1 s, doubling to 10 s; back to 1 s once a
/// connection held), said once in the log per outage; `false` (the shell's
/// end of a channel went) ends it.
fn keep_connected<C, E: std::fmt::Display>(what: &str, mut connect: impl FnMut() -> Result<C, E>, mut sleep: impl FnMut(Duration), mut session: impl FnMut(C) -> bool) {
    const FIRST: Duration = Duration::from_secs(1);
    const MOST: Duration = Duration::from_secs(10);
    let mut wait = FIRST;
    let mut said = false;
    loop {
        match connect() {
            Ok(c) => {
                if said {
                    tracing::info!("{what}: connected again");
                }
                (wait, said) = (FIRST, false);
                if !session(c) {
                    return;
                }
                tracing::warn!("{what}: the connection ended; connecting again");
            }
            Err(e) => {
                if !said {
                    tracing::info!("{what}: no connection ({e}); trying again");
                    said = true;
                }
                sleep(wait);
                wait = (wait * 2).min(MOST);
                continue;
            }
        }
        sleep(wait);
    }
}

fn lists(calls: &mut ede_protocol::Client) -> Option<(Vec<ede_protocol::WindowInfo>, Vec<ede_protocol::WorkspaceInfo>)> {
    let windows = ede_protocol::persons(calls.windows().ok()?);
    let workspaces = calls.workspaces().unwrap_or_default();
    Some((windows, workspaces))
}

/// The sample windows of the goldens (`EDE_SHELL_DEMO=1`): three apps, one grouped.
fn demo_windows() -> Vec<ede_protocol::WindowInfo> {
    let w = |id: &str, app: &str, title: &str, focused: bool| ede_protocol::WindowInfo { id: ede_protocol::WindowId::new(id).unwrap(), title: Some(title.into()), app_id: Some(app.into()), x: 0, y: 0, width: 800, height: 600, focused, fullscreen: false, protected: false, x11: false, workspace: 0, minimized: false, maximized: false, space: None };
    vec![w("w1", "firefox", "Firefox", false), w("w2", "terminal", "Terminal", true), w("w3", "nemo", "Files", false), w("w4", "firefox", "Exponential — Firefox", false)]
}

/// #ERR-29: a start that cannot go on: an error (a GlitchTip event when
/// diagnostics are on), flushed before the exit skips the guard's drop.
/// `detail` (a path under the person's home) stays on stderr.
fn fail_start(what: &'static str, detail: String) -> ! {
    eprintln!("exponential-shell: {detail}");
    tracing::error!("{what}");
    ede_observability::flush();
    std::process::exit(2);
}

fn main() {
    // A crash-monitor process (ede-observability) runs and exits here.
    ede_observability::run_crash_monitor_if_requested();
    // `connect`, `--help` and bad arguments end here, before diagnostics.
    let options = parse_args();
    // #EDE-172 (P4): the channel the compositor's session started this
    // shell with, before any thread or the crash monitor starts.
    if let Err(e) = control::take() {
        eprintln!("exponential-shell: {e}");
        std::process::exit(2);
    }
    // #ERR-29: the compositor's diagnostics (panics, errors, native
    // crashes), tagged `shell`; stdout is the supervisor's frame lines. The
    // person's chats, notifications and accounts pass through here: a
    // report says where a line was logged, never what it said.
    let _diagnostics = ede_observability::init_with("shell", "info", ede_observability::Output::Stderr, ede_observability::Trail::Places);
    ede_observability::watch_native_crashes("shell");
    let dir = ede_ui::default_dir();
    let (ui, initial) = match Runtime::open_readonly(&dir) {
        Ok(opened) => opened,
        Err(e) => fail_start("the ui directory could not be opened", format!("{}: {e}", dir.display())),
    };
    let ui = Arc::new(Mutex::new(ui));
    let (changes_tx, changes_rx) = mpsc::channel::<Vec<Change>>();
    let watcher = match ede_ui::watch(ui.clone(), move |changes| {
        let _ = changes_tx.send(changes);
    }) {
        Ok(w) => w,
        Err(e) => fail_start("the ui directory could not be watched", format!("watch {}: {e}", dir.display())),
    };
    let demo = std::env::var_os("EDE_SHELL_DEMO").is_some_and(|v| !v.is_empty() && v != "0");

    // Window events and hotkeys from the compositor on their own connection.
    let (windows_tx, windows_rx) = mpsc::channel::<(Vec<ede_protocol::WindowInfo>, Vec<ede_protocol::WorkspaceInfo>)>();
    let (hotkeys_tx, hotkeys_rx) = mpsc::channel::<(String, i64)>();
    // #EDE-253: `window.menu` and `keyboard.layout` (and the layouts at each connect).
    let (menus_tx, menus_rx) = mpsc::channel::<(String, Value)>();
    let (outputs_tx, outputs_rx) = mpsc::channel::<()>();
    // #EXP-2024: the session's lock (the capture loop stops while it holds).
    let (locks_tx, locks_rx) = mpsc::channel::<bool>();
    // #EDE-225: a connection that drops (a read failed, the compositor's
    // socket went) is connected again, as the services thread does: the
    // shell never draws on without its hotkeys and its window list.
    std::thread::Builder::new()
        .name("ede-shell-events".into())
        .spawn(move || {
            let connect = || -> Result<_, ede_protocol::Error> { Ok((control::connect()?, control::connect()?)) };
            let mut again = false;
            keep_connected("the ede-protocol events", connect, std::thread::sleep, |(mut events, mut calls): (ede_protocol::Client, ede_protocol::Client)| {
                if !demo {
                    if let Some(list) = lists(&mut calls) {
                        if windows_tx.send(list).is_err() {
                            return false;
                        }
                    }
                    // #EDE-253: the keyboard applet's layouts as they are now
                    // (an older compositor answers none: the applet stays hidden).
                    if let Ok(layouts) = calls.keyboard_layouts() {
                        if menus_tx.send((ede_protocol::events::KEYBOARD_LAYOUT.to_string(), serde_json::to_value(layouts).unwrap_or_default())).is_err() {
                            return false;
                        }
                    }
                }
                // The outputs may have changed while it was away.
                if std::mem::replace(&mut again, true) && outputs_tx.send(()).is_err() {
                    return false;
                }
                if events.subscribe().is_err() {
                    return true;
                }
                while let Ok(event) = events.next_event() {
                    // #EDE-91: an agent space's windows and outputs are not the person's.
                    if !ede_protocol::is_persons_event(&event) {
                        continue;
                    }
                    if event.kind == ede_protocol::events::SESSION_LOCKED || event.kind == ede_protocol::events::SESSION_UNLOCKED {
                        if locks_tx.send(event.kind == ede_protocol::events::SESSION_LOCKED).is_err() {
                            return false;
                        }
                    } else if event.kind == ede_protocol::events::HOTKEY {
                        if let Some(name) = event.data.get("name").and_then(Value::as_str) {
                            if hotkeys_tx.send((name.to_string(), event.at)).is_err() {
                                return false;
                            }
                        }
                    } else if event.kind == ede_protocol::events::WINDOW_MENU || event.kind == ede_protocol::events::KEYBOARD_LAYOUT {
                        if menus_tx.send((event.kind.clone(), event.data.clone())).is_err() {
                            return false;
                        }
                    } else if event.kind.starts_with("output.") {
                        if outputs_tx.send(()).is_err() {
                            return false;
                        }
                    } else if !demo && (event.kind.starts_with("window.") || event.kind.starts_with("workspace.")) {
                        if let Some(list) = lists(&mut calls) {
                            if windows_tx.send(list).is_err() {
                                return false;
                            }
                        }
                    }
                }
                true
            });
        })
        .expect("the events thread");
    // #EDE-227: while eDE's component runs as the session's helper, the
    // person's own app entries (the AppImage's, exp-desktop's) stay out of
    // the start menu, its search, the favourites and the pins: one Exponential.
    let scanned = if demo { apps::Scan { apps: apps::demo(), standalone: Vec::new() } } else { apps::scan_in(&apps::data_dirs(), component_runs()) };
    let standalone = scanned.standalone;
    tracing::info!(?standalone, "the person's own app entries left out for the component");
    let mut installed = scanned.apps;
    installed.extend(builtin_apps(demo));
    // #VAPP-273: the apps on the Engine: the factory ones, then the person's (the goldens only with EDE_APPS_DIR).
    let engine_apps = if demo && std::env::var_os("EDE_APPS_DIR").is_none() { Vec::new() } else { engine::scan_installed(&engine::factory_apps_dir(), &engine::apps_dir()) };
    installed.extend(engine_apps.iter().map(engine::launcher_entry));
    installed.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    tracing::info!(apps = installed.len(), engine_apps = engine_apps.len(), demo, "desktop entries");
    let launchable: Arc<Vec<apps::App>> = Arc::new(installed.clone());
    // #EXP-2024: THE capture loop (the windows' thumbnails, the agent spaces,
    // their list) on its own thread and connection, for the shell's life.
    let captures_shared = Arc::new(captures::Shared::default());
    let (captures_tx, captures_rx) = mpsc::channel::<captures::Out>();
    {
        let shared = captures_shared.clone();
        std::thread::Builder::new()
            .name("ede-shell-captures".into())
            .spawn(move || captures::run(control::connect, &shared, &captures_tx))
            .expect("the capture thread");
    }
    // The hosted popups of every surface (VAPP-281).
    let (hosted_tx, hosted_rx) = mpsc::channel::<HostedReport>();
    // The services data source: every domain's snapshot, then its changes.
    // #EDE-172 (P13): through the compositor when it runs the daemon
    // ([`control::services`]); a daemon that ends and starts again is
    // connected to again.
    let (services_tx, services_rx) = mpsc::channel::<(String, Value)>();
    if !demo {
        std::thread::Builder::new()
            .name("ede-shell-services".into())
            .spawn(move || {
                let mut wait = Duration::from_secs(1);
                let mut said = false;
                loop {
                    // The daemon may start after the shell, or start again.
                    let mut client = match control::services() {
                        Ok(c) => c,
                        Err(e) => {
                            if !said {
                                tracing::info!("no services daemon yet ({e}): /services waits for it");
                                said = true;
                            }
                            std::thread::sleep(wait);
                            wait = (wait * 2).min(Duration::from_secs(10));
                            continue;
                        }
                    };
                    (wait, said) = (Duration::from_secs(1), false);
                    // Subscribed first (#EDE-135): a change during the gets waits
                    // on the stream instead of reaching nobody.
                    if client.subscribe().is_ok() {
                        let domains: Vec<String> = client.call(json!({"op": "list"})).ok().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
                        for d in &domains {
                            if let Ok(snapshot) = client.get(d) {
                                let _ = services_tx.send((d.clone(), snapshot));
                            }
                        }
                        while let Ok(event) = client.next_event() {
                            if let (Some(d), Some(s)) = (event["domain"].as_str(), event.get("snapshot")) {
                                if services_tx.send((d.to_string(), s.clone())).is_err() {
                                    return;
                                }
                            }
                        }
                    }
                    tracing::info!("the services connection ended; connecting again");
                    std::thread::sleep(wait);
                }
            })
            .expect("the services thread");
    }
    // Notifications and tray changes from the session bus.
    let (dbus_tx, dbus_rx) = mpsc::channel::<dbus::Event>();
    let dbus = if demo { None } else { dbus::Dbus::start(dbus_tx, clock::time) };
    // #EDE-56: the Local desktops (the machine's index, #EDE-65; eDE's old one beside compositor.toml is imported once), the
    // shared accounts and their teams; Connect Exponential's threads and the
    // scheme handler's forwarded URLs.
    let config_dir = settings::config_path().parent().map(|p| p.to_path_buf()).unwrap_or_else(|| std::path::PathBuf::from("."));
    let workspaces = match accounts::Workspaces::open(&config_dir, demo) {
        Ok(w) => w,
        Err(e) => fail_start("the desktops and accounts could not be opened", e.to_string()),
    };
    // #EDE-238: no sync of its own: the holder's store, watched.
    let team_deltas = workspaces.deltas();
    let account_changes = workspaces.account_changes();
    let (connect_tx, connect_rx) = mpsc::channel::<connect::Event>();
    let (urls_tx, urls_rx) = mpsc::channel::<String>();
    let (connect_open_tx, connect_open_rx) = mpsc::channel::<()>();
    let (signed_out_tx, signed_out_rx) = mpsc::channel::<String>();
    if !demo {
        connect::listen_urls(urls_tx);
        connect::listen_open(connect_open_tx);
    }
    let connect = connect::Connect::new(workspaces.auth.clone(), connect_tx);
    let (share_tx, share_rx) = mpsc::channel::<share::Event>();
    let share = share::Share::new(share_tx);
    // #EDE-237: the holder's page windows (their icons and titles).
    let (holder_windows_tx, holder_windows_rx) = mpsc::channel::<holder_windows::HolderWindows>();
    // #EDE-258: the start menu's tree, recents and Assistant.
    let (holder_feeds_tx, holder_feeds_rx) = mpsc::channel::<holder_feeds::HolderFeeds>();
    // #EDE-267: the Voice tray's board (its own long poll: the call's level ticks).
    let (voice_tx, voice_rx) = mpsc::channel::<Value>();
    if !demo {
        holder_windows::follow(holder_windows_tx);
        holder_feeds::follow(holder_feeds_tx);
        voice_tray::follow(voice_tx);
    }

    gpui_platform::application().with_assets(assets::Assets).run(move |cx: &mut App| {
        // #EDE-172 (P4): gpui's Wayland connection did its first roundtrip.
        control::refuse_ptrace();
        gpui_component::init(cx);
        let _ = cx.text_system().add_fonts(assets::fonts());
        // #EDE-65: the IDE's colours: the sidebar on the ground, no Sidebar fill.
        // #EDE-235: the theme the ui directory holds (its last good
        // theme.json; the factory ships the Rerusted one), followed live.
        let theme = shell_theme(ui.lock().unwrap_or_else(|p| p.into_inner()).theme());
        let mut feed = Feed::new();
        feed.set("/time", json!(clock::time()));
        feed.set("/date", json!(clock::date()));
        feed.set("/dateText", clock::date_text());
        feed.set("/windows", json!([]));
        feed.set("/apps", Value::Array(apps::filter(&installed, "")));
        feed.set("/query", json!(""));
        feed.set("/switcher", json!({"windows": []}));
        feed.set("/previews", json!({"app": null, "windows": []}));
        feed.set("/thumbnails", json!({}));
        feed.set("/osd", json!({"icon": "media-volume-up", "level": 0, "label": ""}));
        feed.set("/config", settings::config_json());
        feed.set("/pages", settings_pages::pages_json());
        feed.set("/user", settings::user_json());
        feed.set("/notifications", json!({"toasts": [], "list": [], "count": 0}));
        feed.set("/needsYou", json!({"count": 0, "rows": []}));
        feed.set("/spaces", json!({"count": 0, "rows": []}));
        feed.set("/spaceView", json!({"open": false}));
        // The goldens open the Agent spaces list through its bound `open`.
        let demo_spaces = spaces::demo_mode().filter(|_| demo);
        feed.set("/spacesList", json!({"open": demo_spaces.as_deref() == Some("list")}));
        feed.set("/voice", if demo { voice_tray::demo().unwrap_or_else(voice_tray::hidden) } else { voice_tray::hidden() });
        // #EDE-253: the goldens' window menu (open) and, with
        // EDE_SHELL_DEMO_KEYBOARD=1 (`open`: its menu open), two keyboard layouts.
        if demo {
            feed.set("/windowMenu", window_menu::feed(Some(&window_menu::demo_menu()), true));
            let keyboard = std::env::var("EDE_SHELL_DEMO_KEYBOARD").unwrap_or_default();
            if keyboard == "1" || keyboard == "open" {
                let mut value = window_menu::keyboard_feed(&window_menu::demo_keyboard());
                value["open"] = json!(keyboard == "open");
                feed.set("/keyboard", value);
            }
        }
        feed.set("/connecting", connect.feed());
        feed.set("/accounts", workspaces.accounts_json());
        feed.set("/account", json!({"signingOut": false}));
        let installs = if demo { session_account::Installs::demo() } else { session_account::Installs::default() };
        feed.set("/installs", installs.feed());
        feed.set("/sharing", settings_pages::demo_sharing().unwrap_or_else(|| share.feed()));
        if demo {
            feed.set("/osd", json!({"icon": "media-volume-down", "level": 45, "label": "45%"}));
            // #EDE-252: the demo's tray items, the first with a menu.
            let network = dbus::TrayItem {
                service: "demo/network".into(),
                title: "Network".into(),
                status: "Active".into(),
                icon: String::new(),
                menu: [(1, "Connect to Wi-Fi", None, false), (2, "Airplane mode", Some(false), false), (3, "", None, true), (4, "Settings…", None, false)]
                    .into_iter()
                    .map(|(id, label, checked, separator)| dbus::MenuEntry { id, label: label.into(), enabled: true, separator, checked, depth: 0, submenu: false })
                    .collect(),
                item_is_menu: true,
            };
            let sound = dbus::TrayItem { service: "demo/sound".into(), title: "Sound".into(), status: "Active".into(), icon: String::new(), menu: Vec::new(), item_is_menu: false };
            feed.set("/tray", json!([network.to_json(), sound.to_json()]));
            feed.set("/services", settings::demo_services());
            feed.set("/outputs", Value::Array(settings::demo_outputs()));
        } else {
            feed.set("/tray", json!([]));
            feed.set("/services", json!({}));
            feed.set("/outputs", json!([]));
        }
        // The model: the pins from the shell's own file (the demo: fixed pins).
        let mut model = ShellState::default();
        let saved = if demo { json!({"pinned": ["firefox", "nemo", "org.gnome.Calculator"]}) } else { settings::shell_json() };
        for app in saved.get("pinned").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            model = reduce(&model, &ShellEvent::TaskbarPin { app: pinned_as(app, &standalone) });
        }
        // The switcher: the Local desktops and the accounts' teams, the
        // index's current desktop selected.
        let entries = workspaces.entries();
        if !entries.is_empty() {
            model = reduce(&model, &ShellEvent::SwitcherSet { entries });
        }
        // #EDE-223: the signed-in account's team (or the saved pick); a team
        // not synced yet waits on Local, and the app is left where it is.
        let start_on = workspaces.start_on();
        let mut pending_pick = accounts::Pending::default();
        let start = match start_on {
            Some(accounts::StartOn::Local(entry)) => {
                // A start on a Local desktop: a running Exponential app follows it too.
                if !demo {
                    accounts::ide_follow(entry.kind.clone(), &entry.id);
                }
                Some(entry)
            }
            Some(accounts::StartOn::Team(entry)) => {
                if !demo {
                    accounts::ide_follow(entry.kind.clone(), &entry.id);
                }
                Some(entry)
            }
            Some(accounts::StartOn::Wait(pending)) => {
                tracing::info!(?pending, "the start waits for the account's teams");
                pending_pick = pending;
                workspaces.current_local()
            }
            None => None,
        };
        if let Some(current) = start {
            model = reduce(&model, &ShellEvent::SwitcherPick { entry_kind: current.kind, id: current.id });
        }
        let menu_apps: Vec<StartMenuApp> = installed.iter().map(apps::App::start_menu_app).collect();
        let favourites = model.taskbar.pinned.clone();
        feed.set("/shell", uicore_data::shell_feed(&model, &menu_apps, &favourites));
        let applets = applets_json(feed.get("/services").unwrap_or(&Value::Null));
        feed.set("/applets", applets);

        let shell_cell: Rc<RefCell<Option<SharedShell>>> = Rc::new(RefCell::new(None));
        let plugin_for: uicore_gpui::PluginFor = {
            let cell = shell_cell.clone();
            let hosted_tx = hosted_tx.clone();
            Rc::new(move |document: &str| {
                let shell = cell.borrow().clone().expect("the shell exists before a surface opens");
                Rc::new(Host { shell, document: document.to_string(), hosted_tx: hosted_tx.clone() }) as Rc<dyn HostPlugin>
            })
        };
        let on_frame: uicore_gpui::OnFrame = Rc::new(|name, digest| println!("{}", FrameLine::line_for_digest(name, digest)));
        let host: Box<dyn uicore_gpui::ShellHost> = if options.windowed { Box::new(WindowedHost) } else { Box::new(LayerShellHost::default()) };
        let surfaces = Surfaces::new(host, theme, plugin_for, on_frame, "at.exponential.ede.shell");
        let shell: SharedShell = Rc::new(RefCell::new(Shell {
            options,
            ui: ui.clone(),
            surfaces,
            feed,
            model,
            ede: control::connect().ok(),
            apps: installed,
            menu_apps,
            x11_hints: Default::default(),
            windows: Vec::new(),
            thumbnails: HashMap::new(),
            holder_windows: Default::default(),
            // #EDE-258: the goldens' menu draws the draft's sample tree.
            holder_feeds: if demo { holder_feeds::demo() } else { Default::default() },
            local_tree: if demo { holder_feeds::demo_local() } else { Vec::new() },
            captures: captures_shared,
            spaces: demo_spaces.as_deref().map(spaces::Spaces::demo).unwrap_or_default(),
            locked: false,
            first_output: None,
            display_confirm: Default::default(),
            person_outputs: Vec::new(),
            layers_opened: HashMap::new(),
            osd_until: None,
            window_menu: demo.then(window_menu::demo_menu),
            popups_opened: HashMap::new(),
            popup_rooms: HashMap::new(),
            switcher_generation: 0,
            notifications: Notifications {
                // The goldens: two desktop notifications (the first toasting) between the demo inbox's entries.
                demo: if demo {
                    let now = now_ms();
                    vec![
                        (now - 2 * 60_000, json!({"id": 1, "app": "Thunderbird", "summary": "Release notes for 1.4", "body": "Danny: the M1 demo runs on Friday, bring the VM.", "icon": "", "actions": [{"key": "open", "label": "Open"}, {"key": "archive", "label": "Archive"}], "urgency": 1, "at": "11:58"})),
                        (now - 20 * 60_000, json!({"id": 2, "app": "Software", "summary": "Updates installed", "body": "12 packages were updated.", "icon": "", "actions": [], "urgency": 0, "at": "11:40"})),
                    ]
                } else {
                    Vec::new()
                },
                ..Notifications::default()
            },
            needs_you: Vec::new(),
            tray_gate: off_loop::Gate::default(),
            demo,
            // The demo toasts only through `--open notifications`.
            started_ms: if demo { i64::MAX / 2 } else { now_ms() },
            dbus,
            tray: Vec::new(),
            workspaces,
            connect,
            installs,
            signing_out: None,
            signed_out_tx,
            share,
            local: None,
            // #EDE-65: the team picked last, once its account's teams are listed.
            pending_pick,
            engine: None,
            engine_seats: engine::exposes::seats(Vec::new()),
            launches: system::LaunchGuard::default(),
            focus_next: system::FocusNext::default(),
            engine_apps: Vec::new(),
            launchable: Arc::new(Vec::new()),
            team_stores: None,
            issue_index: Arc::new(Vec::new()),
            issue_hits: Vec::new(),
            issue_search: 0,
            menu_selection: start_menu::Selection::default(),
            settings_nav: {
                let mut nav = system_settings::Nav::probe(demo);
                // The goldens: `EDE_SHELL_SETTINGS_PAGE=<module>` opens on that module.
                if let Some(module) = settings_pages::demo_module() {
                    nav.step(&uicore_model::settings::SettingsEvent::Link { target: module });
                }
                nav
            },
            time_changes: Vec::new(),
            _watcher: watcher,
        }));
        // #EDE-92, #EDE-258: the arrows in the start menu (Cinnamon's columns).
        {
            let s = shell.clone();
            cx.intercept_keystrokes(move |event, window, cx| {
                // #EDE-273: Alt+Left in System Settings = its back button.
                let m = &event.keystroke.modifiers;
                let held = window.modifiers();
                if system_settings::is_back_key(&event.keystroke.key, m.alt || held.alt, m.control || m.shift || m.platform) {
                    let in_settings = s.borrow().surfaces.region(system_settings::DOCUMENT).is_some_and(|r| gpui::AnyWindowHandle::from(r.handle) == window.window_handle());
                    if in_settings {
                        let s = s.clone();
                        cx.defer(move |cx| settings_step(&s, uicore_model::settings::SettingsEvent::Back, false, cx));
                        cx.stop_propagation();
                        return;
                    }
                }
                let Some(step) = start_menu::key_of(&event.keystroke.key, event.keystroke.modifiers.modified()) else { return };
                if start_menu_step(&s, window.window_handle(), step, cx) {
                    cx.stop_propagation();
                }
            })
            .detach();
        }
        *shell_cell.borrow_mut() = Some(shell.clone());
        history_refresh(&shell);
        overlays_refresh(&shell);
        exp_tray_refresh(&shell, cx);
        model_changed(&shell, cx);
        if !demo {
            // The outputs first: a stretched surface takes the output's size.
            outputs_refresh(&shell);
        } else {
            // #EDE-168: the output the taskbar's popups measure their room on.
            let outputs = with_ede(&shell, |ede| ede.outputs()).unwrap_or_default();
            let mut s = shell.borrow_mut();
            s.first_output = outputs.iter().find(|o| o.is_persons()).map(|o| (o.x, o.y, o.width, o.height));
            s.person_outputs = person_outputs(&outputs);
        }
        // #EXP-2024: the spaces' feeds at the first display's size.
        spaces_feed(&shell);
        if demo {
            // The sample windows, before any document opens (the goldens).
            windows_changed(&shell, demo_windows(), vec![ede_protocol::WorkspaceInfo { index: 0, active: true, windows: vec![] }, ede_protocol::WorkspaceInfo { index: 1, active: false, windows: vec![] }], cx);
        } else {
            // The tray as it stands at start.
            tray_refresh(&shell);
        }

        // Every layer document on screen, the ground, the sidebar and the
        // taskbar first; popups wait for their anchor.
        let mut docs: Vec<Document> = ui.lock().unwrap_or_else(|p| p.into_inner()).documents().cloned().collect();
        docs.sort_by_key(layer_rank);
        for doc in &docs {
            if matches!(doc.surface, SurfaceHeader::Layer { on_demand: false, .. }) {
                open_layer(&shell, doc, cx);
            }
        }
        for change in initial {
            if let Change::Rejected { name, issues, .. } = change {
                tracing::warn!(document = %name, ?issues, "refused at start");
            }
        }
        // #EDE-202: a document the supervisor set aside at start (an older
        // release's, refused now), told once with the shell's own toast.
        for notice in ede_ui::take_notices(&dir) {
            let (title, body) = notice.toast();
            toast(&shell, &title, &body, cx);
        }
        if shell.borrow().surfaces.open_names().next().is_none() {
            tracing::warn!(dir = %dir.display(), "no layer document to draw (is the factory directory installed? exponential-ui --factory apps/ede/ui check)");
        }
        // #VAPP-273: the apps on the Engine: booted, their widgets on the ground.
        shell.borrow_mut().engine_apps = engine_apps;
        shell.borrow_mut().launchable = launchable.clone();
        engine_boot(&shell, cx);
        let open_apps = shell.borrow().options.open_app.clone();
        let eng = shell.borrow().engine.clone();
        if let Some(eng) = eng {
            for app in open_apps {
                engine::open_window(&eng, &app, cx);
            }
        }

        // The main loop's inputs: file changes, window lists, hotkeys,
        // thumbnails, the bus, the services, the hosted popups, the clock.
        let s = shell.clone();
        cx.spawn(async move |cx| {
            let mut minute = clock::time();
            let (mut tray_pending, mut last_tray) = (false, Instant::now());
            let mut last_installs = Instant::now();
            let mut first_login = !demo && connect::first_login(&config_dir, s.borrow().workspaces.session_account().is_some());
            loop {
                let mut dirty = false;
                while let Ok(changes) = changes_rx.try_recv() {
                    for change in changes {
                        cx.update(|cx| apply_change(&s, change, cx));
                    }
                }
                while let Ok((windows, workspaces)) = windows_rx.try_recv() {
                    cx.update(|cx| windows_changed(&s, windows, workspaces, cx));
                }
                while let Ok((name, at)) = hotkeys_rx.try_recv() {
                    cx.update(|cx| hotkey(&s, &name, at, cx));
                }
                while let Ok((kind, data)) = menus_rx.try_recv() {
                    cx.update(|cx| menu_event(&s, &kind, data, cx));
                }
                while let Ok(out) = captures_rx.try_recv() {
                    cx.update(|cx| capture_out(&s, out, cx));
                    dirty = true;
                }
                if let Some(locked) = locks_rx.try_iter().last() {
                    s.borrow_mut().locked = locked;
                    tracing::info!(locked, "session lock");
                }
                while let Ok(event) = dbus_rx.try_recv() {
                    cx.update(|cx| dbus_event(&s, event, cx));
                }
                if let Some(voice) = voice_rx.try_iter().last() {
                    s.borrow_mut().feed.set("/voice", voice);
                    dirty = true;
                }
                if let Some(feeds) = holder_feeds_rx.try_iter().last() {
                    holder_feeds::keep(&mut s.borrow_mut().holder_feeds, feeds);
                    cx.update(|cx| model_changed(&s, cx));
                    dirty = true;
                }
                if let Some(windows) = holder_windows_rx.try_iter().last() {
                    s.borrow_mut().holder_windows = windows;
                    previews_refresh(&s);
                    dirty = true;
                }
                while let Ok((domain, snapshot)) = services_rx.try_recv() {
                    let mut sh = s.borrow_mut();
                    let mut services = sh.feed.get("/services").cloned().unwrap_or_else(|| json!({}));
                    services[domain.as_str()] = settings::decorate(&domain, snapshot);
                    let applets = applets_json(&services);
                    sh.feed.set("/services", services);
                    sh.feed.set("/applets", applets);
                    dirty = true;
                }
                if outputs_rx.try_iter().count() > 0 {
                    cx.update(|cx| outputs_refresh_relayout(&s, cx));
                    // #EXP-2024: the pinned view's box follows the display
                    // (relayout resized its layer through header_for).
                    spaces_feed(&s);
                    let size = {
                        let sh = s.borrow();
                        sh.spaces.view(first_display(&sh)).map(|(_, v)| (v.width, v.height)).filter(|_| sh.surfaces.is_open(spaces::VIEW))
                    };
                    s.borrow_mut().spaces.shown_size = size;
                    dirty = true;
                }
                // The latest report per surface only (#EDE-65): a hover Popover
                // reports itself empty, then again once its data is in; the
                // window opens at the second size instead of the first.
                for (document, layers) in latest_reports(hosted_rx.try_iter()) {
                    cx.update(|cx| s.borrow_mut().surfaces.sync_hosted(&document, &layers, cx));
                    // #EDE-252: a menu opening closes the hover previews (Cinnamon's
                    // right click hides the thumbnails); with the shell free (#EDE-75).
                    let covered = previews_under_a_menu(&layers);
                    if !covered.is_empty() {
                        let hosted: Vec<_> = s.borrow().surfaces.hosted_layers().into_iter().filter(|(r, owner)| r.name == document && covered.contains(owner)).collect();
                        cx.update(|cx| uicore_gpui::dismiss_hosted(hosted, cx));
                    }
                }
                // #EDE-56: a team store changed, a callback URL arrived, Connect reported.
                let mut teams_changed = false;
                let mut records_changed = false;
                let mut tray_changed = false;
                while let Ok(delta) = team_deltas.try_recv() {
                    if let sync::ShapeDelta::Applied { shape, account_id, .. } = &delta {
                        if inbox::refreshes_tray(shape) {
                            tray_changed = true;
                        }
                        match *shape {
                            "teams" | "team_members" => teams_changed = true,
                            // #VAPP-273: the connected team's apps and records.
                            "apps" | "app_records" => {
                                if s.borrow().team_stores.as_ref().is_some_and(|t| &t.account_id == account_id) {
                                    records_changed = true;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if records_changed {
                    let (team, store) = {
                        let sh = s.borrow();
                        (sh.team_stores.clone(), sh.team_stores.as_ref().and_then(|t| sh.workspaces.store(&t.account_id)))
                    };
                    if let (Some(team), Some(store)) = (team, store) {
                        team.feed(&store);
                    }
                }
                while let Ok(url) = urls_rx.try_recv() {
                    // #EDE-226: the shell is this desktop's handler of
                    // exponential:// links: the sign-in's return is its own,
                    // every other link the app's.
                    let mine = s.borrow_mut().connect.callback(&url);
                    if !mine {
                        cx.update(|_| hand_off(&s, &url));
                    }
                }
                // #EDE-226: the app asks for the sign-in (its login is Connect).
                if connect_open_rx.try_iter().count() > 0 {
                    cx.update(|cx| open_on_demand(&s, "connect", cx));
                    dirty = true;
                }
                while let Ok(account_id) = signed_out_rx.try_recv() {
                    cx.update(|cx| account_changed(&s, api::AuthEvent::SignedOut { account_id }, cx));
                    teams_changed = true;
                    dirty = true;
                }
                if last_installs.elapsed() >= Duration::from_secs(1) {
                    last_installs = Instant::now();
                    cx.update(|cx| installs_follow(&s, cx));
                }
                // #EDE-226: nobody signed in yet on this machine's eDE: Connect
                // shows at the first login, once.
                if first_login && s.borrow().surfaces.is_open("taskbar") {
                    first_login = false;
                    tracing::info!("the first login: Connect Exponential");
                    cx.update(|cx| open_on_demand(&s, "connect", cx));
                    dirty = true;
                }
                while let Ok(event) = connect_rx.try_recv() {
                    if matches!(event, connect::Event::Signed(_) | connect::Event::Failed(_)) {
                        cx.update(|cx| connect_reopen(&s, cx));
                    }
                    let signed = s.borrow_mut().connect.apply(event);
                    if let Some(account) = signed {
                        cx.update(|cx| account_changed(&s, api::AuthEvent::SignedIn { account_id: account.id }, cx));
                        teams_changed = true;
                    }
                    let feed = s.borrow().connect.feed();
                    s.borrow_mut().feed.set("/connecting", feed);
                    dirty = true;
                }
                // #EDE-223: a sign-in or sign-out in another process.
                while let Ok(event) = account_changes.try_recv() {
                    cx.update(|cx| account_changed(&s, event, cx));
                    teams_changed = true;
                    dirty = true;
                }
                while let Ok(event) = share_rx.try_recv() {
                    let done = s.borrow_mut().share.apply(event);
                    if let Some(shared) = done {
                        cx.update(|cx| share_done(&s, shared, cx));
                    }
                    cx.update(|cx| sharing_changed(&s, cx));
                }
                if teams_changed {
                    cx.update(|cx| switcher_refresh_entries(&s, cx));
                }
                // #EDE-65: the tray's inbox and Needs you, at most once a second while a sync streams.
                tray_pending |= tray_changed;
                if tray_pending && last_tray.elapsed() >= Duration::from_secs(1) {
                    tray_pending = false;
                    last_tray = Instant::now();
                    cx.update(|cx| exp_tray_refresh(&s, cx));
                }
                let expired = s.borrow_mut().notifications.expire(Instant::now());
                if expired {
                    cx.update(|cx| notifications_changed(&s, cx));
                }
                let osd_over = s.borrow().osd_until.is_some_and(|t| Instant::now() >= t);
                if osd_over {
                    s.borrow_mut().osd_until = None;
                    cx.update(|cx| close(&s, "osd", cx));
                }
                let now = clock::time();
                if now != minute {
                    minute = now.clone();
                    // The inbox's and Needs you's ages.
                    tray_pending = true;
                    let mut sh = s.borrow_mut();
                    sh.feed.set("/time", json!(now));
                    sh.feed.set("/date", json!(clock::date()));
                    sh.feed.set("/dateText", clock::date_text());
                    dirty = true;
                }
                // #EXP-2024: the capture targets follow what is open (a hosted
                // list, a layer, the lock); the loop wakes only when they moved.
                thumbnails_retarget(&s);
                if dirty {
                    cx.update(|cx| refresh(&s, cx));
                }
                cx.update(|cx| fit_popups(&s, cx));
                cx.background_executor().timer(Duration::from_millis(100)).await;
            }
        })
        .detach();

        let open_now = shell.borrow().options.open.clone();
        for name in open_now {
            // The goldens: the switcher opens as Alt+Tab would (the sample windows, the second selected).
            if name == "switcher" {
                switcher_step(&shell, ShellEvent::AltTabNext, cx);
                switcher_show(&shell, cx);
            } else if name == system_settings::DOCUMENT {
                settings_show(&shell, cx);
            } else {
                open_on_demand(&shell, &name, cx);
            }
        }
        let wanted_popups = shell.borrow().options.popup.clone();
        if !wanted_popups.is_empty() {
            let s = shell.clone();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_millis(700)).await;
                let all = wanted_popups.iter().any(|p| p == "all");
                let names: Vec<String> = s.borrow().ui.lock().unwrap_or_else(|p| p.into_inner()).documents().filter(|d| matches!(d.surface, SurfaceHeader::Popup { .. }) && (all || wanted_popups.contains(&d.name))).map(|d| d.name.clone()).collect();
                cx.update(|cx| {
                    for name in names {
                        if name == "start-menu" || name == "menu" {
                            start_menu_toggle(&s, cx);
                        } else {
                            toggle_popup(&s, &name, cx);
                        }
                    }
                });
            })
            .detach();
        }
        let exit_after = shell.borrow().options.exit_after;
        if let Some(ms) = exit_after {
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_millis(ms)).await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        }
    });
}

#[cfg(test)]
mod notification_lifetime {
    use super::*;

    fn toast(id: u32) -> dbus::Notification {
        dbus::Notification { id, expires_at: Some(Instant::now()), ..failed_toast("send", "message", "", 0) }
    }

    /// #EDE-135: the toast leaves, the row stays in the bell with its
    /// actions, so its id stays alive (no NotificationClosed yet).
    #[test]
    fn a_toast_that_expires_keeps_its_notification_while_the_bell_lists_it() {
        let mut n = Notifications::default();
        n.push(toast(7));
        assert!(n.expire(Instant::now() + Duration::from_secs(1)), "the toast left");
        assert!(n.toasts.is_empty());
        assert!(n.list.iter().any(|x| x.id == 7), "the bell keeps the row");
        // The id stays alive: a press on the row later still reaches the app.
        assert!(n.remove(7), "the row's dismiss is what closes it");
    }

    /// The 101st notification pushes the oldest row out: that one closes.
    #[test]
    fn the_hundred_row_cut_closes_what_it_removes() {
        let mut n = Notifications::default();
        for id in 1..=100 {
            assert!(n.push(toast(id)).is_empty());
        }
        assert_eq!(n.push(toast(101)), vec![1]);
        assert_eq!(n.list.len(), 100);
        assert!(!n.toasts.contains(&1));
    }
}

/// #EDE-196: a toast's presses reach the app that sent it, end to end: a
/// fake app on a private bus calls `Notify`, the shell's notification
/// server hears it, the toast document as it ships draws it from the feed,
/// a press resolves through the document's own bindings, and the app hears
/// the spec's signals. The release check's snapd question sent
/// `{"key":"yes"}` alone, so no app ever heard a press.
#[cfg(test)]
mod toast_presses {
    use super::*;
    use eui_core::surface::{OutEvent, Surface};

    /// An app on the bus: it sends notifications and hears the server's signals.
    struct FakeApp {
        conn: zbus::blocking::Connection,
        signals: mpsc::Receiver<(String, Value)>,
    }

    impl FakeApp {
        fn connect(bus: &dbus::private_bus::PrivateBus) -> FakeApp {
            let conn = bus.builder().and_then(|b| b.build()).expect("the app's connection");
            let rule = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).interface(dbus::NOTIFICATIONS_NAME).expect("the interface").build();
            let iter = zbus::blocking::MessageIterator::for_match_rule(rule, &conn, None).expect("the app listens");
            let (tx, signals) = mpsc::channel();
            std::thread::spawn(move || {
                for msg in iter.flatten() {
                    let member = msg.header().member().map(|m| m.to_string()).unwrap_or_default();
                    let body = match member.as_str() {
                        "ActionInvoked" => msg.body().deserialize::<(u32, String)>().map(|(id, key)| json!([id, key])),
                        "NotificationClosed" => msg.body().deserialize::<(u32, u32)>().map(|(id, reason)| json!([id, reason])),
                        _ => continue,
                    };
                    if tx.send((member, body.expect("the spec's signature"))).is_err() {
                        break;
                    }
                }
            });
            FakeApp { conn, signals }
        }

        /// `Notify` as libnotify sends it; the server's id.
        fn notify(&self, summary: &str, actions: &[&str], resident: bool) -> u32 {
            let mut hints: HashMap<&str, zbus::zvariant::Value> = HashMap::new();
            if resident {
                hints.insert("resident", zbus::zvariant::Value::from(true));
            }
            let reply = self
                .conn
                .call_method(Some(dbus::NOTIFICATIONS_NAME), dbus::NOTIFICATIONS_PATH, Some(dbus::NOTIFICATIONS_NAME), "Notify", &("fake-app", 0u32, "", summary, "", actions.to_vec(), hints, 0i32))
                .expect("Notify answers");
            reply.body().deserialize::<u32>().expect("an id")
        }

        /// The next signal the app hears, within a few seconds.
        fn heard(&self) -> Option<(String, Value)> {
            self.signals.recv_timeout(Duration::from_secs(5)).ok()
        }

        /// Nothing more within a moment.
        fn quiet(&self) -> bool {
            self.signals.recv_timeout(Duration::from_millis(300)).is_err()
        }
    }

    /// The server's next notification, into the shell's list.
    fn arrived(rx: &mpsc::Receiver<dbus::Event>, n: &mut Notifications) {
        loop {
            match rx.recv_timeout(Duration::from_secs(5)).expect("the server heard it") {
                dbus::Event::Notified(notification) => {
                    n.push(notification);
                    return;
                }
                _ => continue,
            }
        }
    }

    /// The toast document as it ships, drawn from the shell's feed.
    fn toasts(n: &Notifications) -> Surface {
        let source = uicore_docs::doc("notifications").expect("the toast document").json();
        let doc = ede_ui::validate_document("notifications", &source).expect("it validates as shipped");
        let mut s = doc.surface(uicore_docs::factory_theme());
        s.set_data("/notifications", Some(n.to_json())).expect("the feed binds");
        s.set_viewport(380.0, 0.0, None);
        s.layout(&mut eui_core::measure::FixedMeasure::default());
        s
    }

    /// The live nodes `pick` matches.
    fn find(s: &mut Surface, pick: impl Fn(&eui_core::surface::PlacedNode) -> bool) -> Vec<eui_core::surface::PlacedNode> {
        s.nodes().into_iter().filter(|n| !n.removed && pick(n)).collect()
    }

    /// An action button of a toast (the template `toast_action`'s items).
    fn is_action(n: &eui_core::surface::PlacedNode) -> bool {
        n.component == "Button" && n.part.is_none() && n.id.starts_with("toast_action.")
    }

    /// The labels of the toasts' action buttons, sorted.
    fn button_labels(s: &mut Surface) -> Vec<String> {
        let mut labels: Vec<String> = find(s, is_action).iter().filter_map(|n| n.props.get("label").and_then(Value::as_str).map(String::from)).collect();
        labels.sort();
        labels
    }

    /// The host function a press on node `index` calls, with its resolved args.
    fn press(s: &mut Surface, index: u32) -> (String, Value) {
        s.event(index, "press", None)
            .into_iter()
            .find_map(|e| match e {
                OutEvent::FunctionCall { name, args, .. } => Some((name, args)),
                _ => None,
            })
            .expect("the press calls a host function")
    }

    fn press_button(s: &mut Surface, label: &str) -> (String, Value) {
        let button = find(s, |n| is_action(n) && n.props.get("label").and_then(Value::as_str) == Some(label));
        assert_eq!(button.len(), 1, "one {label} button");
        press(s, button[0].index)
    }

    /// The press on the card itself (not on one of its buttons).
    fn press_card(s: &mut Surface) -> (String, Value) {
        let cards = find(s, |n| n.component == "Box" && n.pressable && n.id.starts_with("toast."));
        assert_eq!(cards.len(), 1, "one toast card is a press target");
        press(s, cards[0].index)
    }

    /// What the shell's host function does with the press (`ede_notify_open`
    /// = the card itself, `ede_notify_action` = a button).
    fn dispatch(n: &mut Notifications, server: &dbus::Dbus, (name, args): (String, Value)) -> bool {
        let id = args["id"].as_u64().expect("the press names its notification") as u32;
        match name.as_str() {
            "ede_notify_open" => n.invoke(Some(server), id, None),
            "ede_notify_action" => n.invoke(Some(server), id, Some(args["key"].as_str().expect("and its action"))),
            other => panic!("the toast called {other}"),
        }
    }

    #[test]
    fn an_action_button_reaches_the_app_with_the_toast_s_id_and_its_key() {
        let bus = dbus::private_bus::PrivateBus::start();
        let (tx, rx) = mpsc::channel();
        let server = dbus::Dbus::start_on(bus.builder(), tx, || "12:00".into()).expect("the shell on the private bus");
        let app = FakeApp::connect(&bus);
        let mut n = Notifications::default();

        // snapd-desktop-integration's question: Yes, No and the default action.
        let id = app.notify("Some required themes are missing.", &["yes", "Yes", "no", "No", "default", "default"], false);
        arrived(&rx, &mut n);
        let mut s = toasts(&n);
        assert_eq!(button_labels(&mut s), ["No", "Yes"], "the default action is no button");
        let call = press_button(&mut s, "Yes");
        assert_eq!(call, ("ede_notify_action".to_string(), json!({"id": id, "key": "yes"})), "the press names the toast and the action");
        assert!(dispatch(&mut n, &server, call));
        assert_eq!(app.heard(), Some(("ActionInvoked".into(), json!([id, "yes"]))));
        assert_eq!(app.heard(), Some(("NotificationClosed".into(), json!([id, 2]))), "then it closes, dismissed");
        assert!(app.quiet());
        assert!(n.list.is_empty() && n.toasts.is_empty());

        // The card itself is the default action.
        let id = app.notify("Release notes", &["default", "Open", "archive", "Archive"], false);
        arrived(&rx, &mut n);
        let mut s = toasts(&n);
        assert_eq!(button_labels(&mut s), ["Archive"], "Open is the card itself");
        let call = press_card(&mut s);
        assert_eq!(call, ("ede_notify_open".to_string(), json!({"id": id})));
        assert!(dispatch(&mut n, &server, call));
        assert_eq!(app.heard(), Some(("ActionInvoked".into(), json!([id, "default"]))));
        assert_eq!(app.heard(), Some(("NotificationClosed".into(), json!([id, 2]))));
        assert!(app.quiet());

        // Without a default action the card's press only dismisses it.
        let id = app.notify("Updates installed", &[], false);
        arrived(&rx, &mut n);
        let mut s = toasts(&n);
        assert!(dispatch(&mut n, &server, press_card(&mut s)));
        assert_eq!(app.heard(), Some(("NotificationClosed".into(), json!([id, 2]))), "no ActionInvoked the app never offered");
        assert!(app.quiet());

        // A resident notification stays (the bell keeps it); its toast goes.
        let id = app.notify("Call", &["answer", "Answer"], true);
        arrived(&rx, &mut n);
        let mut s = toasts(&n);
        assert!(dispatch(&mut n, &server, press_button(&mut s, "Answer")));
        assert_eq!(app.heard(), Some(("ActionInvoked".into(), json!([id, "answer"]))));
        assert!(app.quiet(), "a resident notification does not close");
        assert!(n.list.iter().any(|x| x.id == id) && !n.toasts.contains(&id));
        // An action it never offered reaches nothing.
        assert!(!n.invoke(Some(&server), id, Some("hang-up")));
        assert!(app.quiet());
    }
}

/// #ERR-29: both binaries of this crate report like the compositor (the
/// same check as ede-session's packaging test, which CI runs only when eDE
/// itself changes).
#[cfg(test)]
mod diagnostics {
    fn starts_diagnostics(bin: &str, source: &str, components: &[&str]) {
        // The body of `main` alone (this test's own text is in the file too).
        let main = &source[source.find("\nfn main(").expect("a main") + 1..];
        let main = &main[..main.find("\n}\n").expect("the end of main")];
        let first = main.lines().skip(1).map(str::trim).find(|l| !l.is_empty() && !l.starts_with("//"));
        assert_eq!(first, Some("ede_observability::run_crash_monitor_if_requested();"), "{bin}: the crash monitor's dispatch comes first");
        assert!(main.contains("ede_observability::init_with(") && main.contains("ede_observability::watch_native_crashes("), "{bin} starts diagnostics");
        assert!(main.contains("ede_observability::Output::Stderr, ede_observability::Trail::Places)"), "{bin}: stdout is a protocol, and no line's text leaves the machine");
        assert!(!source.contains(&["tracing_subscriber", "::fmt()"].concat()), "{bin}: a second subscriber panics");
        for component in components {
            assert!(source.contains(&format!("\"{component}\"")), "{bin}: tagged {component}");
        }
    }

    #[test]
    fn the_shell_and_the_sealed_surfaces_start_diagnostics() {
        starts_diagnostics("exponential-shell", include_str!("main.rs"), &["shell"]);
        starts_diagnostics("exponential-sealed", include_str!("bin/ede-sealed/main.rs"), &["sealed-lock", "sealed-polkit", "sealed-consent", "sealed-share"]);
    }

    /// #ERR-32: a release build of both binaries keeps the `log` lines of
    /// the libraries they link (api, sync, elocal, emcp, eengine-desktop,
    /// gpui, crash-monitor) at least from WARN. Cargo unifies features, so
    /// one crate turning on a `log` level feature compiles the lines out of
    /// every binary in its graph: the runner's `release_max_level_off` did.
    /// The features `cargo tree` resolves for this package alone, the way
    /// `apps/ede/packaging/stage.sh` builds it (`-p ede-shell --bins`).
    #[test]
    fn a_release_build_keeps_the_log_lines() {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        let out = std::process::Command::new(cargo)
            .args(["tree", "--locked", "--offline", "--manifest-path", manifest, "-p", "ede-shell", "-e", "features,no-dev", "-i", "log", "--depth", "1", "--prefix", "none"])
            .output()
            .expect("cargo runs");
        let tree = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success() && tree.lines().any(|line| line.starts_with("log v")), "cargo tree: {}{tree}", String::from_utf8_lossy(&out.stderr));
        // `log`'s STATIC_MAX_LEVEL in a release build: the strictest
        // `release_max_level_*`, else the strictest `max_level_*`, else trace.
        const LEVELS: [&str; 6] = ["off", "error", "warn", "info", "debug", "trace"];
        let strictest = |prefix: &str| LEVELS.iter().position(|level| tree.contains(&format!("log feature \"{prefix}{level}\"")));
        let kept = strictest("release_max_level_").or_else(|| strictest("max_level_")).unwrap_or(LEVELS.len() - 1);
        assert!(kept >= 2, "a release build compiles `log` down to `{}`:\n{tree}", LEVELS[kept]);
    }
}

#[cfg(test)]
mod function_names {
    /// Every host function a factory document's header allows is one this
    /// shell matches by that exact (flat, A2UI 1.0) name; `set` is genUI's
    /// own. #VAPP-289 renamed the documents' calls to `ede_*` while the
    /// headers and the shell said `ede.*`, so eDE 0.2.5 drew only the ground.
    #[test]
    fn the_factory_documents_name_functions_the_shell_matches() {
        let source = include_str!("main.rs");
        let mut checked = 0;
        for doc in uicore_docs::DOCS {
            let parsed: serde_json::Value = doc.json();
            for f in parsed["surface"]["functions"].as_array().into_iter().flatten().filter_map(serde_json::Value::as_str) {
                if f == "set" {
                    continue;
                }
                assert!(f.starts_with("ede_"), "{}: {f} is not an ede_* host function", doc.name);
                assert!(source.contains(&format!("\"{f}\"")), "{}: the shell never matches {f}", doc.name);
                checked += 1;
            }
        }
        assert!(checked > 40, "the headers were read ({checked})");
    }

    /// #EDE-93: a press opens the popups whose anchor is the pressed
    /// component's id (`popups_of`), so every factory popup's anchor names a
    /// component of its parent document. #VAPP-289 renamed the taskbar's
    /// `applet-sound` to `applet_sound` while the popup kept the old anchor,
    /// so a click on the sound applet opened nothing.
    #[test]
    fn every_factory_popup_anchors_at_a_component_of_its_parent() {
        fn ids(node: &serde_json::Value, out: &mut Vec<String>) {
            match node {
                serde_json::Value::Object(map) => {
                    if let Some(id) = map.get("id").and_then(serde_json::Value::as_str) {
                        out.push(id.to_string());
                    }
                    map.values().for_each(|v| ids(v, out));
                }
                serde_json::Value::Array(items) => items.iter().for_each(|v| ids(v, out)),
                _ => {}
            }
        }
        let docs: Vec<(&str, serde_json::Value)> = uicore_docs::DOCS.iter().map(|d| (d.name, d.json())).collect();
        let mut checked = Vec::new();
        for (name, doc) in &docs {
            let popup = &doc["surface"]["popup"];
            let (Some(parent), Some(anchor)) = (popup["parent"].as_str(), popup["anchor"].as_str()) else { continue };
            let (_, parent_doc) = docs.iter().find(|(n, _)| *n == parent).unwrap_or_else(|| panic!("{name}: no parent document {parent}"));
            let mut parent_ids = Vec::new();
            ids(&parent_doc["root"], &mut parent_ids);
            assert!(parent_ids.iter().any(|id| id == anchor), "{name}: {parent} has no component {anchor}");
            checked.push(*name);
        }
        assert!(checked.contains(&"applet-sound"), "the sound applet is a popup ({checked:?})");
    }
}

#[cfg(test)]
mod switcher {
    use super::*;

    fn opened(windows: &[(&str, &str, &str, usize, bool)]) -> ShellState {
        let mut s = ShellState::default();
        s.workspaces.count = 2;
        for (id, app, title, workspace, _) in windows {
            s = reduce(&s, &ShellEvent::WindowOpened { id: id.to_string(), app: app.to_string(), title: title.to_string(), workspace: *workspace });
        }
        for (id, ..) in windows.iter().rev() {
            s = reduce(&s, &ShellEvent::WindowFocused { id: id.to_string() });
        }
        for (id, .., minimized) in windows {
            if *minimized {
                s = reduce(&s, &ShellEvent::WindowMinimized { id: id.to_string() });
            }
        }
        s
    }

    /// #EDE-65: the current workspace's windows (minimized included) with
    /// their app icons, the second most recent selected, its title under the tiles.
    #[test]
    fn the_feed_lists_this_workspaces_windows_with_icons_and_the_selected_title() {
        let s = opened(&[("w1", "firefox", "Firefox", 0, false), ("w2", "nemo", "Files", 0, true), ("w3", "code", "Elsewhere", 1, false)]);
        let s = reduce(&s, &ShellEvent::AltTabNext);
        let feed = switcher_feed(&s, |app| format!("icon:{app}"), |id| format!("thumb:{id}"));
        assert_eq!(feed["title"], "Files");
        let windows = feed["windows"].as_array().unwrap();
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[1], json!({"id": "w2", "title": "Files", "app": "nemo", "icon": "icon:nemo", "minimized": true, "selected": true, "thumbnail": "thumb:w2"}));
        assert_eq!(windows[0]["selected"], false);
    }

    #[test]
    fn alt_grave_lists_only_the_focused_apps_windows() {
        let s = opened(&[("w1", "firefox", "One", 0, false), ("w2", "nemo", "Files", 0, false), ("w3", "firefox", "Two", 0, false)]);
        let s = reduce(&s, &ShellEvent::AltTabNextApp);
        let feed = switcher_feed(&s, |_| String::new(), |_| String::new());
        let ids: Vec<&str> = feed["windows"].as_array().unwrap().iter().filter_map(|w| w["id"].as_str()).collect();
        assert_eq!(ids, ["w1", "w3"]);
        assert_eq!(feed["title"], "Two");
    }

    /// #EDE-65: Mint's hover: the pointer entering a tile selects it, the
    /// title under the tiles follows; the selected tile, a window not listed
    /// or a closed switcher change nothing.
    #[test]
    fn a_hovered_tile_becomes_the_selection() {
        let s = opened(&[("w1", "firefox", "Firefox", 0, false), ("w2", "nemo", "Files", 0, false), ("w3", "code", "Code", 0, false), ("w4", "code", "Elsewhere", 1, false)]);
        assert_eq!(alt_tab_hover(&s, "w3"), None, "closed");
        let s = reduce(&s, &ShellEvent::AltTabNext);
        assert_eq!(s.alt_tab.selected, 1);
        let event = alt_tab_hover(&s, "w3").expect("w3 is the third tile");
        assert_eq!(event, ShellEvent::AltTabSelect { index: 2 });
        let s = reduce(&s, &event);
        let feed = switcher_feed(&s, |_| String::new(), |_| String::new());
        let selected: Vec<bool> = feed["windows"].as_array().unwrap().iter().map(|w| w["selected"] == true).collect();
        assert_eq!(selected, [false, false, true]);
        assert_eq!(feed["title"], "Code");
        assert_eq!(alt_tab_hover(&s, "w3"), None, "already selected");
        assert_eq!(alt_tab_hover(&s, "w4"), None, "another workspace");
        assert_eq!(alt_tab_hover(&s, "w9"), None, "unknown");
    }

    /// The factory switcher's tile calls it with its window on `on.hover`.
    #[test]
    fn the_switcher_tile_selects_on_hover() {
        let doc = uicore_docs::DOCS.iter().find(|d| d.name == "switcher").expect("the switcher document");
        let parsed: Value = doc.json();
        let tile = parsed["root"]["children"].as_array().unwrap().iter().find(|c| c["id"] == "tile").expect("the tile");
        assert_eq!(tile["on"]["hover"], json!({"functionCall": {"@call": "ede_alt_tab_select", "args": {"window": {"@path": "id"}}}}));
    }

    #[test]
    fn the_popup_hugs_its_tiles_and_stays_on_the_output() {
        assert_eq!(switcher_width(3, 1920), 2 * 16 + 3 * 208 + 2 * 8);
        assert_eq!(switcher_width(0, 1920), switcher_width(1, 1920));
        assert_eq!(switcher_width(40, 1280), 1280 - 32);
    }

    /// #EDE-225: the switcher opens on the output with most of the focused
    /// window, as the compositor places it, and is sized to that one.
    #[test]
    fn the_switcher_is_sized_to_the_focused_windows_output() {
        let outputs = [(0, 0, 1920, 1080), (1920, 0, 1280, 800)];
        let window = |x: i32, width: i32| ede_protocol::WindowInfo { id: ede_protocol::WindowId::new("w").unwrap(), title: None, app_id: None, x, y: 100, width, height: 600, focused: true, fullscreen: false, protected: false, x11: false, workspace: 0, minimized: false, maximized: false, space: None };
        assert_eq!(output_of(&outputs, &window(100, 800)), Some(outputs[0]));
        assert_eq!(output_of(&outputs, &window(1920, 1280)), Some(outputs[1]));
        // Across the edge: where most of it is.
        assert_eq!(output_of(&outputs, &window(1800, 800)), Some(outputs[1]));
        assert_eq!(output_of(&outputs, &window(-5000, 800)), None);
        // The shell's overlays are named as the compositor lets them above a fullscreen window.
        for name in ["switcher", "expo", "scale", "osd"] {
            assert!(ede_protocol::ABOVE_FULLSCREEN_NAMESPACES.contains(&uicore_gpui::layer_namespace(&uicore_gpui::LayerShellHost::default().namespace, name).as_str()), "{name}");
        }
    }
}

#[cfg(test)]
mod hosted_reports {
    use super::*;

    /// #EDE-65: the taskbar's hover previews report themselves empty (50 px)
    /// and again with their windows in one tick; the popup opens once, at
    /// the second size.
    #[test]
    fn only_each_surfaces_last_report_counts() {
        let layer = |owner: &str| serde_json::from_value::<Layer>(json!({"layer": 1, "kind": "popover", "owner": owner, "root": 0, "position": "anchored", "class": "overlay", "frames": []})).unwrap();
        let reports = vec![("taskbar".to_string(), vec![layer("empty")]), ("sidebar".to_string(), vec![]), ("taskbar".to_string(), vec![layer("filled")])];
        let latest = latest_reports(reports);
        assert_eq!(latest.len(), 2);
        assert_eq!(latest[0].0, "sidebar");
        assert_eq!((latest[1].0.as_str(), latest[1].1[0].owner.as_str()), ("taskbar", "filled"));
    }
}

#[cfg(test)]
mod layer_sizes {
    use super::*;

    fn factory(name: &str) -> Document {
        uicore_gpui::inline::document(uicore_docs::DOCS.iter().find(|d| d.name == name).expect(name))
    }

    fn size(header: &SurfaceHeader) -> (Option<u32>, Option<u32>, i32) {
        let SurfaceHeader::Layer { size, exclusive_zone, .. } = header else { panic!("a layer header") };
        (size.width, size.height, *exclusive_zone)
    }

    /// #EDE-65: the stretched bars follow the output: a VM window resized
    /// to 1920×1080 gives the taskbar the whole bottom edge, the sidebar the
    /// rail's 56 px standing on it, the ground the full output (eDE 0.2.6
    /// kept the sizes of the start, so the taskbar sat centred with its
    /// first button mid-bar). The rail is the sidebar for good: no 272.
    #[test]
    fn the_bars_take_the_outputs_size() {
        let (taskbar, sidebar, ground) = (factory("taskbar"), factory("sidebar"), factory("ground"));
        for out in [(1280, 720), (1920, 1080)] {
            assert_eq!(size(&layer_header(&taskbar, out)), (Some(out.0), Some(48), 48), "{out:?}");
            assert_eq!(size(&layer_header(&sidebar, out)), (Some(56), Some(out.1 - 48), 56), "{out:?}");
            assert_eq!(size(&layer_header(&ground, out)), (Some(out.0), Some(out.1), -1), "{out:?}");
        }
        assert_ne!(layer_header(&taskbar, (1280, 720)), layer_header(&taskbar, (1920, 1080)), "a new size is a relayout");
    }

    /// #EDE-168: on a 1280×800 output at scale 1.5 (854×534 logical, the
    /// VM's boot size) the sound applet's 600 px reached past the top of the
    /// screen. Every taskbar popup stands on its button within the room
    /// above the taskbar (less the panel's margin), its root no taller; one
    /// that fits keeps its size, and the start menu (620) is held too.
    #[test]
    fn a_taskbar_popup_never_leaves_the_work_area() {
        let room = 534 - 48 - shell_layout().panel_margin;
        let popup_height = |name: &str, out_h: u32| {
            let doc = factory(name);
            let mut header = doc.surface.clone();
            taskbar_popup_room(&mut header, (1920, out_h));
            let drawn = as_drawn(&doc, header.clone());
            let SurfaceHeader::Popup { popup, .. } = header else { panic!("{name}: a popup header") };
            let ede_ui::Body::Nested(root) = &drawn.body else { panic!("{name}: a nested body") };
            let style = root.style.clone().unwrap_or_default();
            assert_eq!(style.get("maxHeight").and_then(Value::as_u64), Some(popup.size.height as u64), "{name}: the root is never taller than its window");
            if let Some(h) = style.get("height").and_then(Value::as_u64) {
                assert!(h <= popup.size.height as u64, "{name}: root {h} in a {} window", popup.size.height);
            }
            popup.size.height
        };
        assert_eq!(popup_height("applet-sound", 534), room);
        assert_eq!(popup_height("start-menu", 534), room);
        assert_eq!(popup_height("applet-battery", 534), 360, "a popup that fits keeps its size");
        assert_eq!(popup_height("applet-network", 534), room, "#EDE-235: the network applet's 520 is held too");
        assert_eq!(popup_height("applet-network", 800), 520);
        assert_eq!(popup_height("applet-sound", 800), 600, "1280×800 logical has room for all of it");
        for d in uicore_docs::DOCS.iter() {
            let doc = factory(d.name);
            if matches!(&doc.surface, SurfaceHeader::Popup { popup, .. } if popup.parent == "taskbar") {
                assert!(popup_height(d.name, 534) <= room, "{}", d.name);
            }
        }
    }

    /// #EDE-259: on that 854×534 output the 860×620 start menu was cut off
    /// at the right and the bottom. Like Cinnamon's, every taskbar popup
    /// shrinks to the work area: no wider than the output less the panel's
    /// margin either side, its root no wider than its window, so its columns
    /// shrink and the contents scroll. 1280×720 keeps the menu's width.
    #[test]
    fn a_taskbar_popup_fits_a_small_output() {
        let margin = shell_layout().panel_margin;
        let sized = |name: &str, out: (u32, u32)| {
            let doc = factory(name);
            let mut header = doc.surface.clone();
            taskbar_popup_room(&mut header, out);
            let drawn = as_drawn(&doc, header.clone());
            let SurfaceHeader::Popup { popup, .. } = header else { panic!("{name}: a popup header") };
            let ede_ui::Body::Nested(root) = &drawn.body else { panic!("{name}: a nested body") };
            let style = root.style.clone().unwrap_or_default();
            if let Some(w) = style.get("width").and_then(Value::as_u64) {
                assert!(w <= popup.size.width as u64, "{name}: root {w} in a {} window", popup.size.width);
            }
            (popup.size.width, popup.size.height)
        };
        assert_eq!(sized("start-menu", (854, 534)), (854 - 2 * margin, 534 - 48 - margin));
        assert_eq!(sized("start-menu", (1280, 720)), (860, 620), "1280×720 has room for all of it");
        assert_eq!(sized("sessions-panel", (854, 534)).0, 460, "a popup that fits keeps its width");
        for d in uicore_docs::DOCS.iter() {
            let doc = factory(d.name);
            if matches!(&doc.surface, SurfaceHeader::Popup { popup, .. } if popup.parent == "taskbar") {
                let (w, h) = sized(d.name, (854, 534));
                assert!(w <= 854 - 2 * margin && h <= 534 - 48 - margin, "{}: {w}×{h}", d.name);
            }
        }
    }

    /// #EXP-2024 (section 8, 6d): the pinned view of an agent space is an
    /// on-demand top layer at the bottom right, above the taskbar, that
    /// never takes the keyboard. #EDE-266: the taskbar's computer-use status
    /// icon is a Menu (the applet menu on a right click) around the button
    /// its popup document anchors at, and the taskbar may call `ede_cu`.
    #[test]
    fn the_space_view_takes_no_keyboard_and_the_tray_is_an_applet() {
        let view = factory(spaces::VIEW);
        let SurfaceHeader::Layer { layer, anchor, keyboard, on_demand, functions, margin, .. } = &view.surface else { panic!("a layer") };
        assert_eq!((*layer, *keyboard, *on_demand), (DocLayer::Top, ede_ui::surface::Keyboard::None, true));
        assert_eq!(anchor, &vec![ede_ui::surface::Edge::Bottom, ede_ui::surface::Edge::Right]);
        assert_eq!((margin.right, margin.bottom), (shell_layout().panel_margin, shell_layout().panel_margin), "beside the edge, above the taskbar's zone");
        assert_eq!(functions, &vec!["ede_space".to_string()]);
        let taskbar: Value = uicore_docs::doc("taskbar").unwrap().json();
        fn node<'a>(n: &'a Value, id: &str) -> Option<&'a Value> {
            if n["id"] == id {
                return Some(n);
            }
            match n {
                Value::Object(m) => m.values().find_map(|v| node(v, id)),
                Value::Array(a) => a.iter().find_map(|v| node(v, id)),
                _ => None,
            }
        }
        assert!(node(&taskbar["root"], spaces::LIST).is_none(), "the Agent spaces list is gone");
        let menu = node(&taskbar["root"], "cu_tray_menu").expect("the computer-use applet menu");
        assert_eq!((menu["component"].as_str(), menu["props"]["openOn"].as_str()), (Some("Menu"), Some("contextmenu")));
        let button = node(&taskbar["root"], "cu_tray").expect("the computer-use status icon");
        assert_eq!(button["props"]["label"], "Computer use");
        let tray = factory(spaces::TRAY);
        let SurfaceHeader::Popup { popup, functions, .. } = &tray.surface else { panic!("a popup") };
        assert_eq!((popup.parent.as_str(), popup.anchor.as_str()), ("taskbar", "cu_tray"));
        assert_eq!(functions, &vec!["ede_cu".to_string()]);
        let functions: Vec<&str> = taskbar["surface"]["functions"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert!(functions.contains(&"ede_cu"));
    }

    /// #EDE-266: the tray's presses (spaces.json `tray` values).
    #[test]
    fn the_trays_presses_parse() {
        use spaces::TrayPress;
        use uicore_model::spaces::SpaceControl;
        let press = |v: Value| spaces::tray_press(&v).map(|(p, s)| (p, s));
        assert_eq!(press(json!({"action": "takeOver", "space": "space-1"})), Some((TrayPress::Control(SpaceControl::Person), Some("space-1".into()))));
        assert_eq!(press(json!({"action": "handBack"})), Some((TrayPress::Control(SpaceControl::Agent), None)));
        assert_eq!(press(json!({"action": "pause"})), Some((TrayPress::Control(SpaceControl::Paused), None)));
        assert_eq!(press(json!({"action": "resume"})), Some((TrayPress::Control(SpaceControl::Agent), None)));
        assert_eq!(press(json!({"action": "select", "space": "space-2"})), Some((TrayPress::Select("space-2".into()), Some("space-2".into()))));
        assert_eq!(press(json!({"action": "select"})), None, "a pick names its space");
        assert_eq!(press(json!({"action": "stop"})), Some((TrayPress::Stop, None)));
        assert_eq!(press(json!({"action": "nonsense"})), None);
        // #EDE-272: the popup's button names the session, the menu's item carries it.
        assert_eq!(press(json!({"action": "allow", "session": "s1"})), Some((TrayPress::Allow("s1".into()), None)));
        assert_eq!(press(json!({"action": "allow:s2"})), Some((TrayPress::Allow("s2".into()), None)));
        assert_eq!(press(json!({"action": "allow"})), None, "Allow names its session");
        assert_eq!(press(json!({"action": "allow:"})), None);
    }

    /// #EDE-273: System Settings is one normal window of the shell
    /// (cinnamon-settings): eDE's frame, its own taskbar entry under the
    /// start menu's System Settings, the grid's header and body filling it.
    #[test]
    fn settings_is_one_window() {
        let settings = factory("settings");
        let SurfaceHeader::Window { window, functions } = &settings.surface else { panic!("a window") };
        assert_eq!((window.title.as_str(), window.app_id.as_str(), window.size.width, window.size.height), ("System Settings", crate::system_settings::APP_ID, 1040, 700));
        assert!(functions.iter().any(|f| f == "ede_settings_nav"));
        assert!(builtin_apps(true).iter().any(|a| a.id == format!("{}.desktop", crate::system_settings::APP_ID)), "the start menu's entry names the window");
        let doc: Value = uicore_docs::doc("settings").unwrap().json();
        assert_eq!(doc["root"]["style"]["width"], "100%", "the document follows the window's size");
    }

    /// The taskbar opens before the sidebar: its exclusive zone takes the
    /// whole bottom edge, the rail stands on it.
    #[test]
    fn the_taskbar_maps_before_the_rail() {
        assert!(layer_rank(&factory("taskbar")) < layer_rank(&factory("sidebar")));
    }

    /// eDE draws the rail of the shared sidebar: the chats, none of the
    /// IDE's header, nav or corners (#EDE-65).
    #[test]
    fn the_sidebar_is_the_chats_rail() {
        let mut doc = factory("sidebar");
        drawn(&mut doc);
        let ede_ui::Body::Nested(tree) = &doc.body else { panic!("a nested sidebar") };
        let text = serde_json::to_string(tree).unwrap();
        for id in uicore_gpui::inline::EDE_SIDEBAR_PRUNED {
            assert!(!text.contains(&format!("\"id\":\"{id}\"")), "{id} is drawn");
        }
        assert!(text.contains("\"id\":\"chat_row\""));
        // Lane I's final call: a chat opens with ede_chat {sessionId}.
        let doc: Value = uicore_docs::doc("sidebar").unwrap().json();
        let row = serde_json::to_string(&doc["root"]).unwrap();
        assert!(row.contains(r#""functionCall":{"@call":"ede_chat","args":{"sessionId":{"@path":"id"}}}"#), "the rail's chat calls ede_chat {{sessionId}}");
    }

}

#[cfg(test)]
mod previews {
    use super::{popups_to_dismiss, previews_after, previews_feed, previews_under_a_menu};

    /// #EDE-237: the Exponential button's thumbnails wear each page's icon
    /// and title (the holder's window metadata); another app's its app icon.
    #[test]
    fn exponential_thumbnails_wear_their_pages_icon_and_title() {
        use uicore_model::{ShellState, ShellWindow};
        let window = |id: &str, app: &str, title: &str| ShellWindow { id: id.into(), app: app.into(), title: title.into(), focused: false, workspace: 0, minimized: false };
        let mut model = ShellState::default();
        model.windows = vec![window("w1", "at.exponential", "Desktop"), window("w2", "at.exponential", "EXP-1 Crash"), window("w3", "nemo", "Files"), window("w4", "at.exponential", "Unknown")];
        model.taskbar.previews = Some("at.exponential".into());
        let meta = |title: &str, icon: &str| api::bus::WindowMeta { key: title.into(), kind: "k".into(), icon: icon.into(), title: title.into(), team: Some("Exponential".into()), team_id: None };
        let holder = super::holder_windows::HolderWindows { app_id: "at.exponential".into(), windows: vec![meta("Desktop", "nav-boards"), meta("EXP-1 Crash", "ui-issue")] };
        let feed = previews_feed(&model, &holder, |app| format!("icon:{app}"), |id| format!("thumb:{id}"));
        let windows = feed["windows"].as_array().unwrap();
        assert_eq!(windows.len(), 3, "only the hovered app's windows");
        assert_eq!(windows[0]["concept"], "nav-boards");
        assert_eq!(windows[0]["title"], "Desktop");
        assert_eq!(windows[0]["team"], "Exponential");
        assert_eq!(windows[1]["concept"], "ui-issue");
        assert_eq!(windows[1]["thumbnail"], "thumb:w2");
        // No page told for it: the app's icon alone.
        assert_eq!((windows[0]["page"].as_bool(), windows[2]["page"].as_bool()), (Some(true), Some(false)));
        assert_eq!(windows[2]["icon"], "icon:at.exponential");
        assert_eq!(previews_feed(&ShellState::default(), &holder, |_| String::new(), |_| String::new())["windows"], serde_json::json!([]));
    }

    /// #EDE-252: a menu of the taskbar opening closes the hover previews.
    #[test]
    fn a_menu_closes_the_hover_previews() {
        let layer = |kind: &str, owner: &str| -> eui_core::surface::Layer { serde_json::from_value(serde_json::json!({"layer": 1, "kind": kind, "owner": owner, "root": 0, "hosted": true, "position": "top", "class": "overlay", "frames": []})).unwrap() };
        let previews = layer("Popover", "previews.nemo");
        assert!(previews_under_a_menu(&[previews.clone()]).is_empty(), "previews alone stay");
        assert_eq!(previews_under_a_menu(&[previews, layer("Menu", "group.nemo"), layer("Popover", "agent_spaces")]), vec!["previews.nemo".to_string()]);
    }

    /// #EDE-89: a press on Start while a preview is open: the compositor's
    /// `popups.dismiss` for that press reaches the shell after the press
    /// opened the start menu (the main loop drains it up to 100 ms later).
    /// The menu opened after the press and stays; a popup open before it goes.
    #[test]
    fn a_press_that_opens_the_start_menu_does_not_dismiss_it() {
        let pressed_at = 1_000;
        let open = vec![("start-menu".to_string(), Some(pressed_at + 12)), ("needs-you".to_string(), Some(pressed_at - 4_000))];
        assert_eq!(popups_to_dismiss(&open, pressed_at), vec!["needs-you".to_string()]);
        // Escape in the open menu (or a press outside it) still closes it.
        assert_eq!(popups_to_dismiss(&open, pressed_at + 500), vec!["start-menu".to_string(), "needs-you".to_string()]);
        // A popup the shell has no stamp for goes, as before.
        assert_eq!(popups_to_dismiss(&[("menu".to_string(), None)], pressed_at), vec!["menu".to_string()]);
    }

    /// #EDE-75: leaving a taskbar button closes its previews (the capture
    /// thread stops with them); the next button's stay when its open came first.
    #[test]
    fn a_close_hides_only_its_own_apps_previews() {
        let (firefox, nemo) = (Some("firefox".to_string()), Some("nemo".to_string()));
        assert_eq!(previews_after(&None, firefox.clone(), true), Some(firefox.clone()));
        assert_eq!(previews_after(&firefox, firefox.clone(), false), Some(None));
        assert_eq!(previews_after(&nemo, firefox.clone(), false), None);
        assert_eq!(previews_after(&firefox, None, false), Some(None));
    }

    /// The factory taskbar tells the shell with an event (its context gets `open`).
    #[test]
    fn the_factory_previews_change_is_an_event() {
        let doc = uicore_docs::doc("taskbar").unwrap().json().to_string();
        assert!(doc.contains(r#""name":"ede_previews""#), "the event");
        assert!(!doc.contains(r#""@call":"ede_previews""#), "no function call: its args never get `open`");
    }
}

#[cfg(test)]
mod loaded_frame {
    use super::*;

    fn factory(name: &str) -> Document {
        uicore_gpui::inline::document(uicore_docs::DOCS.iter().find(|d| d.name == name).expect(name))
    }

    /// #EDE-110: an edit to a closed popup or on-demand layer is proven by
    /// loading it; the supervisor rolled it back after 2 s for want of a frame.
    #[test]
    fn a_closed_popup_or_on_demand_layer_reports_the_document_loaded() {
        for name in ["start-menu", "switcher", "settings"] {
            let doc = factory(name);
            assert_eq!(loaded_frame(false, &doc), Some(FrameLine::line_for_digest(name, &doc.digest)), "{name}");
            assert_eq!(loaded_frame(true, &doc), None, "{name}: an open surface reports its drawn frame");
        }
        assert_eq!(loaded_frame(false, &factory("taskbar")), None, "a layer that opens reports its drawn frame");
    }
}

#[cfg(test)]
mod ide_launch {
    use super::*;

    /// #EDE-65: a cold start hands the item's link over on the command line.
    #[test]
    fn a_cold_start_passes_the_link_as_the_entrys_url() {
        let app = apps::App {
            id: "at.exponential.desktop".into(),
            name: "Exponential".into(),
            comment: String::new(),
            exec: "/opt/Exponential/exponential-desktop".into(),
            exec_url: Some("/opt/Exponential/exponential-desktop %U".into()),
            icon: None,
            keywords: String::new(),
            categories: vec![],
            wm_class: None,
            steam_app: None,
        };
        assert_eq!(ide_command(&app, Some("exponential://issue/ACME-12")), "/opt/Exponential/exponential-desktop 'exponential://issue/ACME-12'");
        assert_eq!(ide_command(&app, None), "/opt/Exponential/exponential-desktop");
        let no_urls = apps::App { exec_url: None, ..app };
        assert_eq!(ide_command(&no_urls, Some("exponential://issue/ACME-12")), "/opt/Exponential/exponential-desktop");
    }

    /// #EDE-227: with the component, saved pins of the person's own app
    /// entries become the component's, and a standalone app's window (app
    /// id `at.exponential`) groups with them: one Exponential button.
    #[test]
    fn the_persons_app_pins_and_windows_are_the_components_one_button() {
        let standalone = vec!["appimagekit_1234-Exponential".to_string(), "at.exponential".to_string(), "exponential".to_string()];
        let mut model = ShellState::default();
        for pin in ["firefox", "exponential", "appimagekit_1234-Exponential.desktop", "at.exponential", "nemo"] {
            model = reduce(&model, &ShellEvent::TaskbarPin { app: pinned_as(pin, &standalone) });
        }
        assert_eq!(model.taskbar.pinned, ["firefox", "at.exponential", "nemo"]);
        model = reduce(&model, &ShellEvent::WindowOpened { id: "w1".into(), app: "at.exponential".into(), title: "Exponential".into(), workspace: 0 });
        let buttons = uicore_model::taskbar_buttons(&model);
        assert_eq!(buttons.iter().map(|b| (b.app.as_str(), b.windows.clone())).collect::<Vec<_>>(), [("firefox", vec![]), ("at.exponential", vec!["w1".to_string()]), ("nemo", vec![])]);
        assert_eq!(pinned_as("firefox.desktop", &standalone), "firefox.desktop", "another app's pin stays");
        // Not in the favourites either: the start menu lists no entry of it.
        let apps = [uicore_model::StartMenuApp { id: "firefox".into(), name: "Firefox".into(), comment: String::new(), categories: vec![], keywords: String::new() }];
        let mut menu = model.clone();
        menu.start_menu.category = "favourites".into();
        assert_eq!(uicore_model::start_menu_apps(&menu, &apps, &model.taskbar.pinned).into_iter().map(|a| a.id).collect::<Vec<_>>(), ["firefox"]);
    }
}

#[cfg(test)]
mod start_menu_document {
    use serde_json::Value;

    fn node<'a>(n: &'a Value, id: &str) -> Option<&'a Value> {
        if n.get("id").and_then(Value::as_str) == Some(id) {
            return Some(n);
        }
        match n {
            Value::Object(m) => m.values().find_map(|v| node(v, id)),
            Value::Array(a) => a.iter().find_map(|v| node(v, id)),
            _ => None,
        }
    }

    fn call(n: &Value) -> (&str, &Value) {
        let f = &n["on"]["press"]["functionCall"];
        (f["@call"].as_str().unwrap_or_default(), &f["args"])
    }

    /// #EDE-258: Cinnamon's layout: the favourites and power on the left,
    /// the search on top, the categories and the contents; no switcher.
    #[test]
    fn the_start_menu_is_cinnamons_with_the_tree_and_no_switcher() {
        let doc: Value = uicore_docs::doc("start-menu").expect("the start menu").json();
        let root = &doc["root"];
        let side = node(root, "side").expect("the left sidebar");
        assert_eq!(node(side, "favourites").unwrap()["template"]["path"], "/shell/startMenu/favourites");
        for (id, f) in [("lock", "ede_lock"), ("logout", "ede_logout")] {
            assert_eq!(call(node(side, id).expect(id)).0, f, "{id}");
        }
        assert!(node(side, "power").is_some());
        assert!(node(node(root, "top").unwrap(), "search").is_some(), "the search on top");
        assert_eq!(node(root, "categories").unwrap()["template"]["path"], "/shell/startMenu/nav/categories");
        assert_eq!(node(root, "rows").unwrap()["template"]["path"], "/shell/startMenu/nav/rows");
        let hover = &node(root, "category").unwrap()["on"]["hover"]["functionCall"];
        assert_eq!((hover["@call"].as_str(), &hover["args"]), (Some("ede_start_menu"), &serde_json::json!({"category": {"@path": "id"}})), "a hovered category switches");
        let (f, args) = call(node(root, "row_item").unwrap());
        assert_eq!((f, &args["open"]), ("ede_start_menu", &serde_json::json!({"@path": "id"})));
        let text = doc.to_string();
        assert!(!text.contains("ede_switcher") && !text.contains("/shell/switcher"), "no team switcher (#EDE-238)");
        assert!(text.contains("/shell/startMenu/assistant/messages"), "the Assistant pane");
    }

    /// #EDE-214: every tray popup's settings link opens Settings on its own
    /// page (one the nav lists, its group open), never the first one.
    #[test]
    fn the_tray_popups_settings_links_name_their_page() {
        let settings = uicore_docs::doc("settings").expect("settings").json().to_string();
        for (applet, id, page) in [("applet-sound", "sound_settings", "sound"), ("applet-network", "network_settings", "network"), ("applet-battery", "power_settings", "power-management"), ("applet-bluetooth", "bluetooth_settings", "bluetooth"), ("applet-calendar", "datetime_settings", "date-and-time")] {
            let doc: Value = uicore_docs::doc(applet).expect(applet).json();
            let (f, args) = call(node(&doc["root"], id).expect(id));
            assert_eq!((f, args["name"].as_str(), args["page"].as_str()), ("ede_open_document", Some("settings"), Some(page)), "{applet}");
            let mut nav = crate::system_settings::Nav::default();
            let module = nav.step(&uicore_model::settings::SettingsEvent::Link { target: page.into() }).state.module;
            assert!(!module.is_empty(), "{page} lands on a module");
            assert!(settings.contains(&format!("\"b\":\"{module}\"")), "{module} is a module of the window");
        }
    }

    /// #EDE-92: the rows draw the shell's selection with the Row's selected
    /// state; Enter in the search box asks the shell to open it, the
    /// composer asks the Assistant.
    #[test]
    fn the_start_menu_selects_a_row_and_enter_opens_it() {
        let source = uicore_docs::doc("start-menu").expect("the start menu").source;
        let doc: Value = uicore_docs::doc("start-menu").expect("the start menu").json();
        let root = &doc["root"];
        for id in ["row_item", "category_row"] {
            assert_eq!(node(root, id).expect(id)["props"]["selected"], serde_json::json!({"@path": "selected"}), "{id}");
        }
        let submit = &node(root, "search").expect("the search box")["on"]["submit"]["functionCall"];
        assert_eq!((submit["@call"].as_str(), &submit["args"]), (Some("ede_start_menu"), &serde_json::json!({"activate": true})));
        let ask = &node(root, "assistant_composer").expect("the composer")["on"]["submit"]["functionCall"];
        assert_eq!(ask["args"]["ask"], serde_json::json!({"@path": "/shell/startMenu/ask"}));
        assert!(source.lines().any(|l| l.starts_with('#') && l.contains("#EDE-92")), "the document says what the keys do");
        assert_eq!(doc.to_string().matches("\"activate\"").count(), 1, "only the search box opens the selection");
    }
}


#[cfg(test)]
mod failed_actions {
    #[test]
    fn a_failed_send_toasts_the_fixture_s_title_with_the_server_s_sentence() {
        let toast = super::failed_toast("send", "the message", " The session ended. ", 7);
        assert_eq!((toast.summary.as_str(), toast.body.as_str(), toast.app.as_str()), ("Couldn't send the message", "The session ended.", "Exponential"));
        assert!(toast.expires_at.is_some(), "a toast, not a sticky notification");
        let dump = super::failed_toast("answer", "the question", "{\"code\":\"CONFLICT\"}", 7);
        assert_eq!((dump.summary.as_str(), dump.body.as_str()), ("Couldn't answer the question", ""));
        assert_ne!(toast.id, dump.id, "each its own id");
    }

    /// #EDE-182: a cancelled polkit prompt and timedated's refusal reach
    /// the failed action's toast; a change timedated took toasts nothing.
    #[test]
    fn a_refused_date_and_time_change_toasts() {
        use crate::settings_pages::{
            apply_time,
            fake_timedated::{serve, Answer},
            TimeChange,
        };
        let toast_for = |answer: Answer, change: TimeChange| {
            let (_bus, _daemon, client, _) = serve(answer);
            super::time_toast(&change, &apply_time(&client, &change), 7).map(|t| (t.summary, t.body))
        };
        assert_eq!(toast_for(Answer::Cancelled, TimeChange::Ntp(true)), Some(("Couldn't turn on automatic time".into(), "Permission denied".into())));
        assert_eq!(toast_for(Answer::Cancelled, TimeChange::Ntp(false)), Some(("Couldn't turn off automatic time".into(), "Permission denied".into())));
        assert_eq!(toast_for(Answer::Fails, TimeChange::TimeZone("Asia/Tokyo".into())), Some(("Couldn't change the time zone".into(), "Invalid or not installed time zone 'Asia/Tokyo'".into())));
        assert_eq!(toast_for(Answer::Takes, TimeChange::Ntp(true)), None);
    }
}

#[cfg(test)]
mod keep_connected_tests {
    use super::*;

    /// #EDE-225: a connection that drops once is made again (after the
    /// first wait), and the session runs on it; a session that says stop
    /// ends the loop.
    #[test]
    fn a_dropped_connection_is_made_again() {
        let mut tries = vec![Err("no socket"), Ok(1), Ok(2)].into_iter();
        let (mut sleeps, mut ran) = (Vec::new(), Vec::new());
        keep_connected("test", || tries.next().expect("no connect after the stop"), |d| sleeps.push(d), |c: i32| {
            ran.push(c);
            // The first connection drops; the shell's channel goes on the second.
            c == 1
        });
        assert_eq!(ran, [1, 2], "the session ran again on the new connection");
        assert_eq!(sleeps, [Duration::from_secs(1), Duration::from_secs(1)]);
    }

    /// No compositor for a while: the waits double to 10 s, and a
    /// connection that held starts them at 1 s again.
    #[test]
    fn the_waits_back_off_and_reset_once_a_connection_held() {
        let mut tries = vec![Err("down"), Err("down"), Err("down"), Err("down"), Err("down"), Ok(1), Err("down"), Ok(2)].into_iter();
        let mut sleeps = Vec::new();
        keep_connected("test", || tries.next().unwrap(), |d| sleeps.push(d.as_secs()), |c: i32| c == 1);
        assert_eq!(sleeps, [1, 2, 4, 8, 10, 1, 1]);
    }
}
