//! VAPP-264 (E4 of the Engine, #VAPP-259): ONE way to start an agent run,
//! whatever runs it. The seams below this ([`crate::engine::LaunchSpec`],
//! `AccountEnv`, `McpProvider`, ...) describe spawning a CLI; local inference
//! spawns nothing and a phone cannot spawn at all, so a host starts a run
//! through an [`AgentProvider`] instead:
//!
//! - [`AcpAgent`]: the user's own Claude Code / Codex over the Agent Client
//!   Protocol, i.e. [`crate::local::LocalAgent::start`] unchanged underneath
//!   (local agent runs, #EDE-58).
//! - `LocalModel` (cargo feature `local-model`, off by default): an
//!   in-process model behind the [`Generate`](local_model::Generate) trait the
//!   genUI generator (`eui-gen`, #SA-8) fills; eagent never links llama.cpp
//!   itself (the offline fallback, #EXP-1721).
//! - [`RemoteSession`]: Exponential's `sessions.send`, through the
//!   [`SessionsSend`] callback the host implements (phones and web), so
//!   eagent stays server-free.
//!
//! Every provider takes the same three things: the [`Brief`] (the prompt and
//! the run's context), the [`HubGrant`] (the ONLY tools a run reaches: the
//! Engine's in-process eMCP hub, `packages/emcp/fixtures/hub.json`) and an
//! [`EventSink`] taking the same `wire` events the ACP engine emits, so UIs
//! and `eagent-hub-core` never care which provider ran. It answers a
//! [`RunHandle`] or an eCore [`Failure`].

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

pub use ecore::{ErrorCode, Failure, RunId, SessionId, Token};
use serde::{Deserialize, Serialize};

pub use crate::engine::EventSink;
use crate::engine::RunOptions;

mod acp;
#[cfg(feature = "local-model")]
pub mod local_model;
mod remote;

pub use acp::{engine_failure, AcpAgent, HUB_SERVER_NAME, HUB_TOKEN_ENV};
#[cfg(feature = "local-model")]
pub use local_model::{Generate, GenerateRequest, LocalModel};
pub use remote::{RemoteSession, SessionsSend, SessionsSendRequest};

/// Something that runs an agent: given a brief, its tools and a sink, it
/// starts ONE run and hands back its handle.
pub trait AgentProvider: Send + Sync {
    /// The provider's stable id (`claude`, `codex`, `local-model`, `remote`):
    /// what a host config or a person's pick names it by.
    fn id(&self) -> &str;

    /// Can it start a run right now (installed, online, a model loaded)?
    /// [`Providers::select`] skips one that cannot when no id is asked for.
    fn available(&self) -> bool {
        true
    }

    /// Start the run. Non-blocking: everything after a successful start
    /// reaches `sink` (`Turn` edges, narration, tools, questions); a start
    /// that cannot happen is the [`Failure`].
    fn start(&self, brief: Brief, tools: HubGrant, sink: Arc<dyn EventSink>) -> Result<RunHandle, Failure>;
}

/// What a run is asked to do: the prompt plus the run's context.
#[derive(Clone, Debug, PartialEq)]
pub struct Brief {
    /// The session the run belongs to (the host's; eCore's id rule).
    pub session_id: SessionId,
    /// The run inside it, when the host numbers runs.
    pub run_id: Option<RunId>,
    /// The first message.
    pub prompt: String,
    /// Appended to the agent's system prompt.
    pub system_append: String,
    /// The working directory (an ACP run needs one; a model or a remote
    /// session ignores it).
    pub cwd: Option<PathBuf>,
    /// A conversation to continue: the CLI's own session id (ACP) or the
    /// Exponential session to message (remote).
    pub resume: Option<String>,
    /// Model / effort picks; blank = the provider's own default.
    pub options: RunOptions,
}

impl Brief {
    pub fn new(session_id: SessionId, prompt: impl Into<String>) -> Brief {
        Brief {
            session_id,
            run_id: None,
            prompt: prompt.into(),
            system_append: String::new(),
            cwd: None,
            resume: None,
            options: RunOptions::default(),
        }
    }

    pub fn run_id(mut self, run_id: RunId) -> Self {
        self.run_id = Some(run_id);
        self
    }

    pub fn system_append(mut self, text: impl Into<String>) -> Self {
        self.system_append = text.into();
        self
    }

    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn resume(mut self, id: impl Into<String>) -> Self {
        self.resume = Some(id.into());
        self
    }

    pub fn options(mut self, options: RunOptions) -> Self {
        self.options = options;
        self
    }
}

/// A server's level in a [`HubGrant`] (hub.json / access.json): `none` <
/// `read` < `write`; a server the grant does not name is `none`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HubLevel {
    #[default]
    None,
    Read,
    Write,
}

