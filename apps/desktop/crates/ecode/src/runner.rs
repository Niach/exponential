//! The runner (feature `runner`, #EXP-1617): runs ONE planned job on this
//! machine. A [`Sandbox`] gives the job a clean checkout at its commit and
//! starts each step's shell ([`ProcessSandbox`]: a git worktree off a bare
//! mirror, the host's processes; containers later). The runner:
//!
//! - builds each step's environment from an allowlist (the host vars the
//!   sandbox passes through, the job's env, `CI=true`), never the whole host
//!   env;
//! - injects ONLY the job's declared secrets through a [`SecretSource`] and
//!   redacts their values from every log line (`ecode_core::redact`);
//! - enforces the job's and each step's timeout and kills the WHOLE process
//!   tree (a process group on unix) on timeout or cancel;
//! - streams log lines and step transitions to a [`LogSink`].
//!
//! Scheduling (which runner claims which job) is `ecode_core::schedule`;
//! the host (the Exponential daemon, Rerusted) does the claiming.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ecode_core::{CheckConclusion, CheckStatus, PlannedJob};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Error, GitCommand, Remote, Result, SecretSource};

/// One step of a job as the runner gets it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStep {
    pub name: String,
    pub run: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub timeout_min: Option<i64>,
    #[serde(default)]
    pub continue_on_error: bool,
    #[serde(default)]
    pub working_directory: Option<String>,
}

/// One job as the runner gets it: a planned job ([`PlannedJob`]) at a commit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSpec {
    pub id: String,
    /// The check name it reports as.
    pub check: String,
    /// The commit to check out.
    pub sha: String,
    /// A ref that contains `sha`, fetched when the remote refuses a bare sha.
    #[serde(default)]
    pub git_ref: Option<String>,
    pub timeout_min: i64,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// The secret NAMES the job may read; values come from the SecretSource.
    #[serde(default)]
    pub secrets: Vec<String>,
    pub steps: Vec<JobStep>,
}

impl JobSpec {
    /// A planned job at `sha`.
    pub fn from_planned(id: impl Into<String>, sha: impl Into<String>, job: &PlannedJob) -> Self {
        Self {
            id: id.into(),
            check: job.check.clone(),
            sha: sha.into(),
            git_ref: None,
            timeout_min: job.timeout_min,
            env: job.env.clone(),
            secrets: job.secrets.clone(),
            steps: job
                .steps
                .iter()
                .map(|s| JobStep {
                    name: s.name.clone(),
                    run: s.run.clone(),
                    env: s.env.clone(),
                    timeout_min: s.timeout_min,
                    continue_on_error: s.continue_on_error,
                    working_directory: s.working_directory.clone(),
                })
                .collect(),
        }
    }
}

/// A step's state, as `ci_jobs.steps` stores it (the run page's trace).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepState {
    pub name: String,
    pub status: CheckStatus,
    pub conclusion: Option<CheckConclusion>,
    /// Unix milliseconds.
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

/// How a job ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobOutcome {
    pub conclusion: CheckConclusion,
    pub steps: Vec<StepState>,
}

/// Where a running job's output goes. Every text is already redacted.
pub trait LogSink: Send {
    /// A chunk of log text (whole lines, each ending in `\n`).
    fn log(&mut self, text: &str);
    /// The steps changed (one started or completed).
    fn steps(&mut self, steps: &[StepState]);
    /// Polled about every 250 ms: `true` cancels the job (the tree is killed).
    fn cancelled(&mut self) -> bool {
        false
    }
}

/// A sink collecting everything (tests, a host that uploads at the end).
#[derive(Clone, Debug, Default)]
pub struct MemorySink {
    pub text: String,
    pub steps: Vec<StepState>,
    pub cancel: bool,
}

impl LogSink for MemorySink {
    fn log(&mut self, text: &str) {
        self.text.push_str(text);
    }
    fn steps(&mut self, steps: &[StepState]) {
        self.steps = steps.to_vec();
    }
    fn cancelled(&mut self) -> bool {
        self.cancel
    }
}

