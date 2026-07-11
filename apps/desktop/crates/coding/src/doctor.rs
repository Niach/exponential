//! The tooling doctor (masterplan-v3 §7.7): runs `<agent> --version`
//! and `git --version` (the agent binary comes from the active
//! [`crate::agent::Agent`] — the configured Claude path by default, the
//! codex path under the experimental codex opt-in) and captures success +
//! version string or the spawn error.
//!
//! The doctor **blocks Start coding when EITHER tool is missing** — the
//! launcher's enabled state ANDs `agent.ok && git.ok` (§7.1 step 1), which
//! prevents the "falsely proceed then crash at git clone" pattern.
//! Errors are actionable per the spec copy: "claude not found on PATH — set
//! an absolute path" / "git not found on PATH".
//!
//! Blocking `std::process` calls — callers run this off the foreground
//! executor (settings "Check tools" button, onboarding, launch step 0).

use crate::agent::Agent;
use crate::settings::Settings;
use std::fmt;
use std::process::Command;

/// The local binaries the launcher ever shells out to (§7.1 step 3:
/// argv `git` + the coding agent, never `gh`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Claude,
    /// EXPERIMENTAL — the OpenAI Codex CLI (v5 "codex-support" opt-in).
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

    /// The §7.7 red actionable message for a missing binary.
    fn not_found_message(self) -> &'static str {
        match self {
            Tool::Claude => "claude not found on PATH — set an absolute path",
            Tool::Codex => "codex not found on PATH — set an absolute path",
            Tool::Git => "git not found on PATH",
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
}

/// Which EXP-56 launch flags the installed Claude CLI advertises
/// (`claude --help` grep). Old CLIs simply lose the corresponding argv piece
/// — the launch itself never hard-fails on a missing flag. All-false for
/// codex and whenever the probe can't run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClaudeFlagSupport {
    /// `--effort <level>` (reasoning effort).
    pub effort: bool,
    /// `--agents <json>` (session-scoped subagent definitions).
    pub agents: bool,
    /// `--settings <json>` (carries the ultracode/dynamic-workflows toggle).
    pub settings: bool,
}

/// `{ agent, git, claude_flags }` — the §7.7 report. `agent` is whichever
/// coding agent is active per the settings (claude by default);
/// `claude_flags` is probed only when that agent is Claude.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoctorReport {
    pub agent: ToolCheck,
    pub git: ToolCheck,
    pub claude_flags: ClaudeFlagSupport,
}

impl DoctorReport {
    /// The Start-coding gate (§7.1 step 1): BOTH tools must resolve.
    pub fn ok(&self) -> bool {
        self.agent.ok && self.git.ok
    }

    /// The first failing check, for `DisabledReason::DoctorFailed` (names
    /// which tool failed).
    pub fn first_failure(&self) -> Option<&ToolCheck> {
        [&self.agent, &self.git]
            .into_iter()
            .find(|check| !check.ok)
    }
}

/// Run both checks. The agent binary comes from the active
/// [`Agent`] (claude's configured/probed path by default, the codex path
/// under the experimental opt-in — checking the binary that will actually be
/// spawned, never falsely blocking on the other one); `git` is always plain
/// `git` from PATH.
pub fn run_doctor(settings: &Settings) -> DoctorReport {
    run_doctor_for(Agent::from_settings(settings), settings)
}

/// [`run_doctor`] for an EXPLICIT agent — the release orchestrator (EXP-56)
/// is Claude-only regardless of the `codingAgent` setting, so it doctors
/// Claude directly instead of whatever the single-issue setting says.
pub fn run_doctor_for(agent: Agent, settings: &Settings) -> DoctorReport {
    let program = agent.program(settings);
    let check = check_tool(agent.tool(), &program);
    // Probe launch-flag support only for a WORKING Claude binary — a dead
    // binary would just add a second slow spawn failure for no signal.
    let claude_flags = if agent == Agent::Claude && check.ok {
        probe_claude_flags(&program)
    } else {
        ClaudeFlagSupport::default()
    };
    DoctorReport {
        agent: check,
        git: check_tool(Tool::Git, "git"),
        claude_flags,
    }
}

/// `<program> --help` grepped for the EXP-56 launch flags. Any spawn/decode
/// failure degrades to all-false (the flags are omitted, never a hard fail).
pub fn probe_claude_flags(program: &str) -> ClaudeFlagSupport {
    let Ok(output) = Command::new(program).arg("--help").output() else {
        return ClaudeFlagSupport::default();
    };
    if !output.status.success() {
        return ClaudeFlagSupport::default();
    }
    let help = String::from_utf8_lossy(&output.stdout);
    ClaudeFlagSupport {
        effort: help.contains("--effort"),
        agents: help.contains("--agents"),
        settings: help.contains("--settings"),
    }
}

