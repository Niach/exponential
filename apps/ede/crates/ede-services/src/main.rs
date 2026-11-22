//! `exponential-services`: the daemon and a command line over the services.
//!
//! ```text
//! exponential-services serve                       the data source (its unit, or the compositor's helper)
//! exponential-services list                        the domains
//! exponential-services get <domain>                a snapshot
//! exponential-services call <domain> <action> [JSON]
//! exponential-services tools                       the MCP tool list
//! ```
//!
//! #EDE-172 (P13): `get` and `call` run in this process: the daemon would
//! run them with the same power, so there is nothing to ask it for.
//! `grant` (an MCP bearer over the socket) is gone. `serve` takes the
//! compositor's intake at fd 3 when the compositor runs it (`[session]
//! helpers = "compositor"`) and binds no socket then.

use std::{
    os::fd::{FromRawFd, OwnedFd},
    process::ExitCode,
    sync::Arc,
};

use ede_services::{socket, Registry};
use serde_json::{json, Value};

fn usage() -> ExitCode {
    eprintln!("usage: exponential-services serve | list | get <domain> | call <domain> <action> [JSON] | tools");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    // #ERR-12: the crash monitor (this binary) runs and exits here.
    ede_observability::run_crash_monitor_if_requested();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let serving = args.first().map(String::as_str) == Some("serve");
    // #EDE-172 (P13): the intake stays this process's (its crash monitor,
    // started below, does not inherit it).
    if serving {
        ede_protocol::process::keep_inherited_to_this_process();
    }
    // #LOGS-2: the daemon reports to Sentry when diagnostics are on (the
    // binary only: ede-shell links this crate as a library). The one-shot
    // commands log to stderr and report nothing.
    let _observability = if serving {
        let guard = ede_observability::init("services", "info", ede_observability::Output::Stderr);
        ede_observability::watch_native_crashes("services");
        Some(guard)
    } else {
        tracing_subscriber::fmt().with_writer(std::io::stderr).with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).init();
        None
    };
    let registry = Arc::new(Registry::default());
    match args.first().map(String::as_str) {
        Some("serve") => serve(registry),
        Some("list") => {
            for n in registry.names() {
                println!("{n}");
            }
            ExitCode::SUCCESS
        }
        Some("tools") => {
            println!("{}", serde_json::to_string_pretty(&ede_services::mcp::tools(&registry)).unwrap());
            ExitCode::SUCCESS
        }
        Some("get") => {
            let Some(domain) = args.get(1) else { return usage() };
            print_result(registry.snapshot(domain).ok_or_else(|| format!("no domain {domain}")))
        }
        Some("call") => {
            let (Some(domain), Some(action)) = (args.get(1), args.get(2)) else { return usage() };
            let call_args: Value = args.get(3).map(|a| serde_json::from_str(a).unwrap_or(json!({}))).unwrap_or(json!({}));
            print_result(registry.call(domain, action, &call_args, &ede_protocol::Capability::ALL).map_err(|e| e.to_string()))
        }
        _ => usage(),
    }
}

/// The data source: over the compositor's intake when it handed one over,
/// else on `services.sock` (unless this session's compositor runs the
/// services itself: then a daemon started by hand is not it).
fn serve(registry: Arc<Registry>) -> ExitCode {
    let served = match take_intake() {
        Some(Ok(intake)) => {
            // No other process of the person reads this one from now on.
            if let Err(e) = ede_protocol::process::refuse_ptrace() {
                tracing::warn!("PR_SET_DUMPABLE: {e}");
            }
            socket::serve_intake(intake, registry)
        }
        Some(Err(e)) => Err(std::io::Error::other(e)),
        None if ede_protocol::process::helpers_spawned() => {
            eprintln!("exponential-services: no intake: this session's compositor runs the services itself ([session] helpers = \"compositor\"); a daemon started by hand is not it");
            return ExitCode::from(2);
        }
        None => socket::serve(&ede_services::socket_path(), registry),
    };
    match served {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("exponential-services: {e}");
            ExitCode::FAILURE
        }
    }
}

/// #EDE-172 (P13): the intake the compositor handed over at the descriptor
/// [`ede_protocol::CONTROL_FD_ENV`] names; taken once (the variable goes).
/// `None` = none was handed over.
fn take_intake() -> Option<Result<OwnedFd, String>> {
    let named = std::env::var(ede_protocol::CONTROL_FD_ENV).ok()?;
    std::env::remove_var(ede_protocol::CONTROL_FD_ENV);
    let Some(raw) = named.trim().parse::<i32>().ok().filter(|fd| *fd > 2) else {
        return Some(Err(format!("{}={named:?} names no descriptor", ede_protocol::CONTROL_FD_ENV)));
    };
    // Safety: fstat on a number only; one that is not open fails.
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(raw, &mut stat) } != 0 || stat.st_mode & libc::S_IFMT != libc::S_IFSOCK {
        return Some(Err(format!("descriptor {raw} is no intake")));
    }
    // Safety: the descriptor is this process's by inheritance, named for it
    // by the compositor, and taken once (the variable is gone).
    Some(Ok(unsafe { OwnedFd::from_raw_fd(raw) }))
}

fn print_result(result: Result<Value, String>) -> ExitCode {
    match result {
        Ok(v) => {
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("exponential-services: {e}");
            ExitCode::FAILURE
        }
    }
}