/// Where a job runs: a clean workspace at its commit and the command each
/// step's script starts as. The runner owns spawning, env, timeouts and the
/// tree kill; a container backend wraps [`Sandbox::shell`] (later).
pub trait Sandbox: Send + Sync {
    /// A clean checkout of `job.sha` for this job alone.
    fn prepare(&self, job: &JobSpec, remote: &dyn Remote) -> Result<PathBuf>;
    /// The command that runs `script` in `cwd` (no env yet: the runner sets it).
    fn shell(&self, cwd: &Path, script: &str) -> Command;
    /// Host env vars a step may inherit (the allowlist).
    fn passthrough(&self) -> &[String];
    /// Removes the workspace (best effort).
    fn release(&self, job: &JobSpec, workspace: &Path);
}

/// The host env every process sandbox passes through by default.
pub const DEFAULT_PASSTHROUGH: &[&str] =
    &["PATH", "HOME", "USER", "LOGNAME", "SHELL", "LANG", "LC_ALL", "TMPDIR", "TZ", "CARGO_HOME", "RUSTUP_HOME"];

/// The process backend: a bare mirror per remote under `root/mirrors`, a
/// detached worktree per job under `root/jobs/<id>`, steps in bash (`sh`
/// when bash is missing) with `-eo pipefail`.
pub struct ProcessSandbox {
    root: PathBuf,
    passthrough: Vec<String>,
    /// One fetch per mirror at a time.
    mirrors: Mutex<BTreeMap<PathBuf, Arc<Mutex<()>>>>,
    bash: bool,
}

impl ProcessSandbox {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let bash = Command::new("bash")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        Self {
            root: root.into(),
            passthrough: DEFAULT_PASSTHROUGH.iter().map(|s| s.to_string()).collect(),
            mirrors: Mutex::new(BTreeMap::new()),
            bash,
        }
    }

    /// Adds host env vars to the allowlist (the runner settings' extra names).
    pub fn with_passthrough<I: IntoIterator<Item = String>>(mut self, names: I) -> Self {
        for name in names {
            if !self.passthrough.contains(&name) {
                self.passthrough.push(name);
            }
        }
        self
    }

    fn mirror(&self, url: &str) -> PathBuf {
        let digest = Sha256::digest(url.as_bytes());
        let key: String = digest.iter().take(10).map(|b| format!("{b:02x}")).collect();
        self.root.join("mirrors").join(format!("{key}.git"))
    }

    fn workspace(&self, job: &JobSpec) -> PathBuf {
        self.root.join("jobs").join(safe_segment(&job.id))
    }

    fn mirror_lock(&self, mirror: &Path) -> Arc<Mutex<()>> {
        let mut locks = self.mirrors.lock().unwrap_or_else(|e| e.into_inner());
        locks.entry(mirror.to_path_buf()).or_default().clone()
    }
}

