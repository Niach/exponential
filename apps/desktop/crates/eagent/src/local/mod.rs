//! The server-free facade (EXP-1426 part 5): ONE local account, ONE live run
//! at a time, no Exponential server — what spezl builds on.
//!
//! ```no_run
//! use eagent::agent::CodingAgent;
//! use eagent::local::{LocalAgent, RunRequest};
//! use eagent::wire::{ActivityEvent, TurnState};
//!
//! let agent = LocalAgent::new(CodingAgent::Claude);
//! if agent.signed_in() != Some(true) {
//!     println!("sign in first: {}", agent.login_command_line());
//!     return;
//! }
//! let run = agent
//!     .start(RunRequest::new(std::env::current_dir().unwrap()).prompt("reply with the word ok"))
//!     .unwrap();
//! for event in run.events().unwrap().iter() {
//!     match event {
//!         ActivityEvent::Narration { text, .. } => print!("{text}"),
//!         ActivityEvent::Turn { state: TurnState::Ended, .. } => break,
//!         _ => {}
//!     }
//! }
//! run.stop();
//! for past in agent.sessions(None, Some(5)) {
//!     println!("{}  {}", past.id, past.title);
//! }
//! ```
//!
//! - **Detect / login.** [`LocalAgent::check`] (`<cli> --version`),
//!   [`LocalAgent::signed_in`] (the CLI's own `auth status` / `login status`),
//!   [`LocalAgent::login`] (the CLI's own sign-in in a PTY,
//!   [`crate::login::LocalLogin`]). Always the CLI's DEFAULT home: no
//!   config-dir variable is set, no credential is read, copied or refreshed.
//! - **Run.** [`LocalAgent::start`] spawns the user's own `claude` / `codex`
//!   over ACP in a cwd ([`RunRequest`]: seed prompt, native resume id, extra
//!   MCP servers, a system prompt append) behind [`NoBackend`]. The
//!   [`LocalRun`] sends, answers a question or permission card, interrupts
//!   and stops; events arrive on a channel ([`LocalRun::events`]) or in the
//!   caller's own [`EventSink`] ([`RunRequest::sink`]).
//! - **History.** [`LocalAgent::sessions`] = the CLI's own past
//!   conversations with their own names ([`history`]); resume one with
//!   [`RunRequest::resume`].
//!
//! The `AccountEnv` seam of the README, locally: the spawn env is PATH (the
//! login shell's, [`crate::spawn::login_path`]) plus the caller's
//! [`RunRequest::env`], and nothing that moves the CLI's home.

pub mod history;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub use history::PastSession;

use crate::agent::CodingAgent;
use crate::detect::{Tool, ToolCheck};
use crate::engine::adapters::{AdapterKind, LaunchSpec};
use crate::engine::context_layout::ContextLayers;
use crate::engine::{
    ChildExitLink, EngineError, EngineExit, EngineSession, EventSink, NoBackend, ResumeHandle,
    RunOptions, SessionSpec,
};
use crate::login::{LocalLogin, LoginCommand};
use crate::mcp::{AgentMcp, McpServerWire};
use crate::wire::{ActivityEvent, RemoteAnswer};

/// One agent CLI on this machine, as the user installed and signed it in.
#[derive(Clone, Debug)]
pub struct LocalAgent {
    agent: CodingAgent,
    program: String,
    path_env: String,
}

impl LocalAgent {
    /// `agent`'s default binary (`claude` / `codex`) on the login shell's PATH.
    pub fn new(agent: CodingAgent) -> LocalAgent {
        LocalAgent::with_program(agent, agent.default_binary())
    }

    /// An explicit binary (a name on PATH or an absolute path).
    pub fn with_program(agent: CodingAgent, program: impl Into<String>) -> LocalAgent {
        LocalAgent { agent, program: program.into(), path_env: crate::spawn::login_path() }
    }

