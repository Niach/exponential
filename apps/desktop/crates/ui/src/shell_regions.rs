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
    view::{ListExternalPhase, SurfaceView},
};
use crate::controls::PointerContextMenuExt as _;
use gpui::{div, prelude::*, AnyElement, AnyWindowHandle, App, Entity, Hsla, SharedString, Window};
use gpui_component::{ActiveTheme as _, Icon, Sizable as _};
use serde_json::{json, Value};
use sync::Store;
use domain::sidebar_folders::{sidebar_entries, sidebar_folder_name, sidebar_folder_open_key, sidebar_move, sidebar_ungroup, SidebarEntry, SidebarEntryKind, SidebarMove, SidebarSection, SidebarWrites};
use uicore_model::ide::{IdeTabInput, ide_sidebar, IdeActiveInput, IdeCount, IdeDeviceInput, IdeItemInput, IdeLayoutEntry, IdePrInput, IdeRepoInput, IdeSidebarInput, IdeTeamInput, IdeTone};

use crate::navigation::{self, active_board_id, navigate, resolved_screen, Navigation, Screen};
use crate::settings::SettingsSection;

/// What the sidebar document draws itself; the rest is the IDE's own until
/// its region moves (the header, the footer). `chats` is eDE's rail list
/// (#EDE-65); the IDE lists its chats under Chats.
const PRUNED: &[&str] = &["header", "switcher_menu", "chats"];

/// The corner: the IDE's footer (the account, the Computer button and the
/// personal settings), mounted on its own under the rail's Getting started
/// entry so the rows above it scroll and the footer stays at the bottom.
const CORNER: &str = "corner";

/// What the footer's mount leaves out: everything but the corner.
const FOOTER_PRUNED: &[&str] = &["header", "switcher_menu", "chats", "ide_nav", "scroll"];

/// The footer's account: the avatar's hue key and label, its decoded image
/// (`None` = signed out: the generic person).
#[derive(Clone, PartialEq)]
pub(crate) struct AccountFace {
    pub(crate) user_id: String,
    pub(crate) label: String,
    pub(crate) image: Option<std::sync::Arc<gpui::Image>>,
}

/// The active account as the footer draws it: the first name only (the full
/// name and email live in Settings › Account; a name-less account takes its
/// email), the profile image from the synced users row.
pub(crate) fn account_face(cx: &mut App) -> Option<AccountFace> {
    let account = crate::queries::active_account(cx)?;
    let label = account.name.clone().filter(|name| !name.trim().is_empty()).unwrap_or_else(|| account.email.clone());
    let image_url = crate::queries::active_user(cx).and_then(|user| user.image);
    let image = crate::user_avatar::cached_avatar_image(cx, image_url.as_deref());
    Some(AccountFace { user_id: account.user_id, label, image })
}

/// A row glyph the IDE paints.
#[derive(Clone, PartialEq)]
enum Glyph {
    Board(domain::rows::Board),
    Agent(domain::session_tree::AgentRow),
    App(crate::apps_view::AppEntry),
    Run(Option<coding::CodingAgent>, SessionState),
    Device { device: String, row: String },
    /// #EXP-2027: a team's mark (its image once loaded, else the team
    /// glyph).
    Team(String, bool),
    /// #EXP-2027: an online device's green dot.
    Online,
}

/// The screen as the fixture's `<kind>` or `<kind>:<id>`.
fn screen_key(screen: Option<&Screen>, active_board: Option<&str>, cx: &App) -> String {
    match screen {
        Some(Screen::Home) => "home".into(),
        Some(Screen::NeedsYou) => "needs-you".into(),
        Some(Screen::Chats) => "chats".into(),
        Some(Screen::Device { device_id }) => format!("device:{}", if device_id.is_empty() { crate::device_page::this_device_row_id(cx).unwrap_or_default() } else { device_id.clone() }),
        Some(Screen::DeviceSettings { device_id }) => format!("device-settings:{device_id}"),
        Some(Screen::Drafts) => "drafts".into(),
        Some(Screen::IssueDetail { issue_id }) => format!("issue:{issue_id}"),
        Some(Screen::Chat { session_id }) => format!("chat:{session_id}"),
        Some(Screen::Session { session_id }) => format!("session:{session_id}"),
        Some(Screen::Agent { agent_id }) => format!("agent:{}", crate::agent_screen::resolve_agent_id(agent_id, cx)),
        Some(Screen::AppRun { app } | Screen::App { app }) => format!("app:{app}"),
        Some(Screen::BoardIssues { board_id }) if board_id.is_empty() => format!("board:{}", active_board.unwrap_or_default()),
        Some(Screen::BoardIssues { board_id }) => format!("board:{board_id}"),
        // #EXP-1616: a repo page (Settings › Repositories › one repo) lights
        // that repo's Pull requests rows.
        Some(Screen::Settings) => match crate::sidebar::any_settings_section(cx) {
            Some(crate::settings::SettingsSection::Repository(id)) => format!("repo:{id}"),
            _ => String::new(),
        },
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
    let mut sessions: HashMap<String, String> = HashMap::new();
    for pull in pulls {
        if pull.team_id.as_deref().is_some_and(|team| team != team_id) {
            continue;
        }
        let (Some(repo), Some(name)) = (pull.repository_id.as_deref(), pull.url.as_deref().and_then(ecode_repo_name)) else { continue };
        if let Some(root) = pull.root_session_id.as_deref() {
            sessions.insert(pull.id.clone(), root.to_string());
        }
        repos.entry((name.to_string(), repo.to_string())).or_default().push(RepoPr::from_row(pull));
    }
    repos
        .into_iter()
        .map(|((name, id), prs)| IdeRepoInput {
            id,
            name,
            prs: repo_pr_tree(&prs).into_iter().map(|row| IdePrInput { session: sessions.get(&row.id).cloned(), id: row.id, identifier: row.identifier, title: row.title, subtitle: row.subtitle, depth: row.depth }).collect(),
        })
        .collect()
}

/// #EXP-1832 / #EXP-2027: the member's synced layout of `team_id`'s ONE
/// mixed list (section `team`).
fn layout_entries(team_id: &str, cx: &App) -> Vec<SidebarEntry> {
    let Some(store) = Store::try_global(cx) else { return Vec::new() };
    sidebar_entries(store.collections().sidebar_layout.read(cx).iter(), team_id, SidebarSection::Team)
}

fn layout_entry(entry: SidebarEntry) -> IdeLayoutEntry {
    IdeLayoutEntry { id: entry.id, kind: entry.kind.as_str().into(), target_id: entry.target_id, parent_id: entry.parent_id, name: entry.name, sort_order: entry.sort_order }
}

/// #EXP-2027: a team's items in the list's default order (the host's:
/// boards, agents, apps, then the placed sessions in entry order).
fn team_items(team: &str, input: &IdeSidebarInput) -> Vec<String> {
    input.teams.iter().find(|t| t.id == team).map(|t| t.items.iter().map(|i| i.id.clone()).collect()).unwrap_or_default()
}

/// #EXP-1832: write one move's rows (`sidebarLayout.save`); the synced shape
/// brings them back.
fn save_layout(team_id: String, writes: SidebarWrites, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else { return };
    cx.background_executor()
        .spawn(async move {
            let input = api::sidebar_layout::SidebarLayoutSaveInput::new(&team_id, SidebarSection::Team, &writes);
            if let Err(err) = api::sidebar_layout::sidebar_layout_save(&trpc, &input) {
                log::warn!("[ui] sidebarLayout.save(team) failed: {err}");
            }
        })
        .detach();
}

/// #EXP-2027: `Remove from sidebar` (`sidebarLayout.remove`).
fn remove_from_sidebar(target_id: String, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else { return };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::sidebar_layout::sidebar_layout_remove(&trpc, &target_id) {
                log::warn!("[ui] sidebarLayout.remove failed: {err}");
            }
        })
        .detach();
}

