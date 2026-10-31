//! Agent detection (EXP-1426, out of `coding::doctor`): is `claude` / `codex`
//! (or `git`) installed, which version, is it new enough, and is it signed
//! in, all answered by the vendor's own binary (`--version`, `claude auth
//! status`, `codex login status`, the codex app-server handshake). Nothing
//! here reads a credential file.
//!
//! Every probe takes the PATH to resolve the program against (a GUI launch
//! carries a minimal one; Exponential passes `terminal::pty::login_path()`)
//! and every shell-out is bounded by [`PROBE_TIMEOUT`]. Every probe FAILS
//! OPEN: an unrecognisable answer (older CLI, changed output) is `None`,
//! never a false "signed out".
//!
//! The `config` pairs (`(CLAUDE_CONFIG_DIR, dir)` / `(CODEX_HOME, dir)`) are
//! how a multi-account host points a probe at one profile; a single-account
//! host passes `None` and gets the CLI's default home
//! ([`probe_claude_auth_status`], [`probe_codex_auth`], [`is_signed_in`]).
//!
//! What stays in `coding::doctor`: `run_doctor`, the account profiles, the
//! device caps and the advertisement, and the product's sign-in copy. Its
//! `ToolCheck` is this one plus the multi-account fields
//! (`From<detect::ToolCheck>`).

use std::fmt;
use std::path::Path;
use std::time::Duration;

use crate::agent::CodingAgent;
use crate::process::background_command;

/// EXP-414: the deadline for every probe shell-out. The CLI daemon re-runs
/// the doctor inline on a 5-minute cadence — an agent CLI that wedges (a
/// hung update check, a dead network filesystem) would otherwise stall that
/// loop, and with it the device's presence, indefinitely.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// The minimum supported Claude Code version: `--permission-mode auto`
/// (EXP-201's default posture) is verified on 2.1.215; `--effort ultracode`
/// landed in 2.1.203, `--permission-mode plan`/`manual` in 2.1.200 —
/// everything the launcher's claude argv relies on.
pub const MIN_CLAUDE_VERSION: (u32, u32, u32) = (2, 1, 215);

/// EXP-746: the minimum Claude Code that speaks the ACP engine's control
/// protocol (`--input-format stream-json` + `--permission-prompt-tool
/// stdio`). Deliberately SEPARATE from [`MIN_CLAUDE_VERSION`]: it never
/// turns the doctor row red, but a coding launch below it is REFUSED with
/// `coding::doctor::ToolCheck::acp_note` saying why (EXP-773: the engine is the one
/// transport, there is no terminal fallback).
pub const MIN_CLAUDE_ACP_VERSION: (u32, u32, u32) = (2, 1, 263);

/// EXP-758: the same floor for codex, and for the same reason — an older
/// `codex app-server` resolves to ACP, fails the `initialize` + `thread/start`
/// handshake the adapter is written against, and leaves a session row nobody
/// can end but by hand.
///
/// The 0.144 line is what the adapter and its fixtures were recorded from
/// (`engine/tests/codex_adapter.rs`: "recorded from the installed codex
/// 0.144.5"), and nothing older was ever exercised — so it is the lowest
/// version this build claims. Pinned at `.0` rather than `.5` because the
/// patch releases inside that minor share the app-server shape; anything
/// below it is unverified, not known-broken, which is exactly why the check
/// never turns the doctor row red — but a coding launch below it is refused
/// with the note (EXP-773), the same as claude's.
pub const MIN_CODEX_ACP_VERSION: (u32, u32, u32) = (0, 144, 0);

