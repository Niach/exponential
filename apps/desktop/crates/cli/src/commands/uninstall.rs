//! `uninstall` — the mirror of install.sh: stop and remove the daemon
//! service, then delete this binary. Signed-in accounts and settings are
//! deliberately KEPT — the data dir is shared with the desktop app, so
//! wiping it would sign the desktop out too. `exponential logout` must run
//! while the binary still exists.

use std::process::ExitCode;

use anyhow::Context as _;

use super::{daemon, reject_unknown_flags, take_flag, CommandResult};
use crate::{context, term};

pub fn run(args: &[String]) -> CommandResult {
    let mut args = args.to_vec();
    let yes = take_flag(&mut args, "--yes") || take_flag(&mut args, "-y");
    reject_unknown_flags(&args)?;

    let exe = std::env::current_exe().context("resolve the running binary path")?;
    if !yes {
        if !term::stdin_is_tty() {
            anyhow::bail!("no terminal to confirm on — re-run with `--yes`");
        }
        println!(
            "This removes the daemon service (if installed) and deletes {}.",
            exe.display()
        );
        println!("Signed-in accounts are kept (the data dir is shared with the desktop app) —");
        println!("run `exponential logout` first if you want to sign out.");
        let answer = term::prompt_line("Uninstall? [y/N] ")?;
        if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
            println!("Aborted.");
            return Ok(ExitCode::FAILURE);
        }
    }

    daemon::remove_service()?;
    // The service stop covers managed daemons; a hand-started
    // `exponential daemon` keeps running from the unlinked inode.
    if let Some(pid) = daemon::daemon_pid(&context::data_dir()) {
        println!("A daemon is still running (pid {pid}) — stop it or it lives until reboot.");
    }
    std::fs::remove_file(&exe).with_context(|| format!("delete {}", exe.display()))?;
    println!("Deleted {}.", exe.display());
    println!("Reinstall any time: curl -fsSL https://exponential.at/install.sh | sh");
    Ok(ExitCode::SUCCESS)
}
