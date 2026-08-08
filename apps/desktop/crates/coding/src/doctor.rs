//! The tooling doctor (masterplan-v3 §7.7, EXP-201): runs `--version` on
//! every agent CLI (`claude`, `codex`, `pi` — each at its configured/probed
//! path) and on `git`, capturing success + version string or the spawn error.
//!
//! Gating is per-agent (EXP-201): **git is required for every launch**, but a
//! missing optional agent only blocks launches that SELECT it —
//! [`DoctorReport::first_failure_for`] is the launcher's step-0 gate, and a
//! machine without codex installed still codes with claude. The
//! Start-coding affordance itself only needs git + at least one usable agent
//! ([`DoctorReport::any_agent_ok`]); the dialog names the selected agent's
//! failure. Errors stay actionable per the spec copy: "claude not found on
//! PATH — set an absolute path" / "git not found on PATH".
//!
//! A resolvable Claude that is OLDER than [`MIN_CLAUDE_VERSION`] also fails
//! its check (with "run: claude update" copy) — one version gate replaces
//! the old per-flag `--help` probe and its whole degradation matrix. Codex
//! and pi have NO minimum version yet (presence-only, deliberately lenient).
//!
//! EXP-409: an installed agent that is SIGNED OUT fails its check too —
//! installed-but-not-signed-in equals not installed for every gate, because
//! a logged-out agent spawns onto its login prompt and a headless/remote
//! session hangs there invisibly. Probes: `claude auth status` (local JSON,
//! `{"loggedIn": bool}`), `codex login status` (exit 0 / "Not logged in"),
//! and pi credential presence (`~/.pi/agent/auth.json` or a provider API-key
//! env var — pi has no login command). Every probe FAILS OPEN: an
//! unrecognisable answer (older CLI, changed output) leaves the check green
//! rather than falsely bricking a working install.
//!
//! [`DoctorReport::installed_agents`] is the steer presence input (EXP-201):
//! the device advertises which agent CLIs it can actually run, so remote
//! Start-coding pickers only offer those.
//!
//! Every probe runs with the terminal layer's augmented login `PATH`
//! ([`terminal::pty::login_path`], §6.12) — the SAME environment the PTY
//! spawns the agent into. A `.app`/`.desktop` launch carries a minimal PATH
//! without Homebrew/npm-global, so probing with the process PATH reported
//! codex/pi as "not found" on machines where every launch worked (EXP-206).
//!
//! Blocking `std::process` calls — callers run this off the foreground
//! executor (settings "Check tools" button, onboarding, launch step 0).

use crate::agent::CodingAgent;
use crate::settings::Settings;
use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;
use terminal::process::background_command;

/// EXP-414: the deadline for every probe shell-out. The CLI daemon re-runs
/// the doctor inline on a 5-minute cadence — an agent CLI that wedges (a
/// hung update check, a dead network filesystem) would otherwise stall that
/// loop, and with it the device's presence, indefinitely.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// The minimum supported Claude Code version: `--permission-mode auto`
/// (EXP-201's default posture) is verified on 2.1.215; `--effort ultracode`
/// landed in 2.1.203, `--permission-mode plan`/`manual` in 2.1.200 —
/// everything the launcher's claude argv relies on.
pub const MIN_CLAUDE_VERSION: (u32, u32, u32) = (2, 1, 215);

/// The local binaries the launcher ever shells out to (§7.1 step 3:
/// argv `git` + the agent CLIs, never `gh`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Claude,
    Codex,
    Pi,
    Git,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::Codex => "codex",
            Tool::Pi => "pi",
            Tool::Git => "git",
        }
    }

    /// The agent this tool check backs (`None` for git).
    pub fn agent(self) -> Option<CodingAgent> {
        match self {
            Tool::Claude => Some(CodingAgent::Claude),
            Tool::Codex => Some(CodingAgent::Codex),
            Tool::Pi => Some(CodingAgent::Pi),
            Tool::Git => None,
        }
    }

    fn for_agent(agent: CodingAgent) -> Tool {
        match agent {
            CodingAgent::Claude => Tool::Claude,
            CodingAgent::Codex => Tool::Codex,
            CodingAgent::Pi => Tool::Pi,
        }
    }

    /// The §7.7 red actionable message for a missing binary.
    fn not_found_message(self) -> &'static str {
        match self {
            Tool::Claude => "claude not found on PATH. Set an absolute path.",
            Tool::Codex => "codex not found on PATH. Set an absolute path.",
            Tool::Pi => "pi not found on PATH. Set an absolute path.",
            Tool::Git => "git not found on PATH",
        }
    }

    /// The EXP-409 red actionable message for an installed-but-signed-out
    /// agent (never produced for git).
    fn signed_out_message(self) -> &'static str {
        match self {
            Tool::Claude => "claude is installed but not signed in. Run `claude` in a terminal and log in.",
            Tool::Codex => "codex is installed but not signed in. Run `codex login` in a terminal.",
            Tool::Pi => "pi has no provider credentials. Run `pi` and sign in with /login, or set a provider API key.",
            Tool::Git => "",
        }
    }
}

