//! `exponential mcp list | connect <server> | set-secret <server> |
//! disconnect <server>` (EXP-792) — the team MCP servers of your teams and
//! YOUR connection to each.
//!
//! The server holds every credential, per member: connect once and it works
//! on every device, remote start and automation. This machine holds none.
//! `connect` opens the web settings page's connect in your signed-in
//! browser (the instance runs the OAuth flow there and keeps the token; its
//! callback only accepts the member whose browser session started it); `set-secret` reads a typed value from a
//! no-echo prompt (or a piped stdin line) — never argv — and sends it to the
//! server once.

use std::io::{BufRead, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::bail;
use api::mcp_servers::{McpConnection, McpServerListEntry};

use super::{reject_unknown_flags, take_value, CommandResult};
use crate::context::{self, Ctx};
use crate::term;

const USAGE: &str = "\
Usage: exponential mcp <command> [--team <team-id|slug>]

  list                  Your teams' MCP servers and your connection to each
  connect <server>      Connect an OAuth server (opens the web settings page)
  set-secret <server>   Store your key for a server that takes one (read from
                        stdin, never argv)
  disconnect <server>   Delete your credential for a server

<server> is a server name or id. Credentials are held by the server for you
(every device, remote starts, automations); owners add servers under
Settings → MCP servers.
";

/// How often `connect` re-reads the list, and for how long.
const CONNECT_POLL: Duration = Duration::from_secs(2);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5 * 60);

pub fn run(args: &[String]) -> CommandResult {
    let sub = args.first().map(String::as_str).unwrap_or("");
    let rest = &args[1.min(args.len())..];
    match sub {
        "list" => list(rest),
        "connect" => connect(rest),
        "set-secret" => set_secret(rest),
        "disconnect" => disconnect(rest),
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

/// One listed server with the team it belongs to.
#[derive(Clone, Debug)]
struct TeamServer {
    team: String,
    /// The team's URL slug (the web deep link `connect` opens).
    team_slug: String,
    entry: McpServerListEntry,
}

/// Every server of `team` (an id or slug), or of every team you belong to.
fn load(ctx: &Ctx, team: Option<&str>) -> anyhow::Result<Vec<TeamServer>> {
    let mut teams = api::mcp_tools::teams_list(&ctx.trpc)?;
    if let Some(wanted) = team {
        teams.retain(|row| row.id == wanted || row.slug == wanted);
        if teams.is_empty() {
            bail!("you are not a member of a team `{wanted}`");
        }
    }
    let mut servers = Vec::new();
    for row in teams {
        let label = if row.name.is_empty() { row.id.clone() } else { row.name };
        for entry in api::mcp_servers::list(&ctx.trpc, &row.id)? {
            servers.push(TeamServer {
                team: label.clone(),
                team_slug: row.slug.clone(),
                entry,
            });
        }
    }
    Ok(servers)
}

/// A server by row id, or by name (case-insensitive).
fn find_server<'a>(servers: &'a [TeamServer], selector: &str) -> anyhow::Result<&'a TeamServer> {
    let wanted = selector.trim();
    if let Some(exact) = servers.iter().find(|server| server.entry.config.id == wanted) {
        return Ok(exact);
    }
    let mut matches: Vec<&TeamServer> = servers
        .iter()
        .filter(|server| server.entry.config.name.eq_ignore_ascii_case(wanted))
        .collect();
    match matches.len() {
        0 => {
            let known = servers
                .iter()
                .map(|server| format!("  {} ({})", server.entry.config.name, server.entry.config.id))
                .collect::<Vec<_>>()
                .join("\n");
            if known.is_empty() {
                bail!("no MCP server named `{wanted}`: your teams have none yet")
            }
            bail!("no MCP server named `{wanted}`. Known servers:\n{known}")
        }
        1 => Ok(matches.remove(0)),
        _ => bail!("`{wanted}` names servers on several teams; use the id (or --team)"),
    }
}

/// Your connection, as one table cell.
fn connection_cell(connection: &McpConnection) -> String {
    match connection.status.as_str() {
        "not_needed" => "no sign-in needed".to_string(),
        "connected" => match &connection.expires_at {
            Some(expires_at) => format!("connected (token until {expires_at})"),
            None => "connected".to_string(),
        },
        "expired" => "EXPIRED: connect again".to_string(),
        "error" => format!(
            "ERROR: {}",
            connection.error.as_deref().unwrap_or("the last refresh failed")
        ),
        _ => "not connected".to_string(),
    }
}

/// The next step for a server you cannot use yet.
fn next_step(server: &TeamServer) -> Option<String> {
    if server.entry.connection.is_ready() {
        return None;
    }
    let name = &server.entry.config.name;
    match server.entry.config.auth.as_str() {
        "oauth" => Some(format!("exponential mcp connect {name}")),
        "secret" => Some(format!("exponential mcp set-secret {name}")),
        _ => None,
    }
}

