//! #NEW-148 (@exp/emcp `fixtures/hub-client.json`, part of #NEW-143 D1):
//! the TEAM MCP SERVERS of a run go through the device's ONE hub
//! ([`emcp::hub::Hub`]) instead of the agent CLI's own config.
//!
//! [`route`] mounts every resolved team server on the hub FOR THAT RUN ONLY
//! (a run-scoped mount: another run never sees it), grants the run `write`
//! on all of them and leaves ONE wired entry, `team`, in
//! [`ResolvedMcp::servers`] (loopback HTTP, the run's bearer token behind
//! [`TOKEN_ENV`]). The credentials stay in this process: they never reach
//! the CLI's env or config any more ([`ResolvedMcp::redact`] keeps them
//! redacted).
//!
//! The hub is the MCP client of those servers (2026-07-28): it declares
//! elicitation (form + url), tasks and skills; a server's question becomes
//! ONE ACP elicitation in the run's session ([`eagent::engine::external`]),
//! a task it starts is polled by the hub and shows as a trace step, a 401
//! re-resolves the credentials through `mcpServers.resolveForLaunch` (the
//! server refreshes an expiring OAuth token; EXP-808 reopened) and retries
//! once.
//!
//! MIGRATION (tool names, hub-client.json `toolNameCases`): a team tool is
//! now hub.json's `<server>__<tool>` on the `team` entry — claude
//! `mcp__team__linear__create_issue` (was `mcp__linear__create_issue`), codex
//! server `team`, tool `linear__create_issue` (was server `linear`, tool
//! `create_issue`). A hub server name is the config key with `_` folded to
//! `-` ([`hub_name`]). Prompts, skills or allow-lists naming the old
//! `mcp__<server>__*` names must be updated.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use api::trpc::TrpcClient;
use emcp::contract::hub_client;
use emcp::hub::{AuditRow, AuditSink, Elicited, FetchedSkill, Hub, HubGrant, HubHost, Mount, TaskTrace};
use emcp::{ServerInfo, SessionId, StdioUpstream, Upstream};
use serde_json::Value;

use crate::argv::{McpServerWire, McpWireTransport};
use crate::mcp_servers::{substitute_env_refs, ResolvedMcp};

/// The CLI's one entry for the team servers (hub-client.json `entry.name`).
pub const ENTRY_NAME: &str = hub_client::ENTRY_NAME;
/// The env var carrying the run's hub token (`entry.tokenEnv`).
pub const TOKEN_ENV: &str = hub_client::TOKEN_ENV;

/// A run's credential refresher for one mounted server.
type Refresh = Arc<dyn Fn() -> Option<Mount> + Send + Sync>;

/// The hub's side of the device: asks and trace steps into the live run,
/// credentials re-resolved on a 401.
#[derive(Default)]
struct DeviceHost {
    refreshers: Mutex<HashMap<(String, String), Refresh>>,
}

impl HubHost for DeviceHost {
    fn elicit(&self, session: &SessionId, server: &str, params: Value, wait: Duration) -> Elicited {
        use eagent::engine::external::{elicit, Elicited as Asked};
        match elicit(session.as_str(), server, &params, wait) {
            Asked::Answered(result) => Elicited::Answered(result),
            Asked::NoSession => Elicited::NoSession,
            Asked::TimedOut => Elicited::TimedOut,
        }
    }

    fn task_trace(&self, session: &SessionId, trace: TaskTrace, result_text: Option<String>) {
        use eagent::engine::external::{trace as step, TraceStatus};
        step(session.as_str(), &trace.id, &trace.title, TraceStatus::parse(&trace.status), result_text.as_deref());
    }

    fn refresh(&self, session: &SessionId, server: &str) -> Option<Mount> {
        let refresh = self
            .refreshers
            .lock()
            .ok()?
            .get(&(session.as_str().to_string(), server.to_string()))
            .cloned()?;
        refresh()
    }
}

/// The run's calls are audited by the server's own `mcp_calls` for
/// Exponential's tools; a team server's calls are logged here at debug.
struct LogAudit;

impl AuditSink for LogAudit {
    fn record(&self, row: AuditRow) {
        log::debug!("[team-hub] {}", row.to_json());
    }
}

struct DeviceHub {
    hub: Hub,
    host: Arc<DeviceHost>,
}

static HUB: OnceLock<Result<DeviceHub, String>> = OnceLock::new();

