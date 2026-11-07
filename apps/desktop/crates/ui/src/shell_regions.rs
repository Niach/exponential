//! #EXP-1725: the IDE's regions on uicore. The sidebar's rows are the shared
//! document (`uicore_docs`, `apps/ede/ui/sidebar.json`) mounted in place: this
//! file gathers what the rows show (`uicore_model::ide`, fixture-locked),
//! feeds it to `/shell/ide` and answers the document's events on the IDE's
//! navigation. A row's glyph stays the IDE's to paint: the data names it
//! `ide:<kind>:<id>` and [`RegionHost::icon_element`] draws the tinted board,
//! the agent's animated mark, a status dot or a count.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use domain::session_tree::SessionState;
use eui_core::{surface::SurfaceCommand, theme::Mode};
use eui_gpui::{
    host::{ActionEvent, FunctionCallEvent, HostPlugin, InputEvent},
    view::SurfaceView,
};
use gpui::{anchored, deferred, div, point, prelude::*, px, AnyElement, AnyWindowHandle, App, Entity, Hsla, SharedString, Window};
use gpui_component::{ActiveTheme as _, Icon, Sizable as _};
use serde_json::{json, Value};
use sync::Store;
use uicore_model::{reduce, ShellEvent, ShellState, StartMenuApp};
use uicore_model::ide::{ide_sidebar, IdeActiveInput, IdeAgentInput, IdeAppInput, IdeBoardInput, IdeDot, IdePinInput, IdeSidebarInput};

use crate::navigation::{self, active_board_id, active_team_id, navigate, resolved_screen, Navigation, Screen};
use crate::settings::SettingsSection;

/// What the sidebar document draws itself; the rest is the IDE's own until
/// its region moves (the header, the footer).
const PRUNED: &[&str] = &["header", "switcher_menu", "header_rail", "nav", "pinned", "workspaces", "sessions", "settings", "rail_settings"];

/// The start menu without what only a desktop session has.
const MENU_PRUNED: &[&str] = &["lock", "power"];

const TERMINAL: &str = "ide.terminal";
const FILES: &str = "ide.files";
const HISTORY: &str = "ide.history";
const SOURCE_CONTROL: &str = "ide.source-control";

/// A team app's start-menu id ([`uicore_model::EXPONENTIAL_APP_PREFIX`]: the
/// menu's Exponential category).
fn team_app(id: &str) -> String {
    format!("{}{id}", uicore_model::EXPONENTIAL_APP_PREFIX)
}

fn tool(id: &str, name: &str) -> StartMenuApp {
    StartMenuApp { id: id.into(), name: name.into(), comment: String::new(), categories: vec!["Development".into()], keywords: String::new() }
}

/// What the footer's Computer menu held, gated like it was (EXP-1105: yolo
/// mode hides Files and Source Control until a git failure needs a person).
fn tools(files: bool, source_control: bool, trunk: &str) -> Vec<StartMenuApp> {
    let mut apps = vec![tool(TERMINAL, "Terminal")];
    if files {
        apps.push(tool(FILES, "Files"));
    }
    if source_control {
        apps.push(tool(HISTORY, "Version history"));
        apps.push(StartMenuApp { comment: trunk.to_string(), ..tool(SOURCE_CONTROL, "Source control") });
    }
    apps
}

fn tool_glyph(id: &str) -> &'static str {
    match id {
        TERMINAL => "nav-terminal",
        FILES => "nav-files",
        HISTORY => "ui-history",
        _ => "nav-source-control",
    }
}

fn category_icon(id: &str) -> &'static str {
    match id {
        "favourites" => "ui-pin",
        "exponential" => "brand-exponential",
        "programming" => "nav-terminal",
        _ => "nav-apps",
    }
}

