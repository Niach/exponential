//! `exponential doctor` — the desktop onboarding checks as a checklist:
//! git + the agent CLIs, probed with the login-shell PATH. Exit code
//! is non-zero when git or the SELECTED default agent fails (the other
//! agents are informational — the doctor never falsely blocks).
//!
//! EXP-746 added a second row per agent: ACP readiness. It never touches the
//! exit code, but it IS the coding gate (EXP-773): an agent that is not
//! ACP-ready cannot start a session on this device at all — the launch is
//! refused with the row's note; there is no terminal fallback.
//!
//! EXP-755/EXP-849: this command is the ONE deep pass (the codex handshake
//! below). Every other caller runs the plain doctor.

use std::process::ExitCode;

use coding::doctor::ToolCheck;
use coding::CodingAgent;

use super::{reject_unknown_flags, CommandResult};
use crate::context;

pub fn run(args: &[String]) -> CommandResult {
    reject_unknown_flags(args)?;
    let data_dir = context::data_dir();
    let settings = coding::Settings::load(&coding::Settings::default_path(&data_dir));
    let mut report = coding::run_doctor(&settings, &data_dir);
    // EXP-746: `run_doctor` also runs on the launch path and inline in the
    // daemon every 5 minutes, so it takes codex's readiness on presence. A
    // hand-typed `exponential doctor` can afford the real handshake, and it
    // is the check a user running this command actually wants.
    deep_probe_codex_acp(&settings, &mut report.codex);

    print_check("git", &report.git, &data_dir);
    print_check("claude", &report.claude, &data_dir);
    print_check("codex", &report.codex, &data_dir);

    if report.check_for(settings.default_agent).acp != Some(true) {
        println!();
        println!(
            "  Note: {} cannot run a coding session here — the acp row above says why.",
            settings.default_agent.label()
        );
    }

    let default_agent = settings.default_agent;
    let gate_failed = report.first_failure_for(default_agent).is_some();
    if gate_failed {
        println!();
        println!(
            "Default agent is {} — fix the failing checks above (or install another agent and make it the default).",
            default_agent.id()
        );
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

fn print_check(name: &str, check: &ToolCheck, data_dir: &std::path::Path) {
    if check.ok {
        let version = check.version.as_deref().unwrap_or("ok");
        // EXP-1138: runnable on a named profile while the ambient login is
        // signed out — say which, so "signed in" is never claimed for it.
        let profile = check.signed_in_profile.as_deref().and_then(|id| {
            check
                .tool
                .agent()
                .and_then(|agent| coding::agent_profiles::get(data_dir, agent, id))
                .map(|profile| profile.label)
        });
        match (check.authed, profile) {
            (_, Some(label)) => println!(
                "  ✓ {name:<8} {version} — {} login signed out; runs on «{label}»",
                coding::agent_profiles::SYSTEM_LABEL
            ),
            (Some(true), None) => println!("  ✓ {name:<8} {version} — signed in"),
            _ => println!("  ✓ {name:<8} {version}"),
        }
    } else if check.signed_out() {
        // Installed but signed out (EXP-409): show the version so it reads
        // as "sign in", not "install".
        let version = check.version.as_deref().unwrap_or("installed");
        let error = check.error.as_deref().unwrap_or("not signed in");
        println!("  ✗ {name:<8} {version} — {error}");
    } else {
        let error = check.error.as_deref().unwrap_or("not found");
        println!("  ✗ {name:<8} {error}");
    }
    print_acp(check);
}

/// EXP-746: the agent's ACP readiness row, indented under its check. Silent
/// where readiness has no meaning (git) or was never probed (an unparseable
/// claude version — the doctor never falsely blocks a nonstandard build,
/// though the launch gate still refuses it, EXP-773).
fn print_acp(check: &ToolCheck) {
    match check.acp {
        Some(true) => println!("             acp: ready"),
        Some(false) => {
            let note = check
                .acp_note
                .as_deref()
                .unwrap_or("coding sessions cannot start with this agent");
            println!("             acp: not supported ({note})");
        }
        None => {}
    }
}

/// The real `codex app-server` handshake, replacing the presence-only answer
/// `run_doctor` gives (bounded by the doctor's own 10 s probe timeout, killed
/// on drop). Only ever DOWNGRADES: a codex that is not installed or not
/// signed in already reads as not supported, and there is nothing to probe.
fn deep_probe_codex_acp(settings: &coding::Settings, check: &mut ToolCheck) {
    if check.acp != Some(true) {
        return;
    }
    let program = settings.resolved_path_for(CodingAgent::Codex);
    if coding::doctor::probe_codex_acp(&program, &terminal::pty::login_path()) {
        return;
    }
    check.acp = Some(false);
    check.acp_note = Some("`codex app-server` did not answer".to_string());
}
