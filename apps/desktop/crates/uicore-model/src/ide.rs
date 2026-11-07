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
    pub busy: bool,
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

/// What the host knows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeSidebarInput {
    /// The open screen as `<kind>` or `<kind>:<id>`.
    pub screen: String,
    pub needs_you: usize,
    pub drafts: usize,
    pub threads_dot: Option<IdeDot>,
    pub owner: bool,
    pub pins: Vec<IdePinInput>,
    pub agents: Vec<IdeAgentInput>,
    pub apps: Vec<IdeAppInput>,
    pub boards: Vec<IdeBoardInput>,
    pub active: Vec<IdeActiveInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeNavRow {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub selected: bool,
    pub count: Option<String>,
    pub dot: Option<IdeDot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeRow {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub selected: bool,
    pub dot: Option<IdeDot>,
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

/// What the document binds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeSidebar {
    pub nav: Vec<IdeNavRow>,
    pub pins: Vec<IdePinRow>,
    pub agents: IdeAgents,
    pub apps: IdeApps,
    pub boards: IdeBoards,
    pub active: Vec<IdeActiveRow>,
}

fn copy(id: &str) -> String {
    static COPY: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let copy = COPY.get_or_init(|| serde_json::from_str::<serde_json::Value>(SOURCE).expect("ide-sidebar.json")["copy"].clone());
    copy[id].as_str().unwrap_or_default().to_string()
}

fn nav(id: &str, icon: &str, selected: bool, count: Option<String>, dot: Option<IdeDot>) -> IdeNavRow {
    IdeNavRow { id: id.into(), title: copy(id), icon: icon.into(), selected, count, dot }
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
        rows.push(nav("needs-you", "nav-needs-you", screen == "needs-you", Some(input.needs_you.to_string()), None));
    }
    rows.push(nav("threads", "nav-threads", screen == "threads", None, input.threads_dot));
    if input.drafts > 0 || screen == "drafts" {
        rows.push(nav("drafts", "nav-drafts", screen == "drafts", Some(input.drafts.to_string()), None));
    }
    IdeSidebar {
        nav: rows,
        pins: input.pins.iter().map(|p| IdePinRow { id: p.id.clone(), identifier: p.identifier.clone(), title: p.title.clone(), selected: on("issue", &p.id) }).collect(),
        agents: IdeAgents {
            add: input.owner,
            rows: input.agents.iter().map(|a| IdeRow { id: a.id.clone(), title: a.name.clone(), icon: a.icon.clone(), selected: on("agent", &a.id), dot: agent_dot(a.needs_you, a.busy), gear: true }).collect(),
        },
        apps: IdeApps {
            rows: input.apps.iter().map(|a| IdeRow { id: a.id.clone(), title: a.name.clone(), icon: a.icon.clone(), selected: on("app", &a.id) || on("app", &a.slug), dot: None, gear: true }).collect(),
        },
        boards: IdeBoards {
            rows: input.boards.iter().map(|b| IdeRow { id: b.id.clone(), title: b.name.clone(), icon: b.icon.clone(), selected: on("board", &b.id), dot: None, gear: input.owner }).collect(),
            empty: input.boards.is_empty(),
        },
        active: input.active.iter().map(|a| IdeActiveRow { id: a.id.clone(), title: a.title.clone(), icon: a.icon.clone(), selected: on("thread", &a.id) || a.session_ids.iter().any(|id| on("session", id)), unread: a.unread }).collect(),
    }
}