/// `/shell/startMenu`: the reducer's selectors over the IDE's tools and the
/// team's apps.
fn start_menu(state: &ShellState, apps: &[StartMenuApp], glyphs: &HashMap<String, Glyph>) -> Value {
    let favourites: Vec<String> = apps.iter().filter(|a| !a.id.starts_with(uicore_model::EXPONENTIAL_APP_PREFIX)).map(|a| a.id.clone()).collect();
    let glyph = |id: &str| match id.strip_prefix(uicore_model::EXPONENTIAL_APP_PREFIX) {
        Some(app) => {
            let name = format!("ide:app:{app}");
            if glyphs.contains_key(&name) { name } else { "nav-apps".to_string() }
        }
        None => tool_glyph(id).to_string(),
    };
    let entries: Vec<Value> = uicore_model::start_menu_apps(state, apps, &favourites).into_iter().map(|e| json!({"id": e.id, "name": e.name, "comment": e.comment, "glyph": glyph(&e.id)})).collect();
    let categories: Vec<Value> = uicore_model::start_menu_categories(apps, &favourites).into_iter().map(|c| json!({"id": c.id, "label": c.label, "icon": category_icon(&c.id), "selected": c.id == state.start_menu.category})).collect();
    let favourites: Vec<Value> = apps.iter().filter(|a| favourites.contains(&a.id)).map(|a| json!({"id": a.id, "name": a.name, "glyph": glyph(&a.id)})).collect();
    json!({"query": state.start_menu.query, "categories": categories, "apps": entries, "favourites": favourites})
}

/// A row glyph the IDE paints.
#[derive(Clone)]
enum Glyph {
    Board(domain::rows::Board),
    Agent(domain::session_tree::AgentRow),
    App(crate::apps_view::AppEntry),
    Run(Option<coding::CodingAgent>, SessionState),
    Device { device: String, row: String },
}

/// The screen as the fixture's `<kind>` or `<kind>:<id>`.
fn screen_key(screen: Option<&Screen>, active_board: Option<&str>, cx: &App) -> String {
    match screen {
        Some(Screen::Home) => "home".into(),
        Some(Screen::NeedsYou) => "needs-you".into(),
        Some(Screen::Threads) => "threads".into(),
        Some(Screen::Devices) => "devices".into(),
        Some(Screen::Drafts) => "drafts".into(),
        Some(Screen::IssueDetail { issue_id }) => format!("issue:{issue_id}"),
        Some(Screen::Thread { session_id }) => format!("thread:{session_id}"),
        Some(Screen::Session { session_id }) => format!("session:{session_id}"),
        Some(Screen::Agent { agent_id }) => format!("agent:{}", crate::agent_screen::resolve_agent_id(agent_id, cx)),
        Some(Screen::AppRun { app } | Screen::App { app }) => format!("app:{app}"),
        Some(Screen::BoardIssues { board_id }) if board_id.is_empty() => format!("board:{}", active_board.unwrap_or_default()),
        Some(Screen::BoardIssues { board_id }) => format!("board:{board_id}"),
        _ => String::new(),
    }
}

fn threads_dot(team_id: &str, cx: &mut App) -> Option<IdeDot> {
    let (needs_you, running) = crate::session_data::rail_threads_badge(team_id, cx);
    uicore_model::ide::agent_dot(needs_you, running).or_else(|| crate::session_data::rail_threads_unread(team_id, cx).then_some(IdeDot::Primary))
}

fn dot_color(dot: IdeDot, cx: &App) -> Hsla {
    match dot {
        IdeDot::Amber => cx.theme().warning,
        IdeDot::Green => cx.theme().success,
        IdeDot::Primary => theme::tokens::PRIMARY.to_hsla(),
    }
}

fn dot_icon(dot: &Value) -> Value {
    dot.as_str().map(|tone| json!(format!("ide:dot:{tone}"))).unwrap_or(Value::Null)
}

