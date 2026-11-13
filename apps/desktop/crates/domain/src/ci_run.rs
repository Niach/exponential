//! #EXP-1617 / #EXP-1804 — our own CI, the pure half ×4: THE CI RUN PAGE
//! (`packages/domain-contract/fixtures/ci-run.json`) and a device's CI
//! RUNNER SECTION (`fixtures/runner-settings.json`); TS reference
//! `packages/domain-contract/src/ci-run.ts`. The glyphs, tones and
//! durations are pr-checks' ([`crate::pr_checks`]). The copy is read off the
//! fixtures themselves (one source, never a Rust mirror to drift).

use std::sync::LazyLock;

use serde_json::Value;

use crate::pr_checks::{check_duration_of, check_glyph, format_check_duration, is_failing, CheckGlyph, CheckTone};
use crate::rows::{CiJobRow, CiRunRow};

const CI_RUN_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/ci-run.json");
const RUNNER_SETTINGS_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/runner-settings.json");

/// ci-run.json as parsed once.
pub static CI_RUN: LazyLock<Value> = LazyLock::new(|| serde_json::from_str(CI_RUN_JSON).expect("ci-run.json parses"));
/// runner-settings.json as parsed once.
pub static RUNNER_SETTINGS: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(RUNNER_SETTINGS_JSON).expect("runner-settings.json parses"));

fn at<'a>(root: &'a Value, path: &str) -> &'a Value {
    path.split('.').fold(root, |value, key| &value[key])
}

fn text_at(root: &'static Value, path: &str) -> &'static str {
    at(root, path).as_str().unwrap_or_default()
}

/// ci-run.json `copy.<path>` (`tiles.status`, `reasons.needs`…).
pub fn copy(path: &str) -> &'static str {
    text_at(&CI_RUN["copy"], path)
}

/// ci-run.json `testIds.<key>`.
pub fn test_id(key: &str) -> &'static str {
    text_at(&CI_RUN["testIds"], key)
}

/// runner-settings.json `copy.<path>`.
pub fn runner_copy(path: &str) -> &'static str {
    text_at(&RUNNER_SETTINGS["copy"], path)
}

/// runner-settings.json `testIds.<key>`.
pub fn runner_test_id(key: &str) -> &'static str {
    text_at(&RUNNER_SETTINGS["testIds"], key)
}

/// `liveTailMs`: how often a running job's log is read again.
pub fn live_tail_ms() -> u64 {
    CI_RUN["liveTailMs"].as_u64().unwrap_or(2000)
}

/// `logMaxLines.wide` (the desktop is wide).
pub fn log_max_lines() -> u64 {
    CI_RUN["logMaxLines"]["wide"].as_u64().unwrap_or(32)
}

/// `copy.title` with the run's number.
pub fn run_title(number: i64) -> String {
    copy("title").replace("{number}", &number.to_string())
}

// ── the run page (ci-run.json) ───────────────────────────────────────────

/// The run's status for its first tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CiRunStatus {
    Queued,
    Running,
    Passed,
    Failed,
    Cancelled,
}

impl CiRunStatus {
    pub fn id(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn tone(self) -> CiTone {
        match self {
            Self::Queued | Self::Cancelled => CiTone::Neutral,
            Self::Running => CiTone::Primary,
            Self::Passed => CiTone::Success,
            Self::Failed => CiTone::Danger,
        }
    }
}

/// A tile's tone (genUI `tone`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CiTone {
    Neutral,
    Primary,
    Success,
    Danger,
}

impl CiTone {
    pub fn id(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Primary => "primary",
            Self::Success => "success",
            Self::Danger => "danger",
        }
    }
}

fn job_failing(job: &CiJobRow) -> bool {
    is_failing(&job.status, job.conclusion.as_deref())
}

