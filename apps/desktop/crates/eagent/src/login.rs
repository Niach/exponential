//! EXP-1426 — ONE local account's sign-in, sign-out and "is it signed in",
//! by running the agent CLI's own commands. Never a re-implementation:
//!
//! | agent  | sign in                        | sign out           |
//! |--------|--------------------------------|--------------------|
//! | claude | `claude auth login --claudeai` | `claude auth logout` |
//! | codex  | `codex login --device-auth`    | `codex logout`     |
//!
//! The login runs in a PTY against the CLI's DEFAULT config home: no
//! staging dir, no profile, no config-dir variable is ever set
//! ([`login_command`] carries only `BROWSER` for a remote sign-in). The CLI
//! writes its own credential wherever it always does; nothing here reads,
//! copies, moves or writes one. [`LocalLogin`] scrapes the PTY's output for
//! the sign-in URL (and codex's device code) through the shared screen
//! readers ([`crate::login_screen`]) and reports it as a [`LoginProgress`];
//! [`is_signed_in`] asks the CLI afterwards.
//!
//! Exponential's multi-account flow (`coding::agent_login`: a staging dir
//! per sign-in, commit by email, import of the ambient login) builds on the
//! same plan ([`login_command`]), the same sign-out ([`logout_in`]) and the
//! same scrapers, and drives its PTY through its own terminal emulator.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};

use crate::agent::CodingAgent;
use crate::detect::{output_with_timeout, PROBE_TIMEOUT};
use crate::login_screen::{observe_login_screen, LoginObservation};

pub use crate::detect::is_signed_in;

/// `devices.completeCommand`'s result cap ([`LoginProgress::to_result_text`]).
pub const RESULT_TEXT_MAX: usize = 2000;

/// What to run for one agent's sign-in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginCommand {
    pub program: String,
    pub args: Vec<String>,
    /// The env overlay. NEVER a config-dir variable: the login lands in the
    /// CLI's default home.
    pub env: Vec<(String, String)>,
    /// A title for whatever shows the login (a terminal tab, a sheet).
    pub title: String,
}

/// The sign-in command for `agent` run as `program`.
///
/// `remote` = nobody is necessarily sitting at this machine (EXP-695), so
/// the CLI must not pop a browser HERE: the requester gets the link and
/// opens it on their own device. The agent CLIs launch the sign-in URL
/// through `$BROWSER` when it is set, so the plan pins `BROWSER=true` (`true`
/// the no-op command).
pub fn login_command(agent: CodingAgent, program: impl Into<String>, remote: bool) -> LoginCommand {
    let (args, title): (&[&str], &str) = match agent {
        // Verified on 2.1.251: `--claudeai` picks the subscription method
        // outright, so there is no picker to drive.
        CodingAgent::Claude => (&["auth", "login", "--claudeai"], "Sign in to Claude"),
        // Device auth prints a URL plus a short code — the shape a phone can
        // finish.
        CodingAgent::Codex => (&["login", "--device-auth"], "Sign in to Codex"),
    };
    let mut env = Vec::new();
    if remote {
        env.push(("BROWSER".to_string(), "true".to_string()));
    }
    LoginCommand {
        program: program.into(),
        args: args.iter().map(|arg| arg.to_string()).collect(),
        env,
        title: title.to_string(),
    }
}

/// The CLI's own sign-out argv for `agent`.
pub fn logout_args(agent: CodingAgent) -> &'static [&'static str] {
    match agent {
        CodingAgent::Claude => &["auth", "logout"],
        CodingAgent::Codex => &["logout"],
    }
}

/// Sign the ONE local login of `agent` out (the CLI's default home).
/// Blocking, bounded by [`PROBE_TIMEOUT`]; `Err` carries a user-facing
/// sentence.
pub fn logout(agent: CodingAgent, program: &str, path_env: &str) -> Result<(), String> {
    logout_in(agent, program, path_env, None)
}

/// [`logout`] with an env pair pointing the CLI at another config dir (how
/// Exponential signs ONE account profile out).
pub fn logout_in(
    agent: CodingAgent,
    program: &str,
    path_env: &str,
    env: Option<(&str, &str)>,
) -> Result<(), String> {
    let mut cmd = crate::process::background_command(program);
    cmd.env("PATH", path_env).args(logout_args(agent));
    if let Some((key, value)) = env {
        cmd.env(key, value);
    }
    match output_with_timeout(cmd, PROBE_TIMEOUT) {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = stderr
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or("the command failed");
            Err(format!("Could not sign out of {}: {detail}", agent.id()))
        }
        Err(err) => Err(format!("Could not run {program}: {err}")),
    }
}