/// One team's rows as the fixture's input, with the glyphs they name.
fn gather(nav: &Entity<Navigation>, cx: &mut App) -> (IdeSidebarInput, HashMap<String, Glyph>) {
    let mut glyphs = HashMap::new();
    let screen = resolved_screen(nav, cx);
    let open_draft = match &screen {
        Some(Screen::IssueDraft { draft_id, .. }) => Some(draft_id.clone()),
        _ => None,
    };
    let board = active_board_id(nav, cx);
    let mut input = IdeSidebarInput { screen: screen_key(screen.as_ref(), board.as_deref(), cx), ..Default::default() };
    let Some(team_id) = active_team_id(nav, cx) else {
        return (input, glyphs);
    };
    input.owner = crate::settings::is_owner(cx, &team_id);
    input.drafts = crate::drafts::drafts_in_team(&team_id, cx).iter().filter(|d| Some(&d.id) != open_draft.as_ref()).count();
    input.needs_you = crate::needs_you::rows_in_team(&team_id, cx).rows.len();
    input.threads_dot = threads_dot(&team_id, cx);
    let collections = Store::global(cx).collections().clone();
    for pin in crate::pins::pins_in_team(&team_id, cx) {
        let Some((domain::contract::PIN_KIND_ISSUE, target)) = crate::pins::pin_target(&pin) else { continue };
        let Some(issue) = collections.issues.read(cx).get(target).cloned() else { continue };
        input.pins.push(IdePinInput { id: issue.id, identifier: issue.identifier, title: issue.title });
    }
    for agent in crate::session_data::agents_in_team(&team_id, cx) {
        let icon = format!("ide:agent:{}", agent.agent_id);
        input.agents.push(IdeAgentInput { id: agent.agent_id.clone(), name: agent.name.clone(), icon: icon.clone(), needs_you: agent.needs_you, busy: agent.busy });
        glyphs.insert(icon, Glyph::Agent(agent));
    }
    for app in crate::apps_view::app_entries(&team_id, cx) {
        let icon = format!("ide:app:{}", app.id);
        input.apps.push(IdeAppInput { id: app.id.clone(), slug: app.slug.clone(), name: app.name.clone(), icon: icon.clone() });
        glyphs.insert(icon, Glyph::App(app));
    }
    for board in collections.boards_in_team(&team_id, cx) {
        let icon = format!("ide:board:{}", board.id);
        input.boards.push(IdeBoardInput { id: board.id.clone(), name: board.name.clone(), icon: icon.clone() });
        glyphs.insert(icon, Glyph::Board(board));
    }
    for row in crate::session_data::live_roots_for_rail(&team_id, cx) {
        let agent = row
            .session_ids
            .iter()
            .rev()
            .filter_map(|id| crate::session_data::session(id, cx))
            .find(crate::session_data::session_is_working)
            .or_else(|| crate::session_data::session(&row.root_id, cx))
            .and_then(|session| crate::run_rows::run_agent(session.cli.as_deref()));
        let icon = format!("ide:run:{}", row.root_id);
        glyphs.insert(icon.clone(), Glyph::Run(agent, crate::sidebar::running_row_state(row.busy, row.needs_you)));
        if let Some(device) = &row.device_id {
            glyphs.insert(format!("ide:device:{}", row.root_id), Glyph::Device { device: device.clone(), row: row.root_id.clone() });
        }
        input.active.push(IdeActiveInput { id: row.root_id, title: row.title, icon, session_ids: row.session_ids, unread: row.unread });
    }
    (input, glyphs)
}

/// The fixture's output as the document binds it: dots as glyph names, the
/// row under the pointer marked, the active rows' device glyphs.
fn shaped(input: &IdeSidebarInput, glyphs: &HashMap<String, Glyph>, hover: Option<&str>, rail: bool, start_badge: Option<&str>) -> Value {
    let mut value = serde_json::to_value(ide_sidebar(input)).expect("the sidebar serializes");
    let mark = |rows: &mut Value| {
        for row in rows.as_array_mut().into_iter().flatten() {
            row["dotIcon"] = match row["count"].as_str() {
                Some(count) => json!(format!("ide:count:{count}")),
                None => dot_icon(&row["dot"]),
            };
            row["hover"] = json!(hover == row["id"].as_str());
            // #EXP-1724: a rail tile says its count (or that there is news)
            // in its corner, and a pin needs a glyph of its own.
            row["meta"] = match (rail, row["count"].as_str(), row["dot"].is_null()) {
                (true, Some(count), _) => json!(count),
                (true, None, false) => json!("•"),
                _ => Value::Null,
            };
            if rail && row.get("identifier").is_some() {
                row["icon"] = json!("ui-pin");
                row["identifier"] = Value::Null;
            }
        }
    };
    mark(&mut value["nav"]);
    mark(&mut value["pins"]);
    mark(&mut value["agents"]["rows"]);
    mark(&mut value["apps"]["rows"]);
    mark(&mut value["boards"]["rows"]);
    value["startBadge"] = start_badge.map(|b| json!(b)).unwrap_or(Value::Null);
    value["boards"]["emptyText"] = json!(crate::empty_state_copy::BOARD_SWITCHER);
    for row in value["active"].as_array_mut().into_iter().flatten() {
        let device = format!("ide:device:{}", row["id"].as_str().unwrap_or_default());
        row["unreadIcon"] = if row["unread"] == json!(true) { json!("ide:dot:primary") } else { Value::Null };
        row["deviceIcon"] = if glyphs.contains_key(&device) { json!(device) } else { Value::Null };
    }
    value
}

