//! `exponential sweep-target [--if-low] [--dry-run] [<target-dir>]` — the
//! shared cargo target sweep (#EXP-1865, `coding::cargo_target_gc`) on
//! demand. `scripts/lanes/lane.sh` runs it with `--if-low` before a lane's
//! build: a no-op unless the target's disk is under `cargoSweep.freeDiskGb`
//! (and the setting is on). Without a dir it takes `$CARGO_TARGET_DIR`, else
//! every clone's shared target under the repos root. The sweep is recorded
//! for the doctor's `Build cache` row exactly like the daemon's.

use std::path::PathBuf;
use std::process::ExitCode;

use coding::cargo_target_gc::{self, Config};

use super::{reject_unknown_flags, take_flag, CommandResult};
use crate::context;

pub fn run(args: &[String]) -> CommandResult {
    let mut args = args.to_vec();
    let if_low = take_flag(&mut args, "--if-low");
    let dry_flag = take_flag(&mut args, "--dry-run");
    reject_unknown_flags(&args)?;
    if args.len() > 1 {
        anyhow::bail!("usage: exponential sweep-target [--if-low] [--dry-run] [<target-dir>]");
    }
    let data_dir = context::data_dir();
    let config = Config::load(&data_dir);
    if if_low && !config.enabled {
        return Ok(ExitCode::SUCCESS);
    }
    let targets: Vec<PathBuf> = match args.first() {
        Some(dir) => vec![PathBuf::from(dir)],
        None => match std::env::var_os("CARGO_TARGET_DIR").filter(|dir| !dir.is_empty()) {
            Some(dir) => vec![PathBuf::from(dir)],
            None => {
                let settings = coding::Settings::load(&coding::Settings::default_path(&data_dir));
                coding::scan_clones(&settings.repos_root_path())
                    .iter()
                    .map(|clone| coding::git_worktree::shared_cargo_target_dir(&clone.path))
                    .collect()
            }
        },
    };
    for target in targets {
        if !cargo_target_gc::is_cargo_target(&target) || (if_low && !config.is_low(&target)) {
            continue;
        }
        // It logs its one line (stderr + exponential-cli.log).
        cargo_target_gc::sweep_now(&target, &data_dir, dry_flag || config.dry_run);
    }
    Ok(ExitCode::SUCCESS)
}