/// How far a login got. `Url` is the useful one — the sign-in link (and
/// Codex's device code) is on screen and the requester can act on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoginPhase {
    Url,
    Failed,
}

/// The `agent_login` command's result payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginProgress {
    pub agent: String,
    pub phase: LoginPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Codex's device code; claude's flow has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// A failure sentence (or a note beside a URL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl LoginProgress {
    /// The sign-in URL is up.
    pub fn url(agent: CodingAgent, url: impl Into<String>, code: Option<String>) -> Self {
        Self {
            agent: agent.id().to_string(),
            phase: LoginPhase::Url,
            url: Some(url.into()),
            code,
            message: None,
        }
    }

    /// The login ended without ever showing one.
    pub fn failed(agent: CodingAgent, message: impl Into<String>) -> Self {
        Self {
            agent: agent.id().to_string(),
            phase: LoginPhase::Failed,
            url: None,
            code: None,
            message: Some(message.into()),
        }
    }

    /// Serialize for `devices.completeCommand`, inside its 2000-char cap.
    /// The cap is measured the way the server measures it (zod `.max` counts
    /// UTF-16 units) on the SERIALIZED text, and over it the MESSAGE shrinks
    /// until the whole thing fits — never the URL, which is the whole point
    /// of the answer, and never by cutting the JSON itself.
    pub fn to_result_text(&self) -> String {
        let fits = |text: &str| text.encode_utf16().count() <= RESULT_TEXT_MAX;
        let rendered = serde_json::to_string(self).unwrap_or_default();
        if fits(&rendered) {
            return rendered;
        }
        let mut trimmed = self.clone();
        let mut message: Vec<char> = self.message.as_deref().unwrap_or_default().chars().collect();
        loop {
            let cut = message.len().saturating_sub((message.len() / 4).max(16));
            message.truncate(cut);
            trimmed.message = (!message.is_empty()).then(|| message.iter().collect());
            let rendered = serde_json::to_string(&trimmed).unwrap_or_default();
            if fits(&rendered) || message.is_empty() {
                return rendered;
            }
        }
    }

    /// Read one back off a `device_commands.result`. `None` = a result from
    /// some other command kind (or an older build) — never an error.
    pub fn parse(text: &str) -> Option<Self> {
        serde_json::from_str(text.trim()).ok()
    }
}

/// PTY size of a [`LocalLogin`]: roomy, so the CLI never wraps its link.
const COLS: u16 = 120;
const ROWS: u16 = 36;

/// How much PTY output a [`LocalLogin`] keeps (the tail): a login prints a
/// screenful, so this only bounds a CLI that keeps talking.
const OUTPUT_KEEP: usize = 64 * 1024;

/// What one [`LoginWatch::observe`] found that the driver must act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoginStep {
    /// New progress to report (a URL first shown or changed, or a failure).
    Report(LoginProgress),
    /// A method picker waits: write `\r` once to take the default.
    PressEnter,
    /// Nothing new.
    Nothing,
}

/// Turns successive screens of one login into [`LoginStep`]s: each URL (and
/// each failure) is reported once, the picker answered once.
#[derive(Clone, Debug)]
pub struct LoginWatch {
    agent: CodingAgent,
    last: Option<LoginProgress>,
    pressed_enter: bool,
}

impl LoginWatch {
    pub fn new(agent: CodingAgent) -> Self {
        Self { agent, last: None, pressed_enter: false }
    }

    /// The latest progress reported, if any.
    pub fn last(&self) -> Option<&LoginProgress> {
        self.last.as_ref()
    }

    pub fn observe(&mut self, lines: &[String]) -> LoginStep {
        let progress = match observe_login_screen(self.agent.id(), lines) {
            LoginObservation::Url { url, code } => LoginProgress::url(self.agent, url, code),
            LoginObservation::Failed(message) => LoginProgress::failed(self.agent, message),
            LoginObservation::MethodPicker if !self.pressed_enter => {
                self.pressed_enter = true;
                return LoginStep::PressEnter;
            }
            LoginObservation::MethodPicker | LoginObservation::Nothing => return LoginStep::Nothing,
        };
        if self.last.as_ref() == Some(&progress) {
            return LoginStep::Nothing;
        }
        self.last = Some(progress.clone());
        LoginStep::Report(progress)
    }
}

