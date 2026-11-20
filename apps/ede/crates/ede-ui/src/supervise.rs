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
    /// commit no longer holds it (another process undid or replaced it,
    /// #EDE-116): nothing reverted;
    /// what the document holds now is taken in as an edit.
    Superseded { name: String, from: String },
    Factory { name: String, commit: String },
}

pub struct Supervisor {
    ui: Arc<Mutex<Runtime>>,
    state: ApplyState,
    /// The machine's commit names (`c<n>`) to git ids; a document's factory
    /// commit (`f-<name>`) to the revision that holds what it stands for
    /// (what was on screen at start, or before an earlier run's edit).
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
        let start = ui.head()?.map(|c| c.id);
        for name in &names {
            known.insert(name.clone(), crate::digest(&std::fs::read(ui.dir().join(format!("{name}.json"))).unwrap_or_default()));
            // An edit from an earlier run stays a suspect (#EDE-110); its
            // rollback goes to what the file held before it (#EDE-137).
            if let Some(commit) = ui.edited_commit(name)? {
                let id = state.edited_before(name, commit.summary.starts_with("Undo: "));
                git_of.insert(apply::factory_commit(name), format!("{}^", commit.id));
                git_of.insert(id, commit.id);
            } else if let Some(start) = &start {
                git_of.insert(apply::factory_commit(name), start.clone());
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
    /// A failure carrying out the machine's effects (#EDE-153: a git another
    /// process held past the wait) is said on stderr and the supervisor goes
    /// on, so the edits after it are still supervised.
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
            let carried = match self.rx.recv_timeout(TICK) {
                Ok(Msg::Changes(changes)) => self.on_changes(changes),
                Ok(Msg::Line(shell, line)) if shell == self.shell => self.on_line(&line),
                Ok(Msg::Exited(shell)) if shell == self.shell => {
                    if let Some(mut child) = self.child.take() {
                        ended = Some(child.wait().ok());
                    }
                    Ok(())
                }
                // A shell before this one: its apps still hold its stdout.
                Ok(Msg::Line(..) | Msg::Exited(..)) => Ok(()),
                Err(mpsc::RecvTimeoutError::Timeout) => self.step(Event::Tick { at: now_ms() }).map(|_| ()),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            if let Err(e) = carried {
                eprintln!("exponential-ui supervise: {e} (the supervisor goes on)");
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
                if let Err(e) = self.step(Event::Crash { at: now_ms() }) {
                    eprintln!("exponential-ui supervise: {e} (the supervisor goes on)");
                }
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
                    // #EDE-161: the commit and what IT holds, never the file as it
                    // is by now. The file may have moved on since the watcher
                    // committed (another process's `set` the moment a git lock
                    // went): that edit comes as a change of its own, and paired
                    // with this commit's id it counted as known, was never
                    // supervised, and the rollback found "its" commit replaced.
                    let taken = {
                        let ui = self.ui.lock().unwrap_or_else(|p| p.into_inner());
                        let git = match commit {
                            Some(git) => Some(git),
                            None => ui.committed(&name)?.map(|c| c.id),
                        };
                        match git {
                            Some(git) => ui.digest_at(&git, &name)?.map(|digest| (git, digest)),
                            None => None,
                        }
                    };
                    let Some((git, digest)) = taken.filter(|(_, digest)| self.known.get(&name) != Some(digest)) else { continue };
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
                Effect::Revert { name, from, to, id } => {
                    let git_from = self.git_of.get(from).cloned();
                    let mut ui = self.ui.lock().unwrap_or_else(|p| p.into_inner());
                    // #EDE-137: what the file holds this moment, before the watcher
                    // brings it: a valid edit is committed (and replaces the wait
                    // below), a broken one refused (the rollback keeps its text aside).
                    if ui.take_in(name)?.iter().any(|c| matches!(c, Change::Rejected { .. })) {
                        (self.on_report)(Report::Refused { name: name.clone() });
                    }
                    let written = match git_from {
                        Some(git_from) => {
                            // #EDE-116: the newest commit no longer holds what `from` put in
                            // (another process undid or replaced it before the watcher brought
                            // that: `exponential-ui undo` while it waited for its frame, another
                            // `set`, a hand edit this moment). Nothing to roll back: what it
                            // holds now is taken in, an edit like any other, and what was shown
                            // stays what it was. Git objects on both sides, so line-ending
                            // filters never tell them apart.
                            let held = ui.digest_at("HEAD", name)? == ui.digest_at(&git_from, name)?;
                            if !held {
                                drop(ui);
                                self.pending.remove(name);
                                if let Some(shown) = self.git_of.get(to).cloned() {
                                    self.git_of.insert(id.clone(), shown);
                                }
                                (self.on_report)(Report::Superseded { name: name.clone(), from: from.clone() });
                                self.on_changes(vec![Change::Applied { name: name.clone(), commit: None }])?;
                                continue;
                            }
                            // #EDE-137: back to what was shown (`to`, apply.json rule 0),
                            // never to an edit between that never drew; this document alone.
                            let shown = match self.git_of.get(to) {
                                Some(rev) => ui.bytes_at(rev, name)?,
                                None => None,
                            };
                            let bytes = match shown {
                                Some(bytes) => bytes,
                                None => ui.factory_copy(name)?,
                            };
                            ui.put_back(name, &bytes, &format!("Undo: Edit {name}.json, rolled back"))?
                        }
                        // A revert of a commit this run never made (the factory commit): the factory copy.
                        None => {
                            let bytes = ui.factory_copy(name)?;
                            ui.put_back(name, &bytes, &format!("Factory {name}"))?
                        }
                    };
                    let git = match written {
                        Some(git) => git,
                        None => ui.head()?.map(|c| c.id).unwrap_or_default(),
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
                    // #EDE-137: this document alone (another one's edit stays for the watcher).
                    let bytes = ui.factory_copy(name)?;
                    let git = match ui.put_back(name, &bytes, &format!("Factory {name}"))? {
                        Some(git) => git,
                        None => ui.head()?.map(|c| c.id).unwrap_or_default(),
                    };
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
        supervised_docs(&["panel"])
    }

    /// [`supervised`] with more factory documents (`<name>` says `factory
    /// <name>`, the panel `factory`), every one drawn.
    fn supervised_docs(names: &[&str]) -> (Supervisor, Fixture) {
        let tmp = tempfile::tempdir().unwrap();
        let factory = tmp.path().join("factory");
        std::fs::create_dir_all(&factory).unwrap();
        let shipped_of = |name: &str| serde_json::to_vec_pretty(&panel(&if name == "panel" { "factory".to_string() } else { format!("factory {name}") })).unwrap();
        for name in names {
            std::fs::write(factory.join(format!("{name}.json")), shipped_of(name)).unwrap();
        }
        let dir = tmp.path().join("ui");
        let (ui, _) = Runtime::open_with_factory(&dir, &factory).unwrap();
        let reports = Arc::new(Mutex::new(Vec::new()));
        let heard = reports.clone();
        let mut s = Supervisor::new(ui, vec!["true".into()], move |r| heard.lock().unwrap().push(r)).unwrap();
        for name in names {
            s.on_line(&FrameLine::line(name, &shipped_of(name))).unwrap();
        }
        (s, Fixture { _tmp: tmp, dir, factory, shipped: shipped_of("panel"), reports })
    }

    /// What the watcher brings of `name`: the reload it is about to make,
    /// or its own when it came first (then its message is on its way).
    fn watched(s: &Supervisor, name: &str) -> Vec<Change> {
        let mut changes = s.ui.lock().unwrap().reload().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !changes.iter().any(|c| c.name() == name) && std::time::Instant::now() < deadline {
            if let Ok(Msg::Changes(more)) = s.rx.recv_timeout(Duration::from_millis(50)) {
                changes.extend(more);
            }
        }
        changes
    }

    /// The newest two commits hold `a` and `b` (either order: the watcher's
    /// own commit may come before or after the supervisor's).
    fn newest_two(s: &Supervisor, a: &str, b: &str) -> bool {
        let top = summaries(s);
        top.len() >= 2 && ((top[0] == a && top[1] == b) || (top[0] == b && top[1] == a))
    }

    /// A hand edit of `name` (an editor's save: nobody commits it), made
    /// while the supervisor's runtime is held so the watcher reloads only
    /// after what the test does next.
    fn hand_edit(s: &Supervisor, name: &str, bytes: &[u8]) {
        let held = s.ui.lock().unwrap();
        std::fs::write(held.dir().join(format!("{name}.json")), bytes).unwrap();
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

    /// #EDE-161: the watcher's commit reaches the supervisor after the file
    /// moved on: another process's `set` wrote and committed meanwhile (in
    /// `a_failed_rollback_does_not_end_the_supervisor`, the moment the git
    /// lock went). That commit is taken in as what IT holds, and the later
    /// edit is one of its own, supervised: nobody draws it, it rolls back.
    /// Before, the commit's id was paired with the later bytes, so the later
    /// edit counted as known, and its rollback found "its" commit replaced:
    /// nothing was rolled back.
    #[test]
    fn a_commit_the_file_moved_on_from_is_taken_in_as_it_was() {
        let (mut s, fx) = supervised();
        let commit_of = |changes: Vec<Change>| changes.into_iter().find_map(|c| if let Change::Applied { commit, .. } = c { commit } else { None }).expect("a commit");
        // The watcher's reload committed `first`; before its change reached
        // the supervisor, `exponential-ui set` wrote and committed `second`.
        let first = commit_of(elsewhere(&s, &fx.factory, |ui| ui.write("panel", &panel("first"), "Edit panel.json").unwrap()));
        let second = commit_of(elsewhere(&s, &fx.factory, |ui| ui.write("panel", &panel("second"), "Edit panel.json").unwrap()));
        s.on_changes(vec![Change::Applied { name: "panel".into(), commit: Some(first.clone()) }]).unwrap();
        s.on_changes(applied()).unwrap();
        let heard = std::mem::take(&mut *fx.reports.lock().unwrap());
        assert!(matches!(&heard[..], [Report::Edit { commit: a, .. }, Report::Edit { commit: b, .. }] if *a == first && *b == second), "{heard:?}");
        s.step(expired()).unwrap();
        let heard = std::mem::take(&mut *fx.reports.lock().unwrap());
        assert!(matches!(&heard[..], [Report::Reverted { .. }]), "{heard:?}");
        assert_eq!(std::fs::read(fx.dir.join("panel.json")).unwrap(), fx.shipped, "back to what was shown");
    }

    /// #EDE-137 (1): another document's hand edit landing while a rollback
    /// runs is no part of it: the rollback commits its own document alone,
    /// the watcher brings the edit, and it is supervised (before: the
    /// rollback's reload committed it and the supervisor dropped it).
    #[test]
    fn a_rollback_leaves_another_documents_edit_to_the_watcher() {
        let (mut s, fx) = supervised_docs(&["menu", "panel"]);
        elsewhere(&s, &fx.factory, |ui| ui.write("panel", &panel("silent"), "Edit panel.json").unwrap());
        s.on_changes(applied()).unwrap();
        hand_edit(&s, "menu", &serde_json::to_vec_pretty(&panel("menu, by hand")).unwrap());
        s.step(expired()).unwrap();
        let changes = watched(&s, "menu");
        s.on_changes(changes).unwrap();
        let heard = std::mem::take(&mut *fx.reports.lock().unwrap());
        assert!(heard.iter().any(|r| matches!(r, Report::Edit { name, .. } if name == "menu")), "the menu's edit is supervised: {heard:?}");
        assert!(s.state().documents["menu"].pending.is_some(), "the menu's edit waits for its frame");
        assert!(newest_two(&s, "Edit menu.json", "Undo: Edit panel.json, rolled back"), "{:?}", summaries(&s));
        assert_eq!(std::fs::read(fx.dir.join("panel.json")).unwrap(), fx.shipped);
    }

    /// #EDE-137 (1): the same for the factory write (three crashes after a
    /// rollback): the menu's edit is never committed as `Factory panel`.
    #[test]
    fn a_factory_write_leaves_another_documents_edit_to_the_watcher() {
        let (mut s, fx) = supervised_docs(&["menu", "panel"]);
        elsewhere(&s, &fx.factory, |ui| ui.write("panel", &panel("drawn"), "Edit panel.json").unwrap());
        s.on_changes(applied()).unwrap();
        s.on_line(&FrameLine::line("panel", &std::fs::read(fx.dir.join("panel.json")).unwrap())).unwrap();
        // The panel was rolled back once already and crashed twice since.
        let d = s.state.documents.get_mut("panel").unwrap();
        (d.reverted, d.crashes) = (true, apply::CRASHES_TO_REVERT - 1);
        hand_edit(&s, "menu", &serde_json::to_vec_pretty(&panel("menu, by hand")).unwrap());
        s.step(Event::Crash { at: now_ms() }).unwrap();
        let changes = watched(&s, "menu");
        s.on_changes(changes).unwrap();
        let heard = std::mem::take(&mut *fx.reports.lock().unwrap());
        assert!(heard.iter().any(|r| matches!(r, Report::Factory { name, .. } if name == "panel")), "{heard:?}");
        assert!(heard.iter().any(|r| matches!(r, Report::Edit { name, .. } if name == "menu")), "the menu's edit is supervised: {heard:?}");
        assert!(newest_two(&s, "Edit menu.json", "Factory panel"), "{:?}", summaries(&s));
    }

    /// #EDE-137 (1): one commit held two documents (one save of both), the
    /// menu drew, the panel did not: the rollback takes the panel back
    /// alone; the drawn menu stays as it is.
    #[test]
    fn a_rollback_of_a_shared_commit_leaves_the_drawn_sibling() {
        let (mut s, fx) = supervised_docs(&["menu", "panel"]);
        let menu = serde_json::to_vec_pretty(&panel("menu, edited")).unwrap();
        elsewhere(&s, &fx.factory, |ui| {
            std::fs::write(ui.dir().join("menu.json"), &menu).unwrap();
            std::fs::write(ui.dir().join("panel.json"), serde_json::to_vec_pretty(&panel("silent")).unwrap()).unwrap();
            ui.reload().unwrap()
        });
        assert_eq!(summaries(&s)[0], "Edit menu.json, panel.json", "one commit holds both");
        s.on_changes(vec![Change::Applied { name: "menu".into(), commit: None }, Change::Applied { name: "panel".into(), commit: None }]).unwrap();
        s.on_line(&FrameLine::line("menu", &menu)).unwrap();
        s.step(expired()).unwrap();
        assert_eq!(std::fs::read(fx.dir.join("panel.json")).unwrap(), fx.shipped);
        assert_eq!(std::fs::read(fx.dir.join("menu.json")).unwrap(), menu, "the drawn menu stays");
        let heard = std::mem::take(&mut *fx.reports.lock().unwrap());
        assert!(matches!(&heard[..], [Report::Edit { .. }, Report::Edit { .. }, Report::Drawn { name, .. }, Report::Reverted { .. }] if name == "menu"), "{heard:?}");
        assert!(s.state().documents["menu"].pending.is_none());
    }

    /// #EDE-137 (2): two quick edits, the second replaced the first's wait
    /// (apply.json rule 1) and never drew: the rollback goes to what was
    /// shown, never to the first edit, which never drew either.
    #[test]
    fn a_rollback_after_two_quick_edits_goes_to_what_was_shown() {
        let (mut s, fx) = supervised();
        elsewhere(&s, &fx.factory, |ui| ui.write("panel", &panel("first"), "Edit panel.json").unwrap());
        s.on_changes(applied()).unwrap();
        elsewhere(&s, &fx.factory, |ui| ui.write("panel", &panel("second"), "Edit panel.json").unwrap());
        s.on_changes(applied()).unwrap();
        s.step(expired()).unwrap();
        let heard = std::mem::take(&mut *fx.reports.lock().unwrap());
        assert!(matches!(&heard[..], [Report::Edit { .. }, Report::Edit { .. }, Report::Reverted { .. }]), "{heard:?}");
        assert_eq!(std::fs::read(fx.dir.join("panel.json")).unwrap(), fx.shipped, "what was shown, not the first edit");
        assert_eq!(summaries(&s)[0], "Undo: Edit panel.json, rolled back");
        assert!(s.state().documents["panel"].pending.is_none());
    }

    /// #EDE-137 (3): a broken hand edit (never committed) lands while a
    /// valid edit waits for its frame; the rollback goes on and so does the
    /// supervisor (before: `git revert` refused the dirty file and `run`
    /// ended). The file holds what was drawn again, the refused text is kept
    /// beside it, and the next edit is still supervised.
    #[test]
    fn a_rollback_over_an_uncommitted_edit_keeps_the_supervisor() {
        let (mut s, fx) = supervised();
        elsewhere(&s, &fx.factory, |ui| ui.write("panel", &panel("silent"), "Edit panel.json").unwrap());
        s.on_changes(applied()).unwrap();
        let broken = br#"{"surface": {"size": {"width": 1, "height": 1}}, "root": {"id": "panel", "component": "Nope"}}"#;
        hand_edit(&s, "panel", broken);
        s.step(expired()).expect("the supervisor goes on");
        // (Refused: reported here unless the watcher took the broken file in first.)
        let mut heard = std::mem::take(&mut *fx.reports.lock().unwrap());
        heard.retain(|r| !matches!(r, Report::Refused { .. }));
        assert!(matches!(&heard[..], [Report::Edit { .. }, Report::Reverted { .. }]), "{heard:?}");
        assert_eq!(summaries(&s)[0], "Undo: Edit panel.json, rolled back");
        assert_eq!(std::fs::read(fx.dir.join("panel.json")).unwrap(), fx.shipped, "what was drawn");
        assert_eq!(std::fs::read(fx.dir.join(crate::refused_file("panel"))).unwrap(), broken, "the refused text, kept");
        {
            let mut ui = s.ui.lock().unwrap();
            ui.reload().unwrap();
            assert_eq!(ui.documents().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["panel"], "the kept text is no document");
            assert!(ui.rejected().is_empty());
        }
        // The next edit is still supervised: nobody draws it, it rolls back.
        elsewhere(&s, &fx.factory, |ui| ui.write("panel", &panel("again"), "Edit panel.json").unwrap());
        s.on_changes(applied()).unwrap();
        s.step(expired()).unwrap();
        let heard = std::mem::take(&mut *fx.reports.lock().unwrap());
        assert!(matches!(&heard[..], [Report::Edit { .. }, Report::Reverted { .. }]), "{heard:?}");
        assert_eq!(std::fs::read(fx.dir.join("panel.json")).unwrap(), fx.shipped);
    }

    /// #EDE-137 (3): a VALID hand edit of the same document landing with
    /// the rollback (the watcher has not brought it yet) is no part of it
    /// either: it replaces the wait like any second edit (apply.json rule
    /// 1), committed as the person's edit and supervised; nothing is put
    /// back over it and nothing is kept aside.
    #[test]
    fn a_valid_edit_landing_with_the_rollback_replaces_the_wait() {
        let (mut s, fx) = supervised();
        elsewhere(&s, &fx.factory, |ui| ui.write("panel", &panel("silent"), "Edit panel.json").unwrap());
        s.on_changes(applied()).unwrap();
        let by_hand = serde_json::to_vec_pretty(&panel("by hand")).unwrap();
        hand_edit(&s, "panel", &by_hand);
        s.step(expired()).expect("the supervisor goes on");
        let heard = std::mem::take(&mut *fx.reports.lock().unwrap());
        assert!(matches!(&heard[..], [Report::Edit { .. }, Report::Superseded { .. }, Report::Edit { .. }]), "{heard:?}");
        assert_eq!(summaries(&s)[..2], ["Edit panel.json", "Edit panel.json"]);
        assert_eq!(std::fs::read(fx.dir.join("panel.json")).unwrap(), by_hand);
        assert!(!fx.dir.join(crate::refused_file("panel")).exists());
        assert!(s.state().documents["panel"].pending.is_some(), "the hand edit waits for its frame");
        // Nobody draws it either: back to what was shown, never to the first edit.
        s.step(expired()).unwrap();
        assert_eq!(std::fs::read(fx.dir.join("panel.json")).unwrap(), fx.shipped);
    }
}