/// The ONLY tools a run reaches: hub.json's grant (a level per mounted
/// server, the team and user the run acts for) plus where the hub answers
/// and the run's own bearer token. A provider never gets an MCP server any
/// other way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubGrant {
    /// The hub's Streamable HTTP endpoint (loopback for a spawned agent).
    pub endpoint: String,
    /// The run's bearer token (its `Debug` hides the value).
    pub token: Token,
    /// Server name -> level; a server not named = `none`.
    #[serde(default)]
    pub servers: BTreeMap<String, HubLevel>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
}

/// hub.json `serverNameMax`.
pub const HUB_SERVER_NAME_MAX: usize = 32;

/// hub.json `serverNamePattern` (`^[a-z0-9][a-z0-9-]*$`), at most
/// [`HUB_SERVER_NAME_MAX`]; `__` (the separator) can never appear.
pub fn hub_server_name_valid(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else { return false };
    name.len() <= HUB_SERVER_NAME_MAX
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl HubGrant {
    pub fn new(endpoint: impl Into<String>, token: Token) -> HubGrant {
        HubGrant { endpoint: endpoint.into(), token, servers: BTreeMap::new(), team_id: None, user_id: None }
    }

    pub fn server(mut self, name: impl Into<String>, level: HubLevel) -> Self {
        self.servers.insert(name.into(), level);
        self
    }

    pub fn acting_for(mut self, team_id: Option<String>, user_id: Option<String>) -> Self {
        self.team_id = team_id;
        self.user_id = user_id;
        self
    }

    /// `server`'s level (`none` when the grant does not name it).
    pub fn level(&self, server: &str) -> HubLevel {
        self.servers.get(server).copied().unwrap_or_default()
    }

    /// Why the grant cannot be handed to a run, as the refusal a start
    /// answers (`None` = fine).
    pub fn refusal(&self) -> Option<Failure> {
        let endpoint = self.endpoint.trim();
        if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
            return Some(Failure::new(ErrorCode::Invalid, "the hub endpoint is not an http(s) URL"));
        }
        if let Some(bad) = self.servers.keys().find(|name| !hub_server_name_valid(name)) {
            return Some(Failure::new(ErrorCode::Invalid, &format!("`{bad}` is not a hub server name")));
        }
        None
    }
}

/// What a [`RunHandle`] drives: each provider's own run.
pub trait RunControl: Send {
    /// End the run (idempotent).
    fn stop(&self);

    /// Has the run ended (on its own or stopped)?
    fn is_done(&self) -> bool;
}

/// A started run: which provider runs it, for which session, and its stop.
pub struct RunHandle {
    provider: String,
    session_id: SessionId,
    control: Box<dyn RunControl>,
}

impl RunHandle {
    pub fn new(provider: impl Into<String>, session_id: SessionId, control: impl RunControl + 'static) -> RunHandle {
        RunHandle { provider: provider.into(), session_id, control: Box::new(control) }
    }

    /// The [`AgentProvider::id`] that started it.
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn stop(&self) {
        self.control.stop();
    }

    pub fn is_done(&self) -> bool {
        self.control.is_done()
    }
}

impl std::fmt::Debug for RunHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunHandle").field("provider", &self.provider).field("session_id", &self.session_id).finish()
    }
}

/// The providers a host offers, in preference order.
#[derive(Clone, Default)]
pub struct Providers(Vec<Arc<dyn AgentProvider>>);

impl Providers {
    pub fn new() -> Providers {
        Providers(Vec::new())
    }

    /// Add one; a provider with the same id is replaced in place.
    pub fn with(mut self, provider: Arc<dyn AgentProvider>) -> Self {
        match self.0.iter().position(|known| known.id() == provider.id()) {
            Some(index) => self.0[index] = provider,
            None => self.0.push(provider),
        }
        self
    }

    pub fn ids(&self) -> Vec<&str> {
        self.0.iter().map(|provider| provider.id()).collect()
    }

    /// `Some(id)`: exactly that provider (`not_found` when none has it, and
    /// `unavailable` when it cannot run now: a named pick never silently
    /// becomes another). `None`: the first available one, in preference
    /// order (the offline fallback), else `unavailable`.
    pub fn select(&self, id: Option<&str>) -> Result<Arc<dyn AgentProvider>, Failure> {
        match id {
            Some(id) => {
                let provider = self
                    .0
                    .iter()
                    .find(|provider| provider.id() == id)
                    .ok_or_else(|| Failure::new(ErrorCode::NotFound, &format!("no agent provider `{id}`")))?;
                if provider.available() {
                    Ok(provider.clone())
                } else {
                    Err(Failure::new(ErrorCode::Unavailable, &format!("the agent provider `{id}` is not available")))
                }
            }
            None => self
                .0
                .iter()
                .find(|provider| provider.available())
                .cloned()
                .ok_or_else(|| Failure::new(ErrorCode::Unavailable, "no agent provider is available")),
        }
    }

