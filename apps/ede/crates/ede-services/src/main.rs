//! `exponential-services`: the daemon and a command line over it.
//!
//! ```text
//! exponential-services serve                       the data socket + the MCP face (exponential-services.service)
//! exponential-services list                        the domains
//! exponential-services get <domain>                a snapshot (through the daemon; direct when it is not running)
//! exponential-services call <domain> <action> [JSON]
//! exponential-services grant <session> [label] [cap,cap…]   an MCP bearer for an agent session (through the daemon)
//! exponential-services tools                       the MCP tool list
//! ```

use std::{process::ExitCode, sync::Arc};

use ede_services::{mcp::Mcp, socket, Registry};
use serde_json::{json, Value};

fn usage() -> ExitCode {
    eprintln!("usage: exponential-services serve | list | get <domain> | call <domain> <action> [JSON] | grant <session> [label] [cap,cap…] | tools");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    // #ERR-12: the crash monitor (this binary) runs and exits here.
    ede_observability::run_crash_monitor_if_requested();
    let args: Vec<String> = std::env::args().skip(1).collect();
    // #LOGS-2: the daemon reports to Sentry when diagnostics are on (the
    // binary only: ede-shell links this crate as a library and never inits
    // a client). The one-shot commands log to stderr and report nothing.
    let _observability = if args.first().map(String::as_str) == Some("serve") {
        let guard = ede_observability::init("services", "info", ede_observability::Output::Stderr);
        ede_observability::watch_native_crashes("services");
        Some(guard)
    } else {
        tracing_subscriber::fmt().with_writer(std::io::stderr).with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).init();
        None
    };
    let registry = Arc::new(Registry::default());
    match args.first().map(String::as_str) {
        Some("serve") => {
            let mcp = match Mcp::serve(registry.clone()) {
                Ok(m) => {
                    tracing::info!(url = %m.url(), "MCP face");
                    Some(Arc::new(m))
                }
                Err(e) => {
                    tracing::warn!("{e}; the data socket serves without the MCP face");
                    None
                }
            };
            match socket::serve(&ede_services::socket_path(), registry, mcp) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("exponential-services: {e}");
                    ExitCode::FAILURE
                }
            }
        }
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
            let result = match socket::Client::connect_default() {
                Ok(mut c) => c.get(domain),
                Err(_) => registry.snapshot(domain).ok_or_else(|| format!("no domain {domain}")),
            };
            print_result(result)
        }
        Some("call") => {
            let (Some(domain), Some(action)) = (args.get(1), args.get(2)) else { return usage() };
            let call_args: Value = args.get(3).map(|a| serde_json::from_str(a).unwrap_or(json!({}))).unwrap_or(json!({}));
            let result = match socket::Client::connect_default() {
                Ok(mut c) => c.call(json!({"op": "call", "domain": domain, "action": action, "args": call_args})),
                Err(_) => registry.call(domain, action, &call_args, &ede_protocol::Capability::ALL).map_err(|e| e.to_string()),
            };
            print_result(result)
        }
        Some("grant") => {
            let Some(session) = args.get(1) else { return usage() };
            let label = args.get(2).cloned().unwrap_or_else(|| "agent".into());
            let caps: Vec<Value> = args.get(3).map(|c| c.split(',').map(|s| json!(s.trim())).collect()).unwrap_or_default();
            let result = socket::Client::connect_default().map_err(|e| format!("no daemon: {e}")).and_then(|mut c| c.call(json!({"op": "grant", "session": session, "label": label, "capabilities": caps})));
            print_result(result)
        }
        _ => usage(),
    }
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
