//! #EXP-2161 (`packages/domain-contract/fixtures/member-space.json`): THE
//! HOST half of member spaces. A member's private Assistant runs on this
//! server in a child daemon of its own, closed off by [`member_space`].
//!
//! - THE RECONCILE: on start, with each heartbeat and on each nudge the
//!   server says which spaces this host runs (`memberSpaces.hosted`);
//!   [`plan`] turns that into starts and stops. No answer = nothing changes.
//! - THE SUPERVISOR: one launcher process per space, restarted with a growing
//!   wait when it ends by itself.
//! - A space's directory is `<data>/members/<spaceId>` (0700): `data` is the
//!   space's own (read-write there), `host` is this daemon's (read-only
//!   there): the space key and the logins' access tokens.
//! - A space the server no longer lists ENDS, on a thread of its own ([`end`]):
//!   stopped (its daemon saves every session's unpushed work first), its key
//!   and logins deleted, saved ONCE more from a fresh, equally closed process
//!   that reaches the git remote alone, and its directory deleted, only while
//!   nothing of it runs ([`member_space::quiet`]).
//! - Limits: a systemd scope where this host gives one ([`scope_argv`]:
//!   memory, tasks), the space's own process and file limits, a small tmpfs,
//!   and no start below the disk floor (`coding::disk_alert::low_bytes`).
//!   NOT bounded: CPU (its scope has a low weight), network volume, disk a
//!   running turn writes; without a user systemd also memory.
//! - This daemon's stop, restart or self-update stops every space; the next
//!   start's reconcile brings them back on the new build.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use api::member_spaces::{HostedSpace, Provisioned};
use serde::{Deserialize, Serialize};

/// How long a space's daemon has to save and stop.
const GRACE: Duration = Duration::from_secs(45);
/// How long nothing of a space may take to be gone once it was ended.
const QUIET_WITHIN: Duration = Duration::from_secs(15);
/// When an end that failed is tried again (its save never is).
const END_RETRY: Duration = Duration::from_secs(5 * 60);
/// The memory one space may use: half the machine's, at most this.
const MEMORY_MAX: u64 = 16 << 30;
const TASKS_MAX: u64 = 4096;
/// Present once a space's last save was started: it is never run again.
const SAVED_FILE: &str = ".saved";
/// How long the last save of a removed space may take.
const SAVE_BUDGET: Duration = Duration::from_secs(120);
/// The wait after an unexpected end doubles from here to the maximum, and
/// starts over once a run lasted [`STABLE_AFTER`].
const BACKOFF_MIN: Duration = Duration::from_secs(5);
const BACKOFF_MAX: Duration = Duration::from_secs(5 * 60);
const STABLE_AFTER: Duration = Duration::from_secs(10 * 60);
/// How often the logins a space holds are renewed.
const LOGINS_EVERY: Duration = Duration::from_secs(15);
/// The probe behind the doctor item, again while it fails and once an hour.
const PROBE_FAILED_EVERY: Duration = Duration::from_secs(5 * 60);
const PROBE_OK_EVERY: Duration = Duration::from_secs(60 * 60);
/// The exit code of a space's daemon whose key the server refused.
pub const EXIT_UNAUTHORIZED: i32 = 77;

/// `<data>/members`.
pub fn root(data_dir: &Path) -> PathBuf {
    data_dir.join("members")
}

const STATUS_FILE: &str = "status.json";
/// Present while `exponential logout` / `uninstall` ends the spaces.
const TEARDOWN_FILE: &str = "teardown";
pub const SPACE_FILE: &str = "space.json";

/// What the host tells a space's daemon (`host/space.json`, 0600).
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceFile {
    pub space_id: String,
    pub device_id: String,
    pub instance_url: String,
    pub key: String,
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub label: String,
    /// The host account it belongs to (a sign-out ends that account's).
    pub host_account: String,
}

/// One line of `exponential members` (`<data>/members/status.json`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MemberStatus {
    pub space_id: String,
    pub member: String,
    pub state: String,
    pub code: Option<String>,
}

/// What the server may name as a path component: a lowercase UUID.
fn valid_space_id(id: &str) -> bool {
    id.len() == 36 && uuid::Uuid::try_parse(id).is_ok_and(|uuid| uuid.hyphenated().to_string() == id)
}

fn valid(space: &HostedSpace) -> bool {
    valid_space_id(&space.space_id) && space.device_id.strip_prefix("space-").is_some_and(valid_space_id)
}

// ── the reconcile ─────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Start(String),
    /// Stop it, save, delete its directory.
    End(String),
}

/// Starts and ends that bring this host to `hosted`. `None` (no answer:
/// offline, an older server) = nothing. `supervised` = spaces this process
/// keeps running, waiting out a restart included; `on_disk` = directories.
pub fn plan(hosted: Option<&[HostedSpace]>, supervised: &BTreeSet<String>, on_disk: &BTreeSet<String>) -> Vec<Action> {
    let Some(hosted) = hosted else { return Vec::new() };
    let wanted: BTreeSet<&String> = hosted.iter().filter(|space| valid(space)).map(|space| &space.space_id).collect();
    let ends = supervised.union(on_disk).filter(|id| !wanted.contains(id)).map(|id| Action::End(id.clone()));
    let starts = wanted.iter().filter(|id| !supervised.contains(**id)).map(|id| Action::Start((*id).clone()));
    ends.chain(starts).collect()
}

/// The wait before a space that ended by itself is started again.
fn backoff(failures: u32) -> Duration {
    BACKOFF_MIN.saturating_mul(1 << failures.min(10)).min(BACKOFF_MAX)
}

