//! #EXP-1725: the IDE's regions on uicore. The sidebar's rows are the shared
//! document (`uicore_docs`, `apps/ede/ui/sidebar.json`) mounted in place: this
//! file gathers what the rows show (`uicore_model::ide`, fixture-locked),
//! feeds it to `/shell/ide` and answers the document's events on the IDE's
//! navigation. A row's glyph stays the IDE's to paint: the data names it
//! `ide:<kind>:<id>` and [`RegionHost::icon_element`] draws the tinted board,
//! the agent's animated mark or a count (#FEED-232: THE count badge,
//! `ide:count:<tone>:<label>`; no dots in the nav).

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use domain::session_tree::SessionState;
use eui_core::theme::Mode;
use eui_gpui::{
    host::{ActionEvent, FunctionCallEvent, HostPlugin},
    view::SurfaceView,
};
use crate::controls::PointerContextMenuExt as _;
use gpui::{div, prelude::*, AnyElement, AnyWindowHandle, App, Entity, Hsla, SharedString, Window};
use gpui_component::{ActiveTheme as _, Icon, Sizable as _};
use serde_json::{json, Value};
use sync::Store;
use uicore_model::ide::{IdeTabInput, ide_sidebar, IdeActiveInput, IdeAgentInput, IdeAppInput, IdeBoardInput, IdeCount, IdePinInput, IdePrInput, IdeRepoInput, IdeSidebarInput, IdeTone};

use crate::navigation::{self, active_board_id, active_team_id, navigate, resolved_screen, Navigation, Screen};
use crate::settings::SettingsSection;

/// What the sidebar document draws itself; the rest is the IDE's own until
/// its region moves (the header, the footer). `chats` is eDE's rail list
/// (#EDE-65); the IDE lists its chats under Chats.
const PRUNED: &[&str] = &["header", "switcher_menu", "chats"];

/// A row glyph the IDE paints.
#[derive(Clone, PartialEq)]
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
        Some(Screen::Chats) => "chats".into(),
        Some(Screen::Devices) => "devices".into(),
        Some(Screen::Drafts) => "drafts".into(),
        Some(Screen::IssueDetail { issue_id }) => format!("issue:{issue_id}"),
        Some(Screen::Chat { session_id }) => format!("chat:{session_id}"),
        Some(Screen::Session { session_id }) => format!("session:{session_id}"),
        Some(Screen::Agent { agent_id }) => format!("agent:{}", crate::agent_screen::resolve_agent_id(agent_id, cx)),
        Some(Screen::AppRun { app } | Screen::App { app }) => format!("app:{app}"),
        Some(Screen::BoardIssues { board_id }) if board_id.is_empty() => format!("board:{}", active_board.unwrap_or_default()),
        Some(Screen::BoardIssues { board_id }) => format!("board:{board_id}"),
        _ => String::new(),
    }
}

/// The Chats row's count (`nav_badges::chats_badge`).
fn chats_count(team_id: &str, cx: &mut App) -> IdeCount {
    let badge = crate::session_data::rail_chats_count(team_id, cx);
    let tone = match badge.tone {
        domain::nav_badges::CountBadgeTone::Attention => IdeTone::Attention,
        domain::nav_badges::CountBadgeTone::Muted => IdeTone::Muted,
    };
    IdeCount { count: badge.count, tone }
}

/// A count as the glyph name the hosts paint: `ide:count:<tone>:<label>`.
fn count_icon(label: Option<&str>, tone: &str) -> Value {
    label.map(|label| json!(format!("ide:count:{tone}:{label}"))).unwrap_or(Value::Null)
}

/// `<tone>:<label>` back to THE count badge (`99+` reads as past the cap).
fn parse_count(rest: &str) -> Option<crate::sidebar::RailBadge> {
    let (tone, label) = rest.split_once(':')?;
    let tone = match tone {
        "attention" => crate::surface::BadgeTone::Attention,
        "muted" => crate::surface::BadgeTone::Muted,
        _ => return None,
    };
    let count = match label.strip_suffix('+') {
        Some(cap) => cap.parse::<usize>().ok()? + 1,
        None => label.parse().ok()?,
    };
    Some(crate::sidebar::RailBadge::Count(count, tone))
}

/// #EXP-1616: an Exponential (`ecode`) PR's repository name, read off its
/// url (`<app>/t/<team>/repos/<name>/pulls/<n>`, `ecode-host.ts`): the
/// repositories list is not synced, the PRs are.
fn ecode_repo_name(url: &str) -> Option<&str> {
    let (_, rest) = url.split_once("/t/")?;
    let (_, rest) = rest.split_once("/repos/")?;
    let (name, number) = rest.split_once("/pulls/")?;
    (!name.is_empty() && !name.contains('/') && number.parse::<u64>().is_ok()).then_some(name)
}

