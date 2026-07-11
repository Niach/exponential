//! The tooling doctor (masterplan-v3 §7.7): runs `claude --version` (the
//! configured/probed Claude path) and `git --version` and captures success +
//! version string or the spawn error.
//!
//! The doctor **blocks Start coding when EITHER tool is missing** — the
//! launcher's enabled state ANDs `agent.ok && git.ok` (§7.1 step 1), which
//! prevents the "falsely proceed then crash at git clone" pattern.
//! Errors are actionable per the spec copy: "claude not found on PATH — set
//! an absolute path" / "git not found on PATH".
//!
//! A resolvable Claude that is OLDER than [`MIN_CLAUDE_VERSION`] also fails
//! the doctor (with "run: claude update" copy) — one version gate replaces
//! the old per-flag `--help` probe and its whole degradation matrix.
//!
//! Blocking `std::process` calls — callers run this off the foreground
//! executor (settings "Check tools" button, onboarding, launch step 0).

use crate::settings::Settings;
use std::fmt;
use std::process::Command;

/// The minimum supported Claude Code version: `--effort ultracode` landed in
/// 2.1.203, `--permission-mode plan`/`manual` in 2.1.200 — everything the
/// launcher's argv relies on.
pub const MIN_CLAUDE_VERSION: (u32, u32, u32) = (2, 1, 203);

/// The local binaries the launcher ever shells out to (§7.1 step 3:
/// argv `git` + `claude`, never `gh`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Claude,
    Git,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::Git => "git",
        }
    }

    /// The §7.7 red actionable message for a missing binary.
    fn not_found_message(self) -> &'static str {
        match self {
            Tool::Claude => "claude not found on PATH — set an absolute path",
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

/// `{ agent, git }` — the §7.7 report. `agent` is the Claude check (the
/// field name predates the codex deletion; ui renders it as the claude row).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoctorReport {
    pub agent: ToolCheck,
    pub git: ToolCheck,
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

/// Run both checks: the resolved Claude program
/// ([`Settings::resolved_claude_path`]) — version-gated against
/// [`MIN_CLAUDE_VERSION`] — and plain `git` from PATH.
pub fn run_doctor(settings: &Settings) -> DoctorReport {
    let mut claude = check_tool(Tool::Claude, &settings.resolved_claude_path());
    apply_version_gate(&mut claude);
    DoctorReport {
        agent: claude,
        git: check_tool(Tool::Git, "git"),
    }
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
/// prefix stripped (`git version 2.39.5 …` → `2.39.5 …`; claude's
/// `1.0.35 (Claude Code)` passes through).
pub fn parse_version_output(tool: Tool, stdout: &str) -> Option<String> {
    let line = first_line(stdout)?;
    let stripped = match tool {
        Tool::Git => line.strip_prefix("git version ").unwrap_or(line),
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
    fn claude_version_triples_parse_and_junk_does_not() {
        assert_eq!(parse_claude_version("2.1.207 (Claude Code)"), Some((2, 1, 207)));
        assert_eq!(parse_claude_version("  9.9.9 (Claude Code stub)"), Some((9, 9, 9)));
        assert_eq!(parse_claude_version("2.1.203"), Some((2, 1, 203)));
        // Not a plain three-part leading version → None (gate stays open).
        assert_eq!(parse_claude_version("git version 2.39.5"), None);
        assert_eq!(parse_claude_version("2.1"), None);
        assert_eq!(parse_claude_version("2.1.203.7"), None);
        assert_eq!(parse_claude_version("v2.1.203"), None);
        assert_eq!(parse_claude_version(""), None);
    }

    fn green_claude(version: &str) -> ToolCheck {
        ToolCheck {
            tool: Tool::Claude,
            ok: true,
            version: Some(version.to_string()),
            error: None,
        }
    }

    /// The version gate: below-minimum flips red with the actionable
    /// "claude update" copy; at/above minimum and unparseable stay green.
    #[test]
    fn version_gate_blocks_old_clis_with_update_copy() {
        let mut old = green_claude("2.1.199 (Claude Code)");
        apply_version_gate(&mut old);
        assert!(!old.ok);
        assert_eq!(
            old.error.as_deref(),
            Some("Claude Code 2.1.199 is too old — update to 2.1.203+ (run: claude update)")
        );

        // Exactly the minimum and newer stay green.
        for version in ["2.1.203 (Claude Code)", "2.1.207 (Claude Code)", "3.0.0"] {
            let mut check = green_claude(version);
            apply_version_gate(&mut check);
            assert!(check.ok, "{version} must pass the gate");
            assert_eq!(check.error, None);
        }

        // Unparseable version → green (never falsely block a nonstandard
        // build).
        let mut odd = green_claude("nightly (Claude Code)");
        apply_version_gate(&mut odd);
        assert!(odd.ok);

        // A check that already failed is left alone (keeps its own error).
        let mut dead = ToolCheck {
            tool: Tool::Claude,
            ok: false,
            version: None,
            error: Some("claude not found on PATH — set an absolute path".into()),
        };
        apply_version_gate(&mut dead);
        assert_eq!(
            dead.error.as_deref(),
            Some("claude not found on PATH — set an absolute path")
        );
    }

    /// `run_doctor` end-to-end against stub claude binaries: an old version
    /// fails the report with the update copy; a new one passes.
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

        let old = write_stub("claude-old", "2.1.100");
        let settings = Settings {
            claude_path: old.to_string_lossy().into_owned(),
            ..Settings::default()
        };
        let report = run_doctor(&settings);
        assert!(!report.ok());
        assert_eq!(
            report.first_failure().and_then(|c| c.error.as_deref()),
            Some("Claude Code 2.1.100 is too old — update to 2.1.203+ (run: claude update)")
        );

        let new = write_stub("claude-new", "2.1.207");
        let settings = Settings {
            claude_path: new.to_string_lossy().into_owned(),
            ..Settings::default()
        };
        let report = run_doctor(&settings);
        assert!(report.ok(), "2.1.207 must pass: {:?}", report.first_failure());

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

    #[test]
    fn report_gate_requires_both_tools() {
        let good = ToolCheck { tool: Tool::Git, ok: true, version: Some("2.45.0".into()), error: None };
        let bad = ToolCheck {
            tool: Tool::Claude,
            ok: false,
            version: None,
            error: Some("claude not found on PATH — set an absolute path".into()),
        };
        let report = DoctorReport { agent: bad.clone(), git: good.clone() };
        assert!(!report.ok());
        assert_eq!(report.first_failure(), Some(&bad));

        let report = DoctorReport { agent: good.clone(), git: good.clone() };
        assert!(report.ok());
        assert_eq!(report.first_failure(), None);

        // git missing must ALSO block (§7.1 step 1 ANDs both).
        let bad_git = ToolCheck { tool: Tool::Git, ok: false, version: None, error: None };
        let report = DoctorReport { agent: good, git: bad_git.clone() };
        assert!(!report.ok());
        assert_eq!(report.first_failure(), Some(&bad_git));
    }
}