fn safe_segment(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

fn job_ref(job: &JobSpec) -> String {
    format!("refs/ecode/jobs/{}", safe_segment(&job.id))
}

impl Sandbox for ProcessSandbox {
    fn prepare(&self, job: &JobSpec, remote: &dyn Remote) -> Result<PathBuf> {
        if job.sha.len() < 7 || !job.sha.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::Invalid(format!("not a commit sha: {}", job.sha)));
        }
        let mirror = self.mirror(&remote.url());
        let lock = self.mirror_lock(&mirror);
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        if !mirror.join("HEAD").exists() {
            std::fs::create_dir_all(&mirror).map_err(|e| Error::Invalid(format!("mirror dir: {e}")))?;
            GitCommand::new(Some(&mirror)).args(["init", "--bare", "--quiet"]).run()?;
        }
        let target = job_ref(job);
        let by_sha = remote.fetch(&mirror, &[&format!("+{}:{target}", job.sha)]);
        if let Err(error) = by_sha {
            let Some(git_ref) = &job.git_ref else { return Err(error) };
            remote.fetch(&mirror, &[&format!("+{git_ref}:{target}")])?;
        }
        let workspace = self.workspace(job);
        if workspace.exists() {
            let _ = GitCommand::new(Some(&mirror)).arg("worktree").arg("remove").arg("--force").arg(&workspace).run();
            let _ = std::fs::remove_dir_all(&workspace);
        }
        let _ = GitCommand::new(Some(&mirror)).args(["worktree", "prune"]).run();
        std::fs::create_dir_all(workspace.parent().unwrap_or(&self.root))
            .map_err(|e| Error::Invalid(format!("jobs dir: {e}")))?;
        GitCommand::new(Some(&mirror))
            .args(["worktree", "add", "--detach", "--force", "--quiet"])
            .arg(&workspace)
            .arg(&job.sha)
            .run()?;
        Ok(workspace)
    }

    fn shell(&self, cwd: &Path, script: &str) -> Command {
        let mut command = if self.bash {
            let mut c = Command::new("bash");
            c.args(["--noprofile", "--norc", "-eo", "pipefail", "-c", script]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-e", "-c", script]);
            c
        };
        command.current_dir(cwd);
        command
    }

    fn passthrough(&self) -> &[String] {
        &self.passthrough
    }

    fn release(&self, job: &JobSpec, workspace: &Path) {
        let lock_dirs = self.mirrors.lock().map(|m| m.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
        for mirror in lock_dirs {
            let _ = GitCommand::new(Some(&mirror)).arg("worktree").arg("remove").arg("--force").arg(workspace).run();
            let _ = GitCommand::new(Some(&mirror)).args(["update-ref", "-d", &job_ref(job)]).run();
        }
        let _ = std::fs::remove_dir_all(workspace);
    }
}

/// The wall clock in unix milliseconds.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// A `workingDirectory` inside the workspace, or `None` when it escapes.
fn step_dir(workspace: &Path, dir: Option<&str>) -> Option<PathBuf> {
    let Some(dir) = dir.filter(|d| !d.is_empty()) else { return Some(workspace.to_path_buf()) };
    let rel = Path::new(dir);
    let escapes = rel.components().any(|c| !matches!(c, Component::Normal(_) | Component::CurDir));
    (!escapes).then(|| workspace.join(rel))
}

enum Ended {
    Exit(ExitStatus),
    TimedOut,
    Cancelled,
}

fn spawn_group(command: &mut Command) -> std::io::Result<Child> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn()
}

/// Kills the child's whole process tree (its process group on unix):
/// SIGTERM, a grace period, then SIGKILL.
fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        let pgid = child.id() as libc::pid_t;
        // SAFETY: kill(2) with a negative pid signals the process group the
        // child leads (spawned with process_group(0)); no memory is touched.
        unsafe { libc::kill(-pgid, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if matches!(child.try_wait(), Ok(Some(_))) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        // SAFETY: as above; the group may still hold grandchildren.
        unsafe { libc::kill(-pgid, libc::SIGKILL) };
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn pump<R: Read + Send + 'static>(reader: R, tx: mpsc::Sender<String>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            // Long lines arrive in 64 KiB slices.
            match reader.by_ref().take(64 * 1024).read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let mut line = String::from_utf8_lossy(&buf).into_owned();
                    if !line.ends_with('\n') {
                        line.push('\n');
                    }
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            }
        }
    })
}