/// #EXP-1832: name a folder (`sidebarLayout.rename`; empty = the default).
fn rename_folder(id: String, name: String, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else { return };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::sidebar_layout::sidebar_layout_rename(&trpc, &id, &name) {
                log::warn!("[ui] sidebarLayout.rename failed: {err}");
            }
        })
        .detach();
}

/// #EXP-2027: the loaded team marks by team id (`None` = the glyph).
type TeamMarks = HashMap<String, Option<crate::markdown::CachedImage>>;

/// #EXP-2027: the team row's glyph name (the host paints the mark).
pub(crate) fn team_mark_icon(team_id: &str) -> String {
    format!("ide:team:{team_id}")
}

/// #EXP-2027 (sidebar-tree.json): the Devices row's children: the member's
/// own devices, then teammates' servers shared with one of their teams.
fn gather_devices(teams: &[String], glyphs: &mut HashMap<String, Glyph>, cx: &App) -> Vec<IdeDeviceInput> {
    use domain::sidebar_tree::{sidebar_device_rows, SidebarDeviceInput};
    let Some(store) = Store::try_global(cx) else { return Vec::new() };
    let me = crate::queries::active_account(cx).map(|account| account.user_id);
    let now = crate::session_data::now_ms();
    let rows: Vec<domain::rows::DeviceRow> = store.collections().devices.read(cx).iter().cloned().collect();
    let inputs: Vec<SidebarDeviceInput> = rows
        .iter()
        .map(|row| SidebarDeviceInput {
            id: row.id.clone(),
            name: row.label.clone().filter(|l| !l.trim().is_empty()).unwrap_or_else(|| "Device".into()),
            mine: me.is_some() && row.user_id == me,
            online: crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now),
            shared_team_ids: row.shared_team_ids.clone(),
        })
        .collect();
    sidebar_device_rows(&inputs, teams)
        .into_iter()
        .filter_map(|out| {
            let row = rows.iter().find(|row| row.id == out.id)?;
            let input = inputs.iter().find(|input| input.id == out.id)?;
            let team_mark = out.team_id.as_deref().map(team_mark_icon);
            if let Some(team) = &out.team_id {
                glyphs.entry(team_mark_icon(team)).or_insert_with(|| Glyph::Team(team.clone(), false));
            }
            Some(IdeDeviceInput { id: out.id, name: input.name.clone(), icon: crate::icons::device_icon_name(row.icon.as_deref(), row.is_server()).into(), online: !out.muted, team_mark })
        })
        .collect()
}

/// Every team's rows as the fixture's input, with the glyphs they name.
fn gather(nav: &Entity<Navigation>, marks: &mut TeamMarks, cx: &mut App) -> (IdeSidebarInput, HashMap<String, Glyph>) {
    let mut glyphs = HashMap::new();
    glyphs.insert("ide:dot:green".to_string(), Glyph::Online);
    let screen = resolved_screen(nav, cx);
    let open_draft = match &screen {
        Some(Screen::IssueDraft { draft_id, .. }) => Some(draft_id.clone()),
        _ => None,
    };
    let board = active_board_id(nav, cx);
    let mut input = IdeSidebarInput { screen: screen_key(screen.as_ref(), board.as_deref(), cx), devices_open: crate::ui_prefs::sidebar_folder_open(domain::sidebar_tree::SIDEBAR_DEVICES_OPEN_KEY), ..Default::default() };
    let Some(store) = Store::try_global(cx) else { return (input, glyphs) };
    let collections = store.collections().clone();
    // #EXP-2027: every member team (the teams shape never syncs the
    // personal team: `getMemberTeamIds`).
    let all_teams = collections.teams_sorted(cx);
    let shown: Vec<String> = domain::sidebar_tree::sidebar_teams(
        &all_teams.iter().map(|t| domain::sidebar_tree::SidebarTeamInput { id: t.id.clone(), name: t.name.clone(), kind: "team".into() }).collect::<Vec<_>>(),
    );
    input.devices = gather_devices(&shown, &mut glyphs, cx);
    let mut chats_attention = false;
    for team in &all_teams {
        input.drafts += crate::drafts::drafts_in_team(&team.id, cx).iter().filter(|d| Some(&d.id) != open_draft.as_ref()).count();
        let badge = crate::session_data::rail_chats_count(&team.id, cx);
        input.chats.count += badge.count;
        chats_attention |= badge.count > 0 && badge.tone == domain::nav_badges::CountBadgeTone::Attention;
    }
    input.chats.tone = if chats_attention { IdeTone::Attention } else { IdeTone::Muted };
    let needs_you = {
        let _span = tracing::trace_span!("rail.needs_you").entered();
        crate::needs_you::counts_by_team(cx)
    };
    input.needs_you = needs_you.values().sum();
    let sessions: Vec<domain::rows::SessionRow> = collections.sessions.read(cx).iter().filter(|s| s.parent_id.is_none()).cloned().collect();
    for team_id in &shown {
        let Some(team) = all_teams.iter().find(|t| &t.id == team_id) else { continue };
        let mark = team_mark_icon(team_id);
        let image = crate::user_avatar::team_image(team.icon_url.as_deref(), cx);
        glyphs.insert(mark.clone(), Glyph::Team(team_id.clone(), image.is_some()));
        marks.insert(team_id.clone(), image);
        let mut items = Vec::new();
        for board in collections.boards_in_team(team_id, cx) {
            let icon = format!("ide:board:{}", board.id);
            items.push(IdeItemInput { kind: "board".into(), id: board.id.clone(), name: board.name.clone(), icon: icon.clone(), ..Default::default() });
            glyphs.insert(icon, Glyph::Board(board));
        }
        // #FEED-232: an agent row counts its conversation's unread replies.
        let unread: HashMap<String, usize> = crate::session_data::agent_unread_counts(team_id, cx);
        for agent in crate::session_data::rail_agents_in_team(team_id, cx) {
            let icon = format!("ide:agent:{}", agent.agent_id);
            let unread = unread.get(&agent.agent_id).copied().unwrap_or(0);
            items.push(IdeItemInput { kind: "agent".into(), id: agent.agent_id.clone(), name: agent.name.clone(), icon: icon.clone(), needs_you: agent.needs_you, unread, ..Default::default() });
            glyphs.insert(icon, Glyph::Agent(agent));
        }
        for app in crate::apps_view::app_entries(team_id, cx) {
            let icon = format!("ide:app:{}", app.id);
            items.push(IdeItemInput { kind: "app".into(), id: app.id.clone(), slug: Some(app.slug.clone()), name: app.name.clone(), icon: icon.clone(), ..Default::default() });
            glyphs.insert(icon, Glyph::App(app));
        }
        let entries = layout_entries(team_id, cx);
        // #EXP-2027: the team's root sessions the member placed, in entry
        // order (sidebarTeamItems), ended ones too until removed.
        let placed = domain::sidebar_tree::sidebar_team_items(
            team_id,
            &[],
            &[],
            &[],
            &sessions.iter().filter_map(|s| Some(domain::sidebar_tree::SidebarSessionInput { id: s.id.clone(), team_id: s.team_id.clone()?, parent_id: None })).collect::<Vec<_>>(),
            &entries,
        );
        for item in placed {
            let Some(session) = sessions.iter().find(|s| s.id == item.id) else { continue };
            let icon = format!("ide:run:{}", session.id);
            let agent = crate::run_rows::run_agent(session.cli.as_deref());
            let state = crate::sidebar::running_row_state(crate::session_data::session_is_working(session), false);
            glyphs.insert(icon.clone(), Glyph::Run(agent, state));
            let tree: Vec<String> = collections.sessions.read(cx).iter().filter(|s| s.root_id_or_self() == session.id).map(|s| s.id.clone()).collect();
            let name = crate::session_data::session_title_of(&session.id, cx).unwrap_or_else(|| domain::session_tree::SESSION_CHAT.copy.untitled.clone());
            items.push(IdeItemInput { kind: "session".into(), id: session.id.clone(), name, icon, session_ids: tree, ..Default::default() });
        }
        input.closed.extend(entries.iter().filter(|e| e.kind == SidebarEntryKind::Folder && !crate::ui_prefs::sidebar_folder_open(&sidebar_folder_open_key(team_id, &e.id))).map(|e| e.id.clone()));
        input.teams.push(IdeTeamInput {
            id: team_id.clone(),
            name: team.name.clone(),
            mark,
            open: crate::ui_prefs::sidebar_folder_open(&domain::sidebar_tree::sidebar_team_open_key(team_id)),
            owner: crate::settings::is_owner(cx, team_id),
            needs_you: needs_you.get(team_id).copied().unwrap_or(0),
            items,
            layout: entries.into_iter().map(layout_entry).collect(),
        });
    }
    {
        let _span = tracing::trace_span!("rail.prs").entered();
        let pulls: Vec<domain::rows::PullRequestRow> = collections.pull_requests.read(cx).iter().cloned().collect();
        input.repos = all_teams.iter().flat_map(|team| ecode_repos(&team.id, pulls.iter())).collect();
    }
    let _span = tracing::trace_span!("rail.live_roots").entered();
    for team in &all_teams {
        for row in crate::session_data::live_roots_for_rail(&team.id, cx) {
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
            let team_mark = shown.contains(&team.id).then(|| team_mark_icon(&team.id));

            input.active.push(IdeActiveInput { id: row.root_id, title: row.title, icon, session_ids: row.session_ids, unread: row.unread, team_mark, team: Some(team.id.clone()) });
        }
    }
    (input, glyphs)
}

