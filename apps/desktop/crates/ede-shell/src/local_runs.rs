//! The chat on a Local desktop (#EDE-58): the desktop's own agent, run by
//! elocal (the person's CLI over ACP, eagent-hub-core in-process, the MCP
//! hub over the store). The shell starts the runner for the current Local
//! desktop when a CLI is installed and signed in, sends the composer's
//! text as a local session (one chat session per surface opening, a
//! message into it after), and builds the chat model's input from the
//! store (#EDE-65 lane I: the shared chat document).

use std::path::PathBuf;
use std::sync::Arc;

use elocal::runs::{HostTools, LocalRuns};
use std::collections::HashMap;

use domain::rows::{AgentDefRow, MessageRow, SessionRow};
use serde_json::Value;
use uicore_model::chat::{ChatAgentInput, ChatInput, ChatRowInput};

pub struct LocalChat {
    pub desktop: Arc<elocal::LocalDesktop>,
    pub runs: Option<Arc<LocalRuns>>,
    /// The runner's change stream (a session or its messages changed).
    pub changes: Option<flume::Receiver<elocal::runs::Changed>>,
    /// The CLI the runs use (`claude` | `codex`), empty when none is usable.
    pub agent: String,
    /// The session the chat surface talks to.
    pub session_id: Option<String>,
}

impl LocalChat {
    /// Opens the runner for `desktop` with the first usable CLI; `host` =
    /// the shell's own tools on the run's hub (#VAPP-273: `shell_edit`).
    pub fn start(desktop: Arc<elocal::LocalDesktop>, host: Option<HostTools>) -> Self {
        let cwd = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        // Every usable CLI (#EDE-65 J5): a session runs the one it names, the first by default.
        let runners = elocal::runs::installed_runners();
        let agent = runners.first().map(|r| r.id().to_string()).unwrap_or_default();
        let runs = (!runners.is_empty()).then_some(runners).and_then(|runners| match LocalRuns::start_with_runners(desktop.clone(), runners, cwd, host) {
            Ok(runs) => Some(runs),
            Err(e) => {
                tracing::warn!("local runs: {e}");
                None
            }
        });
        let changes = runs.as_ref().map(|r| r.changes());
        Self { desktop, runs, changes, agent, session_id: None }
    }

    /// The desktop's agents (its store's `agents`, #EDE-65 J: the Assistant
    /// and the agents made here).
    fn agent_rows(&self) -> Vec<AgentDefRow> {
        self.desktop.store.rows("agents").unwrap_or_default().into_iter().filter_map(|row| serde_json::from_value(Value::Object(row)).ok()).collect()
    }

    /// The agent row like a team's (my Assistant first) and their glyphs.
    pub fn agents(&self) -> (Vec<ChatAgentInput>, HashMap<String, crate::team_chat::AgentGlyph>) {
        let rows = self.agent_rows();
        let team_id = self.desktop.executor.team_id().to_string();
        let me = self.desktop.executor.identity().user_id;
        (crate::team_chat::agent_inputs(&rows, &team_id, &[&team_id], &me), crate::team_chat::agent_glyphs(&rows, &team_id, &[&team_id], &me))
    }

    /// The composer's text: a message into the chat's session, else a new one.
    pub fn send(&mut self, text: &str) -> Result<(), String> {
        self.send_to(text, None)
    }

    /// [`Self::send`]; a new session goes to `agent_id` (the home's pick) and
    /// names the CLI it runs (#EDE-65 J5: the agent's own, else this
    /// desktop's first installed one), so the session row records it.
    pub fn send_to(&mut self, text: &str, agent_id: Option<&str>) -> Result<(), String> {
        self.send_with(text, agent_id, Vec::new())
    }

    /// #EDE-65 lane L: one staged attachment into the desktop's blob store,
    /// answered as an upload (`/api/attachments/{id}`).
    pub fn attach(&self, filename: &str, content_type: &str, bytes: &[u8]) -> anyhow::Result<api::attachment_upload::UploadedImage> {
        let row = self.desktop.executor.attachment_create(filename, content_type, bytes, None).map_err(anyhow::Error::msg)?;
        Ok(serde_json::from_value(Value::Object(row))?)
    }

