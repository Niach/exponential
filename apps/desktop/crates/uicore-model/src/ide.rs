//! #EXP-1725: what the IDE's shell regions show, a pure projection of the
//! host's state, locked by `packages/uicore/fixtures/ide-sidebar.json` (its
//! `$comment` is the spec, `packages/uicore/src/ide.ts` the TypeScript
//! reference). The document binds the result under `/shell/ide`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

const SOURCE: &str = include_str!("../../../../../packages/uicore/fixtures/ide-sidebar.json");

/// #FEED-232 (`nav-badges.json`): a nav row's count badge tone; no dots.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IdeTone {
    Attention,
    #[default]
    Muted,
}

/// A count the host computed (`chatsBadge`): 0 = no badge.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeCount {
    pub count: usize,
    pub tone: IdeTone,
}

// The badge rules below are `domain::nav_badges`' (this crate sits below
// `domain`); tests/ide.rs replays nav-badges.json over them.

/// The badge's label: `None` at zero, `99+` past 99 (`labelCases`).
pub fn count_label(count: usize) -> Option<String> {
    match count {
        0 => None,
        1..=99 => Some(count.to_string()),
        _ => Some("99+".into()),
    }
}

/// An agent row's, a placed session's or the Assistant's badge (`rowCases`):
/// its unread, at least 1 while it needs you, attention when it does.
pub fn row_badge(unread: usize, needs_you: bool) -> IdeCount {
    if needs_you {
        IdeCount { count: unread.max(1), tone: IdeTone::Attention }
    } else {
        IdeCount { count: unread, tone: IdeTone::Muted }
    }
}

/// A folded folder's badge over the rows it hides (`sumCases`).
pub fn sum_badges(rows: impl IntoIterator<Item = IdeCount>) -> IdeCount {
    let mut out = IdeCount::default();
    for row in rows.into_iter().filter(|row| row.count > 0) {
        out.count += row.count;
        if row.tone == IdeTone::Attention {
            out.tone = IdeTone::Attention;
        }
    }
    out
}

/// A team row's badge (`teamCases`): its share of Needs you, and while folded
/// its hidden rows' muted counts.
pub fn team_badge(needs_you: usize, open: bool, rows: impl IntoIterator<Item = IdeCount>) -> IdeCount {
    let share = IdeCount { count: needs_you, tone: IdeTone::Attention };
    sum_badges(std::iter::once(share).chain(rows.into_iter().filter(|row| !open && row.tone == IdeTone::Muted)))
}

/// What a row binds: the label and the tone, both `None` without a count.
fn bound(badge: IdeCount) -> (Option<String>, Option<IdeTone>) {
    let count = count_label(badge.count);
    let tone = count.is_some().then_some(badge.tone);
    (count, tone)
}

/// #EXP-2027 (domain-contract sidebar-tree.json): one item of a team's mixed
/// list: a board, an agent (never an Assistant), an app or a root session the
/// member placed, in the host's default order (`sidebarTeamItems`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeItemInput {
    /// `board` | `agent` | `app` | `session`.
    pub kind: String,
    pub id: String,
    pub name: String,
    pub icon: String,
    /// An app's slug (its row lights on it too).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    /// An agent or a placed session: needs you.
    #[serde(default)]
    pub needs_you: bool,
    /// An agent or a placed session: its unread agent messages.
    #[serde(default)]
    pub unread: usize,
    /// A session: every session of its tree (its row lights on any).
    #[serde(default)]
    pub session_ids: Vec<String>,
}

/// #EXP-2027: one team of the sidebar (`sidebarTeams` order).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeTeamInput {
    pub id: String,
    pub name: String,
    /// The team mark the host paints (its image, else the team glyph).
    pub mark: String,
    /// Device-local expanded flag (`openKeys.team`; no flag = open).
    #[serde(default = "yes")]
    pub open: bool,
    /// Owners get New › Board | Agent | App and the board gears.
    #[serde(default)]
    pub owner: bool,
    /// The team's share of Needs you (nav-badges.json `needsYouCounts`).
    #[serde(default)]
    pub needs_you: usize,
    pub items: Vec<IdeItemInput>,
    /// The member's synced `sidebar_layout` rows of the team's `team` list.
    #[serde(default)]
    pub layout: Vec<IdeLayoutEntry>,
}

