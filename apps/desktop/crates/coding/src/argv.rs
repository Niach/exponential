//! Spawn argv assembly for coding sessions — the ONE place the agent CLI
//! flags are composed (EXP-201: `claude`, `codex`, or `pi`). The model flag
//! is explicit for Claude (never the user's CLI default — §7.7, locked
//! 2026-07-03; codex/pi allow blank = their own default), the seed prompt
//! rides argv positional-last (bytes typed into the PTY before the TUI
//! enters raw mode get swallowed, so the prompt must never ride stdin), and
//! the permission posture is per-agent:
//!
//! - **claude** — EXP-690: the classic `--dangerously-skip-permissions` on
//!   every run, or plan mode when gated (plan wins the STARTING mode, with
//!   the bypass one Shift+Tab away). The doctor's
//!   [`crate::doctor::MIN_CLAUDE_VERSION`] gate guarantees every claude flag
//!   here.
//! - **codex** — EXP-690: always `--dangerously-bypass-approvals-and-sandbox`
//!   (the workspace-write Auto preset is gone). `--full-auto` is deprecated
//!   and never used. Every codex argv also disables the startup update prompt
//!   (EXP-389 — it parks an unattended session; the directory-trust screen is
//!   handled separately by [`crate::codex_trust`], because
//!   `-c projects.….trust_level` cannot express paths containing dots).
//! - **pi** — no permission system exists; no flags either way.

use crate::agent::CodingAgent;
use crate::mcp_json::MCP_JSON_FILE;
use crate::pi_bridge::{PI_BRIDGE_FILE, PI_PLAN_FILE};
use crate::settings::Settings;
use crate::skill::RUN_SKILL;

/// The env var carrying the raw `expu_` key for codex + pi sessions (EXP-201)
/// — those agents get the MCP credential via the spawn environment instead of
/// a worktree file: codex reads it through `bearer_token_env_var`, the pi
/// bridge reads it directly. Never on argv (ps-visible), never on disk.
pub const MCP_TOKEN_ENV: &str = "EXP_MCP_TOKEN";

/// The env var carrying the `/api/mcp` URL for the pi bridge.
pub const MCP_URL_ENV: &str = "EXP_MCP_URL";

/// EXP-637: the `coding_sessions` row id the spawned agent's MCP calls must
/// identify themselves with (`X-Exp-Session-Id`). Read by the pi bridge (pi
/// has no native MCP headers); claude gets it from `.exp-mcp.json` and codex
/// from a `-c mcp_servers.exponential.http_headers` override. NOT a secret —
/// it only names the row the launcher just created for this run.
pub const MCP_SESSION_ID_ENV: &str = "EXP_MCP_SESSION_ID";

/// FEED-25: claude's per-call MCP wall-clock timeout, in milliseconds. The
/// CLI (2.1.263) defaults it to 1e8 ms — 27 hours, i.e. none — so a
/// `tools/call` whose response is lost inside its HTTP client never settles
/// and the whole turn wedges: every later steer message is enqueued and never
/// dequeued, and the run sits "running" for hours. With the variable set, a
/// lost response surfaces as an ordinary tool error the model recovers from.
/// Generous against everything `/api/mcp` does (`sessions_start` polls 10 s,
/// GitHub-backed diffs take seconds); a user's own value in the host env
/// wins ([`crate::launcher`] only fills the gap).
pub const CLAUDE_MCP_TOOL_TIMEOUT_ENV: &str = "MCP_TOOL_TIMEOUT";
pub const CLAUDE_MCP_TOOL_TIMEOUT_MS: u64 = 120_000;

/// Spawn-env gate for the pi plan-mode extension (EXP-441): the launcher
/// sets it to `1` on a pi launch with plan mode on. The extension file
/// itself rides `-e` unconditionally and is inert without this value.
pub const PI_PLAN_MODE_ENV: &str = "EXP_PI_PLAN_MODE";

/// EXP-443: codex's per-spawn originator override — the value lands verbatim
/// in every rollout meta this spawn writes, giving the activity emitter a
/// discriminator against foreign codex processes sharing the cwd. NOT one of
/// the steer↔coding mirrored env pairs: steer never reads the env, it reads
/// the value back OUT of the rollout meta. The name is codex's own internal
/// override; if a codex build ignores it, discovery degrades to the pre-fix
/// cwd-only match (see `steer::codex_activity::find_live_rollout`).
pub const CODEX_ORIGINATOR_ENV: &str = "CODEX_INTERNAL_ORIGINATOR_OVERRIDE";

/// The originator stamped onto codex AGENT SHELLS (EXP-443): shells share the
/// trunk-clone cwd with action runs, so they must carry an originator that no
/// session emitter's strict pass can ever match.
pub const CODEX_SHELL_ORIGINATOR: &str = "exponential-shell";

/// The per-session codex originator: `exponential-<sid8>`, keyed by OUR
/// `coding_sessions` row id (codex's own rollout ids don't exist until it
/// boots). 8 chars is plenty — the value only has to differ between
/// concurrent same-cwd spawns on one machine.
pub fn codex_session_originator(session_id: &str) -> String {
    let sid8: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect();
    format!("exponential-{sid8}")
}

/// The MCP wiring of every CLAUDE coding argv: the launcher-written worktree
/// [`MCP_JSON_FILE`] (`.exp-mcp.json`) rides `--mcp-config` (resolved against
/// the spawn cwd = the worktree) and connects trusted, prompt-free.
///
/// The flags alone are NOT what suppresses claude's "New MCP server found in
/// this project" dialog — EXP-83 assumed they were, and the dialog kept
/// firing (EXP-98). Claude's interactive startup runs an unconditional
/// approval scan of the project-scope config (the literal `.mcp.json` in the
/// cwd) that ignores both `--mcp-config`/`--strict-mcp-config`; those
/// flags only gate which servers CONNECT. The actual fix is the file NAME:
/// `.exp-mcp.json` is invisible to that scan (see [`crate::mcp_json`]).
/// `--strict-mcp-config` still matters — it keeps any repo-carried MCP
/// config from connecting in an unattended session.
pub fn mcp_config_args() -> Vec<String> {
    vec![
        "--mcp-config".into(),
        MCP_JSON_FILE.into(),
        "--strict-mcp-config".into(),
    ]
}

