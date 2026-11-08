//! EXP-1513: install a missing agent CLI on demand with the VENDOR'S OWN
//! installer, exactly what the person would type from the vendor's docs:
//!
//! - **Claude Code:** Anthropic's native installer,
//!   `curl -fsSL https://claude.ai/install.sh | bash` (macOS, Linux) or
//!   `irm https://claude.ai/install.ps1 | iex` (Windows). It drops the
//!   binary at `~/.local/bin/claude` and keeps it updated itself.
//! - **Codex:** OpenAI's npm package, `npm install -g @openai/codex`, offered
//!   only where `npm` runs. (Exponential's own hosts fetch a pinned Codex
//!   instead, `coding::managed_codex`, and never call this for it.)
//!
//! Nothing is re-hosted, patched or pinned here: the installer is fetched
//! from the vendor at run time. The outcome is judged the doctor's way, by
//! `--version` afterwards, never by parsing the installer's chatter.
//!
//! **Credentials are never touched.** An install leaves the CLI signed out;
//! the CLI signs itself in through its own login ([`crate::login`]).

use std::path::PathBuf;
use std::time::Duration;

use crate::agent::CodingAgent;
use crate::detect::{check_tool, output_with_timeout, Tool};
use crate::process::background_command;

/// How long an installer may run: a download plus an unpack on a slow link.
/// Past it the child is killed and the install fails with the deadline named.
pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Anthropic's installer for macOS and Linux.
pub const CLAUDE_INSTALL_SH: &str = "https://claude.ai/install.sh";
/// Anthropic's installer for Windows.
pub const CLAUDE_INSTALL_PS1: &str = "https://claude.ai/install.ps1";
/// OpenAI's npm package for the Codex CLI.
pub const CODEX_NPM_PACKAGE: &str = "@openai/codex";

/// One vendor installer: the argv this host runs, and the line a person
/// would type for the same install (shown in copy and logs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installer {
    pub agent: CodingAgent,
    pub program: String,
    pub args: Vec<String>,
    pub command_line: String,
}

/// The vendor installer for `agent` on this OS. `None` when there is none
/// to run here (Codex without `npm` on `path_env`).
pub fn installer(agent: CodingAgent, path_env: &str) -> Option<Installer> {
    installer_for(agent, cfg!(windows), || npm_available(path_env))
}

/// [`installer`] with the OS and the npm probe injected (pure, testable).
pub fn installer_for(agent: CodingAgent, windows: bool, npm: impl FnOnce() -> bool) -> Option<Installer> {
    let (program, args, command_line) = match (agent, windows) {
        (CodingAgent::Claude, false) => {
            let line = format!("curl -fsSL {CLAUDE_INSTALL_SH} | bash");
            ("bash".to_string(), vec!["-c".to_string(), format!("set -o pipefail; {line}")], line)
        }
        (CodingAgent::Claude, true) => {
            let line = format!("irm {CLAUDE_INSTALL_PS1} | iex");
            let args = ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", line.as_str()];
            ("powershell".to_string(), args.iter().map(|a| a.to_string()).collect(), line)
        }
        (CodingAgent::Codex, windows) => {
            if !npm() {
                return None;
            }
            let program = if windows { "npm.cmd" } else { "npm" };
            let args = vec!["install".to_string(), "-g".to_string(), CODEX_NPM_PACKAGE.to_string()];
            (program.to_string(), args, format!("npm install -g {CODEX_NPM_PACKAGE}"))
        }
    };
    Some(Installer { agent, program, args, command_line })
}

fn npm_available(path_env: &str) -> bool {
    let mut cmd = background_command(if cfg!(windows) { "npm.cmd" } else { "npm" });
    cmd.env("PATH", path_env).arg("--version");
    output_with_timeout(cmd, crate::detect::PROBE_TIMEOUT).is_ok_and(|out| out.status.success())
}

/// Where the vendor installer puts the binary when it is not on PATH yet
/// (Claude's native installer: `~/.local/bin/claude`). A host whose PATH
/// was resolved before the install probes this first. `None` for Codex
/// (npm's global bin is on PATH or it is not).
pub fn installed_program(agent: CodingAgent) -> Option<PathBuf> {
    installed_program_in(agent, dirs::home_dir()?, cfg!(windows))
}

/// [`installed_program`] under an explicit home (pure, testable).
pub fn installed_program_in(agent: CodingAgent, home: PathBuf, windows: bool) -> Option<PathBuf> {
    match agent {
        CodingAgent::Claude => {
            let name = if windows { "claude.exe" } else { "claude" };
            Some(home.join(".local").join("bin").join(name))
        }
        CodingAgent::Codex => None,
    }
}