fn yes() -> bool {
    true
}

/// #EXP-2027: one device under Devices, already in `sidebarDeviceRows`
/// order (the viewer's own, then teammates' shared servers).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeDeviceInput {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub online: bool,
    /// A teammate's server: the mark of the team it is shared with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_mark: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeActiveInput {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub session_ids: Vec<String>,
    /// #EXP-2027: Active spans every team: each row wears its team's mark.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_mark: Option<String>,
    /// #EXP-2027: the chat's team (a drop into a team checks it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
}

/// #EXP-1616: one open PR of an Exponential repository, already placed in
/// its repo's tree by the host (`domain::repo_page::repo_pr_tree`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdePrInput {
    pub id: String,
    /// `#<number>`.
    pub identifier: String,
    pub title: String,
    /// `<head> → <base>`.
    pub subtitle: String,
    pub depth: usize,
    /// The chat (root session) that opened it: its row lights while that
    /// chat is open (a PR has no page of its own).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

/// #EXP-1616: an Exponential (`ecode`) repository and its live PR tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeRepoInput {
    pub id: String,
    pub name: String,
    pub prs: Vec<IdePrInput>,
}

/// #EXP-2124: the viewer's Assistant, the sidebar's first row.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeAssistantInput {
    pub id: String,
    pub name: String,
    /// Its conversation's unread agent messages.
    #[serde(default)]
    pub unread: usize,
    #[serde(default)]
    pub needs_you: bool,
}

/// What the host knows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeSidebarInput {
    /// The open screen as `<kind>` or `<kind>:<id>`.
    pub screen: String,
    /// #EXP-2124: the viewer's Assistant (none = no row).
    #[serde(default)]
    pub assistant: Option<IdeAssistantInput>,
    pub needs_you: usize,
    pub drafts: usize,
    pub chats: IdeCount,
    /// #EXP-2027: the Devices row's children, and whether it is open.
    #[serde(default)]
    pub devices: Vec<IdeDeviceInput>,
    #[serde(default = "yes")]
    pub devices_open: bool,
    /// #EXP-2027: one entry per team (`sidebarTeams`).
    #[serde(default)]
    pub teams: Vec<IdeTeamInput>,
    pub active: Vec<IdeActiveInput>,
    #[serde(default)]
    pub repos: Vec<IdeRepoInput>,
    /// #EXP-1832: the folders this device keeps folded (none = open).
    #[serde(default)]
    pub closed: Vec<String>,
    /// #EXP-1832: the folder whose name is being edited in place.
    #[serde(default)]
    pub editing_folder: Option<String>,
}

/// #EXP-1832: one synced `sidebar_layout` row (`domain::sidebar_folders::
/// SidebarEntry`'s shape; this crate sits below `domain`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeLayoutEntry {
    pub id: String,
    /// `folder` | `item`.
    pub kind: String,
    pub target_id: Option<String>,
    pub parent_id: Option<String>,
    pub name: Option<String>,
    pub sort_order: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeNavRow {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub selected: bool,
    pub count: Option<String>,
    pub tone: Option<IdeTone>,
}

/// One entry of a team menu's New submenu (sidebar-tree.json `teamMenu`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeMenuEntry {
    pub label: String,
    pub value: String,
    pub icon: String,
}