impl fmt::Display for Tool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// One doctor row (§7.7): green check + version, or a red actionable error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCheck {
    pub tool: Tool,
    pub ok: bool,
    pub version: Option<String>,
    pub error: Option<String>,
    /// EXP-409 sign-in state: `Some(false)` = installed but signed out (the
    /// check is then also `!ok`, with [`Tool::signed_out_message`] as the
    /// error but `version` kept — UIs distinguish "sign in" from "install").
    /// `None` = not applicable (git) or unknown (probe failed open).
    pub authed: Option<bool>,
}

impl ToolCheck {
    /// Whether this row is the installed-but-signed-out state — the one
    /// failure whose fix is a login, not an install.
    pub fn signed_out(&self) -> bool {
        self.authed == Some(false)
    }
}

/// `{ claude, codex, pi, git }` — the §7.7 report, one row per agent CLI plus
/// git (EXP-201; the old two-row `agent`/`git` shape is gone).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoctorReport {
    pub claude: ToolCheck,
    pub codex: ToolCheck,
    pub pi: ToolCheck,
    pub git: ToolCheck,
}

impl DoctorReport {
    /// The check backing `agent`.
    pub fn check_for(&self, agent: CodingAgent) -> &ToolCheck {
        match agent {
            CodingAgent::Claude => &self.claude,
            CodingAgent::Codex => &self.codex,
            CodingAgent::Pi => &self.pi,
        }
    }

    /// The launcher's step-0 gate for a launch selecting `agent`: git AND
    /// that agent must resolve — a missing OTHER agent never blocks.
    pub fn first_failure_for(&self, agent: CodingAgent) -> Option<&ToolCheck> {
        [self.check_for(agent), &self.git]
            .into_iter()
            .find(|check| !check.ok)
    }

    /// Whether ANY agent CLI is usable (the Start-coding affordance's gate
    /// half — the dialog names the selected agent's failure itself).
    pub fn any_agent_ok(&self) -> bool {
        CodingAgent::ALL
            .into_iter()
            .any(|agent| self.check_for(agent).ok)
    }

    /// The agents this machine can actually launch — the steer presence
    /// advertisement (EXP-201). A too-old claude is NOT usable (its argv
    /// would carry flags the CLI rejects), so it drops out here too — as
    /// does a signed-out agent (EXP-409).
    pub fn installed_agents(&self) -> Vec<CodingAgent> {
        CodingAgent::ALL
            .into_iter()
            .filter(|agent| self.check_for(*agent).ok)
            .collect()
    }

    /// The agents that are installed but SIGNED OUT (EXP-409) — advertised
    /// separately so remote UIs can say "sign in on that machine" instead of
    /// pretending the binary is missing.
    pub fn unauthed_agents(&self) -> Vec<CodingAgent> {
        CodingAgent::ALL
            .into_iter()
            .filter(|agent| self.check_for(*agent).signed_out())
            .collect()
    }

    /// The steer presence advertisement as wire ids — the ONE shape both
    /// producers (desktop control channel + CLI daemon) send and compare for
    /// re-advertise change detection, so an uninstall of a signed-out agent
    /// re-dials just like an install does — and, since the launch defaults
    /// ride along (EXP-437), so does a Settings → Agents edit.
    pub fn agent_advertisement(&self, settings: &Settings) -> AgentAdvertisement {
        AgentAdvertisement {
            agents: self
                .installed_agents()
                .into_iter()
                .map(|agent| agent.id().to_string())
                .collect(),
            unauthed_agents: self
                .unauthed_agents()
                .into_iter()
                .map(|agent| agent.id().to_string())
                .collect(),
            default_agent: settings.default_agent.id().to_string(),
            launch_defaults: self
                .installed_agents()
                .into_iter()
                .map(|agent| {
                    let options = crate::argv::LaunchOptions::defaults_for(settings, agent);
                    (
                        agent.id().to_string(),
                        AgentLaunchDefaults {
                            model: options.model,
                            effort: options.effort,
                            ultracode: options.ultracode,
                            plan_mode: options.plan_mode,
                            skip_permissions: options.skip_permissions,
                        },
                    )
                })
                .collect(),
        }
    }
}

/// What a device tells the relay + registry about its agent CLIs (EXP-409):
/// `agents` = runnable (installed AND signed in), `unauthed_agents` =
/// installed but signed out (unusable; listed so UIs can say "sign in").
/// EXP-437 adds the machine's per-agent launch defaults so remote
/// Start-coding dialogs pre-fill this device's configuration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentAdvertisement {
    pub agents: Vec<String>,
    pub unauthed_agents: Vec<String>,
    /// `settings.default_agent` as a wire id — remote pickers preselect it
    /// (clamped to `agents` client-side; it may name an uninstalled agent).
    pub default_agent: String,
    /// One entry per RUNNABLE agent, values capability-masked via
    /// [`crate::argv::LaunchOptions::defaults_for`] — the same resolver the
    /// local Start-coding dialog seeds from. `BTreeMap` for deterministic
    /// wire serialization (the steer frames are byte-locked in tests).
    pub launch_defaults: BTreeMap<String, AgentLaunchDefaults>,
}

/// One agent's launch defaults on this machine (EXP-437): mirrors
/// [`crate::argv::LaunchOptions`] minus the agent tag. Blank `model`/`effort`
/// = "CLI default / omit the flag" (valid per the agent's vocabulary).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentLaunchDefaults {
    pub model: String,
    pub effort: String,
    pub ultracode: bool,
    pub plan_mode: bool,
    pub skip_permissions: bool,
}

