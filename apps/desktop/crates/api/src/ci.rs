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

// ── people (#EXP-1804: the run page and a device's CI runner section) ────

/// One read of a job's log (`ci.log`): `text` from `offset`, the offset to
/// read from next, and whether the log is whole (the job completed and
/// every chunk was read).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRead {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub next_offset: u64,
    #[serde(default)]
    pub complete: bool,
}

/// `ci.log{jobId, offset}`: logs are never synced; the run page reads them
/// on demand and tails a running job.
pub fn log(trpc: &TrpcClient, job_id: &str, offset: u64) -> Result<LogRead, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        job_id: &'a str,
        offset: u64,
    }
    trpc.query_with_input("ci.log", &Input { job_id, offset })
}

/// `ci.rerun{runId, failedOnly}`: the new run's id.
pub fn rerun(trpc: &TrpcClient, run_id: &str, failed_only: bool) -> Result<String, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        run_id: &'a str,
        failed_only: bool,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Out {
        run_id: String,
    }
    let out: Out = trpc.mutation("ci.rerun", &Input { run_id, failed_only })?;
    Ok(out.run_id)
}

/// `ci.cancel{runId}`.
pub fn cancel(trpc: &TrpcClient, run_id: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        run_id: &'a str,
    }
    let _: Value = trpc.mutation("ci.cancel", &Input { run_id })?;
    Ok(())
}

/// The runner settings `ci.setRunner` writes (the whole object).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerSettingsInput {
    /// The steer device id (`devices.device_id`), not the row's id.
    pub device_id: String,
    pub enabled: bool,
    pub labels: Vec<String>,
    pub slots: u32,
    pub timeout_min: u32,
}

/// `ci.setRunner`: the device OWNER only.
pub fn set_runner(trpc: &TrpcClient, input: &RunnerSettingsInput) -> Result<(), ApiError> {
    let _: Value = trpc.mutation("ci.setRunner", input)?;
    Ok(())
}

#[cfg(test)]
mod people_tests {
    use super::*;

    #[test]
    fn a_log_read_decodes_the_wire() {
        let read: LogRead = serde_json::from_str(r#"{"text":"ok\n","nextOffset":3,"complete":false}"#).unwrap();
        assert_eq!(read, LogRead { text: "ok\n".into(), next_offset: 3, complete: false });
    }

    #[test]
    fn set_runner_sends_the_whole_object() {
        let input = RunnerSettingsInput { device_id: "dev-1".into(), enabled: true, labels: vec!["gpu".into()], slots: 2, timeout_min: 60 };
        assert_eq!(
            serde_json::to_value(&input).unwrap(),
            serde_json::json!({"deviceId": "dev-1", "enabled": true, "labels": ["gpu"], "slots": 2, "timeoutMin": 60})
        );
    }
}

/// One GitHub repository this device serves Actions for (#EXP-1617 Q6).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionsRunnerTarget {
    pub repository_id: String,
    pub full_name: String,
    pub url: String,
    pub labels: Vec<String>,
    pub name: String,
}

/// `ci.actionsRunners`: what this device registers `actions/runner` for.
pub fn actions_runners(trpc: &TrpcClient, device_id: &str) -> Result<Vec<ActionsRunnerTarget>, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        device_id: &'a str,
    }
    #[derive(Deserialize)]
    struct Out {
        targets: Vec<ActionsRunnerTarget>,
    }
    let out: Out = trpc.query_with_input("ci.actionsRunners", &Input { device_id })?;
    Ok(out.targets)
}

/// `ci.actionsRunnerToken`: `kind` = `registration` | `remove`.
pub fn actions_runner_token(trpc: &TrpcClient, device_id: &str, repository_id: &str, kind: &str) -> Result<String, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        device_id: &'a str,
        repository_id: &'a str,
        kind: &'a str,
    }
    #[derive(Deserialize)]
    struct Out {
        token: String,
    }
    let out: Out = trpc.mutation("ci.actionsRunnerToken", &Input { device_id, repository_id, kind })?;
    Ok(out.token)
}

// ── the repo page's CI section (#EXP-1617 E4 part 4, repo-ci.json) ───────

/// A repository's `ci_config` (every field optional on the wire).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoCiConfig {
    #[serde(default)]
    pub required_checks: Option<Vec<String>>,
    #[serde(default)]
    pub github_statuses: Option<bool>,
    #[serde(default)]
    pub actions_runners: Option<bool>,
}

/// One secret's NAME (values are never read back).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoCiSecret {
    pub name: String,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `ci.repoSettings`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct RepoCiSettings {
    #[serde(default, deserialize_with = "config_or_default")]
    pub config: RepoCiConfig,
    #[serde(default)]
    pub secrets: Vec<RepoCiSecret>,
}

fn config_or_default<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<RepoCiConfig, D::Error> {
    Ok(Option::<RepoCiConfig>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepoRef<'a> {
    repository_id: &'a str,
}

/// `ci.repoSettings{repositoryId}`: the config and the secret names.
pub fn repo_settings(trpc: &TrpcClient, repository_id: &str) -> Result<RepoCiSettings, ApiError> {
    trpc.query_with_input("ci.repoSettings", &RepoRef { repository_id })
}

/// What `ci.setRepoConfig` changes (`None` = left as is).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoCiConfigInput {
    pub repository_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_checks: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_statuses: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions_runners: Option<bool>,
}

/// `ci.setRepoConfig`: the repo's managers only.
pub fn set_repo_config(trpc: &TrpcClient, input: &RepoCiConfigInput) -> Result<(), ApiError> {
    let _: Value = trpc.mutation("ci.setRepoConfig", input)?;
    Ok(())
}

/// `ci.setSecret{repositoryId, name, value}`: create or replace.
pub fn set_secret(trpc: &TrpcClient, repository_id: &str, name: &str, value: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        repository_id: &'a str,
        name: &'a str,
        value: &'a str,
    }
    let _: Value = trpc.mutation("ci.setSecret", &Input { repository_id, name, value })?;
    Ok(())
}

/// `ci.deleteSecret{repositoryId, name}`.
pub fn delete_secret(trpc: &TrpcClient, repository_id: &str, name: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        repository_id: &'a str,
        name: &'a str,
    }
    let _: Value = trpc.mutation("ci.deleteSecret", &Input { repository_id, name })?;
    Ok(())
}

#[cfg(test)]
mod repo_ci_tests {
    use super::*;

    #[test]
    fn repo_settings_decode_the_wire() {
        let out: RepoCiSettings =
            serde_json::from_str(r#"{"config":{"requiredChecks":["build"],"githubStatuses":true},"secrets":[{"name":"NPM_TOKEN","updatedAt":"2026-11-13T09:00:00.000Z","updatedByUserId":"u1"}]}"#).unwrap();
        assert_eq!(out.config.required_checks.as_deref(), Some(&["build".to_string()][..]));
        assert_eq!(out.config.github_statuses, Some(true));
        assert_eq!(out.config.actions_runners, None);
        assert_eq!(out.secrets[0].name, "NPM_TOKEN");
        let empty: RepoCiSettings = serde_json::from_str(r#"{"config":null,"secrets":[]}"#).unwrap();
        assert_eq!(empty, RepoCiSettings::default());
    }

    #[test]
    fn set_repo_config_sends_only_what_changed() {
        let input = RepoCiConfigInput { repository_id: "r1".into(), github_statuses: Some(false), ..Default::default() };
        assert_eq!(serde_json::to_value(&input).unwrap(), serde_json::json!({"repositoryId": "r1", "githubStatuses": false}));
    }
}
