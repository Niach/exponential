//! `logout` / `whoami` / `status`.

use std::process::ExitCode;

use super::{reject_unknown_flags, CommandResult};
use crate::context;

pub fn logout(args: &[String]) -> CommandResult {
    reject_unknown_flags(args)?;
    let ctx = context::load()?;
    // Best-effort server-side revocation; local sign-out proceeds either way
    // (desktop parity). An `expu_` credential (EXP_TOKEN provisioning) is an
    // API key, not a session — sign-out would revoke a synthetic session and
    // leave the key untouched, so skip the call: keys are revoked under
    // Settings → API keys.
    if let Some(token) = ctx.auth.token(&ctx.account.id) {
        if token.starts_with("expu_") {
            log::debug!("credential is an API key — skipping server-side sign-out");
        } else {
            let client = api::login::AuthClient::new();
            if let Err(err) = client.sign_out(&ctx.account.instance_url, &token) {
                log::warn!("server-side sign-out failed (continuing locally): {err}");
            }
        }
    }
    ctx.auth.sign_out(&ctx.account.id);
    println!("Signed out {}.", ctx.account.email);
    Ok(ExitCode::SUCCESS)
}

pub fn whoami(args: &[String]) -> CommandResult {
    reject_unknown_flags(args)?;
    let data_dir = context::data_dir();
    let auth = api::accounts::AuthStore::load(data_dir);
    let signed_in = auth.signed_in_accounts();
    if signed_in.is_empty() {
        println!("Not signed in. Run `exponential login`.");
        return Ok(ExitCode::FAILURE);
    }
    for account in signed_in {
        println!("{} on {}", account.email, account.instance_url);
    }
    Ok(ExitCode::SUCCESS)
}

pub fn status(args: &[String]) -> CommandResult {
    reject_unknown_flags(args)?;
    let ctx = context::load()?;
    println!("Account   {} on {}", ctx.account.email, ctx.account.instance_url);
    println!("Device    {}", ctx.device_id());

    match super::daemon::daemon_pid(&ctx.data_dir) {
        Some(pid) => println!("Daemon    running (pid {pid})"),
        None => println!("Daemon    not running (`exponential daemon install` to set it up)"),
    }

    let report = coding::run_doctor(&ctx.settings);
    let agents = report.installed_agents();
    let unauthed = report.unauthed_agents();
    if agents.is_empty() && unauthed.is_empty() {
        println!("Agents    none installed — install claude, codex or pi (see `exponential doctor`)");
    } else {
        let mut parts: Vec<String> = agents.iter().map(|agent| agent.id().to_string()).collect();
        // EXP-409: an installed-but-signed-out agent is unusable — name it
        // with the fix instead of listing it as available.
        parts.extend(
            unauthed
                .iter()
                .map(|agent| format!("{} (installed, NOT signed in)", agent.id())),
        );
        println!("Agents    {}", parts.join(", "));
        if !unauthed.is_empty() {
            println!("          sign in to use them — see `exponential doctor`");
        }
    }
    // EXP-746: which installed agents run on the session screen. Everything
    // else keeps launching into a terminal tab (`coding::resolve_transport`),
    // which is a fallback, never a failure — hence no ✗ vocabulary here.
    println!("ACP       {}", acp_summary(&ctx.settings, &report));
    let git = if report.git.ok { "ok" } else { "MISSING" };
    println!("Git       {git}");
    Ok(ExitCode::SUCCESS)
}

/// The one-line ACP readiness summary for `status`. The device-global
/// "Start in terminal" setting wins over every per-agent answer, so it is
/// what the line says when it is on.
fn acp_summary(settings: &coding::Settings, report: &coding::DoctorReport) -> String {
    if settings.start_in_terminal {
        return "off — \"Start in terminal\" is on".to_string();
    }
    let ready: Vec<&str> = report
        .installed_agents()
        .into_iter()
        .filter(|agent| report.check_for(*agent).acp == Some(true))
        .map(|agent| agent.id())
        .collect();
    if ready.is_empty() {
        return "none — sessions run in a terminal tab".to_string();
    }
    ready.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use coding::doctor::{Tool, ToolCheck};
    use coding::{CodingAgent, DoctorReport};

    fn check(tool: Tool, ok: bool, acp: Option<bool>) -> ToolCheck {
        ToolCheck {
            tool,
            ok,
            version: ok.then(|| "1.0.0".to_string()),
            error: None,
            authed: None,
            account: None,
            usage_eligible: false,
            acp,
            acp_note: None,
        }
    }

    fn report(claude: Option<bool>, codex: Option<bool>) -> DoctorReport {
        DoctorReport {
            claude: check(Tool::Claude, true, claude),
            codex: check(Tool::Codex, true, codex),
            // Not installed: it can never be ACP-ready, whatever the flag says.
            pi: check(Tool::Pi, false, Some(true)),
            git: check(Tool::Git, true, None),
        }
    }

    /// EXP-746: `status` says which agents reach the session screen. Only
    /// INSTALLED agents count, "Start in terminal" overrides every per-agent
    /// answer, and nothing here is ever phrased as a failure — the terminal
    /// path is a fallback, not a broken machine.
    #[test]
    fn the_acp_line_names_the_ready_agents() {
        let mut settings = coding::Settings::default();
        assert!(!settings.start_in_terminal, "the default is the ACP engine");

        let both = report(Some(true), Some(true));
        assert_eq!(acp_summary(&settings, &both), "claude, codex");
        // pi is not installed, so its `Some(true)` never reaches the line.
        assert!(!both.installed_agents().contains(&CodingAgent::Pi));

        assert_eq!(acp_summary(&settings, &report(Some(true), None)), "claude");
        assert_eq!(
            acp_summary(&settings, &report(Some(false), Some(false))),
            "none — sessions run in a terminal tab"
        );

        settings.start_in_terminal = true;
        assert_eq!(
            acp_summary(&settings, &report(Some(true), Some(true))),
            "off — \"Start in terminal\" is on"
        );
    }
}
