//! `exponential code <ISSUE>` — the desktop's Start-coding flow, headless:
//! resolve the issue over tRPC, run the ONE shared launcher
//! (`coding::prepare_with_hooks`), spawn the agent TUI on a real PTY. With
//! a tty the PTY attaches raw (use it exactly like the desktop terminal);
//! detached it runs to completion, steerable from the web.

use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use coding::{Prepared, PrepareRequest};

use super::{reject_unknown_flags, take_flag, take_value, CommandResult};
use crate::launch::{self, AgentFlags};
use crate::session_host::{self, LaunchEnv, RunningSession};
use crate::sidecars::Sidecars;
use crate::{context, term};

pub fn run(args: &[String]) -> CommandResult {
    let mut args = args.to_vec();
    let flags = AgentFlags {
        agent: take_value(&mut args, "--agent"),
        model: take_value(&mut args, "--model"),
        effort: take_value(&mut args, "--effort"),
        plan: take_flag(&mut args, "--plan"),
        skip_permissions: take_flag(&mut args, "--skip-permissions"),
    };
    let detach = take_flag(&mut args, "--detach");
    reject_unknown_flags(&args)?;
    let Some(issue_ref) = args.first() else {
        anyhow::bail!("usage: exponential code <ISSUE> [--agent claude|codex|pi] [--model m] [--effort e] [--plan] [--skip-permissions] [--detach]");
    };

    let ctx = context::load()?;
    let interactive = !detach && term::stdin_is_tty() && term::stdout_is_tty();
    let options = launch::agent_options(&ctx.settings, &flags, interactive)?;

    let fetched = api::issues::issues_get(&ctx.trpc, issue_ref)
        .with_context(|| format!("resolve issue `{issue_ref}`"))?;
    let issue = fetched.issue;
    println!("Starting {} — {}", issue.identifier, issue.title);

    let request = launch::issue_launch_request(&issue, options, coding::LaunchOrigin::Local, false);
    let mut seeds = HashMap::new();
    seeds.insert(issue.id.clone(), launch::issue_seed(&issue));
    let deps = launch::coding_deps(&ctx, seeds);

    let sidecars = Sidecars::start();
    let runtime = steer::SteerRuntime::new().ok();
    let personal_key = context::ensure_personal_key(&ctx).ok();

    let prepared = coding::prepare_with_hooks(
        &PrepareRequest::Issue(request),
        &deps,
        sidecars.hook_setup().as_ref(),
        sidecars.observer_setup().as_ref(),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;
    let prepared = match prepared {
        Prepared::Ready(prepared) => prepared,
        Prepared::Disabled(reason) => {
            eprintln!("Can't start coding: {}", reason.message());
            return Ok(ExitCode::FAILURE);
        }
    };

    let env = LaunchEnv {
        ctx: &ctx,
        runtime: runtime.as_ref(),
        sidecars: &sidecars,
        personal_key,
    };
    let session = session_host::launch(&env, prepared, interactive, Some(issue.id.clone()))?;
    let session = Arc::new(session);
    println!(
        "Session {} on branch {} (worktree {})",
        session.session_id,
        session.branch,
        session.worktree.display()
    );

    if interactive {
        attend(&session)
    } else {
        println!("Detached — steer it from the web.");
        wait_with_signals(&session)
    }
}

/// Interactive attach: raw-mode stdin straight into the PTY (the same
/// shared writer remote steer input uses), output mirrored by the session
/// host's tee, local resizes forwarded. Shared with `run`.
pub fn attend(session: &Arc<RunningSession>) -> CommandResult {
    let raw = term::RawMode::enter();

    // stdin pump — reads stay blocking; the thread dies with the process.
    {
        let session = Arc::clone(session);
        std::thread::spawn(move || {
            use std::io::Read as _;
            let mut stdin = std::io::stdin().lock();
            let mut buf = [0u8; 1024];
            loop {
                match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => session.write_stdin(&buf[..n]),
                }
            }
        });
    }
    // Resize watcher — polling beats signal plumbing here and reacts fast
    // enough for a human dragging a window.
    {
        let session = Arc::clone(session);
        let mut last = term::window_size();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(400));
            let now = term::window_size();
            if now != last {
                if let Some((cols, rows)) = now {
                    session.resize(cols, rows);
                }
                last = now;
            }
        });
    }

    let exit = session.wait();
    drop(raw);
    println!();
    println!("Session ended (exit {}).", exit.code);
    Ok(if exit.success { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// Detached wait: Ctrl-C / SIGTERM end the session cleanly (kill the child,
/// end the row) instead of orphaning a `running` badge.
pub fn wait_with_signals(session: &Arc<RunningSession>) -> CommandResult {
    crate::commands::daemon::install_signal_handler();
    loop {
        if crate::commands::daemon::shutdown_requested() {
            eprintln!("Stopping the session...");
            session.kill();
            let exit = session.wait();
            return Ok(if exit.success { ExitCode::SUCCESS } else { ExitCode::FAILURE });
        }
        if let Some(exit) = session.wait_timeout(Duration::from_millis(500)) {
            println!("Session ended (exit {}).", exit.code);
            return Ok(if exit.success { ExitCode::SUCCESS } else { ExitCode::FAILURE });
        }
    }
}