fn spaces_on_disk(data_dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(root(data_dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| valid_space_id(name))
        .collect()
}

// ── one space ─────────────────────────────────────────────────────────────

/// The end of a launched process: its stdin asks it to stop, its stdout
/// reports.
struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    since: Instant,
}

#[derive(Clone, Debug, Default)]
struct Report {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
}

struct Space {
    hosted: HostedSpace,
    running: Option<Running>,
    failures: u32,
    next_start: Instant,
    /// The key the server refused: provisioned again at the next start.
    reprovision: bool,
    report: Arc<Mutex<Report>>,
}

/// A report line of a space's daemon (its stdout): the only thing the host
/// reads from a space, bounded and checked.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReportLine {
    pub state: String,
    pub code: Option<String>,
}

fn parse_report(line: &str) -> Option<ReportLine> {
    let mut report: ReportLine = serde_json::from_str(line).ok()?;
    if !["starting", "waiting", "ready"].contains(&report.state.as_str()) {
        return None;
    }
    report.code = report.code.filter(|code| {
        code.len() == 11
            && code.bytes().enumerate().all(|(i, b)| if i == 5 { b == b' ' } else { b.is_ascii_digit() || b.is_ascii_uppercase() })
    });
    Some(report)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

/// What a space writes, fit for a log and a terminal.
fn plain(text: &str) -> String {
    let bidi = |c: char| matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}');
    text.chars().filter(|c| !c.is_control() && !bidi(*c)).take(200).collect()
}

/// The machine's memory, in bytes.
fn memory_total() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = text.lines().find(|line| line.starts_with("MemTotal:"))?;
    line.split_whitespace().nth(1)?.parse::<u64>().ok().map(|kb| kb * 1024)
}

/// `systemd-run`'s arguments for one space: its own scope with hard limits;
/// `env -i` again, the scope's environment is the host's.
fn scope_argv(memory_max: u64) -> Vec<String> {
    let mut argv: Vec<String> = ["--user", "--scope", "--quiet", "--collect"].map(String::from).to_vec();
    for limit in [
        format!("MemoryMax={memory_max}"),
        format!("MemoryHigh={}", memory_max / 10 * 9),
        "MemorySwapMax=0".to_string(),
        format!("TasksMax={TASKS_MAX}"),
        "CPUWeight=50".to_string(),
    ] {
        argv.extend(["-p".to_string(), limit]);
    }
    argv.extend(["--", "/usr/bin/env", "-i"].map(String::from));
    argv
}

/// The scope's arguments when this host's user manager takes them (asked
/// once); None = no such limits here.
fn scope() -> Option<&'static Vec<String>> {
    static SCOPE: std::sync::OnceLock<Option<Vec<String>>> = std::sync::OnceLock::new();
    SCOPE
        .get_or_init(|| {
            if !cfg!(target_os = "linux") {
                return None;
            }
            let argv = scope_argv((memory_total()? / 2).min(MEMORY_MAX));
            let works = std::process::Command::new("systemd-run")
                .args(&argv)
                .arg("/usr/bin/true")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            if !works {
                log::warn!("member spaces: no systemd scope here: their memory is not limited");
            }
            works.then_some(argv)
        })
        .as_ref()
}

/// [`member_space::command`], in a scope of its own where there is one.
fn launcher(exe: &Path) -> std::process::Command {
    use std::os::unix::process::CommandExt as _;
    use std::process::Stdio;
    let Some(argv) = scope() else { return member_space::command(exe) };
    let mut command = std::process::Command::new("systemd-run");
    command
        .args(argv)
        .arg(exe)
        .arg(member_space::LAUNCHER_ARG)
        .current_dir("/")
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// Below the device's low-disk floor: no space starts and none takes new work.
fn disk_low(data_dir: &Path) -> bool {
    coding::worktree_lifecycle::free_bytes(data_dir).is_some_and(|free| free < coding::disk_alert::low_bytes(data_dir))
}

/// Lines of at most 4 KiB; a longer one is dropped with the rest of it.
fn bounded_lines(pipe: impl std::io::Read, mut each: impl FnMut(&str)) {
    let mut reader = BufReader::new(pipe);
    let mut line = Vec::new();
    loop {
        line.clear();
        match (&mut reader).take(4096).read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => each(String::from_utf8_lossy(&line).trim()),
        }
    }
}

impl Running {
    fn launch(spec: &member_space::Spec, report: Arc<Mutex<Report>>, id: &str) -> anyhow::Result<Running> {
        let exe = crate::commands::update::running_exe()?;
        let mut child = launcher(&exe).spawn()?;
        let mut stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("no stdin"))?;
        stdin.write_all(member_space::spec_line(spec).as_bytes())?;
        if let Some(out) = child.stdout.take() {
            let report = Arc::clone(&report);
            std::thread::Builder::new().name("exp-space-report".into()).spawn(move || {
                bounded_lines(out, |line| {
                    if let Some(line) = parse_report(line) {
                        let mut report = lock(&report);
                        report.state = Some(line.state);
                        report.code = line.code;
                        report.error = None;
                    }
                })
            })?;
        }
        if let Some(err) = child.stderr.take() {
            let id = id.to_string();
            std::thread::Builder::new().name("exp-space-log".into()).spawn(move || {
                bounded_lines(err, |line| {
                    if line.is_empty() {
                        return;
                    }
                    // The launcher's own lines are the host's to know; what
                    // the space's daemon logs stays in the space.
                    if let Some(setup) = line.strip_prefix("member-space: ") {
                        // The space's program writes to this pipe too.
                        let setup = plain(setup);
                        log::warn!("member space {id}: {setup}");
                        lock(&report).error = Some(setup);
                    }
                })
            })?;
        }
        Ok(Running { child, stdin: Some(stdin), since: Instant::now() })
    }

    /// Asks it to stop (its daemon suspends and saves).
    fn ask_stop(&mut self) {
        if let Some(mut stdin) = self.stdin.take() {
            let _ = stdin.write_all(b"stop\n");
        }
    }

    fn wait(&mut self, budget: Duration) -> Option<i32> {
        let deadline = Instant::now() + budget;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return Some(status.code().unwrap_or(-1)),
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
                _ => {
                    // The launcher's end ends everything in the space.
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    return None;
                }
            }
        }
    }
}

