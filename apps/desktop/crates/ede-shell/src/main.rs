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
//! and apps, chat, Alt-Tab; #EDE-65: `/shell/chats`, the rail's chats), `/apps` + `/query` (the launcher), `/switcher`
//! (Alt+Tab with thumbnails), `/previews` (a taskbar group's windows with
//! thumbnails), `/osd`, `/notifications` (#EDE-65: with the accounts'
//! unread inbox merged in), `/needsYou`, `/tray`, `/traymenu`,
//! `/services/<domain>`, `/outputs`, `/config`, `/user`. The host functions a
//! document may call are the `ede_*` names its `surface.functions` lists
//! (#EDE-8's Tier C rule, checked again here): `ede_launch {command}`,
//! `ede_focus {window}`, `ede_close {window}`, `ede_minimize {window}`,
//! `ede_lock`, `ede_logout`, `ede_power {action}`, `ede_select {window}`,
//! `ede_alt_tab_select {window}` (#EDE-65: a hovered Alt-Tab tile),
//! `ede_notify_open {id}`, `ede_notify_close {id, toast?}`, `ede_notify_action
//! {id, key}`, `ede_notify_clear` (an inbox entry's id is `inbox:<id>`),
//! `ede_needs_you_open {id}` (#EDE-65: both hand off to the Exponential app),
//! `ede_tray {service}`, `ede_tray_menu {service, id}`, `ede_open_document
//! {name}`, `ede_close_document {name}`, `ede_service {domain, action,
//! args}`, `ede_output {name, mode?, scale?}`, `ede_config {…}`, `ede_settings
//! {page, key, value}` (#EDE-47: the pages `/pages` feeds), and the
//! shell model's: `ede_pin {app}` / `ede_unpin {app}`,
//! `ede_taskbar {app}` (Mint's click), `ede_taskbar_scroll {app, direction}`,
//! `ede_previews {app|null}` (the hover previews), `ede_start_menu {open?,
//! query?, category?}`, `ede_chat` (toggle; #EDE-65: `{sessionId}`
//! opens a chat of the rail), `ede_workspace {index}`; #EDE-65: `ede_issue
//! {identifier, source, sourceId}` (a start-menu issue, opened in the
//! Exponential app on its team or Local desktop); #EDE-56: `ede_switcher {action:
//! pick|new|share|connect, kind?, id?}` (the Local desktops and the connected
//! accounts' teams, `/shell/switcher/sections`), `ede_connect {action:
//! start|code|open|cancel, instance?}` (Connect Exponential: `/connecting`,
//! `/accounts`; `exponential-shell connect <url>` is the scheme handler's
//! command, forwarding the browser's callback to the running shell);
//! #EDE-57: `ede_share {action: start|run|cancel, name?, account?}` (Share a
//! Local desktop as a team: `/sharing`); #EDE-58: `ede_chat {action: send,
//! text}` (the desktop's own agent on a Local desktop: `/chatting`);
//! #EXP-2024: `/spaces` (the machine's agent spaces: the taskbar's Agent
//! spaces button and list) and `/spaceView` (the ONE pinned view),
//! `ede_space {session | null, size?}` (pins a session's view, sizes it,
//! or unpins) and `ede_space_close {space}` (ends a space, not its session).
//!
//! `exponential-shell [--windowed] [--popup] [--open <document>] [--exit-after <ms>]`
//!   --windowed    normal windows instead of layer surfaces (macOS, any desktop)
//!   --popup NAME  open a popup document right away (the goldens; `all` = every one)
//!   --open NAME   open an on-demand layer document right away (the goldens)
//!   --exit-after  quit after this many milliseconds (the headless check)
//! `EDE_SHELL_DEMO=1` feeds fixed sample data (apps, windows, chats) instead
//! of the machine's, so a golden never depends on what is installed. Since
//! #EDE-65 the sidebar is the IDE's 56 px icon rail for good (no 272 px
//! column, no toggle): the chats only, standing on the taskbar, which holds
//! the start, chat and new-issue buttons; a chat of the rail opens in eDE's
//! chat (`exponential://session/<id>` only when the team's chat cannot),
//! `ede_launch {new: issue}` hands off (`exponential://new-issue`);
//! `EDE_SHELL_DEMO_TRAY=1` adds the demo's inbox entries and Needs you;
//! `EDE_SHELL_DEMO_SPACES=list|view` three sample agent spaces, their list
//! open or the first one's view pinned (#EXP-2024).

mod accounts;
mod chat;
mod chat_files;
mod chat_rows;
mod captures;
mod chats;
mod cli_setup;
mod engine;
mod issues;
mod assets;
mod connect;
mod control;
mod dbus;
mod inbox;
mod local_runs;
mod settings;
mod settings_pages;
mod share;
mod sealed;
mod spaces;
mod start_menu;
mod system;
mod team_chat;
mod thumbs;

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
    /// #EDE-65: the rail's chats (the current team's or Local desktop's).
    chats: Vec<uicore_model::chats::ChatInput>,
    ede: Option<ede_protocol::Client>,
    /// The installed applications, scanned at start (the launcher, the start menu).
    apps: Vec<apps::App>,
    menu_apps: Vec<StartMenuApp>,
    /// The compositor's last window list (ede-protocol), to shape `/windows`.
    windows: Vec<ede_protocol::WindowInfo>,
    /// Live thumbnails by window id, refreshed by the capture loop while
    /// the switcher or the previews are open.
    thumbnails: HashMap<String, String>,
    /// #EXP-2024: THE capture loop's plan and asks (`captures.rs`): the
    /// windows and agent spaces it photographs, derived from what is open.
    captures: Arc<captures::Shared>,
    /// #EXP-2024: the machine's agent spaces, their titles and pictures, the pinned view.
    spaces: spaces::Spaces,
    /// The screen is locked (`session.locked` / `session.unlocked`): nothing is captured.
    locked: bool,
    /// The first person output's x, y, width, height (a fullscreen window over it covers the view).
    first_output: Option<(i32, i32, i32, i32)>,
    /// #EXP-2024: when each layer document last opened (a counter): a later
    /// top layer lies above an earlier one.
    layers_opened: HashMap<String, u64>,
    /// The OSD closes when this passes.
    osd_until: Option<Instant>,
    /// #EDE-89: when each popup document last opened (epoch ms, the clock of
    /// the compositor's event stamps).
    popups_opened: HashMap<String, i64>,
    /// #EDE-168: the height each open taskbar popup was drawn to fit in.
    popup_rooms: HashMap<String, u32>,
    /// Bumped by every fresh Alt+Tab: a delay timer of an earlier one shows nothing.
    switcher_generation: u64,
    notifications: Notifications,
    /// #EDE-65: Needs you across the accounts' teams (the tray button, needs-you.json).
    needs_you: Vec<inbox::NeedsYouItem>,
    /// #EDE-141: the tray's read of the accounts' stores runs off the main
    /// loop, one at a time; a refresh asked for meanwhile runs after it.
    tray_reading: bool,
    tray_again: bool,
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
    /// #EDE-57: Share as a team's state.
    share: share::Share,
    /// The Local desktop opened for a share (its store, blobs, identity).
    local: Option<Arc<elocal::LocalDesktop>>,
    /// A team to make current once the account's teams shape carries it.
    pending_pick: Option<String>,
    /// #EDE-58: the chat on the current Local desktop (its agent runs).
    chat_local: Option<local_runs::LocalChat>,
    /// EXP-1513: the chat's Claude Code setup (install + its own sign-in)
    /// and the running one's steps.
    chat_setup: cli_setup::Step,
    chat_setup_rx: Option<flume::Receiver<cli_setup::Step>>,
    /// #EDE-65: the current Local desktop's store changes (its chats).
    local_sessions: Option<flume::Receiver<elocal::LocalDelta>>,
    /// #EDE-65 lane I: the agent picked on the chat's home.
    chat_agent: Option<String>,
    /// The composer's text as it last reported it (the `/` menu reads it).
    chat_draft: String,
    /// #EDE-129: find in chat while its bar is open.
    chat_find: Option<ChatFind>,
    /// #EDE-65 lane L: the composer's staged attachments (`/chatFiles`).
    chat_files: chat_files::ChatFiles,
    /// A send is uploading its attachments (a second one waits).
    chat_uploading: bool,
    /// The open session's CLI (`claude` | `codex`); `None` on the home.
    chat_cli: Option<String>,
    /// The current team's or Local desktop's prompts (the `/` menu's second group).
    chat_prompts: Vec<domain::rows::PromptRow>,
    /// Who `@` can name in the current chat.
    chat_members: Vec<chat::Member>,
    /// The draft a pick closed the menu on (it stays shut for it, the IDE's rule).
    chat_menu_dismissed: Option<String>,
    /// The `/chatMenu` as drawn, and its highlighted row (the keys move it).
    chat_menu: Value,
    chat_menu_selected: usize,
    /// The chat on a team: its account, team and open session.
    chat_team: Option<team_chat::TeamChat>,
    /// #EDE-141: the team chat's rows (its account's), opened with the
    /// app's keyring, moved by the sync's deltas.
    chat_rows: Option<chat_rows::ChatRows>,
    /// The team's agents' glyphs (the hosts paint `ide:agent:<id>` from it).
    chat_glyphs: AgentGlyphs,
    /// The chat document's literal composer props (placeholder, busy) as drawn.
    chat_composer: Value,
    /// #VAPP-273: the apps on the Engine (their windows and widgets).
    engine: Option<engine::SharedEngine>,
    /// #EDE-141: the desktop agent's seats on the running apps: replaced at
    /// every Engine boot, empty after a shutdown.
    engine_seats: engine::exposes::Seats,
    /// #EDE-64: the apps launched in the last seconds.
    launches: system::LaunchGuard,
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
    /// The agents' glyphs the chat's icons name (shared with the shell).
    glyphs: AgentGlyphs,
}

type AgentGlyphs = Rc<RefCell<HashMap<String, team_chat::AgentGlyph>>>;

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

/// THE sender avatar disc around its glyph (the IDE's `agent_avatar`).
fn avatar_disc(inner: gpui::AnyElement, size: f32) -> gpui::AnyElement {
    use gpui::{prelude::*, px};
    gpui::div().flex_shrink_0().size(px(size)).rounded_full().overflow_hidden().flex().items_center().justify_center().bg(gpui::rgba(AVATAR_DISC)).child(inner).into_any_element()
}

/// An icon of the embedded set by its concept or Lucide name.
fn plain_icon(name: &str, size: f32, color: gpui::Hsla) -> Option<gpui::AnyElement> {
    use gpui::{prelude::*, px};
    Some(gpui::svg().path(assets::icon_path(name)?).size(px(size)).text_color(color).into_any_element())
}

/// THE sender avatar's side (transcript-grouping.json `layout.avatar`).
const SENDER_AVATAR: f32 = 32.0;

/// The sender avatar's (disc, glyph) px: the IDE's `agent_avatar` at
/// [`SENDER_AVATAR`] (32 → 18).
fn avatar_sizes() -> (f32, f32) {
    (SENDER_AVATAR, (SENDER_AVATAR / 2.0 + 2.0).round())
}

/// The sender avatar's disc: the exponential theme's dark `secondary`
/// (#ffffff0f, the IDE's `cx.theme().secondary`).
const AVATAR_DISC: u32 = 0xffffff0f;

impl HostPlugin for Host {
    fn icon(&self, name: &str) -> Option<gpui::SharedString> {
        assets::icon_path(name)
    }

    /// #EDE-65 lane O: an image pasted into a chat composer stages like a
    /// picked file (its `attach` event follows, carrying {files}).
    fn on_upload(&self, event: &eui_gpui::host::UploadEvent, cx: &mut App) {
        if !matches!(event.component_id.as_str(), "chat_composer" | "chat_home_composer") || !allowed(&self.shell.borrow(), &event.surface_id, "ede_chat") {
            return;
        }
        let (shell, paths, field) = (self.shell.clone(), event.paths.clone(), event.component_id.clone());
        cx.defer(move |cx| chat_stage(&shell, &field, &paths, cx));
    }

    /// #EDE-65: the rail's glyphs as the IDE paints them (the agent's mark,
    /// its dot), and lane I: a team agent's icon in its colour by the IDE
    /// rail's own name (`ide:agent:<id>`, its `shell_regions` glyphs), which
    /// the host owns and paints from its glyph map.
    fn icon_element(&self, name: &str, size: f32, color: gpui::Hsla, _cx: &App) -> Option<gpui::AnyElement> {
        use gpui::{prelude::*, px, svg};
        // #EDE-65 lane O: THE sender avatar (the IDE's `agent_avatar`): the
        // inner icon on the secondary disc, 32 → an 18 px glyph.
        if let Some(inner) = name.strip_prefix(chat::AVATAR_ICON_PREFIX) {
            // genUI hands an Icon's `size` enum (16), never its 32 px box:
            // the disc is the IDE's fixed sender size.
            let (size, glyph) = avatar_sizes();
            // A host glyph (an agent's own), else a plain icon (the
            // Assistant's brand mark, the session and person glyphs).
            let inner = self.icon_element(inner, glyph, color, _cx).or_else(|| plain_icon(inner, glyph, color))?;
            return Some(avatar_disc(inner, size));
        }
        let glyph = self.glyphs.try_borrow().ok().and_then(|g| g.get(name).cloned());
        if let Some(glyph) = glyph {
            let path = assets::icon_path(&glyph.icon)?;
            return Some(svg().path(path).size(px(size)).text_color(gpui::rgb(glyph.color)).into_any_element());
        }
        uicore_gpui::glyphs::paint(name, size, color, &assets::icon_path)
    }

