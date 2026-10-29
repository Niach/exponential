//! MCP wiring vocabulary (EXP-1426 part 4c, out of `coding::argv`,
//! `coding::mcp_json` and `api::mcp_servers`, which re-export it): the
//! per-agent posture ([`AgentMcp`]), the launch's extra servers
//! ([`McpServerWire`], `${VAR}` references only, never a credential) and the
//! codex renderings of both. Pure data and pure functions.

use serde::{Deserialize, Serialize};

/// The worktree MCP config file claude's `--mcp-config` reads (`coding::mcp_json`
/// writes it; the name keeps claude's project-approval scan blind to it).
pub const MCP_JSON_FILE: &str = ".exp-mcp.json";

/// The env var carrying the raw `expu_` key for codex sessions (EXP-201) —
/// codex gets the MCP credential via the spawn environment instead of a
/// worktree file, reading it through `bearer_token_env_var`. Never on argv
/// (ps-visible), never on disk.
pub const MCP_TOKEN_ENV: &str = "EXP_MCP_TOKEN";

/// A TOML basic string (double-quoted, single line) for a `-c key=value`
/// override — codex parses the value as TOML, and its argv splitter must
/// never meet a raw newline or an unescaped quote. Backslash, quote and
/// every control character are escaped; everything else (UTF-8 included)
/// passes through verbatim.
pub fn toml_basic_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Per-agent MCP wiring, resolved by the launcher (step 4) and consumed by
/// `coding::argv::session_args`:
///
/// - Claude: the worktree `.exp-mcp.json` file (rides `coding::argv::mcp_config_args`).
/// - Codex: `-c mcp_servers.*` CLI overrides pointing at `url`, with the
///   bearer token read from [`MCP_TOKEN_ENV`] in the spawn env — the key
///   never lands on disk or argv for codex.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentMcp {
    ClaudeFile,
    /// EXP-746 (the ACP transport): claude gets the MCP server as INLINE
    /// `--mcp-config` JSON whose `Authorization` header reads
    /// `Bearer ${EXP_MCP_TOKEN}` — the CLI expands env refs inside header
    /// values (measured in the phase-1 spike), so the `expu_` key rides the
    /// child's env like codex and never lands on disk. The engine renders
    /// the JSON; this carries what it needs.
    ClaudeInline {
        url: String,
        /// The `X-Exp-Session-Id` header value (EXP-637).
        session_id: Option<String>,
    },
    CodexOverrides {
        url: String,
        /// EXP-637: the launched `sessions` row id, sent as the
        /// `X-Exp-Session-Id` HTTP header. `None` outside a launched
        /// session (agent shells) keeps the pre-EXP-637 argv byte-identical.
        session_id: Option<String>,
    },
}

/// EXP-792: ONE team MCP server resolved for a launch — the config
/// `mcpServers.resolveForLaunch` returned, with the ENV VAR NAMES the
/// launcher minted for the member's credential values. Every credential position is
/// a `${VAR}` REFERENCE the agent expands from the child's own environment
/// (claude header values, codex `bearer_token_env_var`/`env_http_headers`), so
/// no generated config ever carries a value. `exponential` itself is NOT one of these — it keeps its dedicated
/// [`AgentMcp`] posture (the `expu_` key + the session header); these are the
/// user-declared servers appended beside it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct McpServerWire {
    /// The `mcp_servers` row id (recorded in `runs.json` for resume).
    pub id: String,
    /// The config key (`mcpServers.<name>` / `mcp_servers.<name>`): the
    /// row's name lowercased with non-alphanumerics folded to `_`.
    pub name: String,
    pub transport: McpWireTransport,
    /// Header name → value, where a credential value is the literal
    /// `${VAR}` reference (`Authorization` → `Bearer ${EXP_MCP_TOKEN_1}` for
    /// an OAuth server). `http` only.
    pub headers: Vec<(String, String)>,
    /// The env var (in the spawn env) carrying a bearer token, when the
    /// server authenticates with one — codex's `bearer_token_env_var`.
    pub token_env: Option<String>,
    /// Env NAME → `${VAR}` reference for a `stdio` server's env.
    pub env: Vec<(String, String)>,
    /// FEED-73: whose connection this entry acts as (the transcript's
    /// `as <name>` audit). Never rendered into an agent config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<McpActor>,
}