// ── the supervisor ────────────────────────────────────────────────────────

/// The host's calls (tRPC in the daemon, a double in the tests). BLOCKING.
pub trait MembersApi: Send + Sync + 'static {
    fn hosted(&self, device_id: &str) -> Result<Vec<HostedSpace>, api::ApiError>;
    fn provision(&self, space_id: &str) -> Result<Provisioned, api::ApiError>;
}

impl MembersApi for api::TrpcClient {
    fn hosted(&self, device_id: &str) -> Result<Vec<HostedSpace>, api::ApiError> {
        api::member_spaces::hosted(self, device_id)
    }
    fn provision(&self, space_id: &str) -> Result<Provisioned, api::ApiError> {
        api::member_spaces::provision(self, space_id)
    }
}

pub struct Deps {
    pub data_dir: PathBuf,
    pub device_id: String,
    pub instance_url: String,
    pub account_id: String,
    pub api: Arc<dyn MembersApi>,
    /// The device's account status as its last collection said it.
    pub status: Box<dyn Fn() -> coding::AgentStatusPayload + Send>,
    /// Raised when the probe's answer moved (the beat carries the doctor).
    pub beat_soon: Arc<std::sync::atomic::AtomicBool>,
}

enum Work {
    Reconcile,
    /// Stop every space and answer when done (a stop, a self-update).
    StopAll(flume::Sender<()>),
    /// A self-update that did not happen: the spaces come back.
    Resume,
}

/// The daemon's handle on its spaces.
#[derive(Clone)]
pub struct Members {
    tx: flume::Sender<Work>,
    probed: Arc<Mutex<Option<Result<(), String>>>>,
}

impl Members {
    /// Linux servers only; elsewhere nothing starts and the doctor says so.
    pub fn spawn(deps: Deps) -> Members {
        let (tx, rx) = flume::unbounded();
        let probed = Arc::new(Mutex::new(None));
        let members = Members { tx, probed: Arc::clone(&probed) };
        let spawned = std::thread::Builder::new().name("exp-members".into()).spawn(move || {
            Supervisor { deps, spaces: BTreeMap::new(), probed, probed_at: None, logins_at: None, status: Vec::new(), held: false, ending: BTreeMap::new(), end_failed: BTreeMap::new() }.run(rx)
        });
        if let Err(err) = spawned {
            log::warn!("member spaces: no supervisor: {err}");
        }
        members
    }

    /// A heartbeat or a nudge: reconcile now.
    pub fn reconcile(&self) {
        let _ = self.tx.send(Work::Reconcile);
    }

    /// Stops every space (nothing is deleted); returns when they are down.
    pub fn stop_all(&self) {
        let (done, wait) = flume::bounded(1);
        if self.tx.send(Work::StopAll(done)).is_ok() {
            let _ = wait.recv_timeout(GRACE + Duration::from_secs(15));
        }
    }

    pub fn resume(&self) {
        let _ = self.tx.send(Work::Resume);
    }

    /// The doctor item ([`coding::device_doctor::add_member_space`]); none
    /// before the first probe answered.
    pub fn add_doctor(&self, doctor: &mut coding::device_doctor::DeviceDoctor) {
        if let Some(probed) = lock(&self.probed).as_ref() {
            coding::device_doctor::add_member_space(doctor, probed);
        }
    }
}

struct Supervisor {
    deps: Deps,
    spaces: BTreeMap<String, Space>,
    probed: Arc<Mutex<Option<Result<(), String>>>>,
    probed_at: Option<Instant>,
    logins_at: Option<Instant>,
    /// What `status.json` last said.
    status: Vec<MemberStatus>,
    /// Stopped for a restart: nothing starts.
    held: bool,
    /// Spaces that end, each on its own thread (true = it is gone).
    ending: BTreeMap<String, std::thread::JoinHandle<bool>>,
    /// Ends that did not finish, and when.
    end_failed: BTreeMap<String, Instant>,
}

