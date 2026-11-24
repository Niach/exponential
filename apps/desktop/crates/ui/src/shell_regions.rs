//! #EXP-1725: the IDE's regions on uicore. The sidebar's rows are the shared
//! document (`uicore_docs`, `apps/ede/ui/sidebar.express`) mounted in place: this
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

/// #EXP-2124: the Getting started row, under the scrolling rows (the main
/// mount keeps it; the header and footer mounts drop it).
const GETTING_STARTED: &str = "ide_getting_started";

/// The corner: the IDE's footer (the account, the Computer button and the
/// personal settings), mounted on its own under the rail's Getting started
/// entry so the rows above it scroll and the footer stays at the bottom.
const CORNER: &str = "corner";

/// What the footer's mount leaves out: everything but the corner.
const FOOTER_PRUNED: &[&str] = &["header", "switcher_menu", "chats", "ide_nav", "scroll", GETTING_STARTED];

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

/// #EXP-2124: what the row menus read beside the projection.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RowExtras {
    /// The Active roots the viewer may delete (chats.json `menuCases`).
    deletable: std::collections::HashSet<String>,
    /// The device whose name is an Input in place (Rename).
    editing_device: Option<String>,
    /// The Getting started row shows (its checklist is not done).
    getting_started: bool,
}

/// Every team's rows as the fixture's input, with the glyphs they name.
fn gather(nav: &Entity<Navigation>, marks: &mut TeamMarks, cx: &mut App) -> (IdeSidebarInput, HashMap<String, Glyph>, RowExtras) {
    let mut extras = RowExtras { getting_started: crate::getting_started::getting_started_visible(nav, cx), ..Default::default() };
    let mut glyphs = HashMap::new();
    glyphs.insert("ide:dot:green".to_string(), Glyph::Online);
    let screen = resolved_screen(nav, cx);
    let open_draft = match &screen {
        Some(Screen::IssueDraft { draft_id, .. }) => Some(draft_id.clone()),
        _ => None,
    };
    let board = active_board_id(nav, cx);
    let mut input = IdeSidebarInput { screen: screen_key(screen.as_ref(), board.as_deref(), cx), devices_open: crate::ui_prefs::sidebar_folder_open(domain::sidebar_tree::SIDEBAR_DEVICES_OPEN_KEY), ..Default::default() };
    let Some(store) = Store::try_global(cx) else { return (input, glyphs, extras) };
    let collections = store.collections().clone();
    // #EXP-2027: every member team (the teams shape never syncs the
    // personal team: `getMemberTeamIds`).
    let all_teams = collections.teams_sorted(cx);
    let shown: Vec<String> = domain::sidebar_tree::sidebar_teams(
        &all_teams.iter().map(|t| domain::sidebar_tree::SidebarTeamInput { id: t.id.clone(), name: t.name.clone(), kind: "team".into() }).collect::<Vec<_>>(),
    );
    input.devices = gather_devices(&shown, &mut glyphs, cx);
    // #EXP-2124: the viewer's Assistant, the first row (its conversation's
    // badge, the FAB's own read).
    input.assistant = crate::session_data::my_assistant_anywhere(None, cx).map(|assistant| {
        let badge = crate::session_data::my_conversation(&assistant.id, cx)
            .map(|conversation| crate::session_data::conversation_badge(&conversation, cx))
            .unwrap_or(domain::nav_badges::row_badge(0, false));
        uicore_model::ide::IdeAssistantInput {
            name: crate::agent_screen::agent_def_name(&assistant),
            id: assistant.id,
            unread: badge.count,
            needs_you: badge.tone == domain::nav_badges::CountBadgeTone::Attention,
        }
    });
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
    // #EXP-2082: the rows once (the Assistant's cards sit in every team's list).
    input.needs_you = crate::needs_you::total_count(cx);
    let sessions: Vec<domain::rows::SessionRow> = collections.sessions.read(cx).iter().filter(|s| s.parent_id.is_none()).cloned().collect();
    // ONE pass for every team (was per team, and every placed session's tree
    // walked all synced sessions: O(placed × sessions) per refresh).
    let session_inputs: Vec<domain::sidebar_tree::SidebarSessionInput> =
        sessions.iter().filter_map(|s| Some(domain::sidebar_tree::SidebarSessionInput { id: s.id.clone(), team_id: s.team_id.clone()?, parent_id: None })).collect();
    let root_by_id: HashMap<&str, &domain::rows::SessionRow> = sessions.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut trees: HashMap<String, Vec<String>> = HashMap::new();
    for s in collections.sessions.read(cx).iter() {
        trees.entry(s.root_id_or_self().to_string()).or_default().push(s.id.clone());
    }
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
            &session_inputs,
            &entries,
        );
        for item in placed {
            let Some(session) = root_by_id.get(item.id.as_str()).copied() else { continue };
            let icon = format!("ide:run:{}", session.id);
            let agent = crate::run_rows::run_agent(session.cli.as_deref());
            let state = crate::sidebar::running_row_state(crate::session_data::session_is_working(session), false);
            glyphs.insert(icon.clone(), Glyph::Run(agent, state));
            let tree: Vec<String> = trees.get(&session.id).cloned().unwrap_or_default();
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
            if crate::chat_actions::chat_entries(&row.root_id, cx).contains(&domain::chats::ChatMenuEntry::Delete) {
                extras.deletable.insert(row.root_id.clone());
            }

            input.active.push(IdeActiveInput { id: row.root_id, title: row.title, icon, session_ids: row.session_ids, unread: row.unread, team_mark, team: Some(team.id.clone()) });
        }
    }
    (input, glyphs, extras)
}