/// Where a [`McpServerWire`] lives.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpWireTransport {
    Http { url: String },
    Stdio { command: String, args: Vec<String> },
}

impl McpServerWire {
    /// The config key a server row's `name` becomes: lowercase ASCII
    /// alphanumerics, everything else folded to `_`, never empty.
    pub fn config_key(name: &str) -> String {
        let key: String = name
            .trim()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        if key.is_empty() {
            "server".to_string()
        } else {
            key
        }
    }
}

/// The name of the env var a `${VAR}` reference names, when `value` is
/// EXACTLY one such reference (`${EXP_MCP_ENV_1_X_API_KEY}`); `None` for a
/// literal or a composite (`Bearer ${…}`, see [`bearer_env_reference`]).
pub fn env_reference(value: &str) -> Option<&str> {
    let inner = value.strip_prefix("${")?.strip_suffix('}')?;
    let valid = !inner.is_empty()
        && inner
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then_some(inner)
}

/// The env var a `Bearer ${VAR}` header value names — codex's
/// `bearer_token_env_var` spelling of an OAuth server's `Authorization`.
pub fn bearer_env_reference(value: &str) -> Option<&str> {
    let rest = value.strip_prefix("Bearer ")?;
    env_reference(rest.trim_start())
}

/// EXP-792: ONE `mcp_servers.<key>` entry as codex spells it — the model
/// BOTH codex renderers share (the shell's `-c mcp_servers={…}` inline TOML
/// table here, the app-server's `thread/start` config JSON in the engine),
/// so the two can never disagree on how a header or env reference is
/// expressed. Codex expands NO `${VAR}` syntax of its own; every reference
/// becomes one of its env-NAMED fields instead: `bearer_token_env_var` for
/// `Authorization: Bearer ${VAR}`, `env_http_headers` (`Header-Name → VAR`)
/// for a header that is exactly `${VAR}`, and `env_vars: [NAME…]` for a
/// stdio server (the launcher set `NAME=value` in the spawn env, so the
/// named parent var is forwarded as-is). Literal headers stay
/// `http_headers`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodexMcpEntry {
    pub url: Option<String>,
    pub bearer_token_env_var: Option<String>,
    pub http_headers: Vec<(String, String)>,
    pub env_http_headers: Vec<(String, String)>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env_vars: Vec<String>,
}

/// The WHOLE codex `mcp_servers` table for a launch: `exponential` FIRST
/// (its dedicated posture: url + `bearer_token_env_var = EXP_MCP_TOKEN` +
/// the `X-Exp-Session-Id` header when the run has a row), then every team
/// server in pick order under its config key.
pub fn codex_mcp_entries(
    url: &str,
    session_id: Option<&str>,
    servers: &[McpServerWire],
) -> Vec<(String, CodexMcpEntry)> {
    let mut entries = vec![(
        "exponential".to_string(),
        CodexMcpEntry {
            url: Some(url.to_string()),
            bearer_token_env_var: Some(MCP_TOKEN_ENV.to_string()),
            http_headers: session_id
                .map(|id| vec![("X-Exp-Session-Id".to_string(), id.to_string())])
                .unwrap_or_default(),
            ..Default::default()
        },
    )];
    for server in servers {
        let mut entry = CodexMcpEntry::default();
        match &server.transport {
            McpWireTransport::Http { url } => {
                entry.url = Some(url.clone());
                entry.bearer_token_env_var = server.token_env.clone();
                for (name, value) in &server.headers {
                    if let Some(var) = bearer_env_reference(value) {
                        if name.eq_ignore_ascii_case("authorization") {
                            // The bearer field IS this header; a second copy
                            // with an unexpanded `${…}` would shadow it.
                            if entry.bearer_token_env_var.is_none() {
                                entry.bearer_token_env_var = Some(var.to_string());
                            }
                            continue;
                        }
                    }
                    match env_reference(value) {
                        Some(var) => entry.env_http_headers.push((name.clone(), var.to_string())),
                        None => entry.http_headers.push((name.clone(), value.clone())),
                    }
                }
            }
            McpWireTransport::Stdio { command, args } => {
                entry.command = Some(command.clone());
                entry.args = args.clone();
                entry.env_vars = server
                    .env
                    .iter()
                    .map(|(name, value)| {
                        // `(NAME, "${NAME}")` by contract; a reference to a
                        // differently named var forwards THAT var.
                        env_reference(value).unwrap_or(name).to_string()
                    })
                    .collect();
            }
        }
        entries.push((server.name.clone(), entry));
    }
    entries
}

