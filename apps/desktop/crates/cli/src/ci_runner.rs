//! #EXP-1617: this daemon as a CI runner. A `ci_job` device command names
//! ONE job; the daemon claims it (the server's compare-and-swap; losing is
//! fine), completes the command, takes the job's secrets once, and runs it
//! through `ecode::run_job` in a clean worktree under `{data_dir}/ci`. Log
//! lines (already redacted) go up as chunks about every second; the job
//! beats every 15 s and stops when the server says so (cancelled, timed
//! out, lost). Slots and labels are the server's business: it only
//! dispatches what this runner's settings allow.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ecode::{GitRemote, JobSpec, LogSink, MapSecrets, ProcessSandbox, StepState};

use crate::context::Ctx;

/// The device cap that makes the server dispatch `ci_job` here.
pub const CI_RUNNER_CAP: &str = "ci-runner";
/// The device command kind.
pub const CI_JOB_COMMAND: &str = "ci_job";

const FLUSH_EVERY: Duration = Duration::from_secs(1);
const FLUSH_BYTES: usize = 64 * 1024;
/// Under the server's chunk cap (256 KiB).
const CHUNK_MAX: usize = 192 * 1024;
const BEAT_EVERY: Duration = Duration::from_secs(15);

/// The bearer the daemon's tRPC client sends (for ecode's own HTTP).
fn bearer(ctx: &Ctx) -> Option<String> {
    ctx.trpc
        .auth_headers()
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        .and_then(|(_, value)| value.strip_prefix("Bearer ").map(str::to_string))
}

struct ServerSink {
    ctx: Arc<Ctx>,
    job_id: String,
    token: String,
    buffer: String,
    seq: u64,
    log_open: bool,
    last_flush: Instant,
    last_beat: Instant,
    steps: Vec<StepState>,
    cancel: bool,
}

impl ServerSink {
    fn steps_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.steps).unwrap_or(serde_json::Value::Null)
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
            match api::ci::append_log(&self.ctx.trpc, &self.job_id, &self.token, self.seq, &chunk) {
                Ok(true) => self.seq += 1,
                Ok(false) => {
                    log::warn!("ci job {}: the server stopped taking its log", self.job_id);
                    self.log_open = false;
                }
                // A transient failure loses this chunk's slot only if the
                // next append also fails; retry it once now.
                Err(err) => {
                    log::debug!("ci job {}: log append failed ({err}), retrying", self.job_id);
                    match api::ci::append_log(&self.ctx.trpc, &self.job_id, &self.token, self.seq, &chunk) {
                        Ok(true) => self.seq += 1,
                        _ => self.log_open = false,
                    }
                }
            }
        }
    }

    fn beat(&mut self) {
        self.last_beat = Instant::now();
        match api::ci::beat(&self.ctx.trpc, &self.job_id, &self.token, &self.steps_json()) {
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
        self.cancel
    }
}

/// Handle one `ci_job` command on its own thread (redeliveries of a command
/// already in flight are dropped through `inflight`).
pub fn spawn(ctx: &Arc<Ctx>, command_id: &str, job_id: &str, inflight: &Arc<Mutex<HashSet<String>>>) {
    {
        let Ok(mut guard) = inflight.lock() else { return };
        if !guard.insert(command_id.to_string()) {
            return;
        }
    }
    let ctx = Arc::clone(ctx);
    let command_id = command_id.to_string();
    let job_id = job_id.to_string();
    let claims = Arc::clone(inflight);
    let spawned = std::thread::Builder::new().name("exp-ci-job".to_string()).spawn(move || {
        let (ok, message) = run(&ctx, &command_id, &job_id);
        // A won claim completed the command already (this is then a no-op).
        if let Err(err) = api::devices::complete_command(&ctx.trpc, &command_id, ok, Some(&message)) {
            log::debug!("completeCommand {command_id} failed: {err}");
        }
        if let Ok(mut guard) = claims.lock() {
            guard.remove(&command_id);
        }
    });
    if let Err(err) = spawned {
        log::warn!("ci job: could not start a thread: {err}");
    }
}

/// Claim and run; the command's outcome. The command completes right after
/// the claim (the job's state lives on the server from then on).
fn run(ctx: &Arc<Ctx>, command_id: &str, job_id: &str) -> (bool, String) {
    let device_id = ctx.device_id();
    let claimed = match api::ci::claim(&ctx.trpc, &device_id, job_id) {
        Ok(Some(claimed)) => claimed,
        Ok(None) => return (true, "Another runner has this job.".to_string()),
        Err(err) => return (false, format!("Could not claim the job: {err}")),
    };
    let _ = api::devices::complete_command(&ctx.trpc, command_id, true, Some("Claimed."));
    let token = claimed.token.clone();
    let job: JobSpec = match serde_json::from_value(claimed.job.clone()) {
        Ok(job) => job,
        Err(err) => {
            let _ = api::ci::finish(&ctx.trpc, job_id, &token, "startup_failure", &serde_json::Value::Null);
            return (false, format!("Malformed job: {err}"));
        }
    };
    log::info!("ci job {} ({}): running {} step(s) at {}", job.id, job.check, job.steps.len(), job.sha);

    let mut sink = ServerSink {
        ctx: Arc::clone(ctx),
        job_id: job.id.clone(),
        token: token.clone(),
        buffer: String::new(),
        seq: 0,
        log_open: true,
        last_flush: Instant::now(),
        last_beat: Instant::now(),
        steps: vec![],
        cancel: false,
    };
    let secrets = if job.secrets.is_empty() {
        MapSecrets::default()
    } else {
        match api::ci::job_secrets(&ctx.trpc, &job.id, &token) {
            Ok(values) => MapSecrets::new(values),
            Err(err) => {
                sink.log(&format!("##[error]could not fetch the job's secrets: {err}\n"));
                sink.flush();
                let _ = api::ci::finish(&ctx.trpc, &job.id, &token, "startup_failure", &serde_json::Value::Null);
                return (true, "Secrets unavailable.".to_string());
            }
        }
    };
    let Some(bearer) = bearer(ctx) else {
        let _ = api::ci::finish(&ctx.trpc, &job.id, &token, "startup_failure", &serde_json::Value::Null);
        return (true, "Signed out.".to_string());
    };
    let remote = match ecode::exponential::ExponentialCredentials::new(ctx.trpc.base_url(), bearer, &claimed.repository_id)
        .and_then(GitRemote::new)
    {
        Ok(remote) => remote,
        Err(err) => {
            sink.log(&format!("##[error]no credentials for the repository: {err}\n"));
            sink.flush();
            let _ = api::ci::finish(&ctx.trpc, &job.id, &token, "startup_failure", &serde_json::Value::Null);
            return (true, "No repository credentials.".to_string());
        }
    };
    // The job yields the CPU; the runner keeps beating (see with_idle_steps).
    let sandbox = ProcessSandbox::new(ctx.data_dir.join("ci")).with_idle_steps(true);
    let outcome = ecode::run_job(&sandbox, &remote, &secrets, &job, &mut sink);
    sink.flush();
    let steps = serde_json::to_value(&outcome.steps).unwrap_or(serde_json::Value::Null);
    match api::ci::finish(&ctx.trpc, &job.id, &token, outcome.conclusion.as_str(), &steps) {
        Ok(_) => log::info!("ci job {}: {}", job.id, outcome.conclusion.as_str()),
        Err(err) => log::warn!("ci job {}: finish failed ({err}); the server's clock will conclude it", job.id),
    }
    (true, format!("Job {}.", outcome.conclusion.as_str()))
}
