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
//! [`DoctorReport::installed_agents`] is the steer presence input (EXP-201):
//! the device advertises which agent CLIs it can actually run, so remote
//! Start-coding pickers only offer those.
//!
//! Blocking `std::process` calls — callers run this off the foreground
//! executor (settings "Check tools" button, onboarding, launch step 0).

use crate::agent::CodingAgent;
use crate::settings::Settings;
use std::fmt;
use std::process::Command;

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
            Tool::Claude => "claude not found on PATH — set an absolute path",
            Tool::Codex => "codex not found on PATH — set an absolute path",
            Tool::Pi => "pi not found on PATH — set an absolute path",
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
    /// would carry flags the CLI rejects), so it drops out here too.
    pub fn installed_agents(&self) -> Vec<CodingAgent> {
        CodingAgent::ALL
            .into_iter()
            .filter(|agent| self.check_for(*agent).ok)
            .collect()
    }
}

/// Run every check: each agent's resolved program
/// ([`Settings::resolved_path_for`]) — claude version-gated against
/// [`MIN_CLAUDE_VERSION`] — and plain `git` from PATH.
pub fn run_doctor(settings: &Settings) -> DoctorReport {
    let mut claude = check_tool(Tool::Claude, &settings.resolved_path_for(CodingAgent::Claude));
    apply_version_gate(&mut claude);
    DoctorReport {
        claude,
        codex: check_tool(Tool::Codex, &settings.resolved_path_for(CodingAgent::Codex)),
        pi: check_tool(Tool::Pi, &settings.resolved_path_for(CodingAgent::Pi)),
        git: check_tool(Tool::Git, "git"),
    }
}

/// The check for ONE agent (launch step 0 re-checks only the selected agent
/// + git via [`run_doctor`]'s full report; presence probes use the full one).
pub fn check_agent(settings: &Settings, agent: CodingAgent) -> ToolCheck {
    let mut check = check_tool(Tool::for_agent(agent), &settings.resolved_path_for(agent));
    if agent == CodingAgent::Claude {
        apply_version_gate(&mut check);
    }
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
            "Claude Code {major}.{minor}.{patch} is too old — update to \
{min_major}.{min_minor}.{min_patch}+ (run: claude update)"
        ));
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
        }
    }

    fn red(tool: Tool) -> ToolCheck {
        ToolCheck {
            tool,
            ok: false,
            version: None,
            error: Some(tool.not_found_message().to_string()),
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
            Some("Claude Code 2.1.199 is too old — update to 2.1.215+ (run: claude update)")
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
            Some("claude not found on PATH — set an absolute path")
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
            Some("Claude Code 2.1.100 is too old — update to 2.1.215+ (run: claude update)")
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
        let check = check_tool(Tool::Pi, "definitely-not-a-real-binary-exp");
        assert_eq!(
            check.error.as_deref(),
            Some("pi not found on PATH — set an absolute path")
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
}