/// #EXP-1616: the team's Exponential repositories by name, each with its
/// live PR tree (`repo_pr_tree`, the repo page's).
fn ecode_repos<'a>(team_id: &str, pulls: impl IntoIterator<Item = &'a domain::rows::PullRequestRow>) -> Vec<IdeRepoInput> {
    use domain::repo_page::{repo_pr_tree, RepoPr};
    let mut repos: std::collections::BTreeMap<(String, String), Vec<RepoPr>> = std::collections::BTreeMap::new();
    for pull in pulls {
        if pull.team_id.as_deref().is_some_and(|team| team != team_id) {
            continue;
        }
        let (Some(repo), Some(name)) = (pull.repository_id.as_deref(), pull.url.as_deref().and_then(ecode_repo_name)) else { continue };
        repos.entry((name.to_string(), repo.to_string())).or_default().push(RepoPr::from_row(pull));
    }
    repos
        .into_iter()
        .map(|((name, id), prs)| IdeRepoInput {
            id,
            name,
            prs: repo_pr_tree(&prs).into_iter().map(|row| IdePrInput { id: row.id, identifier: row.identifier, title: row.title, subtitle: row.subtitle, depth: row.depth }).collect(),
        })
        .collect()
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
    input.needs_you = {
        let _span = tracing::trace_span!("rail.needs_you").entered();
        crate::needs_you::rows_in_team(&team_id, cx).rows.len()
    };
    input.chats = {
        let _span = tracing::trace_span!("rail.chats").entered();
        chats_count(&team_id, cx)
    };
    let agents_span = tracing::trace_span!("rail.agents").entered();
    let collections = Store::global(cx).collections().clone();
    for pin in crate::pins::pins_in_team(&team_id, cx) {
        let Some((domain::contract::PIN_KIND_ISSUE, target)) = crate::pins::pin_target(&pin) else { continue };
        let Some(issue) = collections.issues.read(cx).get(target).cloned() else { continue };
        input.pins.push(IdePinInput { id: issue.id, identifier: issue.identifier, title: issue.title });
    }
    // #FEED-232: an agent row counts its conversation's unread replies.
    // #PERF-10: the counts alone (the whole Chats model derived per write).
    let unread: HashMap<String, usize> = crate::session_data::agent_unread_counts(&team_id, cx);
    for agent in crate::session_data::agents_in_team(&team_id, cx) {
        let icon = format!("ide:agent:{}", agent.agent_id);
        let unread = unread.get(&agent.agent_id).copied().unwrap_or(0);
        input.agents.push(IdeAgentInput { id: agent.agent_id.clone(), name: agent.name.clone(), icon: icon.clone(), needs_you: agent.needs_you, unread });
        glyphs.insert(icon, Glyph::Agent(agent));
    }
    drop(agents_span);
    let apps_span = tracing::trace_span!("rail.apps_boards").entered();
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
    drop(apps_span);
    {
        let _span = tracing::trace_span!("rail.prs").entered();
        input.repos = ecode_repos(&team_id, collections.pull_requests.read(cx).iter());
    }
    let _span = tracing::trace_span!("rail.live_roots").entered();
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

/// The fixture's output as the document binds it: counts as glyph names, the
/// row under the pointer marked, the active rows' device glyphs.
fn shaped(input: &IdeSidebarInput, glyphs: &HashMap<String, Glyph>, hover: Option<&str>, assistant: Option<(&str, &str)>) -> Value {
    let mut value = serde_json::to_value(ide_sidebar(input)).expect("the sidebar serializes");
    let mark = |rows: &mut Value| {
        for row in rows.as_array_mut().into_iter().flatten() {
            row["dotIcon"] = count_icon(row["count"].as_str(), row["tone"].as_str().unwrap_or("muted"));
            row["hover"] = json!(hover == row["id"].as_str());
        }
    };
    mark(&mut value["nav"]);
    mark(&mut value["pins"]);
    mark(&mut value["agents"]["rows"]);
    mark(&mut value["apps"]["rows"]);
    mark(&mut value["boards"]["rows"]);
    // #EXP-1741: the corner is the person's Assistant (its chat on a press).
    value["assistant"] = assistant.map(|(id, name)| json!({"id": id, "name": name})).unwrap_or(Value::Null);
    value["computer"] = json!("ide:computer");
    value["boards"]["emptyText"] = json!(crate::empty_state_copy::BOARD_SWITCHER);
    for row in value["active"].as_array_mut().into_iter().flatten() {
        let device = format!("ide:device:{}", row["id"].as_str().unwrap_or_default());
        row["deviceIcon"] = if glyphs.contains_key(&device) { json!(device) } else { Value::Null };
    }
    value
}

/// #EXP-1616: the Pull requests tree's rows as nodes. A Row's `depth`
/// never binds (the tree guides are computed against its siblings), so the
/// host writes them into the document's `Section{tree}` itself.
fn pr_tree_rows(prs: &Value) -> Vec<Value> {
    prs.as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(ix, row)| {
            let mut props = json!({"title": row["title"], "depth": row["depth"], "density": "compact", "pressable": true, "selected": row["selected"]});
            for key in ["identifier", "subtitle", "icon"] {
                if !row[key].is_null() {
                    props[key] = row[key].clone();
                }
            }
            json!({"id": format!("ide_pr_{ix}"), "component": "Row", "props": props, "on": {"press": {"event": {"name": "ide_open", "context": {"kind": "repo", "id": row["repo"]}}}}})
        })
        .collect()
}

/// The document with `prs` as the `ide_prs_tree` Section's rows.
fn with_pr_tree(doc: &uicore_gpui::inline::Document, prs: &Value) -> Option<eui_core::types::NestedNode> {
    fn fill(node: &mut Value, rows: &[Value]) -> bool {
        if node["id"] == "ide_prs_tree" {
            node["children"] = Value::Array(rows.to_vec());
            return true;
        }
        node["children"].as_array_mut().into_iter().flatten().any(|child| fill(child, rows))
            || node["slots"].as_object_mut().into_iter().flat_map(|slots| slots.values_mut()).any(|slot| fill(slot, rows))
    }
    let mut root = uicore_gpui::inline::root_json(doc)?;
    fill(&mut root, &pr_tree_rows(prs)).then(|| serde_json::from_value(root).ok()).flatten()
}

/// The row a hover names: a template instance `<row>.<key>`; a control
/// inside the row is not the row.
fn row_key(component: &str) -> Option<&str> {
    component.split_once('.').filter(|(row, _)| row.ends_with("_row")).map(|(_, key)| key)
}