impl Supervisor {
    fn run(mut self, rx: flume::Receiver<Work>) {
        self.probe();
        loop {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(Work::Reconcile) => {
                    // One answer serves every nudge that queued up.
                    let more: Vec<Work> = rx.drain().collect();
                    self.reconcile();
                    for work in more {
                        match work {
                            Work::Reconcile => {}
                            other => self.handle(other),
                        }
                    }
                }
                Ok(work) => self.handle(work),
                Err(flume::RecvTimeoutError::Timeout) => {}
                Err(flume::RecvTimeoutError::Disconnected) => {
                    self.stop_all();
                    return;
                }
            }
            self.tick();
        }
    }

    fn handle(&mut self, work: Work) {
        match work {
            Work::Reconcile => self.reconcile(),
            Work::StopAll(done) => {
                self.held = true;
                self.stop_all();
                let _ = done.send(());
            }
            Work::Resume => self.held = false,
        }
    }

    fn probe(&mut self) {
        let answer = match crate::commands::update::running_exe() {
            Ok(exe) => member_space::probe(&exe),
            Err(err) => Err(err.to_string()),
        };
        self.probed_at = Some(Instant::now());
        let mut probed = lock(&self.probed);
        if probed.as_ref() != Some(&answer) {
            if let Err(reason) = &answer {
                log::info!("member spaces cannot run here: {reason}");
            }
            *probed = Some(answer);
            self.deps.beat_soon.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn can_isolate(&self) -> bool {
        matches!(*lock(&self.probed), Some(Ok(())))
    }

    fn tick(&mut self) {
        let every = if self.can_isolate() { PROBE_OK_EVERY } else { PROBE_FAILED_EVERY };
        if self.probed_at.is_none_or(|at| at.elapsed() >= every) {
            self.probe();
        }
        if root(&self.deps.data_dir).join(TEARDOWN_FILE).exists() {
            self.held = true;
            for id in self.spaces.keys().cloned().collect::<Vec<_>>() {
                self.end(&id);
            }
        }
        self.reap();
        self.reap_ends();
        if !self.held && !disk_low(&self.deps.data_dir) {
            let due: Vec<String> = self
                .spaces
                .iter()
                .filter(|(_, space)| space.running.is_none() && Instant::now() >= space.next_start)
                .map(|(id, _)| id.clone())
                .collect();
            for id in due {
                self.start(&id);
            }
        }
        if !self.spaces.is_empty() && self.logins_at.is_none_or(|at| at.elapsed() >= LOGINS_EVERY) {
            self.logins_at = Some(Instant::now());
            let logins = coding::space_logins::collect(&self.deps.data_dir, (self.deps.status)());
            for id in self.spaces.keys() {
                if let Err(err) = coding::space_logins::write(&self.dir(id).join("host"), &logins) {
                    log::warn!("member space {id}: the logins were not written: {err}");
                }
            }
        }
        self.write_status();
    }

    fn dir(&self, id: &str) -> PathBuf {
        root(&self.deps.data_dir).join(id)
    }

    fn reconcile(&mut self) {
        let hosted = match self.deps.api.hosted(&self.deps.device_id) {
            Ok(hosted) => hosted,
            // Offline, or a server that knows no spaces: nothing changes.
            Err(err) => {
                log::debug!("memberSpaces.hosted: {err}");
                return;
            }
        };
        for space in &hosted {
            if let Some(known) = self.spaces.get_mut(&space.space_id) {
                known.hosted = space.clone();
            }
        }
        // One that ends, or whose end waits to be tried again, is left alone.
        let mut supervised: BTreeSet<String> = self.spaces.keys().cloned().collect();
        let busy: BTreeSet<String> = self.ending.keys().chain(self.end_failed.keys()).cloned().collect();
        supervised.extend(busy.iter().cloned());
        for action in plan(Some(&hosted), &supervised, &spaces_on_disk(&self.deps.data_dir)) {
            match action {
                Action::Start(id) => {
                    let Some(space) = hosted.iter().find(|space| space.space_id == id) else { continue };
                    self.spaces.insert(
                        id.clone(),
                        Space {
                            hosted: space.clone(),
                            running: None,
                            failures: 0,
                            next_start: Instant::now(),
                            reprovision: false,
                            report: Arc::default(),
                        },
                    );
                }
                Action::End(id) if busy.contains(&id) => {}
                Action::End(id) => self.end(&id),
            }
        }
    }

    fn reap_ends(&mut self) {
        let done: Vec<String> = self.ending.iter().filter(|(_, end)| end.is_finished()).map(|(id, _)| id.clone()).collect();
        for id in done {
            let gone = self.ending.remove(&id).is_some_and(|end| end.join().unwrap_or(false));
            if !gone {
                self.end_failed.insert(id, Instant::now());
            }
        }
        self.end_failed.retain(|_, at| at.elapsed() < END_RETRY);
    }

    /// A space that ended by itself waits, then starts again.
    fn reap(&mut self) {
        for (id, space) in &mut self.spaces {
            let Some(running) = space.running.as_mut() else { continue };
            let Ok(Some(status)) = running.child.try_wait() else { continue };
            let lasted = running.since.elapsed();
            space.running = None;
            space.reprovision |= status.code() == Some(EXIT_UNAUTHORIZED);
            space.failures = if lasted >= STABLE_AFTER { 0 } else { space.failures.saturating_add(1) };
            let wait = backoff(space.failures.saturating_sub(1));
            space.next_start = Instant::now() + wait;
            lock(&space.report).state = None;
            log::warn!("member space {id} ended ({status}); starting again in {}s", wait.as_secs());
        }
    }

    fn start(&mut self, id: &str) {
        // Never unisolated: without a probe that passed nothing starts.
        if !self.can_isolate() {
            return;
        }
        let result = self.launch(id);
        let Some(space) = self.spaces.get_mut(id) else { return };
        match result {
            Ok(running) => {
                log::info!("member space {id} started");
                space.running = Some(running);
            }
            Err(err) => {
                space.failures = space.failures.saturating_add(1);
                space.next_start = Instant::now() + backoff(space.failures.saturating_sub(1));
                lock(&space.report).error = Some(format!("{err:#}"));
                log::warn!("member space {id} did not start: {err:#}");
            }
        }
    }

    fn launch(&mut self, id: &str) -> anyhow::Result<Running> {
        let dir = self.dir(id);
        private_dir(&root(&self.deps.data_dir))?;
        for path in [dir.clone(), dir.join("data"), dir.join("host")] {
            private_dir(&path)?;
        }
        let Some(space) = self.spaces.get_mut(id) else { anyhow::bail!("not hosted") };
        let file = dir.join("host").join(SPACE_FILE);
        let _ = std::fs::remove_file(dir.join("host").join(SAVED_FILE));
        if space.reprovision || !file.is_file() {
            let answer = self.deps.api.provision(id)?;
            anyhow::ensure!(answer.device_id == space.hosted.device_id, "the key names another device");
            let label = if answer.label.is_empty() { space.hosted.label.clone() } else { answer.label };
            let content = SpaceFile {
                space_id: id.to_string(),
                device_id: answer.device_id,
                instance_url: self.deps.instance_url.clone(),
                key: answer.key,
                user_id: answer.user_id,
                email: answer.email.unwrap_or_default(),
                name: space.hosted.member_name.clone(),
                label,
                host_account: self.deps.account_id.clone(),
            };
            coding::atomic_config::write_atomic(&file, &serde_json::to_string(&content)?)?;
            space.reprovision = false;
        }
        let logins = coding::space_logins::collect(&self.deps.data_dir, (self.deps.status)());
        coding::space_logins::write(&dir.join("host"), &logins)?;
        let spec = spec(&self.deps.data_dir, &self.deps.instance_url, &dir, &["__space-daemon"])?;
        let report = Arc::clone(&space.report);
        lock(&report).state = Some("starting".into());
        Running::launch(&spec, report, id)
    }

    /// Stops a space, saves what is not pushed, deletes its directory.
    fn end(&mut self, id: &str) {
        if self.ending.contains_key(id) {
            return;
        }
        let running = self.spaces.remove(id).and_then(|mut space| space.running.take());
        let (data_dir, instance, name) = (self.deps.data_dir.clone(), self.deps.instance_url.clone(), id.to_string());
        let end = std::thread::Builder::new().name("exp-space-end".into()).spawn(move || {
            if let Some(mut running) = running {
                running.ask_stop();
                running.wait(GRACE + Duration::from_secs(10));
            }
            remove(&data_dir, &instance, &name)
        });
        match end {
            Ok(end) => drop(self.ending.insert(id.to_string(), end)),
            Err(err) => log::warn!("member space {id}: its end did not start: {err}"),
        }
    }

    fn stop_all(&mut self) {
        for space in self.spaces.values_mut() {
            if let Some(running) = space.running.as_mut() {
                running.ask_stop();
            }
        }
        let deadline = Instant::now() + GRACE + Duration::from_secs(10);
        for space in self.spaces.values_mut() {
            if let Some(mut running) = space.running.take() {
                running.wait(deadline.saturating_duration_since(Instant::now()));
            }
        }
        self.write_status();
    }

    fn write_status(&mut self) {
        let now: Vec<MemberStatus> = self
            .spaces
            .iter()
            .map(|(id, space)| {
                let report = lock(&space.report);
                let state = match (&space.running, &report.state, &report.error) {
                    (Some(_), Some(state), _) => state.clone(),
                    (Some(_), None, _) => "starting".into(),
                    (None, _, Some(error)) => format!("stopped: {error}"),
                    (None, _, None) => "stopped".into(),
                };
                MemberStatus { space_id: id.clone(), member: space.hosted.member_name.clone(), state, code: report.code.clone() }
            })
            .chain(self.ending.keys().map(|id| MemberStatus { space_id: id.clone(), state: "ending".into(), ..Default::default() }))
            .collect();
        let path = root(&self.deps.data_dir).join(STATUS_FILE);
        if now == self.status && (now.is_empty() || path.is_file()) {
            return;
        }
        if now.is_empty() {
            let _ = std::fs::remove_file(&path);
        } else if let Ok(text) = serde_json::to_string(&now) {
            if let Err(err) = coding::atomic_config::write_atomic(&path, &text) {
                log::debug!("member spaces: the status was not written: {err}");
            }
        }
        self.status = now;
    }
}

fn private_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

/// The instance's host and port: the one address a space reaches whatever
/// network it lies in.
fn instance_endpoint(url: &str) -> Option<(String, u16)> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?.rsplit('@').next()?;
    let default = if scheme == "http" { 80 } else { 443 };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !port.contains(']') => (host, port.parse().ok()?),
        _ => (authority, default),
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    (!host.is_empty()).then(|| (host.to_string(), port))
}

