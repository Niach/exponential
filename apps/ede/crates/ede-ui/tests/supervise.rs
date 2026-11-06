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
        let (ui, _) = Runtime::open_with_factory(dir, factory).unwrap();
        let (tx, reports) = mpsc::channel();
        let mut supervisor = Supervisor::new(ui, vec![fake_shell(dir.parent().unwrap())], move |r| {
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
    assert_eq!(seen[after_edit..].iter().filter(|r| matches!(r, Report::Crashed)).count(), 3, "{seen:?}");
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
