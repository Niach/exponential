//! #EDE-237: eDE mode's windows. On Exponential Desktop the holder
//! (`exponential-app`, [`crate::ede_component`]) hosts every page as a
//! toplevel of its own, all in one process over one `sync::Store`:
//!
//! - **One window per page.** A page is keyed by its target
//!   ([`window_key`]: `board:<id>`, `issue:<id>`, `chat:<root>`, `home`, …);
//!   a navigation to a page another window holds focuses that window, to a
//!   page no window holds opens one ([`route`]). A move inside the page (a
//!   chat to a session of its tree, a filed draft to its issue) stays in its
//!   window. Every navigation funnels through `navigation::navigate_inner`,
//!   so in-page links (`links::open_link`, an `AppLink`) and the bus's
//!   `open {link, token}` take the same path.
//! - **No rail and no tab strip** (`Shell`): the desktop is the chrome; the
//!   taskbar groups the windows under the one Exponential button.
//! - **What each window is** ([`api::bus::WindowMeta`]: kind, concept icon,
//!   title, team) goes to the shell over the session bus
//!   ([`api::bus::Request::Windows`]); the toplevel's title is the page's.
//! - **Restore.** The open pages are written to `ede-windows.json` in the
//!   data directory as they change and reopened when the holder starts
//!   again, after a restart or a crash ([`restore`]). Closing a window
//!   forgets it; quitting keeps the list.
//!
//! The standalone app is untouched: nothing here runs unless
//! [`crate::ede_component::is_component`].

use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Duration,
};

use api::bus::{WindowBoard, WindowMeta};
use gpui::{AnyWindowHandle, App, Global, Window, WindowId};
use sync::Store;

use crate::navigation::{self, ChatSeed, NewItemKind, Screen};

/// The restore file's name in the data directory.
const RESTORE_FILE: &str = "ede-windows.json";

/// The most windows a start reopens.
const RESTORE_MAX: usize = 24;

/// How long a fresh window's first navigation lands in it, not beside it.
const LAUNCH_WINDOW: Duration = Duration::from_secs(3);

// ---- The pure rules -------------------------------------------------------

/// The page `screen` is, as a window key; `None` = no window of its own
/// (a terminal stays in the window it was opened from; the dev screens).
/// `chat_root` = the chat a session belongs to (it shares the chat's
/// window). A board with no id is the window's active board: callers
/// resolve it first ([`normalize`]).
pub(crate) fn window_key(screen: &Screen, chat_root: impl Fn(&str) -> Option<String>) -> Option<String> {
    Some(match screen {
        Screen::BoardIssues { board_id } => format!("board:{board_id}"),
        Screen::IssueDetail { issue_id } => format!("issue:{issue_id}"),
        Screen::Chat { session_id } => format!("chat:{session_id}"),
        Screen::Session { session_id } => match chat_root(session_id) {
            Some(root) => format!("chat:{root}"),
            None => format!("session:{session_id}"),
        },
        Screen::Agent { agent_id } => format!("agent:{agent_id}"),
        Screen::Device { device_id } => format!("device:{device_id}"),
        Screen::DeviceSettings { device_id } => format!("device-settings:{device_id}"),
        Screen::Repository { repository_id } => format!("repository:{repository_id}"),
        // An app's run view and its editor are one page (one tab before).
        Screen::AppRun { app } | Screen::App { app } => format!("app:{app}"),
        Screen::IssueDraft { draft_id, .. } => format!("draft:{draft_id}"),
        Screen::NewItem { team_id, kind } => format!("new-{}:{team_id}", new_item_word(*kind)),
        Screen::CiRun { run_id, .. } => format!("ci:{run_id}"),
        Screen::Home => "home".into(),
        Screen::NeedsYou => "needs-you".into(),
        Screen::Chats => "chats".into(),
        Screen::Drafts => "drafts".into(),
        Screen::Settings => "settings".into(),
        Screen::Files => "files".into(),
        Screen::SourceControl => "source-control".into(),
        Screen::Terminal { .. } | Screen::ExponentialUiKitchenSink | Screen::ExponentialUiSpecimen { .. } | Screen::Specimen { .. } => return None,
    })
}