/// The permission tail of every CLAUDE coding argv (EXP-690 posture — every
/// run bypasses permissions; plan mode is the only starting-mode choice
/// left):
///
/// - Plan mode wins the STARTING mode: `--permission-mode plan` +
///   `--allow-dangerously-skip-permissions` (the skip flag cannot ride NEXT
///   TO a starting mode — both select one; the allow flag instead puts
///   `bypassPermissions` in the Shift+Tab cycle, one keypress to full-auto
///   after the plan is approved).
/// - Otherwise: the classic `--dangerously-skip-permissions`
///   (≡ `--permission-mode bypassPermissions`).
pub fn permission_args(plan_mode: bool) -> Vec<String> {
    if plan_mode {
        vec![
            "--permission-mode".into(),
            "plan".into(),
            "--allow-dangerously-skip-permissions".into(),
        ]
    } else {
        vec!["--dangerously-skip-permissions".into()]
    }
}

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
/// [`session_args`]:
///
/// - Claude: the worktree `.exp-mcp.json` file (rides [`mcp_config_args`]).
/// - Codex: `-c mcp_servers.*` CLI overrides pointing at `url`, with the
///   bearer token read from [`MCP_TOKEN_ENV`] in the spawn env — the key
///   never lands on disk or argv for codex.
/// - Pi: the launcher-written [`PI_BRIDGE_FILE`] extension (rides `-e`); the
///   bridge reads [`MCP_URL_ENV`] + [`MCP_TOKEN_ENV`] from the env.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentMcp {
    ClaudeFile,
    /// EXP-746 (the ACP transport): claude gets the MCP server as INLINE
    /// `--mcp-config` JSON whose `Authorization` header reads
    /// `Bearer ${EXP_MCP_TOKEN}` — the CLI expands env refs inside header
    /// values (measured in the phase-1 spike), so the `expu_` key rides the
    /// child's env like codex/pi and never lands on disk. The engine renders
    /// the JSON; this carries what it needs.
    ClaudeInline {
        url: String,
        /// The `X-Exp-Session-Id` header value (EXP-637).
        session_id: Option<String>,
    },
    CodexOverrides {
        url: String,
        /// EXP-637: the launched `coding_sessions` row id, sent as the
        /// `X-Exp-Session-Id` HTTP header. `None` outside a launched
        /// session (agent shells) keeps the pre-EXP-637 argv byte-identical.
        session_id: Option<String>,
    },
    PiExtension,
    /// EXP-758: a user-declared EXTERNAL ACP agent. There is no config format
    /// of ours to write for a binary we did not ship, so the whole wiring is
    /// the spawn env ([`MCP_URL_ENV`] / [`MCP_TOKEN_ENV`] /
    /// [`MCP_SESSION_ID_ENV`], the shape pi's bridge already reads) and the
    /// agent connects to `/api/mcp` itself if it speaks MCP at all. Nothing
    /// of ours lands in the worktree.
    ExternalEnv {
        url: String,
        /// The `X-Exp-Session-Id` header value (EXP-637).
        session_id: Option<String>,
    },
}

/// EXP-792: ONE team MCP server resolved for a launch — the non-secret
/// config from `mcpServers.listForDevice` joined with the ENV VAR NAMES the
/// launcher minted for its device-held secrets. Every credential position is
/// a `${VAR}` REFERENCE the agent expands from the child's own environment
/// (claude header values, codex `bearer_token_env_var`/`env_http_headers`,
/// the pi bridge's `process.env`), so no generated config ever carries a
/// value. `exponential` itself is NOT one of these — it keeps its dedicated
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
    /// Header name → value, where a device-held value is the literal
    /// `${VAR}` reference (`Authorization` → `Bearer ${EXP_MCP_TOKEN_1}` for
    /// an OAuth server). `http` only.
    pub headers: Vec<(String, String)>,
    /// The env var (in the spawn env) carrying a bearer token, when the
    /// server authenticates with one — codex's `bearer_token_env_var`.
    pub token_env: Option<String>,
    /// Env NAME → `${VAR}` reference for a `stdio` server's env.
    pub env: Vec<(String, String)>,
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

/// EXP-792: the env var carrying the launch's team MCP servers for the
/// agents that have no config format of their own to render them into: the
/// pi bridge and an external ACP binary read this JSON array
/// (`[{name, kind:"http", url, headers} | {name, kind:"stdio", command,
/// args, env}]`) and resolve every `${VAR}` header/env reference from their
/// OWN environment at connect time. Never set for claude/codex (their
/// configs carry the servers) and never for an agent shell. NOT a secret:
/// every credential position is a `${VAR}` reference.
pub const MCP_SERVERS_ENV: &str = "EXP_MCP_SERVERS";

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

