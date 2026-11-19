//! The supervisor against a fake shell: a script that reports a frame for
//! every document it "draws" (by content digest), crashes on a document
//! that says so, and stays silent on one that says so.

use std::{
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

use ede_ui::supervise::{Report, Supervisor};
use ede_ui::Runtime;
use serde_json::{json, Value};

fn panel(text: &str) -> Value {
    json!({
        "surface": {"anchor": ["top", "left", "right"], "size": {"height": 40}, "exclusiveZone": 40},
        "root": {"id": "panel", "component": "Box", "children": [{"id": "clock", "component": "Text", "props": {"text": text}}]}
    })
}

/// The fake shell: polls the documents, prints a frame line when one
/// changes, exits 1 when the panel says "crash", says nothing while it says
/// "silent".
fn fake_shell(dir: &Path) -> String {
    let script = dir.join("fake-shell.sh");
    let ede_ui = env!("CARGO_BIN_EXE_exponential-ui");
    fs::write(
        &script,
        format!(
            r#"#!/bin/bash
cd "$EDE_UI_DIR" || exit 2
declare -A seen
while true; do
  for f in *.json; do
    [ -e "$f" ] || continue
    name="${{f%.json}}"
    d=$("{ede_ui}" digest "$f")
    if grep -q '"crash"' "$f"; then exit 1; fi
    if [ "${{seen[$name]}}" != "$d" ]; then
      seen[$name]="$d"
      if ! grep -q '"silent"' "$f"; then echo "{{\"frame\":{{\"name\":\"$name\",\"digest\":\"$d\"}}}}"; fi
    fi
  done
  sleep 0.05
done
"#
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    script.to_string_lossy().into_owned()
}

struct Run {
    stop: Arc<AtomicBool>,
    reports: mpsc::Receiver<Report>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Run {
    fn start(dir: &Path, factory: &Path) -> Run {
        Run::start_with(dir, factory, vec![fake_shell(dir.parent().unwrap())])
    }

    /// [`Run::start`] with `command` as the shell.
    fn start_with(dir: &Path, factory: &Path, command: Vec<String>) -> Run {
        let (ui, _) = Runtime::open_with_factory(dir, factory).unwrap();
        let (tx, reports) = mpsc::channel();
        let mut supervisor = Supervisor::new(ui, command, move |r| {
            let _ = tx.send(r);
        })
        .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || supervisor.run(&flag).expect("the supervisor runs"));
        Run { stop, reports, thread: Some(thread) }
    }

    /// Waits up to `timeout` for a report `pred` accepts, returning every report seen.
    fn wait(&self, timeout: Duration, pred: impl Fn(&Report) -> bool) -> Vec<Report> {
        let deadline = Instant::now() + timeout;
        let mut seen = Vec::new();
        while Instant::now() < deadline {
            match self.reports.recv_timeout(Duration::from_millis(50)) {
                Ok(r) => {
                    let done = pred(&r);
                    seen.push(r);
                    if done {
                        return seen;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        panic!("no such report within {timeout:?} (elapsed {:?}); saw {seen:?}", timeout - deadline.saturating_duration_since(Instant::now()));
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn setup() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let factory = tmp.path().join("factory");
    fs::create_dir_all(&factory).unwrap();
    fs::write(factory.join("panel.json"), serde_json::to_vec_pretty(&panel("factory")).unwrap()).unwrap();
    let dir = tmp.path().join("ui");
    (tmp, dir, factory)
}

fn log(dir: &Path) -> Vec<String> {
    let out = std::process::Command::new("git").args(["-C", dir.to_str().unwrap(), "log", "--format=%s"]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).lines().map(String::from).collect()
}

fn text(dir: &Path) -> String {
    let v: Value = serde_json::from_slice(&fs::read(dir.join("panel.json")).unwrap()).unwrap();
    v["root"]["children"][0]["props"]["text"].as_str().unwrap().to_string()
}

#[test]
fn the_factory_copy_arrives_as_the_first_commit() {
    let (_tmp, dir, factory) = setup();
    let (ui, changes) = Runtime::open_with_factory(&dir, &factory).unwrap();
    assert_eq!(ui.factory_names().unwrap(), ["panel"]);
    assert!(changes.iter().any(|c| matches!(c, ede_ui::Change::Applied { name, commit: Some(_) } if name == "panel")));
    assert_eq!(log(&dir), ["Factory panel", "exponential-ui: history starts"]);
    assert_eq!(text(&dir), "factory");
    // A second open copies nothing again, and a deleted document is not resurrected.
    let (_, changes) = Runtime::open_with_factory(&dir, &factory).unwrap();
    assert!(changes.iter().all(|c| !matches!(c, ede_ui::Change::Applied { commit: Some(_), .. })));
    assert_eq!(log(&dir).len(), 2);
}

#[test]
fn an_update_reaches_untouched_documents_and_spares_edited_ones() {
    let (_tmp, dir, factory) = setup();
    Runtime::open_with_factory(&dir, &factory).unwrap();
    // The package ships a new panel: the untouched copy follows it.
    fs::write(factory.join("panel.json"), serde_json::to_vec_pretty(&panel("factory 2")).unwrap()).unwrap();
    Runtime::open_with_factory(&dir, &factory).unwrap();
    assert_eq!(text(&dir), "factory 2");
    assert_eq!(log(&dir), ["Factory panel", "Factory panel", "exponential-ui: history starts"]);
    // The person edits it; the next update leaves their copy alone.
    let (mut ui, _) = Runtime::open_with_factory(&dir, &factory).unwrap();
    ui.write("panel", &panel("mine"), "Edit panel.json").unwrap();
    fs::write(factory.join("panel.json"), serde_json::to_vec_pretty(&panel("factory 3")).unwrap()).unwrap();
    Runtime::open_with_factory(&dir, &factory).unwrap();
    assert_eq!(text(&dir), "mine");
    assert_eq!(log(&dir)[0], "Edit panel.json");
}

#[test]
fn a_drawn_edit_stays() {
    let (_tmp, dir, factory) = setup();
    let run = Run::start(&dir, &factory);
    run.wait(Duration::from_secs(5), |r| matches!(r, Report::Started { .. }));
    fs::write(dir.join("panel.json"), serde_json::to_vec_pretty(&panel("edited")).unwrap()).unwrap();
    let seen = run.wait(Duration::from_secs(5), |r| matches!(r, Report::Drawn { name, .. } if name == "panel"));
    assert!(seen.iter().any(|r| matches!(r, Report::Edit { name, .. } if name == "panel")), "{seen:?}");
    std::thread::sleep(Duration::from_millis(2600));
    assert_eq!(text(&dir), "edited");
    assert_eq!(log(&dir)[0], "Edit panel.json");
}

#[test]
fn an_edit_nobody_draws_in_two_seconds_rolls_back() {
    let (_tmp, dir, factory) = setup();
    let run = Run::start(&dir, &factory);
    run.wait(Duration::from_secs(5), |r| matches!(r, Report::Started { .. }));
    fs::write(dir.join("panel.json"), serde_json::to_vec_pretty(&panel("silent")).unwrap()).unwrap();
    let started = Instant::now();
    run.wait(Duration::from_secs(6), |r| matches!(r, Report::Reverted { name, .. } if name == "panel"));
    assert!(started.elapsed() >= Duration::from_millis(1900), "rolled back too early: {:?}", started.elapsed());
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(text(&dir), "factory");
    assert_eq!(log(&dir)[0], "Undo: Edit panel.json, rolled back");
}

#[test]
fn three_crashes_after_an_edit_revert_it() {
    let (_tmp, dir, factory) = setup();
    let run = Run::start(&dir, &factory);
    run.wait(Duration::from_secs(5), |r| matches!(r, Report::Started { .. }));
    fs::write(dir.join("panel.json"), serde_json::to_vec_pretty(&panel("crash")).unwrap()).unwrap();
    let seen = run.wait(Duration::from_secs(10), |r| matches!(r, Report::Reverted { name, .. } if name == "panel"));
    // Crashes before the edit was committed do not count (the panel was still the factory's).
    let after_edit = seen.iter().position(|r| matches!(r, Report::Edit { .. })).expect("the edit");
    assert_eq!(seen[after_edit..].iter().filter(|r| matches!(r, Report::Crashed { .. })).count(), 3, "{seen:?}");
    // #ERR-29: each with the status the shell ended with (the fake's `exit 1`).
    for r in &seen[after_edit..] {
        if let Report::Crashed { status } = r {
            assert_eq!(status.and_then(|s| s.code()), Some(1), "{r:?}");
            assert!(ede_ui::supervise::shell_failed(*status));
        }
    }
    // The shell comes back and draws the reverted panel.
    run.wait(Duration::from_secs(5), |r| matches!(r, Report::Started { .. }));
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(text(&dir), "factory");
}

#[test]
fn a_broken_edit_is_refused_and_never_committed() {
    let (_tmp, dir, factory) = setup();
    let run = Run::start(&dir, &factory);
    run.wait(Duration::from_secs(5), |r| matches!(r, Report::Started { .. }));
    fs::write(dir.join("panel.json"), br#"{"surface": {"size": {"width": 1, "height": 1}}, "root": {"id": "panel", "component": "Nope"}}"#).unwrap();
    run.wait(Duration::from_secs(5), |r| matches!(r, Report::Refused { name } if name == "panel"));
    assert_eq!(log(&dir), ["Factory panel", "exponential-ui: history starts"]);
}

/// `exponential-ui --dir <dir> set panel -`: the agent's shell_edit, another process that writes and commits at once.
fn cli_set(dir: &Path, factory: &Path, doc: &Value) {
    use std::io::Write;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_exponential-ui"))
        .arg("--dir")
        .arg(dir)
        .args(["set", "panel", "-"])
        .env("EDE_FACTORY_DIR", factory)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&serde_json::to_vec_pretty(doc).unwrap()).unwrap();
    assert!(child.wait().unwrap().success());
}

#[test]
fn an_edit_through_the_cli_is_supervised() {
    let (_tmp, dir, factory) = setup();
    let run = Run::start(&dir, &factory);
    run.wait(Duration::from_secs(5), |r| matches!(r, Report::Started { .. }));
    cli_set(&dir, &factory, &panel("crash"));
    let seen = run.wait(Duration::from_secs(10), |r| matches!(r, Report::Reverted { name, .. } if name == "panel"));
    assert!(seen.iter().any(|r| matches!(r, Report::Edit { name, .. } if name == "panel")), "{seen:?}");
    run.wait(Duration::from_secs(5), |r| matches!(r, Report::Started { .. }));
    assert_eq!(text(&dir), "factory");
    assert_eq!(log(&dir)[0], "Undo: Edit panel.json, rolled back");
}

#[test]
fn a_cli_edit_nobody_draws_rolls_back() {
    let (_tmp, dir, factory) = setup();
    let run = Run::start(&dir, &factory);
    run.wait(Duration::from_secs(5), |r| matches!(r, Report::Started { .. }));
    cli_set(&dir, &factory, &panel("silent"));
    run.wait(Duration::from_secs(6), |r| matches!(r, Report::Reverted { name, .. } if name == "panel"));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(text(&dir), "factory");
}

#[test]
fn an_edit_from_an_earlier_run_is_still_a_suspect() {
    let (_tmp, dir, factory) = setup();
    {
        let (mut ui, _) = Runtime::open_with_factory(&dir, &factory).unwrap();
        ui.write("panel", &panel("crash"), "Edit panel.json").unwrap();
    }
    let run = Run::start(&dir, &factory);
    let seen = run.wait(Duration::from_secs(10), |r| matches!(r, Report::Reverted { name, .. } if name == "panel"));
    assert_eq!(seen.iter().filter(|r| matches!(r, Report::Crashed { .. })).count(), 3, "{seen:?}");
    run.wait(Duration::from_secs(5), |r| matches!(r, Report::Started { .. }));
    assert_eq!(text(&dir), "factory");
}

#[test]
fn an_unchanged_factory_document_from_an_earlier_run_is_no_suspect() {
    let (_tmp, dir, factory) = setup();
    Runtime::open_with_factory(&dir, &factory).unwrap();
    let (ui, _) = Runtime::open_with_factory(&dir, &factory).unwrap();
    let supervisor = Supervisor::new(ui, vec!["true".into()], |_| {}).unwrap();
    assert!(supervisor.state().documents["panel"].from_factory);
}

/// `exponential-ui --dir <dir> undo`: Version history's undo, another process.
fn cli_undo(dir: &Path, factory: &Path) {
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_exponential-ui"))
        .arg("--dir")
        .arg(dir)
        .arg("undo")
        .env("EDE_FACTORY_DIR", factory)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
}

/// #EDE-116 end to end: an edit undone through the CLI before its frame is
/// no rollback, the supervisor runs on, and the next edit is still
/// supervised. Here the watcher brings the undo before the deadline; the
/// order that stopped the supervisor (the deadline first) is pinned by
/// the unit tests in `src/supervise.rs`.
#[test]
fn an_edit_undone_before_its_frame_is_no_rollback() {
    let (_tmp, dir, factory) = setup();
    let run = Run::start(&dir, &factory);
    run.wait(Duration::from_secs(5), |r| matches!(r, Report::Started { .. }));
    cli_set(&dir, &factory, &panel("silent"));
    run.wait(Duration::from_secs(5), |r| matches!(r, Report::Edit { name, .. } if name == "panel"));
    let edited = Instant::now();
    cli_undo(&dir, &factory);
    if edited.elapsed() > Duration::from_millis(1500) {
        // A machine too loaded to undo within the 2 s wait: not this case.
        eprintln!("skipped: the undo took {:?}", edited.elapsed());
        return;
    }
    std::thread::sleep(Duration::from_millis(2600));
    let seen: Vec<Report> = run.reports.try_iter().collect();
    assert!(!seen.iter().any(|r| matches!(r, Report::Reverted { .. })), "the undone edit was rolled back: {seen:?}");
    assert_eq!(text(&dir), "factory");
    assert!(!log(&dir).iter().any(|s| s.ends_with("rolled back")), "{:?}", log(&dir));
    // Another silent edit: still supervised, so rolled back.
    let mut again = panel("silent");
    again["root"]["children"][0]["id"] = json!("clock-2");
    cli_set(&dir, &factory, &again);
    run.wait(Duration::from_secs(8), |r| matches!(r, Report::Reverted { name, .. } if name == "panel"));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(text(&dir), "factory");
    assert_eq!(log(&dir)[0], "Undo: Edit panel.json, rolled back");
}

/// #EDE-150: the apps a shell launched inherit its stdout. A shell that
/// dies while one of them runs is seen at once (and started again), not
/// when that app ends; the pipe's late end is not the next shell's; and
/// the supervisor goes on.
#[test]
fn a_shell_that_dies_while_its_apps_run_is_started_again() {
    let (tmp, dir, factory) = setup();
    let marker = tmp.path().join("crashed-once");
    // The first shell leaves an "app" holding its stdout for 7 s and dies;
    // the next one lives.
    let script = format!(r#"if [ -e "{m}" ]; then exec sleep 30; fi; : > "{m}"; sleep 7 & exit 1"#, m = marker.display());
    let began = Instant::now();
    let run = Run::start_with(&dir, &factory, vec!["sh".into(), "-c".into(), script]);
    let seen = run.wait(Duration::from_secs(4), |r| matches!(r, Report::Crashed { .. }));
    assert_eq!(seen.iter().filter(|r| matches!(r, Report::Started { .. })).count(), 1, "{seen:?}");
    assert!(matches!(seen.last(), Some(Report::Crashed { status }) if status.and_then(|s| s.code()) == Some(1)), "{seen:?}");
    run.wait(Duration::from_secs(10), |r| matches!(r, Report::Started { .. }));
    // The first shell's pipe ends when its app does (at 7 s): the shell
    // that runs now is not taken for it.
    while began.elapsed() < Duration::from_millis(8500) {
        if let Ok(report) = run.reports.recv_timeout(Duration::from_millis(100)) {
            assert!(!matches!(report, Report::Crashed { .. } | Report::Started { .. }), "the living shell was taken for the dead one: {report:?}");
        }
    }
    // And the supervisor is not stuck waiting on it: an edit is committed.
    fs::write(dir.join("panel.json"), serde_json::to_vec_pretty(&panel("edited")).unwrap()).unwrap();
    run.wait(Duration::from_secs(10), |r| matches!(r, Report::Edit { name, .. } if name == "panel"));
}

/// #ERR-29: `exponential-ui supervise` starts diagnostics (the compositor's
/// ede-observability) and reports a shell that died of a signal as an error,
/// the level that becomes a GlitchTip event; diagnostics stay off here.
#[test]
fn the_supervisor_logs_a_crashed_shell_as_an_error() {
    use std::io::BufRead;
    let (tmp, dir, factory) = setup();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_exponential-ui"))
        .args(["--dir", dir.to_str().unwrap(), "--factory", factory.to_str().unwrap(), "supervise", "--", "sh", "-c", "ulimit -c 0; kill -SEGV $$"])
        .env("EXP_DIAGNOSTICS", "0")
        .env("NO_COLOR", "1")
        .env("RUST_LOG", "info")
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let (tx, rx) = mpsc::channel();
    let stderr = child.stderr.take().unwrap();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stderr).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                return;
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut seen = Vec::new();
    let found = loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(line) if line.contains("ERROR") && line.contains("the shell crashed") => break Some(line),
            Ok(line) => seen.push(line),
            Err(_) if Instant::now() > deadline => break None,
            Err(_) => {}
        }
    };
    let _ = child.kill();
    let _ = child.wait();
    let line = found.unwrap_or_else(|| panic!("no error for the crashed shell: {seen:#?}"));
    assert!(line.contains("SIGSEGV") || line.contains("signal: 11") || line.contains("139"), "{line}");
}

/// #EDE-153: a git operation that fails while the supervisor carries out
/// an effect (here its rollback: another git holds the repository past the
/// wait) does not end it: the next edit is still supervised. Before, `run`
/// ended on the error and nothing was supervised for the rest of the session.
#[test]
fn a_failed_rollback_does_not_end_the_supervisor() {
    let (_tmp, dir, factory) = setup();
    let run = Run::start(&dir, &factory);
    run.wait(Duration::from_secs(5), |r| matches!(r, Report::Started { .. }));
    cli_set(&dir, &factory, &panel("silent"));
    run.wait(Duration::from_secs(5), |r| matches!(r, Report::Edit { name, .. } if name == "panel"));
    // Another git holds the repository past the rollback's due time and git's own wait.
    let locks = [dir.join(".git/index.lock"), dir.join(".git/HEAD.lock")];
    for lock in &locks {
        fs::write(lock, b"").unwrap();
    }
    std::thread::sleep(Duration::from_secs(6));
    for lock in &locks {
        fs::remove_file(lock).unwrap();
    }
    // Another silent edit: still supervised, so rolled back.
    let mut again = panel("silent");
    again["root"]["children"][0]["id"] = json!("clock-2");
    cli_set(&dir, &factory, &again);
    run.wait(Duration::from_secs(10), |r| matches!(r, Report::Reverted { name, .. } if name == "panel"));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(text(&dir), "factory");
}
