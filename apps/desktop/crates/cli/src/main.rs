//! `exponential` — the headless CLI + daemon (EXP-403).
//!
//! One binary, the desktop's crates underneath: the SAME launcher
//! (`coding::prepare_with_hooks`), the same three agents on a real PTY
//! (`terminal` core — agents run as interactive TUIs, never `-p` print
//! mode), and the same steer publisher/activity machinery, so a session
//! started here is steerable from the web exactly like a desktop session.
//! `daemon` registers this machine as a persistent per-user device
//! (`devices.register`, kind `server`) and executes remote starts.

mod commands;
mod context;
mod launch;
mod prefs;
mod registry;
mod session_host;
mod sidecars;
mod term;

use std::process::ExitCode;

/// The CLI's own version: release CI injects the `cli-v*` tag version via
/// `EXP_CLI_VERSION`; dev builds fall back to the workspace version.
pub fn cli_version() -> &'static str {
    option_env!("EXP_CLI_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// Minimal stderr logger — the daemon runs under systemd/launchd where
/// stderr IS the journal; no logging deps beyond the `log` facade.
struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= max_level()
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}] {}", record.level().as_str().to_lowercase(), record.args());
        }
    }

    fn flush(&self) {}
}

fn max_level() -> log::LevelFilter {
    match std::env::var("EXP_LOG").ok().as_deref() {
        Some("debug") => log::LevelFilter::Debug,
        Some("trace") => log::LevelFilter::Trace,
        _ => log::LevelFilter::Info,
    }
}

static LOGGER: StderrLogger = StderrLogger;

const USAGE: &str = "\
exponential — Exponential from your terminal

Usage: exponential <command> [options]

Commands:
  login [--instance <url>] [--password]   Sign in (device code; --password for scripted setups)
  logout                                  Sign out and drop local credentials
  whoami                                  Show the signed-in account
  status                                  Account + daemon + tooling summary
  doctor                                  Check git and the agent CLIs
  code <ISSUE> [options]                  Start a coding session for an issue
  run <action> [--input k=v ...]          Run a team action (or a builtin)
  daemon [--foreground] [--label <name>]  Run the remote-start daemon
  daemon install|uninstall|status         Manage the systemd/launchd service
  update                                  Self-update from the latest cli release
  uninstall [--yes]                       Remove the daemon service and delete this binary
  version                                 Print the CLI version

Options for code/run:
  --agent claude|codex|pi   --model <m>   --effort <e>
  --plan                    --skip-permissions
Environment:
  EXP_INSTANCE, EXP_EMAIL, EXP_PASSWORD, EXP_TOKEN (non-interactive login), EXP_LOG=debug
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
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(max_level());
    // The CLI is its own 426-gated platform: every request must say
    // `cli/<version>`, never ride the desktop's gate. Before any HTTP.
    domain::client_version::set_client_identity("cli", cli_version());
    // The ONE process-wide blocking HTTP client (EXP-304) — main-thread init.
    api::http::init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("");
    let rest = &args[1.min(args.len())..];

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