/// The sidebar document in the window: one per [`crate::sidebar::RailView`].
pub(crate) struct SidebarRegion {
    view: Entity<SurfaceView>,
    /// The mounted document, for the Pull requests tree's rows.
    doc: uicore_gpui::inline::Document,
    /// The `prs` the document's tree holds.
    prs_built: Value,
    glyphs: Rc<RefCell<HashMap<String, Glyph>>>,
    hover: Rc<RefCell<Option<String>>>,
    pub(crate) input: IdeSidebarInput,
    shown: Option<Value>,
    /// The person's Assistant as the active team lists it: `(id, name)`.
    assistant: Option<(String, String)>,
    /// #EXP-1741: the Computer button's state (the corner paints it).
    computer: Rc<RefCell<Option<crate::sidebar::ComputerState>>>,
    /// The Computer button changed since the document last repainted.
    computer_changed: bool,
    /// The pictures show a sample, not the store.
    #[cfg(test)]
    frozen: bool,
}

impl SidebarRegion {
    pub(crate) fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut gpui::Context<Self>| {
            let glyphs = Rc::new(RefCell::new(HashMap::new()));
            let hover = Rc::new(RefCell::new(None));
            let computer = Rc::new(RefCell::new(None));
            let host = Rc::new(RegionHost { computer: computer.clone(), glyphs: glyphs.clone(), hover: hover.clone(), window: window.window_handle(), region: cx.weak_entity(), nav });
            let mut doc = uicore_gpui::inline::document(uicore_docs::doc("sidebar").expect("the factory sidebar"));
            uicore_gpui::inline::without(&mut doc, PRUNED);
            // #EXP-1741: the IDE's sidebar never folds (eDE's is the rail).
            uicore_gpui::inline::set_prop(&mut doc, "sidebar", "collapsed", json!(false));
            let theme = uicore_gpui::inline::theme(&eui_core::themes::default_theme());
            let view = uicore_gpui::inline::mount(&doc, &uicore_data::Feed::new(), theme, host, Mode::Dark, window, cx);
            Self {
                view,
                doc,
                prs_built: json!([]),
                glyphs,
                hover,
                input: IdeSidebarInput::default(),
                shown: None,
                assistant: None,
                computer,
                computer_changed: true,
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
        let (input, glyphs) = {
            let _span = tracing::trace_span!("rail.gather").entered();
            gather(nav, cx)
        };
        // A glyph is host-painted: its state changes without the data. The
        // rail renders on every tick of a mark inside it (#LOGS-4): repaint
        // the document only when a glyph or the Computer button changed.
        let glyphs_changed = *self.glyphs.borrow() != glyphs;
        *self.glyphs.borrow_mut() = glyphs;
        self.assistant = active_team_id(nav, cx).and_then(|team| crate::session_data::my_assistant(&team, cx)).map(|a| (a.id, a.name.unwrap_or_else(|| "Assistant".into())));
        self.input = input;
        self.push(cx);
        if glyphs_changed || std::mem::take(&mut self.computer_changed) {
            self.view.update(cx, |_, cx| cx.notify());
        }
    }

    /// #LOGS-11: the rows stand; only the Computer button may have moved.
    pub(crate) fn repaint_computer(&mut self, cx: &mut gpui::Context<Self>) {
        if std::mem::take(&mut self.computer_changed) {
            self.view.update(cx, |_, cx| cx.notify());
        }
    }

    /// A sample state, for the pictures.
    #[cfg(test)]
    pub(crate) fn show(&mut self, input: IdeSidebarInput, hover: Option<&str>, cx: &mut gpui::Context<Self>) {
        self.frozen = true;
        self.assistant = Some(("a0".into(), "Assistant".into()));
        // A trunk that needs a person: the warning rides the Computer button.
        *self.computer.borrow_mut() = Some(crate::sidebar::ComputerState {
            files: true,
            source_control: true,
            tooltip: "Source Control: local commits".into(),
            badge: Some(crate::sidebar::RailBadge::Icon(crate::icons::registry::UI_WARNING, cx.theme().warning)),
            active: false,
        });
        self.input = input;
        *self.hover.borrow_mut() = hover.map(str::to_string);
        self.push(cx);
    }

    fn push(&mut self, cx: &mut gpui::Context<Self>) {
        let assistant = self.assistant.as_ref().map(|(id, name)| (id.as_str(), name.as_str()));
        let value = shaped(&self.input, &self.glyphs.borrow(), self.hover.borrow().as_deref(), assistant);
        if self.shown.as_ref() == Some(&value) {
            return;
        }
        self.shown = Some(value.clone());
        let _span = tracing::trace_span!("rail.push").entered();
        if value["prs"] != self.prs_built {
            self.prs_built = value["prs"].clone();
            if let Some(tree) = with_pr_tree(&self.doc, &self.prs_built) {
                self.view.update(cx, |view, cx| {
                    let outcome = view.set_nested(tree, cx);
                    if !outcome.issues.is_empty() {
                        tracing::warn!("the sidebar's Pull requests tree: {:?}", outcome.issues);
                    }
                });
            }
        }
        self.view.update(cx, |view, cx| {
            // The IDE's sidebar never folds: the nodes the rail hides read it.
            let _ = view.set_data("/shell/rail", Some(json!(false)), cx);
            let _ = view.set_data("/shell/ide", Some(value), cx);
        });
    }

    /// #EXP-1741: the Computer button's state, from the rail's render.
    pub(crate) fn set_computer(&mut self, state: crate::sidebar::ComputerState) {
        #[cfg(test)]
        if self.frozen {
            return;
        }
        let state = Some(state);
        if *self.computer.borrow() != state {
            *self.computer.borrow_mut() = state;
            self.computer_changed = true;
        }
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
        let _span = tracing::trace_span!("render", view = "SidebarRegion").entered();
        uicore_gpui::inline::region(&self.view)
    }
}

struct RegionHost {
    computer: Rc<RefCell<Option<crate::sidebar::ComputerState>>>,
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
                "chats" => Screen::Chats,
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
        "active" => navigation::navigate_from_live_rail(window, cx, Screen::Chat { session_id: id }),
        // #EXP-1616: a Pull requests row opens its repository's page.
        "repo" => {
            crate::sidebar::select_settings_section(window, cx, SettingsSection::Repository(id));
            navigate(window, cx, Screen::Settings);
        }
        "assistant" => navigate(window, cx, Screen::Agent { agent_id: id }),
        "settings" => navigate(window, cx, Screen::Settings),
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
        if name == "ide:computer" {
            return self.computer.borrow().as_ref().map(|state| crate::sidebar::computer_button(state, cx));
        }
        if let Some(state) = name.strip_prefix("ide:trunk:") {
            let badge = if state == "error" { crate::sidebar::RailBadge::Icon(crate::icons::registry::UI_ERROR, cx.theme().danger) } else { crate::sidebar::RailBadge::Icon(crate::icons::registry::UI_WARNING, cx.theme().warning) };
            return Some(crate::sidebar::rail_badge_element(badge, 12., cx));
        }
        if let Some(rest) = name.strip_prefix("ide:count:") {
            return Some(crate::sidebar::rail_badge_element(parse_count(rest)?, 12., cx));
        }
        if let Some((badge, glyph)) = name.strip_prefix("ide:badge:").and_then(|rest| rest.split_once('|')) {
            let inner = self.icon_element(glyph, size, color, cx).or_else(|| {
                let path = crate::exponential_ui_host::icon_path(glyph)?;
                Some(gpui::svg().path(path).size(gpui::px(size)).text_color(color).into_any_element())
            })?;
            let corner = crate::sidebar::rail_badge_element(parse_count(badge.strip_prefix("count:")?)?, 12., cx);
            return Some(uicore_gpui::glyphs::cornered(inner, corner, size));
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
            Glyph::Device { device, row } => crate::chats_view::chat_device_icon(Some(device), &format!("rail-{row}"), cx).unwrap_or_else(|| Icon::empty().text_color(color).into_any_element()),
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

/// A tab's lead (`ide:tab:<key>`) and its screen, by key.
#[derive(Default)]
struct OpenTabs {
    screens: HashMap<String, Screen>,
    leads: HashMap<String, crate::surface::RichTabStatus>,
}

fn tab_key(screen: &Screen) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&format!("{screen:?}"), &mut hasher);
    format!("{:x}", std::hash::Hasher::finish(&hasher))
}

/// The tab bar (#EXP-1726): `docs/tabbar.json` in the window, over the
/// screens panel's tabs. Terminals and work tabs are one strip; it sits on
/// top in the standalone IDE and at the bottom inside eDE
/// ([`crate::shell::tab_placement`]).
pub(crate) struct TabBarRegion {
    view: Entity<SurfaceView>,
    /// The region's painted bounds: a press right of the strip's last
    /// control falls through to the window drag behind it.
    bounds: Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    doc: uicore_gpui::inline::Document,
    tabs: Rc<RefCell<OpenTabs>>,
    hover: Rc<RefCell<Option<usize>>>,
    keys: Vec<String>,
    shown: Option<Value>,
    more: Option<Vec<String>>,
    observes_panel: bool,
    _observers: Vec<gpui::Subscription>,
}

impl TabBarRegion {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut gpui::Context<Self>| {
            let tabs = Rc::new(RefCell::new(OpenTabs::default()));
            let hover = Rc::new(RefCell::new(None));
            let host = Rc::new(TabHost { tabs: tabs.clone(), hover: hover.clone(), window: window.window_handle(), region: cx.weak_entity() });
            let mut doc = uicore_gpui::inline::document(&uicore_docs::TABBAR);
            // The +N menu opens away from the window edge the strip sits on.
            let more_side = crate::shell::tab_placement().more_side;
            if more_side == uicore_model::ide::IdeEdge::Bottom {
                uicore_gpui::inline::set_prop(&mut doc, "ide_tabs_more", "side", json!("bottom"));
            }
            let theme = uicore_gpui::inline::theme(&eui_core::themes::default_theme());
            let view = uicore_gpui::inline::mount(&doc, &uicore_data::Feed::new(), theme, host, Mode::Dark, window, cx);
            let nav = crate::navigation::nav_for_window(window, cx);
            // #LOGS-4: the strip is a cached region now, so it observes what
            // its titles and leads read (issues, session states) itself.
            let mut observers = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
            if let Some(issues) = Store::try_global(cx).map(|store| store.collections().issues.clone()) {
                observers.push(cx.observe(&issues, |_, _, cx| cx.notify()));
            }
            observers.extend(crate::session_data::observe_session_collections(cx));
            Self { view, bounds: Rc::default(), doc, tabs, hover, keys: Vec::new(), shown: None, more: None, observes_panel: false, _observers: observers }
        })
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(panel) = crate::screens::screens_for_window(window, cx) else { return };
        if !self.observes_panel {
            self.observes_panel = true;
            self._observers.push(cx.observe(&panel, |_, _, cx| cx.notify()));
        }
        let items = panel.read(cx).tab_items(cx);
        let nav = crate::navigation::nav_for_window(window, cx);
        let active = resolved_screen(&nav, cx);
        let mut open = OpenTabs::default();
        let inputs: Vec<IdeTabInput> = items
            .into_iter()
            .map(|item| {
                let key = tab_key(&item.screen);
                open.screens.insert(key.clone(), item.screen);
                open.leads.insert(key.clone(), item.status);
                IdeTabInput { icon: format!("ide:tab:{key}"), key, title: item.title }
            })
            .collect();
        self.keys = inputs.iter().map(|t| t.key.clone()).collect();
        *self.tabs.borrow_mut() = open;
        let active = active.as_ref().map(tab_key);
        let projected = uicore_model::ide::ide_tabs(&inputs, active.as_deref());
        let more: Vec<Value> = self.more.iter().flatten().filter_map(|key| projected.tabs.iter().find(|t| &t.value == key)).map(|t| json!(t)).collect();
        let mut value = serde_json::to_value(&projected).expect("the tabs serialize");
        value["more"] = Value::Array(more);
        value["moreOpen"] = json!(self.more.is_some());
        if self.shown.as_ref() != Some(&value) {
            let tabs_changed = self.shown.as_ref().map(|s| &s["tabs"]) != Some(&value["tabs"]);
            self.shown = Some(value.clone());
            self.apply(&value, tabs_changed, cx);
        }
        // A lead is host-painted: its state changes without the data. No
        // notify (#LOGS-4): the strip renders only when what it observes
        // changed, and its document renders with it in that same frame; a
        // notify from here would dirty the cached band for the NEXT frame,
        // every frame.
    }

    fn apply(&mut self, value: &Value, tabs_changed: bool, cx: &mut gpui::Context<Self>) {
        if tabs_changed {
            uicore_gpui::inline::set_prop(&mut self.doc, "ide_tabs", "tabs", value["tabs"].clone());
            uicore_gpui::inline::reload(&self.view, &self.doc, cx);
        }
        self.view.update(cx, |view, cx| {
            for (key, value) in value.as_object().expect("an object") {
                if key != "tabs" {
                    let _ = view.set_data(&format!("/shell/ide/{key}"), Some(value.clone()), cx);
                }
            }
        });
    }

    /// A sample strip, for the pictures.
    #[cfg(test)]
    pub(crate) fn show(&mut self, titles: &[&str], cx: &mut gpui::Context<Self>) {
        let inputs: Vec<IdeTabInput> = titles.iter().enumerate().map(|(i, title)| IdeTabInput { key: format!("t{i}"), title: title.to_string(), icon: "nav-chats".into() }).collect();
        let value = serde_json::to_value(uicore_model::ide::ide_tabs(&inputs, Some("t1"))).unwrap();
        self.apply(&value, true, cx);
    }

    /// Where the strip's controls end (region coordinates): its tabs, their
    /// closes, the + and the +N. Right of this the band is empty.
    fn strip_right(&self, cx: &App) -> f32 {
        let view = self.view.read(cx);
        view.placed_nodes()
            .iter()
            .filter(|n| n.owner.as_deref() == Some("ide_tabs") && matches!(n.part.as_deref(), Some("tab" | "close" | "new" | "more")))
            .filter_map(|n| view.frame(n.index))
            .map(|f| f.x + f.w)
            .fold(0., f32::max)
    }

    /// The tab under the pointer, for the context menu and middle-click.
    fn hovered(&self) -> Option<Screen> {
        let index = (*self.hover.borrow())?;
        let key = self.keys.get(index)?;
        self.tabs.borrow().screens.get(key).cloned()
    }
}