/// Where a space's daemon finds the instance: its own URL, or the stand-in
/// address when that names the host's loopback.
pub fn instance_url_inside(url: &str) -> String {
    let Some((host, port)) = instance_endpoint(url) else { return url.to_string() };
    let loopback = host == "localhost" || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback());
    if !loopback {
        return url.to_string();
    }
    let scheme = url.split("://").next().unwrap_or("http");
    format!("{scheme}://{}:{port}", member_space::net::INSTANCE_ADDR)
}

fn spec(data_dir: &Path, instance_url: &str, dir: &Path, args: &[&str]) -> anyhow::Result<member_space::Spec> {
    let settings = coding::Settings::load(&coding::Settings::default_path(data_dir));
    let mut tools = vec![("exponential".to_string(), crate::commands::update::running_exe()?.canonicalize()?)];
    for agent in coding::CodingAgent::ALL {
        if let Some(path) = tool(&settings.resolved_path_for(agent)) {
            tools.push((agent.id().to_string(), path));
        }
    }
    Ok(member_space::Spec {
        data: dir.join("data").canonicalize()?,
        host: dir.join("host").canonicalize()?,
        tools,
        args: args.iter().map(|arg| arg.to_string()).collect(),
        env: member_space::environment(std::env::vars()),
        instance: instance_endpoint(instance_url),
        only: None,
        grace_secs: GRACE.as_secs(),
    })
}

/// An agent's program as one file a space can be given; None = a package
/// tree (npm) or nothing: that agent is not there.
fn tool(program: &str) -> Option<PathBuf> {
    let path = Path::new(program);
    let found = if path.is_absolute() {
        Some(path.to_path_buf())
    } else {
        let login_path = terminal::pty::login_path();
        std::env::split_paths(&login_path).map(|dir| dir.join(program)).find(|candidate| candidate.is_file())
    }?;
    let real = found.canonicalize().ok()?;
    member_space::bindable(&real).then_some(real)
}

