//! #EXP-1617 / #EXP-2174: a device as a CI runner, shared by the headless
//! daemon and the desktop app. A `ci_job` device command names ONE job; the
//! runner claims it (the server's compare-and-swap; losing is fine),
//! completes the command, takes the job's secrets once, and runs it through
//! `ecode::run_job` in a clean worktree under its work root, inside a
//! [`sandbox::JobSandbox`]. Log lines (already redacted) go up as chunks about
//! every second; the job beats every 15 s and stops when the server says so
//! (cancelled, timed out, lost). Slots and labels are the server's business:
//! it only dispatches what this runner's settings allow.
//!
//! Before it claims, the runner declines a job (the server dispatches it
//! again or elsewhere) while
//! - an update waits ([`set_draining`], #EXP-2144): the restart waits for the
//!   running jobs instead, at most until each one's timeout ([`hold_left`]);
//! - a game or a full-screen app runs and the settings say `pauseWhenBusy`;
//! - the host requires the namespace sandbox and this machine cannot hide the
//!   app's data from a job (#EXP-2181).
//!
//! A host that quits or restarts mid-job calls [`stop_all`]: each job is cut
//! and handed back (`ci.release`), and the server queues it again once.

pub mod busy;
pub mod sandbox;

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

use api::TrpcClient;
use ecode::{GitRemote, JobSpec, LogSink, MapSecrets, StepState};
use serde_json::Value;

pub use sandbox::{sandbox_main, Containment, JobSandbox, SANDBOX_ARG};

/// The device cap that makes the server dispatch `ci_job` here.
pub const CI_RUNNER_CAP: &str = "ci-runner";
/// The device command kind.
pub const CI_JOB_COMMAND: &str = "ci_job";

const FLUSH_EVERY: Duration = Duration::from_secs(1);
const FLUSH_BYTES: usize = 64 * 1024;
/// Under the server's chunk cap (256 KiB).
const CHUNK_MAX: usize = 192 * 1024;
const BEAT_EVERY: Duration = Duration::from_secs(15);

/// How a host runs jobs.
#[derive(Debug)]
pub struct Runner {
    /// Mirrors and job worktrees (`mirrors/`, `jobs/`). Never inside a
    /// hidden directory.
    pub work_root: PathBuf,
    /// What a job must not see; its `memory_max_mb` is overridden per job by
    /// the command's `memoryMaxMb`.
    pub containment: Containment,
    /// Refuse every job when the hidden paths cannot be hidden here (the
    /// desktop app: a job must never read its signed-in accounts).
    pub require_hidden: bool,
    ready: OnceLock<Result<(), String>>,
}

impl Runner {
    pub fn new(work_root: PathBuf, containment: Containment, require_hidden: bool) -> Self {
        Self { work_root, containment, require_hidden, ready: OnceLock::new() }
    }

    /// #EXP-2181: the desktop app's runner. Jobs live under the user's cache
    /// directory (`exponential-ci`, outside everything hidden) and cannot see
    /// any Exponential data or config directory (this app's, a daemon's on
    /// the same machine, a staging build's), the agents' sign-ins, the
    /// user's runtime directory (D-Bus, the systemd manager, the display
    /// sockets) or the X authority. Linux only; required, never best-effort.
    pub fn desktop(helper: PathBuf) -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        let mut hide_dirs: Vec<PathBuf> = vec![api::default_data_dir()];
        for base in [dirs::data_dir(), dirs::config_dir(), dirs::state_dir()].into_iter().flatten() {
            hide_dirs.extend(exponential_dirs(&base));
        }
        hide_dirs.extend([".claude", ".codex", ".ssh", ".gnupg"].map(|name| home.join(name)));
        if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").filter(|dir| !dir.is_empty()) {
            hide_dirs.push(PathBuf::from(runtime));
        }
        hide_dirs.sort();
        hide_dirs.dedup();
        let hide_files = [".claude.json", ".Xauthority", ".git-credentials"].map(|name| home.join(name)).to_vec();
        let work_root = dirs::cache_dir().unwrap_or_else(|| home.join(".cache")).join("exponential-ci");
        let containment = Containment { hide_dirs, hide_files, helper: Some(helper), memory_max_mb: None };
        Self::new(work_root, containment, true)
    }

    /// Whether this runner can run jobs at all (probed once): `Err` = the
    /// reason it cannot.
    pub fn readiness(&self) -> Result<(), String> {
        self.ready
            .get_or_init(|| {
                self.containment.check_work_root(&self.work_root)?;
                if self.containment.helper.is_none() {
                    return if self.require_hidden { Err("no sandbox helper".to_string()) } else { Ok(()) };
                }
                match sandbox::hiding_available(&self.containment, &self.work_root) {
                    Ok(()) => Ok(()),
                    Err(err) if self.require_hidden => Err(err),
                    Err(err) => {
                        log::warn!("ci runner: the namespace sandbox is unavailable ({err}); jobs run unhidden");
                        Ok(())
                    }
                }
            })
            .clone()
    }
}