fn with_panel(window: &mut Window, cx: &mut App, f: impl FnOnce(&mut crate::screens::ScreensPanel, &mut Window, &mut gpui::Context<crate::screens::ScreensPanel>)) {
    if let Some(panel) = crate::screens::screens_for_window(window, cx) {
        panel.update(cx, |panel, cx| f(panel, window, cx));
    }
}

impl Render for TabBarRegion {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "TabBarRegion").entered();
        self.refresh(window, cx);
        let region = cx.weak_entity();
        let menu_region = region.clone();
        let (record, bounds) = (self.bounds.clone(), self.bounds.clone());
        let strip_right = self.strip_right(cx);
        // On the top band the strip sits inside the window's titlebar: a
        // press on a control is the strip's (never a window drag, never the
        // Linux window menu), a press on the empty rest is the titlebar's.
        let on_control = move |position: gpui::Point<gpui::Pixels>| bounds.get().is_none_or(|b| f32::from(position.x - b.origin.x) <= strip_right);
        let on_control_right = on_control.clone();
        let strip = div()
            .id("tab-bar")
            .size_full()
            .child(uicore_gpui::inline::region(&self.view))
            .on_mouse_down(gpui::MouseButton::Middle, move |_, window, cx| {
                let Some(screen) = region.read_with(cx, |region, _| region.hovered()).ok().flatten() else { return };
                cx.stop_propagation();
                with_panel(window, cx, |panel, window, cx| panel.close_screen(&screen, window, cx));
            })
            .pointer_context_menu(move |menu, _, cx| {
                let Some(screen) = menu_region.read_with(cx, |region, _| region.hovered()).ok().flatten() else { return menu };
                let (close, others, undock) = (screen.clone(), screen.clone(), screen.clone());
                let undockable = matches!(screen, Screen::Terminal { .. }) || screen.undockable();
                let menu = if undockable {
                    menu.item(crate::controls::pointer_label_item("Open in new window", false).on_click(move |_, window, cx| {
                        let undock = undock.clone();
                        with_panel(window, cx, |panel, window, cx| panel.undock_screen(&undock, window, cx));
                    }))
                } else {
                    menu
                };
                menu.item(crate::controls::pointer_label_item("Close", false).on_click(move |_, window, cx| {
                    let close = close.clone();
                    with_panel(window, cx, |panel, window, cx| panel.close_screen(&close, window, cx));
                }))
                .item(crate::controls::pointer_label_item("Close others", false).on_click(move |_, window, cx| {
                    let others = others.clone();
                    with_panel(window, cx, |panel, window, cx| panel.close_others_of(&others, window, cx));
                }))
                .item(crate::controls::pointer_label_item("Close all", false).on_click(|_, window, cx| {
                    with_panel(window, cx, |panel, window, cx| panel.close_every_tab(window, cx));
                }))
            });
        // A wrapper, so the strip's own listeners (the genUI view's, the
        // context menu) run before the stop below.
        div()
            .size_full()
            .on_children_prepainted(move |children, _, _| record.set(children.first().copied()))
            .on_mouse_down(gpui::MouseButton::Left, move |ev, _, cx| {
                if on_control(ev.position) {
                    cx.stop_propagation();
                }
            })
            .on_mouse_down(gpui::MouseButton::Right, move |ev, _, cx| {
                if on_control_right(ev.position) {
                    cx.stop_propagation();
                }
            })
            .child(strip)
    }
}