    /// [`Self::send_to`] with the attachments' ids (their lines join the body, local-parity.json).
    pub fn send_with(&mut self, text: &str, agent_id: Option<&str>, attachment_ids: Vec<String>) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() && attachment_ids.is_empty() {
            return Ok(());
        }
        let client = self.desktop.client();
        let mut input = api::sessions::SendInput::default();
        match &self.session_id {
            Some(session) if self.desktop.store.row("sessions", session).ok().flatten().is_some_and(|s| s.get("status").and_then(Value::as_str) != Some("ended")) => {
                input.session_id = Some(session.clone());
            }
            _ => {
                input.team_id = Some(self.desktop.executor.team_id().to_string());
                // No pick = the home's default, the first of the agent row.
                let picked = agent_id.map(str::to_string).or_else(|| self.agents().0.first().map(|a| a.id.clone()));
                let agent = picked.and_then(|id| self.agent_rows().into_iter().find(|a| a.id == id));
                input.cli = new_session_cli(agent.as_ref(), &self.agent);
                input.agent_id = agent.map(|a| a.id);
            }
        }
        input.text = (!text.is_empty()).then(|| text.to_string());
        input.attachment_ids = Some(attachment_ids).filter(|ids| !ids.is_empty());
        let out = api::sessions::send(&client, &input).map_err(|e| e.user_message())?;
        self.session_id = Some(out.session_id);
        Ok(())
    }

    /// The chat model's input for the chat's session (#EDE-65 lane I): the
    /// chat the domain builds from the desktop's store, drawn as bubbles
    /// (the desktop's own agent is an agent chat).
    pub fn input(&self) -> ChatInput {
        let mut input = assistant_home();
        let (agents, _) = self.agents();
        if !agents.is_empty() {
            input.agent_id = agents.first().map(|a| a.id.clone());
            input.agents = agents;
        }
        let Some((session, root, view, messages)) = self.session_chat() else { return input };
        let by_id: HashMap<&str, &MessageRow> = messages.iter().map(|m| (m.id.as_str(), m)).collect();
        let me = self.desktop.executor.identity();
        if let Some(agent) = root.agent_id.as_deref().and_then(|id| input.agents.iter().find(|a| a.id == id)) {
            input.title = agent.name.clone();
            input.icon = agent.icon.clone();
        }
        input.session_id = Some(session);
        input.busy = root.busy;
        input.rows = crate::chat::rows(&view.rows, &by_id, |id| (id == Some(me.user_id.as_str()) || id.is_none()).then(|| me.name.clone()));
        input
    }

    /// The open session's chat as the domain builds it from the desktop's
    /// store, with its messages.
    fn session_chat(&self) -> Option<(String, SessionRow, domain::session_tree::SessionChatView, Vec<MessageRow>)> {
        let session = self.session_id.as_ref()?;
        let root = self.desktop.store.row("sessions", session).ok().flatten().and_then(|row| serde_json::from_value::<SessionRow>(Value::Object(row)).ok())?;
        let mut messages: Vec<MessageRow> = self
            .desktop
            .store
            .rows_where("messages", "root_id", session)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|row| serde_json::from_value::<MessageRow>(Value::Object(row)).ok())
            .collect();
        // The store's rows carry no resolved session: they are this chat's.
        for message in &mut messages {
            message.session_id = session.clone();
        }
        let refs: Vec<&MessageRow> = messages.iter().collect();
        let now = chrono::Utc::now().timestamp_millis();
        let view = domain::session_tree::build_session_chat(
            domain::session_tree::SessionChatInput { root: &root, sessions: &[&root], session_issues: &[], issues: &[], messages: &refs, pull_requests: &[], now_ms: now },
            None,
        );
        Some((session.clone(), root, view, messages))
    }

    /// Lane Q: an Embed's inner event on card `message_id`
    /// (`domain::chat_document::card_reply`); a reply goes as the next message.
    pub fn card(&mut self, message_id: &str, name: &str, context: &Value) -> Result<(), String> {
        let Some((_, _, view, _)) = self.session_chat() else { return Ok(()) };
        match crate::chat::card_reply(&view.rows, message_id, Some(name), Some(context)) {
            Some(crate::chat::CardReply::Send { text, .. }) => self.send(&text),
            None => Ok(()),
        }
    }

    /// The person's pick on a question: the desktop's runs take answers as
    /// the next message (elocal has no `sessions.answer`).
    pub fn answer(&mut self, label: &str) -> Result<(), String> {
        self.send(label)
    }

    /// The open session's `cli` (#EDE-65 J5), else the CLI the runs use.
    pub fn session_cli(&self) -> Option<String> {
        let session = self.session_id.as_ref()?;
        let row = self.desktop.store.row("sessions", session).ok().flatten();
        row.and_then(|r| r.get("cli").and_then(Value::as_str).map(str::to_string)).filter(|c| !c.is_empty()).or_else(|| (!self.agent.is_empty()).then(|| self.agent.clone()))
    }

    /// The desktop's prompts (its store's `prompts`, #EDE-65 J).
    pub fn prompts(&self) -> Vec<domain::rows::PromptRow> {
        self.desktop.store.rows("prompts").unwrap_or_default().into_iter().filter_map(|row| serde_json::from_value(Value::Object(row)).ok()).collect()
    }

    /// New chat: the next send starts a session.
    pub fn new_chat(&mut self) {
        self.session_id = None;
    }

    /// A rail chat or a card: the chat shows that session of this desktop.
    pub fn open(&mut self, session_id: &str) {
        if self.desktop.store.row("sessions", session_id).ok().flatten().is_some() {
            self.session_id = Some(session_id.to_string());
        }
    }

    pub fn stop(&self) {
        if let Some(runs) = &self.runs {
            runs.stop_all();
        }
    }
}