/// Every `exponential*` / `at.exponential*` directory right under `base`.
fn exponential_dirs(base: &std::path::Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(base) else { return vec![] };
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            (name.starts_with("exponential") || name.starts_with("at.exponential")) && entry.path().is_dir()
        })
        .map(|entry| entry.path())
        .collect()
}

/// The `ci_job` command's payload: the job, plus the runner settings the
/// runner itself applies (absent from older servers).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobCommand {
    pub job_id: String,
    pub memory_max_mb: Option<u64>,
    pub pause_when_busy: bool,
}

impl JobCommand {
    pub fn parse(payload: &Value) -> Option<Self> {
        Some(Self {
            job_id: payload["jobId"].as_str().filter(|id| !id.is_empty())?.to_string(),
            memory_max_mb: payload["memoryMaxMb"].as_u64().filter(|mb| *mb > 0),
            pause_when_busy: payload["pauseWhenBusy"].as_bool().unwrap_or(false),
        })
    }
}

struct Live {
    deadline: Instant,
    abort: Arc<AtomicBool>,
}

static JOBS: LazyLock<Mutex<BTreeMap<String, Live>>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));
static COMMANDS: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
static DRAINING: AtomicBool = AtomicBool::new(false);

fn jobs() -> std::sync::MutexGuard<'static, BTreeMap<String, Live>> {
    JOBS.lock().unwrap_or_else(|e| e.into_inner())
}

/// #EXP-2144: an update is ready; while set, no new job is claimed.
pub fn set_draining(on: bool) {
    if DRAINING.swap(on, Ordering::SeqCst) != on {
        log::info!("ci runner: {}", if on { "an update waits: no new jobs" } else { "taking jobs again" });
    }
}

pub fn draining() -> bool {
    DRAINING.load(Ordering::SeqCst)
}

/// How many jobs run here now.
pub fn running() -> usize {
    jobs().len()
}

/// #EXP-2144: how long the running jobs may still hold a ready update: until
/// the last one's own timeout. `None` = no job runs.
pub fn hold_left(now: Instant) -> Option<Duration> {
    jobs().values().map(|live| live.deadline.saturating_duration_since(now)).max()
}

/// Cut every running job and hand it back to the server (`ci.release`: it is
/// queued again, once). Waits up to `wait` for the jobs to stop; the number
/// still running then.
pub fn stop_all(wait: Duration) -> usize {
    set_draining(true);
    for live in jobs().values() {
        live.abort.store(true, Ordering::SeqCst);
    }
    let until = Instant::now() + wait;
    while running() > 0 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(100));
    }
    running()
}

/// Handle one `ci_job` command on its own thread (redeliveries of a command
/// already in flight are dropped).
pub fn handle(runner: &Arc<Runner>, trpc: &Arc<TrpcClient>, device_id: &str, command_id: &str, payload: &Value) {
    let Some(command) = JobCommand::parse(payload) else {
        complete(trpc, command_id, false, "Malformed command payload.");
        return;
    };
    if !COMMANDS.lock().unwrap_or_else(|e| e.into_inner()).insert(command_id.to_string()) {
        return;
    }
    let runner = Arc::clone(runner);
    let trpc = Arc::clone(trpc);
    let device_id = device_id.to_string();
    let command_id = command_id.to_string();
    let spawned = std::thread::Builder::new().name("exp-ci-job".to_string()).spawn(move || {
        let (ok, message) = run(&runner, &trpc, &device_id, &command_id, &command);
        // A won claim completed the command already (this is then a no-op).
        complete(&trpc, &command_id, ok, &message);
        COMMANDS.lock().unwrap_or_else(|e| e.into_inner()).remove(&command_id);
    });
    if let Err(err) = spawned {
        log::warn!("ci job: could not start a thread: {err}");
    }
}