/// #EXP-2027: one row of a team's List: the team row (depth 0), a folder
/// (depth 1, never selected, its press folds it) or an item (depth 1, or 2
/// inside a folder).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeRow {
    /// `team` | `folder` | `item`.
    pub kind: String,
    /// An item's kind (`board` | `agent` | `app` | `session`); `None` on a
    /// team or folder row.
    pub item: Option<String>,
    pub id: String,
    /// 0 = the team row, 1 = its top level, 2 = inside a folder.
    pub depth: u8,
    pub group: bool,
    /// A team's or folder's expanded flag (false on an item).
    pub open: bool,
    pub title: String,
    pub icon: String,
    pub selected: bool,
    pub count: Option<String>,
    pub tone: Option<IdeTone>,
    /// The row's hover gear: agents and apps for every member, boards for
    /// owners.
    pub gear: bool,
    /// A FOLDED folder's item count; `None` otherwise.
    pub meta: Option<String>,
    /// A team's or folder's fold glyph: `ui-chevron-down` open,
    /// `ui-chevron-right` folded.
    pub fold_icon: Option<String>,
    /// The folder's name is an Input in place.
    pub editing: bool,
    /// The team row's New submenu (empty elsewhere).
    pub new_items: Vec<IdeMenuEntry>,
}

impl IdeRow {
    fn item(kind: &str, id: &str, title: &str, icon: &str, selected: bool, count: Option<String>, tone: Option<IdeTone>, gear: bool) -> Self {
        Self { kind: "item".into(), item: Some(kind.into()), id: id.into(), depth: 1, group: false, open: false, title: title.into(), icon: icon.into(), selected, count, tone, gear, meta: None, fold_icon: None, editing: false, new_items: Vec::new() }
    }
}

/// The folder name a folder without one shows (`sidebar-folders.json`).
pub const SIDEBAR_FOLDER_DEFAULT_NAME: &str = "New folder";
const FOLDER_ICON: &str = "ui-folder";

fn fold_icon(open: bool) -> Option<String> {
    Some(if open { "ui-chevron-down" } else { "ui-chevron-right" }.into())
}

/// One node of a team's tree: an item (its target id) or a folder with
/// its item ids, in order (`sidebarTree`).
enum Node {
    Item(String),
    Folder { id: String, name: String, children: Vec<String> },
}

fn compare_entries(a: &IdeLayoutEntry, b: &IdeLayoutEntry) -> std::cmp::Ordering {
    a.sort_order.partial_cmp(&b.sort_order).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.id.cmp(&b.id))
}

/// `sidebarTree` (domain-contract `sidebar-folders.ts`, Rust
/// `domain::sidebar_folders::sidebar_tree`): placed nodes first by
/// sortOrder (ties by entry id), then every item with no entry in default
/// order; an entry for a hidden target is skipped, a folder holds items only
/// and shows only while it holds a visible one (or while it is being named:
/// sidebar-tree.json New folder).
fn sidebar_tree(items: &[&str], entries: &[IdeLayoutEntry], editing: Option<&str>) -> Vec<Node> {
    let mut sorted: Vec<&IdeLayoutEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| compare_entries(a, b));
    let mut folders: Vec<&IdeLayoutEntry> = Vec::new();
    let mut placed: Vec<&IdeLayoutEntry> = Vec::new();
    for entry in sorted {
        if entry.kind == "folder" {
            if !folders.iter().any(|f| f.id == entry.id) {
                folders.push(entry);
            }
        } else if let Some(target) = entry.target_id.as_deref() {
            if items.contains(&target) && !placed.iter().any(|p| p.target_id.as_deref() == Some(target)) {
                placed.push(entry);
            }
        }
    }
    let in_folder = |e: &IdeLayoutEntry| -> Option<String> { e.parent_id.clone().filter(|p| folders.iter().any(|f| f.id == *p)) };
    let mut top: Vec<&IdeLayoutEntry> = placed.iter().copied().filter(|e| in_folder(*e).is_none()).collect();
    top.extend(folders.iter().copied().filter(|f| editing == Some(f.id.as_str()) || placed.iter().any(|e| in_folder(*e).as_deref() == Some(f.id.as_str()))));
    top.sort_by(|a, b| compare_entries(a, b));
    let mut nodes: Vec<Node> = top
        .into_iter()
        .map(|entry| {
            if entry.kind == "folder" {
                // `placed` is already in entry order.
                let children = placed.iter().filter(|e| in_folder(**e).as_deref() == Some(entry.id.as_str())).filter_map(|e| e.target_id.clone()).collect();
                Node::Folder { id: entry.id.clone(), name: entry.name.clone().unwrap_or_else(|| SIDEBAR_FOLDER_DEFAULT_NAME.into()), children }
            } else {
                Node::Item(entry.target_id.clone().unwrap_or_default())
            }
        })
        .collect();
    nodes.extend(items.iter().filter(|item| !placed.iter().any(|e| e.target_id.as_deref() == Some(**item))).map(|item| Node::Item(item.to_string())));
    nodes
}