/// The fixture's output as the document binds it: counts as glyph names, the
/// row under the pointer marked, the active rows' device glyphs, every team
/// row's `team`.
fn shaped(input: &IdeSidebarInput, glyphs: &HashMap<String, Glyph>, hover: Option<&str>, extras: &RowExtras) -> Value {
    let mut value = serde_json::to_value(ide_sidebar(input)).expect("the sidebar serializes");
    let mark = |rows: &mut Value| {
        for row in rows.as_array_mut().into_iter().flatten() {
            row["dotIcon"] = count_icon(row["count"].as_str(), row["tone"].as_str().unwrap_or("muted"));
            row["hover"] = json!(hover == row["id"].as_str());
        }
    };
    mark(&mut value["nav"]);
    mark(&mut value["devices"]);
    // #EXP-2124: the Assistant wears the brand mark in its disc.
    for row in value["nav"].as_array_mut().into_iter().flatten().filter(|row| row["id"] == "assistant") {
        row["icon"] = json!(ASSISTANT_MARK);
    }
    // #EXP-2124: a device row's menu (Rename on the viewer's own: no team
    // mark) and its name in place.
    for row in value["devices"].as_array_mut().into_iter().flatten().filter(|row| row["kind"] == "device") {
        let mine = input.devices.iter().any(|d| d.id == row["id"].as_str().unwrap_or_default() && d.team_mark.is_none());
        row["menu"] = json!(if mine { "device-mine" } else { "device-other" });
        row["editing"] = json!(extras.editing_device.as_deref() == row["id"].as_str());
    }
    for team in value["teams"].as_array_mut().into_iter().flatten() {
        let id = team["id"].clone();
        let owner = input.teams.iter().find(|t| t.id == id.as_str().unwrap_or_default()).is_some_and(|t| t.owner);
        mark(&mut team["rows"]);
        let folders: Vec<(String, String)> = team["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| row["kind"] == "folder")
            .map(|row| (row["id"].as_str().unwrap_or_default().to_string(), row["title"].as_str().unwrap_or_default().to_string()))
            .collect();
        // The folder the rows that follow sit in (depth 2 under it).
        let mut folder: Option<String> = None;
        for row in team["rows"].as_array_mut().into_iter().flatten() {
            row["team"] = id.clone();
            if row["kind"] == "folder" {
                folder = row["id"].as_str().map(str::to_string);
            }
            if row["kind"] != "item" {
                continue;
            }
            let inside = (row["depth"] == 2).then(|| folder.clone()).flatten();
            row["menu"] = json!(item_menu_variant(row["item"].as_str().unwrap_or_default(), owner));
            row["moveItems"] = Value::Array(move_items(&folders, inside.as_deref()));
        }
    }
    value["computer"] = json!("ide:computer");
    value["signedIn"] = json!(true);
    for row in value["active"].as_array_mut().into_iter().flatten() {
        let device = format!("ide:device:{}", row["id"].as_str().unwrap_or_default());
        row["deviceIcon"] = if glyphs.contains_key(&device) { json!(device) } else { Value::Null };
    }
    value["tree"] = Value::Array(tree_rows(input, &value, hover, extras));
    value["gettingStarted"] = json!(extras.getting_started);
    value
}

/// #EXP-2124: the Assistant row's glyph (the IDE paints the brand mark in
/// its muted disc, web `AssistantMark`).
const ASSISTANT_MARK: &str = "ide:assistant";

/// #EXP-2124: which of the document's item menus a row opens (the table
/// rows that depend on the viewer: a board's and an agent's Settings, a
/// board's Archive are the owners').
fn item_menu_variant(item: &str, owner: bool) -> &'static str {
    match (item, owner) {
        ("board", true) => "board-owner",
        ("board", false) => "board",
        ("agent", true) => "agent-owner",
        ("agent", false) => "agent",
        ("app", _) => "app",
        _ => "session",
    }
}

/// #EXP-2124 `move`: Move to folder's rows: the team's folders (but the one
/// the item sits in), then New folder, then Top level for an item in a
/// folder.
fn move_items(folders: &[(String, String)], inside: Option<&str>) -> Vec<Value> {
    use domain::sidebar_menus::{sidebar_menu_copy, sidebar_menu_icon};
    let mut rows: Vec<Value> = folders
        .iter()
        .filter(|(id, _)| Some(id.as_str()) != inside)
        .map(|(id, name)| json!({"label": name, "value": format!("{MOVE_FOLDER}{id}"), "icon": "ui-folder"}))
        .collect();
    if !rows.is_empty() {
        rows.push(json!({"kind": "separator"}));
    }
    rows.push(json!({"label": sidebar_menu_copy("new-folder"), "value": "new-folder", "icon": sidebar_menu_icon("new-folder")}));
    if inside.is_some() {
        rows.push(json!({"label": sidebar_menu_copy("top-level"), "value": "top-level"}));
    }
    rows
}

/// A Move to folder row's value for a folder: `folder:<id>`.
const MOVE_FOLDER: &str = "folder:";

/// The key an Active row takes in the tree's List (a placed session's own
/// row keeps the bare id).
const ACTIVE_KEY: &str = "active:";