fn complete(trpc: &TrpcClient, command_id: &str, ok: bool, message: &str) {
    if let Err(err) = api::devices::complete_command(trpc, command_id, ok, Some(message)) {
        log::debug!("completeCommand {command_id} failed: {err}");
    }
}

/// Why this runner declines a job right now, before claiming it.
fn decline(runner: &Runner, command: &JobCommand) -> Option<(bool, String)> {
    if draining() {
        return Some((true, "An update is waiting: this runner takes no new jobs.".to_string()));
    }
    if let Err(reason) = runner.readiness() {
        return Some((false, format!("This computer cannot run CI jobs: {reason}.")));
    }
    if command.pause_when_busy {
        if let Some(reason) = busy::busy_reason() {
            return Some((true, format!("Paused: {reason}.")));
        }
    }
    None
}

/// The bearer the host's tRPC client sends (for ecode's own HTTP).
fn bearer(trpc: &TrpcClient) -> Option<String> {
    trpc.auth_headers()
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        .and_then(|(_, value)| value.strip_prefix("Bearer ").map(str::to_string))
}

/// `ci.jobSecrets`: the values (once) and the server's notice about the
/// names it withheld (#EXP-2180), absent from older servers.
fn job_secrets(trpc: &TrpcClient, job_id: &str, token: &str) -> Result<(Vec<(String, String)>, Option<String>), api::ApiError> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        job_id: &'a str,
        token: &'a str,
    }
    #[derive(serde::Deserialize)]
    struct Out {
        values: BTreeMap<String, String>,
        #[serde(default)]
        notice: Option<String>,
    }
    let out: Out = trpc.mutation("ci.jobSecrets", &Input { job_id, token })?;
    Ok((out.values.into_iter().collect(), out.notice.filter(|n| !n.is_empty())))
}

/// `ci.release{jobId, token}`: the job goes back to the queue (once).
fn release(trpc: &TrpcClient, job_id: &str, token: &str) -> Result<bool, api::ApiError> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        job_id: &'a str,
        token: &'a str,
    }
    #[derive(serde::Deserialize)]
    struct Out {
        requeued: bool,
    }
    let out: Out = trpc.mutation("ci.release", &Input { job_id, token })?;
    Ok(out.requeued)
}

/// Claim and run; the command's outcome. The command completes right after
/// the claim (the job's state lives on the server from then on).
fn run(runner: &Runner, trpc: &Arc<TrpcClient>, device_id: &str, command_id: &str, command: &JobCommand) -> (bool, String) {
    if let Some(declined) = decline(runner, command) {
        log::info!("ci job {}: declined ({})", command.job_id, declined.1);
        return declined;
    }
    let job_id = command.job_id.as_str();
    let claimed = match api::ci::claim(trpc, device_id, job_id) {
        Ok(Some(claimed)) => claimed,
        Ok(None) => return (true, "Another runner has this job.".to_string()),
        Err(err) => return (false, format!("Could not claim the job: {err}")),
    };
    complete(trpc, command_id, true, "Claimed.");
    let token = claimed.token.clone();
    let job: JobSpec = match serde_json::from_value(claimed.job.clone()) {
        Ok(job) => job,
        Err(err) => {
            let _ = api::ci::finish(trpc, job_id, &token, "startup_failure", &Value::Null);
            return (false, format!("Malformed job: {err}"));
        }
    };
    log::info!("ci job {} ({}): running {} step(s) at {}", job.id, job.check, job.steps.len(), job.sha);
    let abort = Arc::new(AtomicBool::new(false));
    let deadline = Instant::now() + Duration::from_secs(job.timeout_min.max(1) as u64 * 60);
    jobs().insert(job.id.clone(), Live { deadline, abort: Arc::clone(&abort) });
    let result = run_claimed(runner, trpc, &job, &token, &claimed.repository_id, command, &abort);
    jobs().remove(&job.id);
    result
}