impl AgentAdvertisement {
    /// Nothing installed at all — the device stays fully offline for remote
    /// start (EXP-367). With only signed-out agents it still dials, so the
    /// machine list can explain itself.
    pub fn nothing_installed(&self) -> bool {
        self.agents.is_empty() && self.unauthed_agents.is_empty()
    }
}

/// Run every check: each agent's resolved program
/// ([`Settings::resolved_path_for`]) — claude version-gated against
/// [`MIN_CLAUDE_VERSION`], every agent sign-in-gated (EXP-409) — and plain
/// `git` from PATH.
pub fn run_doctor(settings: &Settings) -> DoctorReport {
    // EXP-419: a Windows installer edits the registry PATH, which a running
    // process never sees — re-read it so "Check tools" (and every later
    // spawn) finds a just-installed git/agent without an app restart.
    terminal::process::refresh_windows_path();
    let claude_program = settings.resolved_path_for(CodingAgent::Claude);
    let codex_program = settings.resolved_path_for(CodingAgent::Codex);
    let pi_program = settings.resolved_path_for(CodingAgent::Pi);
    let mut claude = check_tool(Tool::Claude, &claude_program);
    apply_version_gate(&mut claude);
    apply_auth_gate(&mut claude, &claude_program);
    let mut codex = check_tool(Tool::Codex, &codex_program);
    apply_auth_gate(&mut codex, &codex_program);
    let mut pi = check_tool(Tool::Pi, &pi_program);
    apply_auth_gate(&mut pi, &pi_program);
    DoctorReport {
        claude,
        codex,
        pi,
        git: check_tool(Tool::Git, "git"),
    }
}

/// The check for ONE agent (launch step 0 re-checks only the selected agent
/// + git via [`run_doctor`]'s full report; presence probes use the full one).
pub fn check_agent(settings: &Settings, agent: CodingAgent) -> ToolCheck {
    terminal::process::refresh_windows_path();
    let program = settings.resolved_path_for(agent);
    let mut check = check_tool(Tool::for_agent(agent), &program);
    if agent == CodingAgent::Claude {
        apply_version_gate(&mut check);
    }
    apply_auth_gate(&mut check, &program);
    check
}

/// Flip a GREEN claude check red when its version parses BELOW
/// [`MIN_CLAUDE_VERSION`]. An unparseable version stays green — never
/// falsely block a nonstandard build.
fn apply_version_gate(check: &mut ToolCheck) {
    if !check.ok {
        return;
    }
    let Some(version) = check.version.as_deref().and_then(parse_claude_version) else {
        return;
    };
    if version < MIN_CLAUDE_VERSION {
        let (major, minor, patch) = version;
        let (min_major, min_minor, min_patch) = MIN_CLAUDE_VERSION;
        check.ok = false;
        check.error = Some(format!(
            "Claude Code {major}.{minor}.{patch} is too old. Update to \
{min_major}.{min_minor}.{min_patch}+ (run: claude update)."
        ));
    }
}

/// EXP-409: stamp `authed` on a still-green agent check and flip it red when
/// the agent is provably signed out. Runs AFTER the version gate (an old
/// claude may predate `auth status`); skips already-failed checks and git.
fn apply_auth_gate(check: &mut ToolCheck, program: &str) {
    apply_auth_gate_with_path(check, program, &terminal::pty::login_path())
}

/// [`apply_auth_gate`] with the PATH injected — split out so tests can probe
/// stub binaries deterministically.
fn apply_auth_gate_with_path(check: &mut ToolCheck, program: &str, path_env: &str) {
    if !check.ok {
        return;
    }
    let authed = match check.tool {
        Tool::Claude => probe_claude_auth(program, path_env),
        Tool::Codex => probe_codex_auth(program, path_env),
        Tool::Pi => probe_pi_auth(),
        Tool::Git => return,
    };
    check.authed = authed;
    if authed == Some(false) {
        check.ok = false;
        check.error = Some(check.tool.signed_out_message().to_string());
    }
}

/// `claude auth status` prints local JSON with a `loggedIn` bool (verified on
/// 2.1.220; [`MIN_CLAUDE_VERSION`] builds carry it). Any spawn failure or
/// unrecognisable output fails open to `None`.
fn probe_claude_auth(program: &str, path_env: &str) -> Option<bool> {
    let mut cmd = background_command(program);
    cmd.env("PATH", path_env).args(["auth", "status"]);
    let output = output_with_timeout(cmd, PROBE_TIMEOUT).ok()?;
    parse_claude_auth_status(&String::from_utf8_lossy(&output.stdout))
}

/// Pull `loggedIn` out of `claude auth status` output, tolerating noise
/// before the JSON object.
pub fn parse_claude_auth_status(stdout: &str) -> Option<bool> {
    let json = &stdout[stdout.find('{')?..];
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    value.get("loggedIn")?.as_bool()
}

/// `codex login status`: exit 0 = logged in; a "not logged in" answer = signed
/// out; anything else (no such subcommand on an old build) fails open.
fn probe_codex_auth(program: &str, path_env: &str) -> Option<bool> {
    let mut cmd = background_command(program);
    cmd.env("PATH", path_env).args(["login", "status"]);
    let output = output_with_timeout(cmd, PROBE_TIMEOUT).ok()?;
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_codex_login_status(output.status.success(), &combined)
}

