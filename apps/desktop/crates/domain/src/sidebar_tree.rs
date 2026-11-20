//! #EXP-2027: the sidebar tree (web md+ and the desktop IDE), the Rust twin
//! of `packages/domain-contract/src/sidebar-tree.ts` (fixture
//! `sidebar-tree.json`, whose `$comment` is the spec; replayed whole by
//! `tests/sidebar_tree.rs`).
//!
//! No active team: Home, Needs you, Chats, Drafts, Devices (each device a
//! row), then one row per team holding ONE mixed list of its boards, agents,
//! apps and the root sessions the member placed, in their own folders
//! (`sidebar_folders` runs unchanged over that list, section `team`).

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::sidebar_folders::{sidebar_move, SidebarEntry, SidebarEntryKind, SidebarMove, SidebarRow, SidebarWrites};

const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/sidebar-tree.json");

/// List nesting: team › folder › item.
pub const SIDEBAR_TREE_MAX_DEPTH: u8 = 3;
/// The device-local expanded flags (no stored flag = open).
pub const SIDEBAR_DEVICES_OPEN_KEY: &str = "sidebar-devices-open";

pub fn sidebar_team_open_key(team_id: &str) -> String {
    format!("sidebar-team-open:{team_id}")
}

/// The fixture's `copy`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarTreeCopy {
    pub home: String,
    pub needs_you: String,
    pub chats: String,
    pub drafts: String,
    pub devices: String,
    pub teams: String,
    pub add_device: String,
    pub device_settings: String,
    pub team_actions: String,
    pub new: String,
    pub new_folder: String,
    pub settings: String,
    pub board: String,
    pub agent: String,
    pub app: String,
    pub session: String,
    pub new_team: String,
    pub join_team: String,
    pub remove_from_sidebar: String,
}

pub fn sidebar_tree_copy() -> &'static SidebarTreeCopy {
    static COPY: OnceLock<SidebarTreeCopy> = OnceLock::new();
    COPY.get_or_init(|| {
        let fixture: serde_json::Value = serde_json::from_str(FIXTURE).expect("sidebar-tree.json");
        serde_json::from_value(fixture["copy"].clone()).expect("sidebar-tree.json copy")
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SidebarRootRow {
    Home,
    NeedsYou,
    Chats,
    Drafts,
    Devices,
}

impl SidebarRootRow {
    pub fn as_str(self) -> &'static str {
        match self {
            SidebarRootRow::Home => "home",
            SidebarRootRow::NeedsYou => "needs-you",
            SidebarRootRow::Chats => "chats",
            SidebarRootRow::Drafts => "drafts",
            SidebarRootRow::Devices => "devices",
        }
    }
}

/// The fixed rows above the teams; Needs you and Drafts only while they count.
pub fn sidebar_root_rows(needs_you: usize, drafts: usize) -> Vec<SidebarRootRow> {
    let mut rows = vec![SidebarRootRow::Home];
    if needs_you > 0 {
        rows.push(SidebarRootRow::NeedsYou);
    }
    rows.push(SidebarRootRow::Chats);
    if drafts > 0 {
        rows.push(SidebarRootRow::Drafts);
    }
    rows.push(SidebarRootRow::Devices);
    rows
}

/// The Teams label's + menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SidebarTeamsMenuItem {
    NewTeam,
    JoinTeam,
}

pub fn sidebar_teams_menu() -> Vec<SidebarTeamsMenuItem> {
    vec![SidebarTeamsMenuItem::NewTeam, SidebarTeamsMenuItem::JoinTeam]
}

