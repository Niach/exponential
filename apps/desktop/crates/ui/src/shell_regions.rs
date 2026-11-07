//! #EXP-1725: the IDE's regions on uicore. The sidebar's rows are the shared
//! document (`uicore_docs`, `apps/ede/ui/sidebar.json`) mounted in place: this
//! file gathers what the rows show (`uicore_model::ide`, fixture-locked),
//! feeds it to `/shell/ide` and answers the document's events on the IDE's
//! navigation. A row's glyph stays the IDE's to paint: the data names it
//! `ide:<kind>:<id>` and [`RegionHost::icon_element`] draws the tinted board,
//! the agent's animated mark, a status dot or a count.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use domain::session_tree::SessionState;
use eui_core::theme::Mode;
use eui_gpui::{
    host::{ActionEvent, HostPlugin},
    view::SurfaceView,
};
use gpui::{prelude::*, AnyElement, AnyWindowHandle, App, Entity, Hsla, SharedString, Window};
use gpui_component::{ActiveTheme as _, Icon, Sizable as _};
use serde_json::{json, Value};
use sync::Store;
use uicore_model::ide::{ide_sidebar, IdeActiveInput, IdeAgentInput, IdeAppInput, IdeBoardInput, IdeDot, IdePinInput, IdeSidebarInput};

use crate::navigation::{self, active_board_id, active_team_id, navigate, resolved_screen, Navigation, Screen};
use crate::settings::SettingsSection;

/// What the sidebar document draws itself; the rest is the IDE's own until
/// its region moves (the header, the footer).
const PRUNED: &[&str] = &["header", "switcher_menu", "header_rail", "nav", "pinned", "workspaces", "sessions", "corner", "corner_rail"];

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
fn shaped(input: &IdeSidebarInput, glyphs: &HashMap<String, Glyph>, hover: Option<&str>) -> Value {
    let mut value = serde_json::to_value(ide_sidebar(input)).expect("the sidebar serializes");
    let mark = |rows: &mut Value| {
        for row in rows.as_array_mut().into_iter().flatten() {
            row["dotIcon"] = match row["count"].as_str() {
                Some(count) => json!(format!("ide:count:{count}")),
                None => dot_icon(&row["dot"]),
            };
            row["hover"] = json!(hover == row["id"].as_str());
        }
    };
    mark(&mut value["nav"]);
    mark(&mut value["pins"]);
    mark(&mut value["agents"]["rows"]);
    mark(&mut value["apps"]["rows"]);
    mark(&mut value["boards"]["rows"]);
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
    /// The pictures show a sample, not the store.
    #[cfg(test)]
    frozen: bool,
}

impl SidebarRegion {
    pub(crate) fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut gpui::Context<Self>| {
            let glyphs = Rc::new(RefCell::new(HashMap::new()));
            let hover = Rc::new(RefCell::new(None));
            let host = Rc::new(RegionHost { glyphs: glyphs.clone(), hover: hover.clone(), window: window.window_handle(), region: cx.weak_entity(), nav });
            let mut doc = uicore_gpui::inline::document(uicore_docs::doc("sidebar").expect("the factory sidebar"));
            uicore_gpui::inline::without(&mut doc, PRUNED);
            let theme = uicore_gpui::inline::theme(&eui_core::themes::default_theme());
            let view = uicore_gpui::inline::mount(&doc, &uicore_data::Feed::new(), theme, host, Mode::Dark, window, cx);
            Self {
                view,
                glyphs,
                hover,
                input: IdeSidebarInput::default(),
                shown: None,
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
        self.input = input;
        self.push(cx);
        // A glyph is host-painted: its state changes without the data.
        self.view.update(cx, |_, cx| cx.notify());
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
        let value = shaped(&self.input, &self.glyphs.borrow(), self.hover.borrow().as_deref());
        if self.shown.as_ref() == Some(&value) {
            return;
        }
        self.shown = Some(value.clone());
        self.view.update(cx, |view, cx| {
            let _ = view.set_data("/shell/ide", Some(value), cx);
        });
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
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        uicore_gpui::inline::region(&self.view)
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
                "home" => Screen::Home,
                "needs-you" => Screen::NeedsYou,
                "threads" => Screen::Threads,
                "devices" => Screen::Devices,
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
        let value = shaped(&input, &HashMap::new(), Some("b1"));
        assert_eq!(value["nav"][1]["dotIcon"], "ide:dot:amber");
        let counted = IdeSidebarInput { needs_you: 5, ..input.clone() };
        assert_eq!(shaped(&counted, &HashMap::new(), None)["nav"][1]["dotIcon"], "ide:count:5");
        assert_eq!(value["boards"]["rows"][0]["hover"], true);
        assert_eq!(value["nav"][0]["hover"], false);
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