/// Classify `codex login status` output (split out for tests).
pub fn parse_codex_login_status(success: bool, output: &str) -> Option<bool> {
    if output.to_lowercase().contains("not logged in") {
        return Some(false);
    }
    success.then_some(true)
}

/// pi has NO login command: credentials are provider API keys, from
/// `~/.pi/agent/auth.json` (oauth/key entries per provider) or environment
/// variables. The env list mirrors the notable providers in `pi --help`;
/// a stale/invalid key is undetectable here — this only catches "no
/// credential anywhere", which is the state that hangs a session.
const PI_PROVIDER_ENV_VARS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_OAUTH_TOKEN",
    "OPENAI_API_KEY",
    "AZURE_OPENAI_API_KEY",
    "GEMINI_API_KEY",
    "GROQ_API_KEY",
    "DEEPSEEK_API_KEY",
    "XAI_API_KEY",
    "MISTRAL_API_KEY",
    "CEREBRAS_API_KEY",
    "FIREWORKS_API_KEY",
    "TOGETHER_API_KEY",
    "OPENROUTER_API_KEY",
    "AI_GATEWAY_API_KEY",
    "ZAI_API_KEY",
    "MOONSHOT_API_KEY",
    "MINIMAX_API_KEY",
    "KIMI_API_KEY",
    "OPENCODE_API_KEY",
    "NVIDIA_API_KEY",
    "CLOUDFLARE_API_KEY",
    "AWS_BEARER_TOKEN_BEDROCK",
];

fn probe_pi_auth() -> Option<bool> {
    let env_credential = PI_PROVIDER_ENV_VARS
        .iter()
        .any(|name| std::env::var(name).is_ok_and(|value| !value.trim().is_empty()));
    let auth_json = dirs::home_dir()
        .map(|home| home.join(".pi").join("agent").join("auth.json"))
        .filter(|path| path.exists())
        .map(|path| std::fs::read_to_string(&path).unwrap_or_default());
    pi_auth_state(auth_json.as_deref(), env_credential)
}

/// Classify pi's credential presence (split out for tests): any env key OR a
/// non-empty provider map in auth.json = signed in; a present-but-unreadable
/// auth.json fails open; nothing anywhere = signed out.
pub fn pi_auth_state(auth_json: Option<&str>, env_credential: bool) -> Option<bool> {
    if env_credential {
        return Some(true);
    }
    match auth_json {
        Some(text) => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(serde_json::Value::Object(map)) => Some(!map.is_empty()),
            _ => None,
        },
        None => Some(false),
    }
}

/// Parse `major.minor.patch` off a claude version line
/// (`"2.1.207 (Claude Code)"` → `(2, 1, 207)`). Anything that is not a plain
/// three-part leading version yields `None`.
pub fn parse_claude_version(line: &str) -> Option<(u32, u32, u32)> {
    let token = line.trim().split_whitespace().next()?;
    let mut parts = token.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// `<program> --version`, capturing stdout/stderr — never a shell. Resolves
/// `program` against the augmented login PATH (§6.12), matching the PTY
/// spawn environment the agent will actually run in.
pub fn check_tool(tool: Tool, program: &str) -> ToolCheck {
    check_tool_with_path(tool, program, &terminal::pty::login_path())
}

/// [`check_tool`] with the PATH injected — split out so tests can probe a
/// stub-only directory deterministically.
fn check_tool_with_path(tool: Tool, program: &str, path_env: &str) -> ToolCheck {
    let mut cmd = background_command(program);
    cmd.env("PATH", path_env).arg("--version");
    match output_with_timeout(cmd, PROBE_TIMEOUT) {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match parse_version_output(tool, &stdout) {
                Some(version) => ToolCheck { tool, ok: true, version: Some(version), error: None, authed: None },
                None => ToolCheck {
                    tool,
                    ok: false,
                    version: None,
                    error: Some(format!("{program} --version produced no output")),
                    authed: None,
                },
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = first_line(&stderr)
                .map(str::to_string)
                .unwrap_or_else(|| format!("exit code {}", output.status.code().unwrap_or(-1)));
            ToolCheck {
                tool,
                ok: false,
                version: None,
                error: Some(format!("{program} --version failed: {detail}")),
                authed: None,
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => ToolCheck {
            tool,
            ok: false,
            version: None,
            error: Some(tool.not_found_message().to_string()),
            authed: None,
        },
        Err(err) => ToolCheck {
            tool,
            ok: false,
            version: None,
            error: Some(format!("could not run {program}: {err}")),
            authed: None,
        },
    }
}

/// `Command::output()` with a deadline (EXP-414). On timeout the child is
/// killed and reaped and `ErrorKind::TimedOut` returns — the auth probes'
/// `.ok()?` then fails OPEN (`authed: None`), and `check_tool_with_path`
/// surfaces it as an ordinary "could not run" failure.
fn output_with_timeout(
    mut cmd: std::process::Command,
    timeout: Duration,
) -> std::io::Result<std::process::Output> {
    use std::io::Read as _;
    use std::process::Stdio;
    use wait_timeout::ChildExt as _;

    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // Own process group: a wedged probe may have children of its own (a
    // shell wrapper's real binary, an updater it spawned) — killing just the
    // direct child would leave them holding the pipes.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn()?;
    // Drain the pipes on threads: a child that fills the ~64KB pipe buffer
    // would otherwise never exit and the wait below would never return.
    let mut out_pipe = child.stdout.take().expect("stdout piped above");
    let mut err_pipe = child.stderr.take().expect("stderr piped above");
    let out = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out_pipe.read_to_end(&mut buf);
        buf
    });
    let err = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err_pipe.read_to_end(&mut buf);
        buf
    });
    match child.wait_timeout(timeout)? {
        Some(status) => Ok(std::process::Output {
            status,
            stdout: out.join().unwrap_or_default(),
            stderr: err.join().unwrap_or_default(),
        }),
        None => {
            #[cfg(unix)]
            unsafe {
                libc::killpg(child.id() as i32, libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait(); // reap the direct child
            // Deliberately NOT joining the readers: a survivor outside the
            // process group could still hold a pipe; the threads exit on
            // their own once every writer is gone.
            drop(out);
            drop(err);
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("probe exceeded {timeout:?}"),
            ))
        }
    }
}