/// Waits until nothing of the space runs; its lock, held.
fn wait_quiet(host: &Path) -> Result<impl Sized, String> {
    let deadline = Instant::now() + QUIET_WITHIN;
    loop {
        match member_space::quiet(host) {
            Ok(lock) => return Ok(lock),
            Err(err) if Instant::now() >= deadline => return Err(err),
            Err(_) => std::thread::sleep(Duration::from_millis(200)),
        }
    }
}

/// Deletes a stopped space: its key and logins, then (once in its life) the
/// last save, from inside the same isolation (a worktree is the member's,
/// its git hooks and filters are too) and reaching the git remote alone, then
/// its directory. Nothing is deleted under a live tree. True = it is gone.
fn remove(data_dir: &Path, instance_url: &str, id: &str) -> bool {
    remove_with(data_dir, id, |dir| {
        let mut spec = spec(data_dir, instance_url, dir, &["__space-save"])?;
        spec.only = Some(vec![("github.com".to_string(), 443)]);
        let mut running = Running::launch(&spec, Arc::default(), id)?;
        anyhow::ensure!(running.wait(SAVE_BUDGET) == Some(0), "it did not finish");
        Ok(())
    })
}

fn remove_with(data_dir: &Path, id: &str, save: impl FnOnce(&Path) -> anyhow::Result<()>) -> bool {
    if !valid_space_id(id) {
        return false;
    }
    let dir = root(data_dir).join(id);
    let host = dir.join("host");
    let failed = |what: &str, err: &dyn std::fmt::Display| {
        log::warn!("member space {id}: {what}: {err}");
        false
    };
    if host.is_dir() {
        let lock = match wait_quiet(&host) {
            Ok(lock) => lock,
            Err(err) => return failed("not removed, something of it runs", &err),
        };
        for secret in [SPACE_FILE, coding::space_logins::LOGINS_FILE] {
            match std::fs::remove_file(host.join(secret)) {
                Err(err) if err.kind() != std::io::ErrorKind::NotFound => return failed("its key was not deleted", &err),
                _ => {}
            }
        }
        // The launcher takes the lock itself.
        drop(lock);
        let marker = host.join(SAVED_FILE);
        if dir.join("data").is_dir() && !marker.exists() {
            if let Err(err) = std::fs::write(&marker, b"") {
                return failed("no last save", &err);
            }
            if let Err(err) = save(&dir) {
                failed("the last save did not finish", &format_args!("{err:#}"));
            }
        }
    }
    let _lock = if host.is_dir() {
        match wait_quiet(&host) {
            Ok(lock) => Some(lock),
            Err(err) => return failed("not removed, something of it runs", &err),
        }
    } else {
        None
    };
    match member_space::remove_tree(&dir) {
        Ok(()) => {
            log::info!("member space {id} removed");
            true
        }
        Err(err) => failed("the directory was not removed", &err),
    }
}