fn print_table(servers: &[TeamServer]) {
    if servers.is_empty() {
        println!("No MCP servers on your teams. Owners add them under Settings → MCP servers.");
        return;
    }
    let width = |pick: fn(&TeamServer) -> usize, header: usize| {
        servers.iter().map(pick).max().unwrap_or(header).max(header)
    };
    let name_width = width(|server| server.entry.config.name.len(), 4);
    let team_width = width(|server| server.team.len(), 4);
    println!(
        "{:<name_width$}  {:<team_width$}  {:<6}  {:<7}  {:<7}  {}",
        "NAME", "TEAM", "AUTH", "KIND", "MEMBERS", "YOU"
    );
    for server in servers {
        let entry = &server.entry;
        let members = if entry.config.auth == "none" {
            "-".to_string()
        } else {
            format!("{}/{}", entry.connected_count, entry.member_count)
        };
        println!(
            "{:<name_width$}  {:<team_width$}  {:<6}  {:<7}  {:<7}  {}",
            entry.config.name,
            server.team,
            entry.config.auth,
            entry.config.transport,
            members,
            connection_cell(&entry.connection)
        );
    }
}

fn team_flag(args: &mut Vec<String>) -> Option<String> {
    take_value(args, "--team").filter(|team| !team.is_empty())
}

