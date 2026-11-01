//! `ede-ui`: the ui directory from a terminal.

use std::{
    io::Read,
    path::PathBuf,
    process::ExitCode,
    sync::{Arc, Mutex},
};

use ede_ui::{Change, EditError, Runtime, THEME};
use serde_json::Value;

const USAGE: &str = "usage: ede-ui [--dir DIR] <command>
  check                    load the directory; list what applies and what is refused
  validate FILE...         validate files without applying them (theme.json = the theme)
  set NAME FILE|-          validate, write NAME.json (NAME = theme: theme.json) and commit it
  rm NAME                  delete NAME.json as a commit
  discard NAME             drop NAME's uncommitted edit (back to the last commit)
  undo                     revert the newest edit
  log [N]                  the history, newest first
  watch                    hot reload; one JSON change per line
  dir                      print the ui directory
DIR defaults to $XDG_CONFIG_HOME/ede/ui (~/.config/ede/ui).";

fn print_changes(changes: &[Change]) {
    for c in changes {
        match c {
            Change::Applied { name, commit } => println!("ok       {name}{}", commit.as_ref().map(|c| format!("  ({})", &c[..8])).unwrap_or_default()),
            Change::Removed { name, .. } => println!("removed  {name}"),
            Change::ThemeApplied { id, builtin, .. } => println!("ok       theme {id}{}", if *builtin { " (built-in)" } else { "" }),
            Change::Rejected { name, issues, fallback } => {
                println!("refused  {name}{}", if *fallback { " (last good version stays)" } else { "" });
                issues.iter().for_each(|i| println!("           {i}"));
            }
            Change::ThemeRejected { issues, fallback } => {
                println!("refused  theme (keeps {fallback})");
                issues.iter().for_each(|i| println!("           {i}"));
            }
        }
    }
}

fn read_json(path: &str) -> Result<Value, String> {
    let mut bytes = Vec::new();
    if path == "-" {
        std::io::stdin().read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    } else {
        bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    }
    ede_ui::doc::parse(&bytes).map_err(|issues| issues.iter().map(|i| format!("{path}: {i}")).collect::<Vec<_>>().join("\n"))
}

fn run(dir: PathBuf, args: &[String]) -> Result<bool, String> {
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let open = || Runtime::open(&dir).map_err(|e| format!("{}: {e}", dir.display()));
    match (cmd, &args[1..]) {
        ("dir", []) => println!("{}", dir.display()),
        ("check", []) => {
            let (ui, changes) = open()?;
            print_changes(&changes);
            return Ok(ui.rejected().is_empty() && ui.theme_rejected().is_none());
        }
        ("validate", files) if !files.is_empty() => {
            let mut ok = true;
            for file in files {
                let path = PathBuf::from(file);
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("document").to_string();
                let result = read_json(file).map_err(|e| vec![e]).and_then(|v| {
                    let checked = if path.file_name().and_then(|f| f.to_str()) == Some(ede_ui::THEME_FILE) {
                        ede_ui::validate_theme(&v).map(|_| ())
                    } else {
                        ede_ui::validate_document(&stem, &v).map(|_| ())
                    };
                    checked.map_err(|issues| issues.iter().map(ToString::to_string).collect())
                });
                match result {
                    Ok(()) => println!("ok       {file}"),
                    Err(issues) => {
                        ok = false;
                        println!("refused  {file}");
                        issues.iter().for_each(|i| println!("           {i}"));
                    }
                }
            }
            return Ok(ok);
        }
        ("set", [name, file]) => {
            let source = read_json(file)?;
            let (mut ui, _) = open()?;
            let message = format!("Set {name}");
            let result = if name == THEME { ui.write_theme(&source, &message) } else { ui.write(name, &source, &message) };
            match result {
                Ok(changes) => print_changes(&changes),
                Err(EditError::Invalid(issues)) => {
                    println!("refused  {name} (nothing written)");
                    issues.iter().for_each(|i| println!("           {i}"));
                    return Ok(false);
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        ("rm", [name]) => {
            let (mut ui, _) = open()?;
            print_changes(&ui.remove(name, &format!("Remove {name}")).map_err(|e| e.to_string())?);
        }
        ("discard", [name]) => {
            let (mut ui, _) = open()?;
            print_changes(&ui.discard(name).map_err(|e| e.to_string())?);
        }
        ("undo", []) => {
            let (mut ui, _) = open()?;
            match ui.undo().map_err(|e| e.to_string())? {
                None => println!("nothing to undo"),
                Some((commit, changes)) => {
                    println!("{} {}", &commit.id[..8], commit.summary);
                    print_changes(&changes);
                }
            }
        }
        ("log", rest) if rest.len() <= 1 => {
            let n = rest.first().map(|n| n.parse().map_err(|_| format!("not a number: {n}"))).transpose()?.unwrap_or(20);
            let (ui, _) = open()?;
            for c in ui.log(n).map_err(|e| e.to_string())? {
                println!("{} {}", &c.id[..8], c.summary);
            }
        }
        ("watch", []) => {
            let (ui, changes) = open()?;
            let line = |c: &Change| println!("{}", serde_json::to_string(c).unwrap_or_default());
            changes.iter().for_each(line);
            let _watcher = ede_ui::watch(Arc::new(Mutex::new(ui)), move |changes| changes.iter().for_each(line)).map_err(|e| e.to_string())?;
            loop {
                std::thread::park();
            }
        }
        _ => {
            eprintln!("{USAGE}");
            return Ok(false);
        }
    }
    Ok(true)
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut dir = ede_ui::default_dir();
    if args.first().map(String::as_str) == Some("--dir") && args.len() >= 2 {
        dir = PathBuf::from(&args[1]);
        args.drain(..2);
    }
    if matches!(args.first().map(String::as_str), Some("-h" | "--help")) {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match run(dir, &args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("ede-ui: {e}");
            ExitCode::from(2)
        }
    }
}