/// The fixture's output as the document binds it: counts as glyph names, the
/// row under the pointer marked, the active rows' device glyphs, every team
/// row's `team`.
fn shaped(input: &IdeSidebarInput, glyphs: &HashMap<String, Glyph>, hover: Option<&str>) -> Value {
    let mut value = serde_json::to_value(ide_sidebar(input)).expect("the sidebar serializes");
    let mark = |rows: &mut Value| {
        for row in rows.as_array_mut().into_iter().flatten() {
            row["dotIcon"] = count_icon(row["count"].as_str(), row["tone"].as_str().unwrap_or("muted"));
            row["hover"] = json!(hover == row["id"].as_str());
        }
    };
    mark(&mut value["nav"]);
    mark(&mut value["devices"]);
    for team in value["teams"].as_array_mut().into_iter().flatten() {
        let id = team["id"].clone();
        mark(&mut team["rows"]);
        for row in team["rows"].as_array_mut().into_iter().flatten() {
            row["team"] = id.clone();
        }
    }
    value["computer"] = json!("ide:computer");
    value["signedIn"] = json!(true);
    for row in value["active"].as_array_mut().into_iter().flatten() {
        let device = format!("ide:device:{}", row["id"].as_str().unwrap_or_default());
        row["deviceIcon"] = if glyphs.contains_key(&device) { json!(device) } else { Value::Null };
    }
    value["tree"] = Value::Array(tree_rows(input, &value, hover));
    value
}

/// The key an Active row takes in the tree's List (a placed session's own
/// row keeps the bare id).
const ACTIVE_KEY: &str = "active:";

/// #EXP-2027: the teams and the Active band as ONE List's rows (so an
/// Active row drags into a team): every team's rows, then the Active label
/// row and one row per running chat (key `active:<root>`, depth 1, its
/// team).
fn tree_rows(input: &IdeSidebarInput, value: &Value, hover: Option<&str>) -> Vec<Value> {
    let mut rows: Vec<Value> = value["teams"].as_array().into_iter().flatten().flat_map(|team| team["rows"].as_array().cloned().unwrap_or_default()).collect();
    let active = value["active"].as_array().cloned().unwrap_or_default();
    if !active.is_empty() {
        rows.push(json!({"kind": "active-head", "id": "active", "depth": 0, "group": true, "open": true, "title": "Active"}));
        for (row, source) in active.into_iter().zip(&input.active) {
            let key = format!("{ACTIVE_KEY}{}", source.id);
            let mut row = row;
            row["kind"] = json!("active");
            row["session"] = json!(source.id);
            row["hover"] = json!(hover == Some(key.as_str()));
            row["id"] = json!(key);
            row["depth"] = json!(1);
            row["group"] = json!(false);
            row["team"] = json!(source.team.clone());
            rows.push(row);
        }
    }
    rows
}

/// #EXP-1616: the Pull requests tree's rows as nodes. A Row's `depth`
/// never binds (the tree guides are computed against its siblings), so the
/// host writes them into the document's `List{divided: false}` itself (the
/// other sidebar sections' list: no dividers; `depth` indents).
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

