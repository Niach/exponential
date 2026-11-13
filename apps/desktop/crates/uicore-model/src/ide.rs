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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeRow {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub selected: bool,
    pub count: Option<String>,
    pub tone: Option<IdeTone>,
    pub gear: bool,
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
            rows: input
                .agents
                .iter()
                .map(|a| {
                    let badge = agent_count(a.unread, a.needs_you);
                    let count = count_label(badge.count);
                    let tone = count.is_some().then_some(badge.tone);
                    IdeRow { id: a.id.clone(), title: a.name.clone(), icon: a.icon.clone(), selected: on("agent", &a.id), count, tone, gear: true }
                })
                .collect(),
        },
        apps: IdeApps {
            rows: input.apps.iter().map(|a| IdeRow { id: a.id.clone(), title: a.name.clone(), icon: a.icon.clone(), selected: on("app", &a.id) || on("app", &a.slug), count: None, tone: None, gear: true }).collect(),
        },
        boards: IdeBoards {
            rows: input.boards.iter().map(|b| IdeRow { id: b.id.clone(), title: b.name.clone(), icon: b.icon.clone(), selected: on("board", &b.id), count: None, tone: None, gear: input.owner }).collect(),
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
