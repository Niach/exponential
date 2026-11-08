//! #EDE-65: the chats eDE's icon rail lists, a pure projection of the root
//! sessions the host knows (the current team's or the Local desktop's),
//! locked by the `chats` cases of `packages/uicore/fixtures/shell.json`
//! (`chatRows` in `packages/uicore/src/shell.ts`).

use serde::{Deserialize, Serialize};

use crate::ide::{agent_dot, IdeDot};

/// The rail keeps this many chats.
pub const CHATS_MAX: usize = 20;

/// One chat the host knows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatInput {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub agent: Option<String>,
    pub needs_you: bool,
    pub busy: bool,
    pub unread: bool,
    /// The last activity, ISO 8601.
    pub at: String,
}

/// What the document binds: the glyph names the agent, the dot the IDE's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatRow {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub dot: Option<IdeDot>,
}

fn icon(agent: Option<&str>) -> &'static str {
    match agent.map(str::trim).unwrap_or_default() {
        "" | "claude" => "agent:claude",
        "codex" => "agent:codex",
        _ => "settings-agents",
    }
}

/// The most recent first (ties by id), at most [`CHATS_MAX`].
pub fn chat_rows(chats: &[ChatInput]) -> Vec<ChatRow> {
    let mut sorted: Vec<&ChatInput> = chats.iter().collect();
    sorted.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.id.cmp(&b.id)));
    sorted
        .into_iter()
        .take(CHATS_MAX)
        .map(|c| {
            let title = c.title.trim();
            ChatRow {
                id: c.id.clone(),
                title: if title.is_empty() { "Untitled".into() } else { title.into() },
                icon: icon(c.agent.as_deref()).into(),
                dot: agent_dot(c.needs_you, c.busy).or(c.unread.then_some(IdeDot::Primary)),
            }
        })
        .collect()
}