/// The row a hover names: a template instance `<row>.<key>`; a control
/// inside the row is not the row.
fn row_key(component: &str) -> Option<&str> {
    component.split_once('.').filter(|(row, _)| row.ends_with("_row")).map(|(_, key)| key)
}

/// The sidebar document in the window: one per [`crate::sidebar::RailView`].
pub(crate) struct SidebarRegion {
    view: Entity<SurfaceView>,
    glyphs: Rc<RefCell<HashMap<String, Glyph>>>,
    hover: Rc<RefCell<Option<String>>>,
    input: IdeSidebarInput,
    shown: Option<Value>,
    rail: bool,
    menu_view: Entity<SurfaceView>,
    menu: Rc<RefCell<ShellState>>,
    region_handle: gpui::WeakEntity<SidebarRegion>,
    /// The tools the footer's gate lets through + the team's apps.
    apps: Vec<StartMenuApp>,
    tools: (bool, bool, String),
    /// A trunk failure's glyph on the start button.
    start_badge: Option<String>,
    /// The pictures show a sample, not the store.
    #[cfg(test)]
    frozen: bool,
}

impl SidebarRegion {
    pub(crate) fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut gpui::Context<Self>| {
            let glyphs = Rc::new(RefCell::new(HashMap::new()));
            let hover = Rc::new(RefCell::new(None));
            let menu = Rc::new(RefCell::new(ShellState::default()));
            let host = Rc::new(RegionHost { glyphs: glyphs.clone(), hover: hover.clone(), window: window.window_handle(), region: cx.weak_entity(), nav });
            let mut doc = uicore_gpui::inline::document(uicore_docs::doc("sidebar").expect("the factory sidebar"));
            uicore_gpui::inline::without(&mut doc, PRUNED);
            let theme = uicore_gpui::inline::theme(&eui_core::themes::default_theme());
            let view = uicore_gpui::inline::mount(&doc, &uicore_data::Feed::new(), theme.clone(), host.clone(), Mode::Dark, window, cx);
            let mut menu_doc = uicore_gpui::inline::popup(uicore_docs::doc("start-menu").expect("the factory start menu"));
            uicore_gpui::inline::without(&mut menu_doc, MENU_PRUNED);
            let menu_view = uicore_gpui::inline::mount(&menu_doc, &uicore_data::Feed::new(), theme, host, Mode::Dark, window, cx);
            Self {
                view,
                glyphs,
                hover,
                input: IdeSidebarInput::default(),
                shown: None,
                rail: false,
                menu_view,
                menu,
                region_handle: cx.weak_entity(),
                apps: Vec::new(),
                tools: (true, true, String::new()),
                start_badge: None,
                #[cfg(test)]
                frozen: false,
            }
        })
    }

    /// Re-reads the rows (the rail's render: it observes what they show).
    pub(crate) fn refresh(&mut self, nav: &Entity<Navigation>, cx: &mut gpui::Context<Self>) {
        #[cfg(test)]
        if self.frozen {
            return;
        }
        let (input, glyphs) = gather(nav, cx);
        *self.glyphs.borrow_mut() = glyphs;
        self.apps = tools(self.tools.0, self.tools.1, &self.tools.2);
        self.apps.extend(self.input_apps(&input));
        self.input = input;
        self.push(cx);
        self.push_menu(cx);
        // A glyph is host-painted: its state changes without the data.
        self.view.update(cx, |_, cx| cx.notify());
    }

    /// Opens the start menu, for the pictures.
    #[cfg(test)]
    pub(crate) fn open_menu(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.apps = tools(true, true, "");
        self.apps.extend(self.input_apps(&self.input.clone()));
        self.menu_event(ShellEvent::StartMenuToggle, window, cx);
    }

    /// A sample state, for the pictures.
    #[cfg(test)]
    pub(crate) fn show(&mut self, input: IdeSidebarInput, hover: Option<&str>, cx: &mut gpui::Context<Self>) {
        self.frozen = true;
        self.input = input;
        *self.hover.borrow_mut() = hover.map(str::to_string);
        self.push(cx);
    }

    fn push(&mut self, cx: &mut gpui::Context<Self>) {
        let rail = crate::ui_prefs::rail();
        let value = shaped(&self.input, &self.glyphs.borrow(), self.hover.borrow().as_deref(), rail, self.start_badge.as_deref());
        if self.shown.as_ref() == Some(&value) && self.rail == rail {
            return;
        }
        self.shown = Some(value.clone());
        self.rail = rail;
        self.view.update(cx, |view, cx| {
            let _ = view.set_data("/shell/rail", Some(json!(rail)), cx);
            let _ = view.set_data("/shell/ide", Some(value), cx);
        });
    }

    fn input_apps(&self, input: &IdeSidebarInput) -> Vec<StartMenuApp> {
        input.apps.iter().map(|a| StartMenuApp { id: team_app(&a.id), name: a.name.clone(), comment: String::new(), categories: Vec::new(), keywords: a.slug.clone() }).collect()
    }

    /// The footer's gate and trunk badge (the rail's render reads them).
    pub(crate) fn set_tools(&mut self, files: bool, source_control: bool, trunk: String, badge: Option<String>) {
        self.tools = (files, source_control, trunk);
        self.start_badge = badge;
    }

    fn push_menu(&mut self, cx: &mut gpui::Context<Self>) {
        let value = start_menu(&self.menu.borrow(), &self.apps, &self.glyphs.borrow());
        let user = crate::queries::active_account(cx).map(|a| json!({"name": a.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| a.email.clone()), "host": a.email})).unwrap_or_else(|| json!({"name": "", "host": ""}));
        self.menu_view.update(cx, |view, cx| {
            let _ = view.set_data("/user", Some(user), cx);
            let _ = view.set_data("/shell/startMenu", Some(value), cx);
        });
    }

    /// Runs a start-menu event through the reducer.
    fn menu_event(&mut self, event: ShellEvent, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let (was, next) = {
            let state = self.menu.borrow();
            (state.start_menu.open, reduce(&state, &event))
        };
        let open = next.start_menu.open;
        *self.menu.borrow_mut() = next;
        self.push_menu(cx);
        if open && !was {
            let focus = self.menu_view.clone();
            window.defer(cx, move |_, cx| focus.update(cx, |view, cx| view.command(&SurfaceCommand::Focus { id: "search".into() }, cx)));
        }
        cx.notify();
    }

    /// The open start menu over the window, above the start button.
    fn overlay(&self, cx: &App) -> Option<AnyElement> {
        if !self.menu.borrow().start_menu.open {
            return None;
        }
        let layout = uicore_model::layout::shell_layout();
        let anchor = self.view.read(cx).painted_bounds("start").or_else(|| self.view.read(cx).painted_bounds("rail_start"))?;
        let region = self.region_handle.clone();
        Some(
            deferred(
                anchored().anchor(gpui::Anchor::BottomLeft).position(anchor.origin - point(px(0.), px(4.))).snap_to_window().child(
                    div()
                        .id("start-menu")
                        .occlude()
                        .w(px(layout.start_menu.width as f32))
                        .h(px(layout.start_menu.height as f32))
                        .on_mouse_down_out(move |_, window, cx| {
                            let _ = region.update(cx, |region, cx| region.menu_event(ShellEvent::StartMenuClose, window, cx));
                        })
                        .child(uicore_gpui::inline::region(&self.menu_view)),
                ),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }

    /// The row under the pointer, for the context menu.
    pub(crate) fn hovered(&self) -> Option<String> {
        self.hover.borrow().clone()
    }

    pub(crate) fn is_agent(&self, id: &str) -> bool {
        self.input.agents.iter().any(|a| a.id == id)
    }

    pub(crate) fn is_active(&self, id: &str) -> bool {
        self.input.active.iter().any(|a| a.id == id)
    }
}

impl Render for SidebarRegion {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().child(uicore_gpui::inline::region(&self.view)).children(self.overlay(cx))
    }
}