struct TabHost {
    tabs: Rc<RefCell<OpenTabs>>,
    hover: Rc<RefCell<Option<usize>>>,
    window: AnyWindowHandle,
    region: gpui::WeakEntity<TabBarRegion>,
}

impl TabHost {
    fn in_window(&self, cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
        let handle = self.window;
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| f(window, cx));
        });
    }

    fn screen(&self, key: &str) -> Option<Screen> {
        self.tabs.borrow().screens.get(key).cloned()
    }
}

/// A strip part's tab index: `<tabs>…​.<index>`.
fn tab_index(component: &str) -> Option<usize> {
    component.contains("ide_tabs").then(|| component.rsplit('.').next()?.parse().ok()).flatten()
}

impl HostPlugin for TabHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        crate::exponential_ui_host::icon_path(name)
    }

    fn icon_element(&self, name: &str, size: f32, _color: Hsla, _cx: &App) -> Option<AnyElement> {
        let key = name.strip_prefix("ide:tab:")?;
        let tabs = self.tabs.borrow();
        Some(match tabs.leads.get(key)? {
            crate::surface::RichTabStatus::Dot(tone) => crate::sidebar::rail_dot(*tone).into_any_element(),
            crate::surface::RichTabStatus::Glyph(icon) => icon.clone().with_size(gpui::px(size)).into_any_element(),
            crate::surface::RichTabStatus::None => div().size(gpui::px(size)).into_any_element(),
        })
    }

    fn on_hover(&self, component_id: &str, hovered: bool, _cx: &mut App) {
        let Some(index) = tab_index(component_id) else { return };
        let mut hover = self.hover.borrow_mut();
        if hovered {
            *hover = Some(index);
        } else if *hover == Some(index) {
            *hover = None;
        }
    }

    fn on_action(&self, event: &ActionEvent, cx: &mut App) {
        if event.name != "ide_tab" {
            return;
        }
        let action = event.context["action"].as_str().unwrap_or_default().to_string();
        let payload = event.payload.clone().unwrap_or(Value::Null);
        let chosen = event.context["value"].as_str().or(payload["value"].as_str()).map(str::to_string);
        let region = self.region.clone();
        match action.as_str() {
            "open" => {
                let Some(screen) = chosen.and_then(|key| self.screen(&key)) else { return };
                self.in_window(cx, move |window, cx| {
                    let _ = region.update(cx, |region, cx| {
                        region.more = None;
                        cx.notify();
                    });
                    with_panel(window, cx, |panel, window, cx| panel.open_screen(screen, window, cx));
                });
            }
            "close" => {
                let Some(screen) = chosen.and_then(|key| self.screen(&key)) else { return };
                self.in_window(cx, move |window, cx| with_panel(window, cx, |panel, window, cx| panel.close_screen(&screen, window, cx)));
            }
            // #EXP-1741: the + opens the composer page (Home), not a terminal.
            "new" => self.in_window(cx, |window, cx| navigate(window, cx, Screen::Home)),
            "reorder" => {
                let Some((screen, to)) = chosen.and_then(|key| self.screen(&key)).zip(payload["to"].as_u64()) else { return };
                self.in_window(cx, move |window, cx| with_panel(window, cx, |panel, _, cx| panel.move_tab(&screen, to as usize, cx)));
            }
            "more" => {
                let hidden: Vec<String> = payload["values"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect();
                self.in_window(cx, move |_, cx| {
                    let _ = region.update(cx, |region, cx| {
                        region.more = Some(hidden);
                        cx.notify();
                    });
                });
            }
            "more_closed" if payload["open"] == json!(false) => self.in_window(cx, move |_, cx| {
                let _ = region.update(cx, |region, cx| {
                    region.more = None;
                    cx.notify();
                });
            }),
            _ => {}
        }
    }
}