/// The local binaries a host ever shells out to (§7.1 step 3: argv `git` +
/// the agent CLIs, never `gh`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Claude,
    Codex,
    Git,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::Codex => "codex",
            Tool::Git => "git",
        }
    }

    /// The agent this tool check backs (`None` for git).
    pub fn agent(self) -> Option<CodingAgent> {
        match self {
            Tool::Claude => Some(CodingAgent::Claude),
            Tool::Codex => Some(CodingAgent::Codex),
            Tool::Git => None,
        }
    }

    /// The tool for `agent`.
    pub fn for_agent(agent: CodingAgent) -> Tool {
        match agent {
            CodingAgent::Claude => Tool::Claude,
            CodingAgent::Codex => Tool::Codex,
        }
    }

    /// The §7.7 red actionable message for a missing binary.
    pub fn not_found_message(self) -> &'static str {
        match self {
            Tool::Claude => "claude not found on PATH. Set an absolute path.",
            Tool::Codex => "codex did not start at the configured path. Remove codexPath from settings.json or set an absolute path.",
            Tool::Git => "git not found on PATH",
        }
    }
}

impl fmt::Display for Tool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// One `--version` probe: green with the version, or a red actionable error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCheck {
    pub tool: Tool,
    pub ok: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

impl ToolCheck {
    fn failed(tool: Tool, error: String) -> Self {
        Self { tool, ok: false, version: None, error: Some(error) }
    }

    /// The parsed `major.minor.patch` of a green check (claude and codex
    /// share the shape once [`parse_version_output`] stripped codex's prefix).
    pub fn semver(&self) -> Option<(u32, u32, u32)> {
        self.version.as_deref().and_then(parse_claude_version)
    }

    /// Whether this agent check is installed AND at least `min`. An
    /// unparseable version reads as new enough: never falsely block a
    /// nonstandard build.
    pub fn at_least(&self, min: (u32, u32, u32)) -> bool {
        self.ok && self.semver().is_none_or(|version| version >= min)
    }
}

/// The DEEP codex ACP check: the real `codex app-server --listen stdio://`
/// handshake, bounded by [`PROBE_TIMEOUT`] and killed on drop. Not part of
/// the routine doctor (a 10-second handshake per launch); hosts call it on
/// demand.
pub fn probe_codex_acp(program: &str, path_env: &str) -> bool {
    crate::codex_app_server::probe(program, path_env, PROBE_TIMEOUT).is_ok()
}

/// The full `claude auth status` answer (EXP-484). `loggedIn` is the EXP-409
/// gate; the rest names the account and decides whether its usage windows
/// may be read at all ([`Self::usage_eligible`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClaudeAuthStatus {
    pub logged_in: bool,
    /// `claude.ai` on a subscription login; `apiKey`/absent otherwise.
    pub auth_method: Option<String>,
    /// `firstParty` on a direct Anthropic account; `bedrock`/`vertex` on a
    /// cloud-provider login (no OAuth usage endpoint there).
    pub api_provider: Option<String>,
    pub email: Option<String>,
    /// `max`/`pro`/… — the caption's plan half.
    pub subscription_type: Option<String>,
}

impl ClaudeAuthStatus {
    /// Whether this login can answer the OAuth usage endpoint: a signed-in
    /// claude.ai subscription on first-party Anthropic. An API-key or
    /// Bedrock/Vertex login has no usage windows to read — never spend a
    /// request finding that out again.
    pub fn usage_eligible(&self) -> bool {
        self.logged_in
            && self.auth_method.as_deref() == Some("claude.ai")
            && self
                .api_provider
                .as_deref()
                .is_none_or(|provider| provider == "firstParty")
    }

}

/// `claude auth status` prints local JSON with a `loggedIn` bool (verified on
/// 2.1.220; [`MIN_CLAUDE_VERSION`] builds carry it). Any spawn failure or
/// unrecognisable output fails open to `None`. EXP-792: `config` = the
/// `(CLAUDE_CONFIG_DIR, dir)` pair of an account profile, so the answer
/// names THAT login; `None` = the ambient login.
pub fn probe_claude_auth_status_in(
    program: &str,
    path_env: &str,
    config: Option<(&str, &Path)>,
) -> Option<ClaudeAuthStatus> {
    let mut cmd = background_command(program);
    cmd.env("PATH", path_env).args(["auth", "status"]);
    if let Some((key, dir)) = config {
        cmd.env(key, dir);
    }
    let output = output_with_timeout(cmd, PROBE_TIMEOUT).ok()?;
    parse_claude_auth_status_full(&String::from_utf8_lossy(&output.stdout))
}