    pub fn agent(&self) -> CodingAgent {
        self.agent
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    /// `<cli> --version`: installed, and which version.
    pub fn check(&self) -> ToolCheck {
        crate::detect::check_tool(Tool::for_agent(self.agent), &self.program, &self.path_env)
    }

    /// The CLI's own sign-in status; `None` when it could not say (not
    /// installed, timed out).
    pub fn signed_in(&self) -> Option<bool> {
        crate::detect::is_signed_in(self.agent, &self.program, &self.path_env)
    }

    /// The sign-in the CLI would run ([`Self::login`] runs exactly this).
    pub fn login_command(&self, remote: bool) -> LoginCommand {
        crate::login::login_command(self.agent, self.program.clone(), remote)
    }

    /// [`Self::login_command`] as a line a person types (`claude auth login --claudeai`).
    pub fn login_command_line(&self) -> String {
        let command = self.login_command(false);
        std::iter::once(command.program).chain(command.args).collect::<Vec<_>>().join(" ")
    }

    /// Start the CLI's own sign-in in a PTY: poll it for the URL / device
    /// code, then [`Self::signed_in`]. `remote` = never open a browser here.
    pub fn login(&self, remote: bool) -> std::io::Result<LocalLogin> {
        LocalLogin::start(self.agent, &self.program, &self.path_env, remote)
    }

    /// The CLI's own sign-out.
    pub fn logout(&self) -> Result<(), String> {
        crate::login::logout(self.agent, &self.program, &self.path_env)
    }

    /// Past conversations, newest first ([`history`]); `cwd` keeps those of
    /// that directory, `limit` caps the count.
    pub fn sessions(&self, cwd: Option<&Path>, limit: Option<usize>) -> Vec<PastSession> {
        history::list_sessions(self.agent, cwd, limit)
    }

    /// Start (or, with [`RunRequest::resume`], reopen) ONE run. Non-blocking:
    /// a handshake failure ends the run ([`LocalRun::wait`]'s `error`).
    pub fn start(&self, request: RunRequest) -> Result<LocalRun, EngineError> {
        let RunRequest { cwd, prompt, resume, servers, env, system_append, options, sink, runtime } =
            request;
        let (runtime, owned) = match runtime {
            Some(handle) => (handle, None),
            None => {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .map_err(EngineError::Spawn)?;
                (runtime.handle().clone(), Some(runtime))
            }
        };
        let (sink, events): (Arc<dyn EventSink>, _) = match sink {
            Some(sink) => (sink, None),
            None => {
                let (tx, rx) = flume::unbounded();
                (Arc::new(ChannelSink(tx)), Some(rx))
            }
        };
        let mut spawn = crate::spawn::SpawnSpec::new(self.program.clone())
            .cwd(cwd.clone())
            .env("PATH", self.path_env.clone());
        for (key, value) in env {
            spawn = spawn.env(key, value);
        }
        let launch = LaunchSpec {
            kind: AdapterKind::from_agent(self.agent),
            agent: self.agent,
            spawn,
            options,
            mcp: AgentMcp::ServersOnly,
            servers,
            cwd,
            session_id: uuid::Uuid::new_v4().to_string(),
            prompt,
            resume: resume.map(ResumeHandle::Native),
            replay: false,
            personal_key: None,
            reaper_settings_path: None,
            system_append,
            context_layers: ContextLayers::default(),
            // The CLI names the conversation itself: that name is the Chats row.
            name_conversation: true,
            aside: false,
            confine_to: None,
            exit: ChildExitLink::new(),
        };
        let mut session = SessionSpec::for_launch(&launch, runtime);
        session.sink = Some(sink);
        let session = crate::engine::start(launch, session, Arc::new(NoBackend::new()))?;
        Ok(LocalRun { session, events, _runtime: owned })
    }
}

/// What to run: built with [`RunRequest::new`] and the setters.
pub struct RunRequest {
    pub cwd: PathBuf,
    /// The first message, sent once the agent is up.
    pub prompt: Option<String>,
    /// A [`PastSession::id`] to reopen instead of starting fresh.
    pub resume: Option<String>,
    /// MCP servers beside the ones the user configured for the CLI.
    /// Credentials stay out of them: a value is a `${VAR}` reference into
    /// [`Self::env`].
    pub servers: Vec<McpServerWire>,
    /// Extra spawn env (the servers' `${VAR}` values).
    pub env: Vec<(String, String)>,
    /// Appended to the agent's system prompt, on start and resume.
    pub system_append: String,
    /// Model / effort picks; blank = the CLI's own default.
    pub options: RunOptions,
    /// Where events go instead of [`LocalRun::events`].
    pub sink: Option<Arc<dyn EventSink>>,
    /// The tokio runtime to drive the connection on; `None` = the run owns one.
    pub runtime: Option<tokio::runtime::Handle>,
}

impl RunRequest {
    pub fn new(cwd: impl Into<PathBuf>) -> RunRequest {
        RunRequest {
            cwd: cwd.into(),
            prompt: None,
            resume: None,
            servers: Vec::new(),
            env: Vec::new(),
            system_append: String::new(),
            options: RunOptions::default(),
            sink: None,
            runtime: None,
        }
    }

    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }

    pub fn resume(mut self, session_id: impl Into<String>) -> Self {
        self.resume = Some(session_id.into());
        self
    }

