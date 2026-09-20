//! EXP-891: the MCP servers THIS MACHINE connects on its own runs — beside
//! the team registry of [`crate::mcp_servers`], and the answer to "I already
//! ran `claude mcp add …`, why does my run not see it?" (the launcher passes
//! `--strict-mcp-config`, so the user's own config is invisible to a run).
//!
//! The local store (`{data_dir}/mcp/device-servers.json`) is the source of
//! truth; the server keeps a per-device copy (`deviceMcpServers.sync`, the
//! whole set replaced on every change + once per sweep when the key moved)
//! so the web can show it per device, read-only. Rows come from two places:
//!
//! - **detected**: what the local agent configs already list —
//!   `mcpServers` in claude's `.claude.json` (the system dir + every
//!   profile, EXP-849) and `[mcp_servers.*]` in codex's `config.toml`.
//!   [`detect`] reads them; [`import`] adds the ones not yet in the store.
//!   Only the config is copied — a url or a command + args. An entry that
//!   carries header/env VALUES is reported as skipped with the reason: the
//!   values are that agent's secrets, and this crate never copies one
//!   (the team registry's `secret` kind is the place for those).
//! - **manual**: typed here (the desktop pane, `exponential mcp add`).
//!
//! Every ENABLED row is appended to every run this device starts
//! ([`wires`], merged into the launch by the launcher after the team pick,
//! a team server folding to the same config key winning). No credential
//! position: whatever auth claude/codex already holds for that server stays
//! where the agent keeps it (claude keys its MCP OAuth tokens by server
//! name + url, so a detected row connects the way the user's own session
//! would).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use api::device_mcp_servers::DeviceMcpServerInput;
use api::trpc::TrpcClient;
use serde::{Deserialize, Serialize};

use crate::agent::CodingAgent;
use crate::agent_profiles;
use crate::argv::{McpServerWire, McpWireTransport};
use crate::mcp_servers::RESERVED_CONFIG_KEY;

/// The store file, under the app data dir (one machine, every account).
pub const STORE_FILE: &str = "device-servers.json";
const STORE_DIR: &str = "mcp";
const STORE_VERSION: u32 = 1;

/// Rows one machine may hold (the router refuses more).
pub const MAX_SERVERS: usize = 64;
pub const MAX_NAME: usize = 64;

pub const SOURCE_DETECTED: &str = "detected";
pub const SOURCE_MANUAL: &str = "manual";

/// One row of the machine's set.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalServer {
    pub id: String,
    pub name: String,
    /// `http` | `stdio`.
    pub transport: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// `detected` | `manual`.
    pub source: String,
    /// `claude` | `codex` for a detected row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub enabled: bool,
}

impl LocalServer {
    pub fn is_http(&self) -> bool {
        self.transport != "stdio"
    }

    /// The URL for http, the command line for stdio.
    pub fn target(&self) -> String {
        if self.is_http() {
            return self.url.clone().unwrap_or_default();
        }
        let mut parts = vec![self.command.clone().unwrap_or_default()];
        parts.extend(self.args.iter().cloned());
        parts.join(" ")
    }