/// Case-insensitive name, ties by id (the TS `byName`).
fn by_name(a_name: &str, a_id: &str, b_name: &str, b_id: &str) -> Ordering {
    a_name.to_lowercase().cmp(&b_name.to_lowercase()).then_with(|| a_id.cmp(b_id))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarDeviceInput {
    pub id: String,
    pub name: String,
    /// The viewer's own device (not a teammate's shared server).
    pub mine: bool,
    pub online: bool,
    pub shared_team_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarDeviceRow {
    pub id: String,
    pub mine: bool,
    /// Offline: drawn muted.
    pub muted: bool,
    /// A teammate's server: the team whose mark it wears; `None` on the viewer's own.
    pub team_id: Option<String>,
}

/// The Devices row's children: the viewer's own by name, then teammates'
/// servers shared with one of the viewer's teams, by name.
pub fn sidebar_device_rows(devices: &[SidebarDeviceInput], viewer_team_ids: &[String]) -> Vec<SidebarDeviceRow> {
    let order = |a: &&SidebarDeviceInput, b: &&SidebarDeviceInput| by_name(&a.name, &a.id, &b.name, &b.id);
    let mut mine: Vec<&SidebarDeviceInput> = devices.iter().filter(|device| device.mine).collect();
    mine.sort_by(order);
    let mut shared: Vec<(&SidebarDeviceInput, String)> = devices
        .iter()
        .filter(|device| !device.mine)
        .filter_map(|device| {
            let team = viewer_team_ids.iter().find(|id| device.shared_team_ids.contains(id))?;
            Some((device, team.clone()))
        })
        .collect();
    shared.sort_by(|(a, _), (b, _)| order(a, b));
    mine.into_iter()
        .map(|device| SidebarDeviceRow { id: device.id.clone(), mine: true, muted: !device.online, team_id: None })
        .chain(shared.into_iter().map(|(device, team_id)| SidebarDeviceRow {
            id: device.id.clone(),
            mine: false,
            muted: !device.online,
            team_id: Some(team_id),
        }))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarTeamInput {
    pub id: String,
    pub name: String,
    pub kind: String,
}

/// The viewer's member teams by name; the personal team never shows.
pub fn sidebar_teams(teams: &[SidebarTeamInput]) -> Vec<String> {
    let mut shown: Vec<&SidebarTeamInput> = teams.iter().filter(|team| team.kind != "personal").collect();
    shown.sort_by(|a, b| by_name(&a.name, &a.id, &b.name, &b.id));
    shown.into_iter().map(|team| team.id.clone()).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarNewKind {
    Board,
    Agent,
    App,
    Session,
}

impl SidebarNewKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SidebarNewKind::Board => "board",
            SidebarNewKind::Agent => "agent",
            SidebarNewKind::App => "app",
            SidebarNewKind::Session => "session",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SidebarMenuId {
    New,
    NewFolder,
    Settings,
}

impl SidebarMenuId {
    pub fn as_str(self) -> &'static str {
        match self {
            SidebarMenuId::New => "new",
            SidebarMenuId::NewFolder => "new-folder",
            SidebarMenuId::Settings => "settings",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarMenuItem {
    pub id: SidebarMenuId,
    pub children: Vec<SidebarNewKind>,
}

/// A team row's `…` / right-click menu (settings-nav.json `newEntries`:
/// board, agent and app are owner-only).
pub fn sidebar_team_menu(owner: bool) -> Vec<SidebarMenuItem> {
    let kinds = if owner {
        vec![SidebarNewKind::Board, SidebarNewKind::Agent, SidebarNewKind::App, SidebarNewKind::Session]
    } else {
        vec![SidebarNewKind::Session]
    };
    vec![
        SidebarMenuItem { id: SidebarMenuId::New, children: kinds },
        SidebarMenuItem { id: SidebarMenuId::NewFolder, children: Vec::new() },
        SidebarMenuItem { id: SidebarMenuId::Settings, children: Vec::new() },
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarItemKind {
    Board,
    Agent,
    App,
    Session,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarTeamItem {
    pub id: String,
    pub kind: SidebarItemKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarSessionInput {
    pub id: String,
    pub team_id: String,
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarBoardInput {
    pub id: String,
    pub sort_order: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarAgentInput {
    pub id: String,
    pub sort_order: f64,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarAppInput {
    pub id: String,
    pub name: String,
}

fn is_team_root(session: &SidebarSessionInput, team_id: &str) -> bool {
    session.team_id == team_id && session.parent_id.is_none()
}

fn by_sort(a_sort: f64, a_id: &str, b_sort: f64, b_id: &str) -> Ordering {
    match a_sort.partial_cmp(&b_sort) {
        Some(Ordering::Equal) | None => a_id.cmp(b_id),
        Some(order) => order,
    }
}

/// A team's targets in DEFAULT order: boards by sortOrder, agents (never an
/// Assistant) by sortOrder, apps by name, then the team's root sessions the
/// member placed (an item entry), in entry order. Feeds `sidebar_tree`.
pub fn sidebar_team_items(
    team_id: &str,
    boards: &[SidebarBoardInput],
    agents: &[SidebarAgentInput],
    apps: &[SidebarAppInput],
    sessions: &[SidebarSessionInput],
    entries: &[SidebarEntry],
) -> Vec<SidebarTeamItem> {
    let item = |id: &str, kind| SidebarTeamItem { id: id.to_string(), kind };
    let mut boards: Vec<&SidebarBoardInput> = boards.iter().collect();
    boards.sort_by(|a, b| by_sort(a.sort_order, &a.id, b.sort_order, &b.id));
    let mut agents: Vec<&SidebarAgentInput> = agents.iter().filter(|agent| agent.role == "agent").collect();
    agents.sort_by(|a, b| by_sort(a.sort_order, &a.id, b.sort_order, &b.id));
    let mut apps: Vec<&SidebarAppInput> = apps.iter().collect();
    apps.sort_by(|a, b| by_name(&a.name, &a.id, &b.name, &b.id));
    // A JS `Map` keeps the LAST session of a duplicated id.
    let by_id: HashMap<&str, &SidebarSessionInput> = sessions.iter().map(|session| (session.id.as_str(), session)).collect();
    let mut placed: Vec<&SidebarEntry> =
        entries.iter().filter(|entry| entry.kind == SidebarEntryKind::Item && entry.target_id.is_some()).collect();
    placed.sort_by(|a, b| by_sort(a.sort_order, &a.id, b.sort_order, &b.id));
    let mut seen: HashSet<&str> = HashSet::new();
    let placed_sessions = placed
        .into_iter()
        .filter_map(|entry| by_id.get(entry.target_id.as_deref()?).copied())
        .filter(|session| is_team_root(session, team_id))
        .filter(|session| seen.insert(session.id.as_str()));
    boards
        .into_iter()
        .map(|board| item(&board.id, SidebarItemKind::Board))
        .chain(agents.into_iter().map(|agent| item(&agent.id, SidebarItemKind::Agent)))
        .chain(apps.into_iter().map(|app| item(&app.id, SidebarItemKind::App)))
        .chain(placed_sessions.map(|session| item(&session.id, SidebarItemKind::Session)))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarTreeRowKind {
    Team,
    Folder,
    Item,
}

/// One List row of a team: the team row, then its nodes one level deeper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarTreeRow {
    pub kind: SidebarTreeRowKind,
    /// Team id, folder entry id or item target id.
    pub id: String,
    /// 0 | 1 | 2.
    pub depth: u8,
    pub group: bool,
    /// Folders only.
    pub name: Option<String>,
    pub open: bool,
    pub count: usize,
}

/// The team row (count = its top-level nodes) and, while open, its
/// `sidebar_rows` one level deeper.
pub fn sidebar_team_rows(team_id: &str, open: bool, rows: &[SidebarRow]) -> Vec<SidebarTreeRow> {
    let count = rows.iter().filter(|row| row.depth == 0).count();
    let team = SidebarTreeRow {
        kind: SidebarTreeRowKind::Team,
        id: team_id.to_string(),
        depth: 0,
        group: true,
        name: None,
        open,
        count,
    };
    if !open {
        return vec![team];
    }
    std::iter::once(team)
        .chain(rows.iter().map(|row| SidebarTreeRow {
            kind: match row.kind {
                SidebarEntryKind::Folder => SidebarTreeRowKind::Folder,
                SidebarEntryKind::Item => SidebarTreeRowKind::Item,
            },
            id: row.id.clone(),
            depth: row.depth + 1,
            group: row.group,
            name: row.name.clone(),
            open: row.open,
            count: row.count,
        }))
        .collect()
}

/// A root session of the team dropped on its List: appended to the items,
/// then a plain `sidebar_move`. `None` = refused (another team, a child).
pub fn sidebar_drop_session(
    team_id: &str,
    items: &[String],
    entries: &[SidebarEntry],
    session: &SidebarSessionInput,
    mv: &SidebarMove,
    new_folder_id: &str,
) -> Option<SidebarWrites> {
    if !is_team_root(session, team_id) || mv.id != session.id {
        return None;
    }
    let mut items = items.to_vec();
    if !items.contains(&session.id) {
        items.push(session.id.clone());
    }
    sidebar_move(&items, entries, mv, new_folder_id)
}