fn run_claimed(
    runner: &Runner,
    trpc: &Arc<TrpcClient>,
    job: &JobSpec,
    token: &str,
    repository_id: &str,
    command: &JobCommand,
    abort: &Arc<AtomicBool>,
) -> (bool, String) {
    let mut sink = ServerSink {
        trpc: Arc::clone(trpc),
        job_id: job.id.clone(),
        token: token.to_string(),
        buffer: String::new(),
        seq: 0,
        log_open: true,
        last_flush: Instant::now(),
        last_beat: Instant::now(),
        steps: vec![],
        cancel: false,
        abort: Arc::clone(abort),
    };
    let fail_start = |sink: &mut ServerSink, line: &str, message: &str| {
        sink.log(&format!("##[error]{line}\n"));
        sink.flush();
        let _ = api::ci::finish(trpc, &job.id, token, "startup_failure", &Value::Null);
        (true, message.to_string())
    };
    let secrets = if job.secrets.is_empty() {
        MapSecrets::default()
    } else {
        match job_secrets(trpc, &job.id, token) {
            Ok((values, notice)) => {
                // #EXP-2180: a pull request run's secrets are withheld; the
                // server says so in one line, which goes into the log here.
                if let Some(notice) = notice {
                    sink.log(&format!("##[warning]{notice}\n"));
                }
                MapSecrets::new(values)
            }
            Err(err) => return fail_start(&mut sink, &format!("could not fetch the job's secrets: {err}"), "Secrets unavailable."),
        }
    };
    let Some(bearer) = bearer(trpc) else {
        return fail_start(&mut sink, "this runner is signed out", "Signed out.");
    };
    let remote = match ecode::exponential::ExponentialCredentials::new(trpc.base_url(), bearer, repository_id).and_then(GitRemote::new) {
        Ok(remote) => remote,
        Err(err) => return fail_start(&mut sink, &format!("no credentials for the repository: {err}"), "No repository credentials."),
    };
    let containment = Containment { memory_max_mb: command.memory_max_mb, ..runner.containment.clone() };
    let sandbox = JobSandbox::new(&runner.work_root, containment);
    let outcome = ecode::run_job(&sandbox, &remote, &secrets, job, &mut sink);
    sink.flush();
    if abort.load(Ordering::SeqCst) {
        // The host quits or restarts: the job goes back to the queue. Never
        // `finish` it: a server without `ci.release` concludes it by its own
        // clock (queued again once, else `runner_lost`).
        return match release(trpc, &job.id, token) {
            Ok(requeued) => {
                log::info!("ci job {}: handed back ({})", job.id, if requeued { "queued again" } else { "not queued again" });
                (true, "Handed back.".to_string())
            }
            Err(err) => {
                log::warn!("ci job {}: release failed ({err}); the server's clock will conclude it", job.id);
                (true, "Cut by a restart.".to_string())
            }
        };
    }
    let steps = serde_json::to_value(&outcome.steps).unwrap_or(Value::Null);
    match api::ci::finish(trpc, &job.id, token, outcome.conclusion.as_str(), &steps) {
        Ok(_) => log::info!("ci job {}: {}", job.id, outcome.conclusion.as_str()),
        Err(err) => log::warn!("ci job {}: finish failed ({err}); the server's clock will conclude it", job.id),
    }
    (true, format!("Job {}.", outcome.conclusion.as_str()))
}

struct ServerSink {
    trpc: Arc<TrpcClient>,
    job_id: String,
    token: String,
    buffer: String,
    seq: u64,
    log_open: bool,
    last_flush: Instant,
    last_beat: Instant,
    steps: Vec<StepState>,
    cancel: bool,
    abort: Arc<AtomicBool>,
}

impl ServerSink {
    fn steps_json(&self) -> Value {
        serde_json::to_value(&self.steps).unwrap_or(Value::Null)
    }

    fn flush(&mut self) {
        self.last_flush = Instant::now();
        while !self.buffer.is_empty() {
            let mut cut = self.buffer.len().min(CHUNK_MAX);
            while !self.buffer.is_char_boundary(cut) {
                cut -= 1;
            }
            let chunk: String = self.buffer.drain(..cut).collect();
            if !self.log_open {
                continue;
            }
            match api::ci::append_log(&self.trpc, &self.job_id, &self.token, self.seq, &chunk) {
                Ok(true) => self.seq += 1,
                Ok(false) => {
                    log::warn!("ci job {}: the server stopped taking its log", self.job_id);
                    self.log_open = false;
                }
                // A transient failure loses this chunk's slot only if the
                // next append also fails; retry it once now.
                Err(err) => {
                    log::debug!("ci job {}: log append failed ({err}), retrying", self.job_id);
                    match api::ci::append_log(&self.trpc, &self.job_id, &self.token, self.seq, &chunk) {
                        Ok(true) => self.seq += 1,
                        _ => self.log_open = false,
                    }
                }
            }
        }
    }

