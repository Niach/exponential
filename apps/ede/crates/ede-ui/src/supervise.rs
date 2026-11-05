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
    process::{Child, Command, Stdio},
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

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

enum Msg {
    Changes(Vec<Change>),
    Line(String),
    Exited,
}

/// What the supervisor did, for logs and tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    Started { pid: u32 },
    Crashed,
    Edit { name: String, commit: String },
    Refused { name: String },
    Drawn { name: String, commit: String },
    Reverted { name: String, from: String, commit: String },
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
    command: Vec<String>,
    child: Option<Child>,
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
        let ui = Arc::new(Mutex::new(ui));
        let (tx, rx) = mpsc::channel();
        let watcher = {
            let tx = tx.clone();
            crate::watch(ui.clone(), move |changes| {
                let _ = tx.send(Msg::Changes(changes));
            })?
        };
        Ok(Supervisor { ui, state: ApplyState::with_factory(names), git_of: HashMap::new(), pending: HashMap::new(), drawn: HashMap::new(), command, child: None, tx, rx, _watcher: watcher, on_report: Box::new(on_report) })
    }

    pub fn state(&self) -> &ApplyState {
        &self.state
    }

    fn spawn(&mut self) -> io::Result<()> {
        let dir = self.ui.lock().unwrap_or_else(|p| p.into_inner()).dir().to_path_buf();
        let mut child = Command::new(&self.command[0]).args(&self.command[1..]).env("EDE_UI_DIR", &dir).stdout(Stdio::piped()).stdin(Stdio::null()).spawn()?;
        let stdout = child.stdout.take().expect("piped stdout");
        let tx = self.tx.clone();
        std::thread::Builder::new().name("ede-ui-shell-out".into()).spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(Msg::Line(line)).is_err() {
                    return;
                }
            }
            let _ = tx.send(Msg::Exited);
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
        unsafe {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
        }
        self.spawn()?;
        let mut started = SystemTime::now();
        let mut quick_exits = 0u32;
        let mut restart_at: Option<SystemTime> = None;
        while !stop.load(Ordering::Relaxed) {
            match self.rx.recv_timeout(TICK) {
                Ok(Msg::Changes(changes)) => self.on_changes(changes)?,
                Ok(Msg::Line(line)) => self.on_line(&line)?,
                Ok(Msg::Exited) => {
                    if let Some(mut child) = self.child.take() {
                        let _ = child.wait();
                    }
                    (self.on_report)(Report::Crashed);
                    self.step(Event::Crash { at: now_ms() })?;
                    quick_exits = if started.elapsed().unwrap_or_default() < QUICK_EXIT { quick_exits + 1 } else { 0 };
                    if quick_exits >= MAX_QUICK_EXITS {
                        return Err(io::Error::other(format!("the shell died {quick_exits} times right after starting; giving up")));
                    }
                    restart_at = Some(SystemTime::now() + RESTART_DELAY);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    self.step(Event::Tick { at: now_ms() })?;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
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
                // `commit: None` = already in history (the supervisor's own revert, a no-op edit).
                Change::Applied { name, commit: Some(git) } => {
                    let at = now_ms();
                    let bytes = std::fs::read(self.ui.lock().unwrap_or_else(|p| p.into_inner()).dir().join(format!("{name}.json"))).unwrap_or_default();
                    let digest = crate::digest(&bytes);
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
                        Some(git_from) => ui.revert_commit(&git_from, &format!("Edit {name}.json, rolled back"))?.0,
                        // A revert of a commit this run never made (the factory commit): the factory copy.
                        None => {
                            ui.restore_factory(name)?;
                            ui.head()?.map(|c| c.id).unwrap_or_default()
                        }
                    };
                    drop(ui);
                    self.pending.remove(name);
                    self.git_of.insert(id.clone(), git.clone());
                    (self.on_report)(Report::Reverted { name: name.clone(), from: from.clone(), commit: git });
                }
                Effect::Factory { name, id } => {
                    let mut ui = self.ui.lock().unwrap_or_else(|p| p.into_inner());
                    ui.restore_factory(name)?;
                    let git = ui.head()?.map(|c| c.id).unwrap_or_default();
                    drop(ui);
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