/// The document with `prs` as the `ide_prs_tree` List's rows.
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
    /// The corner's own mount (the footer, [`CORNER`]).
    footer: Entity<SurfaceView>,
    /// The footer's account.
    me: Rc<RefCell<Option<AccountFace>>>,
    footer_shown: Option<Value>,
    team_marks: Rc<RefCell<TeamMarks>>,
    /// The mounted document, for the Pull requests tree's rows.
    doc: uicore_gpui::inline::Document,
    /// The `prs` the document's tree holds.
    prs_built: Value,
    glyphs: Rc<RefCell<HashMap<String, Glyph>>>,
    hover: Rc<RefCell<Option<String>>>,
    pub(crate) input: IdeSidebarInput,
    shown: Option<Value>,
    /// #EXP-1741: the Computer button's state (the corner paints it).
    computer: Rc<RefCell<Option<crate::sidebar::ComputerState>>>,
    /// The Computer button changed since the document last repainted.
    computer_changed: bool,
    /// #EXP-1832: the folder whose name is an Input in place, and whether
    /// that Input still waits for the focus.
    editing: Rc<RefCell<Option<String>>>,
    focus_pending: bool,
    /// The dev team-menu capture opened its menu ([`Self::dev_open_team_menu`]).
    dev_team_menu_done: bool,
    /// #EXP-2027: the Chats row whose drag the tree's List shows.
    chat_drag: Option<String>,
    window: AnyWindowHandle,
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
            let editing = Rc::new(RefCell::new(None));
            let me = Rc::new(RefCell::new(None));
            let team_marks = Rc::new(RefCell::new(TeamMarks::new()));
            let host = Rc::new(RegionHost { team_marks: team_marks.clone(), computer: computer.clone(), me: me.clone(), glyphs: glyphs.clone(), hover: hover.clone(), editing: editing.clone(), window: window.window_handle(), region: cx.weak_entity(), nav });
            let source = uicore_gpui::inline::document(uicore_docs::doc("sidebar").expect("the factory sidebar"));
            let mut doc = source.clone();
            uicore_gpui::inline::without(&mut doc, PRUNED);
            uicore_gpui::inline::without(&mut doc, &[CORNER]);
            // #EXP-1741: the IDE's sidebar never folds (eDE's is the rail).
            uicore_gpui::inline::set_prop(&mut doc, "sidebar", "collapsed", json!(false));
            let theme = uicore_gpui::inline::theme(&eui_core::themes::default_theme());
            let view = uicore_gpui::inline::mount(&doc, &uicore_data::Feed::new(), theme.clone(), host.clone(), Mode::Dark, window, cx);
            // The footer: the corner alone, its row at the rail's bottom
            // inset (the rail pads under it).
            let mut corner = source;
            uicore_gpui::inline::without(&mut corner, FOOTER_PRUNED);
            uicore_gpui::inline::set_prop(&mut corner, "sidebar", "collapsed", json!(false));
            uicore_gpui::inline::set_style(&mut corner, "sidebar_root", "paddingTop", json!(0));
            uicore_gpui::inline::set_style(&mut corner, "sidebar_root", "paddingBottom", json!(0));
            uicore_gpui::inline::set_style(&mut corner, "sidebar_root", "height", Value::Null);
            let footer = uicore_gpui::inline::mount(&corner, &uicore_data::Feed::new(), theme, host, Mode::Dark, window, cx);
            Self {
                view,
                footer,
                me,
                footer_shown: None,
                team_marks,
                doc,
                prs_built: json!([]),
                glyphs,
                hover,
                input: IdeSidebarInput::default(),
                shown: None,
                computer,
                computer_changed: true,
                editing,
                focus_pending: false,
                dev_team_menu_done: false,
                chat_drag: None,
                window: window.window_handle(),
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
            let mut marks = TeamMarks::new();
            let out = gather(nav, &mut marks, cx);
            *self.team_marks.borrow_mut() = marks;
            out
        };
        // A glyph is host-painted: its state changes without the data. The
        // rail renders on every tick of a mark inside it (#LOGS-4): repaint
        // the document only when a glyph or the Computer button changed.
        let glyphs_changed = *self.glyphs.borrow() != glyphs;
        *self.glyphs.borrow_mut() = glyphs;
        self.input = input;
        self.push(cx);
        if glyphs_changed {
            self.view.update(cx, |_, cx| cx.notify());
        }
        self.repaint_computer(cx);
    }

    /// #LOGS-11: the rows stand; only the Computer button may have moved.
    pub(crate) fn repaint_computer(&mut self, cx: &mut gpui::Context<Self>) {
        if std::mem::take(&mut self.computer_changed) {
            self.footer.update(cx, |_, cx| cx.notify());
        }
    }

    /// The footer (the corner's mount), drawn by the rail under its Getting
    /// started entry.
    pub(crate) fn footer(&self) -> Entity<SurfaceView> {
        self.footer.clone()
    }

    /// The footer's account, from the rail's render (its avatar image lands
    /// after the row).
    pub(crate) fn set_me(&mut self, me: Option<AccountFace>, cx: &mut gpui::Context<Self>) {
        #[cfg(test)]
        if self.frozen {
            return;
        }
        if *self.me.borrow() != me {
            *self.me.borrow_mut() = me;
            self.footer.update(cx, |_, cx| cx.notify());
        }
        self.push_footer(cx);
    }

    /// The corner's data: the account's glyph and first name, the Computer
    /// button's glyph.
    fn push_footer(&mut self, cx: &mut gpui::Context<Self>) {
        let name = self.me.borrow().as_ref().map(|me| crate::user_avatar::first_name(&me.label).to_string()).unwrap_or_else(|| "Not signed in".to_string());
        let value = json!({"computer": "ide:computer", "me": "ide:me", "meName": name});
        if self.footer_shown.as_ref() == Some(&value) {
            return;
        }
        self.footer_shown = Some(value.clone());
        self.footer.update(cx, |view, cx| {
            let _ = view.set_data("/shell/rail", Some(json!(false)), cx);
            let _ = view.set_data("/shell/ide", Some(value), cx);
        });
    }

    /// A sample state, for the pictures.
    #[cfg(test)]
    pub(crate) fn show(&mut self, input: IdeSidebarInput, hover: Option<&str>, cx: &mut gpui::Context<Self>) {
        self.frozen = true;
        // A trunk that needs a person: the warning rides the Computer button.
        *self.computer.borrow_mut() = Some(crate::sidebar::ComputerState {
            files: true,
            source_control: true,
            tooltip: "Source Control: local commits".into(),
            badge: Some(crate::sidebar::RailBadge::Icon(crate::icons::registry::UI_WARNING, cx.theme().warning)),
            active: false,
        });
        *self.me.borrow_mut() = Some(AccountFace { user_id: "u1".into(), label: "Alex Carter".into(), image: None });
        self.push_footer(cx);
        self.input = input;
        *self.hover.borrow_mut() = hover.map(str::to_string);
        self.push(cx);
    }

    /// #EXP-1832: the document's view (the pictures drag its rows) and a
    /// folder's name in place.
    #[cfg(test)]
    pub(crate) fn surface(&self) -> Entity<SurfaceView> {
        self.view.clone()
    }

    #[cfg(test)]
    pub(crate) fn show_editing(&mut self, id: Option<&str>, cx: &mut gpui::Context<Self>) {
        self.edit_folder(id.map(str::to_string), cx);
    }

    /// DEV-ONLY (`EXP_DEV_TEAM_MENU=1`, the `sidebar-team-menu` capture):
    /// once the first team row is drawn, its `…` opens its menu (the row
    /// hovered, then the button pressed, as a pointer would).
    fn dev_open_team_menu(&mut self, cx: &mut gpui::Context<Self>) {
        if self.dev_team_menu_done || std::env::var("EXP_DEV_TEAM_MENU").map_or(true, |v| v.trim().is_empty() || v == "0") {
            return;
        }
        let Some(team) = self.input.teams.first().map(|team| team.id.clone()) else { return };
        self.dev_team_menu_done = true;
        *self.hover.borrow_mut() = Some(team.clone());
        let (view, handle) = (self.view.clone(), self.window);
        // Once the row and its `…` are laid out (a few frames; no frame
        // callback, a headless window never requests one).
        cx.spawn(async move |_, cx| {
            for _ in 0..40 {
                cx.background_executor().timer(std::time::Duration::from_millis(50)).await;
                let pressed = handle
                    .update(cx, |_, window, cx| {
                        view.update(cx, |view, cx| {
                            let trigger = view.placed_nodes().iter().find(|n| n.trigger_for.as_deref().is_some_and(|t| t.starts_with("ide_team_more") && t.ends_with(&team))).map(|n| n.index);
                            let button = trigger.or_else(|| view.placed_nodes().iter().find(|n| n.id.starts_with("ide_team_more_button") && n.id.ends_with(&team)).map(|n| n.index));
                            button.map(|index| view.press(index, window, cx)).is_some()
                        })
                    })
                    .unwrap_or(true);
                if pressed {
                    break;
                }
            }
        })
        .detach();
    }

    fn push(&mut self, cx: &mut gpui::Context<Self>) {
        self.dev_open_team_menu(cx);
        self.input.editing_folder = self.editing.borrow().clone();
        let value = shaped(&self.input, &self.glyphs.borrow(), self.hover.borrow().as_deref());
        if self.focus_pending {
            self.focus_editing(&value, cx);
        }

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

    /// #EXP-1832: edit folder `id`'s name in place (a fresh folder after a
    /// drop, or Rename); `None` ends the edit.
    fn edit_folder(&mut self, id: Option<String>, cx: &mut gpui::Context<Self>) {
        self.focus_pending = id.is_some();
        *self.editing.borrow_mut() = id;
        self.push(cx);
    }

    /// #EXP-1832: focus the name Input once its row is drawn (a fresh
    /// folder's row arrives with the synced layout).
    fn focus_editing(&mut self, value: &Value, cx: &mut gpui::Context<Self>) {
        let Some(id) = self.editing.borrow().clone() else {
            self.focus_pending = false;
            return;
        };
        let drawn = value["teams"].as_array().into_iter().flatten().flat_map(|team| team["rows"].as_array().into_iter().flatten()).any(|row| row["id"] == id.as_str() && row["editing"] == true);
        if !drawn {
            return;
        }
        self.focus_pending = false;
        // The Input's text field is its `field` part.
        let field = format!("ide_folder_name.{id}.field");
        let (view, handle) = (self.view.clone(), self.window);
        // Outside this update (the rail may be mid-render): the field exists
        // once the next frame has drawn the row.
        App::defer(cx, move |cx| {
            let _ = handle.update(cx, move |_, window, _| {
                window.on_next_frame(move |window, cx| {
                    view.update(cx, |view, cx| {
                        view.focus_field(&field, window, cx);
                    });
                });
            });
        });
    }

    /// #EXP-1832: fold or unfold folder `id` on this device.
    fn toggle_folder(&mut self, team_id: &str, id: &str, cx: &mut gpui::Context<Self>) {
        let key = sidebar_folder_open_key(team_id, id);
        let open = !crate::ui_prefs::sidebar_folder_open(&key);
        crate::ui_prefs::set_sidebar_folder_open(&key, open);
        self.input.closed.retain(|closed| closed != id);
        if !open {
            self.input.closed.push(id.to_string());
        }
        self.push(cx);
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
        self.input.teams.iter().flat_map(|t| &t.items).any(|i| i.kind == "agent" && i.id == id)
    }

    /// #EXP-2027 (sidebar-tree.json `drop`): a Chats row's drag over the
    /// window: the tree's List takes it as an outside row (key
    /// `active:<root>`, like an Active row), dropped only between its
    /// team's rows (a placed row of the same session steps aside, as on
    /// web), and draws its own drop.
    pub(crate) fn chat_drag_move(&mut self, drag: &crate::chats_view::ChatRowDrag, position: gpui::Point<gpui::Pixels>, cx: &mut gpui::Context<Self>) {
        let key = format!("{ACTIVE_KEY}{}", drag.id);
        let bounds = drag.bounds.get();
        let start = self.chat_drag.as_deref() != Some(drag.id.as_str());
        let keys = start.then(|| {
            let teams = self.shown.as_ref().and_then(|v| v["teams"].as_array().cloned()).unwrap_or_default();
            let rows = teams.into_iter().find(|team| team["id"] == drag.team_id.as_str()).and_then(|team| team["rows"].as_array().cloned()).unwrap_or_default();
            rows.iter().filter_map(|row| row["id"].as_str()).filter(|id| *id != drag.id).map(str::to_string).collect::<Vec<_>>()
        });
        let live = self.view.update(cx, |view, cx| {
            if let Some(keys) = keys {
                view.list_drag_external(TREE_LIST, &key, bounds, Some(keys), ListExternalPhase::Start, position, cx);
            }
            view.list_drag_external(TREE_LIST, &key, bounds, None, ListExternalPhase::Move, position, cx)
        });
        self.chat_drag = live.then(|| drag.id.clone());
        if live && start {
            cx.set_global(ChatDragTarget(cx.weak_entity()));
        }
    }

    /// #EXP-2027: the release anywhere drops a Chats row's drag (as on
    /// web); an Esc'd drag (no gpui drag left) drops nothing.
    pub(crate) fn chat_drag_end(&mut self, position: gpui::Point<gpui::Pixels>, dropped: bool, cx: &mut gpui::Context<Self>) {
        if self.chat_drag.take().is_none() {
            return;
        }
        let phase = if dropped { ListExternalPhase::End } else { ListExternalPhase::Cancel };
        self.view.update(cx, |view, cx| view.list_drag_external(TREE_LIST, "", Default::default(), None, phase, position, cx));
    }

    /// #EXP-2027: an Active row's chat (its tree key `active:<root>`).
    pub(crate) fn active_root(&self, id: &str) -> Option<String> {
        let root = id.strip_prefix(ACTIVE_KEY)?;
        self.input.active.iter().any(|a| a.id == root).then(|| root.to_string())
    }
}