fn new_item_word(kind: NewItemKind) -> &'static str {
    match kind {
        NewItemKind::Board => "board",
        NewItemKind::Agent => "agent",
        NewItemKind::App => "app",
        NewItemKind::Repository => "repository",
    }
}

/// The page a restored key reopens; `None` = not reopened (a creation page,
/// an unknown key).
pub(crate) fn screen_of_key(key: &str) -> Option<Screen> {
    let (kind, id) = key.split_once(':').unwrap_or((key, ""));
    let id = id.to_string();
    let with_id = |screen: Screen| (!id.is_empty()).then_some(screen);
    match kind {
        "board" => with_id(Screen::BoardIssues { board_id: id.clone() }),
        "issue" => with_id(Screen::IssueDetail { issue_id: id.clone() }),
        "chat" => with_id(Screen::Chat { session_id: id.clone() }),
        "session" => with_id(Screen::Session { session_id: id.clone() }),
        "agent" => with_id(Screen::Agent { agent_id: id.clone() }),
        // `device:` = this device.
        "device" => Some(Screen::Device { device_id: id }),
        "device-settings" => Some(Screen::DeviceSettings { device_id: id }),
        "repository" => with_id(Screen::Repository { repository_id: id.clone() }),
        // The run view: reopening the editor would start its live session.
        "app" => with_id(Screen::AppRun { app: id.clone() }),
        "draft" => with_id(Screen::IssueDraft { draft_id: id.clone(), board_id: String::new(), status_id: None }),
        "ci" => with_id(Screen::CiRun { run_id: id.clone(), job: None }),
        "home" => Some(Screen::Home),
        "needs-you" => Some(Screen::NeedsYou),
        "chats" => Some(Screen::Chats),
        "drafts" => Some(Screen::Drafts),
        "settings" => Some(Screen::Settings),
        "files" => Some(Screen::Files),
        "source-control" => Some(Screen::SourceControl),
        _ => None,
    }
}

/// A page's kind and its concept icon (`packages/icons`), the shell's
/// thumbnail and window list wear them.
pub(crate) fn kind_and_icon(screen: &Screen) -> (&'static str, &'static str) {
    match screen {
        Screen::BoardIssues { .. } => ("board", "nav-boards"),
        Screen::IssueDetail { .. } => ("issue", "ui-issue"),
        Screen::Chat { .. } => ("chat", "action-chat"),
        Screen::Session { .. } => ("session", "session-tree"),
        Screen::Agent { .. } => ("agent", "settings-agents"),
        Screen::Device { .. } => ("device", "nav-devices"),
        Screen::DeviceSettings { .. } => ("device-settings", "nav-settings"),
        Screen::Repository { .. } => ("repository", "action-repository"),
        Screen::AppRun { .. } | Screen::App { .. } => ("app", "nav-apps"),
        Screen::IssueDraft { .. } => ("draft", "nav-create-issue"),
        Screen::NewItem { .. } => ("new", "ui-add"),
        Screen::CiRun { .. } => ("ci", "ci-running"),
        Screen::Home => ("home", "nav-home"),
        Screen::NeedsYou => ("needs-you", "nav-needs-you"),
        Screen::Chats => ("chats", "nav-chats"),
        Screen::Drafts => ("drafts", "nav-drafts"),
        Screen::Settings => ("settings", "nav-settings"),
        Screen::Files => ("files", "nav-files"),
        Screen::SourceControl => ("source-control", "nav-source-control"),
        Screen::Terminal { .. } => ("terminal", "nav-terminal"),
        Screen::ExponentialUiKitchenSink | Screen::ExponentialUiSpecimen { .. } | Screen::Specimen { .. } => ("dev", "nav-apps"),
    }
}

/// Where a navigation to the page `target` goes from a window showing
/// `current` (`None` = a window that never navigated: it shows Home).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Route {
    /// In this window: the same page, or a screen with no window of its own.
    Here,
    /// Focus the window already holding it.
    Focus(usize),
    /// A window of its own.
    Open,
}