/// `runStatus`: queued = pending with nothing in progress, running = pending
/// with a job in progress, passed = success, cancelled = a failure whose
/// failing jobs are ALL cancelled, failed = any other failure.
pub fn ci_run_status(state: &str, jobs: &[CiJobRow]) -> CiRunStatus {
    match state {
        "pending" if jobs.iter().any(|job| job.status == "in_progress") => CiRunStatus::Running,
        "pending" => CiRunStatus::Queued,
        "success" => CiRunStatus::Passed,
        _ => {
            let failing: Vec<&CiJobRow> = jobs.iter().filter(|job| job_failing(job)).collect();
            if !failing.is_empty() && failing.iter().all(|job| job.conclusion.as_deref() == Some("cancelled")) {
                CiRunStatus::Cancelled
            } else {
                CiRunStatus::Failed
            }
        }
    }
}

/// One `StatTile`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CiTile {
    pub label: String,
    pub value: String,
    pub tone: CiTone,
    /// The Status tile of a failed run: `{count} failed` (red); else None.
    pub delta: Option<String>,
}

fn ms_of(value: Option<&str>) -> Option<i64> {
    value.filter(|v| !v.is_empty()).and_then(crate::instant::parse_instant)
}

fn by_position(jobs: &[CiJobRow]) -> Vec<&CiJobRow> {
    let mut ordered: Vec<&CiJobRow> = jobs.iter().collect();
    ordered.sort_by_key(|job| job.position.unwrap_or(0));
    ordered
}

/// `tiles`: Status, Duration, Runner (the distinct runner LABELS of the jobs
/// that ran, in planner order; `runner_label` names a job's runner), Commit.
pub fn ci_run_tiles(
    run: &CiRunRow,
    jobs: &[CiJobRow],
    runner_label: &dyn Fn(&CiJobRow) -> Option<String>,
    now_ms: i64,
) -> Vec<CiTile> {
    let status = ci_run_status(&run.state, jobs);
    let duration = ms_of(run.started_at.as_deref()).map(|started| {
        let end = ms_of(run.completed_at.as_deref()).unwrap_or(now_ms);
        format_check_duration(end - started)
    });
    let mut runners: Vec<String> = Vec::new();
    for job in by_position(jobs) {
        if let Some(label) = runner_label(job).filter(|label| !label.is_empty()) {
            if !runners.contains(&label) {
                runners.push(label);
            }
        }
    }
    let none = copy("none").to_string();
    let failing = jobs.iter().filter(|job| job_failing(job)).count();
    let delta = (status.id() == "failed" && failing > 0).then(|| copy("failedJobs").replace("{count}", &failing.to_string()));
    vec![
        CiTile { label: copy("tiles.status").into(), value: copy(&format!("status.{}", status.id())).into(), tone: status.tone(), delta },
        CiTile { label: copy("tiles.duration").into(), value: duration.unwrap_or_else(|| none.clone()), tone: CiTone::Neutral, delta: None },
        CiTile {
            label: copy("tiles.runner").into(),
            value: if runners.is_empty() { none } else { runners.join(", ") },
            tone: CiTone::Neutral,
            delta: None,
        },
        CiTile { label: copy("tiles.commit").into(), value: run.sha.chars().take(7).collect(), tone: CiTone::Neutral, delta: None },
    ]
}

/// One job's `Row` (`jobRows`, and a runner's `recentJobs`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CiJobListRow {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub glyph: CheckGlyph,
    pub tone: CheckTone,
    pub duration: Option<String>,
}

fn reason_text(reason: Option<&str>) -> Option<String> {
    let reason = reason?;
    CI_RUN["copy"]["reasons"][reason].as_str().map(str::to_string)
}

/// `jobRows`: the jobs in planner order.
pub fn ci_job_rows(jobs: &[CiJobRow], now_ms: i64) -> Vec<CiJobListRow> {
    by_position(jobs)
        .into_iter()
        .map(|job| {
            let (glyph, tone) = check_glyph(&job.status, job.conclusion.as_deref());
            CiJobListRow {
                id: job.id.clone(),
                title: job.check.clone(),
                subtitle: if job.status == "completed" { reason_text(job.reason.as_deref()) } else { None },
                glyph,
                tone,
                duration: check_duration_of(&job.status, job.started_at.as_deref(), job.completed_at.as_deref(), now_ms),
            }
        })
        .collect()
}

