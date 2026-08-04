//! `logout` / `whoami` / `status`.

use std::process::ExitCode;

use super::{reject_unknown_flags, CommandResult};
use crate::context;

pub fn logout(args: &[String]) -> CommandResult {
    reject_unknown_flags(args)?;
    let ctx = context::load()?;
    // Best-effort server-side revocation; local sign-out proceeds either way
    // (desktop parity).
    if let Some(token) = ctx.auth.token(&ctx.account.id) {
        let client = api::login::AuthClient::new();
        if let Err(err) = client.sign_out(&ctx.account.instance_url, &token) {
            log::warn!("server-side sign-out failed (continuing locally): {err}");
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
    let git = if report.git.ok { "ok" } else { "MISSING" };
    println!("Git       {git}");
    Ok(ExitCode::SUCCESS)
}
