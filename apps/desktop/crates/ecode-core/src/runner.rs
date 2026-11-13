//! eCode's runner scheduling (`fixtures/runner.json` is the spec): jobs wait
//! for their `needs`, a runner claims the first one its labels cover, and the
//! clock times jobs out, fails the jobs of a lost runner and flags a job
//! queued too long (once).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    js, CheckConclusion, CheckStatus, ChecksState, Failure, ForgeReason, OpError, Outcome, Skip, QUEUED_FLAG_MINUTES,
    RUNNER_LOST_MS,
};

/// One queued, running or completed job.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub check: String,
    pub runs_on: Vec<String>,
    /// Job ids.
    pub needs: Vec<String>,
    pub timeout_min: f64,
    pub status: CheckStatus,
    pub conclusion: Option<CheckConclusion>,
    pub runner: Option<String>,
    pub queued_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub flagged: bool,
    /// Why it completed without running (a contract skip, `needs`, `runner_lost`).
    pub reason: Option<String>,
}

/// A runner as its beats report it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Runner {
    pub id: String,
    pub labels: Vec<String>,
    pub slots: f64,
    pub last_seen: i64,
    /// Job ids.
    pub running: Vec<String>,
}

/// The scheduler's state (empty = `RunnerState::default()`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RunnerState {
    pub jobs: Vec<Job>,
    pub runners: Vec<Runner>,
}

/// A runner op's outcome.
pub type RunnerOutcome = Outcome<RunnerState>;

enum Stop {
    Refused(ForgeReason),
    Op(OpError),
}

fn complete(
    state: &mut RunnerState,
    index: usize,
    conclusion: CheckConclusion,
    at: i64,
    events: &mut Vec<Value>,
    reason: Option<&str>,
) {
    let job_id = state.jobs[index].id.clone();
    if let Some(runner_id) = &state.jobs[index].runner {
        if let Some(runner) = state.runners.iter_mut().find(|r| &r.id == runner_id) {
            runner.running.retain(|id| *id != job_id);
        }
    }
    let job = &mut state.jobs[index];
    job.status = CheckStatus::Completed;
    job.conclusion = Some(conclusion);
    job.completed_at = Some(at);
    job.reason = reason.map(str::to_string);
    events.push(match reason.filter(|r| !r.is_empty()) {
        Some(reason) => json!({ "kind": "job_completed", "job": job_id, "conclusion": conclusion, "reason": reason }),
        None => json!({ "kind": "job_completed", "job": job_id, "conclusion": conclusion }),
    });
}

/// A completed need lets its dependents start: a satisfying conclusion that
/// is not itself the skip of a blocked chain.
fn satisfied(dep: &Job) -> bool {
    dep.status == CheckStatus::Completed
        && dep.conclusion.is_some_and(CheckConclusion::satisfies)
        && dep.reason.as_deref() != Some("needs")
}

fn find<'a>(state: &'a RunnerState, id: &str) -> Option<&'a Job> {
    state.jobs.iter().find(|j| j.id == id)
}

/// Queued jobs whose needs can no longer be met are skipped (to a fixpoint).
fn settle(state: &mut RunnerState, at: i64, events: &mut Vec<Value>) {
    let mut changed = true;
    while changed {
        changed = false;
        for index in 0..state.jobs.len() {
            let job = &state.jobs[index];
            if job.status != CheckStatus::Queued {
                continue;
            }
            let blocked = job.needs.iter().any(|need| {
                find(state, need).is_some_and(|dep| dep.status == CheckStatus::Completed && !satisfied(dep))
            });
            if blocked {
                complete(state, index, CheckConclusion::Skipped, at, events, Some("needs"));
                changed = true;
            }
        }
    }
}