    fn beat(&mut self) {
        self.last_beat = Instant::now();
        match api::ci::beat(&self.trpc, &self.job_id, &self.token, &self.steps_json()) {
            Ok(true) => self.cancel = true,
            Ok(false) => {}
            Err(err) => log::debug!("ci job {}: beat failed: {err}", self.job_id),
        }
    }
}

impl LogSink for ServerSink {
    fn log(&mut self, text: &str) {
        self.buffer.push_str(text);
        if self.buffer.len() >= FLUSH_BYTES {
            self.flush();
        }
    }

    fn steps(&mut self, steps: &[StepState]) {
        self.steps = steps.to_vec();
        // A step boundary is worth telling right away (the run page's trace).
        self.flush();
        self.beat();
    }

    fn cancelled(&mut self) -> bool {
        if self.last_flush.elapsed() >= FLUSH_EVERY {
            self.flush();
        }
        if self.last_beat.elapsed() >= BEAT_EVERY {
            self.beat();
        }
        self.cancel || self.abort.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The decline tests share the process-wide drain flag.
    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn the_command_payload_carries_the_runner_settings() {
        let full = JobCommand::parse(&json!({"jobId": "j1", "memoryMaxMb": 8192, "pauseWhenBusy": true})).unwrap();
        assert_eq!(full, JobCommand { job_id: "j1".into(), memory_max_mb: Some(8192), pause_when_busy: true });
        // An older server sends the job alone: no cap, no pause.
        let bare = JobCommand::parse(&json!({"jobId": "j1"})).unwrap();
        assert_eq!(bare, JobCommand { job_id: "j1".into(), memory_max_mb: None, pause_when_busy: false });
        assert!(JobCommand::parse(&json!({"jobId": ""})).is_none());
        assert!(JobCommand::parse(&json!({})).is_none());
    }

    #[test]
    fn a_draining_runner_declines_before_it_claims() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let runner = Runner::new(std::env::temp_dir().join("exp-ci-decline"), Containment::default(), false);
        let command = JobCommand { job_id: "j".into(), ..Default::default() };
        set_draining(true);
        let declined = decline(&runner, &command);
        set_draining(false);
        assert!(declined.is_some_and(|(ok, message)| ok && message.contains("update")));
        assert!(decline(&runner, &command).is_none());
    }

    #[test]
    fn a_runner_that_requires_hiding_without_a_helper_runs_nothing() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let runner = Runner::new(std::env::temp_dir().join("exp-ci-nohelper"), Containment::default(), true);
        let command = JobCommand { job_id: "j".into(), ..Default::default() };
        assert!(decline(&runner, &command).is_some_and(|(ok, _)| !ok));
    }

    #[test]
    fn the_desktop_runner_hides_every_exponential_dir_and_works_outside_them() {
        let base = std::env::temp_dir().join(format!("exp-ci-dirs-{}", std::process::id()));
        for name in ["exponential", "exponential-ci", "at.exponential.staging", "other"] {
            std::fs::create_dir_all(base.join(name)).unwrap();
        }
        let mut found: Vec<String> = exponential_dirs(&base).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        found.sort();
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(found, ["at.exponential.staging", "exponential", "exponential-ci"]);
        let runner = Runner::desktop(PathBuf::from("/bin/true"));
        assert!(runner.require_hidden);
        assert!(runner.containment.hide_dirs.contains(&api::default_data_dir()));
        assert!(runner.containment.check_work_root(&runner.work_root).is_ok(), "{:?}", runner.work_root);
    }

    #[test]
    fn hold_left_is_the_last_running_jobs_timeout() {
        let now = Instant::now();
        jobs().insert("hold-a".into(), Live { deadline: now + Duration::from_secs(60), abort: Arc::default() });
        jobs().insert("hold-b".into(), Live { deadline: now + Duration::from_secs(600), abort: Arc::default() });
        let left = hold_left(now);
        jobs().remove("hold-a");
        jobs().remove("hold-b");
        assert!(left.is_some_and(|left| left >= Duration::from_secs(600)));
    }
}
