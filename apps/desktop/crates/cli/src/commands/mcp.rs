//! `exponential mcp list | login <server> [--paste] | set-secret <server>
//! <NAME> | status` (EXP-792) — the team MCP servers this account may
//! connect to, and the credentials THIS machine holds for them.
//!
//! Credentials never travel through argv: `login` runs the OAuth flow
//! locally (a loopback listener, or `--paste` for a machine without a
//! browser — the person opens the printed URL anywhere and pastes the
//! redirect back), `set-secret` reads the value from a no-echo prompt (or a
//! piped stdin line). Both end by re-reporting this device's readiness so
//! the web's matrix flips without waiting for the daemon's next sweep.

use std::io::{BufRead, Write};
use std::process::ExitCode;

use anyhow::{anyhow, bail};
use api::mcp_servers::{list_for_device, McpReadinessReport, McpServerConfig};

use super::{reject_unknown_flags, take_flag, CommandResult};
use crate::context::{self, Ctx};
use crate::term;

const USAGE: &str = "\
Usage: exponential mcp <command>

Commands:
  list                         Team MCP servers and this machine's readiness
  login <server> [--paste]     Sign in to an OAuth server on this machine
  set-secret <server> <NAME>   Store the value for a declared header/env name
  status                       This machine's readiness per server
";

pub fn run(args: &[String]) -> CommandResult {
    let sub = args.first().map(String::as_str).unwrap_or("");
    let rest = &args[1.min(args.len())..];
    match sub {
        "list" => list(rest),
        "login" => login(rest),
        "set-secret" => set_secret(rest),
        "status" => status(rest),
        "help" | "--help" | "-h" | "" => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        other => {
            eprintln!("Unknown mcp command `{other}`.\n");
            print!("{USAGE}");
            Ok(ExitCode::from(2))
        }
    }
}

fn now_secs() -> u64 {
    coding::run_registry::now_secs()
}

/// A server by name (case-insensitive) or row id.
fn find_server<'a>(configs: &'a [McpServerConfig], selector: &str) -> anyhow::Result<&'a McpServerConfig> {
    let wanted = selector.trim();
    let mut matches: Vec<&McpServerConfig> = configs
        .iter()
        .filter(|config| config.id == wanted || config.name.eq_ignore_ascii_case(wanted))
        .collect();
    if let Some(exact) = matches.iter().find(|config| config.id == wanted) {
        return Ok(exact);
    }
    match matches.len() {
        0 => {
            let known = configs
                .iter()
                .map(|config| format!("  {} ({})", config.name, config.id))
                .collect::<Vec<_>>()
                .join("\n");
            bail!("no MCP server named `{wanted}`. Known servers:\n{known}")
        }
        1 => Ok(matches.remove(0)),
        _ => bail!("`{wanted}` names servers on several teams; use the id instead"),
    }
}

fn readiness_cell(entry: Option<&McpReadinessReport>) -> String {
    match entry {
        Some(entry) if entry.ready => match &entry.expires_at {
            Some(expires_at) => format!("ready (until {expires_at})"),
            None => "ready".to_string(),
        },
        Some(entry) => format!("NOT READY: {}", entry.error.as_deref().unwrap_or("no credential")),
        None => "-".to_string(),
    }
}

fn print_table(configs: &[McpServerConfig], entries: &[McpReadinessReport]) {
    if configs.is_empty() {
        println!("No MCP servers on your teams. Owners add them under Settings → MCP servers.");
        return;
    }
    let name_width = configs
        .iter()
        .map(|config| config.name.len())
        .max()
        .unwrap_or(4)
        .max(4);
    println!(
        "{:<name_width$}  {:<7}  {:<6}  {}",
        "NAME", "AUTH", "KIND", "THIS MACHINE"
    );
    for config in configs {
        let entry = entries.iter().find(|entry| entry.server_id == config.id);
        println!(
            "{:<name_width$}  {:<7}  {:<6}  {}",
            config.name,
            config.auth,
            config.transport,
            readiness_cell(entry)
        );
    }
}