fn ready(state: &RunnerState, job: &Job) -> bool {
    job.needs.iter().all(|need| find(state, need).is_some_and(satisfied))
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

fn apply(state: &mut RunnerState, op: &str, args: &Value, at: i64, events: &mut Vec<Value>) -> Result<Value, Stop> {
    match op {
        "enqueue" => {
            for spec in args.get("jobs").and_then(Value::as_array).map_or(&[][..], Vec::as_slice) {
                let id = js::string(spec.get("id"));
                if state.jobs.iter().any(|job| job.id == id) {
                    return Err(Stop::Refused(ForgeReason::Exists));
                }
                let skip = match spec.get("skip").and_then(Value::as_str) {
                    Some(skip) => Some(Skip::parse(skip).ok_or(Stop::Op(OpError::InvalidArg("skip")))?),
                    None => None,
                };
                let check = js::nullish(spec.get("check")).map_or_else(|| id.clone(), |c| js::string(Some(c)));
                let timeout_min = js::nullish(spec.get("timeoutMin")).map_or(60.0, |t| js::number(Some(t)));
                state.jobs.push(Job {
                    id: id.clone(),
                    check,
                    runs_on: strings(spec.get("runsOn")),
                    needs: strings(spec.get("needs")),
                    timeout_min,
                    status: CheckStatus::Queued,
                    conclusion: None,
                    runner: None,
                    queued_at: at,
                    started_at: None,
                    completed_at: None,
                    flagged: false,
                    reason: None,
                });
                events.push(json!({ "kind": "job_queued", "job": id }));
                if let Some(skip) = skip {
                    let index = state.jobs.len() - 1;
                    complete(state, index, skip.conclusion(), at, events, Some(skip.as_str()));
                }
            }
            Ok(json!({}))
        }
        "beat" => {
            let id = js::string(args.get("runner"));
            let labels = strings(args.get("labels"));
            let slots = js::nullish(args.get("slots")).map_or(1.0, |s| js::number(Some(s)));
            match state.runners.iter_mut().find(|r| r.id == id) {
                Some(runner) => {
                    runner.labels = labels;
                    runner.slots = slots;
                    runner.last_seen = at;
                }
                None => state.runners.push(Runner { id, labels, slots, last_seen: at, running: vec![] }),
            }
            Ok(json!({}))
        }
        "claim" => {
            let Some(r) = state.runners.iter().position(|r| js::is_str(args.get("runner"), &r.id)) else {
                return Err(Stop::Refused(ForgeReason::Runner));
            };
            state.runners[r].last_seen = at;
            let runner = &state.runners[r];
            if runner.running.len() as f64 >= runner.slots {
                return Ok(json!({ "job": null }));
            }
            let Some(index) = state.jobs.iter().position(|j| {
                j.status == CheckStatus::Queued
                    && ready(state, j)
                    && j.runs_on.iter().all(|label| runner.labels.contains(label))
            }) else {
                return Ok(json!({ "job": null }));
            };
            let runner_id = runner.id.clone();
            let job = &mut state.jobs[index];
            job.status = CheckStatus::InProgress;
            job.runner = Some(runner_id.clone());
            job.started_at = Some(at);
            let job_id = job.id.clone();
            state.runners[r].running.push(job_id.clone());
            events.push(json!({ "kind": "job_started", "job": job_id, "runner": runner_id }));
            Ok(json!({ "job": job_id }))
        }
        "finish" => {
            let Some(index) = state.jobs.iter().position(|j| js::is_str(args.get("job"), &j.id)) else {
                return Err(Stop::Refused(ForgeReason::Job));
            };
            let job = &state.jobs[index];
            let runs_it = job.runner.as_deref().is_some_and(|r| js::is_str(args.get("runner"), r));
            if job.status != CheckStatus::InProgress || !runs_it {
                return Err(Stop::Refused(ForgeReason::NotRunning));
            }
            let conclusion = args
                .get("conclusion")
                .and_then(Value::as_str)
                .and_then(CheckConclusion::parse)
                .ok_or(Stop::Op(OpError::InvalidArg("conclusion")))?;
            complete(state, index, conclusion, at, events, None);
            Ok(json!({}))
        }
        "cancel" => {
            let ids = args.get("jobs").and_then(Value::as_array).map(|_| strings(args.get("jobs")));
            for index in 0..state.jobs.len() {
                let job = &state.jobs[index];
                if ids.as_ref().is_some_and(|ids| !ids.contains(&job.id)) {
                    continue;
                }
                if job.status == CheckStatus::InProgress {
                    events.push(json!({ "kind": "kill", "job": job.id, "runner": job.runner }));
                }
                if job.status != CheckStatus::Completed {
                    complete(state, index, CheckConclusion::Cancelled, at, events, None);
                }
            }
            Ok(json!({}))
        }
        "tick" => {
            for index in 0..state.jobs.len() {
                let job = &state.jobs[index];
                let timed_out = job.status == CheckStatus::InProgress
                    && job.started_at.is_some_and(|s| (at - s) as f64 >= job.timeout_min * 60_000.0);
                if timed_out {
                    events.push(json!({ "kind": "kill", "job": job.id, "runner": job.runner }));
                    complete(state, index, CheckConclusion::TimedOut, at, events, None);
                }
            }
            for r in 0..state.runners.len() {
                if at - state.runners[r].last_seen < RUNNER_LOST_MS {
                    continue;
                }
                for id in state.runners[r].running.clone() {
                    if let Some(index) = state.jobs.iter().position(|j| j.id == id) {
                        complete(state, index, CheckConclusion::Failure, at, events, Some("runner_lost"));
                    }
                }
            }
            for job in state.jobs.iter_mut() {
                if job.status == CheckStatus::Queued && !job.flagged && at - job.queued_at >= QUEUED_FLAG_MINUTES * 60_000 {
                    job.flagged = true;
                    events.push(json!({ "kind": "queued_too_long", "job": job.id }));
                }
            }
            Ok(json!({}))
        }
        _ => Err(Stop::Op(OpError::UnknownOp(op.to_string()))),
    }
}

/// One op at time `at` (unix ms). A refusal leaves the state unchanged.
/// `Err` = an unknown op, an unknown `skip` or `conclusion`.
pub fn schedule(state: &RunnerState, op: &str, args: &Value, at: i64) -> Result<RunnerOutcome, OpError> {
    let mut next = state.clone();
    let mut events = vec![];
    match apply(&mut next, op, args, at, &mut events) {
        Ok(result) => {
            settle(&mut next, at, &mut events);
            Ok(Outcome { state: next, events, result: Some(result), failure: None })
        }
        Err(Stop::Refused(reason)) => {
            Ok(Outcome { state: state.clone(), events: vec![], result: None, failure: Some(Failure::new(reason)) })
        }
        Err(Stop::Op(error)) => Err(error),
    }
}

/// The run's rollup over its jobs: failure wins, then pending, then success.
pub fn run_conclusion(state: &RunnerState) -> ChecksState {
    let failing = |job: &Job| job.status == CheckStatus::Completed && job.conclusion.is_some_and(CheckConclusion::is_failing);
    if state.jobs.iter().any(failing) {
        ChecksState::Failure
    } else if state.jobs.iter().any(|job| job.status != CheckStatus::Completed) {
        ChecksState::Pending
    } else {
        ChecksState::Success
    }
}