/// `defaultJob`: the requested job when it is one of the run's, else the
/// first failing, else the first running, else the first.
pub fn ci_default_job(jobs: &[CiJobRow], requested: Option<&str>) -> Option<String> {
    let ordered = by_position(jobs);
    if let Some(requested) = requested.filter(|id| ordered.iter().any(|job| job.id == *id)) {
        return Some(requested.to_string());
    }
    ordered
        .iter()
        .find(|job| job_failing(job))
        .or_else(|| ordered.iter().find(|job| job.status == "in_progress"))
        .or_else(|| ordered.first())
        .map(|job| job.id.clone())
}

/// One step as the runner reports it (`ci_jobs.steps[]`, epoch ms).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CiStep {
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

/// A job's `steps` jsonb (tolerant: a malformed entry is skipped).
pub fn ci_steps_of(steps: Option<&Value>) -> Vec<CiStep> {
    steps
        .and_then(Value::as_array)
        .map(|steps| {
            steps
                .iter()
                .filter_map(|step| {
                    Some(CiStep {
                        name: step["name"].as_str()?.to_string(),
                        status: step["status"].as_str().unwrap_or("queued").to_string(),
                        conclusion: step["conclusion"].as_str().map(str::to_string),
                        started_at: step["startedAt"].as_i64(),
                        completed_at: step["completedAt"].as_i64(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A `TraceItem`'s status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CiTraceStatus {
    Pending,
    Running,
    Done,
    Skipped,
    Error,
}

impl CiTraceStatus {
    pub fn id(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Done => "done",
            Self::Skipped => "skipped",
            Self::Error => "error",
        }
    }
}

/// One step as a `TraceItem`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CiTraceItem {
    pub title: String,
    pub status: CiTraceStatus,
    pub description: Option<String>,
}

fn trace_status(step: &CiStep) -> CiTraceStatus {
    match (step.status.as_str(), step.conclusion.as_deref()) {
        ("queued", _) => CiTraceStatus::Pending,
        ("completed", Some("success")) => CiTraceStatus::Done,
        ("completed", Some("skipped" | "neutral")) => CiTraceStatus::Skipped,
        ("completed", _) => CiTraceStatus::Error,
        _ => CiTraceStatus::Running,
    }
}

/// `steps`: a job's steps as trace items, the description = the duration.
pub fn ci_steps(steps: &[CiStep], now_ms: i64) -> Vec<CiTraceItem> {
    steps
        .iter()
        .map(|step| {
            let end = match step.status.as_str() {
                "completed" => step.completed_at,
                "in_progress" => Some(now_ms),
                _ => None,
            };
            CiTraceItem {
                title: step.name.clone(),
                status: trace_status(step),
                description: match (step.started_at, end) {
                    (Some(started), Some(end)) => Some(format_check_duration(end - started)),
                    _ => None,
                },
            }
        })
        .collect()
}

/// The log as shown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CiLogView {
    pub code: String,
    /// 1-based DISPLAYED lines of the runner's errors.
    pub highlight: Vec<usize>,
}

/// `logView`: the runner's markers become plain lines (`##[group]X` → `▸ X`,
/// `##[endgroup]` dropped, `##[error]X` → `Error: X` highlighted,
/// `##[warning]X` → `Warning: X`); a trailing newline adds no line.
pub fn ci_log_view(text: &str) -> CiLogView {
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut highlight = Vec::new();
    for line in lines {
        if line.starts_with("##[endgroup]") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("##[group]") {
            out.push(format!("▸ {rest}"));
        } else if let Some(rest) = line.strip_prefix("##[error]") {
            out.push(format!("Error: {rest}"));
            highlight.push(out.len());
        } else if let Some(rest) = line.strip_prefix("##[warning]") {
            out.push(format!("Warning: {rest}"));
        } else {
            out.push(line.to_string());
        }
    }
    CiLogView { code: out.join("\n"), highlight }
}

/// Which buttons the run shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CiRunActions {
    pub rerun_failed: bool,
    pub cancel: bool,
}

/// `actions`: Re-run failed on a failed run (a pipeline error re-plans too),
/// Cancel on a pending one.
pub fn ci_run_actions(state: &str) -> CiRunActions {
    CiRunActions { rerun_failed: state == "failure", cancel: state == "pending" }
}

/// The run page's link (`details_url`): `/t/<team>/ci/<runId>[?job=<jobId>]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CiRunLink {
    pub team_slug: String,
    pub run_id: String,
    pub job_id: Option<String>,
}

/// Parses an instance PATH (query and hash kept, as `AppLink::App` holds
/// it) into the run page's link; `None` = another page.
pub fn ci_run_link(path: &str) -> Option<CiRunLink> {
    let path = path.split('#').next().unwrap_or_default();
    let (path, query) = match path.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (path, None),
    };
    let segments: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    let ["", "t", team_slug, "ci", run_id] = segments.as_slice() else {
        return None;
    };
    if team_slug.is_empty() || run_id.is_empty() {
        return None;
    }
    let job_id = query.and_then(|query| {
        query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, value)| *key == "job" && !value.is_empty())
            .map(|(_, value)| value.to_string())
    });
    Some(CiRunLink { team_slug: team_slug.to_string(), run_id: run_id.to_string(), job_id })
}