/// The pure rule of [`route`]: `open` = the keys of the other windows, in
/// order; `launch` = this window was just opened for the navigation (a
/// restore, a link with no window up): it becomes the page.
pub(crate) fn route_for(target: Option<&str>, current: Option<&str>, open: &[Option<String>], launch: bool) -> Route {
    let Some(target) = target else { return Route::Here };
    if current == Some(target) {
        return Route::Here;
    }
    if let Some(at) = open.iter().position(|key| key.as_deref() == Some(target)) {
        return Route::Focus(at);
    }
    if launch {
        Route::Here
    } else {
        Route::Open
    }
}

// ---- The holder's windows -------------------------------------------------

/// What a window to come is opened for.
#[derive(Default)]
struct PendingPage {
    screen: Option<Screen>,
    /// The window it was opened from: the new one takes its team and board.
    source: Option<WindowId>,
    seed: Option<ChatSeed>,
    /// Links to open in it once it is up (a bus link with no window).
    links: Vec<String>,
}

#[derive(Default)]
struct PageWindows {
    /// The page windows, oldest first.
    order: Vec<AnyWindowHandle>,
    /// What each one was last published as.
    meta: Vec<(WindowId, WindowMeta)>,
    /// Windows whose next navigation lands in them.
    launch: HashSet<WindowId>,
    pending: VecDeque<PendingPage>,
    /// The keys last written to the restore file.
    written: Vec<String>,
    /// The app is quitting: its windows closing is not the person closing them.
    quitting: bool,
}

impl Global for PageWindows {}

/// The holder's windows as the bus hands them out.
pub fn window_board() -> Arc<WindowBoard> {
    static BOARD: OnceLock<Arc<WindowBoard>> = OnceLock::new();
    BOARD.get_or_init(Default::default).clone()
}

/// The app id the holder's toplevels carry (set once by `app`).
pub fn set_app_id(app_id: &str) {
    let _ = APP_ID.set(app_id.to_string());
}

static APP_ID: OnceLock<String> = OnceLock::new();

/// The holder's toplevels' app id.
pub fn app_id() -> String {
    APP_ID.get().cloned().unwrap_or_default()
}

/// `Shell::new`: a page window came up. Takes what it was opened for and
/// lands it once the window is built.
pub(crate) fn register(window: &mut Window, cx: &mut App) {
    if !crate::ede_component::is_component() {
        return;
    }
    let handle = window.window_handle();
    let page = {
        let windows = cx.default_global::<PageWindows>();
        windows.order.push(handle);
        windows.pending.pop_front()
    };
    let Some(page) = page else { return };
    let window_id = handle.window_id();
    cx.global_mut::<PageWindows>().launch.insert(window_id);
    // The launch lasts for the first navigation, or a moment.
    cx.spawn(async move |cx| {
        cx.background_executor().timer(LAUNCH_WINDOW).await;
        cx.update(|cx| {
            if let Some(windows) = cx.try_global::<PageWindows>() {
                if windows.launch.contains(&window_id) {
                    cx.global_mut::<PageWindows>().launch.remove(&window_id);
                }
            }
        });
    })
    .detach();
    window.defer(cx, move |window, cx| {
        if let Some(source) = page.source {
            navigation::seed_window_team(window, cx, source);
        }
        if let Some(seed) = page.seed {
            navigation::set_pending_chat_seed(window, cx, seed);
        }
        if let Some(screen) = page.screen {
            navigation::navigate_from_rail(window, cx, screen);
        }
        let handle = window.window_handle();
        cx.defer(move |cx| {
            for link in page.links {
                open_link_in(link, handle, cx);
            }
        });
    });
}