/// [`MCP_SERVERS_ENV`]'s value: the pi bridge / external agent wire shape,
/// `${VAR}` references verbatim (the reader expands them), headers and env
/// sorted by name so the string is stable across launches.
pub fn mcp_servers_env_json(servers: &[McpServerWire]) -> String {
    let entries: Vec<serde_json::Value> = servers
        .iter()
        .map(|server| {
            let mut entry = serde_json::Map::new();
            entry.insert("name".into(), serde_json::Value::String(server.name.clone()));
            match &server.transport {
                McpWireTransport::Http { url } => {
                    entry.insert("kind".into(), "http".into());
                    entry.insert("url".into(), serde_json::Value::String(url.clone()));
                    entry.insert("headers".into(), sorted_object(&server.headers));
                }
                McpWireTransport::Stdio { command, args } => {
                    entry.insert("kind".into(), "stdio".into());
                    entry.insert("command".into(), serde_json::Value::String(command.clone()));
                    entry.insert(
                        "args".into(),
                        serde_json::Value::Array(
                            args.iter().map(|arg| arg.as_str().into()).collect(),
                        ),
                    );
                    entry.insert("env".into(), sorted_object(&server.env));
                }
            }
            serde_json::Value::Object(entry)
        })
        .collect();
    serde_json::Value::Array(entries).to_string()
}