/// Raw PTY output → the lines a screen reader expects: escape sequences
/// (CSI, OSC — an OSC 8 hyperlink keeps its visible text — and two-byte
/// ESC codes) and other control characters dropped, `\r\n` a line break, and
/// a lone `\r` (a redrawn line) keeping only what was written after it.
pub fn screen_lines(raw: &str) -> Vec<String> {
    let mut text = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\u{1b}' => match chars.next() {
                // CSI: parameters up to a final byte in @..~.
                Some('[') => {
                    for next in chars.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                // OSC: up to BEL or ST (ESC \).
                Some(']') => {
                    while let Some(next) = chars.next() {
                        if next == '\u{7}' {
                            break;
                        }
                        if next == '\u{1b}' {
                            chars.next_if_eq(&'\\');
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\n' | '\r' | '\t' => text.push(ch),
            ch if ch.is_control() => {}
            ch => text.push(ch),
        }
    }
    text.split('\n')
        .map(|line| {
            // `\r\r\n` too: a PTY's onlcr turns a CLI's own `\r\n` into it.
            let line = line.trim_end_matches('\r');
            let line = line.rsplit('\r').next().unwrap_or(line);
            line.trim_end().to_string()
        })
        .collect()
}

/// ONE agent sign-in running in a PTY against the CLI's default home.
///
/// Poll it ([`Self::poll`], every ~250 ms) for the sign-in URL / device
/// code, hand claude's authorization code back with [`Self::enter_code`]
/// when the browser shows one, and read the exit with [`Self::try_wait`];
/// then [`is_signed_in`] says whether it worked. Dropping it kills a login
/// still running.
pub struct LocalLogin {
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    // Kept so the PTY stays open for the child's life.
    _master: Box<dyn MasterPty + Send>,
    output: Arc<Mutex<String>>,
    watch: LoginWatch,
    exited: Option<bool>,
}

impl LocalLogin {
    /// Start `agent`'s sign-in as `program`, resolved against `path_env`.
    /// `remote` = see [`login_command`].
    pub fn start(agent: CodingAgent, program: &str, path_env: &str, remote: bool) -> std::io::Result<Self> {
        Self::spawn(agent, &login_command(agent, program, remote), path_env)
    }

    /// Run an explicit [`LoginCommand`] (the plan of [`login_command`]).
    pub fn spawn(agent: CodingAgent, command: &LoginCommand, path_env: &str) -> std::io::Result<Self> {
        let pair = native_pty_system()
            .openpty(PtySize { rows: ROWS, cols: COLS, pixel_width: 0, pixel_height: 0 })
            .map_err(std::io::Error::other)?;
        let child = pair
            .slave
            .spawn_command(command_builder(command, path_env))
            .map_err(std::io::Error::other)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(std::io::Error::other)?;
        let writer = pair.master.take_writer().map_err(std::io::Error::other)?;
        let output = Arc::new(Mutex::new(String::new()));
        let sink = output.clone();
        std::thread::Builder::new().name("agent-login-read".into()).spawn(move || {
            let mut chunk = [0u8; 4096];
            let mut pending = Vec::new();
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        pending.extend_from_slice(&chunk[..n]);
                        // Keep a split multi-byte char for the next read.
                        let valid = match std::str::from_utf8(&pending) {
                            Ok(_) => pending.len(),
                            Err(err) if err.error_len().is_none() => err.valid_up_to(),
                            Err(_) => pending.len(),
                        };
                        let text = String::from_utf8_lossy(&pending[..valid]).into_owned();
                        pending.drain(..valid);
                        let Ok(mut out) = sink.lock() else { break };
                        out.push_str(&text);
                        if out.len() > OUTPUT_KEEP {
                            let mut cut = out.len() - OUTPUT_KEEP;
                            while !out.is_char_boundary(cut) {
                                cut += 1;
                            }
                            out.drain(..cut);
                        }
                    }
                }
            }
        })?;
        Ok(Self {
            child,
            writer,
            _master: pair.master,
            output,
            watch: LoginWatch::new(agent),
            exited: None,
        })
    }

    /// The output so far, as screen lines ([`screen_lines`]).
    pub fn lines(&self) -> Vec<String> {
        self.output.lock().map(|out| screen_lines(&out)).unwrap_or_default()
    }

    /// Read the output once: new progress to report, if any. Answers a
    /// login-method picker with `\r` on its own (`--claudeai` should never
    /// show one, but a release that does must not hang the login).
    pub fn poll(&mut self) -> Option<LoginProgress> {
        match self.watch.observe(&self.lines()) {
            LoginStep::Report(progress) => Some(progress),
            LoginStep::PressEnter => {
                let _ = self.write(b"\r");
                None
            }
            LoginStep::Nothing => None,
        }
    }

    /// The latest progress [`Self::poll`] reported.
    pub fn progress(&self) -> Option<&LoginProgress> {
        self.watch.last()
    }

    /// Type claude's authorization code ("Paste code here if prompted >").
    pub fn enter_code(&mut self, code: &str) -> std::io::Result<()> {
        self.write(format!("{}\r", code.trim()).as_bytes())
    }

    fn write(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.writer.write_all(bytes)?;
        self.writer.flush()
    }

    /// `Some(success)` once the CLI exited.
    pub fn try_wait(&mut self) -> std::io::Result<Option<bool>> {
        if self.exited.is_none() {
            self.exited = self.child.try_wait()?.map(|status| status.success());
        }
        Ok(self.exited)
    }

    /// Stop a login nobody finished.
    pub fn kill(&mut self) {
        if self.exited.is_none() {
            let _ = self.child.kill();
            self.exited = self.child.wait().ok().map(|status| status.success());
        }
    }

    /// Drive the login to its end: poll every `interval`, hand each new
    /// progress to `on_progress`, and give up (killing it) after `timeout`.
    /// `Ok(success)` is the CLI's exit; ask [`is_signed_in`] what it means.
    pub fn wait(
        &mut self,
        interval: Duration,
        timeout: Duration,
        mut on_progress: impl FnMut(&LoginProgress),
    ) -> std::io::Result<bool> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(progress) = self.poll() {
                on_progress(&progress);
            }
            if let Some(success) = self.try_wait()? {
                // The last screen may carry what the exit printed.
                if let Some(progress) = self.poll() {
                    on_progress(&progress);
                }
                return Ok(success);
            }
            if Instant::now() >= deadline {
                self.kill();
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "the sign-in timed out"));
            }
            std::thread::sleep(interval);
        }
    }
}

