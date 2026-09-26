//! `exponential` — the headless CLI + daemon (EXP-403).
//!
//! One binary, the desktop's crates underneath: the SAME launcher
//! (`coding::prepare_with_hooks`), the same three agents on a real PTY
//! (`terminal` core — agents run as interactive TUIs, never `-p` print
//! mode), and the same steer publisher/activity machinery, so a session
//! started here is steerable from the web exactly like a desktop session.
//! `daemon` registers this machine as a persistent per-user device
//! (`devices.register`, kind `server`) and executes remote starts.

mod agent_login_host;
mod commands;
mod context;
mod launch;
mod prefs;
mod registry;
mod session_host;
mod term;

use std::process::ExitCode;

/// The CLI's own version: release CI injects the `cli-v*` tag version via
/// `EXP_CLI_VERSION`; dev builds fall back to the workspace version.
pub fn cli_version() -> &'static str {
    option_env!("EXP_CLI_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// The CLI's stderr line (`[info] message`), the terminal half of the
/// logger.
fn print_stderr(record: &log::Record) {
    eprintln!("[{}] {}", record.level().as_str().to_lowercase(), record.args());
}

/// EXP-1099: stderr (as before) PLUS the rotating file under
/// `{data_dir}/logs/` — `exponential-daemon.log` for the daemon itself,
/// `exponential-cli.log` for every other command (the desktop app owns
/// `exponential.log` in the same shared data dir; one writer per file keeps
/// rotation race-free). A daemon under launchd/systemd (stderr not a
/// terminal) echoes only warnings and errors to stderr — the service log
/// file then holds crash traces, not a second copy of every line.
fn install_logger(command: &str, rest: &[String]) {
    use std::io::IsTerminal as _;
    let level = coding::logging::level_from_env();
    let is_daemon = command == "daemon"
        && !matches!(
            rest.first().map(String::as_str),
            Some("install" | "uninstall" | "status")
        );
    let file_name = if is_daemon {
        "exponential-daemon.log"
    } else {
        "exponential-cli.log"
    };
    let stderr_min = if is_daemon && !std::io::stderr().is_terminal() {
        log::LevelFilter::Warn
    } else {
        log::LevelFilter::Trace
    };
    let logger = coding::logging::ExpLogger::new(level)
        .with_file(&context::data_dir(), file_name)
        .with_stderr(print_stderr, stderr_min);
    if let Some(logger) = coding::logging::install(logger) {
        // File only: an interactive command must not print a banner.
        logger.file_only(
            log::Level::Info,
            "exponential",
            format_args!(
                "exponential {} `{command}` started (pid {})",
                cli_version(),
                std::process::id()
            ),
        );
    }
    coding::logging::install_panic_hook();
}

const USAGE: &str = "\
exponential — Exponential from your terminal

Usage: exponential <command> [options]

Commands:
  login [--instance <url>]                Sign in (device code, approved in any browser)
  logout                                  Sign out and drop local credentials
  whoami                                  Show the signed-in account
  status                                  Account + daemon + tooling summary
  doctor                                  Check git and the agent CLIs
  code <ISSUE> [options]                  Start a coding session for an issue
  run <action> [options]                  Run a team action (or a builtin)
  mcp list|login|set-secret|status        Team MCP servers + this machine's credentials
  daemon [--foreground] [--label <name>]  Run the remote-start daemon
  daemon install|uninstall|status         Manage the systemd/launchd service
  update                                  Self-update from the latest cli release
  uninstall [--yes]                       Remove the daemon service and delete this binary
  version                                 Print the CLI version

Options for code/run:
  --agent claude|codex      --model <m>   --effort <e>
  --plan                    --detach      (headless; steer it from the web)
Options for run:
  --team <team-id>          --input k=v   (repeatable; the action's pick inputs)
  --prompt <text>           (additional instructions; the whole request for
                             create-action)
Environment:
  EXP_INSTANCE, EXP_TOKEN (API key expu_… from Settings → Security, or a session
  token; non-interactive login), EXP_LOG=debug
";

fn maybe_prompt_auto_update() {
    if !term::stdin_is_tty() {
        return;
    }
    let data_dir = context::data_dir();
    if prefs::auto_update(&data_dir).is_some() {
        return;
    }
    let answer = term::prompt_line("Keep the exponential CLI up to date automatically? [Y/n] ")
        .unwrap_or_default();
    let enabled = !matches!(answer.trim().to_lowercase().as_str(), "n" | "no");
    prefs::set_auto_update(&data_dir, enabled);
    if enabled {
        println!("Auto-update enabled (cliAutoUpdate in settings.json turns it off).");
    } else {
        println!("Auto-update off — `exponential update` updates manually.");
    }
}

fn main() -> ExitCode {
    // The CLI is its own 426-gated platform: every request must say
    // `cli/<version>`, never ride the desktop's gate. Before any HTTP.
    domain::client_version::set_client_identity("cli", cli_version());
    // The ONE process-wide blocking HTTP client (EXP-304) — main-thread init.
    api::http::init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("");
    let rest = &args[1.min(args.len())..];
    install_logger(command, rest);

    // First interactive run: ask once whether to keep the CLI current
    // automatically (stored as `cliAutoUpdate` in settings.json; the daemon
    // and every later run honor it). Never prompts without a tty.
    if matches!(command, "login" | "code" | "run" | "doctor" | "status" | "whoami" | "daemon") {
        maybe_prompt_auto_update();
    }
    // One-shot commands: a throttled quiet check; a fresh install re-execs
    // this same invocation on the new binary. The daemon runs its own
    // cadence (and the web Update button) inside its loop instead.
    if matches!(command, "login" | "code" | "run" | "doctor" | "status") {
        commands::update::maybe_auto_update_and_reexec();
    }

    let result = match command {
        "login" => commands::login::run(rest),
        "logout" => commands::account::logout(rest),
        "whoami" => commands::account::whoami(rest),
        "status" => commands::account::status(rest),
        "doctor" => commands::doctor::run(rest),
        "code" => commands::code::run(rest),
        "run" => commands::run::run(rest),
        "mcp" => commands::mcp::run(rest),
        "daemon" => commands::daemon::run(rest),
        "update" => commands::update::run(rest),
        "uninstall" => commands::uninstall::run(rest),
        "version" | "--version" | "-V" => {
            println!("exponential {}", cli_version());
            Ok(ExitCode::SUCCESS)
        }
        "help" | "--help" | "-h" | "" => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        other => {
            eprintln!("Unknown command `{other}`.\n");
            print!("{USAGE}");
            Ok(ExitCode::from(2))
        }
    };

    match result {
        Ok(code) => code,
        Err(err) => {
            // ApiError::UpgradeRequired anywhere in the chain means the
            // server 426-gated this build — the one error with a fixed fix.
            if err
                .chain()
                .any(|cause| matches!(cause.downcast_ref::<api::ApiError>(), Some(api::ApiError::UpgradeRequired)))
            {
                eprintln!("This version of the exponential CLI is no longer supported by the server.");
                eprintln!("Run `exponential update` and try again.");
            } else {
                eprintln!("error: {err:#}");
            }
            ExitCode::FAILURE
        }
    }
}