/// #EXP-2027: the sidebar showing a Chats row's drag (Esc on the Chats
/// page drops it there: [`cancel_chat_drag`]).
struct ChatDragTarget(gpui::WeakEntity<SidebarRegion>);

impl gpui::Global for ChatDragTarget {}

/// #EXP-2027: Esc dropped a Chats row's drag: its sidebar draws no drop.
pub(crate) fn cancel_chat_drag(cx: &mut App) {
    let Some(target) = cx.try_global::<ChatDragTarget>().map(|t| t.0.clone()) else { return };
    let _ = target.update(cx, |region, cx| region.chat_drag_end(Default::default(), false, cx));
}

/// The sidebar document's tree List (teams + Active, apps/ede/ui/sidebar.json).
const TREE_LIST: &str = "ide_tree_list";

impl Render for SidebarRegion {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "SidebarRegion").entered();
        // #EXP-1832: Esc in a folder's name Input keeps the name it had.
        div()
            .size_full()
            .on_key_down(cx.listener(|region, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && region.editing.borrow().is_some() {
                    region.edit_folder(None, cx);
                    cx.stop_propagation();
                }
            }))
            .on_drag_move::<crate::chats_view::ChatRowDrag>(cx.listener(|region, event: &gpui::DragMoveEvent<crate::chats_view::ChatRowDrag>, _, cx| {
                let drag = event.drag(cx).clone();
                region.chat_drag_move(&drag, event.event.position, cx);
            }))
            .capture_any_mouse_up(cx.listener(|region, event: &gpui::MouseUpEvent, _, cx| region.chat_drag_end(event.position, cx.has_active_drag(), cx)))
            .on_mouse_up_out(gpui::MouseButton::Left, cx.listener(|region, event: &gpui::MouseUpEvent, _, cx| region.chat_drag_end(event.position, cx.has_active_drag(), cx)))
            .child(uicore_gpui::inline::region(&self.view))
    }
}

struct RegionHost {
    team_marks: Rc<RefCell<TeamMarks>>,
    computer: Rc<RefCell<Option<crate::sidebar::ComputerState>>>,
    me: Rc<RefCell<Option<AccountFace>>>,
    glyphs: Rc<RefCell<HashMap<String, Glyph>>>,
    hover: Rc<RefCell<Option<String>>>,
    editing: Rc<RefCell<Option<String>>>,
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
                "chats" => Screen::Chats,
                _ => Screen::Drafts,
            };
            navigate(window, cx, screen);
        }
        // #EXP-2027: a device row opens the device's page.
        "device" => navigate(window, cx, Screen::Device { device_id: id }),
        "agent" => navigate(window, cx, Screen::Agent { agent_id: id }),
        "app" => navigate(window, cx, Screen::AppRun { app: id }),
        "board" => {
            navigation::set_active_board(window, cx, id.clone());
            navigate(window, cx, Screen::BoardIssues { board_id: id });
        }
        // #EXP-2027: a placed session opens its chat.
        "session" => navigate(window, cx, navigation::session_screen_for(&id, cx)),
        "active" => navigation::navigate_from_live_rail(window, cx, Screen::Chat { session_id: id }),
        // #EXP-1616: a Pull requests row opens its repository's page (in its
        // team's settings).
        "repo" => {
            let team = Store::global(cx).collections().pull_requests.read(cx).iter().find(|p| p.repository_id.as_deref() == Some(id.as_str())).and_then(|p| p.team_id.clone());
            navigation::open_settings(window, cx, team.or_else(|| navigation::fallback_team_id(cx)));
            crate::sidebar::select_settings_section(window, cx, SettingsSection::Repository(id));
        }
        "settings" => navigation::open_settings(window, cx, None),
        _ => {}
    }
}