pub fn list(args: &[String]) -> CommandResult {
    let mut args = args.to_vec();
    let team = team_flag(&mut args);
    reject_unknown_flags(&args)?;
    if let Some(extra) = args.first() {
        bail!("unexpected argument `{extra}` (usage: exponential mcp list [--team <team-id>])");
    }
    let ctx = context::load()?;
    let servers = load(&ctx, team.as_deref())?;
    print_table(&servers);
    if !servers.is_empty() {
        println!();
        for server in &servers {
            let config = &server.entry.config;
            let target = match (&config.url, &config.command) {
                (Some(url), _) if !url.is_empty() => url.clone(),
                (_, Some(command)) => {
                    let mut parts = vec![command.clone()];
                    parts.extend(config.args.iter().cloned());
                    parts.join(" ")
                }
                _ => String::new(),
            };
            println!("  {}  {target}", config.name);
            println!("    id {}", config.id);
            if let Some(step) = next_step(server) {
                println!("    to use it: {step}");
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// `<server> [--team <id>]` → the loaded context and the one server.
fn one_server(args: &[String], usage: &str) -> anyhow::Result<(Ctx, TeamServer)> {
    let mut args = args.to_vec();
    let team = team_flag(&mut args);
    reject_unknown_flags(&args)?;
    let [selector] = args.as_slice() else {
        bail!("usage: {usage}");
    };
    let ctx = context::load()?;
    let servers = load(&ctx, team.as_deref())?;
    let server = find_server(&servers, selector)?.clone();
    Ok((ctx, server))
}

pub fn connect(args: &[String]) -> CommandResult {
    let (ctx, server) = one_server(args, "exponential mcp connect <server> [--team <team-id>]")?;
    let config = &server.entry.config;
    match config.auth.as_str() {
        "oauth" => {}
        "secret" => bail!(
            "{} takes a key, not a sign-in: `exponential mcp set-secret {}`",
            config.name,
            config.name
        ),
        _ => bail!("{} needs no sign-in; every run can use it as is", config.name),
    }
    if server.entry.connection.status == "connected" {
        println!(
            "You are already connected to {}. `exponential mcp disconnect {}` first to connect again.",
            config.name, config.name
        );
        return Ok(ExitCode::SUCCESS);
    }
    // The flow starts in the BROWSER's signed-in session (the web page
    // auto-starts it), never here: the instance's callback only accepts a
    // code for the member whose browser session began the flow.
    let Some(page) =
        api::mcp_servers::connect_page_url(&ctx.account.instance_url, &server.team_slug, &config.id)
    else {
        bail!(
            "cannot build a web link to connect {} (instance `{}`); connect it under Settings → MCP servers on the web",
            config.name,
            ctx.account.instance_url
        );
    };
    match api::opener::open_in_browser(&page) {
        Ok(()) => println!("Opened your browser to connect {}.", config.name),
        Err(error) => {
            log::debug!("{error}");
            println!("Open this URL in the browser you are signed in to Exponential with:");
        }
    }
    println!();
    println!("  {page}");
    println!();
    println!(
        "Sign in to Exponential there as {} if asked, then finish the provider's sign-in.",
        ctx.account.email
    );
    println!("Waiting for the connection (up to 5 minutes) ...");
    let started = Instant::now();
    while started.elapsed() < CONNECT_TIMEOUT {
        std::thread::sleep(CONNECT_POLL);
        let rows = match api::mcp_servers::list(&ctx.trpc, &config.team_id) {
            Ok(rows) => rows,
            Err(error) => {
                log::debug!("mcpServers.list while waiting: {error}");
                continue;
            }
        };
        let Some(row) = rows.into_iter().find(|row| row.config.id == config.id) else {
            bail!("{} was removed from the team while you signed in", config.name);
        };
        if row.connection.status == "connected" {
            match row.connection.expires_at {
                Some(expires_at) => {
                    println!("Connected {} (token valid until {expires_at}).", config.name)
                }
                None => println!("Connected {}.", config.name),
            }
            println!("It works on every device, remote start and automation.");
            return Ok(ExitCode::SUCCESS);
        }
    }
    bail!(
        "{} is still not connected after 5 minutes. Run `exponential mcp connect {}` to try again.",
        config.name,
        config.name
    )
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
    let (ctx, server) = one_server(
        args,
        "exponential mcp set-secret <server> [--team <team-id>]   (the value is read from stdin)",
    )?;
    let config = &server.entry.config;
    if config.auth != "secret" {
        bail!(
            "{} does not take a key ({}){}",
            config.name,
            config.auth,
            if config.is_oauth() {
                format!("; use `exponential mcp connect {}`", config.name)
            } else {
                String::new()
            }
        );
    }
    let label = config
        .secret_names()
        .first()
        .cloned()
        .unwrap_or_else(|| "the key".to_string());
    let value = read_secret(&format!("Value for {label} ({}): ", config.name))?;
    let value = value.trim();
    if value.is_empty() {
        bail!("the value is empty; nothing stored");
    }
    api::mcp_servers::set_secret(&ctx.trpc, &config.id, value)?;
    println!(
        "Stored your {label} for {}. Every device, remote start and automation uses it.",
        config.name
    );
    Ok(ExitCode::SUCCESS)
}

pub fn disconnect(args: &[String]) -> CommandResult {
    let (ctx, server) =
        one_server(args, "exponential mcp disconnect <server> [--team <team-id>]")?;
    let config = &server.entry.config;
    if config.auth == "none" {
        bail!("{} needs no credential; nothing to disconnect", config.name);
    }
    api::mcp_servers::disconnect(&ctx.trpc, &config.id)?;
    println!("Disconnected {}: your credential for it is deleted.", config.name);
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::mcp_servers::McpServerConfig;

    fn server(id: &str, name: &str, team: &str) -> TeamServer {
        TeamServer {
            team: team.into(),
            team_slug: team.to_lowercase(),
            entry: McpServerListEntry {
                config: McpServerConfig {
                    id: id.into(),
                    name: name.into(),
                    auth: "oauth".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    #[test]
    fn find_server_matches_id_then_name_case_insensitively() {
        let servers = vec![server("11111111", "Linear", "Acme"), server("22222222", "Sentry", "Acme")];
        assert_eq!(find_server(&servers, "linear").unwrap().entry.config.id, "11111111");
        assert_eq!(find_server(&servers, "22222222").unwrap().entry.config.name, "Sentry");
        let missing = find_server(&servers, "notion").unwrap_err().to_string();
        assert!(missing.contains("Known servers"), "{missing}");
        assert!(missing.contains("Linear (11111111)"));
        // Same name on two teams: the id disambiguates.
        let twice = vec![server("a", "Docs", "Acme"), server("b", "Docs", "Beta")];
        assert!(find_server(&twice, "docs").unwrap_err().to_string().contains("several teams"));
        assert_eq!(find_server(&twice, "b").unwrap().entry.config.id, "b");
    }

    #[test]
    fn connection_cells_and_next_steps_read_like_the_settings_pane() {
        let connection = |status: &str| McpConnection {
            status: status.into(),
            expires_at: None,
            error: None,
        };
        assert_eq!(connection_cell(&connection("not_needed")), "no sign-in needed");
        assert_eq!(connection_cell(&connection("connected")), "connected");
        assert_eq!(
            connection_cell(&McpConnection {
                status: "connected".into(),
                expires_at: Some("2026-09-09T10:00:00.000Z".into()),
                error: None,
            }),
            "connected (token until 2026-09-09T10:00:00.000Z)"
        );
        assert_eq!(connection_cell(&connection("not_connected")), "not connected");
        assert_eq!(connection_cell(&connection("expired")), "EXPIRED: connect again");
        assert_eq!(
            connection_cell(&McpConnection {
                status: "error".into(),
                expires_at: None,
                error: Some("invalid_grant".into()),
            }),
            "ERROR: invalid_grant"
        );

        let mut linear = server("s", "Linear", "Acme");
        assert_eq!(next_step(&linear).as_deref(), Some("exponential mcp connect Linear"));
        linear.entry.connection = connection("connected");
        assert_eq!(next_step(&linear), None);
        let mut sentry = server("k", "Sentry", "Acme");
        sentry.entry.config.auth = "secret".into();
        assert_eq!(next_step(&sentry).as_deref(), Some("exponential mcp set-secret Sentry"));
    }
}