/// Runs a job start to end: prepare the workspace, run each step, release.
/// A setup failure (fetch, checkout) concludes `failure` with the reason in
/// the log; this returns `Err` only when the sink cannot be told anything.
pub fn run_job(
    sandbox: &dyn Sandbox,
    remote: &dyn Remote,
    secrets: &dyn SecretSource,
    job: &JobSpec,
    sink: &mut dyn LogSink,
) -> JobOutcome {
    let started = Instant::now();
    let job_deadline = started + Duration::from_secs((job.timeout_min.max(1) as u64) * 60);
    let mut steps: Vec<StepState> = job
        .steps
        .iter()
        .map(|s| StepState {
            name: s.name.clone(),
            status: CheckStatus::Queued,
            conclusion: None,
            started_at: None,
            completed_at: None,
        })
        .collect();
    sink.steps(&steps);
    let say = |sink: &mut dyn LogSink, text: &str| sink.log(&secrets.redact(&job.secrets, text));

    say(sink, &format!("##[group]Checkout {}\n", short(&job.sha)));
    let workspace = match sandbox.prepare(job, remote) {
        Ok(workspace) => workspace,
        Err(error) => {
            say(sink, &format!("##[error]checkout failed: {error}\n"));
            return finish(sink, steps, CheckConclusion::Failure);
        }
    };
    say(sink, "##[endgroup]\n");

    let mut conclusion = CheckConclusion::Success;
    for index in 0..job.steps.len() {
        let step = &job.steps[index];
        if conclusion != CheckConclusion::Success {
            steps[index].status = CheckStatus::Completed;
            steps[index].conclusion = Some(CheckConclusion::Skipped);
            continue;
        }
        steps[index].status = CheckStatus::InProgress;
        steps[index].started_at = Some(now_ms());
        sink.steps(&steps);
        say(sink, &format!("##[group]{}\n", step.name));

        let result = run_step(sandbox, secrets, job, step, &workspace, job_deadline, sink);
        let step_conclusion = match result {
            Ok(Ended::Exit(status)) if status.success() => CheckConclusion::Success,
            Ok(Ended::Exit(status)) => {
                say(sink, &format!("##[error]exited with {}\n", describe_exit(status)));
                CheckConclusion::Failure
            }
            Ok(Ended::TimedOut) => {
                say(sink, "##[error]timed out; the process tree was killed\n");
                CheckConclusion::TimedOut
            }
            Ok(Ended::Cancelled) => {
                say(sink, "##[error]cancelled; the process tree was killed\n");
                CheckConclusion::Cancelled
            }
            Err(error) => {
                say(sink, &format!("##[error]{error}\n"));
                CheckConclusion::Failure
            }
        };
        say(sink, "##[endgroup]\n");
        steps[index].status = CheckStatus::Completed;
        steps[index].conclusion = Some(step_conclusion);
        steps[index].completed_at = Some(now_ms());
        let job_fails = match step_conclusion {
            CheckConclusion::Success => false,
            // A job-level timeout or a cancel ends the job however the step
            // is configured.
            CheckConclusion::TimedOut | CheckConclusion::Cancelled => true,
            _ => !step.continue_on_error,
        };
        if job_fails {
            conclusion = step_conclusion;
        }
        sink.steps(&steps);
    }
    sandbox.release(job, &workspace);
    finish(sink, steps, conclusion)
}

fn finish(sink: &mut dyn LogSink, mut steps: Vec<StepState>, conclusion: CheckConclusion) -> JobOutcome {
    for step in steps.iter_mut().filter(|s| s.status != CheckStatus::Completed) {
        step.status = CheckStatus::Completed;
        step.conclusion = Some(CheckConclusion::Skipped);
    }
    sink.steps(&steps);
    JobOutcome { conclusion, steps }
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

fn describe_exit(status: ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("signal {signal}");
        }
    }
    status.code().map_or_else(|| "an unknown status".to_string(), |c| format!("code {c}"))
}