fn open_gear(kind: &str, id: String, window: &mut Window, cx: &mut App) {
    let collections = Store::global(cx).collections().clone();
    match kind {
        // #EXP-2027: an agent's or board's settings open in its team's
        // settings.
        "agent" => {
            let team = collections.agents.read(cx).get(&id).and_then(|agent| agent.team_id.clone());
            navigation::open_settings(window, cx, team);
            crate::sidebar::select_settings_section(window, cx, SettingsSection::Agent(id));
        }
        "board" => {
            let team = collections.boards.read(cx).get(&id).map(|board| board.team_id.clone());
            navigation::open_settings(window, cx, team);
            crate::sidebar::select_settings_section(window, cx, SettingsSection::Board(id));
        }
        "app" => crate::app_screen::open_app_tab(window, cx, id, domain::apps::EditorTab::Settings, false),
        "device" => navigate(window, cx, Screen::DeviceSettings { device_id: id }),
        _ => {}
    }
}

/// #EXP-2027: a team menu's pick: New › Board | Agent | App open their
/// creation pages, New › Session Home with the team as its pick, New folder
/// appends a folder and names it in place, Settings the team's settings.
fn team_menu(team_id: String, value: &str, region: gpui::WeakEntity<SidebarRegion>, window: &mut Window, cx: &mut App) {
    match value {
        "board" => navigate(window, cx, Screen::NewItem { team_id, kind: navigation::NewItemKind::Board }),
        "agent" => navigate(window, cx, Screen::NewItem { team_id, kind: navigation::NewItemKind::Agent }),
        "app" => navigate(window, cx, Screen::NewItem { team_id, kind: navigation::NewItemKind::App }),
        "session" => navigation::open_home_in_team(window, cx, team_id),
        "settings" => navigation::open_settings(window, cx, Some(team_id)),
        "new-folder" => {
            let entries = layout_entries(&team_id, cx);
            let writes = new_folder_writes(&entries, &uuid::Uuid::new_v4().to_string());
            let created = writes.created_folder_id.clone();
            crate::ui_prefs::set_sidebar_folder_open(&domain::sidebar_tree::sidebar_team_open_key(&team_id), true);
            save_layout(team_id, writes, cx);
            let _ = region.update(cx, |region, cx| region.edit_folder(created, cx));
        }
        _ => {}
    }
}

/// #EXP-2027 (sidebar-tree.json New folder): an empty folder named `New
/// folder` at the END of the team's placed nodes.
fn new_folder_writes(entries: &[SidebarEntry], id: &str) -> SidebarWrites {
    let folders: Vec<&str> = entries.iter().filter(|e| e.kind == SidebarEntryKind::Folder).map(|e| e.id.as_str()).collect();
    let last = entries
        .iter()
        .filter(|e| e.kind == SidebarEntryKind::Folder || e.parent_id.as_deref().is_none_or(|p| !folders.contains(&p)))
        .map(|e| e.sort_order)
        .fold(0.0_f64, f64::max);
    SidebarWrites {
        upserts: vec![domain::sidebar_folders::SidebarEntryWrite { id: Some(id.to_string()), kind: SidebarEntryKind::Folder, target_id: None, parent_id: None, name: Some(domain::sidebar_folders::SIDEBAR_FOLDER_DEFAULT_NAME.to_string()), sort_order: last.floor() + 1.0 }],
        deletes: Vec::new(),
        created_folder_id: Some(id.to_string()),
    }
}

/// #EXP-2027: the team a tree row belongs to: a team row's own id, an
/// item of its list, one of its folders.
fn row_team<'a>(input: &'a IdeSidebarInput, id: &str) -> Option<&'a str> {
    input.teams.iter().find(|team| team.id == id || team.items.iter().any(|item| item.id == id) || team.layout.iter().any(|entry| entry.kind == "folder" && entry.id == id)).map(|team| team.id.as_str())
}