    /// `Detected from Claude Code` / `Detected from Codex` / `Added on the
    /// device` (the web `deviceMcpServerSourceLabel`).
    pub fn source_label(&self) -> &'static str {
        if self.source == SOURCE_DETECTED {
            match self.agent.as_deref() {
                Some("claude") => "Detected from Claude Code",
                Some("codex") => "Detected from Codex",
                _ => "Detected on the device",
            }
        } else {
            "Added on the device"
        }
    }

    fn to_input(&self) -> DeviceMcpServerInput {
        DeviceMcpServerInput {
            name: self.name.clone(),
            transport: self.transport.clone(),
            url: if self.is_http() { self.url.clone() } else { None },
            command: if self.is_http() { None } else { self.command.clone() },
            args: if self.is_http() { Vec::new() } else { self.args.clone() },
            source: self.source.clone(),
            agent: self.agent.clone(),
            enabled: self.enabled,
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct StoreFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    servers: Vec<LocalServer>,
}

pub fn store_path(data_dir: &Path) -> PathBuf {
    data_dir.join(STORE_DIR).join(STORE_FILE)
}

/// The machine's set, name order (case-insensitive). A missing or unreadable
/// file is an empty set — never an error a launch would trip over.
pub fn load(data_dir: &Path) -> Vec<LocalServer> {
    let Ok(raw) = fs::read_to_string(store_path(data_dir)) else {
        return Vec::new();
    };
    let file: StoreFile = match serde_json::from_str(&raw) {
        Ok(file) => file,
        Err(error) => {
            log::warn!("device MCP store unreadable, treating as empty: {error}");
            return Vec::new();
        }
    };
    let mut servers = file.servers;
    servers.retain(|server| !server.name.trim().is_empty());
    sort(&mut servers);
    servers
}

fn sort(servers: &mut [LocalServer]) {
    servers.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
}

pub fn save(data_dir: &Path, servers: &[LocalServer]) -> Result<(), String> {
    let mut servers = servers.to_vec();
    sort(&mut servers);
    let file = StoreFile {
        version: STORE_VERSION,
        servers,
    };
    let path = store_path(data_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let body = serde_json::to_string_pretty(&file).map_err(|error| error.to_string())?;
    api::atomic_file::write_atomic(&path, &format!("{body}\n"))
        .map_err(|error| format!("write {}: {error}", path.display()))
}

fn same_name(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// The web `validateDeviceMcpServerInput`, string for string: what the
/// router would refuse, said before the round trip.
pub fn validate(server: &LocalServer) -> Result<(), String> {
    let name = server.name.trim();
    if name.is_empty() {
        return Err("A server needs a name".to_string());
    }
    if name.chars().count() > MAX_NAME {
        return Err(format!("Server names are at most {MAX_NAME} characters"));
    }
    if McpServerWire::config_key(name) == RESERVED_CONFIG_KEY {
        return Err(format!("The name {name} is reserved"));
    }
    if server.is_http() {
        let url = server.url.as_deref().unwrap_or("").trim();
        if url.is_empty() {
            return Err(format!("{name}: an http server needs a URL"));
        }
        let lower = url.to_ascii_lowercase();
        let loopback = lower.starts_with("http://localhost")
            || lower.starts_with("http://127.0.0.1");
        if !lower.starts_with("https://") && !loopback {
            return Err(format!(
                "{name}: the URL must be https:// (http:// only for localhost)"
            ));
        }
    } else if server.command.as_deref().unwrap_or("").trim().is_empty() {
        return Err(format!("{name}: a stdio server needs a command"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// One entry a local agent config lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detected {
    pub name: String,
    /// `claude` | `codex`.
    pub agent: &'static str,
    /// The config file it was read from.
    pub origin: PathBuf,
    pub transport: String,
    pub url: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    /// Why it cannot be imported as-is (header/env values, an unsupported
    /// transport) — `None` = importable.
    pub skipped: Option<String>,
}

impl Detected {
    pub fn target(&self) -> String {
        if self.transport != "stdio" {
            return self.url.clone().unwrap_or_default();
        }
        let mut parts = vec![self.command.clone().unwrap_or_default()];
        parts.extend(self.args.iter().cloned());
        parts.join(" ")
    }

    fn to_local(&self) -> LocalServer {
        LocalServer {
            id: uuid::Uuid::new_v4().to_string(),
            name: self.name.clone(),
            transport: self.transport.clone(),
            url: self.url.clone(),
            command: self.command.clone(),
            args: self.args.clone(),
            source: SOURCE_DETECTED.to_string(),
            agent: Some(self.agent.to_string()),
            enabled: true,
        }
    }
}

/// Every claude `.claude.json` and codex `config.toml` this machine keeps:
/// the ambient one per agent (the `system` profile: `$CLAUDE_CONFIG_DIR` /
/// `$CODEX_HOME` or the home default) plus every custom profile's dir.
pub fn config_files(data_dir: &Path) -> Vec<(CodingAgent, PathBuf)> {
    let mut out = Vec::new();
    for agent in CodingAgent::ALL {
        let mut dirs: Vec<Option<PathBuf>> = vec![None];
        for profile in agent_profiles::list(data_dir, agent) {
            if profile.is_system() {
                continue;
            }
            if let Some(dir) = agent_profiles::profile_dir(data_dir, agent, &profile.id) {
                dirs.push(Some(dir));
            }
        }
        for dir in dirs {
            let path = match agent {
                CodingAgent::Claude => crate::claude_trust::claude_config_path(dir.as_deref()),
                CodingAgent::Codex => {
                    crate::codex_trust::codex_home(dir.as_deref()).map(|home| home.join("config.toml"))
                }
            };
            if let Some(path) = path {
                if !out.iter().any(|(_, known): &(CodingAgent, PathBuf)| known == &path) {
                    out.push((agent, path));
                }
            }
        }
    }
    out
}

/// What the local agent configs list right now, de-duplicated by name
/// (first config wins; the ambient one is read first). `exponential` itself
/// is never a candidate.
pub fn detect(data_dir: &Path) -> Vec<Detected> {
    let mut out: Vec<Detected> = Vec::new();
    for (agent, path) in config_files(data_dir) {
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let found = match agent {
            CodingAgent::Claude => detect_claude(&raw, &path),
            CodingAgent::Codex => detect_codex(&raw, &path),
        };
        for candidate in found {
            if McpServerWire::config_key(&candidate.name) == RESERVED_CONFIG_KEY {
                continue;
            }
            if out.iter().any(|known| same_name(&known.name, &candidate.name)) {
                continue;
            }
            out.push(candidate);
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

const SKIP_VALUES: &str = "carries header or env values this machine will not copy; add it to the team registry with a secret instead";

/// `.claude.json` → its top-level `mcpServers` (the user scope `claude mcp
/// add` writes). `type` defaults to `stdio` when a command is present.
fn detect_claude(raw: &str, origin: &Path) -> Vec<Detected> {
    let Ok(root) = serde_json::from_str::<serde_json::Value>(raw) else {
        log::debug!("{}: not JSON, no MCP servers detected", origin.display());
        return Vec::new();
    };
    let Some(servers) = root.get("mcpServers").and_then(|v| v.as_object()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name, entry) in servers {
        let kind = entry
            .get("type")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| {
                if entry.get("command").is_some() {
                    "stdio".to_string()
                } else {
                    "http".to_string()
                }
            });
        let url = entry.get("url").and_then(|v| v.as_str()).map(str::to_string);
        let command = entry.get("command").and_then(|v| v.as_str()).map(str::to_string);
        let args = entry
            .get("args")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let has_values = ["headers", "env"]
            .iter()
            .any(|key| entry.get(*key).and_then(|v| v.as_object()).is_some_and(|map| !map.is_empty()));
        let skipped = match kind.as_str() {
            "http" | "stdio" if has_values => Some(SKIP_VALUES.to_string()),
            "http" | "stdio" => None,
            other => Some(format!("transport `{other}` is not supported (http or stdio only)")),
        };
        out.push(Detected {
            name: name.clone(),
            agent: "claude",
            origin: origin.to_path_buf(),
            transport: if kind == "stdio" { "stdio".into() } else { "http".into() },
            url,
            command,
            args,
            skipped,
        });
    }
    out
}

/// `config.toml` → `[mcp_servers.<name>]`: `url` = http, `command` = stdio.
fn detect_codex(raw: &str, origin: &Path) -> Vec<Detected> {
    let Ok(root) = toml::from_str::<toml::Value>(raw) else {
        log::debug!("{}: not TOML, no MCP servers detected", origin.display());
        return Vec::new();
    };
    let Some(servers) = root.get("mcp_servers").and_then(toml::Value::as_table) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name, entry) in servers {
        let Some(entry) = entry.as_table() else {
            continue;
        };
        let url = entry.get("url").and_then(toml::Value::as_str).map(str::to_string);
        let command = entry.get("command").and_then(toml::Value::as_str).map(str::to_string);
        let args = entry
            .get("args")
            .and_then(toml::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let has_values = [
            "http_headers",
            "env_http_headers",
            "bearer_token_env_var",
            "env",
        ]
        .iter()
        .any(|key| entry.get(*key).is_some());
        let transport = if command.is_some() { "stdio" } else { "http" };
        let skipped = if has_values {
            Some(SKIP_VALUES.to_string())
        } else if url.is_none() && command.is_none() {
            Some("neither a url nor a command".to_string())
        } else {
            None
        };
        out.push(Detected {
            name: name.clone(),
            agent: "codex",
            origin: origin.to_path_buf(),
            transport: transport.to_string(),
            url,
            command,
            args,
            skipped,
        });
    }
    out
}

/// The detected entries the store does not hold yet — what an import adds.
pub fn importable<'a>(detected: &'a [Detected], held: &[LocalServer]) -> Vec<&'a Detected> {
    detected
        .iter()
        .filter(|candidate| candidate.skipped.is_none())
        .filter(|candidate| !held.iter().any(|row| same_name(&row.name, &candidate.name)))
        .collect()
}

// ---------------------------------------------------------------------------
// Edits (each one saves the store; the host syncs after)
// ---------------------------------------------------------------------------

/// Import every importable detected entry (enabled). Returns the rows added.
pub fn import(data_dir: &Path) -> Result<Vec<LocalServer>, String> {
    let mut held = load(data_dir);
    let detected = detect(data_dir);
    let mut added = Vec::new();
    for candidate in importable(&detected, &held) {
        if held.len() + added.len() >= MAX_SERVERS {
            break;
        }
        added.push(candidate.to_local());
    }
    if added.is_empty() {
        return Ok(added);
    }
    held.extend(added.iter().cloned());
    save(data_dir, &held)?;
    Ok(added)
}

/// Add a typed row. Refuses a duplicate name and anything [`validate`]
/// would; the row is stored enabled unless said otherwise.
pub fn add(data_dir: &Path, mut server: LocalServer) -> Result<LocalServer, String> {
    server.name = server.name.trim().to_string();
    server.source = SOURCE_MANUAL.to_string();
    server.agent = None;
    if server.id.trim().is_empty() {
        server.id = uuid::Uuid::new_v4().to_string();
    }
    validate(&server)?;
    let mut held = load(data_dir);
    if held.iter().any(|row| same_name(&row.name, &server.name)) {
        return Err(format!("{} is already on this machine", server.name));
    }
    if held.len() >= MAX_SERVERS {
        return Err(format!("This machine already holds {MAX_SERVERS} servers"));
    }
    let key = McpServerWire::config_key(&server.name);
    if let Some(clash) = held
        .iter()
        .find(|row| McpServerWire::config_key(&row.name) == key)
    {
        return Err(format!(
            "{} and {} would share the config key {key}",
            server.name, clash.name
        ));
    }
    held.push(server.clone());
    save(data_dir, &held)?;
    Ok(server)
}

/// `name` → `url` when it parses as http(s), else a command line split on
/// whitespace: the one-field add the desktop pane and the CLI share.
pub fn parse_target(name: &str, target: &str) -> LocalServer {
    let target = target.trim();
    let lower = target.to_ascii_lowercase();
    let http = lower.starts_with("http://") || lower.starts_with("https://");
    if http {
        return LocalServer {
            id: String::new(),
            name: name.to_string(),
            transport: "http".into(),
            url: Some(target.to_string()),
            command: None,
            args: Vec::new(),
            source: SOURCE_MANUAL.into(),
            agent: None,
            enabled: true,
        };
    }
    let mut parts = target.split_whitespace().map(str::to_string);
    LocalServer {
        id: String::new(),
        name: name.to_string(),
        transport: "stdio".into(),
        url: None,
        command: parts.next(),
        args: parts.collect(),
        source: SOURCE_MANUAL.into(),
        agent: None,
        enabled: true,
    }
}

/// Switch a row on or off. `Ok(false)` when no row has that name.
pub fn set_enabled(data_dir: &Path, name: &str, enabled: bool) -> Result<bool, String> {
    let mut held = load(data_dir);
    let Some(row) = held.iter_mut().find(|row| same_name(&row.name, name)) else {
        return Ok(false);
    };
    if row.enabled == enabled {
        return Ok(true);
    }
    row.enabled = enabled;
    save(data_dir, &held)?;
    Ok(true)
}

/// Forget a row on this machine (the agent's own config is untouched).
/// `Ok(false)` when no row has that name.
pub fn remove(data_dir: &Path, name: &str) -> Result<bool, String> {
    let mut held = load(data_dir);
    let before = held.len();
    held.retain(|row| !same_name(&row.name, name));
    if held.len() == before {
        return Ok(false);
    }
    save(data_dir, &held)?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// Launch
// ---------------------------------------------------------------------------

/// The enabled rows as the adapters render them — no headers, no env: the
/// agent connects the way the user's own session would. Two rows folding
/// to one config key keep the first (name order); the reserved key and an
/// invalid row are dropped, never a launch blocker (they are the user's own
/// machine-local extras, not a pick they asked for).
pub fn wires(data_dir: &Path) -> Vec<McpServerWire> {
    wires_of(&load(data_dir))
}

pub fn wires_of(servers: &[LocalServer]) -> Vec<McpServerWire> {
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();
    let mut out = Vec::new();
    for server in servers.iter().filter(|server| server.enabled) {
        if validate(server).is_err() {
            continue;
        }
        let key = McpServerWire::config_key(&server.name);
        if key == RESERVED_CONFIG_KEY || seen.contains_key(&key) {
            continue;
        }
        seen.insert(key.clone(), ());
        let transport = if server.is_http() {
            McpWireTransport::Http {
                url: server.url.clone().unwrap_or_default(),
            }
        } else {
            McpWireTransport::Stdio {
                command: server.command.clone().unwrap_or_default(),
                args: server.args.clone(),
            }
        };
        out.push(McpServerWire {
            id: server.id.clone(),
            name: key,
            transport,
            headers: Vec::new(),
            token_env: None,
            env: Vec::new(),
        });
    }
    out
}

/// Append the device rows to a launch's team pick: a team server that
/// folds to the same config key wins (the user asked for THAT one by name).
pub fn merge_into(servers: &mut Vec<McpServerWire>, device: Vec<McpServerWire>) {
    for wire in device {
        if servers.iter().any(|known| known.name == wire.name) {
            continue;
        }
        servers.push(wire);
    }
}

// ---------------------------------------------------------------------------
// Sync
// ---------------------------------------------------------------------------

/// A stable digest of the set — the sweep's change detector.
pub fn sync_key(servers: &[LocalServer]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for server in servers {
        hasher.update(server.to_input_json().as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

impl LocalServer {
    fn to_input_json(&self) -> String {
        serde_json::to_string(&self.to_input()).unwrap_or_default()
    }
}

/// Push the machine's set to the server NOW (after an edit).
pub fn sync_now(data_dir: &Path, trpc: &TrpcClient, device_id: &str) -> Result<usize, api::ApiError> {
    let inputs: Vec<DeviceMcpServerInput> = load(data_dir).iter().map(LocalServer::to_input).collect();
    api::device_mcp_servers::sync(trpc, device_id, &inputs)
}

/// The sweep-side state: the key last pushed, so an unchanged set costs a
/// file read per beat and no request.
#[derive(Default)]
pub struct SyncState {
    sent_key: Option<String>,
}

impl SyncState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Re-push on the next sweep whatever the key says.
    pub fn invalidate(&mut self) {
        self.sent_key = None;
    }

    /// Push when the set moved since the last successful push. Failures
    /// are logged and retried on the next beat. A machine with NO store file
    /// never pushes: nothing was ever held here, so there is nothing to
    /// clear either (an emptied store still exists, and does push its
    /// empty set).
    pub fn sweep(&mut self, data_dir: &Path, trpc: &TrpcClient, device_id: &str) {
        if !store_path(data_dir).is_file() {
            return;
        }
        let servers = load(data_dir);
        let key = sync_key(&servers);
        if self.sent_key.as_deref() == Some(key.as_str()) {
            return;
        }
        let inputs: Vec<DeviceMcpServerInput> = servers.iter().map(LocalServer::to_input).collect();
        match api::device_mcp_servers::sync(trpc, device_id, &inputs) {
            Ok(_) => self.sent_key = Some(key),
            Err(error) => log::debug!("deviceMcpServers.sync failed (retrying next beat): {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-device-mcp-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn http(name: &str, url: &str) -> LocalServer {
        LocalServer {
            id: format!("id-{name}"),
            name: name.into(),
            transport: "http".into(),
            url: Some(url.into()),
            command: None,
            args: Vec::new(),
            source: SOURCE_MANUAL.into(),
            agent: None,
            enabled: true,
        }
    }

    const CLAUDE_JSON: &str = r#"{
      "numStartups": 3,
      "mcpServers": {
        "linear": { "type": "http", "url": "https://mcp.linear.app/mcp" },
        "exponential": { "type": "http", "url": "https://app.exponential.at/api/mcp" },
        "github": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-github"], "env": { "GITHUB_TOKEN": "ghp_x" } },
        "fs": { "type": "stdio", "command": "mcp-fs", "args": ["/tmp"] },
        "legacy": { "type": "sse", "url": "https://old.example/sse" }
      },
      "projects": {}
    }"#;

    const CODEX_TOML: &str = r#"
model = "gpt-5"

[mcp_servers.sentry]
url = "https://mcp.sentry.dev/mcp"

[mcp_servers.acme]
command = "npx"
args = ["-y", "@acme/mcp"]

[mcp_servers.keyed]
url = "https://keyed.example/mcp"
bearer_token_env_var = "KEYED_TOKEN"

[mcp_servers.Linear]
url = "https://mcp.linear.app/mcp"

[projects."/tmp/x"]
trust_level = "trusted"
"#;

    /// claude: `type` defaults off `command`; `exponential` is never a
    /// candidate; an entry with env/header values is reported skipped; sse
    /// is unsupported.
    #[test]
    fn detects_claude_user_scope_servers() {
        let found = detect_claude(CLAUDE_JSON, Path::new("/home/u/.claude.json"));
        let by_name = |name: &str| found.iter().find(|d| d.name == name).unwrap();
        assert_eq!(by_name("linear").transport, "http");
        assert_eq!(by_name("linear").url.as_deref(), Some("https://mcp.linear.app/mcp"));
        assert!(by_name("linear").skipped.is_none());
        assert_eq!(by_name("fs").transport, "stdio");
        assert_eq!(by_name("fs").target(), "mcp-fs /tmp");
        assert_eq!(by_name("github").transport, "stdio");
        assert!(by_name("github").skipped.as_deref().unwrap().contains("env values"));
        assert!(by_name("legacy").skipped.as_deref().unwrap().contains("sse"));
        // `exponential` is only dropped by `detect` (the reserved-key
        // filter is above the per-file parser) — the parser lists it.
        assert!(found.iter().any(|d| d.name == "exponential"));
    }

    #[test]
    fn detects_codex_config_servers() {
        let found = detect_codex(CODEX_TOML, Path::new("/home/u/.codex/config.toml"));
        let by_name = |name: &str| found.iter().find(|d| d.name == name).unwrap();
        assert_eq!(by_name("sentry").transport, "http");
        assert_eq!(by_name("acme").transport, "stdio");
        assert_eq!(by_name("acme").target(), "npx -y @acme/mcp");
        assert!(by_name("keyed").skipped.is_some());
        assert_eq!(by_name("Linear").agent, "codex");
    }

    /// The store: round trip, name order, an unreadable file reads empty.
    #[test]
    fn store_round_trips_sorted() {
        let dir = temp_dir("store");
        assert!(load(&dir).is_empty());
        save(&dir, &[http("zed", "https://z/mcp"), http("Atlas", "https://a/mcp")]).unwrap();
        let held = load(&dir);
        assert_eq!(held.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Atlas", "zed"]);
        fs::write(store_path(&dir), "not json").unwrap();
        assert!(load(&dir).is_empty());
    }

    #[test]
    fn validate_mirrors_the_router() {
        assert!(validate(&http("linear", "https://x/mcp")).is_ok());
        assert!(validate(&http("local", "http://localhost:3333/mcp")).is_ok());
        assert!(validate(&http("plain", "http://x/mcp")).unwrap_err().contains("https"));
        assert!(validate(&http("", "https://x/mcp")).unwrap_err().contains("name"));
        assert!(validate(&http("Exponential", "https://x/mcp")).unwrap_err().contains("reserved"));
        let mut stdio = http("cmd", "");
        stdio.transport = "stdio".into();
        assert!(validate(&stdio).unwrap_err().contains("command"));
        stdio.command = Some("npx".into());
        assert!(validate(&stdio).is_ok());
    }

    #[test]
    fn add_refuses_duplicates_and_key_clashes() {
        let dir = temp_dir("add");
        add(&dir, parse_target("linear", "https://mcp.linear.app/mcp")).unwrap();
        let dup = add(&dir, parse_target("LINEAR", "https://other/mcp")).unwrap_err();
        assert!(dup.contains("already"), "{dup}");
        add(&dir, parse_target("my server", "https://mine/mcp")).unwrap();
        let clash = add(&dir, parse_target("my_server", "https://other/mcp")).unwrap_err();
        assert!(clash.contains("config key my_server"), "{clash}");
        let stdio = add(&dir, parse_target("acme", "npx -y @acme/mcp")).unwrap();
        assert_eq!(stdio.transport, "stdio");
        assert_eq!(stdio.command.as_deref(), Some("npx"));
        assert_eq!(stdio.args, ["-y", "@acme/mcp"]);
        assert_eq!(stdio.source, SOURCE_MANUAL);
        assert!(!stdio.id.is_empty());
        assert_eq!(load(&dir).len(), 3);
    }

    #[test]
    fn enable_disable_remove() {
        let dir = temp_dir("toggle");
        add(&dir, parse_target("linear", "https://mcp.linear.app/mcp")).unwrap();
        assert!(set_enabled(&dir, "Linear", false).unwrap());
        assert!(!load(&dir)[0].enabled);
        assert!(!set_enabled(&dir, "nope", false).unwrap());
        assert!(remove(&dir, "linear").unwrap());
        assert!(!remove(&dir, "linear").unwrap());
        assert!(load(&dir).is_empty());
    }

    /// Only enabled, valid rows reach the wire; the reserved key and a key
    /// clash drop silently; no headers or env ride along.
    #[test]
    fn wires_take_enabled_valid_rows_only() {
        let mut off = http("off", "https://off/mcp");
        off.enabled = false;
        let mut broken = http("broken", "");
        broken.url = None;
        let rows = vec![
            http("my server", "https://mcp.linear.app/mcp"),
            http("my_server", "https://dup/mcp"),
            off,
            broken,
            http("exponential", "https://x/mcp"),
        ];
        let wires = wires_of(&rows);
        assert_eq!(wires.len(), 1);
        assert_eq!(wires[0].name, "my_server");
        assert_eq!(wires[0].id, "id-my server");
        assert!(wires[0].headers.is_empty());
        assert!(wires[0].env.is_empty());
        assert!(wires[0].token_env.is_none());
        assert_eq!(
            wires[0].transport,
            McpWireTransport::Http { url: "https://mcp.linear.app/mcp".into() }
        );
    }

    #[test]
    fn merge_lets_the_team_pick_win_on_a_key_clash() {
        let team = McpServerWire {
            id: "team-1".into(),
            name: "linear".into(),
            transport: McpWireTransport::Http { url: "https://team/mcp".into() },
            headers: vec![("Authorization".into(), "Bearer ${EXP_MCP_TOKEN_1}".into())],
            token_env: Some("EXP_MCP_TOKEN_1".into()),
            env: Vec::new(),
        };
        let mut servers = vec![team.clone()];
        merge_into(
            &mut servers,
            wires_of(&[http("linear", "https://mine/mcp"), http("sentry", "https://s/mcp")]),
        );
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0], team);
        assert_eq!(servers[1].name, "sentry");
    }

    #[test]
    fn importable_skips_held_names_and_skipped_entries() {
        let detected = detect_claude(CLAUDE_JSON, Path::new("/c"));
        let held = vec![http("LINEAR", "https://held/mcp")];
        let names: Vec<&str> = importable(&detected, &held).iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["exponential", "fs"]);
    }

    #[test]
    fn sync_key_moves_with_the_set() {
        let a = vec![http("linear", "https://x/mcp")];
        let mut b = a.clone();
        b[0].enabled = false;
        assert_ne!(sync_key(&a), sync_key(&b));
        assert_eq!(sync_key(&a), sync_key(&a.clone()));
        assert_ne!(sync_key(&a), sync_key(&[]));
    }
}