/// One link of the bus in `window`: an `exponential://` link through the
/// app's URL handler, an in-app URL (`https://<instance>/t/<team>/…`, `~/…`)
/// through the link opener, as a click on it in a page. Never from inside
/// the window's own update: the URL handler updates the window it lands in,
/// and a re-entrant update silently does nothing.
fn open_link_in(link: String, window: AnyWindowHandle, cx: &mut App) {
    if link.starts_with(&format!("{}://", api::login::OAUTH_CALLBACK_SCHEME)) {
        crate::handle_open_urls(vec![link], cx);
    } else {
        let _ = window.update(cx, |_, window, cx| crate::links::open_agent_link(&link, window, cx));
    }
}

/// `Shell`'s release: the window closed.
pub(crate) fn unregister(window_id: WindowId, cx: &mut App) {
    if !crate::ede_component::is_component() || cx.try_global::<PageWindows>().is_none() {
        return;
    }
    let windows = cx.global_mut::<PageWindows>();
    windows.order.retain(|handle| handle.window_id() != window_id);
    windows.meta.retain(|(id, _)| *id != window_id);
    windows.launch.remove(&window_id);
    publish_board(cx);
}

/// The app is quitting: its windows stay in the restore file.
pub fn quitting(cx: &mut App) {
    if cx.has_global::<PageWindows>() {
        cx.global_mut::<PageWindows>().quitting = true;
    }
}

/// The page window a global action or a link with no window of its own
/// runs in: the active one, else a window just opened for it, else the
/// newest. `None` off eDE mode or with no window.
pub(crate) fn any_page_window(cx: &App) -> Option<AnyWindowHandle> {
    let windows = cx.try_global::<PageWindows>()?;
    if let Some(active) = cx.active_window().filter(|active| windows.order.iter().any(|w| w.window_id() == active.window_id())) {
        return Some(active);
    }
    windows
        .order
        .iter()
        .rev()
        .find(|w| windows.launch.contains(&w.window_id()))
        .or_else(|| windows.order.last())
        .copied()
}

/// A board with no id is the window's active board; an agent by a legacy
/// id is its `agents` row.
fn normalize(screen: Screen, window_id: WindowId, cx: &App) -> Screen {
    match screen {
        Screen::BoardIssues { board_id } if board_id.is_empty() => {
            let board_id = navigation::nav_for_window_id(window_id, cx).and_then(|nav| navigation::active_board_id(&nav, cx)).unwrap_or_default();
            Screen::BoardIssues { board_id }
        }
        Screen::Agent { agent_id } => Screen::Agent { agent_id: crate::agent_screen::resolve_agent_id(&agent_id, cx) },
        screen => screen,
    }
}

fn key_of(screen: &Screen, window_id: WindowId, cx: &App) -> Option<String> {
    let screen = normalize(screen.clone(), window_id, cx);
    window_key(&screen, |session_id| crate::screens::in_place_root(session_id, cx))
}

/// The page `window_id` shows now (a window that never navigated shows its
/// default, Home).
fn shown(window_id: WindowId, cx: &App) -> Option<Screen> {
    let nav = navigation::nav_for_window_id(window_id, cx)?;
    navigation::resolved_screen(&nav, cx).or(Some(Screen::Home))
}