/// What the header region leaves to [`SidebarRegion`]: the rows and the
/// corners.
const HEADER_PRUNED: &[&str] = &["ide_nav", "scroll", "corner"];

/// The left column's header (#EXP-1725): the team switcher, the inbox,
/// search and new, the rail toggle: `sidebar.json`'s header and its
/// switcher menu, mounted on their own so the header stays fixed while the
/// rail and the settings nav slide under it.
pub(crate) struct HeaderRegion {
    view: Entity<SurfaceView>,
    team: Rc<RefCell<Option<Option<crate::markdown::CachedImage>>>>,
    shown: Option<Value>,
}

impl HeaderRegion {
    pub(crate) fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut gpui::Context<Self>| {
            let team = Rc::new(RefCell::new(None));
            let host = Rc::new(HeaderHost { team: team.clone(), nav, window: window.window_handle(), region: cx.weak_entity() });
            let mut doc = uicore_gpui::inline::document(uicore_docs::doc("sidebar").expect("the factory sidebar"));
            uicore_gpui::inline::without(&mut doc, HEADER_PRUNED);
            uicore_gpui::inline::set_prop(&mut doc, "sidebar", "collapsed", json!(false));
            let theme = uicore_gpui::inline::theme(&eui_core::themes::default_theme());
            let view = uicore_gpui::inline::mount(&doc, &uicore_data::Feed::new(), theme, host, Mode::Dark, window, cx);
            Self { view, team, shown: None }
        })
    }

    /// The switcher's sections, the active team's mark, the other teams'
    /// needs-you counts (#FEED-232: attention; the corner sums them), whether
    /// there is a board for New.
    pub(crate) fn refresh(&mut self, nav: &Entity<Navigation>, needs_you: &std::collections::BTreeMap<String, usize>, cx: &mut gpui::Context<Self>) {
        let active = active_team_id(nav, cx);
        let locals = crate::local_desktops::entries(cx);
        let on_local = locals.iter().any(|(_, _, current)| *current);
        let teams = Store::global(cx).collections().teams_sorted(cx);
        let account = crate::queries::active_account(cx);
        let current = teams.iter().find(|t| Some(&t.id) == active.as_ref()).cloned();
        let image = crate::user_avatar::team_image(current.as_ref().and_then(|t| t.icon_url.as_deref()), cx);
        *self.team.borrow_mut() = Some(image);
        let mut others = Vec::new();
        let mut team_rows = Vec::new();
        if !on_local {
            for team in &teams {
                let is_current = Some(&team.id) == active.as_ref();
                let count = if is_current { 0 } else { needs_you.get(&team.id).copied().unwrap_or(0) };
                others.push(count as i64);
                let label = uicore_model::ide::count_label(count);
                team_rows.push(json!({"kind": "team", "id": team.id, "name": team.name, "current": is_current, "dotIcon": count_icon(label.as_deref(), "attention")}));
            }
        }
        let elsewhere = uicore_model::ide::count_label(domain::nav_badges::app_icon_badge(others));
        let mut sections = vec![json!({
            "kind": "local", "id": "local", "label": "Local desktops",
            "entries": locals.iter().map(|(id, name, current)| json!({"kind": "local", "id": id, "name": name, "current": current, "dotIcon": Value::Null})).collect::<Vec<_>>(),
        })];
        if !team_rows.is_empty() {
            sections.push(json!({"kind": "account", "id": "account", "label": account.as_ref().map(|a| a.email.clone()).unwrap_or_default(), "entries": team_rows}));
        }
        let name = current.map(|t| t.name).unwrap_or_default();
        let chevron = match elsewhere {
            Some(label) => format!("ide:badge:count:attention:{label}|nav-team-switcher"),
            None => "nav-team-switcher".to_string(),
        };
        let value = json!({
            "switcher": {"current": {"name": name}, "sections": sections},
            "ide": {"teamMark": "ide:team", "chevron": chevron, "inbox": "ide:inbox", "noBoard": active_board_id(nav, cx).is_none(), "signedIn": !on_local},
        });
        if self.shown.as_ref() == Some(&value) {
            self.view.update(cx, |_, cx| cx.notify());
            return;
        }
        self.shown = Some(value.clone());
        self.view.update(cx, |view, cx| {
            let _ = view.set_data("/shell/switcher", Some(value["switcher"].clone()), cx);
            let _ = view.set_data("/shell/ide", Some(value["ide"].clone()), cx);
        });
    }

    fn close_menu(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.view.update(cx, |view, cx| {
            view.escape(window, cx);
        });
    }
}

