//! The chat on a Local desktop (#EDE-58): the desktop's own agent, run by
//! elocal (the person's CLI over ACP, eagent-hub-core in-process, the MCP
//! hub over the store). The shell starts the runner for the current Local
//! desktop when a CLI is installed and signed in, sends the composer's
//! text as a local session (one chat session per surface opening, a
//! message into it after), and feeds `/chatting` from the store.

use std::path::PathBuf;
use std::sync::Arc;

use elocal::runs::{AcpRunner, HostTools, LocalRuns, Runner};
use serde_json::{json, Value};

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
        let mut agent = String::new();
        let mut runner: Option<Arc<dyn Runner>> = None;
        for candidate in [eagent::agent::CodingAgent::Claude, eagent::agent::CodingAgent::Codex] {
            let acp = AcpRunner::new(candidate);
            if acp.available() {
                agent = candidate.id().to_string();
                runner = Some(Arc::new(acp));
                break;
            }
        }
        let runs = runner.and_then(|runner| match LocalRuns::start_with(desktop.clone(), runner, cwd, host) {
            Ok(runs) => Some(runs),
            Err(e) => {
                tracing::warn!("local runs: {e}");
                None
            }
        });
        let changes = runs.as_ref().map(|r| r.changes());
        Self { desktop, runs, changes, agent, session_id: None }
    }

    /// The composer's text: a message into the chat's session, else a new one.
    pub fn send(&mut self, text: &str) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
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
            }
        }
        input.text = Some(text.to_string());
        let out = api::sessions::send(&client, &input).map_err(|e| e.user_message())?;
        self.session_id = Some(out.session_id);
        Ok(())
    }

    /// `/chatting`: the conversation of the chat's session, as the surface shows it.
    pub fn feed(&self) -> Value {
        let identity = self.desktop.executor.identity();
        let (messages, status, busy) = match &self.session_id {
            Some(session) => {
                let row = self.desktop.store.row("sessions", session).ok().flatten();
                let status = row.as_ref().and_then(|s| s.get("status").and_then(Value::as_str)).unwrap_or("queued").to_string();
                let busy = row.as_ref().and_then(|s| s.get("busy").and_then(Value::as_bool)).unwrap_or(false);
                let mut rows = self.desktop.store.rows_where("messages", "root_id", session).unwrap_or_default();
                rows.sort_by(|a, b| a.get("created_at").and_then(Value::as_str).cmp(&b.get("created_at").and_then(Value::as_str)));
                let messages: Vec<Value> = rows
                    .iter()
                    .filter(|m| m.get("body").and_then(Value::as_str).is_some_and(|b| !b.trim().is_empty()))
                    .map(|m| {
                        let author = match m.get("author_kind").and_then(Value::as_str) {
                            Some("user") => identity.name.clone(),
                            Some("agent") => self.agent.clone(),
                            _ => "Exponential Desktop".to_string(),
                        };
                        json!({"id": m["id"], "author": author, "body": m["body"]})
                    })
                    .collect();
                let status = match (status.as_str(), busy) {
                    ("queued", _) => "Starting…".to_string(),
                    (_, true) => format!("{} is working…", self.agent),
                    ("ended", _) => "Ended".to_string(),
                    _ => String::new(),
                };
                (messages, status, busy)
            }
            None => (Vec::new(), String::new(), false),
        };
        json!({"local": true, "agent": self.agent, "title": "Assistant", "status": status, "busy": busy, "messages": messages})
    }

    pub fn stop(&self) {
        if let Some(runs) = &self.runs {
            runs.stop_all();
        }
    }
}

/// `/chatting` on a team (an account): the Assistant lives in the Exponential
/// app; #EDE-65: the status names the team and the account it is connected
/// through (the chat opens the app, it never asks to connect what is).
pub fn remote_feed(team: &str, account: &str) -> Value {
    let connected = match (team.is_empty(), account.is_empty()) {
        (false, false) => format!("{team} · {account}"),
        (false, true) => team.to_string(),
        _ => account.to_string(),
    };
    json!({"local": false, "agent": "", "title": "Assistant", "status": connected, "busy": false, "messages": []})
}

/// The goldens: a short local conversation.
pub fn demo_feed() -> Value {
    json!({"local": true, "agent": "claude", "title": "Assistant", "status": "", "busy": false, "messages": [
        {"id": "m1", "author": "Danny", "body": "Which issues are still open on this desktop?"},
        {"id": "m2", "author": "claude", "body": "Two are open on **General**:\n\n- GEN-1 Set up the printer\n- GEN-2 Back up the photos\n\nWant me to take one?"}
    ]})
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_team_chat_names_the_team_and_its_account() {
        let feed = super::remote_feed("Exponential", "danny@example.com");
        assert_eq!((feed["local"].as_bool(), feed["status"].as_str()), (Some(false), Some("Exponential · danny@example.com")));
        assert_eq!(super::remote_feed("", "danny@example.com")["status"], "danny@example.com");
    }
}