fn run_step(
    sandbox: &dyn Sandbox,
    secrets: &dyn SecretSource,
    job: &JobSpec,
    step: &JobStep,
    workspace: &Path,
    job_deadline: Instant,
    sink: &mut dyn LogSink,
) -> Result<Ended> {
    let cwd = step_dir(workspace, step.working_directory.as_deref())
        .ok_or_else(|| Error::Invalid("workingDirectory leaves the workspace".into()))?;
    let mut command = sandbox.shell(&cwd, &step.run);
    command.env_clear();
    for name in sandbox.passthrough() {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command.env("CI", "true").env("EXP_CI", "true").env("EXP_CI_JOB", &job.id).env("EXP_CI_SHA", &job.sha);
    command.env("GITHUB_WORKSPACE", workspace).env("EXP_CI_WORKSPACE", workspace);
    // The step's env already merges top < job < matrix < step (the planner).
    for (key, value) in job.env.iter().chain(step.env.iter()) {
        command.env(key, value);
    }
    let missing = secrets.inject(&job.secrets, &mut command)?;
    if !missing.is_empty() {
        sink.log(&format!("##[warning]secrets not set: {}\n", missing.join(", ")));
    }
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = spawn_group(&mut command).map_err(|e| Error::Invalid(format!("spawn: {e}")))?;

    let (tx, rx) = mpsc::channel::<String>();
    let readers = [
        child.stdout.take().map(|out| pump(out, tx.clone())),
        child.stderr.take().map(|err| pump(err, tx.clone())),
    ];
    drop(tx);

    let step_deadline = step
        .timeout_min
        .filter(|m| *m > 0)
        .map(|m| Instant::now() + Duration::from_secs(m as u64 * 60))
        .map_or(job_deadline, |d| d.min(job_deadline));
    let mut ended = None;
    loop {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(line) => {
                let mut text = line;
                while let Ok(more) = rx.try_recv() {
                    text.push_str(&more);
                    if text.len() > 16 * 1024 {
                        break;
                    }
                }
                sink.log(&secrets.redact(&job.secrets, &text));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // Both pipes closed: the step's tree is done writing; wait for
                // the shell (bounded by the deadline below).
                if let Some(status) = child.try_wait().ok().flatten() {
                    ended = Some(Ended::Exit(status));
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        if ended.is_none() && Instant::now() >= step_deadline {
            kill_tree(&mut child);
            ended = Some(Ended::TimedOut);
            break;
        }
        if sink.cancelled() {
            kill_tree(&mut child);
            ended = Some(Ended::Cancelled);
            break;
        }
    }
    // A backgrounded grandchild may hold the pipes open past the shell's
    // exit: the group goes with the step.
    if matches!(ended, Some(Ended::Exit(_))) {
        #[cfg(unix)]
        // SAFETY: signals the step's own process group, see kill_tree.
        unsafe {
            libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL)
        };
    }
    for reader in readers.into_iter().flatten() {
        let _ = reader.join();
    }
    // Drain what the readers delivered after the last poll.
    let rest: String = rx.try_iter().collect();
    if !rest.is_empty() {
        sink.log(&secrets.redact(&job.secrets, &rest));
    }
    Ok(ended.unwrap_or(Ended::Cancelled))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GitRemote, MapSecrets};

    fn git(dir: &Path, args: &[&str]) -> String {
        GitCommand::new(Some(dir))
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args.iter().copied())
            .run()
            .unwrap()
            .trim()
            .to_string()
    }

    /// A repo with one commit; returns (dir, sha).
    fn origin() -> (tempdir::Dir, String) {
        let dir = tempdir::Dir::new("origin");
        git(&dir.0, &["init", "--quiet", "-b", "main"]);
        std::fs::write(dir.0.join("hello.txt"), "hi\n").unwrap();
        git(&dir.0, &["add", "."]);
        git(&dir.0, &["commit", "--quiet", "-m", "one"]);
        let sha = git(&dir.0, &["rev-parse", "HEAD"]);
        (dir, sha)
    }

    mod tempdir {
        use std::path::PathBuf;
        pub struct Dir(pub PathBuf);
        impl Dir {
            pub fn new(tag: &str) -> Self {
                let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
                let path = std::env::temp_dir().join(format!("ecode-runner-{tag}-{}-{n}", std::process::id()));
                std::fs::create_dir_all(&path).unwrap();
                Self(path)
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    fn step(name: &str, run: &str) -> JobStep {
        JobStep {
            name: name.into(),
            run: run.into(),
            env: BTreeMap::new(),
            timeout_min: None,
            continue_on_error: false,
            working_directory: None,
        }
    }

    fn job(sha: &str, steps: Vec<JobStep>) -> JobSpec {
        JobSpec {
            id: "job-1".into(),
            check: "test".into(),
            sha: sha.into(),
            git_ref: None,
            timeout_min: 5,
            env: BTreeMap::from([("GREETING".into(), "hello".into())]),
            secrets: vec!["TOKEN".into()],
            steps,
        }
    }

    #[test]
    fn runs_steps_in_a_clean_checkout_with_redacted_secrets() {
        if !crate::git_available() {
            return;
        }
        let (origin, sha) = origin();
        let root = tempdir::Dir::new("root");
        let sandbox = ProcessSandbox::new(&root.0);
        let remote = GitRemote::anonymous(origin.0.to_string_lossy());
        let secrets = MapSecrets::new([("TOKEN".to_string(), "s3cret-value".to_string())]);
        std::env::set_var("ECODE_RUNNER_TEST_LEAK", "leak");
        let spec = job(
            &sha,
            vec![
                step("read", "cat hello.txt; echo \"$GREETING $TOKEN\"; echo \"leak=${ECODE_RUNNER_TEST_LEAK:-none}\""),
                step("fail", "echo boom >&2; exit 3"),
                step("never", "echo never"),
            ],
        );
        let mut sink = MemorySink::default();
        let outcome = run_job(&sandbox, &remote, &secrets, &spec, &mut sink);
        assert_eq!(outcome.conclusion, CheckConclusion::Failure);
        assert!(sink.text.contains("hi\n"), "{}", sink.text);
        assert!(sink.text.contains("hello ***"), "{}", sink.text);
        assert!(!sink.text.contains("s3cret-value"));
        assert!(sink.text.contains("leak=none"), "host env is not inherited: {}", sink.text);
        assert!(sink.text.contains("boom"));
        assert!(!sink.text.contains("never\n"));
        let conclusions: Vec<_> = outcome.steps.iter().map(|s| s.conclusion).collect();
        assert_eq!(
            conclusions,
            [Some(CheckConclusion::Success), Some(CheckConclusion::Failure), Some(CheckConclusion::Skipped)]
        );
        assert!(!root.0.join("jobs").join("job-1").exists(), "the workspace is released");
    }

    #[test]
    fn continue_on_error_keeps_going() {
        if !crate::git_available() {
            return;
        }
        let (origin, sha) = origin();
        let root = tempdir::Dir::new("root");
        let mut flaky = step("flaky", "exit 1");
        flaky.continue_on_error = true;
        let spec = job(&sha, vec![flaky, step("after", "echo after")]);
        let mut sink = MemorySink::default();
        let outcome = run_job(
            &ProcessSandbox::new(&root.0),
            &GitRemote::anonymous(origin.0.to_string_lossy()),
            &MapSecrets::default(),
            &spec,
            &mut sink,
        );
        assert_eq!(outcome.conclusion, CheckConclusion::Success);
        assert!(sink.text.contains("after\n"));
    }

    #[cfg(unix)]
    #[test]
    fn cancel_kills_the_whole_tree() {
        if !crate::git_available() {
            return;
        }
        let (origin, sha) = origin();
        let root = tempdir::Dir::new("root");
        let marker = root.0.join("grandchild-pid");
        let script = format!("(sleep 300 & echo $! > {}; wait) & sleep 300", marker.display());
        let spec = job(&sha, vec![step("hang", &script)]);
        struct CancelSoon(Instant, MemorySink);
        impl LogSink for CancelSoon {
            fn log(&mut self, text: &str) {
                self.1.log(text)
            }
            fn steps(&mut self, steps: &[StepState]) {
                self.1.steps(steps)
            }
            fn cancelled(&mut self) -> bool {
                self.0.elapsed() > Duration::from_millis(800)
            }
        }
        let mut sink = CancelSoon(Instant::now(), MemorySink::default());
        let outcome = run_job(
            &ProcessSandbox::new(&root.0),
            &GitRemote::anonymous(origin.0.to_string_lossy()),
            &MapSecrets::default(),
            &spec,
            &mut sink,
        );
        assert_eq!(outcome.conclusion, CheckConclusion::Cancelled);
        let pid: i32 = std::fs::read_to_string(&marker).unwrap().trim().parse().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        // SAFETY: signal 0 only probes whether the pid exists.
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        assert!(!alive, "the grandchild died with the group");
    }

    #[test]
    fn working_directory_cannot_escape() {
        let ws = Path::new("/w");
        assert_eq!(step_dir(ws, Some("apps/web")), Some(PathBuf::from("/w/apps/web")));
        assert_eq!(step_dir(ws, None), Some(PathBuf::from("/w")));
        assert_eq!(step_dir(ws, Some("../x")), None);
        assert_eq!(step_dir(ws, Some("/etc")), None);
    }
}