// ── the runner section (runner-settings.json) ────────────────────────────

/// `devices.runner` (`defaults` fill a NULL one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunnerSettings {
    pub enabled: bool,
    pub labels: Vec<String>,
    pub slots: u32,
    pub timeout_min: u32,
}

/// The row's `runner` jsonb, the fixture's `defaults` for NULL or a missing
/// key.
pub fn runner_settings(runner: Option<&Value>) -> RunnerSettings {
    let defaults = &RUNNER_SETTINGS["defaults"];
    let pick = |key: &str| runner.and_then(|runner| runner.get(key)).filter(|value| !value.is_null()).unwrap_or(&defaults[key]);
    RunnerSettings {
        enabled: pick("enabled").as_bool().unwrap_or(false),
        labels: pick("labels")
            .as_array()
            .map(|labels| labels.iter().filter_map(|label| label.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        slots: pick("slots").as_u64().unwrap_or(1) as u32,
        timeout_min: pick("timeoutMin").as_u64().unwrap_or(60) as u32,
    }
}

/// `limits.labels`.
pub fn runner_max_labels() -> usize {
    RUNNER_SETTINGS["limits"]["labels"].as_u64().unwrap_or(16) as usize
}

/// `limits.slots` (min, max).
pub fn runner_slot_limits() -> (u32, u32) {
    let slots = &RUNNER_SETTINGS["limits"]["slots"];
    (slots["min"].as_u64().unwrap_or(1) as u32, slots["max"].as_u64().unwrap_or(16) as u32)
}

fn runner_recent_limit() -> usize {
    RUNNER_SETTINGS["limits"]["recent"].as_u64().unwrap_or(5) as usize
}

/// `timeoutOptions` as (value, label).
pub fn runner_timeout_options() -> Vec<(String, String)> {
    RUNNER_SETTINGS["timeoutOptions"]
        .as_array()
        .map(|options| {
            options
                .iter()
                .map(|option| {
                    (
                        option["value"].as_str().unwrap_or_default().to_string(),
                        option["label"].as_str().unwrap_or_default().to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `normalizeLabel`: trimmed, lowercased, then eCode's label rule
/// `^[a-z0-9][a-z0-9._-]{0,31}$`; `None` = refused.
pub fn normalize_runner_label(input: &str) -> Option<String> {
    let label = input.trim().to_lowercase();
    let mut chars = label.chars();
    let first = chars.next()?;
    let ok = (first.is_ascii_lowercase() || first.is_ascii_digit())
        && label.chars().count() <= 32
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    ok.then_some(label)
}

/// The section's notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunnerNotice {
    Phone,
    Update,
}

impl RunnerNotice {
    pub fn id(self) -> &'static str {
        match self {
            Self::Phone => "phone",
            Self::Update => "update",
        }
    }
}

/// `notice`: a phone client never runs jobs; a device without the
/// `ci-runner` cap is too old to.
pub fn runner_notice(client: &str, caps: &[String]) -> Option<RunnerNotice> {
    if matches!(client, "ios" | "android" | "web-mobile") {
        Some(RunnerNotice::Phone)
    } else if !caps.iter().any(|cap| cap == "ci-runner") {
        Some(RunnerNotice::Update)
    } else {
        None
    }
}

/// `osLabel`: the OS label a runner always has.
pub fn runner_os_label(platform: Option<&str>) -> Option<&'static str> {
    match platform? {
        "linux" => Some("linux"),
        "macos" | "darwin" => Some("mac"),
        "windows" => Some("windows"),
        _ => None,
    }
}

/// A job with its run's number and repository, for `recentJobs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunnerJob {
    pub id: String,
    pub runner_device_id: Option<String>,
    pub check: String,
    pub run_number: i64,
    pub repo: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// `recentJobs`: this runner's jobs, newest start first (never started
/// last), at most `limits.recent`.
pub fn runner_recent_jobs(device_id: &str, jobs: &[RunnerJob], now_ms: i64) -> Vec<CiJobListRow> {
    let mut mine: Vec<&RunnerJob> = jobs.iter().filter(|job| job.runner_device_id.as_deref() == Some(device_id)).collect();
    mine.sort_by_key(|job| std::cmp::Reverse(ms_of(job.started_at.as_deref())));
    mine.into_iter()
        .take(runner_recent_limit())
        .map(|job| {
            let (glyph, tone) = check_glyph(&job.status, job.conclusion.as_deref());
            CiJobListRow {
                id: job.id.clone(),
                title: job.check.clone(),
                subtitle: Some(
                    runner_copy("runSubtitle").replace("{number}", &job.run_number.to_string()).replace("{repo}", &job.repo),
                ),
                glyph,
                tone,
                duration: check_duration_of(&job.status, job.started_at.as_deref(), job.completed_at.as_deref(), now_ms),
            }
        })
        .collect()
}

// ── the fixtures as rows (the specimens ×4 draw from them) ───────────────

/// The fixture's camelCase jobs as synced rows (`runnerLabel` rides in
/// `runner_device_id`: the specimens name runners by label; a missing
/// `position` = the index).
pub fn fixture_jobs(jobs: &Value) -> Vec<CiJobRow> {
    jobs.as_array()
        .map(|jobs| {
            jobs.iter()
                .enumerate()
                .map(|(index, job)| {
                    let text = |key: &str| job[key].as_str().map(str::to_string);
                    CiJobRow {
                        id: text("id").unwrap_or_else(|| format!("job-{index}")),
                        team_id: None,
                        run_id: String::new(),
                        key: None,
                        check: text("check").unwrap_or_default(),
                        position: Some(job["position"].as_i64().unwrap_or(index as i64)),
                        runs_on: None,
                        needs: None,
                        matrix: None,
                        timeout_min: None,
                        status: text("status").unwrap_or_default(),
                        conclusion: text("conclusion"),
                        reason: text("reason"),
                        runner_device_id: text("runnerLabel"),
                        steps: job.get("steps").cloned(),
                        log_key: None,
                        log_bytes: None,
                        queued_at: None,
                        started_at: text("startedAt"),
                        completed_at: text("completedAt"),
                        created_at: None,
                        updated_at: None,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The fixture's camelCase run as a synced row.
pub fn fixture_run(run: &Value) -> CiRunRow {
    let text = |key: &str| run[key].as_str().map(str::to_string);
    CiRunRow {
        id: text("id").unwrap_or_default(),
        team_id: None,
        repository_id: None,
        number: run["number"].as_i64(),
        name: text("name").unwrap_or_default(),
        sha: text("sha").unwrap_or_default(),
        r#ref: None,
        trigger: None,
        pull_request_id: None,
        state: text("state").unwrap_or_default(),
        error: text("error"),
        rerun_of: None,
        created_by_user_id: None,
        started_at: text("startedAt"),
        completed_at: text("completedAt"),
        created_at: None,
        updated_at: None,
    }
}

/// runner-settings.json `recentJobs[]` jobs as [`RunnerJob`]s.
pub fn fixture_runner_jobs(jobs: &Value) -> Vec<RunnerJob> {
    jobs.as_array()
        .map(|jobs| {
            jobs.iter()
                .map(|job| {
                    let text = |key: &str| job[key].as_str().map(str::to_string);
                    RunnerJob {
                        id: text("id").unwrap_or_default(),
                        runner_device_id: text("runnerDeviceId"),
                        check: text("check").unwrap_or_default(),
                        run_number: job["runNumber"].as_i64().unwrap_or_default(),
                        repo: text("repo").unwrap_or_default(),
                        status: text("status").unwrap_or_default(),
                        conclusion: text("conclusion"),
                        started_at: text("startedAt"),
                        completed_at: text("completedAt"),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/ci-run.json");
    const RUNNER: &str = include_str!("../../../../../packages/domain-contract/fixtures/runner-settings.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).expect("ci-run.json parses")
    }

    fn runner() -> Value {
        serde_json::from_str(RUNNER).expect("runner-settings.json parses")
    }

    fn now(case: &Value) -> i64 {
        crate::instant::parse_instant(case["now"].as_str().unwrap()).unwrap()
    }

    fn row_json(row: &CiJobListRow) -> Value {
        json!({
            "id": row.id,
            "title": row.title,
            "subtitle": row.subtitle,
            "glyph": row.glyph.concept(),
            "tone": row.tone.id(),
            "duration": row.duration,
        })
    }

    #[test]
    fn run_status_matches_the_fixture() {
        for case in fixture()["runStatus"].as_array().unwrap() {
            let status = ci_run_status(case["state"].as_str().unwrap(), &fixture_jobs(&case["jobs"]));
            assert_eq!(json!({"status": status.id(), "tone": status.tone().id()}), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn tiles_match_the_fixture() {
        for case in fixture()["tiles"].as_array().unwrap() {
            let tiles = ci_run_tiles(&fixture_run(&case["run"]), &fixture_jobs(&case["jobs"]), &|job| job.runner_device_id.clone(), now(case));
            let got: Vec<Value> =
                tiles.iter().map(|tile| json!({"label": tile.label, "value": tile.value, "tone": tile.tone.id(), "delta": tile.delta})).collect();
            assert_eq!(Value::Array(got), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn job_rows_match_the_fixture() {
        for case in fixture()["jobRows"].as_array().unwrap() {
            let got: Vec<Value> = ci_job_rows(&fixture_jobs(&case["jobs"]), now(case)).iter().map(row_json).collect();
            assert_eq!(Value::Array(got), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn default_job_matches_the_fixture() {
        for case in fixture()["defaultJob"].as_array().unwrap() {
            let got = ci_default_job(&fixture_jobs(&case["jobs"]), case["requested"].as_str());
            assert_eq!(got.map(Value::String).unwrap_or(Value::Null), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn steps_match_the_fixture() {
        for case in fixture()["steps"].as_array().unwrap() {
            let got: Vec<Value> = ci_steps(&ci_steps_of(Some(&case["steps"])), now(case))
                .iter()
                .map(|item| json!({"title": item.title, "status": item.status.id(), "description": item.description}))
                .collect();
            assert_eq!(Value::Array(got), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn log_view_matches_the_fixture() {
        for case in fixture()["logView"].as_array().unwrap() {
            let view = ci_log_view(case["text"].as_str().unwrap());
            assert_eq!(json!({"code": view.code, "highlight": view.highlight}), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn actions_match_the_fixture() {
        for case in fixture()["actions"].as_array().unwrap() {
            let actions = ci_run_actions(case["state"].as_str().unwrap());
            assert_eq!(json!({"rerunFailed": actions.rerun_failed, "cancel": actions.cancel}), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn the_specimen_selects_its_failing_job() {
        let fixture = fixture();
        let specimen = &fixture["specimen"];
        assert_eq!(ci_default_job(&fixture_jobs(&specimen["jobs"]), None).as_deref(), specimen["selected"].as_str());
        assert_eq!(ci_log_view(specimen["log"].as_str().unwrap()).highlight.len(), 1);
    }

    #[test]
    fn the_copy_reads_off_the_fixture() {
        assert_eq!(run_title(12), "Run #12");
        assert_eq!(copy("tiles.commit"), "Commit");
        assert_eq!(test_id("log"), "ci-run-log");
        assert_eq!(live_tail_ms(), 2000);
        assert_eq!(log_max_lines(), 32);
        assert_eq!(runner_copy("section"), "CI runner");
        assert_eq!(runner_test_id("switch"), "runner-enabled");
        assert_eq!(runner_timeout_options()[2], ("60".to_string(), "1 h".to_string()));
        assert_eq!(runner_slot_limits(), (1, 16));
        assert_eq!(runner_max_labels(), 16);
    }

    #[test]
    fn the_run_link_parses() {
        assert_eq!(
            ci_run_link("/t/acme/ci/r12?job=j2"),
            Some(CiRunLink { team_slug: "acme".into(), run_id: "r12".into(), job_id: Some("j2".into()) })
        );
        assert_eq!(ci_run_link("/t/acme/ci/r12/").map(|link| link.job_id), Some(None));
        assert_eq!(ci_run_link("/t/acme/boards/x"), None);
        assert_eq!(ci_run_link("/t/acme/ci"), None);
    }

    #[test]
    fn normalize_label_matches_the_fixture() {
        for case in runner()["normalizeLabel"].as_array().unwrap() {
            let got = normalize_runner_label(case["input"].as_str().unwrap());
            assert_eq!(got.map(Value::String).unwrap_or(Value::Null), case["expected"], "{}", case["input"]);
        }
    }

    #[test]
    fn notice_matches_the_fixture() {
        for case in runner()["notice"].as_array().unwrap() {
            let caps: Vec<String> = case["caps"].as_array().unwrap().iter().map(|cap| cap.as_str().unwrap().to_string()).collect();
            let got = runner_notice(case["client"].as_str().unwrap(), &caps);
            assert_eq!(got.map(|n| Value::String(n.id().into())).unwrap_or(Value::Null), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn os_label_matches_the_fixture() {
        for case in runner()["osLabel"].as_array().unwrap() {
            let got = runner_os_label(case["platform"].as_str());
            assert_eq!(got.map(|label| Value::String(label.into())).unwrap_or(Value::Null), case["expected"], "{}", case["platform"]);
        }
    }

    #[test]
    fn recent_jobs_match_the_fixture() {
        for case in runner()["recentJobs"].as_array().unwrap() {
            let got: Vec<Value> = runner_recent_jobs(case["deviceId"].as_str().unwrap(), &fixture_runner_jobs(&case["jobs"]), now(case))
                .iter()
                .map(row_json)
                .collect();
            assert_eq!(Value::Array(got), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn runner_settings_fill_the_defaults() {
        assert_eq!(runner_settings(None), RunnerSettings { enabled: false, labels: vec![], slots: 1, timeout_min: 60 });
        let runner = runner();
        let specimen = runner_settings(Some(&runner["specimen"]["device"]["runner"]));
        assert_eq!(specimen, RunnerSettings { enabled: true, labels: vec!["gpu".into()], slots: 2, timeout_min: 60 });
        assert_eq!(runner_settings(Some(&json!({"enabled": true}))).timeout_min, 60);
    }
}