/// First non-empty line of `--version` output, with the tool's own noise
/// prefix stripped (`git version 2.39.5 …` → `2.39.5 …`; `codex-cli 0.46.0`
/// → `0.46.0`; claude's `1.0.35 (Claude Code)` and pi's bare semver pass
/// through).
pub fn parse_version_output(tool: Tool, stdout: &str) -> Option<String> {
    let line = first_line(stdout)?;
    let stripped = match tool {
        Tool::Git => line.strip_prefix("git version ").unwrap_or(line),
        Tool::Codex => line.strip_prefix("codex-cli ").unwrap_or(line),
        Tool::Claude | Tool::Pi => line,
    };
    let trimmed = stripped.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn first_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|line| !line.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_git_version_line() {
        assert_eq!(
            parse_version_output(Tool::Git, "git version 2.39.5 (Apple Git-154)\n"),
            Some("2.39.5 (Apple Git-154)".to_string())
        );
        assert_eq!(
            parse_version_output(Tool::Git, "git version 2.45.0\n"),
            Some("2.45.0".to_string())
        );
    }

    #[test]
    fn parses_claude_codex_and_pi_version_lines() {
        assert_eq!(
            parse_version_output(Tool::Claude, "1.0.35 (Claude Code)\n"),
            Some("1.0.35 (Claude Code)".to_string())
        );
        // Tolerates warning noise before the version line.
        assert_eq!(
            parse_version_output(Tool::Claude, "\n  2.1.0 (Claude Code)\n"),
            Some("2.1.0 (Claude Code)".to_string())
        );
        // Codex prints a `codex-cli ` prefix.
        assert_eq!(
            parse_version_output(Tool::Codex, "codex-cli 0.46.0\n"),
            Some("0.46.0".to_string())
        );
        assert_eq!(
            parse_version_output(Tool::Codex, "0.46.0\n"),
            Some("0.46.0".to_string())
        );
        // pi prints a bare version.
        assert_eq!(
            parse_version_output(Tool::Pi, "0.80.10\n"),
            Some("0.80.10".to_string())
        );
    }

    #[test]
    fn empty_output_parses_to_none() {
        assert_eq!(parse_version_output(Tool::Claude, ""), None);
        assert_eq!(parse_version_output(Tool::Git, "   \n \n"), None);
    }

    #[test]
    fn claude_version_triples_parse_and_junk_does_not() {
        assert_eq!(parse_claude_version("2.1.215 (Claude Code)"), Some((2, 1, 215)));
        assert_eq!(parse_claude_version("  9.9.9 (Claude Code stub)"), Some((9, 9, 9)));
        assert_eq!(parse_claude_version("2.1.203"), Some((2, 1, 203)));
        // Not a plain three-part leading version → None (gate stays open).
        assert_eq!(parse_claude_version("git version 2.39.5"), None);
        assert_eq!(parse_claude_version("2.1"), None);
        assert_eq!(parse_claude_version("2.1.203.7"), None);
        assert_eq!(parse_claude_version("v2.1.203"), None);
        assert_eq!(parse_claude_version(""), None);
    }

    fn green(tool: Tool, version: &str) -> ToolCheck {
        ToolCheck {
            tool,
            ok: true,
            version: Some(version.to_string()),
            error: None,
            authed: None,
        }
    }

    fn red(tool: Tool) -> ToolCheck {
        ToolCheck {
            tool,
            ok: false,
            version: None,
            error: Some(tool.not_found_message().to_string()),
            authed: None,
        }
    }

    /// The version gate: below-minimum flips red with the actionable
    /// "claude update" copy; at/above minimum and unparseable stay green.
    #[test]
    fn version_gate_blocks_old_clis_with_update_copy() {
        let mut old = green(Tool::Claude, "2.1.199 (Claude Code)");
        apply_version_gate(&mut old);
        assert!(!old.ok);
        assert_eq!(
            old.error.as_deref(),
            Some("Claude Code 2.1.199 is too old. Update to 2.1.215+ (run: claude update).")
        );

        // Exactly the minimum and newer stay green.
        for version in ["2.1.215 (Claude Code)", "2.1.230 (Claude Code)", "3.0.0"] {
            let mut check = green(Tool::Claude, version);
            apply_version_gate(&mut check);
            assert!(check.ok, "{version} must pass the gate");
            assert_eq!(check.error, None);
        }

        // Unparseable version → green (never falsely block a nonstandard
        // build).
        let mut odd = green(Tool::Claude, "nightly (Claude Code)");
        apply_version_gate(&mut odd);
        assert!(odd.ok);

        // A check that already failed is left alone (keeps its own error).
        let mut dead = red(Tool::Claude);
        apply_version_gate(&mut dead);
        assert_eq!(
            dead.error.as_deref(),
            Some("claude not found on PATH. Set an absolute path.")
        );
    }

    /// `run_doctor` end-to-end against stub claude binaries: an old version
    /// fails the CLAUDE gate with the update copy; a new one passes.
    #[cfg(unix)]
    #[test]
    fn run_doctor_gates_on_the_stub_version() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-coding-doctor-gate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        let write_stub = |name: &str, version: &str| {
            let path = dir.join(name);
            fs::write(&path, format!("#!/bin/sh\necho '{version} (Claude Code)'\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            path
        };

        // Exec'ing a just-written script can hit ETXTBSY when a concurrent
        // test's fork briefly holds the write fd (the suite spawns many git
        // children) — retry the transient race instead of flaking.
        let run_doctor_retrying = |settings: &Settings| {
            for _ in 0..20 {
                let report = run_doctor(settings);
                let busy = report
                    .first_failure_for(CodingAgent::Claude)
                    .and_then(|check| check.error.as_deref())
                    .is_some_and(|error| error.contains("Text file busy"));
                if !busy {
                    return report;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            run_doctor(settings)
        };

        let old = write_stub("claude-old", "2.1.100");
        let settings = Settings {
            claude_path: old.to_string_lossy().into_owned(),
            ..Settings::default()
        };
        let report = run_doctor_retrying(&settings);
        assert!(report.first_failure_for(CodingAgent::Claude).is_some());
        assert_eq!(
            report
                .first_failure_for(CodingAgent::Claude)
                .and_then(|c| c.error.as_deref()),
            Some("Claude Code 2.1.100 is too old. Update to 2.1.215+ (run: claude update).")
        );

        let new = write_stub("claude-new", "2.1.215");
        let settings = Settings {
            claude_path: new.to_string_lossy().into_owned(),
            ..Settings::default()
        };
        let report = run_doctor_retrying(&settings);
        assert!(
            report.first_failure_for(CodingAgent::Claude).is_none(),
            "2.1.215 must pass: {:?}",
            report.first_failure_for(CodingAgent::Claude)
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// EXP-206: bare tool names resolve against the INJECTED login PATH, not
    /// the process PATH — a stub-only dir finds the stub, and an empty PATH
    /// misses even a real `git`. This is what fixes codex/pi installed in
    /// Homebrew's bin showing "not found" under a GUI launch's minimal PATH.
    #[cfg(unix)]
    #[test]
    fn check_tool_resolves_against_the_injected_path() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-coding-doctor-path-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let stub = dir.join("codex");
        fs::write(&stub, "#!/bin/sh\necho 'codex-cli 0.46.0'\n").unwrap();
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();

        // Retry the transient ETXTBSY race (see run_doctor_gates_on_the_stub_version).
        let mut check = check_tool_with_path(Tool::Codex, "codex", &dir.to_string_lossy());
        for _ in 0..20 {
            if !check
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Text file busy"))
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            check = check_tool_with_path(Tool::Codex, "codex", &dir.to_string_lossy());
        }
        assert!(check.ok, "stub-only PATH must resolve: {:?}", check.error);
        assert_eq!(check.version.as_deref(), Some("0.46.0"));

        // An empty injected PATH misses even a real git — the process PATH
        // must play no part in resolution.
        let check = check_tool_with_path(Tool::Git, "git", "");
        assert!(!check.ok);
        assert_eq!(check.error.as_deref(), Some("git not found on PATH"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_binary_yields_the_actionable_spec_message() {
        let check = check_tool(Tool::Claude, "definitely-not-a-real-binary-exp");
        assert!(!check.ok);
        assert_eq!(
            check.error.as_deref(),
            Some("claude not found on PATH. Set an absolute path.")
        );
        let check = check_tool(Tool::Codex, "definitely-not-a-real-binary-exp");
        assert_eq!(
            check.error.as_deref(),
            Some("codex not found on PATH. Set an absolute path.")
        );
        let check = check_tool(Tool::Pi, "definitely-not-a-real-binary-exp");
        assert_eq!(
            check.error.as_deref(),
            Some("pi not found on PATH. Set an absolute path.")
        );
        let check = check_tool(Tool::Git, "definitely-not-a-real-binary-exp");
        assert_eq!(check.error.as_deref(), Some("git not found on PATH"));
    }

    #[test]
    fn real_git_passes_the_doctor() {
        // git is a hard dependency of this repo's own CI — a real invocation
        // keeps the success path honest.
        let check = check_tool(Tool::Git, "git");
        assert!(check.ok, "git --version failed: {:?}", check.error);
        let version = check.version.unwrap();
        assert!(!version.is_empty());
        assert!(!version.starts_with("git version"), "prefix not stripped: {version}");
    }

    /// EXP-201 per-agent gating: a missing pi never blocks a claude launch;
    /// a missing git blocks EVERY launch; the presence advertisement lists
    /// exactly the usable agents.
    #[test]
    fn report_gates_per_agent_and_advertises_installed() {
        let report = DoctorReport {
            claude: green(Tool::Claude, "2.1.215 (Claude Code)"),
            codex: red(Tool::Codex),
            pi: red(Tool::Pi),
            git: green(Tool::Git, "2.45.0"),
        };
        assert_eq!(report.first_failure_for(CodingAgent::Claude), None);
        assert_eq!(
            report.first_failure_for(CodingAgent::Codex),
            Some(&report.codex)
        );
        assert_eq!(report.first_failure_for(CodingAgent::Pi), Some(&report.pi));
        assert!(report.any_agent_ok());
        assert_eq!(report.installed_agents(), vec![CodingAgent::Claude]);

        // git missing blocks every agent (§7.1 step 1 ANDs git in).
        let no_git = DoctorReport {
            git: red(Tool::Git),
            ..report.clone()
        };
        assert_eq!(
            no_git.first_failure_for(CodingAgent::Claude),
            Some(&no_git.git)
        );

        // No agent at all: the affordance-level gate flips.
        let none = DoctorReport {
            claude: red(Tool::Claude),
            ..report.clone()
        };
        assert!(!none.any_agent_ok());
        assert!(none.installed_agents().is_empty());

        // All three installed → all three advertised, in ALL order.
        let all = DoctorReport {
            codex: green(Tool::Codex, "0.46.0"),
            pi: green(Tool::Pi, "0.80.10"),
            ..report.clone()
        };
        assert_eq!(
            all.installed_agents(),
            vec![CodingAgent::Claude, CodingAgent::Codex, CodingAgent::Pi]
        );
    }

    /// EXP-437: the advertisement carries the machine's per-agent launch
    /// defaults — capability-masked (codex never plan/ultracode, pi never
    /// skip), blank efforts preserved, only RUNNABLE agents listed, and the
    /// default agent passed through even when it is not installed.
    #[test]
    fn advertisement_carries_capability_masked_launch_defaults() {
        let report = DoctorReport {
            claude: green(Tool::Claude, "2.1.215 (Claude Code)"),
            codex: green(Tool::Codex, "0.46.0"),
            pi: red(Tool::Pi),
            git: green(Tool::Git, "2.45.0"),
        };
        let settings = Settings {
            claude_model: "opus".into(),
            claude_effort: "".into(),
            claude_ultracode: true,
            claude_plan_mode: true,
            claude_skip_permissions: false,
            codex_model: "".into(),
            codex_effort: "high".into(),
            codex_skip_permissions: true,
            pi_model: "grok-4.5".into(),
            ..Settings::default()
        };
        let advert = report.agent_advertisement(&settings);
        assert_eq!(advert.agents, vec!["claude", "codex"]);
        assert_eq!(advert.default_agent, "claude");
        // Only runnable agents get a defaults entry — pi (red) has none.
        assert_eq!(
            advert.launch_defaults.keys().collect::<Vec<_>>(),
            vec!["claude", "codex"]
        );
        let claude = &advert.launch_defaults["claude"];
        assert_eq!(claude.model, "opus");
        assert_eq!(claude.effort, "");
        assert!(claude.ultracode);
        assert!(claude.plan_mode);
        assert!(!claude.skip_permissions);
        // Capability masking: codex can never carry ultracode/plan, but its
        // per-agent skip-permissions and blank model ride through.
        let codex = &advert.launch_defaults["codex"];
        assert_eq!(codex.model, "");
        assert_eq!(codex.effort, "high");
        assert!(!codex.ultracode);
        assert!(!codex.plan_mode);
        assert!(codex.skip_permissions);

        // A default agent that is NOT runnable still passes through as an id
        // (clients clamp); it simply has no launch_defaults entry.
        let settings = Settings {
            default_agent: CodingAgent::Pi,
            ..settings
        };
        let advert = report.agent_advertisement(&settings);
        assert_eq!(advert.default_agent, "pi");
        assert!(!advert.launch_defaults.contains_key("pi"));
    }

    /// EXP-409: `claude auth status` JSON classification — noise-tolerant,
    /// fails open on junk.
    #[test]
    fn claude_auth_status_parses_logged_in_flag() {
        assert_eq!(
            parse_claude_auth_status(
                "{\n  \"loggedIn\": true,\n  \"authMethod\": \"claude.ai\",\n  \"apiProvider\": \"firstParty\"\n}\n"
            ),
            Some(true)
        );
        assert_eq!(
            parse_claude_auth_status("{\"loggedIn\": false, \"authMethod\": \"none\"}"),
            Some(false)
        );
        // Warning noise before the JSON is tolerated.
        assert_eq!(
            parse_claude_auth_status("some warning line\n{\"loggedIn\": true}"),
            Some(true)
        );
        // No JSON / no flag / not a bool → fail open.
        assert_eq!(parse_claude_auth_status("Unknown command `auth`"), None);
        assert_eq!(parse_claude_auth_status("{\"authMethod\": \"none\"}"), None);
        assert_eq!(parse_claude_auth_status("{\"loggedIn\": \"yes\"}"), None);
        assert_eq!(parse_claude_auth_status(""), None);
    }

    /// EXP-409: `codex login status` classification — "not logged in" beats
    /// the exit code; an unknown failure (old build without the subcommand)
    /// fails open.
    #[test]
    fn codex_login_status_classifies_output() {
        assert_eq!(parse_codex_login_status(true, "Logged in using ChatGPT\n"), Some(true));
        assert_eq!(parse_codex_login_status(false, "Not logged in\n"), Some(false));
        // Some builds print the answer but still exit 0.
        assert_eq!(parse_codex_login_status(true, "Not logged in\n"), Some(false));
        assert_eq!(parse_codex_login_status(false, "error: unknown subcommand `login`"), None);
    }

    /// EXP-409: pi credential presence — env key or a non-empty auth.json
    /// map counts; unreadable auth.json fails open; nothing = signed out.
    #[test]
    fn pi_auth_state_classifies_credentials() {
        assert_eq!(pi_auth_state(None, true), Some(true));
        assert_eq!(pi_auth_state(Some("{\"openai-codex\": {\"type\": \"oauth\"}}"), false), Some(true));
        assert_eq!(pi_auth_state(Some("{}"), false), Some(false));
        assert_eq!(pi_auth_state(None, false), Some(false));
        // Present but unparseable → fail open.
        assert_eq!(pi_auth_state(Some("not json"), false), None);
        assert_eq!(pi_auth_state(Some("[]"), false), None);
    }

    /// EXP-409 end-to-end against stub binaries: a signed-out claude/codex
    /// flips red with the sign-in copy (version kept), a signed-in one stays
    /// green with `authed: Some(true)`, and the report's gates treat
    /// signed-out as not installed while `unauthed_agents` still names it.
    #[cfg(unix)]
    #[test]
    fn auth_gate_flips_signed_out_agents() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-coding-doctor-auth-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let write_stub = |name: &str, body: &str| {
            let path = dir.join(name);
            fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            path
        };

        // Retry the transient ETXTBSY race (see run_doctor_gates_on_the_stub_version).
        let checked = |tool: Tool, program: &std::path::Path| {
            let program = program.to_string_lossy();
            for _ in 0..20 {
                let mut check = check_tool(tool, &program);
                apply_auth_gate_with_path(&mut check, &program, &terminal::pty::login_path());
                let busy = check
                    .error
                    .as_deref()
                    .is_some_and(|error| error.contains("Text file busy"));
                if !busy {
                    return check;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("stub stayed ETXTBSY");
        };

        let claude_out = write_stub(
            "claude-out",
            "case \"$1\" in\n--version) echo '9.9.9 (Claude Code)';;\nauth) echo '{\"loggedIn\": false, \"authMethod\": \"none\"}';;\nesac",
        );
        let check = checked(Tool::Claude, &claude_out);
        assert!(!check.ok);
        assert!(check.signed_out());
        assert_eq!(check.version.as_deref(), Some("9.9.9 (Claude Code)"));
        assert_eq!(
            check.error.as_deref(),
            Some("claude is installed but not signed in. Run `claude` in a terminal and log in.")
        );

        let claude_in = write_stub(
            "claude-in",
            "case \"$1\" in\n--version) echo '9.9.9 (Claude Code)';;\nauth) echo '{\"loggedIn\": true}';;\nesac",
        );
        let check = checked(Tool::Claude, &claude_in);
        assert!(check.ok, "{:?}", check.error);
        assert_eq!(check.authed, Some(true));

        let codex_out = write_stub(
            "codex-out",
            "case \"$1\" in\n--version) echo 'codex-cli 0.46.0';;\nlogin) echo 'Not logged in' >&2; exit 1;;\nesac",
        );
        let check = checked(Tool::Codex, &codex_out);
        assert!(!check.ok);
        assert!(check.signed_out());
        assert_eq!(
            check.error.as_deref(),
            Some("codex is installed but not signed in. Run `codex login` in a terminal.")
        );

        // The report-level view: signed-out codex is out of installed_agents
        // but named by unauthed_agents; the affordance gate ignores it.
        let report = DoctorReport {
            claude: checked(Tool::Claude, &claude_in),
            codex: checked(Tool::Codex, &codex_out),
            pi: red(Tool::Pi),
            git: green(Tool::Git, "2.45.0"),
        };
        assert_eq!(report.installed_agents(), vec![CodingAgent::Claude]);
        assert_eq!(report.unauthed_agents(), vec![CodingAgent::Codex]);
        assert!(report.any_agent_ok());
        assert!(report.first_failure_for(CodingAgent::Codex).is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    /// EXP-414: a wedged probe is killed at the deadline instead of stalling
    /// the caller (the CLI daemon runs the doctor inline every 5 minutes).
    #[cfg(unix)]
    #[test]
    fn output_with_timeout_kills_a_hung_child() {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "sleep 30"]);
        let started = std::time::Instant::now();
        let err = output_with_timeout(cmd, Duration::from_millis(300)).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "must return at the deadline, not the child's own runtime"
        );
    }

    /// A fast child behaves exactly like `Command::output()` — stdout,
    /// stderr and exit status all ride through.
    #[cfg(unix)]
    #[test]
    fn output_with_timeout_passes_a_fast_child_through() {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "echo out; echo err >&2; exit 3"]);
        let output = output_with_timeout(cmd, Duration::from_secs(10)).unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout), "out\n");
        assert_eq!(String::from_utf8_lossy(&output.stderr), "err\n");
        assert_eq!(output.status.code(), Some(3));
    }
}
