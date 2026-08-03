//! `exponential doctor` — the desktop onboarding checks as a checklist:
//! git + the three agent CLIs, probed with the login-shell PATH. Exit code
//! is non-zero when git or the SELECTED default agent fails (the other
//! agents are informational — the doctor never falsely blocks).

use std::process::ExitCode;

use coding::doctor::ToolCheck;

use super::{reject_unknown_flags, CommandResult};
use crate::context;

pub fn run(args: &[String]) -> CommandResult {
    reject_unknown_flags(args)?;
    let data_dir = context::data_dir();
    let settings = coding::Settings::load(&coding::Settings::default_path(&data_dir));
    let report = coding::run_doctor(&settings);

    print_check("git", &report.git);
    print_check("claude", &report.claude);
    print_check("codex", &report.codex);
    print_check("pi", &report.pi);

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

fn print_check(name: &str, check: &ToolCheck) {
    if check.ok {
        let version = check.version.as_deref().unwrap_or("ok");
        println!("  ✓ {name:<8} {version}");
    } else {
        let error = check.error.as_deref().unwrap_or("not found");
        println!("  ✗ {name:<8} {error}");
    }
}