/// `claude auth status` against the CLI's DEFAULT config home.
pub fn probe_claude_auth_status(program: &str, path_env: &str) -> Option<ClaudeAuthStatus> {
    probe_claude_auth_status_in(program, path_env, None)
}

/// `codex login status` against the CLI's DEFAULT home.
pub fn probe_codex_auth(program: &str, path_env: &str) -> Option<bool> {
    probe_codex_auth_in(program, path_env, None)
}

/// Whether `agent`'s ONE local login (the CLI's default home) is signed in:
/// `Some(false)` only when the CLI provably says so, `None` when the probe
/// could not answer (fail open).
pub fn is_signed_in(agent: CodingAgent, program: &str, path_env: &str) -> Option<bool> {
    match agent {
        CodingAgent::Claude => probe_claude_auth_status(program, path_env).map(|status| status.logged_in),
        CodingAgent::Codex => probe_codex_auth(program, path_env),
    }
}

/// Pull `loggedIn` out of `claude auth status` output, tolerating noise
/// before the JSON object.
pub fn parse_claude_auth_status(stdout: &str) -> Option<bool> {
    parse_claude_auth_status_full(stdout).map(|status| status.logged_in)
}

/// The whole `claude auth status` object (EXP-484). `loggedIn` must be a
/// real bool — everything else is optional and tolerated absent, so an
/// older/narrower answer still gates correctly and simply names nobody.
pub fn parse_claude_auth_status_full(stdout: &str) -> Option<ClaudeAuthStatus> {
    let json = &stdout[stdout.find('{')?..];
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let text = |key: &str| {
        value
            .get(key)
            .and_then(|found| found.as_str())
            .map(str::trim)
            .filter(|found| !found.is_empty())
            .map(str::to_string)
    };
    Some(ClaudeAuthStatus {
        logged_in: value.get("loggedIn")?.as_bool()?,
        auth_method: text("authMethod"),
        api_provider: text("apiProvider"),
        email: text("email"),
        subscription_type: text("subscriptionType"),
    })
}