impl Render for HeaderRegion {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "HeaderRegion").entered();
        div().w_full().child(self.view.clone())
    }
}

struct HeaderHost {
    team: Rc<RefCell<Option<Option<crate::markdown::CachedImage>>>>,
    nav: Entity<Navigation>,
    window: AnyWindowHandle,
    region: gpui::WeakEntity<HeaderRegion>,
}

impl HeaderHost {
    fn in_window(&self, cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
        let handle = self.window;
        let region = self.region.clone();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = region.update(cx, |region, cx| region.close_menu(window, cx));
                f(window, cx);
                window.refresh();
            });
        });
    }
}

impl HostPlugin for HeaderHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        crate::exponential_ui_host::icon_path(name)
    }

    fn icon_element(&self, name: &str, size: f32, color: Hsla, cx: &App) -> Option<AnyElement> {
        match name {
            "ide:team" => Some(crate::user_avatar::team_avatar_of(self.team.borrow().clone().flatten(), 28.)),
            "ide:inbox" => Some(crate::inbox_popover::InboxButton::new(self.nav.clone()).into_any_element()),
            _ => {
                let (badge, glyph) = name.strip_prefix("ide:badge:count:")?.split_once('|')?;
                let path = crate::exponential_ui_host::icon_path(glyph)?;
                let corner = crate::sidebar::rail_badge_element(parse_count(badge)?, 12., cx);
                Some(uicore_gpui::glyphs::cornered(gpui::svg().path(path).size(gpui::px(size)).text_color(color).into_any_element(), corner, size))
            }
        }
    }

    fn on_function_call(&self, event: &FunctionCallEvent, cx: &mut App) {
        let nav = self.nav.clone();
        let args = event.args.clone();
        match (event.name.as_str(), event.component_id.as_str()) {
            ("ede_start_menu", _) => self.in_window(cx, |window, cx| crate::search_sheet::open_search(window, cx)),
            ("ede_launch", "new" | "rail_new") => self.in_window(cx, move |window, cx| {
                if let Some(board) = active_board_id(&nav, cx) {
                    crate::issue_draft_screen::open_new_from_rail(window, cx, board);
                }
            }),
            ("ede_switcher", _) => self.in_window(cx, move |window, cx| {
                let id = args["id"].as_str().unwrap_or_default().to_string();
                match (args["action"].as_str(), args["kind"].as_str()) {
                    (Some("pick"), Some("local")) => {
                        crate::local_desktops::enter(&id, cx);
                    }
                    (Some("pick"), _) => window.dispatch_action(Box::new(crate::actions::SwitchTeam { team_id: id }), cx),
                    (Some("new"), _) => {
                        crate::local_desktops::create(cx);
                    }
                    (Some("connect"), _) => crate::local_desktops::leave(cx),
                    _ => {}
                }
            }),
            _ => {}
        }
    }

    fn on_action(&self, event: &ActionEvent, cx: &mut App) {
        if event.name != "ide_switcher" {
            return;
        }
        let action = event.context["action"].as_str().unwrap_or_default().to_string();
        self.in_window(cx, move |window, cx| match action.as_str() {
            "new_team" => window.dispatch_action(Box::new(crate::actions::CreateTeam), cx),
            "join_team" => window.dispatch_action(Box::new(crate::actions::JoinTeam), cx),
            _ => {}
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rows_hover_and_count_reach_the_document() {
        let input = IdeSidebarInput {
            screen: "home".into(),
            chats: IdeCount { count: 2, tone: IdeTone::Attention },
            boards: vec![IdeBoardInput { id: "b1".into(), name: "Web".into(), icon: "ide:board:b1".into() }],
            ..Default::default()
        };
        let value = shaped(&input, &HashMap::new(), Some("b1"), None);
        assert_eq!(value["nav"][0]["dotIcon"], "ide:count:attention:2");
        let counted = IdeSidebarInput { needs_you: 5, drafts: 120, ..input.clone() };
        let counted = shaped(&counted, &HashMap::new(), None, None);
        assert_eq!(counted["nav"][0]["dotIcon"], "ide:count:attention:5");
        assert_eq!(counted["nav"][3]["dotIcon"], "ide:count:muted:99+");
        assert_eq!(value["boards"]["rows"][0]["hover"], true);

    }

    /// #FEED-232: a glyph name reads back as THE count badge.
    #[test]
    fn a_count_glyph_parses_back() {
        assert!(matches!(parse_count("attention:3"), Some(crate::sidebar::RailBadge::Count(3, crate::surface::BadgeTone::Attention))));
        assert!(matches!(parse_count("muted:99+"), Some(crate::sidebar::RailBadge::Count(100, crate::surface::BadgeTone::Muted))));
        assert!(parse_count("amber:3").is_none());
    }

    /// #EXP-1616: only eCode PRs (their url) of the team make the tree.
    #[test]
    fn the_pr_tree_reads_ecode_prs_off_their_url() {
        assert_eq!(ecode_repo_name("https://exponential.at/t/acme/repos/engine/pulls/12"), Some("engine"));
        assert_eq!(ecode_repo_name("https://github.com/acme/engine/pull/12"), None);
        assert_eq!(ecode_repo_name("https://exponential.at/t/acme/repos/engine/pulls/x"), None);
        let pull = |id: &str, team: &str, url: &str, number: i64, head: &str, base: &str| {
            serde_json::from_value::<domain::rows::PullRequestRow>(json!({
                "id": id, "team_id": team, "repository_id": "r1", "url": url, "number": number, "title": id, "head": head, "base": base, "state": "open",
            }))
            .unwrap()
        };
        let url = |n: i64| format!("https://exponential.at/t/acme/repos/engine/pulls/{n}");
        let pulls = vec![
            pull("p1", "t1", &url(1), 1, "sync", "main"),
            pull("p2", "t1", &url(2), 2, "sync-tests", "sync"),
            pull("gh", "t1", "https://github.com/acme/engine/pull/3", 3, "x", "main"),
            pull("other", "t2", &url(4), 4, "y", "main"),
        ];
        let repos = ecode_repos("t1", &pulls);
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name, "engine");
        let rows: Vec<(&str, usize, &str)> = repos[0].prs.iter().map(|p| (p.identifier.as_str(), p.depth, p.subtitle.as_str())).collect();
        assert_eq!(rows, vec![("#1", 0, "sync → main"), ("#2", 1, "sync-tests → sync")]);
    }

    /// #EXP-1616: the tree's rows land in the document's Section{tree}.
    #[test]
    fn the_pr_tree_fills_its_section() {
        let doc = uicore_gpui::inline::document(uicore_docs::doc("sidebar").expect("the sidebar"));
        let input = IdeSidebarInput {
            repos: vec![IdeRepoInput { id: "r1".into(), name: "engine".into(), prs: vec![IdePrInput { id: "p1".into(), identifier: "#1".into(), title: "Sync".into(), subtitle: "sync → main".into(), depth: 0 }] }],
            ..Default::default()
        };
        let value = shaped(&input, &HashMap::new(), None, None);
        let tree = serde_json::to_value(with_pr_tree(&doc, &value["prs"]).expect("the tree")).unwrap();
        fn find<'a>(node: &'a Value, id: &str) -> Option<&'a Value> {
            if node["id"] == id {
                return Some(node);
            }
            node["children"].as_array().into_iter().flatten().chain(node["slots"].as_object().into_iter().flat_map(|slots| slots.values())).find_map(|c| find(c, id))
        }
        let section = find(&tree, "ide_prs_tree").expect("the section");
        assert_eq!(section["props"]["tree"], true);
        let rows = section["children"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["props"]["title"], "engine");
        assert_eq!(rows[0]["props"]["depth"], 0);
        assert_eq!(rows[1]["props"]["identifier"], "#1");
        assert_eq!(rows[1]["props"]["subtitle"], "sync → main");
        assert_eq!(rows[1]["props"]["depth"], 1);
        assert_eq!(rows[1]["on"]["press"]["event"]["context"], json!({"kind": "repo", "id": "r1"}));
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
        for id in PRUNED.iter().chain(HEADER_PRUNED) {
            assert!(doc.source.contains(&format!("\"id\": \"{id}\"")), "{id}");
        }
    }
}