    /// [`Self::select`], then [`AgentProvider::start`].
    pub fn start(
        &self,
        id: Option<&str>,
        brief: Brief,
        tools: HubGrant,
        sink: Arc<dyn EventSink>,
    ) -> Result<RunHandle, Failure> {
        self.select(id)?.start(brief, tools, sink)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    pub(crate) fn token() -> Token {
        Token::new("a".repeat(ecore::TOKEN_LENGTH)).unwrap()
    }

    pub(crate) fn grant() -> HubGrant {
        HubGrant::new("http://127.0.0.1:4100/mcp", token())
            .server("files", HubLevel::Write)
            .server("computer", HubLevel::Read)
            .acting_for(Some("t1".to_string()), Some("u1".to_string()))
    }

    pub(crate) fn session() -> SessionId {
        SessionId::new("s-1").unwrap()
    }

    struct Fake {
        id: &'static str,
        available: bool,
    }

    struct Done(AtomicBool);

    impl RunControl for Done {
        fn stop(&self) {
            self.0.store(true, Ordering::SeqCst);
        }
        fn is_done(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }
    }

    impl AgentProvider for Fake {
        fn id(&self) -> &str {
            self.id
        }
        fn available(&self) -> bool {
            self.available
        }
        fn start(&self, brief: Brief, _tools: HubGrant, _sink: Arc<dyn EventSink>) -> Result<RunHandle, Failure> {
            Ok(RunHandle::new(self.id, brief.session_id, Done(AtomicBool::new(false))))
        }
    }

    fn providers() -> Providers {
        Providers::new()
            .with(Arc::new(Fake { id: "claude", available: false }))
            .with(Arc::new(Fake { id: "remote", available: true }))
            .with(Arc::new(Fake { id: "local-model", available: true }))
    }

    #[test]
    fn a_named_provider_is_that_one_or_a_failure() {
        let providers = providers();
        assert_eq!(providers.select(Some("local-model")).unwrap().id(), "local-model");
        let missing = providers.select(Some("gemini")).err().unwrap();
        assert_eq!(missing.code, ErrorCode::NotFound);
        let offline = providers.select(Some("claude")).err().unwrap();
        assert_eq!(offline.code, ErrorCode::Unavailable);
        assert!(offline.retryable);
    }

    #[test]
    fn no_pick_takes_the_first_available_in_order() {
        assert_eq!(providers().select(None).unwrap().id(), "remote");
        let none = Providers::new().with(Arc::new(Fake { id: "claude", available: false }));
        assert_eq!(none.select(None).err().unwrap().code, ErrorCode::Unavailable);
        assert_eq!(Providers::new().select(None).err().unwrap().code, ErrorCode::Unavailable);
    }

    #[test]
    fn the_same_id_replaces_in_place() {
        let providers = providers().with(Arc::new(Fake { id: "claude", available: true }));
        assert_eq!(providers.ids(), ["claude", "remote", "local-model"]);
        assert_eq!(providers.select(None).unwrap().id(), "claude");
    }

    #[test]
    fn a_start_goes_through_the_selected_provider() {
        let handle = providers()
            .start(None, Brief::new(session(), "hi"), grant(), crate::engine::RecordingSink::new())
            .unwrap();
        assert_eq!(handle.provider(), "remote");
        assert_eq!(handle.session_id().as_str(), "s-1");
        assert!(!handle.is_done());
        handle.stop();
        assert!(handle.is_done());
    }

    /// hub.json's grants parse into a [`HubGrant`] once the endpoint and
    /// token sit beside them, and its server names follow its rule.
    #[test]
    fn the_grant_mirrors_hub_json() {
        let hub: serde_json::Value =
            serde_json::from_str(include_str!("../../../../../../packages/emcp/fixtures/hub.json")).unwrap();
        for case in hub["listCases"].as_array().unwrap() {
            let mut value = case["grant"].clone();
            value["endpoint"] = serde_json::json!("http://127.0.0.1:4100/mcp");
            value["token"] = serde_json::json!(token().as_str());
            let parsed: HubGrant = serde_json::from_value(value).unwrap();
            for (server, level) in case["grant"]["servers"].as_object().unwrap() {
                let level = serde_json::from_value::<HubLevel>(level.clone()).unwrap();
                assert_eq!(parsed.level(server), level, "{}", case["name"]);
            }
            assert_eq!(parsed.level("not-granted"), HubLevel::None);
        }
        for case in hub["serverNameCases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            assert_eq!(hub_server_name_valid(name), case["expected"].as_bool().unwrap(), "{name}");
        }
        let parsed = grant();
        assert_eq!(parsed.team_id.as_deref(), Some("t1"));
        assert!(!format!("{parsed:?}").contains(token().as_str()), "Debug hides the token");
    }

    #[test]
    fn a_bad_grant_is_refused_as_invalid() {
        assert!(grant().refusal().is_none());
        let mut bad = grant();
        bad.endpoint = "loopback".to_string();
        assert_eq!(bad.refusal().unwrap().code, ErrorCode::Invalid);
        let bad = grant().server("Files", HubLevel::Read);
        assert_eq!(bad.refusal().unwrap().code, ErrorCode::Invalid);
    }
}