/// `sidebarRows` + `sidebarTeamRows` over one team's item rows: a folder
/// row heads its items one level deeper; a folded folder's items are left
/// out (its `meta` = their count). Every row sits one level under the team.
pub fn folder_rows(rows: Vec<IdeRow>, counts: &HashMap<String, IdeCount>, entries: &[IdeLayoutEntry], closed: &[String], editing: Option<&str>) -> Vec<IdeRow> {
    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    let tree = sidebar_tree(&ids, entries, editing);
    let take = |id: &str, depth: u8| rows.iter().find(|r| r.id == id).cloned().map(|r| IdeRow { depth, ..r });
    let mut out = Vec::new();
    for node in tree {
        match node {
            Node::Item(id) => out.extend(take(&id, 1)),
            Node::Folder { id, name, children } => {
                let open = !closed.contains(&id);
                let hidden = children.iter().filter(|_| !open).filter_map(|child| counts.get(child).copied());
                let (count, tone) = bound(sum_badges(hidden));
                out.push(IdeRow {
                    kind: "folder".into(),
                    item: None,
                    depth: 1,
                    group: true,
                    open,
                    title: name,
                    icon: FOLDER_ICON.into(),
                    selected: false,
                    count,
                    tone,
                    gear: false,
                    meta: (!open).then(|| children.len().to_string()),
                    fold_icon: fold_icon(open),
                    editing: editing == Some(id.as_str()),
                    new_items: Vec::new(),
                    id,
                });
                if open {
                    out.extend(children.iter().filter_map(|child| take(child, 2)));
                }
            }
        }
    }
    out
}

/// #EXP-2027: the Devices row (depth 0, no nav target, its press folds it,
/// its hover + adds a device) and, while open, one row per device (depth 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeDeviceRow {
    /// `devices` | `device`.
    pub kind: String,
    pub id: String,
    pub depth: u8,
    pub group: bool,
    pub open: bool,
    pub title: String,
    pub icon: String,
    pub selected: bool,
    /// Offline: drawn muted.
    pub muted: bool,
    /// The trailing glyph: the viewer's own online device's green dot.
    pub mark: Option<String>,
    /// A teammate's server: its team's mark, on the device glyph
    /// (nav-badges.json THE TEAM MARK ON A ROW).
    pub team_mark: Option<String>,
    pub fold_icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeActiveRow {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub selected: bool,
    pub team_mark: Option<String>,
}

/// #EXP-2027: one team's List (the team row first).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeTeamList {
    pub id: String,
    pub rows: Vec<IdeRow>,
}

/// #EXP-1616: one row of the Pull requests tree: a repository at depth 0,
/// its PRs under it (their tree depth + 1). Every row opens `repo`'s page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdePrRow {
    pub id: String,
    pub repo: String,
    pub title: String,
    pub identifier: Option<String>,
    pub subtitle: Option<String>,
    pub icon: Option<String>,
    pub depth: usize,
    pub selected: bool,
}

/// What the document binds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeSidebar {
    pub nav: Vec<IdeNavRow>,
    pub devices: Vec<IdeDeviceRow>,
    pub teams: Vec<IdeTeamList>,
    pub active: Vec<IdeActiveRow>,
    pub prs: Vec<IdePrRow>,
}