struct RegionHost {
    glyphs: Rc<RefCell<HashMap<String, Glyph>>>,
    hover: Rc<RefCell<Option<String>>>,
    window: AnyWindowHandle,
    region: gpui::WeakEntity<SidebarRegion>,
    nav: Entity<Navigation>,
}

impl RegionHost {
    fn in_window(&self, cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
        let handle = self.window;
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| f(window, cx));
        });
    }
}

fn open_row(kind: &str, id: String, window: &mut Window, cx: &mut App) {
    match kind {
        "nav" => {
            let screen = match id.as_str() {
                "needs-you" => Screen::NeedsYou,
                "threads" => Screen::Threads,
                _ => Screen::Drafts,
            };
            navigate(window, cx, screen);
        }
        "pin" => {
            let board = Store::global(cx).collections().issues.read(cx).get(&id).map(|issue| issue.board_id.clone());
            if let Some(board) = board {
                navigation::set_active_board(window, cx, board);
            }
            navigation::navigate_from_rail(window, cx, Screen::IssueDetail { issue_id: id });
        }
        "agent" => navigate(window, cx, Screen::Agent { agent_id: id }),
        "app" => navigate(window, cx, Screen::AppRun { app: id }),
        "board" => {
            navigation::set_active_board(window, cx, id.clone());
            navigate(window, cx, Screen::BoardIssues { board_id: id });
        }
        "active" => navigation::navigate_from_live_rail(window, cx, Screen::Thread { session_id: id }),
        _ => {}
    }
}

