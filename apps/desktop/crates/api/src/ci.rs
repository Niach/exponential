//! `ci.*` (#EXP-1617): the runner's side of eCode's own CI. A daemon that
//! got a `ci_job` device command claims the job (compare-and-swap on the
//! server), then beats, appends log chunks, takes the job's secrets ONCE
//! and finishes, every call carrying the job's one-time token.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// A won claim: the job (the shape of `ecode::JobSpec`), its token and repo.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimedJob {
    pub token: String,
    pub job: Value,
    pub run_id: String,
    pub repository_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JobRef<'a> {
    job_id: &'a str,
    token: &'a str,
}

/// `ci.claim`: `None` = another runner has it, or it is not claimable.
pub fn claim(trpc: &TrpcClient, device_id: &str, job_id: &str) -> Result<Option<ClaimedJob>, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        device_id: &'a str,
        job_id: &'a str,
    }
    #[derive(Deserialize)]
    struct Out {
        claimed: Option<ClaimedJob>,
    }
    let out: Out = trpc.mutation("ci.claim", &Input { device_id, job_id })?;
    Ok(out.claimed)
}

/// `ci.beat`: `true` = stop the job (cancelled, timed out, lost).
pub fn beat(trpc: &TrpcClient, job_id: &str, token: &str, steps: &Value) -> Result<bool, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        job_id: &'a str,
        token: &'a str,
        steps: &'a Value,
    }
    #[derive(Deserialize)]
    struct Out {
        cancel: bool,
    }
    let out: Out = trpc.mutation("ci.beat", &Input { job_id, token, steps })?;
    Ok(out.cancel)
}

/// `ci.appendLog`: chunk `seq` (0-based). `Ok(false)` = the server refused
/// (the job ended, the log or the team's storage is full): stop appending.
pub fn append_log(trpc: &TrpcClient, job_id: &str, token: &str, seq: u64, text: &str) -> Result<bool, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        job_id: &'a str,
        token: &'a str,
        seq: u64,
        text: &'a str,
    }
    #[derive(Deserialize)]
    struct Out {
        ok: bool,
    }
    let out: Out = trpc.mutation("ci.appendLog", &Input { job_id, token, seq, text })?;
    Ok(out.ok)
}

/// `ci.jobSecrets`: the job's secrets, once (a second call is refused).
pub fn job_secrets(trpc: &TrpcClient, job_id: &str, token: &str) -> Result<Vec<(String, String)>, ApiError> {
    #[derive(Deserialize)]
    struct Out {
        values: std::collections::BTreeMap<String, String>,
    }
    let out: Out = trpc.mutation("ci.jobSecrets", &JobRef { job_id, token })?;
    Ok(out.values.into_iter().collect())
}

/// `ci.finish`.
pub fn finish(trpc: &TrpcClient, job_id: &str, token: &str, conclusion: &str, steps: &Value) -> Result<bool, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        job_id: &'a str,
        token: &'a str,
        conclusion: &'a str,
        steps: &'a Value,
    }
    #[derive(Deserialize)]
    struct Out {
        ok: bool,
    }
    let out: Out = trpc.mutation("ci.finish", &Input { job_id, token, conclusion, steps })?;
    Ok(out.ok)
}
