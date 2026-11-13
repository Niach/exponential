//! #EXP-1617 E4 part 4 — THE REPO PAGE'S RUNS TAB AND ITS CI SETTINGS, the
//! pure half ×4 (`packages/domain-contract/fixtures/repo-ci.json`, TS
//! reference `packages/domain-contract/src/repo-ci.ts`). A run's status is
//! ci-run.json's ([`crate::ci_run::ci_run_status`]); durations are
//! pr-checks' ([`crate::pr_checks::format_check_duration`]). The copy is
//! read off the fixture itself (one source, never a Rust mirror to drift).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use serde_json::Value;

use crate::ci_run::{ci_run_status, CiRunStatus};
use crate::pr_checks::{format_check_duration, CheckGlyph, CheckTone};
use crate::rows::{CiJobRow, CiRunRow};

const REPO_CI_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/repo-ci.json");

/// repo-ci.json as parsed once.
pub static REPO_CI: LazyLock<Value> = LazyLock::new(|| serde_json::from_str(REPO_CI_JSON).expect("repo-ci.json parses"));

/// repo-ci.json `copy.<path>` (`runs.columns.status`, `ci.secrets.add`…).
pub fn copy(path: &str) -> &'static str {
    path.split('.').fold(&REPO_CI["copy"], |value, key| &value[key]).as_str().unwrap_or_default()
}

/// repo-ci.json `testIds.<key>`.
pub fn test_id(key: &str) -> &'static str {
    REPO_CI["testIds"][key].as_str().unwrap_or_default()
}

/// `limits.runs`: the most runs the Runs tab lists.
pub fn runs_limit() -> usize {
    REPO_CI["limits"]["runs"].as_u64().unwrap_or(50) as usize
}

/// The Runs tab's columns in order: (key, header) off `copy.runs.columns`.
pub fn run_columns() -> Vec<(&'static str, &'static str)> {
    ["status", "run", "commit", "ref", "trigger", "duration", "started"]
        .into_iter()
        .map(|key| (key, copy(&format!("runs.columns.{key}"))))
        .collect()
}

/// One run of the Runs tab (`runRows`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoRunRow {
    pub id: String,
    /// `#<number>`.
    pub identifier: String,
    pub title: String,
    pub status: CiRunStatus,
    pub status_label: &'static str,
    pub glyph: CheckGlyph,
    pub tone: CheckTone,
    /// The sha's first 7.
    pub commit: String,
    /// The run's ref, else `copy.runs.noRef`.
    pub r#ref: String,
    /// `copy.triggers[trigger]`, else the raw trigger.
    pub trigger: String,
    /// Phones: `ref · sha7 · trigger`.
    pub subtitle: String,
    pub duration: Option<String>,
    pub started_at: Option<String>,
}

/// A run status's glyph and tint.
pub fn status_glyph(status: CiRunStatus) -> (CheckGlyph, CheckTone) {
    match status {
        CiRunStatus::Queued => (CheckGlyph::Queued, CheckTone::Muted),
        CiRunStatus::Running => (CheckGlyph::Running, CheckTone::Muted),
        CiRunStatus::Passed => (CheckGlyph::Passed, CheckTone::Green),
        CiRunStatus::Failed => (CheckGlyph::Failed, CheckTone::Red),
        CiRunStatus::Cancelled => (CheckGlyph::Skipped, CheckTone::Muted),
    }
}

fn ms_of(value: Option<&str>) -> Option<i64> {
    value.filter(|v| !v.is_empty()).and_then(crate::instant::parse_instant)
}

fn trigger_label(trigger: &str) -> String {
    REPO_CI["copy"]["triggers"][trigger].as_str().map(str::to_string).unwrap_or_else(|| trigger.to_string())
}

/// `runRows`: newest run number first, at most [`runs_limit`]. `now_ms` =
/// UTC milliseconds (a running run's duration).
pub fn run_rows(runs: &[CiRunRow], jobs: &[CiJobRow], now_ms: i64) -> Vec<RepoRunRow> {
    let mut sorted: Vec<&CiRunRow> = runs.iter().collect();
    sorted.sort_by(|a, b| b.number.unwrap_or(0).cmp(&a.number.unwrap_or(0)));
    sorted
        .into_iter()
        .take(runs_limit())
        .map(|run| {
            let own: Vec<CiJobRow> = jobs.iter().filter(|job| job.run_id == run.id).cloned().collect();
            let status = ci_run_status(&run.state, &own);
            let (glyph, tone) = status_glyph(status);
            let commit: String = run.sha.chars().take(7).collect();
            let r#ref = run.r#ref.clone().unwrap_or_else(|| copy("runs.noRef").to_string());
            let trigger = trigger_label(run.trigger.as_deref().unwrap_or_default());
            let duration = ms_of(run.started_at.as_deref()).map(|started| format_check_duration(ms_of(run.completed_at.as_deref()).unwrap_or(now_ms) - started));
            RepoRunRow {
                id: run.id.clone(),
                identifier: format!("#{}", run.number.unwrap_or(0)),
                title: run.name.clone(),
                status,
                status_label: copy(&format!("status.{}", status.id())),
                glyph,
                tone,
                subtitle: format!("{} \u{b7} {} \u{b7} {}", r#ref, commit, trigger),
                commit,
                r#ref,
                trigger,
                duration,
                started_at: run.started_at.clone(),
            }
        })
        .collect()
}

/// One pipeline of the CI section (`pipelineRows`): its newest run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineRow {
    pub name: String,
    pub run_id: String,
    /// `pending` | `success` | `failure` (repo-page.json `copy.checks`).
    pub state: &'static str,
}