fn load_with_readiness(ctx: &Ctx) -> anyhow::Result<(Vec<McpServerConfig>, Vec<McpReadinessReport>)> {
    let configs = list_for_device(&ctx.trpc)?;
    let entries = coding::mcp_servers::readiness(&ctx.data_dir, &ctx.account.id, &configs, now_secs());
    Ok((configs, entries))
}

pub fn list(args: &[String]) -> CommandResult {
    reject_unknown_flags(args)?;
    let ctx = context::load()?;
    let (configs, entries) = load_with_readiness(&ctx)?;
    print_table(&configs, &entries);
    if !configs.is_empty() {
        println!();
        for config in &configs {
            let target = match (&config.url, &config.command) {
                (Some(url), _) if !url.is_empty() => url.clone(),
                (_, Some(command)) => {
                    let mut parts = vec![command.clone()];
                    parts.extend(config.args.iter().cloned());
                    parts.join(" ")
                }
                _ => String::new(),
            };
            let names = config.secret_names().join(", ");
            let detail = match config.auth.as_str() {
                "secret" if !names.is_empty() => format!(" (needs {names})"),
                "oauth" if !config.scopes.is_empty() => format!(" (scopes: {})", config.scopes.join(" ")),
                _ => String::new(),
            };
            println!("  {}  {target}{detail}", config.name);
            println!("    id {}", config.id);
        }
    }
    Ok(ExitCode::SUCCESS)
}

pub fn status(args: &[String]) -> CommandResult {
    reject_unknown_flags(args)?;
    let ctx = context::load()?;
    println!("Device    {}", ctx.device_id());
    let (configs, entries) = load_with_readiness(&ctx)?;
    print_table(&configs, &entries);
    let blocked = entries.iter().filter(|entry| !entry.ready).count();
    if blocked > 0 {
        println!();
        println!(
            "{blocked} server{} need{} a credential on this machine: `exponential mcp login <server>` or `exponential mcp set-secret <server> <NAME>`.",
            if blocked == 1 { "" } else { "s" },
            if blocked == 1 { "s" } else { "" }
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// Push this machine's readiness now, so the web matrix follows at once.
fn report(ctx: &Ctx) {
    match coding::mcp_servers::report_now(&ctx.data_dir, &ctx.account.id, &ctx.trpc, &ctx.device_id()) {
        Ok(_) => {}
        Err(error) => log::debug!("readiness report failed (the daemon's next sweep retries): {error}"),
    }
}

pub fn login(args: &[String]) -> CommandResult {
    let mut args = args.to_vec();
    let paste = take_flag(&mut args, "--paste");
    reject_unknown_flags(&args)?;
    let selector = args
        .first()
        .ok_or_else(|| anyhow!("usage: exponential mcp login <server> [--paste]"))?;
    let ctx = context::load()?;
    let configs = list_for_device(&ctx.trpc)?;
    let config = find_server(&configs, selector)?;
    if !config.is_oauth() {
        bail!(
            "{} does not use OAuth ({}); {}",
            config.name,
            config.auth,
            if config.auth == "secret" {
                "use `exponential mcp set-secret` instead"
            } else {
                "nothing to sign in to"
            }
        );
    }
    let app_base = ctx.trpc.base_url().to_string();
    let login = coding::mcp_servers::begin_local_login(
        &ctx.data_dir,
        &ctx.account.id,
        &app_base,
        config,
        paste,
    )?;
    let set = if paste {
        println!("Open this URL in any browser and sign in to {}:", config.name);
        println!();
        println!("  {}", login.authorize_url);
        println!();
        println!("The browser will end on a page that cannot load (127.0.0.1:1). Copy its full address.");
        let pasted = term::prompt_line("Paste the redirect URL: ")?;
        coding::mcp_servers::finish_pasted_login(&ctx.data_dir, &ctx.account.id, &login, &pasted)?
    } else {
        match api::opener::open_in_browser(&login.authorize_url) {
            Ok(()) => println!("Opened your browser to sign in to {}.", config.name),
            Err(error) => {
                log::debug!("{error}");
                println!("Open this URL to sign in to {}:", config.name);
            }
        }
        println!();
        println!("  {}", login.authorize_url);
        println!();
        println!("Waiting for the browser to come back (up to 5 minutes; --paste for a machine without one) ...");
        coding::mcp_servers::finish_local_login(&ctx.data_dir, &ctx.account.id, login)?
    };
    match set.expires_at_iso() {
        Some(expires_at) => println!("Signed in to {} on this machine (token valid until {expires_at}).", config.name),
        None => println!("Signed in to {} on this machine.", config.name),
    }
    report(&ctx);
    Ok(ExitCode::SUCCESS)
}

/// Read a secret from stdin: no-echo when it is a tty, one line otherwise
/// (`echo "$VALUE" | exponential mcp set-secret …`).
fn read_secret(prompt: &str) -> anyhow::Result<String> {
    if !term::stdin_is_tty() {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        return Ok(line.trim_end_matches(['\n', '\r']).to_string());
    }
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
    let mut restored = None;
    // Turn ECHO off for the read; every exit path below puts it back.
    unsafe {
        if libc::tcgetattr(libc::STDIN_FILENO, original.as_mut_ptr()) == 0 {
            let original = original.assume_init();
            let mut quiet = original;
            quiet.c_lflag &= !libc::ECHO;
            if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &quiet) == 0 {
                restored = Some(original);
            }
        }
    }
    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line);
    if let Some(original) = restored {
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &original);
        }
    }
    println!();
    read?;
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