/// The CLI a new Local session names (#EDE-65 J5: `sessions.send {cli}`):
/// the picked agent's own `cli`, else the desktop's first installed one;
/// `None` when neither is known (the runner picks).
pub fn new_session_cli(agent: Option<&AgentDefRow>, installed: &str) -> Option<String> {
    agent.and_then(|a| a.cli.clone()).filter(|c| !c.is_empty()).or_else(|| (!installed.is_empty()).then(|| installed.to_string()))
}

/// The chat's home with the one agent a Local desktop has (and a team
/// has until the shell syncs its agents): the Assistant, the brand mark.
pub fn assistant_home() -> ChatInput {
    ChatInput {
        title: ASSISTANT.into(),
        icon: ASSISTANT_ICON.into(),
        bubbles: true,
        agents: vec![ChatAgentInput { id: ASSISTANT_ID.into(), name: ASSISTANT.into(), icon: ASSISTANT_ICON.into() }],
        agent_id: Some(ASSISTANT_ID.into()),
        // eDE's composer has THE `/` menu.
        slash_commands: true,
        ..Default::default()
    }
}

pub const ASSISTANT_ID: &str = "assistant";
const ASSISTANT: &str = "Assistant";
const ASSISTANT_ICON: &str = "brand-exponential";

/// The goldens: a short local conversation.
pub fn demo_input() -> ChatInput {
    let message = |id: &str, author: &str, body: &str, time: &str| ChatRowInput::Message {
        id: id.into(),
        author: author.into(),
        body: body.into(),
        name: (author == "user").then(|| "Danny".to_string()),
        time: time.into(),
        queued: false,
    };
    ChatInput {
        session_id: Some("demo".into()),
        rows: vec![
            message("m1", "user", "Which issues are still open on this desktop?", "09:14"),
            message("m2", "agent", "Two are open on **General**:\n\n- GEN-1 Set up the printer\n- GEN-2 Back up the photos\n\nWant me to take one?", "09:14"),
            ChatRowInput::Pill { id: "d1".into(), icon: "coding-compact".into(), label: "Conversation compacted · 09:20".into() },
            message("m3", "user", "Take the printer.", "09:21"),
        ],
        ..assistant_home()
    }
}

#[cfg(test)]
mod tests {
    use uicore_model::chat::{chat_view, ChatFace};

    #[test]
    fn a_new_session_names_the_agent_s_cli_else_the_installed_one() {
        let agent = |cli: Option<&str>| -> super::AgentDefRow { serde_json::from_value(serde_json::json!({"id": "a1", "defaults": {"cli": cli}})).unwrap() };
        assert_eq!(super::new_session_cli(Some(&agent(Some("codex"))), "claude").as_deref(), Some("codex"));
        assert_eq!(super::new_session_cli(Some(&agent(None)), "claude").as_deref(), Some("claude"));
        assert_eq!(super::new_session_cli(Some(&agent(Some(""))), "claude").as_deref(), Some("claude"));
        assert_eq!(super::new_session_cli(None, "").as_deref(), None);
    }

    #[test]
    fn the_home_offers_the_assistant_and_the_demo_is_a_conversation() {
        let home = chat_view(&super::assistant_home());
        assert_eq!((home.face, home.agent_row.label.as_str()), (ChatFace::Home, "To Assistant"));
        let demo = chat_view(&super::demo_input());
        assert_eq!(demo.face, ChatFace::Conversation);
        assert_eq!(demo.rows.len(), 4);
        assert_eq!(demo.composer.placeholder, "Type / for commands", "eDE's composer has the / menu");
    }
}