/// A start-menu entry: the tools the footer's Computer menu held, a team app.
fn launch(app: &str, window: &mut Window, cx: &mut App) {
    if let Some(id) = app.strip_prefix(uicore_model::EXPONENTIAL_APP_PREFIX) {
        return navigate(window, cx, Screen::AppRun { app: id.to_string() });
    }
    match app {
        TERMINAL => crate::session_bar::open_new_shell(window, cx),
        FILES => crate::sidebar::activate_tool(window, cx, crate::sidebar::ToolWindow::Files),
        HISTORY | SOURCE_CONTROL => crate::sidebar::activate_tool(window, cx, crate::sidebar::ToolWindow::SourceControl),
        _ => {}
    }
}

fn open_gear(kind: &str, id: String, window: &mut Window, cx: &mut App) {
    match kind {
        "agent" => {
            crate::sidebar::select_settings_section(window, cx, SettingsSection::Agent(id));
            navigate(window, cx, Screen::Settings);
        }
        "board" => {
            crate::sidebar::select_settings_section(window, cx, SettingsSection::Board(id));
            navigate(window, cx, Screen::Settings);
        }
        "app" => crate::app_screen::open_app_tab(window, cx, id, domain::apps::EditorTab::Settings, false),
        _ => {}
    }
}

fn add(kind: &str, nav: &Entity<Navigation>, window: &mut Window, cx: &mut App) {
    let Some(team_id) = active_team_id(nav, cx) else { return };
    match kind {
        "agent" => {
            crate::sidebar::select_settings_section(window, cx, SettingsSection::NewAgent);
            navigate(window, cx, Screen::Settings);
        }
        "board" => crate::create_board_dialog::open(window, cx, team_id),
        "newApp" => crate::apps_view::new_app(team_id, window, cx),
        "import" => crate::app_import_dialog::open(window, cx, team_id),
        _ => {}
    }
}