/// `navigation::navigate_inner` in eDE mode: `true` = handled here (another
/// window focused or opened), the caller leaves this window alone.
pub(crate) fn route(window: &Window, screen: &Screen, cx: &mut App) -> bool {
    if !crate::ede_component::is_component() || cx.try_global::<PageWindows>().is_none() {
        return false;
    }
    let this = window.window_handle().window_id();
    if !cx.global::<PageWindows>().order.iter().any(|w| w.window_id() == this) {
        return false;
    }
    let target = key_of(screen, this, cx);
    let current = shown(this, cx).and_then(|shown| key_of(&shown, this, cx));
    let others: Vec<AnyWindowHandle> = cx.global::<PageWindows>().order.iter().copied().filter(|w| w.window_id() != this).collect();
    let open: Vec<Option<String>> = others.iter().map(|w| shown(w.window_id(), cx).and_then(|shown| key_of(&shown, w.window_id(), cx))).collect();
    let launch = cx.global_mut::<PageWindows>().launch.remove(&this);
    // Home carries the composer's seed in its window's navigation.
    let seed = if *screen == Screen::Home { navigation::take_window_chat_seed(this, cx) } else { None };
    match route_for(target.as_deref(), current.as_deref(), &open, launch) {
        Route::Here => {
            if let Some(seed) = seed {
                navigation::set_window_chat_seed(this, seed, cx);
            }
            false
        }
        Route::Focus(at) => {
            let other = others[at];
            let landing = normalize(screen.clone(), this, cx);
            let handle = window.window_handle();
            // Deferred: a cross-window update from inside this window's
            // own silently does nothing.
            cx.defer(move |cx| {
                let _ = other.update(cx, |_, window, cx| {
                    if let Some(seed) = seed {
                        navigation::set_pending_chat_seed(window, cx, seed);
                    }
                    // The same page, maybe another state of it (a session
                    // of the chat, the app's editor): a move in its window.
                    navigation::navigate(window, cx, landing);
                    window.activate_window();
                });
                // A window opened for this navigation is not needed.
                if launch {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                }
            });
            true
        }
        Route::Open => {
            open_page(Some(normalize(screen.clone(), this, cx)), Some(this), seed, Vec::new(), cx);
            true
        }
    }
}

/// Opens a window for `screen` (or Home), taking `source`'s team.
fn open_page(screen: Option<Screen>, source: Option<WindowId>, seed: Option<ChatSeed>, links: Vec<String>, cx: &mut App) {
    cx.default_global::<PageWindows>().pending.push_back(PendingPage { screen, source, seed, links });
    if !crate::window_hooks::request_shell_window(cx) {
        cx.global_mut::<PageWindows>().pending.pop_back();
    }
}

/// The bus's `open {link}` in eDE mode: a link (`exponential://…` or an
/// in-app URL) lands on its page's window (opened, or focused when one
/// holds it); with no window up yet it opens one for the link. With no link: the newest window comes forward, else
/// Home opens.
pub(crate) fn open_from_bus(link: Option<String>, cx: &mut App) {
    let window = any_page_window(cx);
    match (link, window) {
        (Some(link), Some(window)) => open_link_in(link, window, cx),
        (Some(link), None) => open_page(None, None, None, vec![link], cx),
        (None, Some(window)) => {
            let _ = window.update(cx, |_, window, _| window.activate_window());
        }
        (None, None) => open_page(None, None, None, Vec::new(), cx),
    }
}

/// `Shell`'s render in eDE mode: the window's title is its page's, and
/// what it is goes to the shell.
pub(crate) fn publish(window: &mut Window, cx: &mut App) {
    if !crate::ede_component::is_component() || cx.try_global::<PageWindows>().is_none() || Store::try_global(cx).is_none() {
        return;
    }
    let window_id = window.window_handle().window_id();
    let Some(screen) = shown(window_id, cx) else { return };
    let screen = normalize(screen, window_id, cx);
    let Some(meta) = meta_of(&screen, window_id, cx) else { return };
    let previous = cx.global::<PageWindows>().meta.iter().find(|(id, _)| *id == window_id).map(|(_, meta)| meta.clone());
    if previous.as_ref() == Some(&meta) {
        return;
    }
    if previous.as_ref().is_none_or(|previous| previous.title != meta.title) {
        window.set_window_title(&meta.title);
    }
    let windows = cx.global_mut::<PageWindows>();
    match windows.meta.iter_mut().find(|(id, _)| *id == window_id) {
        Some(entry) => entry.1 = meta,
        None => windows.meta.push((window_id, meta)),
    }
    publish_board(cx);
}

fn meta_of(screen: &Screen, window_id: WindowId, cx: &App) -> Option<WindowMeta> {
    let key = window_key(screen, |session_id| crate::screens::in_place_root(session_id, cx)).unwrap_or_else(|| format!("window:{window_id:?}"));
    let (kind, icon) = kind_and_icon(screen);
    let title = navigation::screen_title(screen, cx).to_string();
    let nav = navigation::nav_for_window_id(window_id, cx)?;
    let team_id = navigation::team_of_screen(screen, &nav, cx).or_else(|| navigation::screen_team_id(&nav, cx));
    let team = team_id.as_ref().and_then(|id| Store::global(cx).collections().teams.read(cx).get(id).map(|team| team.name.clone()));
    Some(WindowMeta { key, kind: kind.into(), icon: icon.into(), title, team, team_id })
}