impl Drop for LocalLogin {
    fn drop(&mut self) {
        self.kill();
    }
}

/// The PTY command: argv, a terminal, the PATH, the plan's overlay. The
/// inherited environment is otherwise untouched and NO config-dir variable
/// is added, so the CLI signs in to its default home.
fn command_builder(command: &LoginCommand, path_env: &str) -> CommandBuilder {
    let mut cmd = CommandBuilder::new(&command.program);
    cmd.args(&command.args);
    cmd.env("TERM", "xterm-256color");
    cmd.env("PATH", path_env);
    for (key, value) in &command.env {
        cmd.env(key, value);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG_VARS: [&str; 2] = ["CLAUDE_CONFIG_DIR", "CODEX_HOME"];

    #[test]
    fn plans_spawn_each_agents_own_login() {
        let claude = login_command(CodingAgent::Claude, "/bin/claude", false);
        assert_eq!(claude.program, "/bin/claude");
        assert_eq!(claude.args, vec!["auth", "login", "--claudeai"]);
        assert_eq!(claude.title, "Sign in to Claude");
        let codex = login_command(CodingAgent::Codex, "codex", false);
        assert_eq!(codex.args, vec!["login", "--device-auth"]);
        assert_eq!(codex.title, "Sign in to Codex");
        assert_eq!(logout_args(CodingAgent::Claude), ["auth", "logout"]);
        assert_eq!(logout_args(CodingAgent::Codex), ["logout"]);
    }

    #[test]
    fn a_login_never_points_the_cli_at_a_staging_dir() {
        for agent in CodingAgent::ALL {
            for remote in [false, true] {
                let plan = login_command(agent, agent.default_binary(), remote);
                assert!(
                    plan.env.iter().all(|(key, _)| !CONFIG_VARS.contains(&key.as_str())),
                    "{agent}: {:?}",
                    plan.env
                );
                let expected: &[(String, String)] =
                    if remote { &[("BROWSER".into(), "true".into())] } else { &[] };
                assert_eq!(plan.env, expected, "{agent}");
                // The PTY command adds no config dir of its own either: it is
                // whatever this process already had (the CLI's default home).
                let cmd = command_builder(&plan, "/usr/bin");
                for var in CONFIG_VARS {
                    assert_eq!(cmd.get_env(var), std::env::var_os(var).as_deref(), "{var}");
                }
                assert_eq!(cmd.get_env("PATH"), Some(std::ffi::OsStr::new("/usr/bin")));
            }
        }
    }

    #[test]
    fn screen_lines_strip_escapes_and_redraws() {
        let raw = "\u{1b}[1mOpening\u{1b}[0m your browser…\r\n\
                   \u{1b}]8;;https://x\u{7}https://claude.ai/oauth/authorize?code=true&state=s\u{1b}]8;;\u{1b}\\\r\n\
                   spinner 1\rspinner 2\r\r\n\u{1b}[?25l\u{1b}[2K\r\n";
        assert_eq!(
            screen_lines(raw),
            vec![
                "Opening your browser…",
                "https://claude.ai/oauth/authorize?code=true&state=s",
                "spinner 2",
                "",
                "",
            ]
        );
    }

    #[test]
    fn a_claude_url_is_scraped_once() {
        let raw = "Opening your browser to sign in…\r\n\
                   If the browser didn't open, visit: https://claude.ai/oauth/authorize?code=true&state=xyz\r\n\r\n";
        let mut watch = LoginWatch::new(CodingAgent::Claude);
        let step = watch.observe(&screen_lines(raw));
        let LoginStep::Report(progress) = step else { panic!("{step:?}") };
        assert_eq!(progress.phase, LoginPhase::Url);
        assert_eq!(progress.agent, "claude");
        assert_eq!(
            progress.url.as_deref(),
            Some("https://claude.ai/oauth/authorize?code=true&state=xyz")
        );
        assert_eq!(progress.code, None);
        // The same screen again reports nothing new.
        assert_eq!(watch.observe(&screen_lines(raw)), LoginStep::Nothing);
        // An off-domain link is never published.
        let mut spoof = LoginWatch::new(CodingAgent::Claude);
        assert_eq!(
            spoof.observe(&screen_lines("visit: https://evil.test/oauth/authorize?code=true\r\n")),
            LoginStep::Nothing
        );
    }

    #[test]
    fn a_codex_device_code_is_scraped_with_its_url() {
        let raw = "\u{1b}[94mWelcome to Codex\u{1b}[0m\r\n\r\n  Open this URL in your browser:\r\n  \
                   \u{1b}[94mhttps://auth.openai.com/codex/device\u{1b}[0m\r\n  \
                   and enter the code: \u{1b}[94mWDJB-MJHT\u{1b}[0m\r\n";
        let mut watch = LoginWatch::new(CodingAgent::Codex);
        let LoginStep::Report(progress) = watch.observe(&screen_lines(raw)) else {
            panic!("no device code");
        };
        assert_eq!(progress.url.as_deref(), Some("https://auth.openai.com/codex/device"));
        assert_eq!(progress.code.as_deref(), Some("WDJB-MJHT"));
        assert_eq!(watch.last(), Some(&progress));

        let expired = format!("{raw}  The device code expired.\r\n");
        let LoginStep::Report(failed) = watch.observe(&screen_lines(&expired)) else {
            panic!("no failure");
        };
        assert_eq!(failed.phase, LoginPhase::Failed);
        assert_eq!(failed.message.as_deref(), Some("The device code expired."));
    }

    #[test]
    fn a_method_picker_is_answered_once() {
        let picker = screen_lines(
            "  Select login method:\r\n\r\n  ❯ 1. Claude account with subscription\r\n    2. Anthropic Console account\r\n",
        );
        let mut watch = LoginWatch::new(CodingAgent::Claude);
        assert_eq!(watch.observe(&picker), LoginStep::PressEnter);
        assert_eq!(watch.observe(&picker), LoginStep::Nothing);
    }

    /// The whole driver over a real PTY, with a stub CLI that prints codex's
    /// device-auth screen and exits.
    #[cfg(unix)]
    #[test]
    fn a_local_login_reports_the_device_code_from_a_pty() {
        let command = LoginCommand {
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                "printf '  Open this URL in your browser:\\r\\n  https://auth.openai.com/codex/device\\r\\n  and enter the code: ABCD-EFGH\\r\\n'; sleep 0.3".into(),
            ],
            env: Vec::new(),
            title: "Sign in to Codex".into(),
        };
        let mut login = LocalLogin::spawn(CodingAgent::Codex, &command, "/bin:/usr/bin").unwrap();
        let mut seen = Vec::new();
        let success = login
            .wait(Duration::from_millis(20), Duration::from_secs(10), |progress| {
                seen.push(progress.clone())
            })
            .unwrap();
        assert!(success);
        assert_eq!(
            seen,
            vec![LoginProgress::url(
                CodingAgent::Codex,
                "https://auth.openai.com/codex/device",
                Some("ABCD-EFGH".into())
            )]
        );
    }
}