/// `pipelineRows`: one row per pipeline name (its newest run), by name.
pub fn pipeline_rows(runs: &[CiRunRow]) -> Vec<PipelineRow> {
    let mut newest: BTreeMap<&str, &CiRunRow> = BTreeMap::new();
    for run in runs {
        match newest.get(run.name.as_str()) {
            Some(seen) if run.number.unwrap_or(0) <= seen.number.unwrap_or(0) => {}
            _ => {
                newest.insert(run.name.as_str(), run);
            }
        }
    }
    newest
        .into_values()
        .map(|run| PipelineRow {
            name: run.name.clone(),
            run_id: run.id.clone(),
            state: match run.state.as_str() {
                "success" => "success",
                "failure" => "failure",
                _ => "pending",
            },
        })
        .collect()
}

/// `requiredSuggestions`: the checks of each pipeline's newest run, minus
/// the required ones, sorted.
pub fn required_suggestions(runs: &[CiRunRow], jobs: &[CiJobRow], required: &[String]) -> Vec<String> {
    let newest: BTreeSet<String> = pipeline_rows(runs).into_iter().map(|row| row.run_id).collect();
    let mut names: BTreeSet<String> = jobs.iter().filter(|job| newest.contains(&job.run_id)).map(|job| job.check.clone()).collect();
    for name in required {
        names.remove(name);
    }
    names.into_iter().collect()
}

/// `secretName`: an env var name the server stores (`[A-Z_][A-Z0-9_]*`, ≤ 128).
pub fn secret_name(input: &str) -> bool {
    let mut chars = input.chars();
    input.len() <= 128
        && chars.next().is_some_and(|c| c.is_ascii_uppercase() || c == '_')
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// `switches`: the GitHub-only switches of the CI section, in order.
pub fn switches(provider: &str) -> Vec<&'static str> {
    if provider == "github" {
        vec!["githubStatuses", "actionsRunners"]
    } else {
        Vec::new()
    }
}

// ── the fixture as rows (the specimens draw from it) ─────────────────────

fn snake(key: &str) -> String {
    let mut out = String::new();
    for c in key.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn snake_object(value: &Value, index: usize, prefix: &str) -> Value {
    let mut object: serde_json::Map<String, Value> = value.as_object().map(|o| o.iter().map(|(k, v)| (snake(k), v.clone())).collect()).unwrap_or_default();
    object.entry("id").or_insert_with(|| Value::String(format!("{prefix}-{index}")));
    Value::Object(object)
}

/// The fixture's camelCase runs as synced rows.
pub fn fixture_runs(runs: &Value) -> Vec<CiRunRow> {
    runs.as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, run)| serde_json::from_value(snake_object(run, index, "run")).ok())
        .collect()
}

/// The fixture's camelCase jobs as synced rows.
pub fn fixture_jobs(jobs: &Value) -> Vec<CiJobRow> {
    jobs.as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, job)| serde_json::from_value(snake_object(job, index, "job")).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/repo-ci.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).expect("repo-ci.json parses")
    }

    fn row_json(row: &RepoRunRow) -> Value {
        json!({
            "id": row.id,
            "identifier": row.identifier,
            "title": row.title,
            "status": row.status.id(),
            "statusLabel": row.status_label,
            "glyph": row.glyph.concept(),
            "tone": row.tone.id(),
            "commit": row.commit,
            "ref": row.r#ref,
            "trigger": row.trigger,
            "subtitle": row.subtitle,
            "duration": row.duration,
            "startedAt": row.started_at,
        })
    }

    #[test]
    fn run_rows_match_the_fixture() {
        for case in fixture()["runRows"].as_array().unwrap() {
            let now = crate::instant::parse_instant(case["now"].as_str().unwrap()).unwrap();
            let rows: Vec<Value> = run_rows(&fixture_runs(&case["runs"]), &fixture_jobs(&case["jobs"]), now).iter().map(row_json).collect();
            assert_eq!(Value::Array(rows), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn pipeline_rows_match_the_fixture() {
        for case in fixture()["pipelineRows"].as_array().unwrap() {
            let rows: Vec<Value> =
                pipeline_rows(&fixture_runs(&case["runs"])).into_iter().map(|row| json!({"name": row.name, "runId": row.run_id, "state": row.state})).collect();
            assert_eq!(Value::Array(rows), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn required_suggestions_match_the_fixture() {
        for case in fixture()["requiredSuggestions"].as_array().unwrap() {
            let required: Vec<String> = case["required"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
            let got = required_suggestions(&fixture_runs(&case["runs"]), &fixture_jobs(&case["jobs"]), &required);
            assert_eq!(json!(got), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn secret_names_match_the_fixture() {
        for case in fixture()["secretName"].as_array().unwrap() {
            let input = case["input"].as_str().unwrap();
            assert_eq!(secret_name(input), case["valid"].as_bool().unwrap(), "{input}");
        }
    }

    #[test]
    fn switches_match_the_fixture() {
        for case in fixture()["switches"].as_array().unwrap() {
            assert_eq!(json!(switches(case["provider"].as_str().unwrap())), case["expected"], "{}", case["provider"]);
        }
    }

    #[test]
    fn the_copy_and_limits_read_off_the_fixture() {
        assert_eq!(runs_limit(), 50);
        assert_eq!(run_columns().len(), 7);
        assert_eq!(copy("ci.secrets.add"), "Add secret");
        assert_eq!(test_id("runs"), "repo-runs");
    }
}