/// `codex login status`: exit 0 = logged in; a "not logged in" answer = signed
/// out; anything else (no such subcommand on an old build) fails open.
/// `config` = a profile's `(CODEX_HOME, dir)` pair (EXP-792); `None` = the
/// ambient login.
pub fn probe_codex_auth_in(program: &str, path_env: &str, config: Option<(&str, &Path)>) -> Option<bool> {
    let mut cmd = background_command(program);
    cmd.env("PATH", path_env).args(["login", "status"]);
    if let Some((key, dir)) = config {
        cmd.env(key, dir);
    }
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

/// EXP-758: the same parse for codex. [`parse_version_output`] has already
/// stripped the `codex-cli ` prefix, so the stored version is the bare
/// `0.144.5` triple [`parse_claude_version`] reads — named separately so the
/// two ACP floors read alike at their call sites.
pub fn parse_codex_version(line: &str) -> Option<(u32, u32, u32)> {
    parse_claude_version(line)
}


/// `<program> --version`, capturing stdout/stderr (never a shell), with
/// `program` resolved against `path_env`: pass the PATH the agent will
/// actually be spawned with.
pub fn check_tool(tool: Tool, program: &str, path_env: &str) -> ToolCheck {
    let mut cmd = background_command(program);
    cmd.env("PATH", path_env).arg("--version");
    match output_with_timeout(cmd, PROBE_TIMEOUT) {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match parse_version_output(tool, &stdout) {
                Some(version) => ToolCheck { tool, ok: true, version: Some(version), error: None },
                None => ToolCheck::failed(tool, format!("{program} --version produced no output")),
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = first_line(&stderr)
                .map(str::to_string)
                .unwrap_or_else(|| format!("exit code {}", output.status.code().unwrap_or(-1)));
            ToolCheck::failed(tool, format!("{program} --version failed: {detail}"))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            ToolCheck::failed(tool, tool.not_found_message().to_string())
        }
        Err(err) => ToolCheck::failed(tool, format!("could not run {program}: {err}")),
    }
}

/// `Command::output()` with a deadline (EXP-414). On timeout the child is
/// killed and reaped and `ErrorKind::TimedOut` returns — the auth probes'
/// `.ok()?` then fails OPEN (`authed: None`), and [`check_tool`]
/// surfaces it as an ordinary "could not run" failure.
pub fn output_with_timeout(
    cmd: std::process::Command,
    timeout: Duration,
) -> std::io::Result<std::process::Output> {
    output_with_optional_stdin(cmd, None, timeout)
}

/// [`output_with_timeout`] for a child that must be FED on stdin.
///
/// EXP-852: `security -i` reads `add-generic-password … -X <hex>` as a line on
/// stdin precisely so the secret never lands in argv, where `ps` would publish
/// it to every process on the machine. Same process-group, kill-on-deadline
/// and drain-on-threads discipline as [`output_with_timeout`] — the only
/// difference is that stdin is a pipe, written from its OWN thread which then
/// drops it. That drop is the point: a child which reads less than we wrote
/// (or nothing at all) would otherwise leave the write blocked on a full pipe
/// buffer forever, while a child which waits for EOF needs the close to make
/// progress at all.
pub fn output_with_stdin_timeout(
    cmd: std::process::Command,
    input: &str,
    timeout: Duration,
) -> std::io::Result<std::process::Output> {
    output_with_optional_stdin(cmd, Some(input.to_string()), timeout)
}

/// The one implementation behind both; `None` keeps stdin closed.
fn output_with_optional_stdin(
    mut cmd: std::process::Command,
    input: Option<String>,
    timeout: Duration,
) -> std::io::Result<std::process::Output> {
    use std::io::Write as _;
    use std::process::Stdio;
    use wait_timeout::ChildExt as _;

    cmd.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Own process group: a wedged probe may have children of its own (a
    // shell wrapper's real binary, an updater it spawned) — killing just the
    // direct child would leave them holding the pipes.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn()?;
    // Write stdin on its own thread and close the pipe by dropping it. Taking
    // the handle off the `Child` matters even when the write finishes
    // instantly: a `Child` that still owns its stdin never lets the callee see
    // EOF, and `security -i` reads until it does.
    if let Some(input) = input {
        if let Some(mut pipe) = child.stdin.take() {
            std::thread::spawn(move || {
                let _ = pipe.write_all(input.as_bytes());
                let _ = pipe.flush();
            });
        }
    }
    // Drain the pipes on threads: a child that fills the ~64KB pipe buffer
    // would otherwise never exit and the wait below would never return.
    // EXP-1287: the readers poll with a stop flag instead of blocking in
    // `read_to_end`, so a probe survivor outside the process group that
    // keeps a pipe open no longer parks two threads for the host's life.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let out_pipe = child.stdout.take().expect("stdout piped above");
    let err_pipe = child.stderr.take().expect("stderr piped above");
    let out = {
        let stop = stop.clone();
        std::thread::spawn(move || drain_until(out_pipe, &stop))
    };
    let err = {
        let stop = stop.clone();
        std::thread::spawn(move || drain_until(err_pipe, &stop))
    };
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
            // The readers end at their next poll (a survivor holding a
            // pipe cannot keep them); not waited for here.
            stop.store(true, std::sync::atomic::Ordering::Release);
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("probe exceeded {timeout:?}"),
            ))
        }
    }
}