/// The process's hub, served on first use; its task records live under
/// `data_dir` (hub-client.json `tasks.storeFile`).
fn device_hub(data_dir: &Path) -> Result<&'static DeviceHub, String> {
    HUB.get_or_init(|| {
        let info = ServerInfo { name: ENTRY_NAME.into(), version: env!("CARGO_PKG_VERSION").into(), instructions: None };
        let hub = Hub::serve(info, Arc::new(LogAudit))?;
        let host = Arc::new(DeviceHost::default());
        hub.set_host(host.clone());
        if let Err(err) = hub.set_task_store(data_dir.join(hub_client::TASK_STORE_FILE)) {
            log::warn!("coding: the team MCP hub's task store is unreadable: {err}");
        }
        install_stop_hook();
        Ok(DeviceHub { hub, host })
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// A session's Stop sends `tasks/cancel` for its live third-party tasks.
fn install_stop_hook() {
    eagent::engine::external::set_stop_hook(Arc::new(|session_id: &str| {
        if let (Some(Ok(hub)), Ok(session)) = (HUB.get(), SessionId::new(session_id)) {
            hub.hub.cancel_tasks(&session);
        }
    }));
}

/// The hub server name of a team server's config key (`linear_app` →
/// `linear-app`): hub.json's `serverNamePattern` has no `_`. `None` when
/// nothing valid is left.
pub fn hub_name(config_key: &str) -> Option<String> {
    let folded: String = config_key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let mut name = String::with_capacity(folded.len());
    for c in folded.chars() {
        if c == '-' && (name.is_empty() || name.ends_with('-')) {
            continue;
        }
        name.push(c);
    }
    let mut name: String = name.chars().take(emcp::contract::hub::SERVER_NAME_MAX).collect();
    while name.ends_with('-') {
        name.pop();
    }
    emcp::contract::hub::server_name_valid(&name).then_some(name)
}

/// The hub mount of one resolved team server: its `${VAR}` references
/// substituted from `env` (the values never leave this process).
fn mount_of(wire: &McpServerWire, env: &[(String, String)]) -> Mount {
    match &wire.transport {
        McpWireTransport::Http { url } => Mount::Upstream(Upstream {
            name: wire.name.clone(),
            url: url.clone(),
            headers: wire.headers.iter().map(|(name, value)| (name.clone(), substitute_env_refs(value, env))).collect(),
        }),
        McpWireTransport::Stdio { command, args } => Mount::Stdio(StdioUpstream {
            name: wire.name.clone(),
            command: command.clone(),
            args: args.clone(),
            env: wire.env.iter().map(|(name, value)| (name.clone(), substitute_env_refs(value, env))).collect(),
        }),
    }
}

/// What [`route`] mounted for a run: hub name → (server id, wire).
#[derive(Clone, Debug, Default)]
pub struct Mounted {
    pub servers: BTreeMap<String, (String, McpServerWire)>,
}

/// Mount the run's resolved team servers behind the hub and leave the ONE
/// `team` entry in `resolved.servers` (call it BEFORE the device's own
/// entries are attached: computer use, code mode, screen memory). A server
/// whose name folds to nothing valid, or one the hub refuses, is skipped
/// with a warning; a hub that cannot serve leaves the launch as resolved
/// (the servers stay direct entries, with a warning). `trpc` re-resolves a
/// server's credentials on a 401.
pub fn route(resolved: &mut ResolvedMcp, data_dir: &Path, trpc: Arc<TrpcClient>, session_id: &str, label: &str) -> Mounted {
    let mut mounted = Mounted::default();
    if resolved.servers.is_empty() {
        return mounted;
    }
    let hub = match device_hub(data_dir) {
        Ok(hub) => hub,
        Err(err) => {
            log::warn!("coding: the team MCP hub is unavailable, wiring the servers directly: {err}");
            return mounted;
        }
    };
    let Ok(session) = SessionId::new(session_id) else {
        return mounted;
    };
    let mut levels = serde_json::Map::new();
    for wire in &resolved.servers {
        let Some(name) = hub_name(&wire.name) else {
            resolved.warnings.push(format!("MCP server {}: the name cannot be served; starting without it.", wire.name));
            continue;
        };
        if mounted.servers.contains_key(&name) {
            resolved.warnings.push(format!("MCP server {}: another server already uses the name {name}; starting without it.", wire.name));
            continue;
        }
        if let Err(err) = hub.hub.mount_for(&session, &name, &wire.id, mount_of(wire, &resolved.env)) {
            resolved.warnings.push(format!("MCP server {}: {err}; starting without it.", wire.name));
            continue;
        }
        let refresh = refresher(trpc.clone(), session_id.to_string(), wire.id.clone(), wire.name.clone());
        if let Ok(mut refreshers) = hub.host.refreshers.lock() {
            refreshers.insert((session_id.to_string(), name.clone()), refresh);
        }
        levels.insert(name.clone(), Value::String("write".into()));
        mounted.servers.insert(name, (wire.id.clone(), wire.clone()));
    }
    let grant: HubGrant = match serde_json::from_value(serde_json::json!({ "servers": levels, "teamId": null, "userId": null })) {
        Ok(grant) => grant,
        Err(err) => {
            log::warn!("coding: the team MCP grant is malformed: {err}");
            return Mounted::default();
        }
    };
    let token = hub.hub.grant(&session, label, grant);
    // The routed servers under the names the run sees (FEED-73's note).
    resolved.routed = mounted
        .servers
        .iter()
        .map(|(name, (_, wire))| McpServerWire { name: name.clone(), ..wire.clone() })
        .collect();
    resolved.servers.clear();
    // The servers' own values stay redacted but never reach the spawn env.
    resolved.redact.extend(std::mem::take(&mut resolved.env).into_iter().map(|(_, value)| value));
    resolved.servers.push(McpServerWire {
        id: ENTRY_NAME.to_string(),
        name: ENTRY_NAME.to_string(),
        transport: McpWireTransport::Http { url: hub.hub.url().to_string() },
        headers: vec![("Authorization".to_string(), format!("Bearer ${{{TOKEN_ENV}}}"))],
        token_env: Some(TOKEN_ENV.to_string()),
        env: Vec::new(),
        actor: None,
    });
    resolved.env.push((TOKEN_ENV.to_string(), token.as_str().to_string()));
    mounted
}

/// EXP-808 (reopened by #NEW-148): ask the server for the run's credentials
/// again (it refreshes an expiring OAuth token) and rebuild the one mount.
fn refresher(trpc: Arc<TrpcClient>, session_id: String, server_id: String, config_key: String) -> Refresh {
    Arc::new(move || {
        let resolved = crate::mcp_servers::resolve(&trpc, None, &session_id);
        let wire = resolved.servers.iter().find(|wire| wire.id == server_id && wire.name == config_key)?;
        Some(mount_of(wire, &resolved.env))
    })
}

/// The run ended: its token and mounts go (a resume mounts again and picks
/// its tasks up). `ended` = the SESSION ended (not a release or a lost
/// run): its live tasks are cancelled too.
pub fn run_ended(session_id: &str, ended: bool) {
    let (Some(Ok(hub)), Ok(session)) = (HUB.get(), SessionId::new(session_id)) else {
        return;
    };
    if ended {
        hub.hub.cancel_tasks(&session);
    }
    hub.hub.revoke(&session);
    if let Ok(mut refreshers) = hub.host.refreshers.lock() {
        refreshers.retain(|(session, _), _| session != session_id);
    }
}

/// #NEW-148 skills: every skill of the run's mounted servers (no approval),
/// verified by the hub, written into the session's skill dir (team_skills)
/// and cached read-only outside every discovery path. Never a blocker:
/// every miss is a warning.
pub fn write_mcp_skills(
    data_dir: &Path,
    session_id: &str,
    mounted: &Mounted,
    skills: &mut crate::team_skills::ResolvedSkills,
) {
    if mounted.servers.is_empty() {
        return;
    }
    let (Some(Ok(hub)), Ok(session)) = (HUB.get(), SessionId::new(session_id)) else {
        return;
    };
    for (name, (server_id, _)) in &mounted.servers {
        let listed = match hub.hub.skills(&session, name) {
            Ok(listed) => listed,
            Err(err) => {
                skills.warnings.push(format!("MCP server {name}: could not list its skills ({err})."));
                continue;
            }
        };
        let (fetched, warnings) = listed;
        skills.warnings.extend(warnings);
        for skill in fetched {
            if let Err(err) = cache_skill(data_dir, server_id, &skill) {
                log::warn!("coding: could not cache MCP skill {}: {err}", skill.uri);
            }
            match crate::team_skills::write_mcp_skill(data_dir, session_id, name, &skill) {
                Ok(dir) => skills.dir = Some(dir),
                Err(err) => skills.warnings.push(format!("MCP skill {}: {err}", skill.uri)),
            }
        }
    }
}

/// `<data_dir>/mcp-skills-cache/<server id>/<digest set hex>/`: the verified
/// files, read-only, outside every discovery path; kept while the digest set
/// matches.
pub fn cache_dir(data_dir: &Path, server_id: &str, digest_set: &str) -> Option<PathBuf> {
    let hex = digest_set.strip_prefix("sha256:")?;
    let plain = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    (plain(server_id) && plain(hex)).then(|| data_dir.join(hub_client::SKILLS_CACHE_DIR).join(server_id).join(hex))
}

fn cache_skill(data_dir: &Path, server_id: &str, skill: &FetchedSkill) -> std::io::Result<()> {
    let Some(dir) = cache_dir(data_dir, server_id, &skill.digest_set) else {
        return Ok(());
    };
    if dir.exists() {
        return Ok(());
    }
    let staging = dir.with_extension(format!("tmp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    for (path, bytes) in &skill.files {
        let target = staging.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, bytes)?;
    }
    read_only(&staging)?;
    std::fs::rename(&staging, &dir)
}

#[cfg(unix)]
fn read_only(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            read_only(&path)?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o555))?;
        } else {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444))?;
        }
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o555))
}