fn sorted_object(pairs: &[(String, String)]) -> serde_json::Value {
    let sorted: std::collections::BTreeMap<&str, &str> = pairs
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    serde_json::Value::Object(
        sorted
            .into_iter()
            .map(|(name, value)| (name.to_string(), serde_json::Value::String(value.to_string())))
            .collect(),
    )
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

/// The Start-coding dialog's choices — ONE shape for both run modes (a
/// single-issue session and a multi-issue batch session differ only in their
/// settings DEFAULTS, not in the flags they can carry).
#[derive(Clone, Debug)]
pub struct LaunchOptions {
    /// Which agent CLI to spawn (EXP-201).
    pub agent: CodingAgent,
    /// Model choice within the agent's closed set. Blank = omit the model
    /// flag (valid for codex/pi only; claude is explicit-always).
    pub model: String,
    /// Effort/reasoning/thinking level; blank = omit the flag. Ignored while
    /// ultracode is on (ultracode IS the effort level — `--effort ultracode`).
    pub effort: String,
    /// Dynamic workflows (`--effort ultracode`, CLI ≥2.1.203 —
    /// model-independent, no opus pin). Claude-only; wins over `effort`.
    pub ultracode: bool,
    /// Launch-into-plan mode: claude natively (`--permission-mode plan`),
    /// pi via the injected `.exp-pi-plan.ts` extension gated on
    /// [`PI_PLAN_MODE_ENV`] (EXP-441). Never codex.
    pub plan_mode: bool,
    /// EXP-746 (D13): run this launch on a user-declared external ACP agent
    /// instead of `agent`'s CLI. `None` = the builtin above, which is every
    /// path but a local start that picked an external pill — an external
    /// agent is never remotely startable and has no TUI argv, so it always
    /// resolves to [`crate::launcher::LaunchTransport::Acp`].
    pub external: Option<crate::settings::ExternalAgentSpec>,
    /// EXP-792: the team MCP servers (`mcp_servers` row ids) this run
    /// connects to beside `exponential`. Empty = none. The launcher resolves
    /// them against the device's secret store and REFUSES the launch with a
    /// named blocker when one has no credential here.
    pub mcp_server_ids: Vec<String>,
    /// EXP-792 (EXP-747 B7): the agent ACCOUNT PROFILE to run on — `None` or
    /// `system` = the ambient login; else a device-local profile id under
    /// `{data_dir}/agents/<agent>/<id>/` (`CLAUDE_CONFIG_DIR` / `CODEX_HOME`).
    pub account: Option<String>,
}

impl LaunchOptions {
    /// The settings-default options (the local Start-coding dialog's seed
    /// values). EXP-206: ONE set of defaults — a single-issue run and a
    /// multi-issue batch run seed identically; the per-run-mode settings
    /// pairs are gone.
    pub fn defaults(settings: &Settings) -> Self {
        Self::defaults_for(settings, settings.default_agent)
    }

    /// The settings-default options for an EXPLICIT agent pick (EXP-325 —
    /// the terminal dock's "+" menu launches whichever installed agent was
    /// clicked, with that agent's persisted model/effort/toggles and the
    /// usual capability masking).
    pub fn defaults_for(settings: &Settings, agent: CodingAgent) -> Self {
        Self {
            agent,
            model: settings.model_for(agent).to_string(),
            effort: settings.effort_for(agent).to_string(),
            ultracode: settings.claude_ultracode && agent.supports_ultracode(),
            plan_mode: settings.plan_mode_for(agent) && agent.supports_plan_mode(),
            // EXP-746: the settings defaults always name a BUILTIN agent —
            // an external one is only ever an explicit local pick.
            mcp_server_ids: Vec::new(),
            account: None,
            external: None,
        }
    }

    /// The shared RELAY-start normalization (EXP-149/EXP-201) — one form for
    /// issue and batch starts alike (EXP-206): the remote client's
    /// Start-coding choices normalized against the AGENT's closed sets, over
    /// settings defaults for anything it didn't send.
    ///
    /// - Absent/unknown `agent` → **Claude** (an option-less legacy frame
    ///   must behave exactly as before EXP-201 — never the local default
    ///   agent, or an old phone's claude vocabulary could land on a codex
    ///   launch).
    /// - `effort: Some("")` is an explicit "CLI default" and beats a
    ///   non-blank settings effort; same for a blank codex/pi model.
    /// - An absent ultracode falls to the settings default; plan mode
    ///   defaults OFF when absent (F7 — an option-less start must never park
    ///   an unattended desktop at the plan-approval TUI); a remote client
    ///   sending `plan_mode: true` opted in knowingly.
    /// - Capabilities mask everything: a non-claude agent can never carry
    ///   ultracode, codex never carries plan.
    pub fn remote(
        settings: &Settings,
        agent: Option<&str>,
        model: Option<&str>,
        effort: Option<&str>,
        ultracode: Option<bool>,
        plan_mode: Option<bool>,
    ) -> Self {
        use crate::settings::normalize_choice;
        let agent = agent
            .and_then(CodingAgent::parse)
            .unwrap_or(CodingAgent::Claude);
        let model_fallback = if agent == settings.default_agent {
            settings.model_for(agent)
        } else if agent.allows_blank_model() {
            ""
        } else {
            crate::settings::DEFAULT_CLAUDE_MODEL
        };
        let model = match model {
            Some(model) => {
                let normalized = normalize_choice(model, agent.model_values(), "");
                if normalized.is_empty() && !agent.allows_blank_model() {
                    model_fallback.to_string()
                } else {
                    normalized
                }
            }
            None => model_fallback.to_string(),
        };
        let effort = match effort {
            Some(effort) => normalize_choice(effort, agent.effort_values(), ""),
            None if agent == settings.default_agent => settings.effort_for(agent).to_string(),
            None => String::new(),
        };
        Self {
            agent,
            model,
            effort,
            ultracode: ultracode.unwrap_or(settings.claude_ultracode)
                && agent.supports_ultracode(),
            plan_mode: plan_mode.unwrap_or(false) && agent.supports_plan_mode(),
            // EXP-746 (D13): external agents are LOCAL-only — a relay start
            // can never name one.
            external: None,
            mcp_server_ids: Vec::new(),
            account: None,
        }
    }

    /// EXP-792: the remote frame's MCP server picks. Deduplicated, blanks
    /// dropped, capped at 16 (the web server's own bound).
    pub fn with_mcp_servers(mut self, ids: Option<Vec<String>>) -> Self {
        let mut out: Vec<String> = Vec::new();
        for id in ids.unwrap_or_default() {
            let id = id.trim().to_string();
            if !id.is_empty() && !out.contains(&id) && out.len() < 16 {
                out.push(id);
            }
        }
        self.mcp_server_ids = out;
        self
    }

    /// EXP-792 (EXP-747 B7): the remote frame's account profile pick. Blank
    /// and `system` both mean the ambient login (`None`).
    pub fn with_account(mut self, account: Option<&str>) -> Self {
        self.account = account
            .map(str::trim)
            .filter(|a| !a.is_empty() && *a != "system")
            .map(str::to_string);
        self
    }
}

/// EXP-773: the argv of an EXP-325 agent SHELL — the terminal dock's "+"
/// menu launch. The one interactive TUI spawn left in this crate: coding
/// runs are the ACP engine, whose adapters compose their own argv.
///
/// - claude: `--model <m> [--effort ultracode|<e>] <mcp_config_args>
///   <permission_args> --append-system-prompt <playbook>`
/// - codex: `-c check_for_update_on_startup=false [-m <m>]
///   [-c model_reasoning_effort=<e>] <mcp -c overrides>
///   -c developer_instructions=<playbook> --dangerously-bypass-…`
/// - pi: `[--model <m>] [--thinking <t>] -e ./<bridge> -e ./<plan>
///   --append-system-prompt <playbook>`
///
/// No prompt, no session pin, no resume: a shell spawns fresh and waits for
/// the user to type.
pub fn shell_args(opts: &LaunchOptions, mcp: &AgentMcp) -> Vec<String> {
    let trimmed_model = opts.model.trim();
    let trimmed_effort = opts.effort.trim();
    let mut args: Vec<String> = Vec::new();
    match opts.agent {
        CodingAgent::Claude => {
            args.push("--model".into());
            args.push(if trimmed_model.is_empty() {
                // Claude is explicit-always; a blank here is a caller bug —
                // degrade to the spec default rather than the user's CLI one.
                crate::settings::DEFAULT_CLAUDE_MODEL.to_string()
            } else {
                trimmed_model.to_string()
            });
            let effort = if opts.ultracode {
                Some("ultracode".to_string())
            } else {
                (!trimmed_effort.is_empty()).then(|| trimmed_effort.to_string())
            };
            if let Some(effort) = effort {
                args.push("--effort".into());
                args.push(effort);
            }
            args.extend(mcp_config_args());
            args.extend(permission_args(opts.plan_mode));
            // EXP-763: the run playbook, appended to the system prompt.
            args.push("--append-system-prompt".into());
            args.push(RUN_SKILL.into());
        }
        CodingAgent::Codex => {
            // EXP-389: codex's startup update prompt ("Update now / Skip …
            // Press enter to continue") blocks the shell exactly like the
            // trust screen. Session-scoped override, the user's own
            // config/interactive runs keep their update checks.
            args.push("-c".into());
            args.push("check_for_update_on_startup=false".into());
            if !trimmed_model.is_empty() {
                args.push("-m".into());
                args.push(trimmed_model.to_string());
            }
            if !trimmed_effort.is_empty() {
                args.push("-c".into());
                args.push(format!("model_reasoning_effort=\"{trimmed_effort}\""));
            }
            if let AgentMcp::CodexOverrides { url, session_id } = mcp {
                // Streamable-HTTP MCP via a -c override (codex has no
                // --mcp-config flag); the token rides MCP_TOKEN_ENV in the
                // spawn env — never argv, never disk. EXP-792: the WHOLE
                // `mcp_servers` table as ONE inline TOML value (url +
                // bearer_token_env_var + the EXP-637 session header, not a
                // secret). A shell connects to `exponential` alone: team
                // servers are a coding run's, never an interactive TUI's.
                // The rmcp toggle is defensive for older builds where HTTP
                // MCP was feature-gated (harmless on current ones).
                args.push("-c".into());
                args.push(format!(
                    "mcp_servers={}",
                    codex_mcp_servers_toml(url, session_id.as_deref(), &[])
                ));
                args.push("-c".into());
                args.push("experimental_use_rmcp_client=true".into());
            }
            // EXP-763: the run playbook as codex's own developer message
            // (additive to AGENTS.md; `developer_instructions` is a plain
            // config string, so it rides `-c` like the MCP block). A TOML
            // basic string on ONE line — the `-c key=value` parser must
            // never see a raw newline.
            args.push("-c".into());
            args.push(format!(
                "developer_instructions={}",
                toml_basic_string(RUN_SKILL)
            ));
            // EXP-690: every codex run bypasses approvals and the sandbox.
            args.push("--dangerously-bypass-approvals-and-sandbox".into());
        }
        CodingAgent::Pi => {
            if !trimmed_model.is_empty() {
                args.push("--model".into());
                args.push(trimmed_model.to_string());
            }
            if !trimmed_effort.is_empty() {
                args.push("--thinking".into());
                args.push(trimmed_effort.to_string());
            }
            // The MCP bridge extension (pi has no native MCP). `-e` loads it
            // independent of pi's project-trust prompt; never pass
            // -a/--approve (it would auto-trust repo-carried extensions).
            args.push("-e".into());
            args.push(format!("./{PI_BRIDGE_FILE}"));
            // The plan-mode extension (EXP-441): inert without
            // [`PI_PLAN_MODE_ENV`], so it rides unconditionally.
            args.push("-e".into());
            args.push(format!("./{PI_PLAN_FILE}"));
            // EXP-763: the run playbook. pi appends the argument's TEXT and
            // rebuilds the system prompt on every launch.
            args.push("--append-system-prompt".into());
            args.push(RUN_SKILL.into());
        }
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-763: the codex playbook override is one TOML basic string — what
    /// `-c developer_instructions=<value>` hands the TOML parser must decode
    /// back to the exact playbook, newlines and quotes included.
    #[test]
    fn toml_basic_string_round_trips_the_playbook() {
        let encoded = toml_basic_string(RUN_SKILL);
        assert!(encoded.starts_with('"') && encoded.ends_with('"'));
        assert!(!encoded[1..encoded.len() - 1].contains('\n'));
        let doc: toml::Value = format!("v = {encoded}").parse().unwrap();
        assert_eq!(doc["v"].as_str(), Some(RUN_SKILL));
    }

    #[test]
    fn toml_basic_string_escapes_quotes_backslashes_and_controls() {
        let encoded = toml_basic_string("say \"hi\"\\ tab\there\r\n\u{1}end");
        assert_eq!(encoded, r#""say \"hi\"\\ tab\there\r\n\u0001end""#);
        let doc: toml::Value = format!("v = {encoded}").parse().unwrap();
        assert_eq!(doc["v"].as_str(), Some("say \"hi\"\\ tab\there\r\n\u{1}end"));
    }

    fn claude_opts() -> LaunchOptions {
        LaunchOptions {
            agent: CodingAgent::Claude,
            model: "fable".to_string(),
            effort: "".to_string(),
            ultracode: false,
            plan_mode: false,
            mcp_server_ids: Vec::new(),
            account: None,
            external: None,
        }
    }

    /// EXP-792: the two-server pick every renderer test uses — one OAuth
    /// http server (bearer via `${EXP_MCP_TOKEN_1}`) and one stdio server
    /// with a typed env value (`GITHUB_TOKEN` → `${GITHUB_TOKEN}`).
    fn two_servers() -> Vec<McpServerWire> {
        vec![
            McpServerWire {
                id: "srv-1".to_string(),
                name: "linear".to_string(),
                transport: McpWireTransport::Http {
                    url: "https://mcp.linear.app/mcp".to_string(),
                },
                headers: vec![
                    ("Authorization".to_string(), "Bearer ${EXP_MCP_TOKEN_1}".to_string()),
                    ("X-Api-Key".to_string(), "${EXP_MCP_ENV_1_X_API_KEY}".to_string()),
                    ("X-Client".to_string(), "exponential".to_string()),
                ],
                token_env: Some("EXP_MCP_TOKEN_1".to_string()),
                env: Vec::new(),
            },
            McpServerWire {
                id: "srv-2".to_string(),
                name: "github".to_string(),
                transport: McpWireTransport::Stdio {
                    command: "npx".to_string(),
                    args: vec!["-y".to_string(), "@acme/github-mcp".to_string()],
                },
                headers: Vec::new(),
                token_env: None,
                env: vec![("GITHUB_TOKEN".to_string(), "${GITHUB_TOKEN}".to_string())],
            },
        ]
    }

    #[test]
    fn env_references_are_exact_or_bearer_prefixed() {
        assert_eq!(env_reference("${EXP_MCP_ENV_1_X_API_KEY}"), Some("EXP_MCP_ENV_1_X_API_KEY"));
        assert_eq!(env_reference("Bearer ${EXP_MCP_TOKEN_1}"), None);
        assert_eq!(env_reference("${}"), None);
        assert_eq!(env_reference("${a-b}"), None);
        assert_eq!(env_reference("literal"), None);
        assert_eq!(bearer_env_reference("Bearer ${EXP_MCP_TOKEN_1}"), Some("EXP_MCP_TOKEN_1"));
        assert_eq!(bearer_env_reference("Bearer expu_literal"), None);
        assert_eq!(bearer_env_reference("${EXP_MCP_TOKEN_1}"), None);
    }

    /// EXP-792: the codex table is one TOML inline table — parseable, with
    /// `exponential` first and each server's references turned into codex's
    /// env-NAMED fields (it expands no `${…}` of its own).
    #[test]
    fn codex_mcp_table_carries_every_server_as_env_named_fields() {
        let bare = codex_mcp_servers_toml("http://x/api/mcp", None, &[]);
        assert_eq!(
            bare,
            "{exponential={url=\"http://x/api/mcp\",bearer_token_env_var=\"EXP_MCP_TOKEN\"}}"
        );
        let with_session = codex_mcp_servers_toml("http://x/api/mcp", Some("sess-1"), &[]);
        assert_eq!(
            with_session,
            "{exponential={url=\"http://x/api/mcp\",bearer_token_env_var=\"EXP_MCP_TOKEN\",http_headers={X-Exp-Session-Id=\"sess-1\"}}}"
        );

        let full = codex_mcp_servers_toml("http://x/api/mcp", Some("sess-1"), &two_servers());
        let doc: toml::Value = format!("mcp_servers={full}").parse().expect("valid TOML");
        let table = doc["mcp_servers"].as_table().unwrap();
        assert_eq!(table.len(), 3);
        // `exponential` first, then pick order — on the RENDERED text (the
        // parsed table sorts its keys).
        let position = |key: &str| full.find(&format!("{key}={{")).expect(key);
        assert!(position("exponential") < position("linear"));
        assert!(position("linear") < position("github"));
        let linear = &table["linear"];
        assert_eq!(linear["url"].as_str(), Some("https://mcp.linear.app/mcp"));
        assert_eq!(linear["bearer_token_env_var"].as_str(), Some("EXP_MCP_TOKEN_1"));
        assert_eq!(linear["env_http_headers"]["X-Api-Key"].as_str(), Some("EXP_MCP_ENV_1_X_API_KEY"));
        assert_eq!(linear["http_headers"]["X-Client"].as_str(), Some("exponential"));
        // The bearer header is the bearer FIELD, never a literal `${…}` copy.
        assert!(linear["http_headers"].get("Authorization").is_none());
        let github = &table["github"];
        assert_eq!(github["command"].as_str(), Some("npx"));
        assert_eq!(
            github["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect::<Vec<_>>(),
            vec!["-y", "@acme/github-mcp"]
        );
        assert_eq!(
            github["env_vars"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect::<Vec<_>>(),
            vec!["GITHUB_TOKEN"]
        );
        // No value ever lands in the table: only names and references.
        assert!(!full.contains("expu_"));
        assert!(!full.contains("${"), "codex gets env NAMES, never a `${{…}}` it cannot expand");
    }

    /// EXP-792: the pi bridge / external agent wire keeps every `${VAR}`
    /// verbatim (they resolve it from their own env) and is byte-stable.
    #[test]
    fn mcp_servers_env_json_is_the_bridge_wire_shape() {
        assert_eq!(mcp_servers_env_json(&[]), "[]");
        let rendered = mcp_servers_env_json(&two_servers());
        let parsed: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(parsed[0]["name"], "linear");
        assert_eq!(parsed[0]["kind"], "http");
        assert_eq!(parsed[0]["url"], "https://mcp.linear.app/mcp");
        assert_eq!(parsed[0]["headers"]["Authorization"], "Bearer ${EXP_MCP_TOKEN_1}");
        assert_eq!(parsed[0]["headers"]["X-Api-Key"], "${EXP_MCP_ENV_1_X_API_KEY}");
        assert_eq!(parsed[1]["name"], "github");
        assert_eq!(parsed[1]["kind"], "stdio");
        assert_eq!(parsed[1]["command"], "npx");
        assert_eq!(parsed[1]["args"], serde_json::json!(["-y", "@acme/github-mcp"]));
        assert_eq!(parsed[1]["env"]["GITHUB_TOKEN"], "${GITHUB_TOKEN}");
        // The row ids stay off the wire: the agent needs names, not rows.
        assert!(!rendered.contains("srv-1"));
        assert_eq!(rendered, mcp_servers_env_json(&two_servers()));
    }

    /// EXP-773: the agent SHELL argv — the one interactive TUI spawn left.
    /// No prompt positional, no session pin, no resume tail; the MCP wiring
    /// and the run playbook still ride it on every agent.
    #[test]
    fn shell_args_per_agent() {
        let claude = shell_args(&claude_opts(), &AgentMcp::ClaudeFile);
        assert_eq!(
            claude,
            vec![
                "--model".to_string(),
                "fable".to_string(),
                "--mcp-config".to_string(),
                ".exp-mcp.json".to_string(),
                "--strict-mcp-config".to_string(),
                "--dangerously-skip-permissions".to_string(),
                "--append-system-prompt".to_string(),
                RUN_SKILL.to_string(),
            ]
        );

        let mut codex = claude_opts();
        codex.agent = CodingAgent::Codex;
        codex.model = "gpt-5.6-sol".to_string();
        codex.effort = "high".to_string();
        let args = shell_args(
            &codex,
            &AgentMcp::CodexOverrides {
                url: "http://x/api/mcp".to_string(),
                session_id: None,
            },
        );
        assert_eq!(args[..2], ["-c", "check_for_update_on_startup=false"]);
        // EXP-792: ONE inline-table flag carries the whole `mcp_servers`
        // table — url + bearer_token_env_var, no dotted overrides left.
        let table = args
            .iter()
            .find(|arg| arg.starts_with("mcp_servers="))
            .expect("the codex shell carries the mcp_servers table");
        assert_eq!(
            table,
            "mcp_servers={exponential={url=\"http://x/api/mcp\",bearer_token_env_var=\"EXP_MCP_TOKEN\"}}"
        );
        assert!(!args.iter().any(|arg| arg.starts_with("mcp_servers.exponential")));
        assert!(args.contains(&"experimental_use_rmcp_client=true".to_string()));
        assert!(args.contains(&"model_reasoning_effort=\"high\"".to_string()));
        assert!(args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
        assert!(!args.iter().any(|arg| arg.contains("expu_")));

        let mut pi = claude_opts();
        pi.agent = CodingAgent::Pi;
        pi.model = "grok-4.5".to_string();
        let args = shell_args(&pi, &AgentMcp::PiExtension);
        assert_eq!(args[..2], ["--model", "grok-4.5"]);
        assert!(args.windows(2).any(|w| w == ["-e", "./.exp-pi-mcp.ts"]));
        assert!(args.windows(2).any(|w| w == ["-e", "./.exp-pi-plan.ts"]));
        // A shell never resumes and never pins a session file.
        for args in [&claude, &args] {
            assert!(!args.iter().any(|arg| arg == "--session"
                || arg == "--session-id"
                || arg == "--resume"
                || arg == "resume"));
        }
    }

    #[test]
    fn permission_args_split_on_plan() {
        // Gated: plan START mode + bypass ALLOWED (Shift+Tab reachable) but
        // never `--dangerously-skip-permissions` itself — that flag IS a
        // starting mode and would erase the gate.
        assert_eq!(
            permission_args(true),
            vec![
                "--permission-mode".to_string(),
                "plan".to_string(),
                "--allow-dangerously-skip-permissions".to_string(),
            ]
        );
        // EXP-690: everything else bypasses permissions outright.
        assert_eq!(
            permission_args(false),
            vec!["--dangerously-skip-permissions".to_string()]
        );
    }

    #[test]
    fn mcp_config_args_pass_the_worktree_file_explicitly_and_strictly() {
        // Explicit --mcp-config on the non-discoverable name (EXP-98 — the
        // dialog scan only sees `.mcp.json`) + strict mode (repo-carried MCP
        // config never connects).
        assert_eq!(
            mcp_config_args(),
            vec![
                "--mcp-config".to_string(),
                ".exp-mcp.json".to_string(),
                "--strict-mcp-config".to_string(),
            ]
        );
    }

    /// EXP-325: an explicit agent pick seeds THAT agent's persisted pair and
    /// masks capabilities — regardless of the default agent.
    #[test]
    fn defaults_for_follow_the_picked_agent() {
        let mut settings = Settings::default();
        settings.default_agent = CodingAgent::Claude;
        settings.claude_ultracode = true; // claude-only — must mask on codex
        settings.claude_plan_mode = true;
        settings.codex_model = "gpt-5.6-terra".to_string();
        settings.codex_effort = "xhigh".to_string();

        let opts = LaunchOptions::defaults_for(&settings, CodingAgent::Codex);
        assert_eq!(opts.agent, CodingAgent::Codex);
        assert_eq!(opts.model, "gpt-5.6-terra");
        assert_eq!(opts.effort, "xhigh");
        assert!(!opts.ultracode);
        assert!(!opts.plan_mode);

        // `defaults` stays the default-agent shorthand.
        let via_default = LaunchOptions::defaults(&settings);
        let via_for = LaunchOptions::defaults_for(&settings, settings.default_agent);
        assert_eq!(via_default.agent, via_for.agent);
        assert_eq!(via_default.model, via_for.model);
        assert_eq!(via_default.effort, via_for.effort);

        let opts = LaunchOptions::defaults_for(&settings, CodingAgent::Pi);
        assert_eq!(opts.agent, CodingAgent::Pi);
        assert!(opts.plan_mode, "pi seeds its OWN plan default (EXP-441)");

        // The pi plan default is its own field — independent of claude's.
        settings.pi_plan_mode = false;
        assert!(!LaunchOptions::defaults_for(&settings, CodingAgent::Pi).plan_mode);
        assert!(LaunchOptions::defaults_for(&settings, CodingAgent::Claude).plan_mode);
    }

    #[test]
    fn defaults_map_model_effort_and_toggles() {
        let mut settings = Settings::default();
        settings.claude_model = "sonnet".to_string();
        settings.claude_effort = "high".to_string();
        settings.claude_plan_mode = false;
        let opts = LaunchOptions::defaults(&settings);
        assert_eq!(opts.agent, CodingAgent::Claude);
        assert_eq!(opts.model, "sonnet");
        assert_eq!(opts.effort, "high");
        assert!(!opts.plan_mode);
        assert!(!opts.ultracode);

        // The stock defaults (EXP-206 — ONE set, no issue/batch split):
        // plan mode ON, ultracode OFF.
        let opts = LaunchOptions::defaults(&Settings::default());
        assert_eq!(opts.model, "fable");
        assert_eq!(opts.effort, "");
        assert!(opts.plan_mode);
        assert!(!opts.ultracode);
    }

    /// EXP-201: a non-claude default agent seeds ITS model/effort pair and
    /// masks the claude-only toggles even when their settings are on.
    #[test]
    fn defaults_follow_the_default_agent_and_mask_capabilities() {
        let mut settings = Settings::default();
        settings.default_agent = CodingAgent::Codex;
        settings.codex_model = "gpt-5.6-terra".to_string();
        settings.codex_effort = "xhigh".to_string();
        settings.claude_ultracode = true; // claude-only — must mask
        settings.claude_plan_mode = true; // claude-only — must mask
        let opts = LaunchOptions::defaults(&settings);
        assert_eq!(opts.agent, CodingAgent::Codex);
        assert_eq!(opts.model, "gpt-5.6-terra");
        assert_eq!(opts.effort, "xhigh");
        assert!(!opts.ultracode);
        assert!(!opts.plan_mode);

        settings.default_agent = CodingAgent::Pi;
        settings.pi_model = "grok-4.5".to_string();
        settings.pi_thinking = "max".to_string();
        let opts = LaunchOptions::defaults(&settings);
        assert_eq!(opts.agent, CodingAgent::Pi);
        assert_eq!(opts.model, "grok-4.5");
        assert_eq!(opts.effort, "max");
    }

    #[test]
    fn remote_all_absent_matches_pre_options_relay_behavior() {
        // The F7 baseline: settings model/effort/ultracode, plan mode OFF —
        // exactly what an option-less relay start ran before EXP-149.
        let mut settings = Settings::default();
        settings.claude_model = "opus".to_string();
        settings.claude_effort = "high".to_string();
        settings.claude_ultracode = true;
        settings.claude_plan_mode = true; // must NOT leak into a remote start
        let opts = LaunchOptions::remote(&settings, None, None, None, None, None);
        assert_eq!(opts.agent, CodingAgent::Claude);
        assert_eq!(opts.model, "opus");
        assert_eq!(opts.effort, "high");
        assert!(opts.ultracode);
        assert!(!opts.plan_mode);
    }

    #[test]
    fn remote_applies_and_normalizes_sent_options() {
        let mut settings = Settings::default();
        settings.claude_effort = "high".to_string();

        let opts = LaunchOptions::remote(
            &settings,
            Some("Claude"),
            Some("Sonnet"),
            Some("max"),
            Some(false),
            Some(true),
        );
        assert_eq!(opts.agent, CodingAgent::Claude);
        assert_eq!(opts.model, "sonnet", "case-normalized");
        assert_eq!(opts.effort, "max");
        assert!(!opts.ultracode);
        assert!(opts.plan_mode, "explicit remote opt-in");

        // Bogus model falls back to the settings model, never to a crash or
        // a raw pass-through to the CLI argv.
        let opts = LaunchOptions::remote(&settings, None, Some("gpt-6"), None, None, None);
        assert_eq!(opts.model, "fable");

        // Explicit blank effort = "CLI default" and beats the settings value.
        let opts = LaunchOptions::remote(&settings, None, None, Some(""), None, None);
        assert_eq!(opts.effort, "");
        // Bogus effort also degrades to blank (omit --effort).
        let opts = LaunchOptions::remote(&settings, None, None, Some("extreme"), None, None);
        assert_eq!(opts.effort, "");
    }

    /// EXP-201: a remote CODEX start normalizes against codex sets and can
    /// never carry the claude-only toggles; an unknown agent degrades to
    /// claude (legacy behavior, never the local default agent).
    #[test]
    fn remote_normalizes_per_agent_and_masks_capabilities() {
        let mut settings = Settings::default();
        settings.claude_ultracode = true; // must not leak onto codex

        let opts = LaunchOptions::remote(
            &settings,
            Some("codex"),
            Some("gpt-5.6-luna"),
            Some("minimal"),
            Some(true), // ultracode — claude-only, must mask
            Some(true), // plan — claude/pi-only, must mask on codex
        );
        assert_eq!(opts.agent, CodingAgent::Codex);
        assert_eq!(opts.model, "gpt-5.6-luna");
        assert_eq!(opts.effort, "minimal");
        assert!(!opts.ultracode);
        assert!(!opts.plan_mode);

        // A claude model on a codex start is bogus → blank (codex default).
        let opts = LaunchOptions::remote(
            &settings,
            Some("codex"),
            Some("fable"),
            None,
            None,
            None,
        );
        assert_eq!(opts.model, "");

        // pi: thinking set; an explicit plan opt-in passes through
        // (EXP-441 — pi plans via the injected extension).
        let opts = LaunchOptions::remote(
            &settings,
            Some("pi"),
            Some("grok-4.5"),
            Some("xhigh"),
            None,
            Some(true),
        );
        assert_eq!(opts.agent, CodingAgent::Pi);
        assert_eq!(opts.model, "grok-4.5");
        assert_eq!(opts.effort, "xhigh");
        assert!(opts.plan_mode, "explicit remote opt-in (EXP-441)");

        // F7 holds for pi too: an option-less start must never park an
        // unattended desktop at the plan gate.
        let opts = LaunchOptions::remote(&settings, Some("pi"), None, None, None, None);
        assert!(!opts.plan_mode, "absent plan defaults OFF");

        // Unknown agent string → claude with claude normalization.
        let opts = LaunchOptions::remote(
            &settings,
            Some("cursor"),
            Some("sonnet"),
            None,
            None,
            None,
        );
        assert_eq!(opts.agent, CodingAgent::Claude);
        assert_eq!(opts.model, "sonnet");
    }

    /// A non-default remote agent with NO model/effort sent uses ITS blank
    /// CLI defaults — never the default agent's persisted pair.
    #[test]
    fn remote_non_default_agent_falls_to_blank_not_foreign_settings() {
        let mut settings = Settings::default();
        settings.claude_model = "opus".to_string();
        settings.claude_effort = "high".to_string();
        let opts = LaunchOptions::remote(&settings, Some("codex"), None, None, None, None);
        assert_eq!(opts.agent, CodingAgent::Codex);
        assert_eq!(opts.model, "", "claude's opus must not leak onto codex");
        assert_eq!(opts.effort, "");

        // And when codex IS the default agent, its persisted pair applies.
        settings.default_agent = CodingAgent::Codex;
        settings.codex_model = "gpt-5.6-sol".to_string();
        settings.codex_effort = "high".to_string();
        let opts = LaunchOptions::remote(&settings, Some("codex"), None, None, None, None);
        assert_eq!(opts.model, "gpt-5.6-sol");
        assert_eq!(opts.effort, "high");
    }

    /// EXP-443: the per-session codex originator is stable, filesystem-safe
    /// and distinct from the agent-shell one.
    #[test]
    fn codex_session_originator_takes_eight_alphanumerics() {
        assert_eq!(
            codex_session_originator("0d9f7f6e-8e1c-4b62-9a6e-2f1c9b3d4e5f"),
            "exponential-0d9f7f6e"
        );
        assert_eq!(codex_session_originator("ab"), "exponential-ab");
        assert_ne!(codex_session_originator("deadbeef"), CODEX_SHELL_ORIGINATOR);
    }
}