    fn on_action(&self, event: &ActionEvent, cx: &mut App) {
        // #EDE-75: fired by a view the shell updates while it is borrowed: after that borrow.
        if self.shell.try_borrow_mut().is_err() {
            let (host, event) = (self.clone(), event.clone());
            cx.defer(move |cx| host.on_action(&event, cx));
            return;
        }
        tracing::info!(surface = %event.surface_id, component = %event.component_id, name = %event.name, context = %event.context, "action");
        // The view is mid-update while it dispatches; a popup reads its
        // painted bounds, so that waits for the update to end.
        // The composer's menu keys ride an event: genUI hands an event's
        // payload ({key}) to its context, never to a function's args.
        if event.name == "ede_chat" && event.context["action"] == "key" && allowed(&self.shell.borrow(), &event.surface_id, "ede_chat") {
            let (shell, key) = (self.shell.clone(), event.context["key"].as_str().unwrap_or("").to_string());
            cx.defer(move |cx| chat_menu_key(&shell, &key, cx));
            return;
        }
        // #EDE-65 lane O: the composer's attach is an event: a pasted
        // image's carries its {files} (already staged through `on_upload`),
        // a bare one opens the chooser.
        if event.name == "ede_chat" && event.context["action"] == "attach" && allowed(&self.shell.borrow(), &event.surface_id, "ede_chat") {
            if event.context.get("files").is_none_or(Value::is_null) {
                let shell = self.shell.clone();
                cx.defer(move |cx| chat_attach(&shell, cx));
            }
            return;
        }
        // #EDE-65 lane O: the "+" Menu's pick (an event: its context carries the value).
        if event.name == "ede_chat" && event.context["action"] == "plus" && allowed(&self.shell.borrow(), &event.surface_id, "ede_chat") {
            let (shell, value) = (self.shell.clone(), event.context["value"].as_str().unwrap_or("").to_string());
            cx.defer(move |cx| match uicore_model::chat::plus_pick(&value) {
                (chat::PLUS_ADD_FILE, None) => chat_attach(&shell, cx),
                (chat::PLUS_PROMPTS, Some(id)) => chat_load_prompt(&shell, id, cx),
                other => tracing::warn!(?other, "a + row eDE does not serve"),
            });
            return;
        }
        // Lane Q: a chat card's Embed bubbles its inner events as
        // {name, context} over the row's {action: card, messageId}.
        if event.name == "ede_chat" && event.context["action"] == "card" && allowed(&self.shell.borrow(), &event.surface_id, "ede_chat") {
            let shell = self.shell.clone();
            let context = event.context.clone();
            cx.defer(move |cx| {
                let message_id = context["messageId"].as_str().unwrap_or("");
                chat_card(&shell, message_id, context["name"].as_str().unwrap_or(""), &context["context"], cx);
            });
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
            "ede_notify_clear" => {
                cx.defer(move |cx| {
                    let ids: Vec<u32> = shell.borrow().notifications.list.iter().map(|n| n.id).collect();
                    for id in ids {
                        notification_close(&shell, id, 2, cx);
                    }
                    // #EDE-65: and every inbox entry it lists is marked read.
                    let keys: Vec<String> = shell.borrow().notifications.inbox.iter().map(|i| i.key.clone()).collect();
                    for key in keys {
                        inbox_mark_read(&shell, &key, cx);
                    }
                    close(&shell, "notification-center", cx);
                });
            }
            "ede_needs_you_open" => {
                if let Some(key) = args.get("id").and_then(Value::as_str).map(String::from) {
                    cx.defer(move |cx| needs_you_open(&shell, &key, cx));
                }
            }
            "ede_tray" => {
                if let Some(service) = args.get("service").and_then(Value::as_str).map(String::from) {
                    cx.defer(move |_| tray_press(&shell, &service));
                }
            }
            "ede_open_document" | "ede_close_document" => {
                if let Some(doc) = args.get("name").and_then(Value::as_str).map(String::from) {
                    let open_it = name == "ede_open_document";
                    cx.defer(move |cx| {
                        if open_it {
                            close(&shell, "menu", cx);
                            close(&shell, "start-menu", cx);
                            dispatch(&shell, ShellEvent::StartMenuClose, cx);
                            open_on_demand(&shell, &doc, cx);
                            refresh(&shell, cx);
                        } else {
                            close(&shell, &doc, cx);
                        }
                    });
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
                if let Some(output) = args.get("name").and_then(Value::as_str).map(String::from) {
                    let mode = args.get("mode").and_then(Value::as_str).map(String::from);
                    let scale = args.get("scale").and_then(|s| s.as_f64().or_else(|| s.as_str().and_then(|t| t.parse().ok())));
                    with_ede(&self.shell, |ede| ede.set_output(&output, mode.as_deref(), scale, None));
                    if let Err(e) = settings::persist_output(&output, mode.as_deref(), scale) {
                        tracing::warn!("compositor.toml: {e}");
                    }
                    cx.defer(move |cx| {
                        outputs_refresh(&shell);
                        refresh(&shell, cx);
                    });
                }
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
                    refresh(&shell, cx);
                });
            }
            "ede_tray_menu" => {
                let (service, id) = (args.get("service").and_then(Value::as_str).map(String::from), args.get("id").and_then(Value::as_i64));
                if let (Some(service), Some(id)) = (service, id) {
                    if let Some(d) = self.shell.borrow().dbus.as_ref() {
                        // On the tray thread (#EDE-135).
                        d.tray_menu_event(&service, id as i32);
                    }
                    cx.defer(move |cx| close(&shell, "tray-menu", cx));
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
                    Some("connect") => {
                        close(&shell, "start-menu", cx);
                        open_on_demand(&shell, "connect", cx);
                        refresh(&shell, cx);
                    }
                    Some("share") => {
                        settings_open_page(&shell, "local-general", cx);
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
            // #EDE-56: Connect Exponential.
            "ede_connect" => {
                let args = args.clone();
                cx.defer(move |cx| {
                    let instance = args.get("instance").and_then(Value::as_str).unwrap_or("").to_string();
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
                    refresh(&shell, cx);
                });
            }
            "ede_taskbar_menu" => {
                let (app, value) = (args.get("app").and_then(Value::as_str).map(String::from), args.get("value").and_then(Value::as_str).map(String::from));
                if let (Some(app), Some(value)) = (app, value) {
                    cx.defer(move |cx| match value.as_str() {
                        "pin" => {
                            dispatch(&shell, ShellEvent::TaskbarPin { app }, cx);
                            pins_persist(&shell);
                        }
                        "unpin" => {
                            dispatch(&shell, ShellEvent::TaskbarUnpin { app }, cx);
                            pins_persist(&shell);
                        }
                        "close" => {
                            let ids: Vec<ede_protocol::WindowId> = shell.borrow().model.windows.iter().filter(|w| w.app == app).filter_map(|w| ede_protocol::WindowId::new(&w.id).ok()).collect();
                            for id in ids {
                                with_ede(&shell, |ede| ede.close_window(id));
                            }
                        }
                        other => tracing::warn!(value = other, "unknown taskbar menu entry"),
                    });
                }
            }
            "ede_start_menu" => {
                let args = args.clone();
                // #EDE-92: Enter in the search box runs the selected row as a press on it does
                // (after the box's text, which is deferred just before).
                if args.get("activate").and_then(Value::as_bool) == Some(true) {
                    let (host, event) = (self.clone(), event.clone());
                    cx.defer(move |cx| {
                        let selected = host.shell.borrow().feed.get("/shell").and_then(|shell| start_menu::activation(&shell["startMenu"]));
                        if let Some((name, args)) = selected {
                            host.on_function_call(&FunctionCallEvent { name: name.to_string(), args, ..event }, cx);
                        }
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
            "ede_chat" => {
                let args = args.clone();
                cx.defer(move |cx| {
                    let arg = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or("").to_string();
                    match args.get("action").and_then(Value::as_str) {
                        // #EDE-65: `{sessionId}` alone = a chat of the rail.
                        None if !arg("sessionId").is_empty() => rail_chat_open(&shell, &arg("sessionId"), cx),
                        Some("send") => chat_send(&shell, &arg("text"), cx),
                        Some("answer") => chat_answer(&shell, &arg("messageId"), &arg("answer"), cx),
                        Some("pick") => {
                            shell.borrow_mut().chat_agent = Some(arg("agentId"));
                            chatting_changed(&shell, cx);
                        }
                        Some("open") => chat_open(&shell, Some(&arg("sessionId")), cx),
                        Some("new") => chat_open(&shell, None, cx),
                        Some("stop") => chat_stop(&shell, cx),
                        // EXP-1513: the empty state's Set up Claude Code / Open sign-in.
                        Some("setup") => chat_setup_start(&shell, cx),
                        Some("signIn") => {
                            if let cli_setup::Step::SigningIn { url: Some(url) } = &shell.borrow().chat_setup {
                                connect::open_url(url);
                            }
                        }
                        Some("attach") => chat_attach(&shell, cx),
                        Some("unattach") => {
                            if let Ok(key) = arg("key").parse::<u64>() {
                                chat_unattach(&shell, key, cx);
                            }
                        }
                        // #EDE-129 (find-in-chat.json): the header's Search in chat, the bar.
                        Some("findOpen") => {
                            if shell.borrow().chat_find.is_none() {
                                shell.borrow_mut().chat_find = Some(ChatFind::default());
                                shell.borrow_mut().feed.set("/chatFind", json!({"query": ""}));
                            }
                            chatting_changed(&shell, cx);
                            shell.borrow().surfaces.focus_component("chat", "chat_find_input", cx);
                        }
                        Some("findClose") => {
                            shell.borrow_mut().chat_find = None;
                            chatting_changed(&shell, cx);
                        }
                        Some("find") => {
                            if let Some(find) = shell.borrow_mut().chat_find.as_mut() {
                                let query = domain::find_in_chat::query(&arg("query"));
                                if find.query != query {
                                    *find = ChatFind { query, land: true, ..Default::default() };
                                }
                            }
                            chatting_changed(&shell, cx);
                        }
                        Some("findStep") => {
                            let step = domain::find_in_chat::FindStep::parse(&arg("step")).unwrap_or(domain::find_in_chat::FindStep::Previous);
                            if let Some(find) = shell.borrow_mut().chat_find.as_mut() {
                                let index = find.current.as_ref().and_then(|was| find.stops.iter().position(|stop| stop == was));
                                if let Some(next) = domain::find_in_chat::step(index, find.stops.len(), step) {
                                    find.current = find.stops.get(next).cloned();
                                    find.land = true;
                                }
                            }
                            chatting_changed(&shell, cx);
                        }
                        Some("draft") => match chat::commit_emoji(&arg("text")) {
                            // EXP-551: a closed exact `:shortcode:` becomes its emoji at once.
                            Some(committed) => chat_set_draft(&shell, &committed, cx),
                            None => {
                                shell.borrow_mut().chat_draft = arg("text");
                                chat_menu_changed(&shell);
                                refresh(&shell, cx);
                            }
                        },
                        Some("slash") => chat_menu_pick(&shell, &arg("insert"), &arg("prompt"), cx),
                        // The quick replies: eDE feeds none yet.
                        Some("reply" | "dismissReplies") => {}
                        _ => chat_toggle(&shell, cx),
                    }
                });
            }
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
        apps::App { id: format!("{FILES_APP}.desktop"), name: "Files".into(), comment: "Browse and organize your files".into(), exec: files_exec, exec_url: None, icon: files_icon, keywords: "files nemo file manager folders".into(), categories: vec!["Utility".into(), "System".into()] },
        apps::App { id: format!("{HISTORY_APP}.desktop"), name: "Version history".into(), comment: "Every edit of the desktop's shell, undo any".into(), exec: String::new(), exec_url: None, icon: None, keywords: "version history git undo shell edits".into(), categories: vec!["Utility".into()] },
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
/// Opens the Settings window on `page` (the document owns `/settings`
/// again right after: the feed hands it the page once).
fn settings_open_page(shell: &SharedShell, page: &str, cx: &mut App) {
    let state = json!({"page": page, "deviceOpen": false, "personalOpen": false, "codingOpen": false, "team": {"kind": "", "id": "", "name": ""}, "switcherOpen": false});
    shell.borrow_mut().feed.set("/settings", state);
    close(shell, "start-menu", cx);
    dispatch(shell, ShellEvent::StartMenuClose, cx);
    open_on_demand(shell, "settings", cx);
    refresh(shell, cx);
    shell.borrow_mut().feed.remove("/settings");
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
    let accounts: Vec<(String, String)> = shell.borrow().workspaces.auth.signed_in_accounts().iter().map(|a| (a.id.clone(), a.email.clone())).collect();
    let summary = share::summary(&desktop);
    shell.borrow_mut().share.start(&desktop.entry.id, accounts, summary);
}

/// The upload, to the picked account, named as asked (else as the desktop).
fn share_run(shell: &SharedShell, name: &str, account: &str) {
    let Some(desktop) = local_desktop(shell) else { return };
    let accounts = shell.borrow().workspaces.auth.signed_in_accounts();
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
    shell.borrow_mut().pending_pick = Some(shared.team_id);
    switcher_refresh_entries(shell, cx);
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
    let pending = shell.borrow().pending_pick.clone().filter(|id| entries.iter().any(|e| e.kind == uicore_model::SwitcherKind::Team && &e.id == id));
    dispatch(shell, ShellEvent::SwitcherSet { entries }, cx);
    if let Some(id) = pending {
        shell.borrow_mut().pending_pick = None;
        switcher_picked(shell, uicore_model::SwitcherKind::Team, &id, cx);
    }
}

/// #EDE-65: a pick from the switcher (or a team that arrived for a pending
/// one): the current entry, remembered across logins; the chat, the apps and
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
    // Another desktop or a team: the opened desktop and its chat go.
    let changed = shell.borrow().local.as_ref().is_some_and(|d| !picked_local || d.entry.id != id);
    if changed {
        if let Some(chat) = shell.borrow_mut().chat_local.take() {
            chat.stop();
        }
        shell.borrow_mut().local = None;
    }
    chatting_changed(shell, cx);
    chats_refresh(shell, cx);
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
        return;
    }
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

/// The Exponential app's ids: one IDE per person, a hand-off target.
const EXPONENTIAL_APPS: [&str; 2] = ["at.exponential", "at.exponential.staging"];

/// #EDE-64: the sidebar hands off to the Exponential app (Inbox, New issue,
/// Needs you, Chats): its window comes forward when one is up, a burst of
/// clicks launches anything once.
fn launch_app(shell: &SharedShell, app: &str) {
    let wanted = app.trim_end_matches(".desktop");
    if EXPONENTIAL_APPS.contains(&wanted) {
        let open = {
            let s = shell.borrow();
            let mine: Vec<_> = s.model.windows.iter().filter(|w| w.app == wanted).collect();
            mine.iter().find(|w| w.focused).or(mine.first()).map(|w| w.id.clone())
        };
        if let Some(id) = open.and_then(|id| ede_protocol::WindowId::new(&id).ok()) {
            with_ede(shell, |ede| ede.focus(id));
            return;
        }
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

/// The installed entry for an app id (a window's app_id, a pin), if any.
fn app_entry<'a>(shell: &'a Shell, app: &str) -> Option<&'a apps::App> {
    let wanted = app.trim_end_matches(".desktop");
    shell.apps.iter().find(|a| a.id.trim_end_matches(".desktop").eq_ignore_ascii_case(wanted)).or_else(|| shell.apps.iter().find(|a| a.id.trim_end_matches(".desktop").rsplit('.').next().is_some_and(|tail| tail.eq_ignore_ascii_case(wanted))))
}

fn app_icon(shell: &Shell, app: &str) -> String {
    app_entry(shell, app).and_then(|a| a.icon.as_ref()).map(|p| format!("file://{}", p.display())).unwrap_or_default()
}

fn app_name(shell: &Shell, app: &str) -> String {
    app_entry(shell, app).map(|a| a.name.clone()).unwrap_or_else(|| app.to_string())
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
/// icons for apps, the rail's chats, the start menu's favourites.
fn decorate_shell(shell: &Shell, mut value: Value) -> Value {
    if let Some(buttons) = value["taskbar"]["buttons"].as_array_mut() {
        for b in buttons {
            let app = b["app"].as_str().unwrap_or_default().to_string();
            b["name"] = json!(app_name(shell, &app));
            b["icon"] = json!(app_icon(shell, &app));
        }
    }
    value["chats"] = chats::chats_json(&shell.chats);
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
    value["startMenu"]["issues"] = json!(uicore_model::start_menu_issues(&shell.model, &shell.issue_hits));
    start_menu::mark(&mut value["startMenu"], &shell.menu_selection);
    value
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

/// A stretched axis (anchored to both of its edges) sized to the output,
/// since the window takes the size it asks for; the sidebar the rail's
/// width and exclusive zone (`shell-layout.json`), standing on the taskbar
/// (#EDE-65); any other document's as written.
fn header_for(shell: &Shell, doc: &Document) -> SurfaceHeader {
    let out = output_size(shell);
    let mut header = layer_header(doc, out);
    // The Alt-Tab popup hugs its tiles (#EDE-65).
    if let (true, SurfaceHeader::Layer { size, .. }) = (doc.name == "switcher", &mut header) {
        size.width = Some(switcher_width(uicore_model::alt_tab_order(&shell.model).len(), out.0));
    }
    // #EXP-2024: the pinned view is sized by the display and its space (spaces.json).
    if let (true, SurfaceHeader::Layer { size, .. }) = (doc.name == spaces::VIEW, &mut header) {
        if let Some((_, view)) = shell.spaces.view(first_display(shell)) {
            size.width = Some(view.width);
            size.height = Some(view.height);
        }
    }
    taskbar_popup_room(&mut header, out.1);
    header
}

/// #EDE-168: a taskbar popup never reaches past the output's work area: it
/// stands on its button, at most the height above the taskbar less the
/// panel's margin (854×534 logical, 1280×800 at 1.5: the sound applet's
/// 600 became 476). Its root follows ([`size_root`]) and its content
/// scrolls; a root that hugs its content takes the window down to it
/// ([`fit_popups`]).
fn taskbar_popup_room(header: &mut SurfaceHeader, out_h: u32) {
    use ede_ui::surface::Placement;
    let SurfaceHeader::Popup { popup, .. } = header else { return };
    if popup.parent != "taskbar" || !matches!(popup.placement, Placement::TopStart | Placement::TopEnd) {
        return;
    }
    let layout = shell_layout();
    let room = out_h.saturating_sub(layout.taskbar_height + layout.panel_margin).max(1);
    popup.size.height = popup.size.height.min(room);
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
        let before = shell.borrow_mut().popup_rooms.insert(doc.name.clone(), room);
        if before.is_some_and(|b| b != room) {
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
        // #EDE-65 lane I: the chat fills the panel like a maximised window.
        if doc.name == "chat" {
            let (w, h) = engine::window_max((out_w, out_h));
            size.width = Some(w);
            size.height = Some(h);
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
        let room = popup.size.height;
        let style = root.style.get_or_insert_with(Default::default);
        if style.get("height").and_then(Value::as_f64).is_some_and(|h| h > room as f64) {
            style.insert("height".into(), json!(room));
        }
        style.insert("maxHeight".into(), json!(room));
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
/// IDE's sidebar parts, the chat's composer as `/chatting` says (its busy,
/// placeholder and + menu are literal props), the root at the surface's
/// size, the header it opens with. Every load goes through it: an open, an
/// edit on disk, an output change, a composer change.
fn as_drawn(doc: &Document, header: SurfaceHeader, chatting: Option<&Value>) -> Document {
    let mut doc = doc.clone();
    drawn(&mut doc);
    patch_chat(chatting, &mut doc);
    size_root(&mut doc, &header);
    doc.surface = header;
    doc
}

/// [`as_drawn`] on the output and `/chatting` of now.
fn drawn_now(shell: &Shell, doc: &Document) -> Document {
    as_drawn(doc, header_for(shell, doc), shell.feed.get("/chatting"))
}

fn open_layer(shell: &SharedShell, doc: &Document, cx: &mut App) {
    let (doc, feed) = {
        let s = shell.borrow();
        (drawn_now(&s, doc), s.feed.clone())
    };
    if shell.borrow_mut().surfaces.open_layer(&doc, &feed, cx) {
        let mut s = shell.borrow_mut();
        let next = s.layers_opened.values().max().map_or(1, |n| n + 1);
        s.layers_opened.insert(doc.name.clone(), next);
    }
    // #EXP-1780: the chat's rows follow the newest one.
    if doc.name == "chat" {
        let view = shell.borrow().surfaces.region("chat").map(|r| r.view.clone());
        if let Some(view) = view {
            view.update(cx, uicore_gpui::follow_chat);
        }
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
        shell.borrow_mut().popup_rooms.insert(name.to_string(), popup.size.height);
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
                (false, SurfaceHeader::Layer { on_demand: true, .. }) | (false, SurfaceHeader::Popup { .. }) => {
                    if let Some(line) = loaded_frame(false, &doc) {
                        println!("{line}");
                    }
                }
            }
        }
        Change::Removed { name, .. } => close(shell, &name, cx),
        Change::Rejected { name, issues, .. } => tracing::warn!(document = %name, ?issues, "refused; the last good version stays"),
        Change::ThemeApplied { .. } | Change::ThemeRejected { .. } => {}
    }
}

/// The frame line a change owes the supervisor without drawing (#EDE-110):
/// a closed popup or on-demand layer is loaded, not drawn, and loading it
/// proves the edit (one that breaks once opened crashes the shell, and
/// three crashes revert it). An open or opening surface reports its frame.
fn loaded_frame(open: bool, doc: &Document) -> Option<String> {
    let closed = !open && matches!(doc.surface, SurfaceHeader::Layer { on_demand: true, .. } | SurfaceHeader::Popup { .. });
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
    {
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
        s.windows = list;
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
            (relaid(&doc, header_for(&s, &doc), s.feed.get("/chatting")), s.feed.clone())
        };
        let Some(doc) = doc else { continue };
        shell.borrow_mut().surfaces.resize_layer(&doc, &feed, cx);
    }
}

/// An open layer document as [`relayout`] resizes it to `header`, `None`
/// when it has that size already.
fn relaid(doc: &Document, header: SurfaceHeader, chatting: Option<&Value>) -> Option<Document> {
    (header != doc.surface).then(|| as_drawn(doc, header, chatting))
}

/// Mint's click on a taskbar button: launch, focus, minimize, or the
/// previews of a group.
fn taskbar_press(shell: &SharedShell, app: &str, cx: &mut App) {
    let press = uicore_model::taskbar_press(&shell.borrow().model, app);
    match press {
        TaskbarPress::Launch => launch_app(shell, app),
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
        TaskbarPress::Previews => {
            let open = shell.borrow().model.taskbar.previews.as_deref() == Some(app);
            previews_set(shell, (!open).then(|| app.to_string()), cx);
        }
    }
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
    let value = match s.model.taskbar.previews.clone() {
        None => json!({"app": null, "windows": []}),
        Some(app) => {
            let windows: Vec<Value> = s
                .model
                .windows
                .iter()
                .filter(|w| w.app == app && w.workspace == s.model.workspaces.active)
                .map(|w| json!({"id": w.id, "title": w.title, "focused": w.focused, "minimized": w.minimized, "thumbnail": s.thumbnails.get(&w.id).cloned().unwrap_or_default()}))
                .collect();
            json!({"app": app, "windows": windows})
        }
    };
    s.feed.set("/previews", value);
}

/// The chat surface (the Assistant's composer) opens or closes with the model.
fn chat_toggle(shell: &SharedShell, cx: &mut App) {
    dispatch(shell, ShellEvent::ChatToggle, cx);
    let open = shell.borrow().model.chat.open;
    if open {
        chatting_changed(shell, cx);
        // `#` ranks the start menu's issue pool: build it when nothing did yet.
        if shell.borrow().issue_index.is_empty() {
            issues_reindex(shell, cx);
        }
        open_on_demand(shell, "chat", cx);
        refresh(shell, cx);
    } else {
        close(shell, "chat", cx);
    }
}

fn demo() -> bool {
    std::env::var_os("EDE_SHELL_DEMO").is_some_and(|v| !v.is_empty() && v != "0")
}

/// #EDE-65: the rail's chats from the current entry's sessions: the team's
/// synced store or the Local desktop's (whose changes it then watches).
fn chats_refresh(shell: &SharedShell, cx: &mut App) {
    if demo() {
        return;
    }
    let current = shell.borrow().model.switcher.current.clone();
    let chats = if current.kind == uicore_model::SwitcherKind::Local {
        shell.borrow_mut().local_sessions = None;
        local_desktop(shell)
            .map(|d| {
                shell.borrow_mut().local_sessions = Some(d.store.subscribe());
                chats::chat_inputs(&chats::sessions(d.store.rows("sessions").unwrap_or_default()), d.executor.team_id(), &Default::default())
            })
            .unwrap_or_default()
    } else {
        shell.borrow_mut().local_sessions = None;
        let store = current.account.as_deref().and_then(|a| shell.borrow().workspaces.store(a));
        match store {
            Some(store) => {
                let read = |shape: &str| sync::shapes::shape_by_name(shape).and_then(|spec| store.read_all(spec).ok()).unwrap_or_default();
                let sessions = chats::sessions(read("sessions"));
                let unread = chats::unread_roots(&sessions, &chats::rows(read("session_reads")), &current.id);
                chats::chat_inputs(&sessions, &current.id, &unread)
            }
            None => Vec::new(),
        }
    };
    if shell.borrow().chats != chats {
        shell.borrow_mut().chats = chats;
        model_changed(shell, cx);
    }
}

/// Where a rail chat opens (#EDE-65): eDE's chat (lane I's document) on a
/// Local desktop, and on a team whose chat has the session synced; the
/// Exponential app (`exponential://session/<id>`) only as the fallback,
/// when the team's chat cannot open it (no account store yet, or the
/// session is not among its rows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RailChat {
    Chat,
    HandOff,
}

/// `team_has_session`: the team chat's rows carry the session (`None` = no
/// team chat).
fn rail_chat_route(on_local: bool, team_has_session: Option<bool>) -> RailChat {
    match (on_local, team_has_session) {
        (true, _) | (false, Some(true)) => RailChat::Chat,
        _ => RailChat::HandOff,
    }
}

/// `ede_chat {sessionId}`, a chat of the rail: the chat on that session,
/// shown ([`rail_chat_route`]).
fn rail_chat_open(shell: &SharedShell, id: &str, cx: &mut App) {
    if id.is_empty() {
        return;
    }
    let local = on_local(shell);
    let team_has_session = if local || demo() { None } else { team_chat(shell).map(|(_, rows)| rows.account.sessions.iter().any(|s| s.id == id)) };
    if !demo() && rail_chat_route(local, team_has_session) == RailChat::HandOff {
        hand_off(shell, &accounts::session_deep_link(id));
        return;
    }
    if !demo() {
        chat_open(shell, Some(id), cx);
    }
    if !shell.borrow().model.chat.open {
        chat_toggle(shell, cx);
    }
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
    if accounts::ide_send(url) {
        tracing::info!(url, "handed to the Exponential app");
        return;
    }
    let command = EXPONENTIAL_APPS.iter().find_map(|id| app_entry(&shell.borrow(), id).map(|app| format!("{} '{url}'", app.exec)));
    match command {
        Some(command) => person_launch(shell, &command),
        None => tracing::warn!(url, "no Exponential app to hand off to"),
    }
}

/// The Exponential app's desktop entry.
const EXPONENTIAL_APP: &str = "at.exponential";

/// Is the switcher on a Local desktop?
fn on_local(shell: &SharedShell) -> bool {
    shell.borrow().model.switcher.current.kind == uicore_model::SwitcherKind::Local
}

/// The chat on the current Local desktop, its runner started once (#EDE-58).
fn chat_local(shell: &SharedShell) -> bool {
    if !on_local(shell) {
        return false;
    }
    let Some(desktop) = local_desktop(shell) else { return false };
    let stale = shell.borrow().chat_local.as_ref().is_some_and(|c| c.desktop.entry.id != desktop.entry.id);
    if stale {
        if let Some(old) = shell.borrow_mut().chat_local.take() {
            old.stop();
        }
    }
    if shell.borrow().chat_local.is_none() {
        // #VAPP-273: the desktop's own agent edits the shell through `shell_edit` (never an
        // app) and reaches the running apps' exposed tools (`<app>__<tool>`).
        let ui_dir = shell.borrow().ui.lock().unwrap_or_else(|p| p.into_inner()).dir().to_path_buf();
        let mut providers: Vec<Arc<dyn emcp::server::ToolProvider>> = vec![Arc::new(engine::shell_edit::ShellEdit::new(ui_dir))];
        let mut note = engine::shell_edit::ShellEdit::note();
        // #EDE-141: the seats the Engine's boots replace, so this chat's
        // tools reach the apps that run, whichever Engine booted them.
        let seats = shell.borrow().engine_seats.clone();
        let names: Vec<String> = seats.read().unwrap_or_else(|p| p.into_inner()).iter().map(|a| a.server.clone()).collect();
        if !names.is_empty() {
            note.push_str(&format!(" The apps of this desktop expose their tools as <app>__<tool> (apps: {}); a records tool reads or writes the app's own records, an action tool runs one of the exponential tools via the app.", names.join(", ")));
            providers.push(Arc::new(engine::exposes::AppsProvider { apps: seats, exponential: elocal::runs::local_tools(desktop.clone()) }));
        }
        let host = elocal::runs::HostTools { provider: Arc::new(engine::exposes::Providers(providers)), note };
        let chat = local_runs::LocalChat::start(desktop, Some(host));
        shell.borrow_mut().chat_local = Some(chat);
    }
    true
}

/// EXP-1513: Set up Claude Code (the chat's empty state on a Local desktop
/// without a usable CLI): one setup at a time ([`cli_setup`]).
fn chat_setup_start(shell: &SharedShell, cx: &mut App) {
    if shell.borrow().chat_setup.running() {
        return;
    }
    {
        let mut s = shell.borrow_mut();
        s.chat_setup = cli_setup::Step::Installing;
        s.chat_setup_rx = Some(cli_setup::start());
    }
    chatting_changed(shell, cx);
}

/// EXP-1513: the demo's setup step for a capture (`EDE_SHELL_DEMO_SETUP` =
/// idle, installing, signingIn or failed); unset = the real chat.
fn demo_setup() -> Option<cli_setup::Step> {
    match std::env::var("EDE_SHELL_DEMO_SETUP").ok()?.as_str() {
        "idle" => Some(cli_setup::Step::Idle),
        "installing" => Some(cli_setup::Step::Installing),
        "signingIn" => Some(cli_setup::Step::SigningIn { url: None }),
        "failed" => Some(cli_setup::Step::Failed("Anthropic's installer exited with status 1.".into())),
        _ => None,
    }
}

/// A setup step arrived. `Ready` restarts the desktop's agent so its runner
/// finds the CLI; `Ready` and `Failed` end the setup.
fn chat_setup_step(shell: &SharedShell, step: cli_setup::Step, cx: &mut App) {
    let done = !step.running();
    if step == cli_setup::Step::Ready {
        if let Some(chat) = shell.borrow_mut().chat_local.take() {
            chat.stop();
        }
    }
    {
        let mut s = shell.borrow_mut();
        s.chat_setup = if step == cli_setup::Step::Ready { cli_setup::Step::Idle } else { step };
        if done {
            s.chat_setup_rx = None;
        }
    }
    chatting_changed(shell, cx);
}

/// `/chatting` into the views (#EDE-65 lane I): the chat model of the
/// current conversation: the desktop's own agent on a Local desktop; on a
/// team, until the shell syncs its sessions, the home with the Assistant.
fn chatting_changed(shell: &SharedShell, cx: &mut App) {
    let demo = std::env::var_os("EDE_SHELL_DEMO").is_some_and(|v| !v.is_empty() && v != "0");
    let local = !demo && chat_local(shell);
    // This draws the chat: whatever moved is drawn with it (#EDE-141).
    let team = if demo || local { None } else { team_chat_rows(shell, true).map(|(chat, rows, _)| (chat, rows)) };
    // The open session's CLI (the `/` menu's builtins), `None` = the CLI default.
    let (mut input, no_cli, cli, prompts, members) = {
        let s = shell.borrow();
        match (demo, s.chat_local.as_ref().filter(|_| local), team) {
            (true, _, _) if on_local(shell) => (local_runs::demo_input(), false, None, Vec::new(), Vec::new()),
            (true, _, _) => {
                *s.chat_glyphs.borrow_mut() = team_chat::demo_glyphs();
                (team_chat::demo_input(), false, None, Vec::new(), Vec::new())
            }
            (false, Some(chat), _) => {
                *s.chat_glyphs.borrow_mut() = chat.agents().1;
                (chat.input(), chat.agent.is_empty(), chat.session_cli(), chat.prompts(), Vec::new())
            }
            (false, None, Some((chat, rows))) => {
                *s.chat_glyphs.borrow_mut() = team_chat::glyphs(&rows.account, &chat.team_id);
                let cli = chat.session_id.as_deref().and_then(|id| rows.account.sessions.iter().find(|r| r.id == id)).and_then(|r| r.cli.clone());
                let prompts = inbox_prompts(&rows, &chat.team_id);
                let members = team_chat::members(&rows, &chat.team_id);
                (team_chat::input(&rows, &chat, now_ms()), false, cli, prompts, members)
            }
            _ => (local_runs::assistant_home(), false, None, Vec::new(), Vec::new()),
        }
    };
    // A session's menu offers its CLI's builtins; the home (no session) none.
    shell.borrow_mut().chat_cli = input.session_id.as_ref().map(|_| chat::session_agent(cli.as_deref()).to_string());
    shell.borrow_mut().chat_prompts = chat::ordered_prompts(prompts);
    shell.borrow_mut().chat_members = members;
    chat_menu_changed(shell);
    // #EDE-65 lane O: the "+" rows eDE serves (the paperclip's file chooser, the prompts).
    input.plus = chat::plus_rows(&shell.borrow().chat_prompts);
    input.agent_id = shell.borrow().chat_agent.clone().or(input.agent_id);
    // eDE's composer has THE `/` menu: the conversation promises it (chat.json slashCommands).
    input.slash_commands = true;
    // #EDE-129: find in chat over the synced bubbles.
    let (find_value, land) = chat_find_apply(shell, &mut input.rows);
    let view = uicore_model::chat::chat_view(&input);
    let mut value = chat::chatting(&view);
    value["find"] = find_value;
    value["noCli"] = json!(no_cli);
    value["setup"] = shell.borrow().chat_setup.to_value();
    shell.borrow_mut().feed.set("/chatting", value);
    chat_composer(shell, &view.composer, cx);
    refresh(shell, cx);
    if let Some(index) = land {
        shell.borrow().surfaces.scroll_to_index("chat", uicore_gpui::CHAT_SCROLL, index as u32, "center", cx);
    }
}

/// #EDE-129: find in chat's state (find-in-chat.json): the query, the stops
/// of the last frame, the current one, a landing asked for.
#[derive(Default)]
struct ChatFind {
    query: String,
    stops: Vec<domain::find_in_chat::FindStop>,
    current: Option<domain::find_in_chat::FindStop>,
    land: bool,
}

/// #EDE-129: eDE's rows are the SYNCED bubbles (no transcript): counts
/// each bubble's genUI highlight, keeps the current stop (a new query
/// starts at the newest), sets every bubble's `highlight`; returns
/// `/chatting/find` and the row index to centre (a landing).
fn chat_find_apply(shell: &SharedShell, rows: &mut [uicore_model::chat::ChatRowInput]) -> (Value, Option<usize>) {
    use domain::find_in_chat as find;
    use uicore_model::chat::ChatRowInput;
    let mut s = shell.borrow_mut();
    let Some(state) = s.chat_find.as_mut() else { return (json!({"open": false, "counter": "", "status": ""}), None) };
    if state.query.is_empty() {
        state.stops.clear();
        state.current = None;
        return (json!({"open": true, "counter": "", "status": ""}), None);
    }
    let query = state.query.clone();
    let find_rows: Vec<(usize, find::FindRow)> = rows
        .iter()
        .enumerate()
        .filter_map(|(ix, row)| match row {
            ChatRowInput::Message { id, body, .. } => Some((ix, find::FindRow { key: find::message_key(id), count: eui_gpui::paint::highlight::markdown_highlight_count(body, &query), group: None, output_hit: false })),
            _ => None,
        })
        .collect();
    let stops = find::stops(&find_rows.iter().map(|(_, row)| row.clone()).collect::<Vec<_>>(), &[]);
    let index = state
        .current
        .as_ref()
        .and_then(|was| stops.iter().position(|stop| stop.key == was.key && stop.local == was.local))
        .or_else(|| find::step(None, stops.len(), find::FindStep::Start));
    state.current = index.and_then(|index| stops.get(index).cloned());
    state.stops = stops;
    let current = state.current.clone();
    for row in rows.iter_mut() {
        if let ChatRowInput::Message { id, highlight, .. } = row {
            let local = current.as_ref().filter(|stop| stop.key == find::message_key(id)).map(|stop| stop.local);
            *highlight = Some(match local {
                Some(local) => json!({"query": query, "current": local}),
                None => json!({"query": query}),
            });
        }
    }
    let land = std::mem::take(&mut state.land)
        .then(|| current.as_ref().and_then(|stop| find_rows.iter().find(|(_, row)| row.key == stop.key).map(|(ix, _)| *ix)))
        .flatten();
    let counter = find::counter(index, state.stops.len(), true, false);
    (json!({"open": true, "counter": counter, "status": ""}), land)
}

/// The conversation composer's placeholder and busy (and the "+" Menu's
/// items, #EDE-65 lane O) are literal props (genUI binds only a Composer's
/// value): set by id, redrawn on a change.
fn chat_composer(shell: &SharedShell, composer: &uicore_model::chat::ChatComposer, cx: &mut App) {
    // EXP-1513: the setup's failure reason is a literal prop too (`chat_cli_failed`).
    let message = shell.borrow().chat_setup.to_value()["message"].clone();
    let props = json!({"placeholder": composer.placeholder, "busy": composer.busy, "items": composer.menu, "setupMessage": message});
    if shell.borrow().chat_composer == props {
        return;
    }
    shell.borrow_mut().chat_composer = props;
    if !shell.borrow().surfaces.is_open("chat") {
        return;
    }
    let Some(doc) = document(&shell.borrow(), "chat") else { return };
    let (doc, feed) = {
        let s = shell.borrow();
        (drawn_now(&s, &doc), s.feed.clone())
    };
    shell.borrow_mut().surfaces.redraw(&doc, &feed, cx);
}

/// The chat model's composer (`/chatting/composer`) into the chat
/// document's literal props.
fn patch_chat(chatting: Option<&Value>, doc: &mut Document) {
    if doc.name != "chat" {
        return;
    }
    let Some(composer) = chatting.map(|c| &c["composer"]) else { return };
    for prop in ["placeholder", "busy"] {
        if let Some(value) = composer.get(prop) {
            uicore_gpui::inline::set_prop(doc, "chat_composer", prop, value.clone());
        }
    }
    // #EDE-65 lane O: the "+" Menu's items are a literal prop too.
    if let Some(menu) = composer.get("menu") {
        for id in ["chat_plus", "chat_home_plus"] {
            uicore_gpui::inline::set_prop(doc, id, "items", menu.clone());
        }
    }
    if let Some(message) = chatting.map(|c| &c["setup"]["message"]).filter(|m| m.is_string()) {
        uicore_gpui::inline::set_prop(doc, "chat_cli_failed", "description", message.clone());
    }
}

/// The chat on the current team (its account's synced rows), kept on its
/// session while the team stays current; `None` off a team or before its
/// account's store opened.
fn team_chat(shell: &SharedShell) -> Option<(team_chat::TeamChat, Rc<team_chat::TeamRows>)> {
    team_chat_rows(shell, false).map(|(chat, rows, _)| (chat, rows))
}

/// [`team_chat`] (#EDE-141: [`chat_rows::ChatRows`] reads only what the
/// sync noted, the open tree's messages only); `paint` = the caller draws
/// the chat now, so it learns whether the open chat's rows moved since the
/// last paint (and it is no longer owed).
fn team_chat_rows(shell: &SharedShell, paint: bool) -> Option<(team_chat::TeamChat, Rc<team_chat::TeamRows>, bool)> {
    let current = shell.borrow().model.switcher.current.clone();
    let account_id = current.account.clone().filter(|_| current.kind == uicore_model::SwitcherKind::Team)?;
    let (account, store) = {
        let s = shell.borrow();
        (s.workspaces.account(&account_id)?, s.workspaces.store(&account_id)?)
    };
    let mut chat = team_chat::TeamChat { account_id: account_id.clone(), team_id: current.id.clone(), session_id: None };
    if let Some(open) = shell.borrow().chat_team.as_ref().filter(|c| c.account_id == chat.account_id && c.team_id == chat.team_id) {
        chat.session_id = open.session_id.clone();
    }
    shell.borrow_mut().chat_team = Some(chat.clone());
    // Sealed chats open with the keys the Exponential app holds.
    let mut rows = match shell.borrow_mut().chat_rows.take().filter(|r| r.account_id() == account_id) {
        Some(rows) => rows,
        None => chat_rows::ChatRows::new(&account_id, &account.user_id, Arc::new(sealed::opener(&account_id)), sealed::keyring_path(&account_id)),
    };
    rows.refresh(&store, &chat);
    let moved = paint && rows.take_owed();
    let read = rows.rows();
    shell.borrow_mut().chat_rows = Some(rows);
    Some((chat, read, moved))
}

/// The composer's text: to the desktop's agent (#EDE-58), or on a team
/// through `sessions.send` (the IDE's path): the open session, else the
/// picked agent; the chat then shows the session the server names.
fn chat_send(shell: &SharedShell, text: &str, cx: &mut App) {
    // #EDE-134: the composer emptied its field at the press (composer-send.json):
    // every exit that did not send hands the text back.
    if shell.borrow().chat_uploading {
        chat_keep_draft(shell, text, cx);
        return;
    }
    if chat_local(shell) {
        // #EDE-65 lane L: the staged attachments go into the desktop's blob store first.
        let jobs = shell.borrow().chat_files.jobs();
        let stored = {
            let s = shell.borrow();
            match s.chat_local.as_ref() {
                Some(chat) => api::attachment_upload::upload_all(jobs, |filename, content_type, bytes| chat.attach(filename, content_type, bytes)),
                None => Ok(Vec::new()),
            }
        };
        let resolved = match stored {
            Ok(resolved) => resolved,
            Err((landed, e)) => {
                shell.borrow_mut().chat_files.note_uploaded(&landed);
                failed_action(shell, "upload", "the attachment", &e, cx);
                chat_keep_draft(shell, text, cx);
                return;
            }
        };
        shell.borrow_mut().chat_files.note_uploaded(&resolved);
        let picked = shell.borrow().chat_agent.clone();
        let ids = chat_files::attachment_ids(&resolved);
        let sent = shell.borrow_mut().chat_local.as_mut().map(|c| c.send_with(text, picked.as_deref(), ids)).unwrap_or(Ok(()));
        match sent {
            Ok(()) => {
                chat_files_sent(shell, &resolved, cx);
                chat_clear_draft(shell, cx);
            }
            Err(e) => {
                failed_action(shell, "send", "the message", &e, cx);
                chat_keep_draft(shell, text, cx);
            }
        }
        return;
    }
    let Some((chat, rows)) = team_chat(shell) else {
        chat_keep_draft(shell, text, cx);
        launch_app(shell, "at.exponential");
        return;
    };
    let picked = shell.borrow().chat_agent.clone().or_else(|| team_chat::agents(&rows.account, &chat.team_id).first().map(|a| a.id.clone()));
    let jobs = shell.borrow().chat_files.jobs();
    let Some(mut input) = team_chat::send_input(&chat, picked.as_deref(), text, !jobs.is_empty()) else {
        chat_keep_draft(shell, text, cx);
        return;
    };
    let tree = team_chat::send_tree(&rows.account, &input);
    let Some(account) = shell.borrow().workspaces.account(&chat.account_id) else {
        chat_keep_draft(shell, text, cx);
        return;
    };
    let trpc = shell.borrow().workspaces.trpc(&account);
    let transport = shell.borrow().workspaces.attachments(&account);
    let team_id = chat.team_id.clone();
    shell.borrow_mut().chat_uploading = !jobs.is_empty();
    let shell = shell.clone();
    let typed = text.to_string();
    use api::attachment_upload::{AttachmentTransport as _, UploadSendError};
    cx.spawn(async move |cx| {
        // #EDE-65 lane L: the IDE's path: the staged files upload into the
        // team's session files, then the send carries their ids.
        let sent = cx
            .background_executor()
            .spawn(async move {
                let refused = |why: String| {
                    tracing::warn!("team chat: not sent, it could not be encrypted: {why}");
                    UploadSendError::Send(Vec::new(), no_key())
                };
                // #EDE-131: a tree this shell cannot seal for refuses before anything uploads.
                let wire = team_chat::wire_for(&team_chat::Server(&trpc), &tree).map_err(refused)?;
                let resolved = api::attachment_upload::upload_all(jobs, |filename, content_type, bytes| transport.upload_team_session_file(&team_id, filename, content_type, bytes))
                    .map_err(|(landed, e)| UploadSendError::Upload(landed, e))?;
                let ids = chat_files::attachment_ids(&resolved);
                // #EDE-134: sealed with the attachments' lines inside it.
                if let Some(text) = input.text.take() {
                    input.text = Some(wire.text(&text, &ids).map_err(|why| match refused(why) {
                        UploadSendError::Send(_, e) => UploadSendError::Send(resolved.clone(), e),
                        other => other,
                    })?);
                }
                input.attachment_ids = Some(ids).filter(|ids| !ids.is_empty());
                match api::sessions::send(&trpc, &input) {
                    Ok(out) => Ok((out, resolved)),
                    Err(e) => Err(UploadSendError::Send(resolved, e)),
                }
            })
            .await;
        shell.borrow_mut().chat_uploading = false;
        match sent {
            // The draft goes once the server took it; a failure hands it back.
            Ok((out, resolved)) => {
                if let Some(open) = shell.borrow_mut().chat_team.as_mut().filter(|c| c.team_id == chat.team_id) {
                    open.session_id = Some(out.session_id);
                }
                cx.update(|cx| {
                    chat_files_sent(&shell, &resolved, cx);
                    chat_clear_draft(&shell, cx);
                });
            }
            // What landed stays noted: a retry uploads only the rest.
            Err(UploadSendError::Upload(landed, e)) => cx.update(|cx| {
                shell.borrow_mut().chat_files.note_uploaded(&landed);
                failed_action(&shell, "upload", "the attachment", &e, cx);
                chat_keep_draft(&shell, &typed, cx);
            }),
            Err(UploadSendError::Send(landed, e)) => cx.update(|cx| {
                shell.borrow_mut().chat_files.note_uploaded(&landed);
                failed_action(&shell, "send", "the message", &e.user_message(), cx);
                chat_keep_draft(&shell, &typed, cx);
            }),
        }
    })
    .detach();
}

/// #EDE-65 lane L: the composer's attach control: the desktop's file
/// chooser (the xdg portal's), the picks staged above the composer; a pick
/// that does not fit is the IDE's notice as a toast.
fn chat_attach(shell: &SharedShell, cx: &mut App) {
    let on_tap: eengine_desktop::OnTap = Arc::new(|_| {});
    let Some(dialogs) = eengine_desktop::dialogs(&eengine_desktop::DesktopConfig::new("Exponential", on_tap)) else { return };
    let request = eengine_desktop::OpenRequest { title: "Attach".into(), filter: Default::default(), multiple: true };
    let shell = shell.clone();
    cx.spawn(async move |cx| {
        let picked = cx.background_executor().spawn(async move { dialogs.open(&request) }).await;
        let paths = match picked {
            Ok(Some(paths)) if !paths.is_empty() => paths,
            Ok(_) => return,
            Err(e) => {
                let message = match e {
                    eengine_desktop::NativeError::Failed(message) => message,
                    other => format!("{other:?}"),
                };
                cx.update(|cx| failed_action(&shell, "open", "the file chooser", &message, cx));
                return;
            }
        };
        cx.update(|cx| {
            let field = chat_field(&shell.borrow());
            chat_stage(&shell, field, &paths, cx);
        });
    })
    .detach();
}

/// #EXP-1845: the chat's drawn composer: the home's on the start face,
/// the conversation's otherwise.
fn chat_field(s: &Shell) -> &'static str {
    if s.feed.get("/chatting").and_then(|c| c["face"].as_str()) == Some("home") {
        "chat_home_composer"
    } else {
        "chat_composer"
    }
}

/// The composer `field`'s live text and caret (bytes); before the chat is
/// drawn, the host's draft with the caret at its end.
fn chat_field_draft(shell: &SharedShell, field: &str, cx: &App) -> (String, usize) {
    let s = shell.borrow();
    s.surfaces
        .region("chat")
        .and_then(|region| {
            let view = region.view.read(cx);
            Some((view.field_text(field, cx)?, view.field_caret(field, cx)?))
        })
        .unwrap_or_else(|| (s.chat_draft.clone(), s.chat_draft.len()))
}

/// #EXP-1845: a pick (the "+", a paste, a drop) staged the app composers'
/// way (`domain::composer_attachments`): an `[Image #N]` per taken image
/// typed at `field`'s caret, each refusal its own error toast.
fn chat_stage(shell: &SharedShell, field: &str, paths: &[std::path::PathBuf], cx: &mut App) {
    let (text, caret) = chat_field_draft(shell, field, cx);
    let outcome = shell.borrow_mut().chat_files.stage_paths(paths, &text, caret);
    for message in &outcome.toasts {
        toast(shell, message, "", cx);
    }
    if let Some((at, inserted)) = &outcome.inserted {
        // The field types it (its caret, its undo); its change follows.
        let region = shell.borrow().surfaces.region("chat").cloned();
        let typed = region.is_some_and(|region| {
            region.handle.update(cx, |_, window, cx| region.view.update(cx, |view, cx| view.insert_into_field(field, *at, inserted, window, cx))).unwrap_or(false)
        });
        if typed {
            shell.borrow_mut().chat_draft = outcome.text.clone();
            chat_menu_changed(shell);
        } else {
            chat_set_draft(shell, &outcome.text, cx);
        }
    }
    chat_files_changed(shell, cx);
}

/// #EXP-1845: a chip's X: an image's markers leave the draft, the higher
/// ones slide down, and the caret keeps the writer's place.
fn chat_unattach(shell: &SharedShell, key: u64, cx: &mut App) {
    let field = chat_field(&shell.borrow());
    let (text, caret) = chat_field_draft(shell, field, cx);
    let rewritten = shell.borrow_mut().chat_files.remove(key, &text, caret);
    if let Some((next, caret)) = rewritten {
        let region = shell.borrow().surfaces.region("chat").cloned();
        match region {
            Some(region) => {
                // A programmatic write: the field adopts it even focused.
                region.view.update(cx, |view, cx| {
                    view.caret_on_write(field, caret);
                    let _ = view.assign_data("/chat/draft", Some(json!(next)), cx);
                });
                shell.borrow_mut().chat_draft = next;
                chat_menu_changed(shell);
            }
            None => chat_set_draft(shell, &next, cx),
        }
    }
    chat_files_changed(shell, cx);
}

/// A send landed: what went out leaves the strip.
fn chat_files_sent(shell: &SharedShell, resolved: &[api::attachment_upload::Uploaded], cx: &mut App) {
    if resolved.is_empty() {
        return;
    }
    let sent: Vec<u64> = resolved.iter().map(|(key, _, _)| *key).collect();
    shell.borrow_mut().chat_files.settle_sent(&sent);
    chat_files_changed(shell, cx);
}

/// The strip above the composer (`/chatFiles`).
fn chat_files_changed(shell: &SharedShell, cx: &mut App) {
    let feed = shell.borrow().chat_files.feed();
    shell.borrow_mut().feed.set("/chatFiles", feed);
    refresh(shell, cx);
}

/// A question's option: on a team `sessions.answer` stamps it in the card
/// (the IDE's path); the desktop's runs take it as the next message.
fn chat_answer(shell: &SharedShell, message_id: &str, label: &str, cx: &mut App) {
    if label.trim().is_empty() {
        return;
    }
    if chat_local(shell) {
        let answered = shell.borrow_mut().chat_local.as_mut().map(|c| c.answer(label));
        if let Some(Err(e)) = answered {
            failed_action(shell, "answer", "the question", &e, cx);
        }
        chatting_changed(shell, cx);
        return;
    }
    let Some((chat, rows)) = team_chat(shell) else { return };
    let Some(account) = shell.borrow().workspaces.account(&chat.account_id) else { return };
    let trpc = shell.borrow().workspaces.trpc(&account);
    let input = api::sessions::AnswerInput { message_id: message_id.to_string(), answer: Some(label.to_string()), answers: Some(json!([label])), ..Default::default() };
    // #EDE-131: a sealed card's answer re-sealed; a card this shell cannot
    // seal for is refused here, never sent in clear.
    let input = match team_chat::answer_wire(&rows, &input) {
        Ok(input) => input,
        Err(why) => {
            failed_action(shell, "answer", "the question", &why, cx);
            return;
        }
    };
    let shell = shell.clone();
    cx.spawn(async move |cx| {
        let answered = cx.background_executor().spawn(async move { api::sessions::answer(&trpc, &input) }).await;
        if let Err(e) = answered {
            cx.update(|cx| failed_action(&shell, "answer", "the question", &e.user_message(), cx));
        }
    })
    .detach();
}

/// Lane Q: an Embed's inner event on chat card `message_id`, answered the
/// way the IDE does (`domain::chat_document::card_reply`): a show card's
/// `reply` = the person's next message into its session.
fn chat_card(shell: &SharedShell, message_id: &str, name: &str, context: &Value, cx: &mut App) {
    if chat_local(shell) {
        let replied = shell.borrow_mut().chat_local.as_mut().map(|c| c.card(message_id, name, context));
        if let Some(Err(e)) = replied {
            failed_action(shell, "send", "the message", &e, cx);
        }
        chatting_changed(shell, cx);
        return;
    }
    let Some((chat, rows)) = team_chat(shell) else { return };
    let Some(reply) = team_chat::card_reply(&rows, &chat, message_id, name, context, now_ms()) else { return };
    let mut input = team_chat::reply_input(&chat, &reply);
    let tree = team_chat::send_tree(&rows.account, &input);
    let Some(account) = shell.borrow().workspaces.account(&chat.account_id) else { return };
    let trpc = shell.borrow().workspaces.trpc(&account);
    let shell = shell.clone();
    cx.spawn(async move |cx| {
        let sent = cx
            .background_executor()
            .spawn(async move {
                // #EDE-131: sealed, plaintext only into a clear tree.
                if let Some(text) = input.text.take() {
                    let wire = team_chat::wire_text(&team_chat::Server(&trpc), &tree, &text).map_err(|why| {
                        tracing::warn!("team chat: not sent, it could not be encrypted: {why}");
                        no_key()
                    })?;
                    input.text = Some(wire);
                }
                api::sessions::send(&trpc, &input)
            })
            .await;
        if let Err(e) = sent {
            cx.update(|cx| failed_action(&shell, "send", "the message", &e.user_message(), cx));
        }
    })
    .detach();
}

/// #EDE-97: a send that could not be sealed (`noKeyRefusal`, the toast's
/// description).
fn no_key() -> api::ApiError {
    api::ApiError::Http { status: 400, message: team_chat::NO_KEY_REFUSAL.to_string() }
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

/// A card's or a rail chat's session (`None` = New chat: the home).
fn chat_open(shell: &SharedShell, session_id: Option<&str>, cx: &mut App) {
    let session_id = session_id.filter(|id| !id.is_empty());
    if chat_local(shell) {
        if let Some(chat) = shell.borrow_mut().chat_local.as_mut() {
            match session_id {
                Some(id) => chat.open(id),
                None => chat.new_chat(),
            }
        }
    } else if team_chat(shell).is_some() {
        if let Some(chat) = shell.borrow_mut().chat_team.as_mut() {
            chat.session_id = session_id.map(str::to_string);
        }
    }
    chatting_changed(shell, cx);
}

/// The draft is the document's: hand it an empty one, then let it own it again.
fn chat_clear_draft(shell: &SharedShell, cx: &mut App) {
    shell.borrow_mut().chat_draft.clear();
    shell.borrow_mut().feed.set("/chat", json!({"draft": ""}));
    chatting_changed(shell, cx);
    shell.borrow_mut().feed.remove("/chat");
}

/// The composer's text set by the host (a `/` menu row's insertion), then
/// the document's own again.
fn chat_set_draft(shell: &SharedShell, text: &str, cx: &mut App) {
    shell.borrow_mut().chat_draft = text.to_string();
    chat_menu_changed(shell);
    shell.borrow_mut().feed.set("/chat", json!({"draft": text}));
    refresh(shell, cx);
    shell.borrow_mut().feed.remove("/chat");
}

/// #EDE-134: a send that did not go hands its text back to the composer,
/// once its own emptying has landed; a field typed in since keeps what it
/// holds (composer-send.json `failed`).
fn chat_keep_draft(shell: &SharedShell, text: &str, cx: &mut App) {
    if text.trim().is_empty() {
        return;
    }
    let (shell, text) = (shell.clone(), text.to_string());
    cx.defer(move |cx| {
        if shell.borrow().chat_draft.trim().is_empty() {
            chat_set_draft(&shell, &text, cx);
        }
    });
}

/// #EDE-134: the busy composer's Stop (chat.json: submit becomes stop):
/// the open session's turn stops; on a Local desktop its run here, on a
/// team `sessions.stop` (the IDE's `stop_session`).
fn chat_stop(shell: &SharedShell, cx: &mut App) {
    if chat_local(shell) {
        let stopped = shell.borrow().chat_local.as_ref().and_then(|c| c.stop_session());
        if let Some((summary, body)) = stopped.and_then(|delivered| stop_toast(Ok(delivered))) {
            toast(shell, &summary, &body, cx);
        }
        chatting_changed(shell, cx);
        return;
    }
    let Some((chat, _)) = team_chat(shell) else { return };
    let Some(session_id) = chat.session_id else { return };
    let Some(account) = shell.borrow().workspaces.account(&chat.account_id) else { return };
    let trpc = shell.borrow().workspaces.trpc(&account);
    let shell = shell.clone();
    cx.spawn(async move |cx| {
        let stopped = cx.background_executor().spawn(async move { api::sessions::stop(&trpc, &session_id) }).await;
        if let Some((summary, body)) = stop_toast(stopped.map(|out| !out.not_delivered()).map_err(|e| e.user_message())) {
            cx.update(|cx| toast(&shell, &summary, &body, cx));
        }
    })
    .detach();
}

/// [`chat_stop`]'s toast (chats.json `copy.stopTurn`, the IDE's): a
/// refusal = `failed` with the reason, a stop no run got = `notDelivered`.
fn stop_toast(outcome: Result<bool, String>) -> Option<(String, String)> {
    let copy = &domain::chats::CHATS.copy.stop_turn;
    match outcome {
        Ok(true) => None,
        Ok(false) => Some((copy.not_delivered.clone(), String::new())),
        Err(message) => Some((copy.failed.clone(), domain::action_error::description_of(&message).unwrap_or_default())),
    }
}

/// THE `/` menu over the composer's draft (`/chatMenu`); shut for the
/// draft a pick closed it on.
fn chat_menu_changed(shell: &SharedShell) {
    let mut s = shell.borrow_mut();
    let (mut menu, keep) = if s.chat_menu_dismissed.as_deref() == Some(s.chat_draft.as_str()) {
        (json!({"open": false, "rows": []}), false)
    } else {
        // The IDE's rule: an Escape lasts exactly as long as its draft.
        s.chat_menu_dismissed = None;
        // `@` / `#` behind the caret first, else THE `/` menu.
        let current = &s.model.switcher.current;
        let query = domain::typeahead::detect_trigger(&s.chat_draft, s.chat_draft.len()).filter(|t| t.trigger == domain::typeahead::CompletionTrigger::IssueRef).map(|t| t.query);
        let ranked = query.map(|q| issues::rank_in(&s.issue_index, &current.kind, &current.id, &q, domain::typeahead::MAX_ITEMS)).unwrap_or_default();
        let ranked: Vec<chat::MentionIssue> = ranked.into_iter().map(Into::into).collect();
        match chat::mention_menu(&s.chat_draft, &s.chat_members, &ranked) {
            Some(menu) => (menu, false),
            None => (chat::slash_menu(&s.chat_draft, s.chat_cli.as_deref(), &s.chat_prompts), true),
        }
    };
    s.chat_menu_selected = chat::highlight(&mut menu, s.chat_menu_selected, keep);
    s.chat_menu = menu.clone();
    s.feed.set("/chatMenu", menu);
}

/// A menu row picked (its press, or Enter/Tab on the highlight): a prompt
/// loads its body, a builtin or a typeahead row puts its insertion in the
/// composer, which keeps the menu shut for it; nothing sends.
fn chat_menu_pick(shell: &SharedShell, insert: &str, prompt: &str, cx: &mut App) {
    if !prompt.is_empty() {
        return chat_load_prompt(shell, prompt, cx);
    }
    shell.borrow_mut().chat_menu_dismissed = Some(insert.to_string());
    chat_set_draft(shell, insert, cx);
}

/// The composer's menu keys (`ede_chat key {key}`, `chat::menu_key`, the IDE's).
fn chat_menu_key(shell: &SharedShell, key: &str, cx: &mut App) {
    let (rows, selected) = {
        let s = shell.borrow();
        (s.chat_menu["rows"].as_array().cloned().unwrap_or_default(), s.chat_menu_selected)
    };
    match chat::menu_key(key, selected, rows.len()) {
        chat::MenuKey::Highlight(index) => {
            let mut s = shell.borrow_mut();
            let mut menu = s.chat_menu.clone();
            s.chat_menu_selected = chat::highlight(&mut menu, index, true);
            s.chat_menu = menu.clone();
            s.feed.set("/chatMenu", menu);
        }
        chat::MenuKey::Pick(index) => {
            let field = |key: &str| rows[index][key].as_str().unwrap_or("").to_string();
            return chat_menu_pick(shell, &field("insert"), &field("prompt"), cx);
        }
        chat::MenuKey::Dismiss => {
            let draft = shell.borrow().chat_draft.clone();
            shell.borrow_mut().chat_menu_dismissed = Some(draft);
            chat_menu_changed(shell);
        }
        chat::MenuKey::Ignore => return,
    }
    refresh(shell, cx);
}

/// The current team's prompts off its account's store.
fn inbox_prompts(rows: &team_chat::TeamRows, team_id: &str) -> Vec<domain::rows::PromptRow> {
    rows.prompts.iter().filter(|p| p.team_id.as_deref() == Some(team_id)).cloned().collect()
}

/// NEW-2 (the IDE's `pick_prompt`): a picked prompt's BODY (`prompts.get`,
/// the shape is body-less) replaces the `/name` draft, to edit and send; a
/// draft that moved on meanwhile is left alone; a failure is the `Couldn't
/// load <name>` toast. The menu stays shut for the draft it was picked on.
fn chat_load_prompt(shell: &SharedShell, id: &str, cx: &mut App) {
    let name = shell.borrow().chat_prompts.iter().find(|p| p.id == id).and_then(|p| p.name.clone()).unwrap_or_else(|| "the prompt".into());
    let client = if chat_local(shell) {
        shell.borrow().chat_local.as_ref().map(|c| c.desktop.client())
    } else {
        let account = shell.borrow().chat_team.as_ref().and_then(|c| shell.borrow().workspaces.account(&c.account_id));
        account.map(|a| shell.borrow().workspaces.trpc(&a))
    };
    {
        let mut s = shell.borrow_mut();
        s.chat_menu_dismissed = Some(s.chat_draft.clone());
    }
    chat_menu_changed(shell);
    refresh(shell, cx);
    let Some(client) = client else {
        failed_action(shell, "load", &name, "", cx);
        return;
    };
    let id = id.to_string();
    let shell = shell.clone();
    cx.spawn(async move |cx| {
        let got = cx.background_executor().spawn(async move { api::prompts::get(&client, &id) }).await;
        cx.update(|cx| match got {
            Ok(prompt) if domain::slash_menu::slash_query(&shell.borrow().chat_draft).is_some() => chat_set_draft(&shell, &prompt.body, cx),
            Ok(_) => {}
            Err(e) => failed_action(&shell, "load", &name, &e.user_message(), cx),
        });
    })
    .detach();
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
}

/// #EDE-92: Up or Down pressed in `window`: in the start menu with results
/// of a search, the selection moves (and its row scrolls into view) and the
/// key is the menu's; anywhere else, or with nothing to select, it is not.
fn start_menu_step(shell: &SharedShell, window: gpui::AnyWindowHandle, step: i64, cx: &mut App) -> bool {
    let moved = {
        let Ok(mut s) = shell.try_borrow_mut() else { return false };
        let Some(name) = ["start-menu", "menu"].into_iter().find(|name| s.surfaces.region(name).is_some_and(|r| gpui::AnyWindowHandle::from(r.handle) == window)) else { return false };
        let Some(menu) = s.feed.get("/shell").map(|shell| shell["startMenu"].clone()) else { return false };
        let query = menu["query"].as_str().unwrap_or_default().to_string();
        let Some(row) = s.menu_selection.step(&query, start_menu::rows(&menu), step) else { return false };
        let apps = menu["apps"].as_array().map_or(0, Vec::len);
        (name, row, if row < apps { ("apps", row) } else { ("issues", row - apps) })
    };
    // The key is still being dispatched in that window: its view redraws after it.
    let shell = shell.clone();
    cx.defer(move |cx| {
        model_changed(&shell, cx);
        let (name, row, (list, index)) = moved;
        // The results' ScrollArea: a list that cannot scroll is left alone (its rows would go blank).
        // #EDE-133: the first row = the results' top, its group heading included.
        if row == 0 {
            shell.borrow().surfaces.reveal_top(name, "apps_scroll", cx);
        } else {
            shell.borrow().surfaces.reveal_row(name, "apps_scroll", list, index, cx);
        }
    });
    true
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
            shell.borrow().surfaces.reveal_top(name, "apps_scroll", cx);
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
        let hits = cx.background_executor().spawn(async move { issues::search(&index, &query) }).await;
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
        Some(doc) if matches!(doc.surface, SurfaceHeader::Layer { .. }) => open_layer(shell, &doc, cx),
        _ => tracing::warn!(document = name, "no such layer document"),
    }
}

/// The popup documents a press outside them closes (`popups.dismiss`):
/// those open when it was pressed (`pressed_at`, the event's stamp). One the
/// press itself opened (the start menu from its button) stays: its press
/// reaches the shell before the event does, or after (#EDE-89).
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
        "agent.toggle" => chat_toggle(shell, cx),
        "expo.toggle" => overlay_toggle(shell, "expo", cx),
        "scale.toggle" => overlay_toggle(shell, "scale", cx),
        "volume.up" | "volume.down" => osd_show(shell, system::volume_step(name == "volume.up"), cx),
        "volume.mute" => osd_show(shell, system::volume_toggle_mute(), cx),
        "mic.mute" => osd_show(shell, system::mic_toggle_mute(), cx),
        "brightness.up" | "brightness.down" => osd_show(shell, system::brightness_step(name == "brightness.up"), cx),
        "media.play" | "media.next" | "media.prev" => system::media(name),
        "terminal" => match system::terminal_command() {
            Some(t) => person_launch(shell, t),
            None => tracing::warn!("no terminal emulator installed"),
        },
        "screenshot" => screenshot(shell),
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
/// fullscreen window over the first display; the chat or Settings opened
/// after the view (a later top layer lies above an earlier one).
fn spaces_pause(s: &Shell) -> spaces::Pause {
    let opened = |name: &str| s.surfaces.is_open(name).then(|| s.layers_opened.get(name).copied().unwrap_or(0));
    let view = opened(spaces::VIEW).unwrap_or(u64::MAX);
    let view_covered = ["chat", "settings"].iter().any(|n| opened(n).is_some_and(|at| at > view));
    spaces::Pause { locked: s.locked, fullscreen: spaces::fullscreen_over(&s.windows, s.model.workspaces.active, s.first_output), view_covered }
}

/// What THE capture loop photographs, derived from what is open now (the
/// switcher, the previews, Expo; the Agent spaces list, the pinned view),
/// nothing while locked; the loop wakes only when it moved.
fn thumbnails_retarget(shell: &SharedShell) {
    let s = shell.borrow();
    let plan = spaces::plan(thumbnail_windows(&s), &s.spaces, s.surfaces.is_open(spaces::VIEW), spaces_list_open(&s), spaces_pause(&s), first_display(&s));
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
        captures::Out::Gone { space } => {
            shell.borrow_mut().spaces.gone(&space);
            shell.borrow().captures.ask(captures::Ask::List);
            spaces_changed(shell, cx);
        }
    }
}

/// `/spaces` and `/spaceView` from what the shell knows now.
fn spaces_feed(shell: &SharedShell) {
    let mut s = shell.borrow_mut();
    let display = first_display(&s);
    let (list, view) = (s.spaces.feed(), s.spaces.view_feed(display));
    s.feed.set("/spaces", list);
    s.feed.set("/spaceView", view);
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
    // The last space ended: the list closes with its button.
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

/// Print: the first output to `~/Pictures/Screenshots`.
fn screenshot(shell: &SharedShell) {
    let path = system::screenshot_path();
    let saved = with_ede(shell, |ede| {
        let outputs = ede.outputs()?;
        let Some(first) = outputs.iter().find(|o| o.is_persons()) else { return Ok(false) };
        let capture = ede.capture(ede_protocol::CaptureTarget::Output { name: first.name.clone() })?;
        Ok(thumbs::write_png(&path, &capture).is_ok())
    });
    if saved == Some(true) {
        tracing::info!(path = %path.display(), "screenshot saved");
    }
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
        dbus::Event::TrayMenu { service, entries } => tray_menu_show(shell, &service, &entries, cx),
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
    let (sources, read, wanted) = {
        let mut s = shell.borrow_mut();
        if s.tray_reading {
            s.tray_again = true;
            return;
        }
        s.tray_reading = true;
        (s.workspaces.tray_sources(), s.notifications.read_locally.clone(), s.spaces.wanted_sessions().into_iter().collect::<HashSet<String>>())
    };
    let shell = shell.clone();
    cx.spawn(async move |cx| {
        let (items, needs, space_sessions) = cx.background_executor().spawn(async move { tray_read(&sources, &read, &wanted, now_ms()) }).await;
        let again = {
            let mut s = shell.borrow_mut();
            s.tray_reading = false;
            std::mem::take(&mut s.tray_again)
        };
        cx.update(|cx| {
            // #EXP-2024: the spaces' titles ride on the same read.
            if shell.borrow().spaces.sessions != space_sessions {
                shell.borrow_mut().spaces.sessions = space_sessions;
                spaces_changed(&shell, cx);
            }
            tray_apply(&shell, items, needs, cx);
            if again {
                exp_tray_refresh(&shell, cx);
            }
        });
    })
    .detach();
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
        s.feed.set("/needsYou", inbox::needs_you_feed(&needs));
        s.needs_you = needs;
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
fn outputs_refresh(shell: &SharedShell) {
    let outputs = with_ede(shell, |ede| ede.outputs()).unwrap_or_default();
    let mut s = shell.borrow_mut();
    s.first_output = outputs.iter().find(|o| o.is_persons()).map(|o| (o.x, o.y, o.width, o.height));
    s.feed.set("/outputs", Value::Array(settings::outputs_json(&outputs)));
}

/// Reads the tray items again, on the tray thread: [`dbus::Event::TrayItems`]
/// (#EDE-135: a hung tray app never holds up the shell).
fn tray_refresh(shell: &SharedShell) {
    if let Some(d) = shell.borrow().dbus.as_ref() {
        d.tray_refresh();
    }
}

/// A tray item pressed: its menu ([`dbus::Event::TrayMenu`]), else `Activate`.
fn tray_press(shell: &SharedShell, service: &str) {
    let has_menu = shell.borrow().tray.iter().find(|i| i.service == service).is_some_and(|i| i.has_menu);
    if let Some(d) = shell.borrow().dbus.as_ref() {
        if has_menu {
            d.tray_menu(service);
        } else {
            d.tray_activate(service);
        }
    }
}

/// A pressed item's menu as a popup.
fn tray_menu_show(shell: &SharedShell, service: &str, entries: &[dbus::MenuEntry], cx: &mut App) {
    let title = shell.borrow().tray.iter().find(|i| i.service == service).map(|i| i.title.clone()).unwrap_or_default();
    let items: Vec<Value> = entries
        .iter()
        .map(|e| {
            let mut v = e.to_json();
            if e.depth > 0 {
                v["label"] = json!(format!("    {}", e.label));
            }
            v
        })
        .collect();
    shell.borrow_mut().feed.set("/traymenu", json!({"service": service, "title": title, "items": items}));
    if shell.borrow().surfaces.is_open("tray-menu") {
        close(shell, "tray-menu", cx);
    }
    toggle_popup(shell, "tray-menu", cx);
    refresh(shell, cx);
}

/// The taskbar's pins, kept in the shell's own file.
fn pins_persist(shell: &SharedShell) {
    let pinned = shell.borrow().model.taskbar.pinned.clone();
    settings::persist_shell(json!({"pinned": pinned}));
}

/// The compositor's window and workspace lists (one round trip each); the
/// person's windows only (#EDE-91).
fn lists(calls: &mut ede_protocol::Client) -> Option<(Vec<ede_protocol::WindowInfo>, Vec<ede_protocol::WorkspaceInfo>)> {
    let windows = ede_protocol::persons(calls.windows().ok()?);
    let workspaces = calls.workspaces().unwrap_or_default();
    Some((windows, workspaces))
}

/// The sample windows of the goldens (`EDE_SHELL_DEMO=1`): three apps, one grouped.
fn demo_windows() -> Vec<ede_protocol::WindowInfo> {
    let w = |id: &str, app: &str, title: &str, focused: bool| ede_protocol::WindowInfo { id: ede_protocol::WindowId::new(id).unwrap(), title: Some(title.into()), app_id: Some(app.into()), x: 0, y: 0, width: 800, height: 600, focused, fullscreen: false, protected: false, x11: false, workspace: 0, minimized: false, space: None };
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
    let (outputs_tx, outputs_rx) = mpsc::channel::<()>();
    // #EXP-2024: the session's lock (the capture loop stops while it holds).
    let (locks_tx, locks_rx) = mpsc::channel::<bool>();
    std::thread::Builder::new()
        .name("ede-shell-events".into())
        .spawn(move || {
            let Ok(mut events) = control::connect() else {
                tracing::info!("no ede-protocol socket: the taskbar stays empty");
                return;
            };
            let Ok(mut calls) = control::connect() else { return };
            if !demo {
                if let Some(list) = lists(&mut calls) {
                    let _ = windows_tx.send(list);
                }
            }
            if events.subscribe().is_err() {
                return;
            }
            while let Ok(event) = events.next_event() {
                // #EDE-91: an agent space's windows and outputs are not the person's.
                if !ede_protocol::is_persons_event(&event) {
                    continue;
                }
                if event.kind == ede_protocol::events::SESSION_LOCKED || event.kind == ede_protocol::events::SESSION_UNLOCKED {
                    if locks_tx.send(event.kind == ede_protocol::events::SESSION_LOCKED).is_err() {
                        return;
                    }
                } else if event.kind == ede_protocol::events::HOTKEY {
                    if let Some(name) = event.data.get("name").and_then(Value::as_str) {
                        if hotkeys_tx.send((name.to_string(), event.at)).is_err() {
                            return;
                        }
                    }
                } else if event.kind.starts_with("output.") {
                    if outputs_tx.send(()).is_err() {
                        return;
                    }
                } else if !demo && (event.kind.starts_with("window.") || event.kind.starts_with("workspace.")) {
                    if let Some(list) = lists(&mut calls) {
                        if windows_tx.send(list).is_err() {
                            return;
                        }
                    }
                }
            }
        })
        .expect("the events thread");
    let mut installed = if demo { apps::demo() } else { apps::scan() };
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
    workspaces.start_sync();
    let team_deltas = workspaces.deltas();
    let (connect_tx, connect_rx) = mpsc::channel::<connect::Event>();
    let (urls_tx, urls_rx) = mpsc::channel::<String>();
    if !demo {
        connect::listen_urls(urls_tx);
    }
    let connect = connect::Connect::new(workspaces.auth.clone(), connect_tx);
    let (share_tx, share_rx) = mpsc::channel::<share::Event>();
    let share = share::Share::new(share_tx);

    gpui_platform::application().with_assets(assets::Assets).run(move |cx: &mut App| {
        // #EDE-172 (P4): gpui's Wayland connection did its first roundtrip.
        control::refuse_ptrace();
        gpui_component::init(cx);
        let _ = cx.text_system().add_fonts(assets::fonts());
        // #EDE-65: the IDE's colours: the sidebar on the ground, no Sidebar fill.
        let theme = uicore_gpui::inline::theme(&eui_core::themes::builtin_theme("exponential").expect("the exponential theme"));
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
        feed.set("/traymenu", json!({"service": "", "title": "", "items": []}));
        feed.set("/config", settings::config_json());
        feed.set("/pages", settings_pages::pages_json());
        if let Some(state) = settings_pages::demo_state() {
            feed.set("/settings", state);
        }
        feed.set("/user", settings::user_json());
        feed.set("/notifications", json!({"toasts": [], "list": [], "count": 0}));
        feed.set("/needsYou", json!({"count": 0, "rows": []}));
        feed.set("/spaces", json!({"count": 0, "rows": []}));
        feed.set("/spaceView", json!({"open": false}));
        // The goldens open the Agent spaces list through its bound `open`.
        let demo_spaces = spaces::demo_mode().filter(|_| demo);
        feed.set("/spacesList", json!({"open": demo_spaces.as_deref() == Some("list")}));
        feed.set("/connecting", connect.feed());
        feed.set("/accounts", workspaces.accounts_json());
        feed.set("/sharing", settings_pages::demo_sharing().unwrap_or_else(|| share.feed()));
        // The goldens: the sample conversation (Local, or a demo team's).
        let on_team = workspaces.current_local().is_some_and(|e| e.kind == uicore_model::SwitcherKind::Team);
        // EXP-1513: `EDE_SHELL_DEMO_SETUP` = a Local desktop without a CLI at that setup step.
        let setup = demo_setup().filter(|_| demo && !on_team);
        let mut chat = match (demo, on_team) {
            (true, false) if setup.is_none() => local_runs::demo_input(),
            (true, true) => team_chat::demo_input(),
            _ => local_runs::assistant_home(),
        };
        // #EDE-65 lane O: the "+" (Photo or file; the prompts arrive with a team's rows).
        chat.plus = chat::plus_rows(&[]);
        let mut chatting = chat::chatting(&uicore_model::chat::chat_view(&chat));
        if let Some(step) = &setup {
            chatting["noCli"] = json!(true);
            chatting["setup"] = step.to_value();
        }
        feed.set("/chatting", chatting);
        feed.set("/chatFiles", if demo && std::env::var_os("EDE_SHELL_DEMO_ATTACH").is_some() { chat_files::demo().feed() } else { chat_files::ChatFiles::default().feed() });
        // The goldens: `EDE_SHELL_DEMO_DRAFT` types into the demo composer (`/` opens the menu).
        if let Some(draft) = std::env::var("EDE_SHELL_DEMO_DRAFT").ok().filter(|_| demo) {
            feed.set("/chat", json!({"draft": draft}));
            let mut menu = chat::mention_menu(&draft, &chat::demo_members(), &chat::demo_issues().into_iter().map(Into::into).collect::<Vec<chat::MentionIssue>>()).unwrap_or_else(|| chat::slash_menu(&draft, Some("claude"), &chat::demo_prompts()));
            // `EDE_SHELL_DEMO_HIGHLIGHT` = the row the menu keys moved to.
            chat::highlight(&mut menu, std::env::var("EDE_SHELL_DEMO_HIGHLIGHT").ok().and_then(|i| i.parse().ok()).unwrap_or(0), true);
            feed.set("/chatMenu", menu);
        }
        if demo {
            feed.set("/osd", json!({"icon": "media-volume-down", "level": 45, "label": "45%"}));
            feed.set("/tray", json!([{"service": "demo/network", "title": "Network", "status": "Active", "icon": ""}, {"service": "demo/sound", "title": "Sound", "status": "Active", "icon": ""}]));
            feed.set(
                "/traymenu",
                json!({"service": "demo/network", "title": "Network", "items": [
                    {"id": 1, "label": "Connect to Wi-Fi", "enabled": true, "separator": false, "checked": false, "checkable": false, "depth": 0},
                    {"id": 2, "label": "Airplane mode", "enabled": true, "separator": false, "checked": false, "checkable": true, "depth": 0},
                    {"id": 3, "label": "", "enabled": true, "separator": true, "checked": false, "checkable": false, "depth": 0},
                    {"id": 4, "label": "Settings…", "enabled": true, "separator": false, "checked": false, "checkable": false, "depth": 0}
                ]}),
            );
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
            model = reduce(&model, &ShellEvent::TaskbarPin { app: app.into() });
        }
        // The switcher: the Local desktops and the accounts' teams, the
        // index's current desktop selected.
        let entries = workspaces.entries();
        if !entries.is_empty() {
            model = reduce(&model, &ShellEvent::SwitcherSet { entries });
        }
        if let Some(current) = workspaces.current_local() {
            model = reduce(&model, &ShellEvent::SwitcherPick { entry_kind: current.kind, id: current.id });
        }
        let saved_team = workspaces.saved_team();
        // A start on a Local desktop: a running Exponential app follows it too.
        if saved_team.is_none() && !demo {
            if let Some(current) = workspaces.current_local() {
                accounts::ide_follow(current.kind, &current.id);
            }
        }
        let menu_apps: Vec<StartMenuApp> = installed.iter().map(apps::App::start_menu_app).collect();
        let favourites = model.taskbar.pinned.clone();
        feed.set("/shell", uicore_data::shell_feed(&model, &menu_apps, &favourites));
        let applets = applets_json(feed.get("/services").unwrap_or(&Value::Null));
        feed.set("/applets", applets);

        let shell_cell: Rc<RefCell<Option<SharedShell>>> = Rc::new(RefCell::new(None));
        // The agents' glyphs, shared by the shell and every host (a host is
        // made while the shell is borrowed: it never borrows the shell for them).
        let chat_glyphs: AgentGlyphs = Rc::new(RefCell::new(if demo { team_chat::demo_glyphs() } else { HashMap::new() }));
        let plugin_for: uicore_gpui::PluginFor = {
            let cell = shell_cell.clone();
            let hosted_tx = hosted_tx.clone();
            let chat_glyphs = chat_glyphs.clone();
            Rc::new(move |document: &str| {
                let shell = cell.borrow().clone().expect("the shell exists before a surface opens");
                let glyphs = chat_glyphs.clone();
                Rc::new(Host { shell, document: document.to_string(), hosted_tx: hosted_tx.clone(), glyphs }) as Rc<dyn HostPlugin>
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
            chats: if demo { chats::demo_chats() } else { Vec::new() },
            ede: control::connect().ok(),
            apps: installed,
            menu_apps,
            windows: Vec::new(),
            thumbnails: HashMap::new(),
            captures: captures_shared,
            spaces: demo_spaces.as_deref().map(spaces::Spaces::demo).unwrap_or_default(),
            locked: false,
            first_output: None,
            layers_opened: HashMap::new(),
            osd_until: None,
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
            tray_reading: false,
            tray_again: false,
            demo,
            // The demo toasts only through `--open notifications`.
            started_ms: if demo { i64::MAX / 2 } else { now_ms() },
            dbus,
            tray: Vec::new(),
            workspaces,
            connect,
            share,
            local: None,
            // #EDE-65: the team picked last, once its account's teams are listed.
            pending_pick: saved_team,
            chat_local: None,
            chat_setup: cli_setup::Step::Idle,
            chat_setup_rx: None,
            local_sessions: None,
            chat_agent: None,
            chat_draft: String::new(),
            // #EDE-129: `EDE_SHELL_DEMO_FIND` opens the bar on a query (the goldens, the pictures).
            chat_find: std::env::var("EDE_SHELL_DEMO_FIND").ok().filter(|q| demo && !q.is_empty()).map(|query| ChatFind { query: domain::find_in_chat::query(&query), land: true, ..Default::default() }),
            // The goldens: `EDE_SHELL_DEMO_ATTACH` stages a screenshot.
            chat_files: if demo && std::env::var_os("EDE_SHELL_DEMO_ATTACH").is_some() { chat_files::demo() } else { chat_files::ChatFiles::default() },
            chat_uploading: false,
            chat_cli: None,
            chat_prompts: Vec::new(),
            chat_members: Vec::new(),
            chat_menu_dismissed: None,
            chat_menu: Value::Null,
            chat_menu_selected: 0,
            chat_team: None,
            chat_rows: None,
            chat_glyphs,
            chat_composer: Value::Null,
            engine: None,
            engine_seats: engine::exposes::seats(Vec::new()),
            launches: system::LaunchGuard::default(),
            engine_apps: Vec::new(),
            launchable: Arc::new(Vec::new()),
            team_stores: None,
            issue_index: Arc::new(Vec::new()),
            issue_hits: Vec::new(),
            issue_search: 0,
            menu_selection: start_menu::Selection::default(),
            time_changes: Vec::new(),
            _watcher: watcher,
        }));
        // #EDE-92: Up and Down in the start menu's search move its selection.
        {
            let s = shell.clone();
            cx.intercept_keystrokes(move |event, window, cx| {
                let Some(step) = start_menu::key_step(&event.keystroke.key, event.keystroke.modifiers.modified()) else { return };
                if start_menu_step(&s, window.window_handle(), step, cx) {
                    cx.stop_propagation();
                }
            })
            .detach();
        }
        *shell_cell.borrow_mut() = Some(shell.clone());
        chats_refresh(&shell, cx);
        history_refresh(&shell);
        overlays_refresh(&shell);
        exp_tray_refresh(&shell, cx);
        model_changed(&shell, cx);
        if !demo {
            // The outputs first: a stretched surface takes the output's size.
            outputs_refresh(&shell);
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
            // #EDE-65 lane I: an open team chat repaints at most every 250 ms while a reply streams.
            let (mut chat_pending, mut last_chat) = (false, Instant::now());
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
                    let before = output_size(&s.borrow());
                    outputs_refresh(&s);
                    if output_size(&s.borrow()) != before {
                        cx.update(|cx| relayout(&s, cx));
                    }
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
                }
                // #EDE-56: a team store changed, a callback URL arrived, Connect reported.
                let mut teams_changed = false;
                let mut records_changed = false;
                let mut chats_changed = false;
                let mut tray_changed = false;
                let mut team_chat_changed = false;
                while let Ok(delta) = team_deltas.try_recv() {
                    if let sync::ShapeDelta::Applied { shape, account_id, keys, full_replace, .. } = &delta {
                        if inbox::refreshes_tray(shape) {
                            tray_changed = true;
                        }
                        // #EDE-65 lane I: the team chat's rows moved; #EDE-141: noted
                        // (no read), the tick reads them and repaints only for the open chat's.
                        let noted = {
                            let mut sh = s.borrow_mut();
                            let rows = sh.chat_rows.as_mut().filter(|r| r.account_id() == account_id.as_str());
                            rows.map(|r| r.note(shape, keys, *full_replace)).is_some()
                        };
                        if noted && team_chat::SHAPES.contains(shape) {
                            team_chat_changed = true;
                        }
                        match *shape {
                            "teams" | "team_members" => teams_changed = true,
                            // #EDE-65: the current team's chats.
                            "sessions" | "session_reads" => chats_changed |= s.borrow().model.switcher.current.account.as_ref() == Some(account_id),
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
                    s.borrow_mut().connect.callback(&url);
                }
                while let Ok(event) = connect_rx.try_recv() {
                    let signed = s.borrow_mut().connect.apply(event);
                    if let Some(account) = signed {
                        s.borrow().workspaces.start_account(&account);
                        let accounts = s.borrow().workspaces.accounts_json();
                        s.borrow_mut().feed.set("/accounts", accounts);
                        teams_changed = true;
                    }
                    let feed = s.borrow().connect.feed();
                    s.borrow_mut().feed.set("/connecting", feed);
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
                chat_pending |= team_chat_changed;
                if chat_pending && last_chat.elapsed() >= Duration::from_millis(250) {
                    chat_pending = false;
                    last_chat = Instant::now();
                    let open = s.borrow().surfaces.is_open("chat");
                    if open && team_chat_rows(&s, true).is_some_and(|(_, _, moved)| moved) {
                        cx.update(|cx| chatting_changed(&s, cx));
                    }
                }
                if tray_pending && last_tray.elapsed() >= Duration::from_secs(1) {
                    tray_pending = false;
                    last_tray = Instant::now();
                    cx.update(|cx| exp_tray_refresh(&s, cx));
                }
                // EXP-1513: the chat's Claude Code setup moved on.
                let setup = s.borrow().chat_setup_rx.as_ref().and_then(|rx| rx.drain().last());
                if let Some(step) = setup {
                    cx.update(|cx| chat_setup_step(&s, step, cx));
                }
                // #EDE-58: the local agent's runs changed a session or its messages.
                let chat_changed = s.borrow().chat_local.as_ref().and_then(|c| c.changes.as_ref()).map(|rx| rx.drain().count() > 0).unwrap_or(false);
                if chat_changed {
                    cx.update(|cx| chatting_changed(&s, cx));
                }
                // #EDE-134: the other app's runner wrote (a reply, a status): the rail too.
                chats_changed |= chat_changed;
                chats_changed |= s.borrow().local_sessions.as_ref().is_some_and(|rx| rx.drain().any(|d| d.table == "sessions"));
                if chats_changed {
                    cx.update(|cx| chats_refresh(&s, cx));
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
        let mut s = doc.surface(eui_core::themes::builtin_theme("exponential").expect("the theme"));
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
            let parsed: serde_json::Value = serde_json::from_str(doc.source).expect(doc.name);
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
        let docs: Vec<(&str, serde_json::Value)> = uicore_docs::DOCS.iter().map(|d| (d.name, serde_json::from_str(d.source).expect(d.name))).collect();
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
        let parsed: Value = serde_json::from_str(doc.source).unwrap();
        let tile = parsed["root"]["children"].as_array().unwrap().iter().find(|c| c["id"] == "tile").expect("the tile");
        assert_eq!(tile["on"]["hover"], json!({"functionCall": {"@call": "ede_alt_tab_select", "args": {"window": {"@path": "id"}}}}));
    }

    #[test]
    fn the_popup_hugs_its_tiles_and_stays_on_the_output() {
        assert_eq!(switcher_width(3, 1920), 2 * 16 + 3 * 208 + 2 * 8);
        assert_eq!(switcher_width(0, 1920), switcher_width(1, 1920));
        assert_eq!(switcher_width(40, 1280), 1280 - 32);
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
            taskbar_popup_room(&mut header, out_h);
            let drawn = as_drawn(&doc, header.clone(), None);
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
        assert_eq!(popup_height("applet-network", 534), 350, "a popup that fits keeps its size");
        assert_eq!(popup_height("applet-sound", 800), 600, "1280×800 logical has room for all of it");
        for d in uicore_docs::DOCS.iter() {
            let doc = factory(d.name);
            if matches!(&doc.surface, SurfaceHeader::Popup { popup, .. } if popup.parent == "taskbar") {
                assert!(popup_height(d.name, 534) <= room, "{}", d.name);
            }
        }
    }

    /// #EXP-2024 (section 8, 6d): the pinned view of an agent space is an
    /// on-demand top layer at the bottom right, above the taskbar, that
    /// never takes the keyboard; the taskbar lists the spaces in a hosted
    /// Popover (never a popup document: those take the keyboard) and may
    /// call the two functions the view and the list use.
    #[test]
    fn the_space_view_takes_no_keyboard_and_the_list_is_hosted() {
        let view = factory(spaces::VIEW);
        let SurfaceHeader::Layer { layer, anchor, keyboard, on_demand, functions, margin, .. } = &view.surface else { panic!("a layer") };
        assert_eq!((*layer, *keyboard, *on_demand), (DocLayer::Top, ede_ui::surface::Keyboard::None, true));
        assert_eq!(anchor, &vec![ede_ui::surface::Edge::Bottom, ede_ui::surface::Edge::Right]);
        assert_eq!((margin.right, margin.bottom), (shell_layout().panel_margin, shell_layout().panel_margin), "beside the edge, above the taskbar's zone");
        assert_eq!(functions, &vec!["ede_space".to_string()]);
        let taskbar: Value = serde_json::from_str(uicore_docs::doc("taskbar").unwrap().source).unwrap();
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
        let list = node(&taskbar["root"], spaces::LIST).expect("the Agent spaces list");
        assert_eq!(list["component"], "Popover", "the list is a hosted Popover of the taskbar");
        assert_eq!(list["slots"]["trigger"]["props"]["label"], "Agent spaces");
        assert!(!uicore_docs::DOCS.iter().any(|d| d.source.contains("\"anchor\": \"agent_spaces")), "no popup document anchors at it");
        let functions: Vec<&str> = taskbar["surface"]["functions"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert!(functions.contains(&"ede_space") && functions.contains(&"ede_space_close"));
    }

    /// #EDE-65: Settings open into the sidebar like the IDE's: over the
    /// rail and the ground, standing on the taskbar, the nav as wide as the
    /// IDE's settings column.
    #[test]
    fn settings_take_the_sidebar_and_the_ground() {
        let settings = factory("settings");
        for out in [(1280, 720), (1920, 1080)] {
            assert_eq!(size(&layer_header(&settings, out)), (Some(out.0), Some(out.1 - 48), -1), "{out:?}");
        }
        let SurfaceHeader::Layer { margin, on_demand, .. } = &settings.surface else { panic!("a layer") };
        assert_eq!((margin.bottom, *on_demand), (shell_layout().taskbar_height, true));
        let doc: Value = serde_json::from_str(uicore_docs::doc("settings").unwrap().source).unwrap();
        let nav = &doc["root"]["children"][0];
        assert_eq!((nav["component"].as_str(), nav["props"]["width"].as_u64()), (Some("Sidebar"), Some(shell_layout().settings_width as u64)));
    }

    /// The taskbar opens before the sidebar: its exclusive zone takes the
    /// whole bottom edge, the rail stands on it.
    /// #EDE-65 (after #2308): a team's rail chat opens in eDE's chat like a
    /// Local one; the Exponential app only when the team chat cannot.
    #[test]
    fn a_rail_chat_opens_in_edes_chat_and_hands_off_only_as_the_fallback() {
        assert_eq!(rail_chat_route(true, None), RailChat::Chat);
        assert_eq!(rail_chat_route(false, Some(true)), RailChat::Chat);
        assert_eq!(rail_chat_route(false, Some(false)), RailChat::HandOff, "a session the team chat has not synced");
        assert_eq!(rail_chat_route(false, None), RailChat::HandOff, "no team chat (no account store yet)");
    }

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
        let doc: Value = serde_json::from_str(uicore_docs::doc("sidebar").unwrap().source).unwrap();
        let row = serde_json::to_string(&doc["root"]).unwrap();
        assert!(row.contains(r#""functionCall":{"@call":"ede_chat","args":{"sessionId":{"@path":"id"}}}"#), "the rail's chat calls ede_chat {{sessionId}}");
    }

    /// #EDE-65 lane P: both composers hand their menu keys to the shell
    /// while /chatMenu is open, and both menus paint the highlighted row.
    #[test]
    fn the_chat_composers_hand_the_menu_keys_to_the_shell() {
        let doc: Value = serde_json::from_str(uicore_docs::doc("chat").unwrap().source).unwrap();
        let mut composers = 0;
        let mut rows = 0;
        let mut walk = vec![&doc["root"]];
        while let Some(node) = walk.pop() {
            match node["id"].as_str() {
                Some("chat_home_composer" | "chat_composer") => {
                    assert_eq!(node["props"]["menuKeys"], json!({"@path": "/chatMenu/open"}));
                    assert_eq!(node["on"]["key"], json!({"event": {"name": "ede_chat", "context": {"action": "key"}}}));
                    composers += 1;
                }
                Some("chat_home_slash_row" | "chat_slash_row") => {
                    assert_eq!(node["props"]["selected"], json!({"@path": "selected"}));
                    rows += 1;
                }
                _ => {}
            }
            walk.extend(node["children"].as_array().into_iter().flatten());
        }
        assert_eq!((composers, rows), (2, 2));
    }

    /// #EDE-134: a busy composer's submit is `stop` (chat.json), routed to
    /// `ede_chat {action: stop}`; its toasts are chats.json's `stopTurn`.
    #[test]
    fn the_busy_composers_stop_reaches_the_shell_and_says_what_failed() {
        let doc: Value = serde_json::from_str(uicore_docs::doc("chat").unwrap().source).unwrap();
        let mut stops = 0;
        let mut walk = vec![&doc["root"]];
        while let Some(node) = walk.pop() {
            // The home's composer is never busy (no turn runs there yet).
            if node["id"] == json!("chat_composer") {
                assert_eq!(node["on"]["stop"], json!({"functionCall": {"@call": "ede_chat", "args": {"action": "stop"}}}));
                stops += 1;
            }
            walk.extend(node["children"].as_array().into_iter().flatten());
        }
        assert_eq!(stops, 1);
        assert_eq!(super::stop_toast(Ok(true)), None);
        assert_eq!(super::stop_toast(Ok(false)), Some(("The device didn't get the stop".into(), String::new())));
        assert_eq!(super::stop_toast(Err(" The session ended. ".into())), Some(("Couldn't stop the turn".into(), "The session ended.".into())));
    }
}

#[cfg(test)]
mod chat_as_drawn {
    use super::*;

    fn factory(name: &str) -> Document {
        uicore_gpui::inline::document(uicore_docs::DOCS.iter().find(|d| d.name == name).expect(name))
    }

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

    fn prop(doc: &Document, id: &str, prop: &str) -> Value {
        let ede_ui::Body::Nested(tree) = &doc.body else { panic!("a nested chat") };
        let tree = serde_json::to_value(tree).unwrap();
        node(&tree, id).unwrap_or_else(|| panic!("no {id}"))["props"].get(prop).cloned().unwrap_or(Value::Null)
    }

    /// #EDE-141: the chat's composer stays as `/chatting` says (busy, its
    /// placeholder, the + menu) through an output change (`relayout`) and
    /// an edit of chat.json on disk (`apply_change`). Both reloaded the
    /// document as the file has it (`busy: false`, `items: []`), and
    /// `chat_composer` never repaired it (its model had not changed).
    #[test]
    fn the_chat_keeps_its_composer_through_a_relayout_and_an_edit_on_disk() {
        let chatting = json!({"composer": {"placeholder": "Reply to Triage", "busy": true, "menu": [{"id": "photo", "label": "Photo or file"}]}, "setup": {}});
        let doc = factory("chat");
        assert_eq!(prop(&doc, "chat_composer", "busy"), json!(false), "the file's literal");
        let relaid = relaid(&doc, layer_header(&doc, (1920, 1080)), Some(&chatting)).expect("the chat takes the output's new size");
        let redrawn = as_drawn(&doc, layer_header(&doc, (1280, 720)), Some(&chatting));
        for (how, drawn) in [("relayout", &relaid), ("an edit on disk", &redrawn)] {
            assert_eq!(prop(drawn, "chat_composer", "busy"), json!(true), "{how}");
            assert_eq!(prop(drawn, "chat_composer", "placeholder"), json!("Reply to Triage"), "{how}");
            for plus in ["chat_plus", "chat_home_plus"] {
                assert_eq!(prop(drawn, plus, "items"), chatting["composer"]["menu"], "{how}: {plus}");
            }
        }
        let ede_ui::Body::Nested(root) = &relaid.body else { panic!("a nested chat") };
        let (w, h) = engine::window_max((1920, 1080));
        assert_eq!(root.style.as_ref().map(|s| (s["width"].clone(), s["height"].clone())), Some((json!(w), json!(h))), "relayout's size");
    }
}

#[cfg(test)]
mod previews {
    use super::{popups_to_dismiss, previews_after};

    /// #EDE-89: a press on Start while a preview is open: the compositor's
    /// `popups.dismiss` for that press reaches the shell after the press
    /// opened the start menu (the main loop drains it up to 100 ms later).
    /// The menu opened after the press and stays; a popup open before it goes.
    #[test]
    fn a_press_that_opens_the_start_menu_does_not_dismiss_it() {
        let pressed_at = 1_000;
        let open = vec![("start-menu".to_string(), Some(pressed_at + 12)), ("tray-menu".to_string(), Some(pressed_at - 4_000))];
        assert_eq!(popups_to_dismiss(&open, pressed_at), vec!["tray-menu".to_string()]);
        // Escape in the open menu (or a press outside it) still closes it.
        assert_eq!(popups_to_dismiss(&open, pressed_at + 500), vec!["start-menu".to_string(), "tray-menu".to_string()]);
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
        let doc = uicore_docs::doc("taskbar").unwrap().source;
        assert!(doc.contains(r#""name": "ede_previews""#), "the event");
        assert!(!doc.contains(r#""@call": "ede_previews""#), "no function call: its args never get `open`");
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
        };
        assert_eq!(ide_command(&app, Some("exponential://issue/ACME-12")), "/opt/Exponential/exponential-desktop 'exponential://issue/ACME-12'");
        assert_eq!(ide_command(&app, None), "/opt/Exponential/exponential-desktop");
        let no_urls = apps::App { exec_url: None, ..app };
        assert_eq!(ide_command(&no_urls, Some("exponential://issue/ACME-12")), "/opt/Exponential/exponential-desktop");
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

    /// #EDE-65: the start menu holds the desktop/team switcher (the
    /// sidebar's entries and rules, moved) and the Settings button.
    #[test]
    fn the_start_menu_holds_the_switcher_and_settings() {
        let doc: Value = serde_json::from_str(uicore_docs::doc("start-menu").expect("the start menu").source).unwrap();
        let functions: Vec<&str> = doc["surface"]["functions"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert!(functions.contains(&"ede_switcher") && functions.contains(&"ede_open_document"), "{functions:?}");
        let root = &doc["root"];
        let left = node(root, "left").expect("the left column");
        assert!(node(left, "switcher").is_some(), "the switcher sits in the person's column");
        let menu = node(root, "switcher_menu").expect("the switcher's menu");
        assert_eq!(menu["props"]["anchor"], "switcher");
        assert_eq!(node(menu, "switcher_sections").unwrap()["template"]["path"], "/shell/switcher/sections");
        let (f, args) = call(node(menu, "switcher_entry").unwrap());
        assert_eq!((f, args["action"].as_str()), ("ede_switcher", Some("pick")));
        for (id, action) in [("switcher_new", "new"), ("switcher_share", "share"), ("switcher_connect", "connect")] {
            let (f, args) = call(node(menu, id).expect(id));
            assert_eq!((f, args["action"].as_str()), ("ede_switcher", Some(action)), "{id}");
        }
        let visible = |id: &str| node(menu, id).unwrap()["visible"].to_string();
        assert!(visible("switcher_share").contains("/shell/switcher/connection/onLocal"));
        assert!(visible("switcher_connect").contains("/shell/switcher/connection/connected") && visible("switcher_connect").contains("\"not\""));
        assert!(!doc.to_string().contains("/shell/ide/"), "the IDE's rows stay in its sidebar");
        let (f, args) = call(node(root, "settings").expect("the Settings button"));
        assert_eq!((f, args["name"].as_str()), ("ede_open_document", Some("settings")));
    }

    /// #EDE-65: the search lists Apps, then Issues (each opening in the app).
    #[test]
    fn the_start_menu_lists_apps_then_issues() {
        let doc: Value = serde_json::from_str(uicore_docs::doc("start-menu").expect("the start menu").source).unwrap();
        assert!(doc["surface"]["functions"].as_array().unwrap().iter().any(|f| f == "ede_issue"));
        let scroll = node(&doc["root"], "apps_scroll").expect("the results");
        let order: Vec<&str> = scroll["children"].as_array().unwrap().iter().filter_map(|c| c["id"].as_str()).collect();
        assert_eq!(order, ["apps_label", "apps", "app", "issues_label", "issues", "issue", "no_apps"]);
        assert_eq!(node(scroll, "issues").unwrap()["template"]["path"], "/shell/startMenu/issues");
        let (f, args) = call(node(scroll, "issue").unwrap());
        assert_eq!((f, &args["identifier"]), ("ede_issue", &serde_json::json!({"@path": "identifier"})));
    }

    /// #EDE-92: the result rows draw the shell's selection with the Row's
    /// selected state, and Enter in the search box asks the shell to run it;
    /// the footer's buttons keep their own presses.
    #[test]
    fn the_start_menu_selects_a_result_and_enter_runs_it() {
        let source = uicore_docs::doc("start-menu").expect("the start menu").source;
        let doc: Value = serde_json::from_str(source).unwrap();
        let root = &doc["root"];
        for id in ["app", "issue"] {
            assert_eq!(node(root, id).expect(id)["props"]["selected"], serde_json::json!({"@path": "selected"}), "{id}");
        }
        let submit = &node(root, "search").expect("the search box")["on"]["submit"]["functionCall"];
        assert_eq!((submit["@call"].as_str(), &submit["args"]), (Some("ede_start_menu"), &serde_json::json!({"activate": true})));
        assert!(doc["$comment"].as_str().unwrap().contains("#EDE-92"), "the document says what the keys do");
        for (id, f) in [("whats_new", "ede_launch"), ("account", "ede_launch"), ("devices", "ede_launch"), ("settings", "ede_open_document")] {
            assert_eq!(call(node(root, id).expect(id)).0, f, "{id}");
        }
        assert_eq!(source.matches("\"activate\"").count(), 1, "only the search box runs the selection");
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
mod sender_avatar {
    /// #EDE-65 lane O: every sender avatar's glyph resolves to a plain icon
    /// when it is no host glyph (the Assistant's brand mark, the session's
    /// and a person's), so the disc never falls back to genUI's placeholder.
    #[test]
    fn a_sender_avatar_paints_plain_icons() {
        let white = gpui::Hsla::white();
        for name in [crate::team_chat::ASSISTANT_ICON, "coding-assistant", "user"] {
            assert!(super::plain_icon(name, 18.0, white).is_some(), "{name}");
        }
        assert!(super::plain_icon("no-such-icon", 18.0, white).is_none());
        // Whatever size genUI's Icon hands over: the IDE's 32 px disc, 18 px glyph.
        assert_eq!(super::avatar_sizes(), (domain::transcript_grouping::layout().avatar, 18.0));
    }
}
