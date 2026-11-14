//! #EXP-1725: what the IDE's shell regions show, a pure projection of the
//! host's state, locked by `packages/uicore/fixtures/ide-sidebar.json` (its
//! `$comment` is the spec, `packages/uicore/src/ide.ts` the TypeScript
//! reference). The document binds the result under `/shell/ide`.

use serde::{Deserialize, Serialize};

const SOURCE: &str = include_str!("../../../../../packages/uicore/fixtures/ide-sidebar.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IdeDot {
    Amber,
    Green,
    Primary,
}

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

/// The badge's label: `None` at zero, `99+` past 99 (`labelCases`).
pub fn count_label(count: usize) -> Option<String> {
    match count {
        0 => None,
        1..=99 => Some(count.to_string()),
        _ => Some("99+".into()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdePinInput {
    pub id: String,
    pub identifier: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeAgentInput {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub needs_you: bool,
    /// The agent conversation's unread agent messages.
    pub unread: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeAppInput {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub icon: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeBoardInput {
    pub id: String,
    pub name: String,
    pub icon: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeActiveInput {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub session_ids: Vec<String>,
    pub unread: bool,
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

/// What the host knows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeSidebarInput {
    /// The open screen as `<kind>` or `<kind>:<id>`.
    pub screen: String,
    pub needs_you: usize,
    pub drafts: usize,
    pub chats: IdeCount,
    pub owner: bool,
    pub pins: Vec<IdePinInput>,
    pub agents: Vec<IdeAgentInput>,
    pub apps: Vec<IdeAppInput>,
    pub boards: Vec<IdeBoardInput>,
    pub active: Vec<IdeActiveInput>,
    #[serde(default)]
    pub repos: Vec<IdeRepoInput>,
    /// #EXP-1832: the member's `sidebar_layout` rows per section (none =
    /// default order).
    #[serde(default)]
    pub layout: IdeLayout,
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

/// #EXP-1832: the layout rows of each foldable section.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IdeLayout {
    #[serde(default)]
    pub board: Vec<IdeLayoutEntry>,
    #[serde(default)]
    pub agent: Vec<IdeLayoutEntry>,
    #[serde(default)]
    pub app: Vec<IdeLayoutEntry>,
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

/// #EXP-1832: a Boards / Agents / Apps row: a target (`item`) or a folder
/// (`group`, never selected, no badge or gear; its press folds it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeRow {
    /// `item` | `folder`.
    pub kind: String,
    pub id: String,
    /// 0 = top level, 1 = inside a folder.
    pub depth: u8,
    pub group: bool,
    /// A folder's expanded flag (false on an item).
    pub open: bool,
    pub title: String,
    pub icon: String,
    pub selected: bool,
    pub count: Option<String>,
    pub tone: Option<IdeTone>,
    pub gear: bool,
    /// A FOLDED folder's item count; `None` otherwise.
    pub meta: Option<String>,
    /// A folder's fold glyph: `ui-chevron-down` open, `ui-chevron-right` folded.
    pub fold_icon: Option<String>,
    /// The folder's name is an Input in place.
    pub editing: bool,
}

impl IdeRow {
    /// A top-level target row (the fold fields at rest).
    fn item(id: &str, title: &str, icon: &str, selected: bool, count: Option<String>, tone: Option<IdeTone>, gear: bool) -> Self {
        Self { kind: "item".into(), id: id.into(), depth: 0, group: false, open: false, title: title.into(), icon: icon.into(), selected, count, tone, gear, meta: None, fold_icon: None, editing: false }
    }
}

/// The folder name a folder without one shows (`sidebar-folders.json`).
pub const SIDEBAR_FOLDER_DEFAULT_NAME: &str = "New folder";
const FOLDER_ICON: &str = "ui-folder";

/// One node of a section's tree: an item (its target id) or a folder with
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
/// and shows only while it holds a visible one.
fn sidebar_tree(items: &[&str], entries: &[IdeLayoutEntry]) -> Vec<Node> {
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
    top.extend(folders.iter().copied().filter(|f| placed.iter().any(|e| in_folder(*e).as_deref() == Some(f.id.as_str()))));
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

/// `sidebarRows` over the section's rows: a folder row heads its items one
/// level deeper; a folded folder's items are left out (its `meta` = their
/// count).
pub fn folder_rows(rows: Vec<IdeRow>, entries: &[IdeLayoutEntry], closed: &[String], editing: Option<&str>) -> Vec<IdeRow> {
    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    let tree = sidebar_tree(&ids, entries);
    let take = |id: &str, depth: u8| rows.iter().find(|r| r.id == id).cloned().map(|r| IdeRow { depth, ..r });
    let mut out = Vec::new();
    for node in tree {
        match node {
            Node::Item(id) => out.extend(take(&id, 0)),
            Node::Folder { id, name, children } => {
                let open = !closed.contains(&id);
                out.push(IdeRow {
                    kind: "folder".into(),
                    depth: 0,
                    group: true,
                    open,
                    title: name,
                    icon: FOLDER_ICON.into(),
                    selected: false,
                    count: None,
                    tone: None,
                    gear: false,
                    meta: (!open).then(|| children.len().to_string()),
                    fold_icon: Some(if open { "ui-chevron-down" } else { "ui-chevron-right" }.into()),
                    editing: editing == Some(id.as_str()),
                    id,
                });
                if open {
                    out.extend(children.iter().filter_map(|child| take(child, 1)));
                }
            }
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdePinRow {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeActiveRow {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub selected: bool,
    pub unread: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeAgents {
    pub add: bool,
    pub rows: Vec<IdeRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeApps {
    pub rows: Vec<IdeRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeBoards {
    pub rows: Vec<IdeRow>,
    pub empty: bool,
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
    pub pins: Vec<IdePinRow>,
    pub agents: IdeAgents,
    pub apps: IdeApps,
    pub boards: IdeBoards,
    pub active: Vec<IdeActiveRow>,
    pub prs: Vec<IdePrRow>,
}

fn copy(id: &str) -> String {
    static COPY: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let copy = COPY.get_or_init(|| serde_json::from_str::<serde_json::Value>(SOURCE).expect("ide-sidebar.json")["copy"].clone());
    copy[id].as_str().unwrap_or_default().to_string()
}

fn nav(id: &str, icon: &str, selected: bool, count: usize, tone: IdeTone) -> IdeNavRow {
    let count = count_label(count);
    let tone = count.is_some().then_some(tone);
    IdeNavRow { id: id.into(), title: copy(id), icon: icon.into(), selected, count, tone }
}

/// An agent row's badge (`rowBadge`): its unread, at least 1 while it needs
/// you, attention when it does. Busy is no badge.
pub fn agent_count(unread: usize, needs_you: bool) -> IdeCount {
    if needs_you {
        IdeCount { count: unread.max(1), tone: IdeTone::Attention }
    } else {
        IdeCount { count: unread, tone: IdeTone::Muted }
    }
}

/// An agent's dot: needs you before busy.
pub fn agent_dot(needs_you: bool, busy: bool) -> Option<IdeDot> {
    if needs_you {
        Some(IdeDot::Amber)
    } else if busy {
        Some(IdeDot::Green)
    } else {
        None
    }
}

pub fn ide_sidebar(input: &IdeSidebarInput) -> IdeSidebar {
    let screen = input.screen.as_str();
    let editing = input.editing_folder.as_deref();
    let folders = |rows: Vec<IdeRow>, entries: &[IdeLayoutEntry]| folder_rows(rows, entries, &input.closed, editing);
    let on = |kind: &str, id: &str| screen.strip_prefix(kind).and_then(|s| s.strip_prefix(':')) == Some(id);
    let mut rows = Vec::new();
    if input.needs_you > 0 || screen == "needs-you" {
        rows.push(nav("needs-you", "nav-needs-you", screen == "needs-you", input.needs_you, IdeTone::Attention));
    }
    rows.push(nav("chats", "nav-chats", screen == "chats", input.chats.count, input.chats.tone));
    rows.push(nav("devices", "nav-devices", screen == "devices", 0, IdeTone::Muted));
    if input.drafts > 0 || screen == "drafts" {
        rows.push(nav("drafts", "nav-drafts", screen == "drafts", input.drafts, IdeTone::Muted));
    }
    IdeSidebar {
        nav: rows,
        pins: input.pins.iter().map(|p| IdePinRow { id: p.id.clone(), identifier: p.identifier.clone(), title: p.title.clone(), selected: on("issue", &p.id) }).collect(),
        agents: IdeAgents {
            add: input.owner,
            rows: folders(
                input
                    .agents
                    .iter()
                    .map(|a| {
                        let badge = agent_count(a.unread, a.needs_you);
                        let count = count_label(badge.count);
                        let tone = count.is_some().then_some(badge.tone);
                        IdeRow::item(&a.id, &a.name, &a.icon, on("agent", &a.id), count, tone, true)
                    })
                    .collect(),
                &input.layout.agent,
            ),
        },
        apps: IdeApps {
            rows: folders(input.apps.iter().map(|a| IdeRow::item(&a.id, &a.name, &a.icon, on("app", &a.id) || on("app", &a.slug), None, None, true)).collect(), &input.layout.app),
        },
        boards: IdeBoards {
            rows: folders(input.boards.iter().map(|b| IdeRow::item(&b.id, &b.name, &b.icon, on("board", &b.id), None, None, input.owner)).collect(), &input.layout.board),
            empty: input.boards.is_empty(),
        },
        active: input.active.iter().map(|a| IdeActiveRow { id: a.id.clone(), title: a.title.clone(), icon: a.icon.clone(), selected: on("chat", &a.id) || a.session_ids.iter().any(|id| on("session", id)), unread: a.unread }).collect(),
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
