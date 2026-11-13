//! `Ci`: pipelines, runs, jobs and logs. The trait and its types only; E4
//! (#EXP-1617) implements it with our runner (and GitHub Actions reads).
//! The plan is ecode-core's planner output, jobs are its scheduler's.

use ecode_core::{ChecksState, Job, Plan, Trigger, TriggerInput};
use serde::{Deserialize, Serialize};

use crate::Result;

/// One pipeline run at a commit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    /// The pipeline's name.
    pub name: String,
    pub sha: String,
    pub trigger: Trigger,
    /// The rollup over its jobs (`checks.json`).
    pub state: ChecksState,
    /// Unix milliseconds.
    pub created_at: i64,
}

/// A slice of a job's log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogChunk {
    /// Redacted text from the requested offset.
    pub text: String,
    /// Where the next read starts.
    pub next_offset: u64,
    /// The job finished and this is the end of its log.
    pub complete: bool,
}

/// Pipelines, runs, jobs and logs.
pub trait Ci: Send + Sync {
    /// What the pipeline at `sha` runs for `trigger`; `None` = not triggered.
    fn plan(&self, sha: &str, trigger: &TriggerInput) -> Result<Option<Plan>>;
    /// The runs at a commit, newest first.
    fn runs(&self, sha: &str) -> Result<Vec<Run>>;
    /// A run's jobs.
    fn jobs(&self, run: &str) -> Result<Vec<Job>>;
    /// A job's log from `offset`.
    fn log(&self, job: &str, offset: u64) -> Result<LogChunk>;
    /// Starts the run again (only its failed jobs when `failed_only`);
    /// returns the new run's id.
    fn rerun(&self, run: &str, failed_only: bool) -> Result<String>;
    /// Cancels a run's queued and running jobs.
    fn cancel(&self, run: &str) -> Result<()>;
}
