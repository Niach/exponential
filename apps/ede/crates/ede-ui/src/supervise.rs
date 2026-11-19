//! The supervisor (#EDE-9): runs the shell, owns the history, and carries
//! out the apply machine ([`crate::apply`]). It watches the ui directory
//! (commits hand edits, refuses broken ones), reads the shell's frame lines
//! (`{"frame": {"name": "panel", "digest": "…"}}` on stdout, one per
//! document it drew), counts shell exits as crashes and restarts the shell,
//! and when the machine says so reverts a commit or puts the factory copy
//! back. The shell runs with `EDE_UI_DIR` pointing at the directory and a
//! read-only runtime of its own ([`crate::Runtime::open_readonly`]).

use std::{
    collections::HashMap,
    io::{self, BufRead, BufReader},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    apply::{self, ApplyState, Effect, Event},
    Change, Runtime,
};

/// How long after a crash the shell starts again.
pub const RESTART_DELAY: Duration = Duration::from_millis(500);
/// A shell that dies within this of its start is a quick exit; after
/// [`MAX_QUICK_EXITS`] in a row the supervisor gives up (the display is gone,
/// or nothing the machine can revert is at fault).
pub const QUICK_EXIT: Duration = Duration::from_secs(5);
pub const MAX_QUICK_EXITS: u32 = 10;
/// The clock the machine's deadlines run on.
pub const TICK: Duration = Duration::from_millis(500);

/// What the shell writes on stdout after it drew a document.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FrameLine {
    pub frame: Frame,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Frame {
    pub name: String,
    pub digest: String,
}

impl FrameLine {
    /// The line the shell prints for `name` drawn from `bytes`.
    pub fn line(name: &str, bytes: &[u8]) -> String {
        Self::line_for_digest(name, &crate::digest(bytes))
    }

    /// The line for a document whose digest the runtime already knows
    /// ([`crate::Document::digest`]).
    pub fn line_for_digest(name: &str, digest: &str) -> String {
        serde_json::to_string(&FrameLine { frame: Frame { name: name.to_string(), digest: digest.to_string() } }).unwrap_or_default()
    }
}

/// #ERR-29: whether the shell's ending is a failure worth a report: a
/// crash signal, a panic's 101, any code but 0, or no status at all. The
/// signals that end a session under it (SIGTERM, SIGINT, SIGHUP: systemd
/// stopping the unit, a closed terminal) and a quit (0) are not; SIGKILL is
/// (the OOM killer: systemd's own SIGKILL comes after the SIGTERM that
/// already ended this process).
pub fn shell_failed(status: Option<ExitStatus>) -> bool {
    let Some(status) = status else { return true };
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        // SIGHUP 1, SIGINT 2, SIGTERM 15: the same numbers on every unix.
        if let Some(signal) = status.signal() {
            return !matches!(signal, 1 | 2 | 15);
        }
    }
    !status.success()
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// `Line` and `Exited` name the shell they are of (its number since the
/// supervisor started): the apps a shell launched inherit its stdout, so
/// the pipe of a shell that died stays open, and may still speak, for as
/// long as they run.
enum Msg {
    Changes(Vec<Change>),
    Line(u64, String),
    Exited(u64),
}

/// What the supervisor did, for logs and tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    Started { pid: u32 },
    /// The shell ended without the supervisor stopping it; `status` as
    /// `wait` read it (#ERR-29: [`shell_failed`] tells a crash from a stop).
    Crashed { status: Option<ExitStatus> },
    Edit { name: String, commit: String },
    Refused { name: String },
    Drawn { name: String, commit: String },
    Reverted { name: String, from: String, commit: String },
    /// The commit `from` was to be rolled back, but the document's newest
    /// commit no longer holds it (another process undid or replaced it, or
    /// a rollback of a commit it shared did, #EDE-116): nothing reverted;
    /// what the document holds now is taken in as an edit.
    Superseded { name: String, from: String },
    Factory { name: String, commit: String },
}