/// The codex shell's `-c mcp_servers={…}` value: the table from
/// [`codex_mcp_entries`] as ONE TOML inline table (the `-c key=value` parser
/// takes a whole table in one flag, which is what lets a launch carry any
/// number of servers without a flag explosion; verified parsed by codex
/// 0.144.5 for the inline `http_headers` form).
pub fn codex_mcp_servers_toml(
    url: &str,
    session_id: Option<&str>,
    servers: &[McpServerWire],
) -> String {
    let entries = codex_mcp_entries(url, session_id, servers);
    let rendered: Vec<String> = entries
        .iter()
        .map(|(key, entry)| format!("{}={}", toml_key(key), toml_entry(entry)))
        .collect();
    format!("{{{}}}", rendered.join(","))
}

fn toml_entry(entry: &CodexMcpEntry) -> String {
    let mut fields: Vec<String> = Vec::new();
    if let Some(url) = &entry.url {
        fields.push(format!("url={}", toml_basic_string(url)));
    }
    if let Some(var) = &entry.bearer_token_env_var {
        fields.push(format!("bearer_token_env_var={}", toml_basic_string(var)));
    }
    if !entry.http_headers.is_empty() {
        fields.push(format!("http_headers={}", toml_string_table(&entry.http_headers)));
    }
    if !entry.env_http_headers.is_empty() {
        fields.push(format!(
            "env_http_headers={}",
            toml_string_table(&entry.env_http_headers)
        ));
    }
    if let Some(command) = &entry.command {
        fields.push(format!("command={}", toml_basic_string(command)));
        fields.push(format!("args={}", toml_string_array(&entry.args)));
    }
    if !entry.env_vars.is_empty() {
        fields.push(format!("env_vars={}", toml_string_array(&entry.env_vars)));
    }
    format!("{{{}}}", fields.join(","))
}

fn toml_string_table(pairs: &[(String, String)]) -> String {
    let rendered: Vec<String> = pairs
        .iter()
        .map(|(name, value)| format!("{}={}", toml_key(name), toml_basic_string(value)))
        .collect();
    format!("{{{}}}", rendered.join(","))
}

fn toml_string_array(items: &[String]) -> String {
    let rendered: Vec<String> = items.iter().map(|item| toml_basic_string(item)).collect();
    format!("[{}]", rendered.join(","))
}

/// A TOML key: bare when it can be, quoted otherwise (`X-Exp-Session-Id` is
/// bare-legal; a header like `X Api Key` is not).
fn toml_key(key: &str) -> String {
    let bare = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare {
        key.to_string()
    } else {
        toml_basic_string(key)
    }
}

/// FEED-73: the member a resolved launch entry acts as.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpActor {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub name: String,
    /// Another member's SHARED connection (vs the launcher's own).
    #[serde(default)]
    pub shared: bool,
}