fn copy(id: &str) -> String {
    static COPY: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let copy = COPY.get_or_init(|| serde_json::from_str::<serde_json::Value>(SOURCE).expect("ide-sidebar.json")["copy"].clone());
    copy[id].as_str().unwrap_or_default().to_string()
}

fn nav(id: &str, icon: &str, selected: bool, count: usize, tone: IdeTone) -> IdeNavRow {
    let (count, tone) = bound(IdeCount { count, tone });
    IdeNavRow { id: id.into(), title: copy(id), icon: icon.into(), selected, count, tone }
}

/// THE New list (#EXP-2124, domain-contract sidebar-menus.json `newKinds`:
/// a chat for every member, then agent, app and board for owners), the team
/// menu's New submenu and the new-tab menu's team submenus.
pub fn new_items(owner: bool) -> Vec<IdeMenuEntry> {
    let entry = |value: &str, icon: &str| IdeMenuEntry { label: copy(value), value: value.into(), icon: icon.into() };
    let mut out = vec![entry("session", "action-chat")];
    if owner {
        out.push(entry("agent", "settings-agents"));
        out.push(entry("app", "nav-apps"));
        out.push(entry("board", "nav-boards"));
    }
    out
}

pub fn ide_sidebar(input: &IdeSidebarInput) -> IdeSidebar {
    let screen = input.screen.as_str();
    let editing = input.editing_folder.as_deref();
    let on = |kind: &str, id: &str| screen.strip_prefix(kind).and_then(|s| s.strip_prefix(':')) == Some(id);
    let mut rows = Vec::new();
    if let Some(assistant) = &input.assistant {
        let (count, tone) = bound(row_badge(assistant.unread, assistant.needs_you));
        rows.push(IdeNavRow { id: "assistant".into(), title: assistant.name.clone(), icon: "brand-exponential".into(), selected: on("agent", &assistant.id), count, tone });
    }
    rows.push(nav("home", "nav-home", screen == "home", 0, IdeTone::Muted));
    if input.needs_you > 0 || screen == "needs-you" {
        rows.push(nav("needs-you", "nav-needs-you", screen == "needs-you", input.needs_you, IdeTone::Attention));
    }
    rows.push(nav("chats", "nav-chats", screen == "chats", input.chats.count, input.chats.tone));
    if input.drafts > 0 || screen == "drafts" {
        rows.push(nav("drafts", "nav-drafts", screen == "drafts", input.drafts, IdeTone::Muted));
    }
    let mut devices = vec![IdeDeviceRow {
        kind: "devices".into(),
        id: "devices".into(),
        depth: 0,
        group: true,
        open: input.devices_open,
        title: copy("devices"),
        icon: "nav-devices".into(),
        selected: false,
        muted: false,
        mark: None,
        team_mark: None,
        fold_icon: fold_icon(input.devices_open),
    }];
    if input.devices_open {
        devices.extend(input.devices.iter().map(|d| IdeDeviceRow {
            kind: "device".into(),
            id: d.id.clone(),
            depth: 1,
            group: false,
            open: false,
            title: d.name.clone(),
            icon: d.icon.clone(),
            selected: on("device", &d.id) || on("device-settings", &d.id),
            muted: !d.online,
            mark: (d.online && d.team_mark.is_none()).then(|| "ide:dot:green".into()),
            team_mark: d.team_mark.clone(),
            fold_icon: None,
        }));
    }
    let teams = input
        .teams
        .iter()
        .map(|team| {
            let counts: HashMap<String, IdeCount> = team
                .items
                .iter()
                .filter(|item| matches!(item.kind.as_str(), "agent" | "session"))
                .map(|item| (item.id.clone(), row_badge(item.unread, item.needs_you)))
                .collect();
            let items = team
                .items
                .iter()
                .map(|item| {
                    let (count, tone) = bound(counts.get(&item.id).copied().unwrap_or_default());
                    let selected = match item.kind.as_str() {
                        "app" => on("app", &item.id) || item.slug.as_deref().is_some_and(|slug| on("app", slug)),
                        "session" => on("chat", &item.id) || on("session", &item.id) || item.session_ids.iter().any(|id| on("session", id)),
                        kind => on(kind, &item.id),
                    };
                    let gear = match item.kind.as_str() {
                        "agent" | "app" => true,
                        "board" => team.owner,
                        _ => false,
                    };
                    IdeRow::item(&item.kind, &item.id, &item.name, &item.icon, selected, count, tone, gear)
                })
                .collect();
            let nodes = folder_rows(items, &counts, &team.layout, &input.closed, editing);
            let (count, tone) = bound(team_badge(team.needs_you, team.open, counts.values().copied()));
            let head = IdeRow {
                kind: "team".into(),
                item: None,
                id: team.id.clone(),
                depth: 0,
                group: true,
                open: team.open,
                title: team.name.clone(),
                icon: team.mark.clone(),
                selected: false,
                count,
                tone,
                gear: false,
                meta: None,
                fold_icon: fold_icon(team.open),
                editing: false,
                new_items: new_items(team.owner),
            };
            let mut rows = vec![head];
            if team.open {
                rows.extend(nodes);
            }
            IdeTeamList { id: team.id.clone(), rows }
        })
        .collect();
    IdeSidebar {
        nav: rows,
        devices,
        teams,
        active: input
            .active
            .iter()
            .map(|a| IdeActiveRow { id: a.id.clone(), title: a.title.clone(), icon: a.icon.clone(), selected: on("chat", &a.id) || a.session_ids.iter().any(|id| on("session", id)), team_mark: a.team_mark.clone() })
            .collect(),
        prs: input
            .repos
            .iter()
            .filter(|r| !r.prs.is_empty())
            .flat_map(|r| {
                let head = IdePrRow { id: r.id.clone(), repo: r.id.clone(), title: r.name.clone(), identifier: None, subtitle: None, icon: Some("ui-repository".into()), depth: 0, selected: on("repo", &r.id) };
                std::iter::once(head).chain(r.prs.iter().map(|p| IdePrRow {
                    id: p.id.clone(),
                    repo: r.id.clone(),
                    title: p.title.clone(),
                    identifier: Some(p.identifier.clone()),
                    subtitle: Some(p.subtitle.clone()),
                    icon: None,
                    depth: p.depth + 1,
                    selected: p.session.as_deref().is_some_and(|id| on("chat", id) || on("session", id)),
                }))
            })
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeTabInput {
    pub key: String,
    pub title: String,
    pub icon: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeTab {
    pub value: String,
    pub label: String,
    pub icon: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeTabs {
    pub tabs: Vec<IdeTab>,
    pub active_tab: Option<String>,
}

/// The tab bar: every open tab in tab order, the active one by key.
pub fn ide_tabs(tabs: &[IdeTabInput], active: Option<&str>) -> IdeTabs {
    IdeTabs {
        tabs: tabs.iter().map(|t| IdeTab { value: t.key.clone(), label: t.title.clone(), icon: t.icon.clone() }).collect(),
        active_tab: active.filter(|a| tabs.iter().any(|t| t.key == *a)).map(str::to_string),
    }
}

/// Which edge of the IDE's window a strip sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IdeEdge {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeTabPlacement {
    /// The tab bar's edge.
    pub placement: IdeEdge,
    /// The side the +N menu opens on: away from that edge.
    pub more_side: IdeEdge,
}

/// Where the tab bar sits: on top in the standalone IDE, at the bottom
/// inside Exponential Desktop (`ede`).
pub fn ide_tab_placement(ede: bool) -> IdeTabPlacement {
    if ede {
        IdeTabPlacement { placement: IdeEdge::Bottom, more_side: IdeEdge::Top }
    } else {
        IdeTabPlacement { placement: IdeEdge::Top, more_side: IdeEdge::Bottom }
    }
}
