//! `exponential-ui`: the ui directory from a terminal.

use std::{
    io::Read,
    path::PathBuf,
    process::ExitCode,
    sync::{Arc, Mutex},
};

use ede_ui::{Change, EditError, Runtime, THEME};
use serde_json::Value;

const USAGE: &str = "usage: exponential-ui [--dir DIR] [--factory DIR] <command>
  check                    load the directory; list what applies and what is refused
  validate FILE...         validate files without applying them (NAME.express = a document, theme.json = the theme)
  set NAME FILE|-          validate, write NAME.express and commit it (Express; a JSON document is
                           written as Express; NAME = theme: theme.json)
  rm NAME                  delete NAME.express as a commit
  discard NAME             drop NAME's uncommitted edit (back to the last commit)
  undo                     revert the newest edit
  reset                    every factory document back as it ships (your edits stay in the history)
  log [N]                  the history, newest first
  watch                    hot reload; one JSON change per line
  supervise -- CMD ARGS…   run the shell: watch, commit, frame-ack watchdog, crash revert
  digest FILE              the content digest a shell reports with a frame
  dir                      print the ui directory
DIR defaults to $EDE_UI_DIR, then $XDG_CONFIG_HOME/exponential-desktop/ui (~/.config/exponential-desktop/ui);
the factory directory to $EDE_FACTORY_DIR, then /usr/share/exponential-desktop/ui.
EDE_SAFE_MODE=1 (exponential-session's safe mode) supervises a scratch copy of the
factory documents instead of the person's directory, which stays untouched.";

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

fn read_bytes(path: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    if path == "-" {
        std::io::stdin().read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    } else {
        bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    }
    Ok(bytes)
}

fn read_json(path: &str) -> Result<Value, String> {
    let bytes = read_bytes(path)?;
    ede_ui::doc::parse(&bytes).map_err(|issues| issues.iter().map(|i| format!("{path}: {i}")).collect::<Vec<_>>().join("\n"))
}

/// What `set` writes for a document: Express as given, or a JSON document
/// (`{…}`) that is written as Express.
enum Source {
    Express(String),
    Json(Value),
}

fn read_document(path: &str) -> Result<Source, String> {
    let bytes = read_bytes(path)?;
    let text = String::from_utf8(bytes).map_err(|e| format!("{path}: not UTF-8 text: {e}"))?;
    if text.trim_start().starts_with('{') {
        return read_json_text(path, &text).map(Source::Json);
    }
    Ok(Source::Express(text))
}

fn read_json_text(path: &str, text: &str) -> Result<Value, String> {
    ede_ui::doc::parse(text.as_bytes()).map_err(|issues| issues.iter().map(|i| format!("{path}: {i}")).collect::<Vec<_>>().join("\n"))
}

/// #EDE-172 (P4): the channel `exponential-ui supervise` was started with
/// by the compositor (`EDE_CONTROL_FD`), taken before anything starts a
/// process; after its hello nothing else of the person's reads this
/// process. A supervisor the compositor's session should have started
/// that has none is not the one.
#[cfg(target_os = "linux")]
type Control = Option<ede_protocol::Client>;
#[cfg(not(target_os = "linux"))]
type Control = ();

#[cfg(target_os = "linux")]
fn take_control() -> Result<Control, String> {
    match ede_protocol::Client::inherited() {
        Some(Ok(client)) => {
            if let Err(e) = ede_protocol::process::refuse_ptrace() {
                eprintln!("exponential-ui: PR_SET_DUMPABLE: {e}");
            }
            Ok(Some(client))
        }
        Some(Err(e)) => Err(format!("the inherited channel: {e}")),
        None => Ok(None),
    }
}

#[cfg(not(target_os = "linux"))]
fn take_control() -> Result<Control, String> {
    Ok(())
}

fn run(dir: PathBuf, factory: PathBuf, args: &[String], control: Control) -> Result<bool, String> {
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let open = || {
        let opened = if factory.is_dir() { Runtime::open_with_factory(&dir, &factory) } else { Runtime::open(&dir) };
        opened.map_err(|e| format!("{}: {e}", dir.display()))
    };
    match (cmd, &args[1..]) {
        ("dir", []) => println!("{}", dir.display()),
        ("digest", [file]) => println!("{}", ede_ui::digest(&std::fs::read(file).map_err(|e| format!("{file}: {e}"))?)),
        ("supervise", rest) if rest.first().map(String::as_str) == Some("--") && rest.len() >= 2 => {
            #[cfg(target_os = "linux")]
            if control.is_none() && ede_protocol::process::helpers_spawned() {
                return Err("no channel: this session's compositor starts the shell's supervisor itself ([session] helpers = \"compositor\"); one started by hand is not it".into());
            }
            let (ui, changes) = open()?;
            print_changes(&changes);
            let supervisor = ede_ui::supervise::Supervisor::new(ui, rest[1..].to_vec(), |report| {
                #[cfg(feature = "diagnostics")]
                diagnostics::report(&report);
                println!("{report:?}");
            })
            .map_err(|e| e.to_string())?;
            #[cfg(target_os = "linux")]
            let mut supervisor = match control {
                Some(control) => supervisor.with_control(control),
                None => supervisor,
            };
            #[cfg(not(target_os = "linux"))]
            let mut supervisor = {
                let () = control;
                supervisor
            };
            let stop = std::sync::atomic::AtomicBool::new(false);
            supervisor.run(&stop).map_err(|e| e.to_string())?;
        }
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
                let result = if path.file_name().and_then(|f| f.to_str()) == Some(ede_ui::THEME_FILE) {
                    read_json(file).map_err(|e| vec![e]).and_then(|v| ede_ui::validate_theme(&v).map(|_| ()).map_err(|issues| issues.iter().map(ToString::to_string).collect()))
                } else if path.extension().is_some_and(|e| e == "json") {
                    Err(vec![format!("a shell document is Express now: {stem}.express (#EDE-216)")])
                } else {
                    read_bytes(file).map_err(|e| vec![e]).and_then(|b| ede_ui::doc::validate_bytes(&stem, &b).map(|_| ()).map_err(|issues| issues.iter().map(ToString::to_string).collect()))
                };
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
            let source = if name == THEME { Source::Json(read_json(file)?) } else { read_document(file)? };
            let (mut ui, _) = open()?;
            let message = format!("Set {name}");
            let result = match source {
                Source::Json(source) if name == THEME => ui.write_theme(&source, &message),
                Source::Json(source) => ui.write(name, &source, &message),
                Source::Express(source) => ui.write_express(name, &source, &message),
            };
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
        ("reset", []) => {
            let (mut ui, _) = open()?;
            print_changes(&ui.reset_to_factory().map_err(|e| e.to_string())?);
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

/// Safe mode (#EDE-2): the stock shell from a scratch directory under
/// `$XDG_RUNTIME_DIR` (else the temp dir), seeded from the factory copies,
/// so a broken ~/.config/exponential-desktop/ui never keeps the person out and is never
/// rewritten behind their back.
fn safe_mode_dir() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let dir = base.join("exponential-desktop").join("safe-ui");
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// #ERR-29: the supervisor's diagnostics, the compositor's own
/// (ede-observability): its panics and native crashes, and the shell's
/// crashes as errors. Every other command is a one-shot and starts none.
#[cfg(feature = "diagnostics")]
mod diagnostics {
    use ede_ui::supervise::{shell_failed, Report};

    pub fn start() -> ede_observability::Guard {
        let guard = ede_observability::init("ui-supervise", "info", ede_observability::Output::Stderr);
        ede_observability::watch_native_crashes("ui-supervise");
        guard
    }

    /// A shell that crashed is an error (one GlitchTip issue, the status a
    /// field); one the session stopped is a warning.
    pub fn report(report: &Report) {
        match report {
            Report::Crashed { status } => {
                let how = status.map_or_else(|| "unknown".to_string(), |s| s.to_string());
                if shell_failed(*status) {
                    tracing::error!(status = %how, "the shell crashed; starting it again");
                } else {
                    tracing::warn!(status = %how, "the shell ended; starting it again");
                }
            }
            Report::Reverted { .. } | Report::Factory { .. } => tracing::warn!(?report, "a document was rolled back after shell crashes"),
            _ => {}
        }
    }

    /// The supervisor gave up (the shell kept dying, the ui directory
    /// would not open). What exactly is on stderr: it names a path under
    /// the person's home.
    pub fn failed() {
        tracing::error!("the shell supervisor stopped");
        ede_observability::flush();
    }
}

fn main() -> ExitCode {
    // A crash-monitor process (ede-observability) runs and exits here.
    #[cfg(feature = "diagnostics")]
    ede_observability::run_crash_monitor_if_requested();
    // #EDE-172 (P4): before the crash monitor or anything else starts.
    let control = match take_control() {
        Ok(control) => control,
        Err(e) => {
            eprintln!("exponential-ui: {e}");
            return ExitCode::from(2);
        }
    };
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let safe_mode = std::env::var_os("EDE_SAFE_MODE").is_some_and(|v| !v.is_empty() && v != "0");
    let mut dir = if safe_mode { safe_mode_dir() } else { ede_ui::default_dir() };
    let mut factory = ede_ui::default_factory_dir();
    loop {
        match (args.first().map(String::as_str), args.get(1)) {
            (Some("--dir"), Some(d)) => dir = PathBuf::from(d),
            (Some("--factory"), Some(f)) => factory = PathBuf::from(f),
            _ => break,
        }
        args.drain(..2);
    }
    if safe_mode {
        eprintln!("exponential-ui: safe mode, the factory documents from {}", dir.display());
    }
    if matches!(args.first().map(String::as_str), Some("-h" | "--help")) {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    #[cfg(feature = "diagnostics")]
    let supervising = args.first().map(String::as_str) == Some("supervise");
    #[cfg(feature = "diagnostics")]
    let _diagnostics = supervising.then(diagnostics::start);
    match run(dir, factory, &args, control) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("exponential-ui: {e}");
            #[cfg(feature = "diagnostics")]
            if supervising {
                diagnostics::failed();
            }
            ExitCode::from(2)
        }
    }
}