/// `<program> --version`, capturing stdout/stderr — never a shell.
pub fn check_tool(tool: Tool, program: &str) -> ToolCheck {
    match Command::new(program).arg("--version").output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match parse_version_output(tool, &stdout) {
                Some(version) => ToolCheck { tool, ok: true, version: Some(version), error: None },
                None => ToolCheck {
                    tool,
                    ok: false,
                    version: None,
                    error: Some(format!("{program} --version produced no output")),
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
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => ToolCheck {
            tool,
            ok: false,
            version: None,
            error: Some(tool.not_found_message().to_string()),
        },
        Err(err) => ToolCheck {
            tool,
            ok: false,
            version: None,
            error: Some(format!("could not run {program}: {err}")),
        },
    }
}

/// First non-empty line of `--version` output, with the tool's own noise
/// prefix stripped (`git version 2.39.5 …` → `2.39.5 …`; claude's
/// `1.0.35 (Claude Code)` and codex's version line pass through).
pub fn parse_version_output(tool: Tool, stdout: &str) -> Option<String> {
    let line = first_line(stdout)?;
    let stripped = match tool {
        Tool::Git => line.strip_prefix("git version ").unwrap_or(line),
        Tool::Claude | Tool::Codex => line,
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
    fn parses_claude_version_line() {
        assert_eq!(
            parse_version_output(Tool::Claude, "1.0.35 (Claude Code)\n"),
            Some("1.0.35 (Claude Code)".to_string())
        );
        // Tolerates warning noise before the version line.
        assert_eq!(
            parse_version_output(Tool::Claude, "\n  2.1.0 (Claude Code)\n"),
            Some("2.1.0 (Claude Code)".to_string())
        );
    }

    #[test]
    fn empty_output_parses_to_none() {
        assert_eq!(parse_version_output(Tool::Claude, ""), None);
        assert_eq!(parse_version_output(Tool::Git, "   \n \n"), None);
    }

    #[test]
    fn missing_binary_yields_the_actionable_spec_message() {
        let check = check_tool(Tool::Claude, "definitely-not-a-real-binary-exp");
        assert!(!check.ok);
        assert_eq!(
            check.error.as_deref(),
            Some("claude not found on PATH — set an absolute path")
        );
        let check = check_tool(Tool::Codex, "definitely-not-a-real-binary-exp");
        assert_eq!(
            check.error.as_deref(),
            Some("codex not found on PATH — set an absolute path")
        );
        let check = check_tool(Tool::Git, "definitely-not-a-real-binary-exp");
        assert_eq!(check.error.as_deref(), Some("git not found on PATH"));
    }

    /// The doctor checks the binary that will actually be spawned: under the
    /// experimental codex opt-in the agent row is `codex` at the configured
    /// codex path — a missing `claude` must never block a codex launch (and
    /// vice versa: a missing codex binary blocks it with the codex copy).
    #[test]
    fn codex_opt_in_doctors_the_codex_binary() {
        let mut settings = crate::settings::Settings {
            claude_path: "definitely-not-a-real-binary-exp".to_string(),
            coding_agent: "codex".to_string(),
            // A real binary answering `--version` (same trick as the
            // launcher tests): the codex slot goes green while claude is dead.
            codex_path: "git".to_string(),
            ..crate::settings::Settings::default()
        };
        let report = run_doctor(&settings);
        assert_eq!(report.agent.tool, Tool::Codex);
        assert!(report.agent.ok, "codex check: {:?}", report.agent.error);
        assert!(report.ok(), "claude's absence must not gate a codex launch");

        settings.codex_path = "definitely-not-a-real-binary-exp".to_string();
        let report = run_doctor(&settings);
        assert!(!report.ok());
        assert_eq!(
            report.first_failure().and_then(|c| c.error.as_deref()),
            Some("codex not found on PATH — set an absolute path")
        );
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

    #[test]
    fn report_gate_requires_both_tools() {
        let good = ToolCheck { tool: Tool::Git, ok: true, version: Some("2.45.0".into()), error: None };
        let bad = ToolCheck {
            tool: Tool::Claude,
            ok: false,
            version: None,
            error: Some("claude not found on PATH — set an absolute path".into()),
        };
        let flags = ClaudeFlagSupport::default();
        let report = DoctorReport { agent: bad.clone(), git: good.clone(), claude_flags: flags };
        assert!(!report.ok());
        assert_eq!(report.first_failure(), Some(&bad));

        let report =
            DoctorReport { agent: good.clone(), git: good.clone(), claude_flags: flags };
        assert!(report.ok());
        assert_eq!(report.first_failure(), None);

        // git missing must ALSO block (§7.1 step 1 ANDs both).
        let bad_git = ToolCheck { tool: Tool::Git, ok: false, version: None, error: None };
        let report = DoctorReport { agent: good, git: bad_git.clone(), claude_flags: flags };
        assert!(!report.ok());
        assert_eq!(report.first_failure(), Some(&bad_git));
    }

    /// The EXP-56 flag probe: greps `--help` output; degrades to all-false on
    /// a missing binary; never probed for codex.
    #[test]
    fn flag_probe_greps_help_and_degrades_gracefully() {
        // A missing binary → all-false, no panic.
        assert_eq!(
            probe_claude_flags("definitely-not-a-real-binary-exp"),
            ClaudeFlagSupport::default()
        );
        // `git --help` mentions none of the claude flags → all-false, proving
        // the grep is specific (and giving the probe a real-binary run).
        assert_eq!(probe_claude_flags("git"), ClaudeFlagSupport::default());
        // The codex agent never probes: run_doctor_for leaves flags default
        // even with a green binary.
        let settings = crate::settings::Settings {
            coding_agent: "codex".to_string(),
            codex_path: "git".to_string(),
            ..crate::settings::Settings::default()
        };
        let report = run_doctor(&settings);
        assert!(report.agent.ok);
        assert_eq!(report.claude_flags, ClaudeFlagSupport::default());
    }
}