pub struct Supervisor {
    ui: Arc<Mutex<Runtime>>,
    state: ApplyState,
    /// The machine's commit names (`c<n>`) to git ids.
    git_of: HashMap<String, String>,
    /// Per document: the machine's commit waiting for a frame and the digest of its bytes.
    pending: HashMap<String, (String, String)>,
    /// Per document: the digest of the newest frame the shell reported. The
    /// shell has its own watcher and may draw an edit before this side has
    /// debounced and committed it; the frame then counts at commit time.
    drawn: HashMap<String, String>,
    /// Per document: the digest of the content the machine last took in
    /// (an edit, its own revert or factory write). A change to other bytes is
    /// an edit, whoever committed it (#EDE-110: `exponential-ui set` commits in
    /// its own process and reaches this side as `commit: None`).
    known: HashMap<String, String>,
    command: Vec<String>,
    child: Option<Child>,
    /// The number of the shell `child` is (or was).
    shell: u64,
    tx: mpsc::Sender<Msg>,
    rx: mpsc::Receiver<Msg>,
    _watcher: crate::Watcher,
    on_report: Box<dyn FnMut(Report) + Send>,
}

impl Supervisor {
    /// A supervisor over `ui` (opened with its factory) that will run
    /// `command` as the shell.
    pub fn new(ui: Runtime, command: Vec<String>, on_report: impl FnMut(Report) + Send + 'static) -> io::Result<Supervisor> {
        let names = ui.factory_names()?;
        let mut state = ApplyState::with_factory(names.clone());
        let (mut git_of, mut known) = (HashMap::new(), HashMap::new());
        for name in &names {
            known.insert(name.clone(), crate::digest(&std::fs::read(ui.dir().join(format!("{name}.json"))).unwrap_or_default()));
            // An edit from an earlier run stays a suspect (#EDE-110).
            if let Some(commit) = ui.edited_commit(name)? {
                let id = state.edited_before(name, commit.summary.starts_with("Undo: "));
                git_of.insert(id, commit.id);
            }
        }
        let ui = Arc::new(Mutex::new(ui));
        let (tx, rx) = mpsc::channel();
        let watcher = {
            let tx = tx.clone();
            crate::watch(ui.clone(), move |changes| {
                let _ = tx.send(Msg::Changes(changes));
            })?
        };
        Ok(Supervisor { ui, state, git_of, known, pending: HashMap::new(), drawn: HashMap::new(), command, child: None, shell: 0, tx, rx, _watcher: watcher, on_report: Box::new(on_report) })
    }

    pub fn state(&self) -> &ApplyState {
        &self.state
    }