/// The program to spawn for `agent`: the bare name when it answers on
/// `path_env`, else the installer's location when that answers.
pub fn resolve_program(agent: CodingAgent, path_env: &str) -> String {
    let bare = agent.default_binary();
    if check_tool(Tool::for_agent(agent), bare, path_env).ok {
        return bare.to_string();
    }
    installed_program(agent)
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| bare.to_string())
}

/// What a finished install left behind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub agent: CodingAgent,
    /// The program that answered `--version` (bare name or absolute path).
    pub program: String,
    pub version: String,
}

impl Installed {
    /// The completion line a host shows (a toast, a chat line).
    pub fn message(&self) -> String {
        format!("{} installed (v{}).", self.agent.label(), self.version)
    }
}

/// Run `agent`'s vendor installer and confirm the CLI answers `--version`.
/// Blocking for up to [`INSTALL_TIMEOUT`]: callers run it off the UI thread.
/// Signs nothing in.
pub fn install(agent: CodingAgent, path_env: &str) -> Result<Installed, String> {
    let label = agent.label();
    let Some(installer) = installer(agent, path_env) else {
        return Err(format!("{label} needs npm to install. Install Node.js, then try again."));
    };
    let mut cmd = background_command(&installer.program);
    cmd.env("PATH", path_env).args(&installer.args);
    let output = output_with_timeout(cmd, INSTALL_TIMEOUT).map_err(|err| {
        if err.kind() == std::io::ErrorKind::TimedOut {
            format!("{label} install did not finish within {} minutes.", INSTALL_TIMEOUT.as_secs() / 60)
        } else {
            format!("Could not run `{}`: {err}", installer.command_line)
        }
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = last_line(&stderr)
            .or_else(|| last_line(&stdout))
            .map(str::to_string)
            .unwrap_or_else(|| format!("exit code {}", output.status.code().unwrap_or(-1)));
        return Err(format!("{label} install failed: {detail}"));
    }
    let program = resolve_program(agent, path_env);
    let check = check_tool(Tool::for_agent(agent), &program, path_env);
    match check.version {
        Some(version) if check.ok => Ok(Installed { agent, program, version }),
        _ => Err(check
            .error
            .unwrap_or_else(|| format!("{label} install finished but the CLI does not answer --version."))),
    }
}

/// The last non-empty line: installers print the reason last.
fn last_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).filter(|line| !line.is_empty()).next_back()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_runs_anthropics_installer_per_os() {
        let unix = installer_for(CodingAgent::Claude, false, || unreachable!()).unwrap();
        assert_eq!(unix.command_line, "curl -fsSL https://claude.ai/install.sh | bash");
        assert_eq!(unix.program, "bash");
        assert_eq!(unix.args, ["-c", "set -o pipefail; curl -fsSL https://claude.ai/install.sh | bash"]);
        let windows = installer_for(CodingAgent::Claude, true, || unreachable!()).unwrap();
        assert_eq!(windows.command_line, "irm https://claude.ai/install.ps1 | iex");
        assert_eq!(windows.program, "powershell");
        assert_eq!(windows.args.last().map(String::as_str), Some("irm https://claude.ai/install.ps1 | iex"));
    }

    #[test]
    fn codex_is_npm_only_where_npm_runs() {
        assert_eq!(installer_for(CodingAgent::Codex, false, || false), None);
        let npm = installer_for(CodingAgent::Codex, false, || true).unwrap();
        assert_eq!((npm.program.as_str(), npm.command_line.as_str()), ("npm", "npm install -g @openai/codex"));
        assert_eq!(installer_for(CodingAgent::Codex, true, || true).unwrap().program, "npm.cmd");
    }

    #[test]
    fn the_native_install_lands_in_local_bin() {
        let home = PathBuf::from("/home/me");
        assert_eq!(installed_program_in(CodingAgent::Claude, home.clone(), false), Some(home.join(".local/bin/claude")));
        assert_eq!(installed_program_in(CodingAgent::Claude, home.clone(), true), Some(home.join(".local").join("bin").join("claude.exe")));
        assert_eq!(installed_program_in(CodingAgent::Codex, home, false), None);
    }

    #[test]
    fn last_line_skips_trailing_blanks() {
        assert_eq!(last_line("Downloading…\nerror: no network\n\n"), Some("error: no network"));
        assert_eq!(last_line(" \n"), None);
    }
}