impl HostPlugin for RegionHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        crate::exponential_ui_host::icon_path(name)
    }

    fn icon_element(&self, name: &str, size: f32, color: Hsla, cx: &App) -> Option<AnyElement> {
        if let Some(tone) = name.strip_prefix("ide:dot:") {
            let dot = serde_json::from_value::<IdeDot>(json!(tone)).ok()?;
            return Some(crate::sidebar::rail_dot(dot_color(dot, cx)).into_any_element());
        }
        if let Some(state) = name.strip_prefix("ide:trunk:") {
            let badge = if state == "error" { crate::sidebar::RailBadge::Icon(crate::icons::registry::UI_ERROR, cx.theme().danger) } else { crate::sidebar::RailBadge::Icon(crate::icons::registry::UI_WARNING, cx.theme().warning) };
            return Some(crate::sidebar::rail_badge_element(badge, 12., cx));
        }
        if let Some(count) = name.strip_prefix("ide:count:") {
            let badge = crate::sidebar::RailBadge::Count(count.parse().ok()?, crate::surface::BadgeTone::Muted);
            return Some(crate::sidebar::rail_badge_element(badge, 12., cx));
        }
        let glyphs = self.glyphs.borrow();
        let element = match glyphs.get(name)? {
            Glyph::Board(board) => {
                let tint = board.color.as_deref().and_then(crate::issue_header::parse_hex_color).unwrap_or(cx.theme().muted_foreground);
                crate::icons::board_icon(board).text_color(tint).with_size(gpui::px(size)).into_any_element()
            }
            Glyph::Agent(agent) => crate::icons::agent_row_icon(agent).with_size(gpui::px(size)).into_any_element(),
            Glyph::App(app) => app.rail_icon().with_size(gpui::px(size)).into_any_element(),
            Glyph::Run(agent, state) => crate::run_rows::run_mark(*agent, *state, cx).into_any_element(),
            Glyph::Device { device, row } => crate::threads_view::thread_device_icon(Some(device), &format!("rail-{row}"), cx).unwrap_or_else(|| Icon::empty().text_color(color).into_any_element()),
        };
        Some(element)
    }

    fn on_hover(&self, component_id: &str, hovered: bool, cx: &mut App) {
        let Some(key) = row_key(component_id) else { return };
        {
            let mut hover = self.hover.borrow_mut();
            if hovered {
                *hover = Some(key.to_string());
            } else if hover.as_deref() == Some(key) {
                *hover = None;
            } else {
                return;
            }
        }
        let region = self.region.clone();
        cx.defer(move |cx| {
            let _ = region.update(cx, |region, cx| region.push(cx));
        });
    }

    fn on_function_call(&self, event: &FunctionCallEvent, cx: &mut App) {
        let region = self.region.clone();
        let closing = region.clone();
        let close = move |window: &mut Window, cx: &mut App| {
            let _ = closing.update(cx, |region, cx| region.menu_event(ShellEvent::StartMenuClose, window, cx));
        };
        let args = event.args.clone();
        match (event.name.as_str(), event.component_id.as_str()) {
            ("ede_start_menu", _) => {
                let region = region.clone();
                let ev = if args["toggle"] == json!(true) {
                    ShellEvent::StartMenuToggle
                } else if let Some(category) = args["category"].as_str() {
                    ShellEvent::StartMenuCategory { category: category.to_string() }
                } else {
                    ShellEvent::StartMenuClose
                };
                self.in_window(cx, move |window, cx| {
                    let _ = region.update(cx, |region, cx| region.menu_event(ev, window, cx));
                });
            }
            ("ede_chat", _) => self.in_window(cx, |window, cx| navigate(window, cx, Screen::Home)),
            ("ede_launch", "whats_new") => self.in_window(cx, move |window, cx| {
                close(window, cx);
                crate::changelog::open_whats_new(window, cx);
            }),
            ("ede_launch", "account") => self.in_window(cx, move |window, cx| {
                close(window, cx);
                crate::sidebar::select_settings_section(window, cx, SettingsSection::Account);
                navigate(window, cx, Screen::Settings);
            }),
            ("ede_launch", "devices") => self.in_window(cx, move |window, cx| {
                close(window, cx);
                navigate(window, cx, Screen::Devices);
            }),
            ("ede_open_document", "settings") => self.in_window(cx, move |window, cx| {
                close(window, cx);
                navigate(window, cx, Screen::Settings);
            }),
            ("ede_logout", _) => self.in_window(cx, move |window, cx| {
                close(window, cx);
                window.dispatch_action(Box::new(crate::actions::SignOut), cx);
            }),
            ("ede_launch", _) => {
                let app = args["app"].as_str().unwrap_or_default().to_string();
                self.in_window(cx, move |window, cx| {
                    close(window, cx);
                    launch(&app, window, cx);
                });
            }
            _ => {}
        }
    }

    fn on_input(&self, event: &InputEvent, cx: &mut App) {
        if event.path.as_deref() != Some("/shell/startMenu/query") {
            return;
        }
        let query = event.value.as_str().unwrap_or_default().to_string();
        let region = self.region.clone();
        self.in_window(cx, move |window, cx| {
            let _ = region.update(cx, |region, cx| region.menu_event(ShellEvent::StartMenuQuery { query }, window, cx));
        });
    }

    fn on_action(&self, event: &ActionEvent, cx: &mut App) {
        let text = |key: &str| event.context[key].as_str().unwrap_or_default().to_string();
        let (kind, id) = (text("kind"), text("id"));
        let nav = self.nav.clone();
        match event.name.as_str() {
            "ide_open" => self.in_window(cx, move |window, cx| open_row(&kind, id, window, cx)),
            "ide_gear" => self.in_window(cx, move |window, cx| open_gear(&kind, id, window, cx)),
            "ide_add" => {
                let kind = event.payload.as_ref().and_then(|p| p["value"].as_str()).map(str::to_string).unwrap_or(kind);
                self.in_window(cx, move |window, cx| add(&kind, &nav, window, cx))
            }
            "ide_unpin" => {
                if let Some(team_id) = active_team_id(&nav, cx) {
                    crate::pins::toggle_pin(team_id, domain::contract::PIN_KIND_ISSUE, id, cx);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rows_hover_and_dot_reach_the_document() {
        let input = IdeSidebarInput {
            screen: "home".into(),
            threads_dot: Some(IdeDot::Amber),
            boards: vec![IdeBoardInput { id: "b1".into(), name: "Web".into(), icon: "ide:board:b1".into() }],
            ..Default::default()
        };
        let value = shaped(&input, &HashMap::new(), Some("b1"), false, None);
        assert_eq!(value["nav"][0]["dotIcon"], "ide:dot:amber");
        let counted = IdeSidebarInput { needs_you: 5, ..input.clone() };
        assert_eq!(shaped(&counted, &HashMap::new(), None, false, None)["nav"][0]["dotIcon"], "ide:count:5");
        assert_eq!(value["boards"]["rows"][0]["hover"], true);

    }

    #[test]
    fn a_rail_tile_carries_its_count_and_a_pin_a_glyph() {
        let input = IdeSidebarInput {
            screen: "home".into(),
            needs_you: 5,
            threads_dot: Some(IdeDot::Primary),
            pins: vec![IdePinInput { id: "i1".into(), identifier: "APP-1".into(), title: "Crash".into() }],
            ..Default::default()
        };
        let value = shaped(&input, &HashMap::new(), None, true, None);
        assert_eq!(value["nav"][0]["meta"], "5");
        assert_eq!(value["nav"][1]["meta"], "•");
        assert_eq!(value["pins"][0]["icon"], "ui-pin");
        assert_eq!(shaped(&input, &HashMap::new(), None, false, None)["nav"][0]["meta"], Value::Null);
    }

    #[test]
    fn the_menu_holds_the_gated_tools_and_the_teams_apps() {
        let mut apps = tools(false, false, "");
        apps.push(StartMenuApp { id: team_app("x1"), name: "CRM".into(), comment: String::new(), categories: Vec::new(), keywords: String::new() });
        let value = start_menu(&ShellState::default(), &apps, &HashMap::new());
        let names: Vec<&str> = value["apps"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["CRM", "Terminal"]);
        assert_eq!(value["favourites"][0]["glyph"], "nav-terminal");
        assert_eq!(value["categories"].as_array().unwrap().iter().filter(|c| c["id"] == "exponential").count(), 1);
    }

    #[test]
    fn only_a_row_instance_is_a_hover_target() {
        assert_eq!(row_key("ide_board_row.b1"), Some("b1"));
        assert_eq!(row_key("ide_board_gear.b1"), None);
        assert_eq!(row_key("ide_app_add"), None);
    }

    #[test]
    fn the_document_prunes_only_nodes_it_has() {
        let doc = uicore_docs::doc("sidebar").expect("the sidebar");
        for id in PRUNED {
            assert!(doc.source.contains(&format!("\"id\": \"{id}\"")), "{id}");
        }
    }
}