/// Read `pipe` to EOF, giving up once `stop` is set: the read waits in
/// `poll(2)` slices of [`DRAIN_POLL`] so a writer that never closes cannot
/// pin the thread.
#[cfg(unix)]
fn drain_until<R: std::io::Read + std::os::fd::AsRawFd>(
    mut pipe: R,
    stop: &std::sync::atomic::AtomicBool,
) -> Vec<u8> {
    let fd = pipe.as_raw_fd();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        if stop.load(std::sync::atomic::Ordering::Acquire) {
            return buf;
        }
        let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
        // SAFETY: one valid pollfd for an fd this thread owns.
        let ready = unsafe { libc::poll(&mut pfd, 1, DRAIN_POLL.as_millis() as libc::c_int) };
        if ready < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return buf;
        }
        if ready == 0 {
            continue;
        }
        match pipe.read(&mut chunk) {
            Ok(0) => return buf,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return buf,
        }
    }
}

/// Windows pipes are not pollable this way and no probe runs in a process
/// group there: a plain read to EOF.
#[cfg(not(unix))]
fn drain_until<R: std::io::Read>(mut pipe: R, _stop: &std::sync::atomic::AtomicBool) -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = pipe.read_to_end(&mut buf);
    buf
}

/// How long a draining reader waits per poll before looking at its stop flag.
const DRAIN_POLL: Duration = Duration::from_millis(100);

/// First non-empty line of `--version` output, with the tool's own noise
/// prefix stripped (`git version 2.39.5 …` → `2.39.5 …`; `codex-cli 0.46.0`
/// → `0.46.0`; claude's `1.0.35 (Claude Code)` passes
/// through).
pub fn parse_version_output(tool: Tool, stdout: &str) -> Option<String> {
    let line = first_line(stdout)?;
    let stripped = match tool {
        Tool::Git => line.strip_prefix("git version ").unwrap_or(line),
        Tool::Codex => line.strip_prefix("codex-cli ").unwrap_or(line),
        Tool::Claude => line,
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
    fn a_missing_binary_is_the_actionable_not_found_row() {
        let check = check_tool(Tool::Codex, "exp-1426-definitely-not-installed", "/nonexistent");
        assert!(!check.ok);
        assert_eq!(check.error.as_deref(), Some(Tool::Codex.not_found_message()));
        assert!(!check.at_least(MIN_CODEX_ACP_VERSION));
    }

    #[test]
    fn version_floors_read_the_parsed_triple_and_fail_open() {
        let check = |version: &str| ToolCheck {
            tool: Tool::Claude,
            ok: true,
            version: Some(version.to_string()),
            error: None,
        };
        assert!(check("2.1.300 (Claude Code)").at_least(MIN_CLAUDE_VERSION));
        assert!(!check("2.1.100 (Claude Code)").at_least(MIN_CLAUDE_VERSION));
        // A nonstandard build is never falsely blocked.
        assert!(check("nightly").at_least(MIN_CLAUDE_VERSION));
        assert_eq!(Tool::for_agent(CodingAgent::Codex), Tool::Codex);
        assert_eq!(Tool::Codex.agent(), Some(CodingAgent::Codex));
    }

    #[test]
    fn auth_and_version_parsers() {
        assert_eq!(parse_claude_auth_status("noise {\"loggedIn\": true}"), Some(true));
        assert_eq!(parse_claude_auth_status("{}"), None);
        assert_eq!(parse_codex_login_status(false, "Not logged in"), Some(false));
        assert_eq!(parse_codex_login_status(true, "Logged in using ChatGPT"), Some(true));
        assert_eq!(parse_codex_login_status(false, "error: unknown"), None);
        assert_eq!(
            parse_version_output(Tool::Codex, "codex-cli 0.144.5\n").as_deref(),
            Some("0.144.5")
        );
        assert_eq!(parse_codex_version("0.144.5"), Some((0, 144, 5)));
    }
}