pub fn set_secret(args: &[String]) -> CommandResult {
    reject_unknown_flags(args)?;
    let [selector, name] = args else {
        bail!("usage: exponential mcp set-secret <server> <NAME>   (the value is read from stdin)");
    };
    let ctx = context::load()?;
    let configs = list_for_device(&ctx.trpc)?;
    let config = find_server(&configs, selector)?;
    if config.auth != "secret" {
        bail!(
            "{} does not take a typed secret ({}){}",
            config.name,
            config.auth,
            if config.is_oauth() { "; use `exponential mcp login`" } else { "" }
        );
    }
    let value = read_secret(&format!("Value for {name} ({}): ", config.name))?;
    coding::mcp_servers::set_secret(&ctx.data_dir, &ctx.account.id, config, name, &value)
        .map_err(|error| anyhow!(error))?;
    println!("Stored {name} for {} on this machine.", config.name);
    report(&ctx);
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(id: &str, name: &str) -> McpServerConfig {
        McpServerConfig {
            id: id.into(),
            name: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn find_server_matches_id_then_name_case_insensitively() {
        let configs = vec![config("11111111", "Linear"), config("22222222", "Sentry")];
        assert_eq!(find_server(&configs, "linear").unwrap().id, "11111111");
        assert_eq!(find_server(&configs, "22222222").unwrap().name, "Sentry");
        let missing = find_server(&configs, "notion").unwrap_err().to_string();
        assert!(missing.contains("Known servers"), "{missing}");
        assert!(missing.contains("Linear (11111111)"));
        // Same name on two teams: the id disambiguates.
        let twice = vec![config("a", "Docs"), config("b", "Docs")];
        assert!(find_server(&twice, "docs").unwrap_err().to_string().contains("several teams"));
        assert_eq!(find_server(&twice, "b").unwrap().id, "b");
    }

    #[test]
    fn readiness_cells_read_like_the_web_matrix() {
        assert_eq!(readiness_cell(None), "-");
        assert_eq!(
            readiness_cell(Some(&McpReadinessReport {
                server_id: "s".into(),
                ready: true,
                expires_at: Some("2026-09-09T10:00:00.000Z".into()),
                error: None,
            })),
            "ready (until 2026-09-09T10:00:00.000Z)"
        );
        assert_eq!(
            readiness_cell(Some(&McpReadinessReport {
                server_id: "s".into(),
                ready: false,
                expires_at: None,
                error: Some("not signed in on this machine".into()),
            })),
            "NOT READY: not signed in on this machine"
        );
    }
}