    pub fn mcp_server(mut self, server: McpServerWire) -> Self {
        self.servers.push(server);
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn system_append(mut self, text: impl Into<String>) -> Self {
        self.system_append = text.into();
        self
    }

    pub fn sink(mut self, sink: Arc<dyn EventSink>) -> Self {
        self.sink = Some(sink);
        self
    }

    pub fn runtime(mut self, handle: tokio::runtime::Handle) -> Self {
        self.runtime = Some(handle);
        self
    }
}

/// The ONE live run. Dropping it does not stop the agent: call [`Self::stop`].
pub struct LocalRun {
    session: EngineSession,
    events: Option<flume::Receiver<ActivityEvent>>,
    // Declared last: the runtime outlives the session's handles.
    _runtime: Option<tokio::runtime::Runtime>,
}

impl LocalRun {
    /// The run's events, in order; `None` when [`RunRequest::sink`] took them.
    /// A question or permission card is an [`ActivityEvent::Question`]:
    /// answer it with [`Self::answer`].
    pub fn events(&self) -> Option<&flume::Receiver<ActivityEvent>> {
        self.events.as_ref()
    }

    /// The CLI's own id for this conversation (resume it with this), once
    /// the agent reported it.
    pub fn native_session_id(&self) -> Option<String> {
        self.session.agent_native_session_id()
    }

    /// The name the CLI gave the conversation, once it did.
    pub fn title(&self) -> Option<String> {
        self.session.agent_title()
    }

    /// A message: between turns it starts one, during one it is queued.
    pub fn send(&self, text: impl Into<String>) {
        self.session.send_prompt(text.into());
    }

    /// Answer the card `question_id` with the option keys picked (and the
    /// typed reply of a free-text option).
    pub fn answer(&self, question_id: impl Into<String>, keys: Vec<String>, text: Option<String>) {
        self.session.answer(RemoteAnswer { question_id: question_id.into(), ask_id: None, keys, text });
    }

    /// Stop the running turn; the run stays up for the next message.
    pub fn interrupt(&self) {
        self.session.cancel_turn();
    }

    /// Is the agent between turns?
    pub fn idle(&self) -> bool {
        self.session.turn_signal().is_idle()
    }

    /// End the run and wait (bounded) for the agent to exit.
    pub fn stop(&self) -> Option<EngineExit> {
        self.session.kill("killed");
        self.session.wait_timeout(Duration::from_secs(10))
    }

    pub fn is_done(&self) -> bool {
        self.session.is_done()
    }

    /// Block until the run ends on its own (the agent exited or never connected).
    pub fn wait(&self) -> EngineExit {
        self.session.wait()
    }

    /// Everything else the core session offers (mode, config, unqueue, …).
    pub fn session(&self) -> &EngineSession {
        &self.session
    }
}

/// The default sink: every event onto a channel.
struct ChannelSink(flume::Sender<ActivityEvent>);

impl EventSink for ChannelSink {
    fn send(&self, event: ActivityEvent) {
        let _ = self.0.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::McpWireTransport;

    fn server() -> McpServerWire {
        McpServerWire {
            id: "s1".to_string(),
            name: "notes".to_string(),
            transport: McpWireTransport::Http { url: "http://127.0.0.1:9/mcp".to_string() },
            headers: vec![("Authorization".to_string(), "Bearer ${NOTES_TOKEN}".to_string())],
            token_env: Some("NOTES_TOKEN".to_string()),
            env: Vec::new(),
            actor: None,
        }
    }

    /// `ServersOnly` renders the run's own servers and never `exponential`.
    #[test]
    fn a_local_run_wires_only_its_own_mcp_servers() {
        let claude: serde_json::Value = serde_json::from_str(
            &crate::engine::adapters::claude_wire::servers_mcp_config(&[server()]),
        )
        .unwrap();
        let servers = claude["mcpServers"].as_object().unwrap();
        assert_eq!(servers.keys().collect::<Vec<_>>(), ["notes"]);
        assert_eq!(servers["notes"]["headers"]["Authorization"], "Bearer ${NOTES_TOKEN}");

        let entries = crate::mcp::codex_server_entries(&[server()]);
        let table = crate::engine::adapters::codex_wire::mcp_servers_table(&entries);
        assert_eq!(table.as_object().unwrap().keys().collect::<Vec<_>>(), ["notes"]);
        assert_eq!(table["notes"]["bearer_token_env_var"], "NOTES_TOKEN");
    }

    #[test]
    fn the_login_line_is_the_clis_own_sign_in() {
        assert_eq!(
            LocalAgent::with_program(CodingAgent::Claude, "claude").login_command_line(),
            "claude auth login --claudeai"
        );
        assert_eq!(
            LocalAgent::with_program(CodingAgent::Codex, "codex").login_command_line(),
            "codex login --device-auth"
        );
    }
}