/// The windows in order to the bus, and their pages to the restore file.
fn publish_board(cx: &mut App) {
    let windows = cx.global::<PageWindows>();
    let metas: Vec<WindowMeta> = windows
        .order
        .iter()
        .filter_map(|handle| windows.meta.iter().find(|(id, _)| *id == handle.window_id()).map(|(_, meta)| meta.clone()))
        .collect();
    window_board().set(metas.clone());
    if windows.quitting {
        return;
    }
    let keys: Vec<String> = metas.into_iter().map(|meta| meta.key).filter(|key| screen_of_key(key).is_some()).collect();
    if keys == windows.written {
        return;
    }
    cx.global_mut::<PageWindows>().written = keys.clone();
    let path = restore_path();
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = write_restore(&path, &keys) {
                log::warn!("[ede-windows] {}: {err}", path.display());
            }
        })
        .detach();
}

fn restore_path() -> PathBuf {
    api::default_data_dir().join(RESTORE_FILE)
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct RestoreFile {
    windows: Vec<String>,
}

fn write_restore(path: &std::path::Path, keys: &[String]) -> std::io::Result<()> {
    let body = serde_json::to_vec_pretty(&RestoreFile { windows: keys.to_vec() }).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, path)
}

fn read_restore(path: &std::path::Path) -> Vec<String> {
    std::fs::read(path).ok().and_then(|body| serde_json::from_slice::<RestoreFile>(&body).ok()).map(|file| file.windows).unwrap_or_default()
}

/// The pages to reopen from the restore file's keys: known ones, once
/// each, at most [`RESTORE_MAX`].
pub(crate) fn restorable(keys: &[String]) -> Vec<Screen> {
    let mut seen = HashSet::new();
    keys.iter().filter(|key| seen.insert(key.as_str())).filter_map(|key| screen_of_key(key)).take(RESTORE_MAX).collect()
}

