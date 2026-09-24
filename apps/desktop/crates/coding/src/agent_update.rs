//! The `agent_update` device command: run an agent CLI's OWN self-updater on
//! this machine (`claude update` / `codex update`), remotely, from the device
//! settings dialog — the agent-side twin of the daemon's FEED-36 `update_now`.
//!
//! Nothing here knows how the CLI is installed (npm, brew, the native
//! installer): the CLI's updater does, and it is what the person would type
//! at the machine. The outcome is judged by re-reading `--version` before
//! and after rather than by parsing the updater's chatter, so a "nothing to
//! do" run and a real bump read differently without a per-CLI grammar. Live
//! sessions keep running: the binary they hold stays mapped, and the alias
//! (`opus`, `sonnet`, …) they were started with resolves on the NEXT start.

use std::time::Duration;

use crate::agent::CodingAgent;
use crate::doctor::{check_tool, output_with_timeout, Tool};
use crate::settings::Settings;

/// The device-command kind, byte-identical with the server's `createCommand`
/// enum member and the web/desktop queue paths.
pub const AGENT_UPDATE_COMMAND: &str = "agent_update";

/// How long an updater may run: a download plus an install on a slow link.
/// Past it the child is killed (process group) and the command fails with
/// the deadline named, never a row pending forever.
pub const UPDATE_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// What one run of the updater changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentUpdateOutcome {
    pub agent: CodingAgent,
    /// The doctor's version line before the run (`None` = the CLI did not
    /// answer `--version`, e.g. a half-installed tool).
    pub before: Option<String>,
    /// … and after it. Always `Some` on `Ok` (an updater that leaves no
    /// runnable CLI behind is an error, not an outcome).
    pub after: String,
}

impl AgentUpdateOutcome {
    /// Whether the run moved the installed version.
    pub fn changed(&self) -> bool {
        self.before.as_deref() != Some(self.after.as_str())
    }

    /// The completion message the requester reads under the row.
    pub fn message(&self) -> String {
        describe(self.agent, self.before.as_deref(), &self.after)
    }
}

/// The completion copy for a finished run, factored out so the shape is
/// testable without spawning anything.
pub fn describe(agent: CodingAgent, before: Option<&str>, after: &str) -> String {
    match before {
        Some(before) if before == after => {
            format!("{} is already up to date (v{after}).", agent.label())
        }
        Some(before) => format!("{} updated: v{before} → v{after}.", agent.label()),
        None => format!("{} updated to v{after}.", agent.label()),
    }
}

fn tool_for(agent: CodingAgent) -> Tool {
    match agent {
        CodingAgent::Claude => Tool::Claude,
        CodingAgent::Codex => Tool::Codex,
    }
}

/// Run `agent`'s self-updater and report what it did. Blocking for up to
/// [`UPDATE_TIMEOUT`]: callers run it off the heartbeat thread.
///
/// The program is the SAME resolved path every launch and doctor probe uses
/// ([`Settings::resolved_path_for`]) under the augmented login PATH, so the
/// CLI that gets updated is the one runs actually start. A CLI the doctor
/// cannot run at all is refused up front with the doctor's own reason (an
/// updater cannot install a tool that is not there).
pub fn update_agent(settings: &Settings, agent: CodingAgent) -> Result<AgentUpdateOutcome, String> {
    let program = settings.resolved_path_for(agent);
    let path_env = terminal::pty::login_path();
    let tool = tool_for(agent);
    let before = check_tool(tool, &program);
    if before.version.is_none() {
        return Err(before
            .error
            .clone()
            .unwrap_or_else(|| format!("{} is not installed on this machine.", agent.label())));
    }
    let mut cmd = terminal::process::background_command(&program);
    cmd.env("PATH", &path_env).arg("update");
    // The updaters detect a terminal to draw progress; a plain pipe gets the
    // line-mode output, which is what lands in the log on failure.
    let output = output_with_timeout(cmd, UPDATE_TIMEOUT).map_err(|err| {
        if err.kind() == std::io::ErrorKind::TimedOut {
            format!(
                "{} update did not finish within {} minutes.",
                agent.label(),
                UPDATE_TIMEOUT.as_secs() / 60
            )
        } else {
            format!("Could not run `{program} update`: {err}")
        }
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = last_line(&stderr)
            .or_else(|| last_line(&stdout))
            .map(str::to_string)
            .unwrap_or_else(|| format!("exit code {}", output.status.code().unwrap_or(-1)));
        return Err(format!("{} update failed: {detail}", agent.label()));
    }
    let after = check_tool(tool, &program);
    match after.version {
        Some(version) => Ok(AgentUpdateOutcome {
            agent,
            before: before.version,
            after: version,
        }),
        None => Err(after.error.unwrap_or_else(|| {
            format!("{} update finished but the CLI no longer answers --version.", agent.label())
        })),
    }
}

/// The last non-empty line — updaters print the reason last, after their
/// progress chatter.
fn last_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).filter(|line| !line.is_empty()).next_back()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_completion_copy_names_the_move() {
        assert_eq!(
            describe(CodingAgent::Claude, Some("2.1.272"), "2.1.281"),
            "Claude Code updated: v2.1.272 → v2.1.281."
        );
        assert_eq!(
            describe(CodingAgent::Codex, Some("0.144.5"), "0.144.5"),
            "Codex is already up to date (v0.144.5)."
        );
        assert_eq!(describe(CodingAgent::Claude, None, "2.1.281"), "Claude Code updated to v2.1.281.");
    }

    #[test]
    fn changed_compares_the_version_lines() {
        let same = AgentUpdateOutcome {
            agent: CodingAgent::Claude,
            before: Some("2.1.281".into()),
            after: "2.1.281".into(),
        };
        assert!(!same.changed());
        let moved = AgentUpdateOutcome { before: Some("2.1.272".into()), ..same.clone() };
        assert!(moved.changed());
        let fresh = AgentUpdateOutcome { before: None, ..same };
        assert!(fresh.changed());
    }

    #[test]
    fn last_line_skips_trailing_blanks() {
        assert_eq!(last_line("downloading…\nerror: no network\n\n"), Some("error: no network"));
        assert_eq!(last_line("  \n"), None);
    }

    /// A CLI that is not there is refused before anything is spawned — the
    /// doctor's reason is the message.
    #[test]
    fn a_missing_cli_is_refused_up_front() {
        let mut settings = Settings::default();
        settings.claude_path = "/nonexistent/exp-agent-update-test/claude".to_string();
        let err = update_agent(&settings, CodingAgent::Claude).unwrap_err();
        assert!(err.contains("claude"), "{err}");
    }
}