#[cfg(not(unix))]
fn read_only(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            read_only(&path)?;
        } else {
            let mut permissions = std::fs::metadata(&path)?.permissions();
            permissions.set_readonly(true);
            std::fs::set_permissions(&path, permissions)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../../../packages/emcp/fixtures/hub-client.json")).expect("hub-client.json")
    }

    #[test]
    fn the_entry_matches_the_fixture() {
        let f = fixture();
        assert_eq!(f["entry"]["name"], ENTRY_NAME);
        assert_eq!(f["entry"]["tokenEnv"], TOKEN_ENV);
    }

    #[test]
    fn hub_names_fold_config_keys() {
        assert_eq!(hub_name("linear").as_deref(), Some("linear"));
        assert_eq!(hub_name("linear_app").as_deref(), Some("linear-app"));
        assert_eq!(hub_name("__x__y__").as_deref(), Some("x-y"));
        assert_eq!(hub_name("___"), None);
    }

    #[test]
    fn routing_leaves_one_team_entry_and_keeps_secrets_out_of_the_env() {
        let dir = crate::test_support::temp_dir("team-hub");
        let mut resolved = ResolvedMcp {
            servers: vec![McpServerWire {
                id: "srv-1".into(),
                name: "linear_app".into(),
                transport: McpWireTransport::Http { url: "http://127.0.0.1:9/mcp".into() },
                headers: vec![("Authorization".into(), "Bearer ${EXP_MCP_TOKEN_1}".into())],
                token_env: Some("EXP_MCP_TOKEN_1".into()),
                env: Vec::new(),
                actor: None,
            }],
            env: vec![("EXP_MCP_TOKEN_1".into(), "secret-token".into())],
            ..ResolvedMcp::default()
        };
        let trpc = Arc::new(TrpcClient::new("http://127.0.0.1:9", Arc::new(api::StaticToken("t".into()))));
        let mounted = route(&mut resolved, dir.0.as_path(), trpc, "sess-route-1", "EXP-1");
        assert_eq!(mounted.servers.keys().collect::<Vec<_>>(), vec!["linear-app"]);
        assert_eq!(resolved.servers.len(), 1);
        assert_eq!(resolved.servers[0].name, ENTRY_NAME);
        assert_eq!(resolved.routed.len(), 1);
        assert!(resolved.env.iter().all(|(name, _)| name == TOKEN_ENV), "{:?}", resolved.env.iter().map(|(n, _)| n).collect::<Vec<_>>());
        assert!(resolved.secret_values().contains(&"secret-token".to_string()));
        run_ended("sess-route-1", true);
    }

    #[test]
    fn the_cache_dir_is_outside_every_discovery_path() {
        let data = Path::new("/data");
        let dir = cache_dir(data, "srv-1", "sha256:abc").unwrap();
        assert_eq!(dir, Path::new("/data/mcp-skills-cache/srv-1/abc"));
        assert!(!dir.components().any(|c| matches!(c.as_os_str().to_str(), Some(".claude" | ".codex" | "skills"))));
        assert!(cache_dir(data, "../x", "sha256:abc").is_none());
        assert!(cache_dir(data, "srv", "md5:abc").is_none());
    }
}