/// The holder starts: the windows open when it last ended come back.
/// `true` = some did.
pub fn restore(cx: &mut App) -> bool {
    if !crate::ede_component::is_component() {
        return false;
    }
    let keys = read_restore(&restore_path());
    let screens = restorable(&keys);
    cx.default_global::<PageWindows>().written = keys;
    log::info!("[ede-windows] reopening {} window(s)", screens.len());
    for screen in &screens {
        open_page(Some(screen.clone()), None, None, Vec::new(), cx);
    }
    !screens.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_chat(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn a_page_is_keyed_by_its_target() {
        assert_eq!(window_key(&Screen::IssueDetail { issue_id: "i1".into() }, no_chat).as_deref(), Some("issue:i1"));
        assert_eq!(window_key(&Screen::BoardIssues { board_id: "b1".into() }, no_chat).as_deref(), Some("board:b1"));
        // A session of a chat shares the chat's window; any other its own.
        let root = |id: &str| (id == "child").then(|| "root".to_string());
        assert_eq!(window_key(&Screen::Session { session_id: "child".into() }, root).as_deref(), Some("chat:root"));
        assert_eq!(window_key(&Screen::Chat { session_id: "root".into() }, root).as_deref(), Some("chat:root"));
        assert_eq!(window_key(&Screen::Session { session_id: "lone".into() }, root).as_deref(), Some("session:lone"));
        // An app's run view and editor are one page.
        assert_eq!(window_key(&Screen::App { app: "a".into() }, no_chat), window_key(&Screen::AppRun { app: "a".into() }, no_chat));
        assert_eq!(window_key(&Screen::ExponentialUiKitchenSink, no_chat), None);
        assert_eq!(window_key(&Screen::Home, no_chat).as_deref(), Some("home"));
    }

    #[test]
    fn restored_keys_reopen_their_pages() {
        for screen in [
            Screen::IssueDetail { issue_id: "i1".into() },
            Screen::BoardIssues { board_id: "b1".into() },
            Screen::Chat { session_id: "s1".into() },
            Screen::Agent { agent_id: "a1".into() },
            Screen::Device { device_id: String::new() },
            Screen::Device { device_id: "d1".into() },
            Screen::AppRun { app: "slug".into() },
            Screen::CiRun { run_id: "r1".into(), job: None },
            Screen::Home,
            Screen::NeedsYou,
            Screen::Chats,
            Screen::Drafts,
            Screen::Settings,
        ] {
            let key = window_key(&screen, no_chat).unwrap();
            assert_eq!(screen_of_key(&key), Some(screen), "{key}");
        }
        // The editor reopens as the run view; creation pages and junk not at all.
        assert_eq!(screen_of_key("app:a"), Some(Screen::AppRun { app: "a".into() }));
        assert_eq!(screen_of_key("new-board:t1"), None);
        assert_eq!(screen_of_key("issue:"), None);
        assert_eq!(screen_of_key("nonsense"), None);
        let keys = vec!["issue:i1".to_string(), "issue:i1".into(), "junk".into(), "home".into()];
        assert_eq!(restorable(&keys), vec![Screen::IssueDetail { issue_id: "i1".into() }, Screen::Home]);
        let many: Vec<String> = (0..40).map(|i| format!("issue:{i}")).collect();
        assert_eq!(restorable(&many).len(), RESTORE_MAX);
    }

    #[test]
    fn an_open_target_is_focused_a_new_one_opens() {
        let open = vec![Some("board:b1".to_string()), Some("issue:i1".to_string())];
        assert_eq!(route_for(Some("issue:i1"), Some("board:b1"), &open, false), Route::Focus(1));
        assert_eq!(route_for(Some("issue:i2"), Some("board:b1"), &open, false), Route::Open);
        // A move inside the page, or a screen with no window of its own.
        assert_eq!(route_for(Some("chat:r"), Some("chat:r"), &open, false), Route::Here);
        assert_eq!(route_for(None, Some("chat:r"), &open, false), Route::Here);
        // A window opened for the navigation becomes the page, unless
        // another already holds it.
        assert_eq!(route_for(Some("issue:i2"), Some("home"), &open, true), Route::Here);
        assert_eq!(route_for(Some("issue:i1"), Some("home"), &open, true), Route::Focus(1));
    }

    #[test]
    fn every_page_icon_is_a_concept() {
        let screens = [
            Screen::BoardIssues { board_id: String::new() },
            Screen::IssueDetail { issue_id: String::new() },
            Screen::Chat { session_id: String::new() },
            Screen::Session { session_id: String::new() },
            Screen::Agent { agent_id: String::new() },
            Screen::Device { device_id: String::new() },
            Screen::DeviceSettings { device_id: String::new() },
            Screen::AppRun { app: String::new() },
            Screen::IssueDraft { draft_id: String::new(), board_id: String::new(), status_id: None },
            Screen::NewItem { team_id: String::new(), kind: NewItemKind::Board },
            Screen::CiRun { run_id: String::new(), job: None },
            Screen::Home,
            Screen::NeedsYou,
            Screen::Chats,
            Screen::Drafts,
            Screen::Settings,
            Screen::Files,
            Screen::SourceControl,
            Screen::ExponentialUiKitchenSink,
        ];
        for screen in screens {
            let (_, icon) = kind_and_icon(&screen);
            assert!(crate::icons::registry::concept_by_name(icon).is_some(), "{icon}");
        }
    }

    #[test]
    fn the_restore_file_round_trips() {
        let dir = std::env::temp_dir().join(format!("ede-windows-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(RESTORE_FILE);
        assert!(read_restore(&path).is_empty());
        write_restore(&path, &["issue:i1".into(), "home".into()]).unwrap();
        assert_eq!(read_restore(&path), vec!["issue:i1".to_string(), "home".into()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
