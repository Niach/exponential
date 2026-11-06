//! `exponential-theme sync|apply|remove|print [--files-only]` (#EDE-19).
//!
//! `sync` follows `[theme] bridge` in `compositor.toml` (the Theme page's
//! switch): apply when on, remove when off. exponential-theme.service runs it at
//! login before the shell; the shell runs it after the switch moves.

use std::process::ExitCode;

use ede_theme::{apply, remove, Choice, Installed, Options, Overrides, Palette, Paths, Plan};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = Options { files_only: args.iter().any(|a| a == "--files-only") };
    let command = args.iter().find(|a| !a.starts_with("--")).map(String::as_str).unwrap_or("print");
    let paths = Paths::from_env();
    let overrides = Overrides::read(&paths);
    let plan = Plan::build(&paths, &Palette::from_tokens(), Choice::resolve(&overrides, &Installed::scan(&paths)));
    let verb = match command {
        "sync" => if overrides.bridge { "apply" } else { "remove" },
        "apply" | "remove" | "print" => command,
        other => {
            eprintln!("unknown command {other:?}; usage: exponential-theme sync|apply|remove|print [--files-only]");
            return ExitCode::FAILURE;
        }
    };
    let result = match verb {
        "apply" => apply(&paths, &plan, options).map(|files| ("wrote", files)),
        "remove" => remove(&paths, &plan, options).map(|files| ("removed", files)),
        _ => {
            println!("{}", plan.choice.summary());
            println!("bridge {}", if overrides.bridge { "on" } else { "off" });
            for file in &plan.files {
                println!("{}", file.path.display());
            }
            for (k, v) in &plan.env {
                println!("{k}={v}");
            }
            return ExitCode::SUCCESS;
        }
    };
    match result {
        Ok((what, files)) => {
            println!("{}", plan.choice.summary());
            for file in files {
                println!("{what} {}", file.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("exponential-theme {verb}: {e}");
            ExitCode::FAILURE
        }
    }
}