/// #EXP-2027: the teams and the Active band as ONE List's rows (so an
/// Active row drags into a team): every team's rows, then the Active label
/// row and one row per running chat (key `active:<root>`, depth 1, its
/// team).
fn tree_rows(input: &IdeSidebarInput, value: &Value, hover: Option<&str>, extras: &RowExtras) -> Vec<Value> {
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
            row["menu"] = json!(if extras.deletable.contains(&source.id) { "active-delete" } else { "active" });
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
    /// #EXP-2124: what the row menus read beside the projection.
    extras: RowExtras,
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
    /// The dev sidebar-scroll capture wheeled its rows ([`Self::dev_scroll_rows`]).
    dev_scroll_done: bool,
    /// The row kind the dev row-menu capture right-clicked last ([`Self::dev_open_row_menu`]).
    dev_row_menu_done: Option<String>,
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
            with_table_menus(&mut doc);
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
            uicore_gpui::inline::set_style(&mut corner, "root", "paddingTop", json!(0));
            uicore_gpui::inline::set_style(&mut corner, "root", "paddingBottom", json!(0));
            uicore_gpui::inline::set_style(&mut corner, "root", "height", Value::Null);
            // #EXP-2052: the account menu is taller than the 36px row it
            // hangs from: it is placed against the window (above the row,
            // `side: top`), not the row's own box, where "below" had more
            // room and it opened off the window's bottom edge.
            let footer = uicore_gpui::inline::mount_lifted(&corner, &uicore_data::Feed::new(), theme, host, Mode::Dark, window, cx);
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
                extras: RowExtras::default(),
                shown: None,
                computer,
                computer_changed: true,
                editing,
                focus_pending: false,
                dev_team_menu_done: false,
                dev_scroll_done: false,
                dev_row_menu_done: None,
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
        let (input, glyphs, extras) = {
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
        self.extras = RowExtras { editing_device: self.extras.editing_device.take(), ..extras };
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

    /// DEV-ONLY (`EXP_DEV_SIDEBAR_SCROLL=1`, the sidebar-scroll pictures):
    /// a wheel over the rows past their end, through the window's own hit
    /// test as a pointer resting on a row would, every half second for ten
    /// seconds (the synced rows keep growing the list while they land).
    fn dev_scroll_rows(&mut self, cx: &mut gpui::Context<Self>) {
        if self.dev_scroll_done || std::env::var("EXP_DEV_SIDEBAR_SCROLL").map_or(true, |v| v.trim().is_empty() || v == "0") {
            return;
        }
        self.dev_scroll_done = true;
        let (view, handle) = (self.view.clone(), self.window);
        cx.spawn(async move |_, cx| {
            for _ in 0..20 {
                cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
                let _ = handle.update(cx, |_, window, cx| {
                    let Some((origin, frame)) = view.read_with(cx, |view, _| Some((view.origin(), view.frame(view.index_of("scroll")?)?))) else { return };
                    let at = gpui::point(origin.x + gpui::px(frame.x + frame.w / 2.0), origin.y + gpui::px(frame.y + frame.h / 2.0));
                    window.dispatch_event(gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent { position: at, pressed_button: None, modifiers: Default::default() }), cx);
                    let wheel = gpui::ScrollWheelEvent { position: at, delta: gpui::ScrollDelta::Lines(gpui::point(0., -1000.)), ..Default::default() };
                    #[allow(deprecated)]
                    window.dispatch_event(gpui::InputEvent::to_platform_input(wheel), cx);
                });
            }
        })
        .detach();
    }

    /// DEV-ONLY (`EXP_DEV_SIDEBAR_MENU=device|board`, the
    /// `sidebar-device-menu` / `sidebar-board-menu` captures, #EXP-2124): once
    /// the first row of that kind is drawn, a right-click on it opens its
    /// menu, through the window's own hit test as a pointer would.
    fn dev_open_row_menu(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(kind) = std::env::var("EXP_DEV_SIDEBAR_MENU").ok().filter(|v| !v.trim().is_empty() && v != "0") else { return };
        if self.dev_row_menu_done.as_deref() == Some(kind.as_str()) {
            return;
        }
        let rows: &[&str] = match kind.as_str() {
            "device" => &["ide_device_mine_row.", "ide_device_other_row.", "ide_device_mine_muted_row.", "ide_device_other_muted_row."],
            "board" => &["ide_board_owner_row.", "ide_board_row."],
            "agent" => &["ide_agent_owner_row.", "ide_agent_row."],
            "app" => &["ide_app_row."],
            _ => &["ide_team_row."],
        };
        let rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
        self.dev_row_menu_done = Some(kind);
        let (view, handle) = (self.view.clone(), self.window);
        cx.spawn(async move |_, cx| {
            for _ in 0..80 {
                cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
                let clicked = handle
                    .update(cx, |_, window, cx| {
                        let Some((origin, frame)) = view.read_with(cx, |view, _| {
                            let node = view.placed_nodes().iter().find(|n| rows.iter().any(|r| n.id.starts_with(r.as_str()))).map(|n| n.index)?;
                            Some((view.origin(), view.frame(node).filter(|f| f.w > 0. && f.h > 0.)?))
                        }) else {
                            return false;
                        };
                        let at = gpui::point(origin.x + gpui::px(frame.x + frame.w / 3.0), origin.y + gpui::px(frame.y + frame.h / 2.0));
                        window.dispatch_event(gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent { position: at, pressed_button: None, modifiers: Default::default() }), cx);
                        window.dispatch_event(gpui::PlatformInput::MouseDown(gpui::MouseDownEvent { button: gpui::MouseButton::Right, position: at, modifiers: Default::default(), click_count: 1, first_mouse: false }), cx);
                        window.dispatch_event(gpui::PlatformInput::MouseUp(gpui::MouseUpEvent { button: gpui::MouseButton::Right, position: at, modifiers: Default::default(), click_count: 1 }), cx);
                        true
                    })
                    .unwrap_or(true);
                if clicked {
                    break;
                }
            }
        })
        .detach();
    }

    fn push(&mut self, cx: &mut gpui::Context<Self>) {
        self.dev_open_team_menu(cx);
        self.dev_open_row_menu(cx);
        self.dev_scroll_rows(cx);
        self.input.editing_folder = self.editing.borrow().clone();
        let value = shaped(&self.input, &self.glyphs.borrow(), self.hover.borrow().as_deref(), &self.extras);
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

    /// #EXP-2124: edit device `id`'s name in place (Rename); `None` ends it.
    fn edit_device(&mut self, id: Option<String>, cx: &mut gpui::Context<Self>) {
        self.extras.editing_device = id.clone();
        self.push(cx);
        let Some(id) = id else { return };
        let field = format!("ide_device_name.{id}.field");
        let (view, handle) = (self.view.clone(), self.window);
        App::defer(cx, move |cx| {
            let _ = handle.update(cx, move |_, window, _| {
                window.on_next_frame(move |window, cx| {
                    view.update(cx, |view, cx| view.focus_field(&field, window, cx));
                });
            });
        });
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

/// The sidebar document's tree List (teams + Active, apps/ede/ui/sidebar.express).
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
                } else if event.keystroke.key == "escape" && region.extras.editing_device.is_some() {
                    region.edit_device(None, cx);
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
        // #EXP-2124: the Assistant opens its chat.
        "nav" if id == "assistant" => {
            if let Some(assistant) = crate::session_data::my_assistant_anywhere(None, cx) {
                navigate(window, cx, Screen::Agent { agent_id: assistant.id });
            }
        }
        "getting-started" => crate::getting_started::open_dialog(window, cx),
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
        "board" | "agent" | "app" | "session" => new_in_team(team_id, value, window, cx),
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

/// #EXP-2124: every menu node of the sidebar document and the table row
/// (sidebar-menus.json `kinds`, the viewer's input) its items are. The
/// document spells them out; the IDE writes them again from the table at
/// mount (the platform's shortcut), and a test holds the two equal.
fn menu_nodes() -> Vec<(String, &'static str, domain::sidebar_menus::SidebarMenuInput)> {
    use domain::sidebar_menus::SidebarMenuInput as I;
    let owner = I { owner: true, ..I::default() };
    let mine = I { mine: true, ..I::default() };
    let deletes = I { can_delete: true, ..I::default() };
    let mut nodes: Vec<(String, &'static str, I)> = vec![
        ("ide_nav_assistant_menu".into(), "assistant", I::default()),
        ("ide_nav_compose_menu".into(), "home", I::default()),
        ("ide_nav_compose_menu".into(), "chats", I::default()),
        ("ide_nav_page_menu".into(), "needs-you", I::default()),
        ("ide_nav_page_menu".into(), "drafts", I::default()),
        ("ide_devices_menu".into(), "devices", I::default()),
        ("ide_device_mine_menu".into(), "device", mine),
        ("ide_device_mine_muted_menu".into(), "device", mine),
        ("ide_device_other_menu".into(), "device", I::default()),
        ("ide_device_other_muted_menu".into(), "device", I::default()),
        ("ide_teams_head_menu".into(), "teams", I::default()),
        ("ide_team_menu".into(), "team", I::default()),
        ("ide_team_more".into(), "team", owner),
        ("ide_folder_menu".into(), "folder", I::default()),
        ("ide_active_delete_menu".into(), "active-session", deletes),
        ("ide_active_keep_menu".into(), "active-session", I::default()),
        ("ide_getting_started".into(), "getting-started", I::default()),
    ];
    for sfx in ["", "_child"] {
        for (node, kind, input) in [("board_owner", "board", owner), ("board", "board", I::default()), ("app", "app", I::default()), ("agent_owner", "agent", owner), ("agent", "agent", I::default()), ("session", "session", I::default())] {
            nodes.push((format!("ide_{node}{sfx}_menu"), kind, input));
        }
    }
    nodes
}

/// A table menu's genUI items: New › and Move to folder › bind the row's
/// own rows (`newItems`, `moveItems`).
fn table_menu_items(kind: &str, input: domain::sidebar_menus::SidebarMenuInput) -> Vec<Value> {
    domain::sidebar_menus::sidebar_menu_items(kind, input, |id| match id {
        "new" => Some(json!({"@path": "newItems"})),
        "move" => Some(json!({"@path": "moveItems"})),
        _ => None,
    })
}

/// #EXP-2124: the document's menus, written from the table.
fn with_table_menus(doc: &mut uicore_gpui::inline::Document) {
    for (node, kind, input) in menu_nodes() {
        uicore_gpui::inline::set_prop(doc, &node, "items", Value::Array(table_menu_items(kind, input)));
    }
}

/// #EXP-2124: the page a row opens (`open`, `open-new-tab`).
fn row_screen(kind: &str, id: &str, cx: &App) -> Option<Screen> {
    Some(match kind {
        "assistant" => Screen::Agent { agent_id: crate::session_data::my_assistant_anywhere(None, cx)?.id },
        "home" => Screen::Home,
        "needs-you" => Screen::NeedsYou,
        "chats" => Screen::Chats,
        "drafts" => Screen::Drafts,
        "device" => Screen::Device { device_id: id.into() },
        "board" => Screen::BoardIssues { board_id: id.into() },
        "app" => Screen::AppRun { app: id.into() },
        "agent" => Screen::Agent { agent_id: id.into() },
        "session" => navigation::session_screen_for(id, cx),
        "active-session" => Screen::Chat { session_id: id.into() },
        _ => return None,
    })
}

/// #EXP-2124 `copy-link`: the page's web URL (web's routes).
fn row_link(kind: &str, id: &str, cx: &App) -> Option<String> {
    let base = crate::queries::active_account(cx)?.instance_url.trim_end_matches('/').to_string();
    let collections = Store::try_global(cx)?.collections().clone();
    let team_slug = |team_id: &str| collections.teams.read(cx).get(team_id).and_then(|team| team.slug.clone());
    let path = match kind {
        "device" => format!("/devices/{id}"),
        "board" => {
            let board = collections.boards.read(cx).get(id).cloned()?;
            format!("/t/{}/boards/{}", team_slug(&board.team_id)?, board.slug?)
        }
        "app" => {
            let app = collections.apps.read(cx).get(id).cloned()?;
            format!("/t/{}/apps/{}", team_slug(app.team_id.as_deref()?)?, app.slug)
        }
        "session" | "active-session" => {
            let session = crate::session_data::session(id, cx)?;
            format!("/t/{}/sessions/{id}", team_slug(session.team_id.as_deref()?)?)
        }
        _ => return None,
    };
    Some(format!("{base}{path}"))
}

/// #EXP-2124: one pick of a sidebar row's menu (sidebar-menus.json VERBS).
fn row_menu(kind: &str, id: String, team: String, value: &str, region: gpui::WeakEntity<SidebarRegion>, window: &mut Window, cx: &mut App) {
    use domain::session_screen::SESSION_SCREEN;
    match value {
        "open" => match (kind, row_screen(kind, &id, cx)) {
            ("getting-started", _) => crate::getting_started::open_dialog(window, cx),
            ("board", _) => open_row("board", id, window, cx),
            ("active-session", _) => open_row("active", id, window, cx),
            (_, Some(screen)) => navigate(window, cx, screen),
            _ => {}
        },
        "open-new-tab" => {
            if let Some(screen) = row_screen(kind, &id, cx) {
                crate::screens::open_tab_in_background(window, cx, screen);
            }
        }
        "new-chat" => navigate(window, cx, Screen::Home),
        "compact" | "new-conversation" => {
            let sends = &SESSION_SCREEN.menu_sends;
            let text = if value == "compact" { sends.compact.clone() } else { sends.new_conversation.clone() };
            let agent_id = if kind == "assistant" { crate::session_data::my_assistant_anywhere(None, cx).map(|a| a.id) } else { Some(id.clone()) };
            if let Some(agent_id) = agent_id {
                crate::chat_actions::send_to_agent(&agent_id, &text, cx);
            }
        }
        "add-device" => crate::machines::open_add_server_dialog(window, cx),
        "new-team" => window.dispatch_action(Box::new(crate::actions::CreateTeam), cx),
        "join-team" => window.dispatch_action(Box::new(crate::actions::JoinTeam), cx),
        "new-issue" => crate::issue_draft_screen::open_new_from_rail(window, cx, id),
        "edit" => crate::app_screen::open_app_tab(window, cx, id, domain::apps::EditorTab::Chat, false),
        "settings" => match kind {
            "assistant" => {
                navigation::open_settings(window, cx, None);
                crate::sidebar::select_settings_section(window, cx, SettingsSection::Voice);
            }
            "device" => navigate(window, cx, Screen::DeviceSettings { device_id: id }),
            kind => open_gear(kind, id, window, cx),
        },
        "rename" if kind == "device" => {
            let _ = region.update(cx, |region, cx| region.edit_device(Some(id), cx));
        }
        "copy-link" => {
            if let Some(url) = row_link(kind, &id, cx) {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(url));
                crate::toast::success(domain::sidebar_menus::sidebar_menu_copy("linkCopied"), window, cx);
            }
        }
        "remove-from-sidebar" => remove_from_sidebar(id, cx),
        "add-to-sidebar" => {
            let Some(row) = crate::session_data::session(&id, cx) else { return };
            let Some(team_id) = row.team_id.clone() else { return };
            let Some(input) = region.read_with(cx, |region, _| region.input.clone()).ok() else { return };
            let items = team_items(&team_id, &input);
            let entries = layout_entries(&team_id, cx);
            let source = domain::sidebar_tree::SidebarSessionInput { id: row.id.clone(), team_id: team_id.clone(), parent_id: row.parent_id.clone() };
            // Appended to its team's list (sidebar-tree.json `drop`).
            let last = domain::sidebar_folders::sidebar_tree(&items, &entries).last().map(|node| node.id().to_string());
            let mv = SidebarMove { id: row.id.clone(), before: None, after: last, into: None };
            let writes = if mv.after.is_some() {
                domain::sidebar_tree::sidebar_drop_session(&team_id, &items, &entries, &source, &mv, &uuid::Uuid::new_v4().to_string())
            } else {
                Some(append_writes(&entries, &row.id))
            };
            if let Some(writes) = writes {
                crate::ui_prefs::set_sidebar_folder_open(&domain::sidebar_tree::sidebar_team_open_key(&team_id), true);
                save_layout(team_id, writes, cx);
            }
        }
        "archive" if kind == "board" => {
            let Some(board) = Store::global(cx).collections().boards.read(cx).get(&id).cloned() else { return };
            crate::settings::board_detail::BoardDetailPane::open_archive_dialog(board.id, board.name, window, cx);
        }
        "delete" if kind == "active-session" => crate::chat_actions::confirm_delete(&id, window, cx),
        // Move to folder › a folder, New folder, Top level.
        target => {
            let target = match target.strip_prefix(MOVE_FOLDER) {
                Some(folder) => MoveTarget::Folder(folder.to_string()),
                None if target == "new-folder" && kind == "team" => {
                    return team_menu(id, "new-folder", region, window, cx);
                }
                None if target == "new-folder" => MoveTarget::NewFolder,
                None if target == "top-level" => MoveTarget::TopLevel,
                None => return,
            };
            let items = region.read_with(cx, |region, _| team_items(&team, &region.input)).unwrap_or_default();
            let entries = layout_entries(&team, cx);
            let Some(writes) = move_writes(&items, &entries, &id, &target, &uuid::Uuid::new_v4().to_string()) else { return };
            let created = writes.created_folder_id.clone();
            save_layout(team, writes, cx);
            if created.is_some() {
                let _ = region.update(cx, |region, cx| region.edit_folder(created, cx));
            }
        }
    }
}

/// #EXP-2124 `move`: where Move to folder puts an item.
#[derive(Debug, Clone, PartialEq)]
enum MoveTarget {
    Folder(String),
    /// A new folder holding the item, its name in edit.
    NewFolder,
    /// Out of its folder, right after it.
    TopLevel,
}

/// #EXP-2124 `move` as the team's writes; `None` = nothing to do.
fn move_writes(items: &[String], entries: &[SidebarEntry], item: &str, target: &MoveTarget, new_folder: &str) -> Option<SidebarWrites> {
    let mv = |after: Option<String>, into: Option<String>| SidebarMove { id: item.to_string(), before: None, after, into };
    match target {
        MoveTarget::Folder(folder) => sidebar_move(items, entries, &mv(None, Some(folder.clone())), new_folder),
        MoveTarget::TopLevel => {
            let parent = entries.iter().find(|e| e.kind == SidebarEntryKind::Item && e.target_id.as_deref() == Some(item))?.parent_id.clone()?;
            sidebar_move(items, entries, &mv(Some(parent), None), new_folder)
        }
        MoveTarget::NewFolder => {
            let folder = new_folder_writes(entries, new_folder);
            let created = folder.upserts.first()?;
            let mut with_folder = entries.to_vec();
            with_folder.push(SidebarEntry { id: new_folder.to_string(), kind: SidebarEntryKind::Folder, target_id: None, parent_id: None, name: created.name.clone(), sort_order: created.sort_order });
            let moved = sidebar_move(items, &with_folder, &mv(None, Some(new_folder.to_string())), new_folder)?;
            let mut upserts = folder.upserts;
            upserts.extend(moved.upserts.into_iter().filter(|w| w.id.as_deref() != Some(new_folder)));
            Some(SidebarWrites { upserts, deletes: moved.deletes, created_folder_id: Some(new_folder.to_string()) })
        }
    }
}

/// An item entry for `target_id` at the end of an empty team list.
fn append_writes(entries: &[SidebarEntry], target_id: &str) -> SidebarWrites {
    let last = entries.iter().filter(|e| e.parent_id.is_none()).map(|e| e.sort_order).fold(0.0_f64, f64::max);
    SidebarWrites {
        upserts: vec![domain::sidebar_folders::SidebarEntryWrite { id: None, kind: SidebarEntryKind::Item, target_id: Some(target_id.to_string()), parent_id: None, name: None, sort_order: last.floor() + 1.0 }],
        deletes: Vec::new(),
        created_folder_id: None,
    }
}

/// #EXP-2124 THE New list's verbs (the team menu's New › and the new-tab
/// menu's team submenus): Board, Agent, App open their creation pages, Chat
/// Home's composer with the team as its pick.
fn new_in_team(team_id: String, kind: &str, window: &mut Window, cx: &mut App) {
    match kind {
        "board" => navigate(window, cx, Screen::NewItem { team_id, kind: navigation::NewItemKind::Board }),
        "agent" => navigate(window, cx, Screen::NewItem { team_id, kind: navigation::NewItemKind::Agent }),
        "app" => navigate(window, cx, Screen::NewItem { team_id, kind: navigation::NewItemKind::App }),
        "session" => navigation::open_home_in_team(window, cx, team_id),
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
        // #EXP-2124: the Assistant's row: the brand mark at three quarters of
        // a muted disc (web `AssistantMark`).
        if name == ASSISTANT_MARK {
            return Some(assistant_mark(size, cx));
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
            // #EXP-2124: every other row's menu (sidebar-menus.json).
            "ide_menu" => {
                let Some(value) = value else { return };
                self.in_window(cx, move |window, cx| row_menu(&kind, id, team, &value, region, window, cx));
            }
            // #EXP-2124: a device's name in place: Enter or leaving it saves
            // (`devices.rename`; a blank name keeps the old one).
            "ide_device" => {
                let action = text("action");
                cx.defer(move |cx| {
                    let Some(editing) = region.read_with(cx, |region, _| region.extras.editing_device.clone()).ok().flatten() else { return };
                    if editing != id {
                        return;
                    }
                    let field = format!("ide_device_name.{id}.field");
                    let typed = match action.as_str() {
                        "save" => value.clone(),
                        _ => region.read_with(cx, |region, cx| region.view.read(cx).field_text(&field, cx)).ok().flatten(),
                    };
                    let label = typed.unwrap_or_default().trim().to_string();
                    if !label.is_empty() {
                        if let Some(trpc) = crate::queries::trpc_client(cx) {
                            let device_id = id.clone();
                            cx.background_executor()
                                .spawn(async move {
                                    if let Err(err) = api::devices::rename(&trpc, &device_id, &label) {
                                        log::warn!("[ui] devices.rename failed: {err}");
                                    }
                                })
                                .detach();
                        }
                    }
                    let _ = region.update(cx, |region, cx| region.edit_device(None, cx));
                });
            }
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
    /// #EXP-2124: the new-tab menu's items as the document holds them, and
    /// the team marks they name.
    new_menu: Value,
    marks: Rc<RefCell<TeamMarks>>,
    /// The dev new-tab-menu capture pressed its + ([`Self::dev_open_new_menu`]).
    dev_new_menu_done: bool,
    more: Option<Vec<String>>,
    observes_panel: bool,
    _observers: Vec<gpui::Subscription>,
}

impl TabBarRegion {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut gpui::Context<Self>| {
            let tabs = Rc::new(RefCell::new(OpenTabs::default()));
            let hover = Rc::new(RefCell::new(None));
            let marks = Rc::new(RefCell::new(TeamMarks::new()));
            let host = Rc::new(TabHost { tabs: tabs.clone(), hover: hover.clone(), marks: marks.clone(), window: window.window_handle(), region: cx.weak_entity() });
            let mut doc = uicore_gpui::inline::document(&uicore_docs::TABBAR);
            // The +N menu opens away from the window edge the strip sits on.
            let more_side = crate::shell::tab_placement().more_side;
            if more_side == uicore_model::ide::IdeEdge::Bottom {
                uicore_gpui::inline::set_prop(&mut doc, "ide_tabs_more", "side", json!("bottom"));
                uicore_gpui::inline::set_prop(&mut doc, "ide_tabs_new_menu", "side", json!("bottom"));
            }
            let theme = uicore_gpui::inline::theme(&eui_core::themes::default_theme());
            // #EXP-1814: the +N menu is taller than the 44px band: it is
            // placed against the window (below the top band, on screen).
            let view = uicore_gpui::inline::mount_lifted(&doc, &uicore_data::Feed::new(), theme, host, Mode::Dark, window, cx);
            let nav = crate::navigation::nav_for_window(window, cx);
            // #LOGS-4: the strip is a cached region now, so it observes what
            // its titles and leads read (issues, session states) itself.
            let mut observers = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
            if let Some(collections) = Store::try_global(cx).map(|store| store.collections().clone()) {
                observers.push(cx.observe(&collections.issues, |_, _, cx| cx.notify()));
                // #EXP-2124: the new-tab menu lists the teams and the Assistant.
                observers.push(cx.observe(&collections.teams, |_, _, cx| cx.notify()));
                observers.push(cx.observe(&collections.agents, |_, _, cx| cx.notify()));
            }
            observers.extend(crate::session_data::observe_session_collections(cx));
            Self { view, bounds: Rc::default(), doc, tabs, hover, keys: Vec::new(), shown: None, new_menu: json!([]), marks, dev_new_menu_done: false, more: None, observes_panel: false, _observers: observers }
        })
    }

    /// DEV-ONLY (`EXP_DEV_NEW_TAB_MENU=1`, the `new-tab-menu` capture,
    /// #EXP-2124): once the strip's + is laid out with its menu written, it
    /// is pressed as a pointer would.
    fn dev_open_new_menu(&mut self, window: &Window, cx: &mut gpui::Context<Self>) {
        if self.dev_new_menu_done || self.new_menu.as_array().is_none_or(|items| items.is_empty()) || std::env::var("EXP_DEV_NEW_TAB_MENU").map_or(true, |v| v.trim().is_empty() || v == "0") {
            return;
        }
        self.dev_new_menu_done = true;
        let view = self.view.clone();
        let handle = window.window_handle();
        cx.spawn(async move |_, cx| {
            for _ in 0..40 {
                cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
                let pressed = handle
                    .update(cx, |_, window, cx| {
                        view.update(cx, |view, cx| {
                            let plus = view.index_of("ide_tabs.new").filter(|i| view.frame(*i).is_some_and(|f| f.w > 0.));
                            plus.map(|index| view.press(index, window, cx)).is_some()
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
        // #EXP-2124: THE new-tab menu, rewritten when what it lists moved.
        let new_menu = Value::Array(self.gather_new_menu(cx));
        if new_menu != self.new_menu {
            self.new_menu = new_menu.clone();
            uicore_gpui::inline::set_prop(&mut self.doc, "ide_tabs_new_menu", "items", new_menu);
            uicore_gpui::inline::reload(&self.view, &self.doc, cx);
        }
        self.dev_open_new_menu(window, cx);
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

    /// #EXP-2124: the new-tab menu for the viewer (their Assistant, their
    /// teams in sidebar order), the team marks loaded on the way.
    fn gather_new_menu(&mut self, cx: &mut gpui::Context<Self>) -> Vec<Value> {
        let assistant = crate::session_data::my_assistant_anywhere(None, cx).map(|a| crate::agent_screen::agent_def_name(&a));
        let Some(store) = Store::try_global(cx) else { return Vec::new() };
        let all = store.collections().teams_sorted(cx);
        let order = domain::sidebar_tree::sidebar_teams(&all.iter().map(|t| domain::sidebar_tree::SidebarTeamInput { id: t.id.clone(), name: t.name.clone(), kind: "team".into() }).collect::<Vec<_>>());
        let mut marks = TeamMarks::new();
        let teams: Vec<NewTabTeamRow> = order
            .into_iter()
            .filter_map(|id| {
                let team = all.iter().find(|t| t.id == id)?;
                marks.insert(id.clone(), crate::user_avatar::team_image(team.icon_url.as_deref(), cx));
                Some(NewTabTeamRow { owner: crate::settings::is_owner(cx, &id), name: team.name.clone(), id })
            })
            .collect();
        *self.marks.borrow_mut() = marks;
        new_tab_menu_items(assistant.as_deref(), &teams)
    }

    /// The new-tab menu open on a sample, for the pictures.
    #[cfg(test)]
    pub(crate) fn show_new_menu(&mut self, assistant: Option<&str>, teams: &[(&str, &str, bool)], cx: &mut gpui::Context<Self>) {
        let teams: Vec<NewTabTeamRow> = teams.iter().map(|(id, name, owner)| NewTabTeamRow { id: id.to_string(), name: name.to_string(), owner: *owner }).collect();
        self.new_menu = Value::Array(new_tab_menu_items(assistant, &teams));
        uicore_gpui::inline::set_prop(&mut self.doc, "ide_tabs_new_menu", "items", self.new_menu.clone());
        uicore_gpui::inline::reload(&self.view, &self.doc, cx);
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

/// #EXP-2124: one team of the new-tab menu.
#[derive(Debug, Clone, PartialEq)]
struct NewTabTeamRow {
    id: String,
    name: String,
    owner: bool,
}

/// #EXP-2124 THE new-tab menu (sidebar-menus.json `newTabMenu`) as genUI
/// items: the Assistant (its name + the brand mark), a separator, one
/// submenu per team (its mark + name) holding THE New list (uicore's
/// `new_items`, the team menu's own), each value `<kind>:<teamId>`.
fn new_tab_menu_items(assistant: Option<&str>, teams: &[NewTabTeamRow]) -> Vec<Value> {
    let shape: Vec<domain::sidebar_menus::NewTabTeam> = teams.iter().map(|t| domain::sidebar_menus::NewTabTeam { id: t.id.clone(), owner: t.owner }).collect();
    domain::sidebar_menus::new_tab_menu(assistant.is_some(), &shape)
        .into_iter()
        .filter_map(|row| match row.id.as_str() {
            "assistant" => Some(json!({"label": assistant?, "value": "assistant", "icon": ASSISTANT_MARK})),
            domain::sidebar_menus::SIDEBAR_MENU_SEPARATOR => Some(json!({"kind": "separator"})),
            id => {
                let team = teams.iter().find(|t| Some(t.id.as_str()) == id.strip_prefix("team:"))?;
                let items: Vec<Value> = uicore_model::ide::new_items(team.owner)
                    .into_iter()
                    .map(|entry| json!({"label": entry.label, "value": format!("{}:{}", entry.value, team.id), "icon": entry.icon}))
                    .collect();
                Some(json!({"kind": "submenu", "label": team.name, "icon": team_mark_icon(&team.id), "items": items}))
            }
        })
        .collect()
}

/// #EXP-2124: the Assistant's mark: the brand logo at three quarters of a
/// muted disc (web `AssistantMark`).
fn assistant_mark(size: f32, cx: &App) -> AnyElement {
    div()
        .size(gpui::px(size))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(cx.theme().muted)
        .child(Icon::from(crate::icons::ExpIcon::Logo).size(gpui::px(size * 0.75)).text_color(cx.theme().foreground))
        .into_any_element()
}

struct TabHost {
    tabs: Rc<RefCell<OpenTabs>>,
    hover: Rc<RefCell<Option<usize>>>,
    marks: Rc<RefCell<TeamMarks>>,
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

    fn icon_element(&self, name: &str, size: f32, _color: Hsla, cx: &App) -> Option<AnyElement> {
        // #EXP-2124: the new-tab menu's Assistant and team marks.
        if name == ASSISTANT_MARK {
            return Some(assistant_mark(size, cx));
        }
        if let Some(team) = name.strip_prefix("ide:team:") {
            return Some(crate::user_avatar::team_avatar_of(self.marks.borrow().get(team).cloned().flatten(), size));
        }
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
            // #EXP-1741: the + opens the composer page (Home), not a terminal;
            // #EXP-2124: the + opens THE new-tab menu (its anchor), so `new`
            // only comes from a strip without it.
            "new" => self.in_window(cx, |window, cx| navigate(window, cx, Screen::Home)),
            // #EXP-2124: a pick of the new-tab menu: the Assistant's chat, or
            // New › <kind> of a team (the team menu's own verbs).
            "new_menu" => {
                let Some(value) = chosen else { return };
                self.in_window(cx, move |window, cx| match value.split_once(':') {
                    None if value == "assistant" => {
                        if let Some(assistant) = crate::session_data::my_assistant_anywhere(None, cx) {
                            navigate(window, cx, Screen::Agent { agent_id: assistant.id });
                        }
                    }
                    Some((kind, team)) => new_in_team(team.to_string(), kind, window, cx),
                    None => {}
                });
            }
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
const HEADER_PRUNED: &[&str] = &["ide_nav", "scroll", GETTING_STARTED, "corner"];

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
        let value = shaped(&input, &HashMap::new(), Some("b1"), &RowExtras::default());
        // Home leads, Chats carries its count.
        assert_eq!(value["nav"][1]["dotIcon"], "ide:count:attention:2");
        let counted = IdeSidebarInput { needs_you: 5, drafts: 120, ..input.clone() };
        let counted = shaped(&counted, &HashMap::new(), None, &RowExtras::default());
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
        let value = shaped(&input, &HashMap::new(), None, &RowExtras::default());
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

    fn find_node<'a>(node: &'a Value, id: &str) -> Option<&'a Value> {
        if node["id"] == id {
            return Some(node);
        }
        node["children"].as_array().into_iter().flatten().chain(node["slots"].as_object().into_iter().flat_map(|slots| slots.values())).find_map(|c| find_node(c, id))
    }

    /// #EXP-2124: every menu the sidebar document spells out is its table
    /// row (sidebar-menus.json), item for item (the shortcut as Linux and
    /// Windows write it).
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_document_menus_are_the_table() {
        let doc = uicore_docs::doc("sidebar").expect("the sidebar").json();
        for (node, kind, input) in menu_nodes() {
            let found = find_node(&doc["root"], &node).unwrap_or_else(|| panic!("{node} is in the document"));
            assert_eq!(found["component"], "Menu", "{node}");
            assert_eq!(found["props"]["items"], Value::Array(table_menu_items(kind, input)), "{node} = the {kind} table row");
            if kind != "team" || node == "ide_team_menu" {
                assert_eq!(found["props"]["openOn"], "contextmenu", "{node} opens on a right-click");
            }
        }
        // Every row kind of the table has its menu in the document.
        let kinds: Vec<&str> = menu_nodes().into_iter().map(|(_, kind, _)| kind).collect();
        for kind in domain::sidebar_menus::sidebar_menu_kinds() {
            assert!(kinds.contains(&kind), "{kind} has no menu");
        }
    }

    /// #EXP-2124: a row knows which menu it opens and where Move to folder
    /// can put it.
    #[test]
    fn rows_carry_their_menu() {
        let item = |kind: &str, id: &str| IdeItemInput { kind: kind.into(), id: id.into(), name: id.into(), icon: String::new(), ..Default::default() };
        let layout = vec![
            IdeLayoutEntry { id: "f1".into(), kind: "folder".into(), target_id: None, parent_id: None, name: Some("Work".into()), sort_order: 1. },
            IdeLayoutEntry { id: "e1".into(), kind: "item".into(), target_id: Some("b2".into()), parent_id: Some("f1".into()), name: None, sort_order: 2. },
        ];
        let team = |id: &str, owner: bool| IdeTeamInput { id: id.into(), name: id.into(), mark: String::new(), open: true, owner, needs_you: 0, items: vec![item("board", "b1"), item("board", "b2"), item("agent", "a1"), item("app", "p1")], layout: layout.clone() };
        let input = IdeSidebarInput {
            devices: vec![IdeDeviceInput { id: "d1".into(), name: "Mint".into(), icon: String::new(), online: true, team_mark: None }, IdeDeviceInput { id: "d2".into(), name: "Box".into(), icon: String::new(), online: false, team_mark: Some("ide:team:t1".into()) }],
            devices_open: true,
            teams: vec![team("t1", true)],
            active: vec![IdeActiveInput { id: "s1".into(), title: "Chat".into(), icon: String::new(), session_ids: vec!["s1".into()], unread: false, team_mark: None, team: Some("t1".into()) }],
            assistant: Some(uicore_model::ide::IdeAssistantInput { id: "as1".into(), name: "Bro".into(), unread: 0, needs_you: false }),
            ..Default::default()
        };
        let extras = RowExtras { deletable: ["s1".to_string()].into_iter().collect(), editing_device: Some("d1".into()), getting_started: true };
        let value = shaped(&input, &HashMap::new(), None, &extras);
        assert_eq!(value["nav"][0]["id"], "assistant");
        assert_eq!(value["nav"][0]["icon"], ASSISTANT_MARK);
        assert_eq!(value["devices"][1]["menu"], "device-mine");
        assert_eq!(value["devices"][1]["editing"], true);
        assert_eq!(value["devices"][2]["menu"], "device-other");
        let rows = value["teams"][0]["rows"].as_array().unwrap();
        let row = |id: &str| rows.iter().find(|r| r["id"] == id).unwrap().clone();
        assert_eq!(row("b1")["menu"], "board-owner");
        assert_eq!(row("a1")["menu"], "agent-owner");
        assert_eq!(row("p1")["menu"], "app");
        // A top-level item: the folders, New folder; one inside: the others,
        // New folder, Top level.
        let values = |id: &str| row(id)["moveItems"].as_array().unwrap().iter().map(|i| i["value"].as_str().unwrap_or("-").to_string()).collect::<Vec<_>>();
        assert_eq!(values("b1"), ["folder:f1", "-", "new-folder"]);
        assert_eq!(values("b2"), ["new-folder", "top-level"]);
        let active = value["tree"].as_array().unwrap().iter().find(|r| r["kind"] == "active").unwrap();
        assert_eq!(active["menu"], "active-delete");
        assert_eq!(value["gettingStarted"], true);
        let member = IdeSidebarInput { teams: vec![team("t1", false)], ..input };
        let value = shaped(&member, &HashMap::new(), None, &RowExtras::default());
        let rows = value["teams"][0]["rows"].as_array().unwrap();
        assert_eq!(rows.iter().find(|r| r["id"] == "b1").unwrap()["menu"], "board");
        assert_eq!(rows.iter().find(|r| r["id"] == "a1").unwrap()["menu"], "agent");
    }

    /// #EXP-2124 Move to folder: into a folder, out to the top level, into
    /// a new folder (named in place).
    #[test]
    fn move_to_folder_writes() {
        let entry = |id: &str, kind: SidebarEntryKind, target: Option<&str>, parent: Option<&str>, sort_order: f64| SidebarEntry { id: id.into(), kind, target_id: target.map(Into::into), parent_id: parent.map(Into::into), name: None, sort_order };
        let items: Vec<String> = ["b1", "b2", "a1"].iter().map(|s| s.to_string()).collect();
        let entries = vec![entry("f1", SidebarEntryKind::Folder, None, None, 1.), entry("e1", SidebarEntryKind::Item, Some("b2"), Some("f1"), 2.), entry("e3", SidebarEntryKind::Item, Some("a1"), Some("f1"), 3.)];
        let into = move_writes(&items, &entries, "b1", &MoveTarget::Folder("f1".into()), "fx").expect("into the folder");
        assert!(into.upserts.iter().any(|w| w.target_id.as_deref() == Some("b1") && w.parent_id.as_deref() == Some("f1")));
        let out = move_writes(&items, &entries, "b2", &MoveTarget::TopLevel, "fx").expect("to the top level");
        assert!(out.upserts.iter().any(|w| w.target_id.as_deref() == Some("b2") && w.parent_id.is_none()));
        assert!(move_writes(&items, &entries, "b1", &MoveTarget::TopLevel, "fx").is_none(), "already top level");
        let fresh = move_writes(&items, &entries, "b1", &MoveTarget::NewFolder, "fn").expect("a new folder");
        assert_eq!(fresh.created_folder_id.as_deref(), Some("fn"));
        assert!(fresh.upserts.iter().any(|w| w.id.as_deref() == Some("fn") && w.kind == SidebarEntryKind::Folder));
        assert!(fresh.upserts.iter().any(|w| w.target_id.as_deref() == Some("b1") && w.parent_id.as_deref() == Some("fn")));
    }

    /// #EXP-2124: THE new-tab menu: the Assistant, a separator, each team's
    /// New list as its submenu (values `<kind>:<team>`).
    #[test]
    fn the_new_tab_menu_lists_the_assistant_then_the_teams() {
        let teams = vec![NewTabTeamRow { id: "t1".into(), name: "Acme".into(), owner: true }, NewTabTeamRow { id: "t2".into(), name: "Beta".into(), owner: false }];
        let items = new_tab_menu_items(Some("Bro"), &teams);
        assert_eq!(items[0], json!({"label": "Bro", "value": "assistant", "icon": ASSISTANT_MARK}));
        assert_eq!(items[1], json!({"kind": "separator"}));
        assert_eq!(items[2]["label"], "Acme");
        assert_eq!(items[2]["icon"], "ide:team:t1");
        let values: Vec<&str> = items[2]["items"].as_array().unwrap().iter().map(|i| i["value"].as_str().unwrap()).collect();
        assert_eq!(values, ["session:t1", "agent:t1", "app:t1", "board:t1"]);
        assert_eq!(items[3]["items"], json!([{"label": "Chat", "value": "session:t2", "icon": "action-chat"}]));
        assert_eq!(new_tab_menu_items(None, &teams)[0]["label"], "Acme");
        assert_eq!(new_tab_menu_items(Some("Bro"), &[]).len(), 1);
    }

    #[test]
    fn only_a_row_instance_is_a_hover_target() {
        assert_eq!(row_key("ide_board_owner_row.b1"), Some("b1"));
        // A row inside a folder, a folder, a team, a device.
        assert_eq!(row_key("ide_board_child_row.b1"), Some("b1"));
        assert_eq!(row_key("ide_folder_row.f1"), Some("f1"));
        assert_eq!(row_key("ide_team_row.t1"), Some("t1"));
        assert_eq!(row_key("ide_device_mine_row.d1"), Some("d1"));
        assert_eq!(row_key("ide_board_gear.b1"), None);
        assert_eq!(row_key("ide_teams_add"), None);
    }

    #[test]
    fn the_document_prunes_only_nodes_it_has() {
        let doc = uicore_docs::doc("sidebar").expect("the sidebar");
        for id in PRUNED.iter().chain(HEADER_PRUNED).chain(FOOTER_PRUNED).chain([&CORNER]) {
            assert!(doc.source.lines().any(|l| l.starts_with(&format!("{id} = "))), "{id}");
        }
    }
}
