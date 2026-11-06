//! `exponential bundle` (#VAPP-270, E7): a signed package + an SVG icon +
//! bundle.toml → native binaries on the prebuilt runners
//! (`packages/eengine/fixtures/bundle.json`, crate `eengine-bundle`).
//! Offline except for fetching the pinned release's runner kits when no
//! `--runners` directory is given.

use std::path::PathBuf;
use std::process::ExitCode;

use eengine_bundle::Options;

use super::{reject_unknown_flags, take_flag, take_value, CommandResult};

const USAGE: &str = "\
Usage: exponential bundle <package.app.json> --icon <icon.svg> --config <bundle.toml> --target <targets>
                          [--runners <dir>] [--out <dir>] [--allow-unsigned]

Targets (comma separated): macos, windows, linux, android, ios, web

bundle.toml:
  id = \"com.example.photobook\"   # reverse-DNS: the bundle id, the app-data directory, the link scheme
  name = \"Photobook\"
  version = \"1.0.0\"
  network = \"off\"                # off (default) | upstreamsOnly | any
  [signing.<target>]              # the author's own keys; without them outputs are ad-hoc or unsigned

--runners <dir>   prebuilt runner kits <dir>/<target> (scripts/runner-kits.sh builds them from the workspace);
                  default: the pinned eengine release's assets
--out <dir>       default dist/
";

pub fn run(args: &[String]) -> CommandResult {
    let mut args = args.to_vec();
    if args.is_empty() || take_flag(&mut args, "--help") || take_flag(&mut args, "-h") {
        print!("{USAGE}");
        return Ok(if args.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(2) });
    }
    let icon = take_value(&mut args, "--icon");
    let config = take_value(&mut args, "--config");
    let targets = take_value(&mut args, "--target");
    let runners = take_value(&mut args, "--runners").map(PathBuf::from);
    let out = take_value(&mut args, "--out").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("dist"));
    let allow_unsigned = take_flag(&mut args, "--allow-unsigned");
    reject_unknown_flags(&args)?;
    let (Some(package), Some(icon), Some(config), Some(targets)) = (args.first(), icon, config, targets) else {
        print!("{USAGE}");
        return Ok(ExitCode::from(2));
    };
    let cache = dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("exponential").join("runner-kits");
    let opts = Options {
        package: PathBuf::from(package),
        icon: PathBuf::from(icon),
        config: PathBuf::from(config),
        targets: targets.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect(),
        runners,
        out,
        allow_unsigned,
        cache,
    };
    match eengine_bundle::run(&opts) {
        Ok(done) => {
            for (target, output) in done {
                for file in &output.files {
                    println!("{target}: {}", file.display());
                }
                for note in &output.notes {
                    println!("{target}: note: {note}");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            eprintln!("exponential bundle: {err:#}");
            Ok(ExitCode::from(1))
        }
    }
}