/// `exponential logout` / `uninstall`: ends the spaces of `account` (None =
/// every account's) and deletes their directories. A running daemon does it
/// itself; without one it is done here. BLOCKING.
pub fn teardown(data_dir: &Path, account: Option<&str>) {
    let of_account = |id: &String| {
        let file = root(data_dir).join(id).join("host").join(SPACE_FILE);
        let held = std::fs::read_to_string(file).ok().and_then(|text| serde_json::from_str::<SpaceFile>(&text).ok());
        match (account, held) {
            (Some(account), Some(held)) => held.host_account == account,
            _ => true,
        }
    };
    let ids: Vec<String> = spaces_on_disk(data_dir).into_iter().filter(of_account).collect();
    if ids.is_empty() {
        return;
    }
    let flag = root(data_dir).join(TEARDOWN_FILE);
    if crate::commands::daemon::daemon_pid(data_dir).is_some() && std::fs::write(&flag, b"").is_ok() {
        let deadline = Instant::now() + GRACE + SAVE_BUDGET;
        while Instant::now() < deadline && ids.iter().any(|id| root(data_dir).join(id).exists()) {
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    for id in &ids {
        let file = root(data_dir).join(id).join("host").join(SPACE_FILE);
        let instance = std::fs::read_to_string(file)
            .ok()
            .and_then(|text| serde_json::from_str::<SpaceFile>(&text).ok())
            .map(|file| file.instance_url)
            .unwrap_or_default();
        remove(data_dir, &instance, id);
    }
    let _ = std::fs::remove_file(root(data_dir).join(STATUS_FILE));
    let _ = std::fs::remove_file(flag);
}

// ── `exponential members` ─────────────────────────────────────────────────

/// What the daemon wrote, and spaces on disk it does not run.
pub fn status(data_dir: &Path) -> Vec<MemberStatus> {
    let daemon = crate::commands::daemon::daemon_pid(data_dir).is_some();
    let mut rows: Vec<MemberStatus> = std::fs::read_to_string(root(data_dir).join(STATUS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    if !daemon {
        for row in &mut rows {
            row.state = "stopped".into();
        }
    }
    for id in spaces_on_disk(data_dir) {
        if !rows.iter().any(|row| row.space_id == id) {
            rows.push(MemberStatus { space_id: id, state: "stopped".into(), ..Default::default() });
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "0b0f2f0e-0000-4000-8000-00000000000a";
    const B: &str = "0b0f2f0e-0000-4000-8000-00000000000b";

    fn hosted(id: &str) -> HostedSpace {
        HostedSpace {
            space_id: id.into(),
            device_id: format!("space-{id}"),
            member_id: "u".into(),
            member_name: "Ada".into(),
            label: "box".into(),
        }
    }

    fn set(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn the_reconcile_table() {
        let (start, end) = (|id: &str| Action::Start(id.into()), |id: &str| Action::End(id.into()));
        let cases: Vec<(&str, Option<Vec<HostedSpace>>, &[&str], &[&str], Vec<Action>)> = vec![
            ("nothing", Some(vec![]), &[], &[], vec![]),
            ("a new space starts", Some(vec![hosted(A)]), &[], &[], vec![start(A)]),
            ("a host restart brings it back", Some(vec![hosted(A)]), &[], &[A], vec![start(A)]),
            ("running stays", Some(vec![hosted(A)]), &[A], &[A], vec![]),
            ("gone is ended", Some(vec![]), &[A], &[A], vec![end(A)]),
            ("gone while the host was down is ended", Some(vec![]), &[], &[A], vec![end(A)]),
            ("one goes, one comes", Some(vec![hosted(B)]), &[A], &[A], vec![end(A), start(B)]),
            ("offline: nothing changes", None, &[A], &[A], vec![]),
            ("offline: nothing new starts", None, &[], &[A], vec![]),
            ("a crashed space is still supervised, its restart is the backoff's", Some(vec![hosted(A)]), &[A], &[A], vec![]),
        ];
        for (name, hosted, supervised, disk, want) in cases {
            assert_eq!(plan(hosted.as_deref(), &set(supervised), &set(disk)), want, "{name}");
        }
    }

    #[test]
    fn a_server_never_names_a_path() {
        for id in ["../x", "..", "", "a/b", &A.to_uppercase(), "0b0f2f0e00004000800000000000000a", "status.json"] {
            assert!(!valid_space_id(id), "{id}");
        }
        let mut bad = hosted(A);
        bad.device_id = "my-laptop".into();
        assert_eq!(plan(Some(&[bad]), &set(&[]), &set(&[])), vec![]);
        let mut bad = hosted(A);
        bad.space_id = "../../etc".into();
        assert_eq!(plan(Some(&[bad]), &set(&[]), &set(&[])), vec![]);
    }

    #[test]
    fn the_wait_after_a_crash_grows() {
        assert_eq!(backoff(0), Duration::from_secs(5));
        assert_eq!(backoff(1), Duration::from_secs(10));
        assert_eq!(backoff(3), Duration::from_secs(40));
        assert_eq!(backoff(9), BACKOFF_MAX);
        assert_eq!(backoff(u32::MAX), BACKOFF_MAX);
    }

    #[test]
    fn a_report_is_checked() {
        let ok = parse_report(r#"{"state":"waiting","code":"ABCDE 12345"}"#).unwrap();
        assert_eq!(ok.code.as_deref(), Some("ABCDE 12345"));
        assert_eq!(parse_report(r#"{"state":"ready","code":"\u001b[2J"}"#).unwrap().code, None);
        assert!(parse_report(r#"{"state":"\u001b[2J"}"#).is_none());
        assert!(parse_report("junk").is_none());
        let mut lines = Vec::new();
        let long = format!("{}\nok\n", "x".repeat(10_000));
        bounded_lines(long.as_bytes(), |line| lines.push(line.len()));
        assert!(lines.iter().all(|len| *len <= 4096));
        assert_eq!(lines.last(), Some(&2));
    }

    #[test]
    fn the_instance_inside_a_space() {
        assert_eq!(instance_endpoint("https://app.exponential.at"), Some(("app.exponential.at".into(), 443)));
        assert_eq!(instance_endpoint("http://localhost:3000/x"), Some(("localhost".into(), 3000)));
        assert_eq!(instance_endpoint("http://[::1]:8080"), Some(("::1".into(), 8080)));
        assert_eq!(instance_url_inside("https://app.exponential.at"), "https://app.exponential.at");
        assert_eq!(instance_url_inside("http://localhost:3000"), "http://198.18.0.3:3000");
        assert_eq!(instance_url_inside("http://[::1]:8080"), "http://198.18.0.3:8080");
    }

    struct Fake {
        hosted: Mutex<Result<Vec<HostedSpace>, ()>>,
        provisions: Mutex<Vec<String>>,
    }

    impl MembersApi for Fake {
        fn hosted(&self, _device_id: &str) -> Result<Vec<HostedSpace>, api::ApiError> {
            lock(&self.hosted).clone().map_err(|()| api::ApiError::Transport { message: "offline".into(), offline: true })
        }
        fn provision(&self, space_id: &str) -> Result<Provisioned, api::ApiError> {
            lock(&self.provisions).push(space_id.into());
            Err(api::ApiError::Http { status: 404, message: "gone".into() })
        }
    }

    fn supervisor(api: Arc<Fake>) -> Supervisor {
        let data_dir = std::env::temp_dir().join(format!("exp-members-{}-{}", std::process::id(), uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&data_dir).unwrap();
        Supervisor {
            deps: Deps {
                data_dir,
                device_id: "host".into(),
                instance_url: "https://x.test".into(),
                account_id: "acc".into(),
                api,
                status: Box::new(Default::default),
                beat_soon: Arc::default(),
            },
            spaces: BTreeMap::new(),
            probed: Arc::new(Mutex::new(None)),
            probed_at: Some(Instant::now()),
            logins_at: None,
            status: Vec::new(),
            held: false,
            ending: BTreeMap::new(),
            end_failed: BTreeMap::new(),
        }
    }

    fn join_ends(host: &mut Supervisor) {
        let deadline = Instant::now() + Duration::from_secs(60);
        while !host.ending.is_empty() && Instant::now() < deadline {
            host.reap_ends();
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn nothing_starts_without_a_passed_probe_and_a_failed_start_waits() {
        let api = Arc::new(Fake { hosted: Mutex::new(Ok(vec![hosted(A)])), provisions: Mutex::default() });
        let mut host = supervisor(Arc::clone(&api));
        host.reconcile();
        for probed in [None, Some(Err("User namespaces are off".to_string()))] {
            *lock(&host.probed) = probed;
            host.tick();
            assert!(lock(&api.provisions).is_empty());
            assert!(host.spaces[A].running.is_none());
        }
        *lock(&host.probed) = Some(Ok(()));
        host.tick();
        assert_eq!(*lock(&api.provisions), vec![A.to_string()]);
        assert_eq!(host.spaces[A].failures, 1);
        assert!(host.spaces[A].next_start > Instant::now());
        host.tick();
        assert_eq!(lock(&api.provisions).len(), 1, "the backoff holds");
        assert_eq!(status(&host.deps.data_dir)[0].state, "stopped");

        // Offline: it stays supervised. Gone: its directory is deleted.
        *lock(&api.hosted) = Err(());
        host.reconcile();
        assert!(host.spaces.contains_key(A));
        let dir = host.dir(A);
        std::fs::remove_dir_all(dir.join("data")).unwrap();
        std::os::unix::fs::symlink("/", dir.join("link")).unwrap();
        *lock(&api.hosted) = Ok(vec![]);
        host.reconcile();
        join_ends(&mut host);
        assert!(host.spaces.is_empty() && host.end_failed.is_empty() && !dir.exists());
        assert!(Path::new("/etc").exists());
        let _ = std::fs::remove_dir_all(&host.deps.data_dir);
    }

    /// Review finding 5: one save in a space's life, its secrets gone before
    /// it, never a delete while something of it runs.
    #[test]
    fn a_space_is_saved_once_and_never_removed_while_it_runs() {
        use std::os::unix::fs::PermissionsExt as _;
        let data_dir = std::env::temp_dir().join(format!("exp-members-end-{}", uuid::Uuid::new_v4()));
        let dir = root(&data_dir).join(A);
        let (data, host) = (dir.join("data"), dir.join("host"));
        std::fs::create_dir_all(data.join("closed")).unwrap();
        std::fs::create_dir_all(&host).unwrap();
        for secret in [SPACE_FILE, coding::space_logins::LOGINS_FILE] {
            std::fs::write(host.join(secret), "secret").unwrap();
        }
        std::fs::set_permissions(data.join("closed"), std::fs::Permissions::from_mode(0)).unwrap();
        let saves = std::cell::Cell::new(0);
        let save = |_: &Path| {
            saves.set(saves.get() + 1);
            assert!(!host.join(SPACE_FILE).exists() && !host.join(coding::space_logins::LOGINS_FILE).exists());
            anyhow::bail!("a member's filter that hangs")
        };
        if cfg!(target_os = "linux") {
            // Something of it still runs: nothing is saved, nothing deleted.
            let alive = member_space::quiet(&host).ok().unwrap();
            assert!(!remove_with(&data_dir, A, save) && dir.exists() && saves.get() == 0);
            drop(alive);
        }
        assert!(remove_with(&data_dir, A, save));
        assert!(!dir.exists() && saves.get() == 1);
        // A delete that failed half way: no second save.
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&host).unwrap();
        std::fs::write(host.join(SAVED_FILE), "").unwrap();
        assert!(remove_with(&data_dir, A, save));
        assert!(!dir.exists() && saves.get() == 1);
        assert!(!remove_with(&data_dir, "../x", save));
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    /// Finding 5: a slow end blocks no other space.
    #[test]
    fn an_end_runs_beside_the_rest() {
        let api = Arc::new(Fake { hosted: Mutex::new(Ok(vec![])), provisions: Mutex::default() });
        let mut host = supervisor(Arc::clone(&api));
        host.ending.insert(A.into(), std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(400));
            false
        }));
        std::fs::create_dir_all(host.dir(A)).unwrap();
        let since = Instant::now();
        // It is not ended a second time, nor started while it ends.
        *lock(&api.hosted) = Ok(vec![hosted(A), hosted(B)]);
        host.reconcile();
        assert!(since.elapsed() < Duration::from_millis(300));
        assert!(host.spaces.contains_key(B) && !host.spaces.contains_key(A));
        join_ends(&mut host);
        // Failed: left alone until it is tried again.
        assert!(host.end_failed.contains_key(A));
        *lock(&api.hosted) = Ok(vec![]);
        host.spaces.clear();
        host.reconcile();
        assert!(host.ending.is_empty());
        let _ = std::fs::remove_dir_all(&host.deps.data_dir);
    }

    #[test]
    fn what_a_space_writes_is_plain_text() {
        assert_eq!(plain("a\u{1b}[2J\u{7}b\u{202e}c\u{200b}d\r"), "a[2Jbcd");
        assert_eq!(plain(&"x".repeat(500)).len(), 200);
    }

    #[test]
    fn a_scope_is_a_hard_limit_and_a_clean_environment() {
        let argv = scope_argv(8 << 30);
        for want in ["MemoryMax=8589934592", "MemorySwapMax=0", "TasksMax=4096"] {
            assert!(argv.iter().any(|arg| arg == want), "{want}");
        }
        assert_eq!(argv[argv.len() - 3..], ["--", "/usr/bin/env", "-i"]);
    }

    #[test]
    fn the_key_file_is_the_owners_alone() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = std::env::temp_dir().join(format!("exp-members-dir-{}", uuid::Uuid::new_v4()));
        private_dir(&dir).unwrap();
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        coding::atomic_config::write_atomic(&dir.join(SPACE_FILE), "{}").unwrap();
        assert_eq!(std::fs::metadata(dir.join(SPACE_FILE)).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