    fn spawn(&mut self) -> io::Result<()> {
        let dir = self.ui.lock().unwrap_or_else(|p| p.into_inner()).dir().to_path_buf();
        let mut child = Command::new(&self.command[0]).args(&self.command[1..]).env("EDE_UI_DIR", &dir).stdout(Stdio::piped()).stdin(Stdio::null()).spawn()?;
        let stdout = child.stdout.take().expect("piped stdout");
        let tx = self.tx.clone();
        self.shell += 1;
        let shell = self.shell;
        std::thread::Builder::new().name("exponential-ui-shell-out".into()).spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(Msg::Line(shell, line)).is_err() {
                    return;
                }
            }
            let _ = tx.send(Msg::Exited(shell));
        })?;
        (self.on_report)(Report::Started { pid: child.id() });
        self.child = Some(child);
        Ok(())
    }

    /// Runs the shell and the machine until `stop` is set, or the shell
    /// kept dying right after starting; the shell is killed on the way out.
    /// The supervisor itself ends with the process that started it (the
    /// compositor), so a dead display never leaves a restart loop behind.
    pub fn run(&mut self, stop: &AtomicBool) -> io::Result<()> {
        // SAFETY: prctl with PR_SET_PDEATHSIG only sets this process's parent-death signal.
        #[cfg(target_os = "linux")]
        unsafe {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
        }
        self.spawn()?;
        let mut started = SystemTime::now();
        let mut quick_exits = 0u32;
        let mut restart_at: Option<SystemTime> = None;
        while !stop.load(Ordering::Relaxed) {
            // The shell's end, with its status when it was read.
            let mut ended: Option<Option<ExitStatus>> = None;
            match self.rx.recv_timeout(TICK) {
                Ok(Msg::Changes(changes)) => self.on_changes(changes)?,
                Ok(Msg::Line(shell, line)) if shell == self.shell => self.on_line(&line)?,
                Ok(Msg::Exited(shell)) if shell == self.shell => {
                    if let Some(mut child) = self.child.take() {
                        ended = Some(child.wait().ok());
                    }
                }
                // A shell before this one: its apps still hold its stdout.
                Ok(Msg::Line(..) | Msg::Exited(..)) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    self.step(Event::Tick { at: now_ms() })?;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            // #EDE-150: the apps the shell launched inherit its stdout, so a
            // shell that died never closed the pipe while one of them ran:
            // its exit is read here too, not from the pipe's end alone.
            if ended.is_none() {
                if let Some(Ok(Some(status))) = self.child.as_mut().map(Child::try_wait) {
                    self.child = None;
                    ended = Some(Some(status));
                }
            }
            if let Some(status) = ended {
                (self.on_report)(Report::Crashed { status });
                self.step(Event::Crash { at: now_ms() })?;
                quick_exits = if started.elapsed().unwrap_or_default() < QUICK_EXIT { quick_exits + 1 } else { 0 };
                if quick_exits >= MAX_QUICK_EXITS {
                    return Err(io::Error::other(format!("the shell died {quick_exits} times right after starting; giving up")));
                }
                restart_at = Some(SystemTime::now() + RESTART_DELAY);
            }
            if restart_at.is_some_and(|t| SystemTime::now() >= t) {
                restart_at = None;
                started = SystemTime::now();
                self.spawn()?;
            }
        }
        self.kill();
        Ok(())
    }

    pub fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn on_changes(&mut self, changes: Vec<Change>) -> io::Result<()> {
        for change in changes {
            match change {
                // `commit: None` = already in history: another process committed it
                // (`exponential-ui set`), or nothing changed since the machine last saw it.
                Change::Applied { name, commit } => {
                    let at = now_ms();
                    let (bytes, git) = {
                        let ui = self.ui.lock().unwrap_or_else(|p| p.into_inner());
                        let bytes = std::fs::read(ui.dir().join(format!("{name}.json"))).unwrap_or_default();
                        let git = match commit {
                            Some(git) => Some(git),
                            None => ui.committed(&name)?.map(|c| c.id),
                        };
                        (bytes, git)
                    };
                    let digest = crate::digest(&bytes);
                    let Some(git) = git.filter(|_| self.known.get(&name) != Some(&digest)) else { continue };
                    self.known.insert(name.clone(), digest.clone());
                    let effects = self.step(Event::Edit { at, name: name.clone(), valid: true, issues: Vec::new() })?;
                    for effect in effects {
                        if let Effect::Commit { id, .. } = effect {
                            self.git_of.insert(id.clone(), git.clone());
                            self.pending.insert(name.clone(), (id.clone(), digest.clone()));
                            (self.on_report)(Report::Edit { name: name.clone(), commit: git.clone() });
                            if self.drawn.get(&name) == Some(&digest) {
                                self.frame(&name, &digest)?;
                            }
                        }
                    }
                }
                Change::Rejected { name, issues, .. } => {
                    self.step(Event::Edit { at: now_ms(), name: name.clone(), valid: false, issues })?;
                    (self.on_report)(Report::Refused { name });
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn on_line(&mut self, line: &str) -> io::Result<()> {
        let Ok(FrameLine { frame }) = serde_json::from_str::<FrameLine>(line) else {
            return Ok(());
        };
        self.drawn.insert(frame.name.clone(), frame.digest.clone());
        self.frame(&frame.name, &frame.digest)
    }

    /// The shell drew `name` from bytes with `digest`: the pending commit
    /// with that digest is proven.
    fn frame(&mut self, name: &str, digest: &str) -> io::Result<()> {
        let Some((commit, pending_digest)) = self.pending.get(name).cloned() else {
            return Ok(());
        };
        if pending_digest == digest {
            self.pending.remove(name);
            self.step(Event::Frame { at: now_ms(), name: name.to_string(), commit: commit.clone() })?;
            (self.on_report)(Report::Drawn { name: name.to_string(), commit: self.git_of.get(&commit).cloned().unwrap_or(commit) });
        }
        Ok(())
    }

    /// One event through the machine; the effects carried out, and returned.
    fn step(&mut self, event: Event) -> io::Result<Vec<Effect>> {
        let (state, effects) = apply::reduce(&self.state, &event);
        self.state = state;
        for effect in &effects {
            match effect {
                Effect::Revert { name, from, id, .. } => {
                    let git_from = self.git_of.get(from).cloned();
                    let mut ui = self.ui.lock().unwrap_or_else(|p| p.into_inner());
                    let git = match git_from {
                        Some(git_from) => {
                            // #EDE-116: the newest commit no longer holds what `from` put in
                            // (another process undid or replaced it before the watcher brought
                            // that: `exponential-ui undo` while it waited for its frame, another
                            // `set`; or the rollback of a commit it shared with another
                            // document). Nothing to revert: what it holds now is taken in, an
                            // edit like any other. Git objects on both sides, so line-ending
                            // filters never tell them apart. An empty revert is no error.
                            let held = ui.digest_at("HEAD", name)? == ui.digest_at(&git_from, name)?;
                            let reverted = if held { ui.revert_commit(&git_from, &format!("Edit {name}.json, rolled back"))? } else { None };
                            let Some((git, _)) = reverted else {
                                drop(ui);
                                self.pending.remove(name);
                                (self.on_report)(Report::Superseded { name: name.clone(), from: from.clone() });
                                self.on_changes(vec![Change::Applied { name: name.clone(), commit: None }])?;
                                continue;
                            };
                            git
                        }
                        // A revert of a commit this run never made (the factory commit): the factory copy.
                        None => {
                            ui.restore_factory(name)?;
                            ui.head()?.map(|c| c.id).unwrap_or_default()
                        }
                    };
                    let bytes = std::fs::read(ui.dir().join(format!("{name}.json"))).unwrap_or_default();
                    drop(ui);
                    self.known.insert(name.clone(), crate::digest(&bytes));
                    self.pending.remove(name);
                    self.git_of.insert(id.clone(), git.clone());
                    (self.on_report)(Report::Reverted { name: name.clone(), from: from.clone(), commit: git });
                }
                Effect::Factory { name, id } => {
                    let mut ui = self.ui.lock().unwrap_or_else(|p| p.into_inner());
                    ui.restore_factory(name)?;
                    let git = ui.head()?.map(|c| c.id).unwrap_or_default();
                    let bytes = std::fs::read(ui.dir().join(format!("{name}.json"))).unwrap_or_default();
                    drop(ui);
                    self.known.insert(name.clone(), crate::digest(&bytes));
                    self.pending.remove(name);
                    self.git_of.insert(id.clone(), git.clone());
                    (self.on_report)(Report::Factory { name: name.clone(), commit: git });
                }
                Effect::Commit { .. } | Effect::Refuse { .. } | Effect::Reload { .. } => {}
            }
        }
        Ok(effects)
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn panel(text: &str) -> Value {
        json!({
            "surface": {"anchor": ["top", "left", "right"], "size": {"height": 40}, "exclusiveZone": 40},
            "root": {"id": "panel", "component": "Box", "children": [{"id": "clock", "component": "Text", "props": {"text": text}}]}
        })
    }

    /// What `exponential-ui set` or `undo` does in another process. The
    /// supervisor's runtime is held meanwhile, so its watcher reloads only
    /// once that process committed.
    fn elsewhere<R>(s: &Supervisor, factory: &std::path::Path, f: impl FnOnce(&mut Runtime) -> R) -> R {
        let held = s.ui.lock().unwrap();
        let (mut other, _) = Runtime::open_with_factory(held.dir(), factory).unwrap();
        f(&mut other)
    }

    fn summaries(s: &Supervisor) -> Vec<String> {
        s.ui.lock().unwrap().log(10).unwrap().into_iter().map(|c| c.summary).collect()
    }

    /// A supervisor over a ui directory with the factory panel, which the
    /// shell drew; it never runs the shell (the test drives it).
    struct Fixture {
        _tmp: tempfile::TempDir,
        dir: std::path::PathBuf,
        factory: std::path::PathBuf,
        shipped: Vec<u8>,
        reports: Arc<Mutex<Vec<Report>>>,
    }

    fn supervised() -> (Supervisor, Fixture) {
        let tmp = tempfile::tempdir().unwrap();
        let factory = tmp.path().join("factory");
        std::fs::create_dir_all(&factory).unwrap();
        let shipped = serde_json::to_vec_pretty(&panel("factory")).unwrap();
        std::fs::write(factory.join("panel.json"), &shipped).unwrap();
        let dir = tmp.path().join("ui");
        let (ui, _) = Runtime::open_with_factory(&dir, &factory).unwrap();
        let reports = Arc::new(Mutex::new(Vec::new()));
        let heard = reports.clone();
        let mut s = Supervisor::new(ui, vec!["true".into()], move |r| heard.lock().unwrap().push(r)).unwrap();
        s.on_line(&FrameLine::line("panel", &shipped)).unwrap();
        (s, Fixture { _tmp: tmp, dir, factory, shipped, reports })
    }

    fn applied() -> Vec<Change> {
        vec![Change::Applied { name: "panel".into(), commit: None }]
    }

    /// #ERR-29: a crash signal, a panic or an error code is a failure; the
    /// session stopping the shell or the shell quitting is not.
    #[cfg(unix)]
    #[test]
    fn a_crash_is_a_failure_and_a_stop_is_not() {
        use std::os::unix::process::ExitStatusExt;
        let signal = |n: i32| Some(ExitStatus::from_raw(n));
        let code = |n: i32| Some(ExitStatus::from_raw(n << 8));
        for (status, failed) in [(signal(11), true), (signal(6), true), (signal(9), true), (code(101), true), (code(2), true), (None, true), (signal(15), false), (signal(2), false), (signal(1), false), (code(0), false)] {
            assert_eq!(shell_failed(status), failed, "{status:?}");
        }
    }

    /// A tick past every deadline.
    fn expired() -> Event {
        Event::Tick { at: now_ms() + apply::FRAME_ACK_MS + 1 }
    }

    /// #EDE-116: an undo that lands before the edit's deadline but reaches
    /// the supervisor after it (the watcher's debounce). The rollback finds
    /// the edit gone and the supervisor goes on (before: the revert of the
    /// undone commit failed and stopped it); the undo is then an edit like
    /// any other, and the next edit is still supervised.
    #[test]
    fn an_undo_the_watcher_brings_after_the_deadline_is_no_rollback() {
        let (mut s, fx) = supervised();
        let (factory, panel_file) = (&fx.factory, fx.dir.join("panel.json"));
        let taken = || std::mem::take(&mut *fx.reports.lock().unwrap());

        // `exponential-ui set`, brought by the watcher: it waits for its frame.
        elsewhere(&s, factory, |ui| ui.write("panel", &panel("silent"), "Edit panel.json").unwrap());
        s.on_changes(applied()).unwrap();
        assert!(s.state().documents["panel"].pending.is_some());
        // `exponential-ui undo` before any frame; the wait runs out before
        // the watcher brings it. The rollback takes the undo in: an edit,
        // drawn already. The watcher's late change is then nothing new.
        elsewhere(&s, factory, |ui| ui.undo().unwrap().expect("an undo"));
        s.step(expired()).expect("the supervisor goes on");
        s.on_changes(applied()).unwrap();
        let heard = taken();
        assert!(matches!(&heard[..], [Report::Edit { .. }, Report::Superseded { .. }, Report::Edit { .. }, Report::Drawn { .. }]), "{heard:?}");
        assert_eq!(summaries(&s)[..2], ["Undo: Edit panel.json", "Edit panel.json"]);
        assert!(s.state().documents["panel"].pending.is_none());
        assert_eq!(std::fs::read(&panel_file).unwrap(), fx.shipped);

        // The next edit is still supervised: nobody draws it, it rolls back.
        elsewhere(&s, factory, |ui| ui.write("panel", &panel("again"), "Edit panel.json").unwrap());
        s.on_changes(applied()).unwrap();
        s.step(expired()).unwrap();
        let heard = taken();
        assert!(matches!(&heard[..], [Report::Edit { .. }, Report::Reverted { .. }]), "{heard:?}");
        assert_eq!(summaries(&s)[0], "Undo: Edit panel.json, rolled back");
        assert_eq!(std::fs::read(&panel_file).unwrap(), fx.shipped);
    }

    /// The other order: the second `set` is brought before any tick, and
    /// its own event finds the first edit's deadline passed. The rollback
    /// still sees the first edit replaced (the supervisor already took the
    /// second in, so its record is no guide) and the second is supervised.
    #[test]
    fn a_set_that_brings_the_deadline_along_is_its_own_edit() {
        let (mut s, fx) = supervised();
        let factory = &fx.factory;
        elsewhere(&s, factory, |ui| ui.write("panel", &panel("first"), "Edit panel.json").unwrap());
        s.on_changes(applied()).unwrap();
        s.state.documents.get_mut("panel").unwrap().pending.as_mut().unwrap().deadline = 0;
        elsewhere(&s, factory, |ui| ui.write("panel", &panel("second"), "Edit panel.json").unwrap());
        s.on_changes(applied()).expect("the supervisor goes on");
        assert!(s.state().documents["panel"].pending.is_some(), "the second edit waits for its frame");
        s.step(expired()).unwrap();
        let heard = std::mem::take(&mut *fx.reports.lock().unwrap());
        assert!(matches!(&heard[..], [Report::Edit { .. }, Report::Superseded { .. }, Report::Edit { .. }, Report::Reverted { .. }]), "{heard:?}");
    }

    /// The same with another `set` in place of the undo: reverting the
    /// first edit under the second would conflict (and stop the
    /// supervisor). The second arrives as its own edit, supervised.
    #[test]
    fn a_set_the_watcher_brings_after_the_deadline_is_its_own_edit() {
        let (mut s, fx) = supervised();
        let factory = &fx.factory;
        elsewhere(&s, factory, |ui| ui.write("panel", &panel("first"), "Edit panel.json").unwrap());
        s.on_changes(applied()).unwrap();
        elsewhere(&s, factory, |ui| ui.write("panel", &panel("second"), "Edit panel.json").unwrap());
        s.step(expired()).expect("the supervisor goes on");
        s.on_changes(applied()).unwrap();
        assert!(s.state().documents["panel"].pending.is_some(), "the second edit waits for its frame");
        s.step(expired()).unwrap();
        let heard = std::mem::take(&mut *fx.reports.lock().unwrap());
        assert!(matches!(&heard[..], [Report::Edit { .. }, Report::Superseded { .. }, Report::Edit { .. }, Report::Reverted { .. }]), "{heard:?}");
    }
}