/// #EXP-2027: one drop on the tree's List as the team and its writes; `None`
/// = refused (another team's target, an Active row onto an Active row, a
/// session of another team, a child). A drop INTO a team row lands after
/// its last top-level row.
fn tree_move(
    input: &IdeSidebarInput,
    mv: &SidebarMove,
    entries: impl Fn(&str) -> Vec<SidebarEntry>,
    session: impl Fn(&str) -> Option<domain::rows::SessionRow>,
    new_folder: &str,
) -> Option<(String, SidebarWrites)> {
    let target = mv.before.as_deref().or(mv.after.as_deref()).or(mv.into.as_deref())?;
    let team_id = row_team(input, target)?.to_string();
    let team = input.teams.iter().find(|t| t.id == team_id)?;
    let items: Vec<String> = team.items.iter().map(|i| i.id.clone()).collect();
    let entries = entries(&team_id);
    let mut mv = mv.clone();
    if mv.into.as_deref() == Some(team_id.as_str()) {
        let tree = domain::sidebar_folders::sidebar_tree(&items, &entries);
        mv.into = None;
        mv.after = Some(tree.last()?.id().to_string());
    } else if target == team_id {
        return None;
    }
    match mv.id.strip_prefix(ACTIVE_KEY) {
        Some(root) => {
            let row = session(root)?;
            mv.id = root.to_string();
            let source = domain::sidebar_tree::SidebarSessionInput { id: row.id.clone(), team_id: row.team_id.clone().unwrap_or_default(), parent_id: row.parent_id.clone() };
            Some((team_id.clone(), domain::sidebar_tree::sidebar_drop_session(&team_id, &items, &entries, &source, &mv, new_folder)?))
        }
        None => {
            if row_team(input, &mv.id)? != team_id {
                return None;
            }
            Some((team_id.clone(), sidebar_move(&items, &entries, &mv, new_folder)?))
        }
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
        // The footer's account: the profile image or the initials on the
        // user's hue; signed out, the generic person.
        if name == "ide:me" {
            return Some(match self.me.borrow().as_ref() {
                Some(me) => crate::user_avatar::avatar_element(&me.user_id, &me.label, me.image.clone(), gpui_component::Size::Small),
                None => gpui_component::avatar::Avatar::new().with_size(gpui_component::Size::Small).into_any_element(),
            });
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
            Glyph::Team(team, _) => {
                let image = self.team_marks.borrow().get(team).cloned().flatten();
                crate::user_avatar::team_avatar_of(image, size)
            }
            Glyph::Online => crate::sidebar::rail_dot(cx.theme().success).into_any_element(),
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
        let (kind, id, team) = (text("kind"), text("id"), text("team"));
        let value = event.payload.as_ref().and_then(|p| p["value"].as_str()).map(str::to_string);
        let region = self.region.clone();
        match event.name.as_str() {
            "ide_open" => self.in_window(cx, move |window, cx| open_row(&kind, id, window, cx)),
            "ide_gear" => self.in_window(cx, move |window, cx| open_gear(&kind, id, window, cx)),
            // #EXP-2027: the Devices row's hover + adds a device.
            "ide_add" if kind == "device" => self.in_window(cx, crate::machines::open_add_server_dialog),
            "ide_devices" => cx.defer(move |cx| {
                let key = domain::sidebar_tree::SIDEBAR_DEVICES_OPEN_KEY;
                crate::ui_prefs::set_sidebar_folder_open(key, !crate::ui_prefs::sidebar_folder_open(key));
                let _ = region.update(cx, |region, cx| {
                    region.input.devices_open = crate::ui_prefs::sidebar_folder_open(key);
                    region.push(cx);
                });
            }),
            // #EXP-2027: a team row folds; its menu creates, adds a folder
            // or opens the team's settings.
            "ide_team" => match (text("action").as_str(), value) {
                ("fold", _) => cx.defer(move |cx| {
                    let key = domain::sidebar_tree::sidebar_team_open_key(&id);
                    let open = !crate::ui_prefs::sidebar_folder_open(&key);
                    crate::ui_prefs::set_sidebar_folder_open(&key, open);
                    let _ = region.update(cx, |region, cx| {
                        if let Some(team) = region.input.teams.iter_mut().find(|t| t.id == id) {
                            team.open = open;
                        }
                        region.push(cx);
                    });
                }),
                (_, Some(value)) => self.in_window(cx, move |window, cx| team_menu(id, &value, region, window, cx)),
                _ => {}
            },
            // The footer's account menu: web's, item for item.
            "ide_account" => {
                let action = value.unwrap_or_default();
                self.in_window(cx, move |window, cx| match action.as_str() {
                    "whats_new" => window.dispatch_action(Box::new(crate::actions::OpenWhatsNew), cx),
                    "about" => window.dispatch_action(Box::new(crate::actions::OpenAbout), cx),
                    "sign_out" => window.dispatch_action(Box::new(crate::actions::SignOut), cx),
                    _ => {}
                });
            }
            // #EXP-2027: a placed session's menu: Remove from sidebar.
            "ide_session" if value.as_deref() == Some("remove") => remove_from_sidebar(id, cx),
            "ide_switcher" => {
                let action = value.unwrap_or_default();
                self.in_window(cx, move |window, cx| match action.as_str() {
                    "new_team" => window.dispatch_action(Box::new(crate::actions::CreateTeam), cx),
                    "join_team" => window.dispatch_action(Box::new(crate::actions::JoinTeam), cx),
                    _ => {}
                });
            }
            // #EXP-1832 / #EXP-2027: a row dropped in the tree's List: a
            // team's row moves within its team (sidebarMove), an Active row
            // dropped into a team joins it (sidebarDropSession: another
            // team's session is refused); anything across teams is refused.
            "ide_move" => {
                let Some(mv) = event.payload.as_ref().and_then(SidebarMove::from_context).or_else(|| SidebarMove::from_context(&event.context)) else { return };
                cx.defer(move |cx| {
                    let Some(input) = region.read_with(cx, |region, _| region.input.clone()).ok() else { return };
                    let Some((team, writes)) = tree_move(&input, &mv, |team| layout_entries(team, cx), |id| crate::session_data::session(id, cx), &uuid::Uuid::new_v4().to_string()) else { return };
                    let created = writes.created_folder_id.clone();
                    save_layout(team, writes, cx);
                    if created.is_some() {
                        let _ = region.update(cx, |region, cx| region.edit_folder(created, cx));
                    }
                });
            }
            // #EXP-1832: a folder row: its press folds it, its menu renames
            // or ungroups it, its name Input saves (Enter) or keeps (leaving).
            "ide_folder" => {
                let action = event.context["action"].as_str().map(str::to_string).or_else(|| value.clone()).unwrap_or_default();
                let editing = self.editing.clone();
                cx.defer(move |cx| match action.as_str() {
                    "fold" => {
                        let _ = region.update(cx, |region, cx| region.toggle_folder(&team, &id, cx));
                    }
                    "rename" => {
                        let _ = region.update(cx, |region, cx| region.edit_folder(Some(id), cx));
                    }
                    "save" => {
                        if editing.borrow().as_deref() != Some(id.as_str()) {
                            return;
                        }
                        rename_folder(id, sidebar_folder_name(value.as_deref().unwrap_or_default()), cx);
                        let _ = region.update(cx, |region, cx| region.edit_folder(None, cx));
                    }
                    // Leaving the Input saves what it holds (web's blur).
                    "leave" => {
                        if editing.borrow().as_deref() != Some(id.as_str()) {
                            return;
                        }
                        let field = format!("ide_folder_name.{id}.field");
                        let Some(value) = region.read_with(cx, |region, cx| region.view.read(cx).field_text(&field, cx)).ok().flatten() else { return };
                        rename_folder(id, sidebar_folder_name(&value), cx);
                        let _ = region.update(cx, |region, cx| region.edit_folder(None, cx));
                    }
                    "ungroup" => {
                        let Some(items) = region.read_with(cx, |region, _| team_items(&team, &region.input)).ok() else { return };
                        if let Some(writes) = sidebar_ungroup(&items, &layout_entries(&team, cx), &id) {
                            save_layout(team, writes, cx);
                        }
                    }
                    _ => {}
                });
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
            // #EXP-1814: the +N menu is taller than the 44px band: it is
            // placed against the window (below the top band, on screen).
            let view = uicore_gpui::inline::mount_lifted(&doc, &uicore_data::Feed::new(), theme, host, Mode::Dark, window, cx);
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

    /// The sample strip's +N menu open on its `count` last tabs, as the
    /// host's `more` answer writes it.
    #[cfg(test)]
    pub(crate) fn show_more(&mut self, titles: &[&str], count: usize, cx: &mut gpui::Context<Self>) {
        let inputs: Vec<IdeTabInput> = titles.iter().enumerate().map(|(i, title)| IdeTabInput { key: format!("t{i}"), title: title.to_string(), icon: "nav-chats".into() }).collect();
        let mut value = serde_json::to_value(uicore_model::ide::ide_tabs(&inputs, Some("t1"))).unwrap();
        let tabs = value["tabs"].as_array().cloned().unwrap_or_default();
        value["more"] = Value::Array(tabs[tabs.len().saturating_sub(count)..].to_vec());
        value["moreOpen"] = json!(true);
        self.apply(&value, false, cx);
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

    /// The strip's pressable areas as laid out (region coordinates): each
    /// pressable node or overlay trigger with its x span, and where the
    /// controls end.
    #[cfg(test)]
    pub(crate) fn pressable_spans(&self, cx: &App) -> (Vec<(String, f32, f32)>, f32) {
        let view = self.view.read(cx);
        let spans = view
            .placed_nodes()
            .iter()
            .filter(|n| n.pressable || n.trigger_for.is_some())
            .filter_map(|n| view.frame(n.index).filter(|f| f.w > 0. && f.h > 0.).map(|f| (n.id.clone(), f.x, f.x + f.w)))
            .collect();
        (spans, self.strip_right(cx))
    }

    /// The open +N menu's frame (region coordinates) and the +N's.
    #[cfg(test)]
    pub(crate) fn more_menu_frame(&self, cx: &App) -> (Option<eui_core::surface::Frame>, Option<eui_core::surface::Frame>) {
        let view = self.view.read(cx);
        let menu = view.layers().iter().find(|l| l.owner == "ide_tabs_more").and_then(|l| view.frame(l.root));
        let more = view.index_of("ide_tabs.more").and_then(|i| view.frame(i));
        (menu, more)
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

    /// #EXP-2027: the brand (no team is active: the team switcher is gone;
    /// its menu keeps the Local desktops), the inbox, search and New issue.
    pub(crate) fn refresh(&mut self, nav: &Entity<Navigation>, _needs_you: &std::collections::BTreeMap<String, usize>, cx: &mut gpui::Context<Self>) {
        let locals = crate::local_desktops::entries(cx);
        let on_local = locals.iter().any(|(_, _, current)| *current);
        let sections = vec![json!({
            "kind": "local", "id": "local", "label": "Local desktops",
            "entries": locals.iter().map(|(id, name, current)| json!({"kind": "local", "id": id, "name": name, "current": current, "dotIcon": Value::Null})).collect::<Vec<_>>(),
        })];
        let name = locals.iter().find(|(_, _, current)| *current).map(|(_, name, _)| name.clone()).unwrap_or_else(|| "Exponential".to_string());
        let value = json!({
            "switcher": {"current": {"name": name}, "sections": sections},
            "ide": {"teamMark": "brand-exponential", "chevron": if locals.is_empty() { Value::Null } else { json!("nav-team-switcher") }, "inbox": "ide:inbox", "noBoard": active_board_id(nav, cx).is_none(), "signedIn": false, "onLocal": on_local},
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
            devices_open: true,
            teams: vec![IdeTeamInput { id: "t1".into(), name: "Acme".into(), mark: "ide:team:t1".into(), open: true, owner: true, needs_you: 0, items: vec![IdeItemInput { kind: "board".into(), id: "b1".into(), name: "Web".into(), icon: "ide:board:b1".into(), ..Default::default() }], layout: Vec::new() }],
            ..Default::default()
        };
        let value = shaped(&input, &HashMap::new(), Some("b1"));
        // Home leads, Chats carries its count.
        assert_eq!(value["nav"][1]["dotIcon"], "ide:count:attention:2");
        let counted = IdeSidebarInput { needs_you: 5, drafts: 120, ..input.clone() };
        let counted = shaped(&counted, &HashMap::new(), None);
        assert_eq!(counted["nav"][1]["dotIcon"], "ide:count:attention:5");
        assert_eq!(counted["nav"][3]["dotIcon"], "ide:count:muted:99+");
        // #EXP-2027: every team row knows its team; the hovered row is marked.
        assert_eq!(value["teams"][0]["rows"][1]["hover"], true);
        assert_eq!(value["teams"][0]["rows"][1]["team"], "t1");
    }

    /// #EXP-2027: New folder lands after the team's placed nodes.
    #[test]
    fn a_new_folder_lands_after_the_placed_nodes() {
        let entry = |id: &str, kind: SidebarEntryKind, parent: Option<&str>, sort_order: f64| SidebarEntry { id: id.into(), kind, target_id: (kind == SidebarEntryKind::Item).then(|| format!("t-{id}")), parent_id: parent.map(Into::into), name: None, sort_order };
        let entries = vec![entry("f1", SidebarEntryKind::Folder, None, 2.), entry("e1", SidebarEntryKind::Item, Some("f1"), 7.), entry("e2", SidebarEntryKind::Item, None, 3.)];
        let writes = new_folder_writes(&entries, "fn");
        assert_eq!(writes.created_folder_id.as_deref(), Some("fn"));
        assert_eq!(writes.upserts[0].sort_order, 4.);
        assert_eq!(writes.upserts[0].name.as_deref(), Some("New folder"));
        assert_eq!(new_folder_writes(&[], "fn").upserts[0].sort_order, 1.);
    }

    /// #EXP-2027: one List for the teams and Active: an Active row joins
    /// the team it is dropped into (its own team only); a row never moves
    /// to another team.
    #[test]
    fn a_tree_drop_stays_in_its_team() {
        let item = |kind: &str, id: &str| IdeItemInput { kind: kind.into(), id: id.into(), name: id.into(), icon: String::new(), ..Default::default() };
        let team = |id: &str, items: Vec<IdeItemInput>| IdeTeamInput { id: id.into(), name: id.into(), mark: String::new(), open: true, owner: true, needs_you: 0, items, layout: Vec::new() };
        let input = IdeSidebarInput { teams: vec![team("t1", vec![item("board", "b1"), item("agent", "a1")]), team("t2", vec![item("board", "b9")])], ..Default::default() };
        let session = |id: &str| -> Option<domain::rows::SessionRow> {
            let team = if id == "s9" { "t2" } else { "t1" };
            let parent = (id == "s5").then_some("s1");
            serde_json::from_value(json!({"id": id, "team_id": team, "parent_id": parent, "status": "idle"})).ok()
        };
        let mv = |id: &str, after: Option<&str>, into: Option<&str>| SidebarMove { id: id.into(), before: None, after: after.map(Into::into), into: into.map(Into::into) };
        // An Active row of t1 after a t1 row: appended, then moved there.
        let (to, writes) = tree_move(&input, &mv("active:s1", Some("b1"), None), |_| Vec::new(), session, "f").expect("the drop");
        assert_eq!(to, "t1");
        assert!(writes.upserts.iter().any(|w| w.target_id.as_deref() == Some("s1") && w.sort_order == 2.));
        // Into the team row: after its last row.
        let (_, writes) = tree_move(&input, &mv("active:s1", None, Some("t1")), |_| Vec::new(), session, "f").expect("into the team");
        assert!(writes.upserts.iter().any(|w| w.target_id.as_deref() == Some("s1") && w.sort_order == 3.));
        // A Chats row of t1 (the same `active:` key): into another team's
        // row, a child session: refused.
        assert!(tree_move(&input, &mv("active:s1", None, Some("t2")), |_| Vec::new(), session, "f").is_none());
        assert!(tree_move(&input, &mv("active:s5", Some("b1"), None), |_| Vec::new(), session, "f").is_none());
        // Another team's session, a row across teams, an Active row onto Active.
        assert!(tree_move(&input, &mv("active:s9", Some("b1"), None), |_| Vec::new(), session, "f").is_none());
        assert!(tree_move(&input, &mv("b9", Some("b1"), None), |_| Vec::new(), session, "f").is_none());
        assert!(tree_move(&input, &mv("active:s1", Some("active:s9"), None), |_| Vec::new(), session, "f").is_none());
        // Inside a team: a plain move.
        assert!(tree_move(&input, &mv("a1", None, None).clone(), |_| Vec::new(), session, "f").is_none());
        assert!(tree_move(&input, &SidebarMove { id: "a1".into(), before: Some("b1".into()), after: None, into: None }, |_| Vec::new(), session, "f").is_some());
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
            repos: vec![IdeRepoInput { id: "r1".into(), name: "engine".into(), prs: vec![IdePrInput { id: "p1".into(), identifier: "#1".into(), title: "Sync".into(), subtitle: "sync → main".into(), depth: 0, session: None }] }],
            ..Default::default()
        };
        let value = shaped(&input, &HashMap::new(), None);
        let tree = serde_json::to_value(with_pr_tree(&doc, &value["prs"]).expect("the tree")).unwrap();
        fn find<'a>(node: &'a Value, id: &str) -> Option<&'a Value> {
            if node["id"] == id {
                return Some(node);
            }
            node["children"].as_array().into_iter().flatten().chain(node["slots"].as_object().into_iter().flat_map(|slots| slots.values())).find_map(|c| find(c, id))
        }
        let section = find(&tree, "ide_prs_tree").expect("the list");
        assert_eq!(section["component"], "List");
        assert_eq!(section["props"]["divided"], false);
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
        assert_eq!(row_key("ide_item_row.b1"), Some("b1"));
        // A row inside a folder, a folder, a team, a device.
        assert_eq!(row_key("ide_item_child_row.b1"), Some("b1"));
        assert_eq!(row_key("ide_folder_row.f1"), Some("f1"));
        assert_eq!(row_key("ide_team_row.t1"), Some("t1"));
        assert_eq!(row_key("ide_device_row.d1"), Some("d1"));
        assert_eq!(row_key("ide_item_gear.b1"), None);
        assert_eq!(row_key("ide_teams_add"), None);
    }

    #[test]
    fn the_document_prunes_only_nodes_it_has() {
        let doc = uicore_docs::doc("sidebar").expect("the sidebar");
        for id in PRUNED.iter().chain(HEADER_PRUNED).chain(FOOTER_PRUNED).chain([&CORNER]) {
            assert!(doc.source.contains(&format!("\"id\": \"{id}\"")), "{id}");
        }
    }
}
