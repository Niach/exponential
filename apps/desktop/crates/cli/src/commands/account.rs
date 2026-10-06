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
    // Settings → Security.
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
    // EXP-1110: what the SERVER knows about this machine (best-effort — an
    // unreachable server only drops the label, never the status).
    let device_id = ctx.device_id();
    let row = api::mcp_tools::devices_list(&ctx.trpc, None)
        .map_err(|err| log::debug!("status: devices list failed: {err}"))
        .ok()
        .and_then(|devices| devices.into_iter().find(|device| device.device_id == device_id));
    let pid = super::daemon::daemon_pid(&ctx.data_dir);
    for line in device_lines(&device_id, pid, row.as_ref()) {
        println!("{line}");
    }
    let auto_update = match crate::prefs::auto_update(&ctx.data_dir) {
        Some(true) => "on",
        Some(false) => "off (`exponential update --auto on` turns it on)",
        None => "not set (`exponential update --auto on|off`)",
    };
    println!("Updates   {} {auto_update}", crate::cli_version());

    // EXP-1196: the device readiness block, the same rows `doctor` prints
    // (the plain pass: no codex handshake here).
    println!();
    let doctor = super::doctor::local_doctor(&ctx.settings, &ctx.data_dir, false);
    for line in super::doctor::render_block(&doctor, None) {
        println!("{line}");
    }
    Ok(ExitCode::SUCCESS)
}

/// EXP-1110: said wherever a person could believe signing in was enough —
/// `status`, `login`, `devices`. Only a running daemon registers this machine
/// and keeps it online for remote starts.
pub const NOT_REGISTERED_HINT: &str = "This machine is NOT registered as a device: it is not visible to your \
other clients and can't be remote-started until the daemon runs. \
Set it up with `exponential daemon install` (or run `exponential daemon`).";

/// The `Device` + `Daemon` lines of `status` (pure, unit-tested): the local
/// daemon state beside what the server's device row says.
fn device_lines(
    device_id: &str,
    daemon_pid: Option<u32>,
    row: Option<&api::mcp_tools::RemoteDevice>,
) -> Vec<String> {
    let mut lines = Vec::new();
    match row {
        Some(row) => {
            let label = if row.label.is_empty() { "(no label)" } else { row.label.as_str() };
            let state = if row.online { "online" } else { "offline" };
            lines.push(format!("Device    {label} — {state} ({device_id})"));
        }
        None => lines.push(format!("Device    {device_id} (not registered)")),
    }
    match daemon_pid {
        Some(pid) => lines.push(format!("Daemon    running (pid {pid})")),
        None => {
            lines.push("Daemon    NOT running".to_string());
            let detail = match row {
                Some(row) if row.online => {
                    "          the server still reports this machine online; it drops offline within a minute".to_string()
                }
                Some(_) => "          this machine is registered but OFFLINE: it can't be remote-started until \
the daemon runs (`exponential daemon install`)"
                    .to_string(),
                None => format!("          {NOT_REGISTERED_HINT}"),
            };
            lines.push(detail);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-1110: with no daemon, `status` says in so many words that this
    /// machine cannot be remote-started — and why.
    #[test]
    fn a_stopped_daemon_says_the_machine_is_not_a_device() {
        let lines = device_lines("cli-1", None, None);
        assert_eq!(lines[0], "Device    cli-1 (not registered)");
        assert_eq!(lines[1], "Daemon    NOT running");
        assert!(lines[2].contains("NOT registered as a device"), "{lines:?}");
        assert!(lines[2].contains("exponential daemon install"), "{lines:?}");

        let row = api::mcp_tools::RemoteDevice {
            device_id: "cli-1".into(),
            label: "build box".into(),
            online: false,
            ..Default::default()
        };
        let lines = device_lines("cli-1", None, Some(&row));
        assert_eq!(lines[0], "Device    build box — offline (cli-1)");
        assert!(lines[2].contains("registered but OFFLINE"), "{lines:?}");

        let online = api::mcp_tools::RemoteDevice { online: true, ..row };
        let lines = device_lines("cli-1", Some(42), Some(&online));
        assert_eq!(
            lines,
            vec![
                "Device    build box — online (cli-1)".to_string(),
                "Daemon    running (pid 42)".to_string(),
            ]
        );
    }
}
